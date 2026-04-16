# dev-qemu
A platform targeting QEMU RISCV virt using mock embedded-services.

## Run
Install `qemu-run` (this is a convenience tool for spawning QEMU and defmt-print):  
`cargo install --locked --git https://github.com/kurtjd/defmt --branch qemu-run-riscv qemu-run`

Then run:  
`cargo run --release`

The PTY virtual serial port path will be displayed, and this can be used to connect over serial.

E.g. to connect with [ec-test-app](https://github.com/OpenDevicePartnership/odp-platform-common/tree/main/ec-test-app) built with the `serial` feature:  
`./ec-test-app /dev/pts/<N> none`
