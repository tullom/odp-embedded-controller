use embassy_npcx::{bind_interrupts, peripherals, uart};
use platform_common::board::BoardIo;

bind_interrupts!(pub struct Irqs {
    CR_UART1_MDMA1 => uart::InterruptHandler<peripherals::CR_UART1>;
});

/// Wrapper around split UART for embedded-io-async compatibility.
///
/// The NPCX HAL does not define `embedded-io` traits or public methods
/// for `Uart`, so the UART must be split into `UartRx` and `UartTx`
/// and wrapped with `embedded-io-async` trait implementations.
pub struct UartWrap {
    rx: uart::UartRx<'static>,
    tx: uart::UartTx<'static>,
}

impl embedded_io_async::ErrorType for UartWrap {
    type Error = uart::Error;
}

impl embedded_io_async::Read for UartWrap {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.rx.read(buf).await
    }
}

impl embedded_io_async::Write for UartWrap {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        let res = self.tx.write(buf).await.map_err(|_| uart::Error::Break)?;
        self.tx.flush().await.map_err(|_| uart::Error::Break)?;
        Ok(res)
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.tx.flush().await.map_err(|_| uart::Error::Break)
    }
}

/// Board IO for the dev-npcx platform.
///
/// This minimal development board provides a UART interface
/// for ODP service communication.
pub struct Board {
    /// UART for ODP service communication (wrapped for embedded-io-async).
    pub uart: UartWrap,
}

impl BoardIo for Board {
    type Peripherals = embassy_npcx::Peripherals;

    fn init(p: Self::Peripherals) -> Self {
        let mut config = uart::Config::default();
        config.baudrate = 115200;

        let uart = uart::Uart::new(p.CR_UART1, p.PG04, p.PH04, Irqs, config);
        let (rx, tx) = uart.split();

        Board {
            uart: UartWrap { tx, rx },
        }
    }
}
