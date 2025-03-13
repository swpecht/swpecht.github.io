TODOs:
[*] Get unit test running
[*] Get the embassy example running on the pi
    * Note: need to use the blinky_wifi example since the pin for the led is on the wifi board
[ ] How do we want to abstract (or not) the signal from the input?
    [ ] Should we do sampling? Wait for signal edge? Something else?

Things to get:
* Debug probe

https://googlechrome.github.io/samples/web-bluetooth/index.html
 	Server can read peripheral: https://googlechrome.github.io/samples/web-bluetooth/read-characteristic-value-changed.html
	Server can write to peripheral: https://googlechrome.github.io/samples/web-bluetooth/reset-energy.html

Make USB device available:

# prep the boards with firmware
    // To make flashing faster for development, you may want to flash the firmwares independently
    // at hardcoded addresses, instead of baking them into the program with `include_bytes!`:
    //     probe-rs download ../../cyw43-firmware/43439A0.bin --binary-format bin --chip RP2040 --base-address 0x10100000
    //     probe-rs download ../../cyw43-firmware/43439A0_clm.bin --binary-format bin --chip RP2040 --base-address 0x10140000
    //     probe-rs download ../../cyw43-firmware/43439A0_btfw.bin --binary-format bin --chip RP2040 --base-address 0x10180000

Replaces the need to do:
    // let fw = include_bytes!("../cyw43-firmware/43439A0.bin");
    // let clm = include_bytes!("../cyw43-firmware/43439A0_clm.bin");

# Get usb attached

usbipd attach --wsl --busid 8-2 -u --auto-attach
usbipd attach --wsl --busid 8-3 -u --auto-attach # debug probe

cannot run any probe-rs info commands, need to immediately run the actual program `cargo run` for probe-rs to work

picotool uf2 convert ./target/thumbv8m.main-none-eabihf/release/blinky_wifi -t elf ./jcc.uf2

picotool load jcc.uf2

picotool info