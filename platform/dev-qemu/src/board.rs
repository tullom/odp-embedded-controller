use embassy_qemu_riscv::uart::{Blocking, Uart};
use platform_common::board::BoardIo;

/// Board IO for the dev-qemu platform.
///
/// This minimal development board provides a UART interface
/// for ODP service communication.
pub struct Board {
    /// UART for ODP service communication.
    pub uart: Uart<'static, Blocking>,
}

impl BoardIo for Board {
    type Peripherals = embassy_qemu_riscv::Peripherals;

    fn init(p: Self::Peripherals) -> Self {
        // Note: The embedded-io-async traits are implemented for blocking UART in the HAL until we have async support there
        // IMPORTANT: So the UART service will block the entire executor while it waits for a request,
        // which is fine with mock services, but proper async support will need to be added to the HAL in the future
        // if this becomes no longer acceptable.
        let uart = Uart::new_blocking(p.UART0, Default::default()).expect("Failed to initialize UART");

        Board { uart }
    }
}
