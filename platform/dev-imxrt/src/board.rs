use embassy_imxrt::{bind_interrupts, peripherals, uart};
use platform_common::board::BoardIo;

bind_interrupts!(pub struct Irqs {
    FLEXCOMM0 => uart::InterruptHandler<peripherals::FLEXCOMM0>;
});

/// Board IO for the dev-imxrt platform.
///
/// This minimal development board provides a UART interface
/// for ODP service communication.
pub struct Board {
    /// UART for ODP service communication.
    pub uart: uart::Uart<'static, uart::Async>,
}

impl BoardIo for Board {
    type Peripherals = embassy_imxrt::Peripherals;

    fn init(p: Self::Peripherals) -> Self {
        let config = uart::Config {
            baudrate: 115200,
            ..Default::default()
        };
        let uart = uart::Uart::new_async(p.FLEXCOMM0, p.PIO0_1, p.PIO0_2, Irqs, p.DMA0_CH1, p.DMA0_CH0, config)
            .expect("failed to initialize async UART");

        Board { uart }
    }
}
