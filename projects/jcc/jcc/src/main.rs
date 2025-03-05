//! This example uses the RP Pico on board LED to test input pin 28. This is not the button on the board.
//!
//! It does not work with the RP Pico W board. Use wifi_blinky.rs and add input pin.

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_rp::gpio::{Input, Level, Output};
use embassy_time::{Duration, Timer};
use jcc_lib::add;
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::task]
async fn blinker(mut led: Output<'static>, interval: Duration) {
    loop {
        led.set_high();
        Timer::after(interval).await;
        led.set_low();
        Timer::after(interval).await;
    }
}

#[embassy_executor::task]
async fn reader(mut input: Input<'static>) {
    loop {
        input.wait_for_rising_edge().await;
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let led = Output::new(p.PIN_25, Level::Low);
    spawner
        .spawn(blinker(led, Duration::from_millis(300)))
        .unwrap();
}
