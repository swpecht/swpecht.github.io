//! This example tests the RP Pico 2 W onboard LED and detects Morse code from pin 0.
//!
//! It does not work with the RP Pico 2 board. See `blinky.rs`.

#![no_std]
#![no_main]

use bt_hci::controller::ExternalController;
use cyw43::Control;
use cyw43_pio::{PioSpi, DEFAULT_CLOCK_DIVIDER};
use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::adc::{Adc, Async, Channel, Config, InterruptHandler as AdcInterruptHandler};
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Level, Output, Pull};
use embassy_rp::peripherals::{DMA_CH0, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Instant;
// We'll use a simple GPIO pin for tone generation instead of PWM
use embassy_time::{Duration, Timer};
use globals::SHARED;
use static_cell::StaticCell;
// We don't need these imports anymore
use {defmt_rtt as _, panic_probe as _};

mod bluetooth_app;
mod fmt;
mod globals;

// Program metadata for `picotool info`.
// This isn't needed, but it's recommended to have these minimal entries.
#[link_section = ".bi_entries"]
#[used]
pub static PICOTOOL_ENTRIES: [embassy_rp::binary_info::EntryAddr; 4] = [
    embassy_rp::binary_info::rp_program_name!(c"Blinky Example"),
    embassy_rp::binary_info::rp_program_description!(
        c"This example tests the RP Pico 2 W's onboard LED, connected to GPIO 0 of the cyw43 \
        (WiFi chip) via PIO 0 over the SPI bus."
    ),
    embassy_rp::binary_info::rp_cargo_version!(),
    embassy_rp::binary_info::rp_program_build_attribute!(),
];

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
    ADC_IRQ_FIFO => AdcInterruptHandler;
});

//

#[embassy_executor::task]
async fn cyw43_task(
    runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, PIO0, 0, DMA_CH0>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn blinker(mut led_pin: Output<'static>, mut control: Control<'static>) {
    let delay = Duration::from_millis(2000);

    loop {
        info!("leds on!");
        control.gpio_set(0, true).await;
        led_pin.set_high();
        globals::SHARED.signal(1);
        Timer::after(delay).await;

        info!("leds off!");
        control.gpio_set(0, false).await;
        led_pin.set_low();
        globals::SHARED.signal(0);
        Timer::after(delay).await;
    }
}

#[embassy_executor::task]
async fn tone_generator(mut tone_pin: Output<'static>) {
    // Frequencies for different tones (in Hz)
    const TONE_C4: u32 = 262; // C4 note
    const TONE_E4: u32 = 330; // E4 note
    const TONE_G4: u32 = 392; // G4 note
    const TONE_C5: u32 = 523; // C5 note

    info!("Tone generator ready on GPIO 16");

    loop {
        // Play a sequence of tones (C major chord arpeggio)

        play_tone(&mut tone_pin, TONE_C4, Duration::from_millis(500)).await;
        play_tone(&mut tone_pin, TONE_E4, Duration::from_millis(500)).await;
        play_tone(&mut tone_pin, TONE_G4, Duration::from_millis(500)).await;
        play_tone(&mut tone_pin, TONE_C5, Duration::from_millis(500)).await;

        // Pause between sequences
        Timer::after(Duration::from_millis(1000)).await;
    }
}

// Helper function to play a tone of a specified frequency and duration
async fn play_tone(pin: &mut Output<'_>, frequency: u32, duration: Duration) {
    // Calculate period in microseconds
    let period_us = 1_000_000 / frequency;
    let half_period_us = period_us / 2;

    // Play tone for the given duration
    let end_time = embassy_time::Instant::now() + duration;

    while embassy_time::Instant::now() < end_time {
        pin.set_high();
        Timer::after(Duration::from_micros(half_period_us as u64)).await;
        pin.set_low();
        Timer::after(Duration::from_micros(half_period_us as u64)).await;
    }

    // Ensure pin is low after playing
    pin.set_low();
}

#[embassy_executor::task]
async fn tone_detector(mut adc: Adc<'static, Async>, mut adc_pin: Channel<'static>) {
    // Amplitude threshold for tone detection (value depends on ADC range)
    const TONE_THRESHOLD: u16 = 1000; // Adjust based on testing

    let mut tone_start: Option<Instant> = None;
    let mut last_tone_end: Option<Instant> = None;
    let mut tone_active = false;
    let mut prev_tone_active = false;

    info!("Tone detector ready. Connect analog signal to PIN_26 (ADC0).");

    loop {
        // Read the ADC value
        let adc_result = adc.read(&mut adc_pin).await;
        let now = Instant::now();

        // Only process if ADC read was successful
        if let Ok(adc_value) = adc_result {
            // Detect if a tone is present based on the ADC reading
            tone_active = adc_value > TONE_THRESHOLD;

            // Debug output
            if adc_value > 100 {
                // Filter out noise/near-zero readings
                // info!("ADC value: {}", adc_value);
            }
        } else {
            // Log ADC read error
            info!("ADC read error");
        }

        // Detect state transitions
        if tone_active != prev_tone_active {
            if tone_active {
                // Tone started
                info!("Tone started");
                tone_start = Some(now);

                // Calculate gap duration if we have a previous tone
                if let Some(prev_end) = last_tone_end {
                    let gap_duration = now - prev_end;
                    info!("Gap duration: {} ms", gap_duration.as_millis());

                    // We could add BLE messaging for gap duration here
                    // but we'll keep it simple for now
                }
            } else {
                // Tone ended
                if let Some(start) = tone_start {
                    let tone_duration = now - start;
                    info!("Tone ended. Duration: {} ms", tone_duration.as_millis());
                    last_tone_end = Some(now);

                    // We'll add BLE notification in a different way
                }
            }

            // For Bluetooth notifications (to be implemented later)
            // We'd share the tone state here

            prev_tone_active = tone_active;
        }

        Timer::after(Duration::from_millis(10)).await;
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // Initialize ADC for tone detection on pin 26 (ADC0)
    // The RP2040 has 4 ADC channels:
    // - Channel 0 = GPIO26
    // - Channel 1 = GPIO27
    // - Channel 2 = GPIO28
    // - Channel 3 = GPIO29
    let adc = Adc::new(p.ADC, Irqs, Config::default());
    let adc_pin = Channel::new_pin(p.PIN_26, Pull::None);

    // Initialize LED on pin 15
    let led_pin = Output::new(p.PIN_15, Level::Low);

    // Initialize GPIO pin for tone generation
    let tone_pin = Output::new(p.PIN_16, Level::Low);

    // To make flashing faster for development, you may want to flash the firmwares independently
    // at hardcoded addresses, instead of baking them into the program with `include_bytes!`:
    //     probe-rs download ../../cyw43-firmware/43439A0.bin --binary-format bin --chip RP2040 --base-address 0x10100000
    //     probe-rs download ../../cyw43-firmware/43439A0_clm.bin --binary-format bin --chip RP2040 --base-address 0x10140000
    //     probe-rs download ../../cyw43-firmware/43439A0_btfw.bin --binary-format bin --chip RP2040 --base-address 0x10180000

    // Load firmware from hardcoded addresses for faster development
    let fw = unsafe { core::slice::from_raw_parts(0x10100000 as *const u8, 230321) };
    let clm = unsafe { core::slice::from_raw_parts(0x10140000 as *const u8, 4752) };
    // BT firmware will be used in future update
    let btfw = unsafe { core::slice::from_raw_parts(0x10180000 as *const u8, 245760) };

    // Initialize CYW43 WiFi/BT chip
    let pwr = Output::new(p.PIN_23, Level::Low);
    let cs = Output::new(p.PIN_25, Level::High);
    let mut pio = Pio::new(p.PIO0, Irqs);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        DEFAULT_CLOCK_DIVIDER,
        pio.irq0,
        cs,
        p.PIN_24,
        p.PIN_29,
        p.DMA_CH0,
    );

    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());

    // Create WiFi device - Bluetooth to be added in future update
    // let (_net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw).await;
    let (_net_device, bt_device, mut control, runner) =
        cyw43::new_with_bluetooth(state, pwr, spi, fw, btfw).await;

    // Spawn the cyw43 task
    unwrap!(spawner.spawn(cyw43_task(runner)));

    // Initialize the CYW43 firmware
    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    // Spawn other tasks
    unwrap!(spawner.spawn(blinker(led_pin, control)));
    unwrap!(spawner.spawn(tone_generator(tone_pin)));
    unwrap!(spawner.spawn(tone_detector(adc, adc_pin)));

    // Spawn bluetooth task
    let controller: ExternalController<_, 10> = ExternalController::new(bt_device);
    bluetooth_app::run::<_, 128>(controller).await;
}
