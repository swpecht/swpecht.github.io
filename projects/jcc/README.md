TODOs:
[*] Get unit test running
[*] Get the embassy example running on the pi
    * Note: need to use the blinky_wifi example since the pin for the led is on the wifi board
[ ] How do we want to abstract (or not) the signal from the input?
    [ ] Should we do sampling? Wait for signal edge? Something else?

Things to get:
* Debug probe


Make USB device available:

usbipd attach --wsl --busid 8-2 -u --auto-attach
usbipd attach --wsl --busid 8-3 -u --auto-attach # debug probe

cannot run any probe-rs info commands, need to immediately run the actual program `cargo run` for probe-rs to work

picotool uf2 convert ./target/thumbv8m.main-none-eabihf/release/blinky_wifi -t elf ./jcc.uf2

picotool load jcc.uf2

picotool info