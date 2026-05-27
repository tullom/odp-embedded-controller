#![no_std]
#![no_main]

mod board;
mod imxrt;

use defmt_rtt as _;
#[cfg(not(feature = "test-runner"))]
use defmt::info;
use embassy_executor::Spawner;
#[cfg(not(feature = "test-runner"))]
use embassy_imxrt::uart;
use panic_probe as _;
use platform_common::board::BoardIo;
#[cfg(not(feature = "test-runner"))]
use platform_common::mock::MockOdpRelayHandler;
#[cfg(feature = "test-runner")]
use platform_common::mock::MockServices;
#[cfg(not(feature = "test-runner"))]
use static_cell::StaticCell;

#[cfg(not(feature = "test-runner"))]
#[embassy_executor::task]
async fn uart_service(uart: uart::Uart<'static, uart::Async>, relay: MockOdpRelayHandler) {
    info!("Starting uart service");
    static UART_SERVICE: StaticCell<uart_service::DefaultService<MockOdpRelayHandler>> = StaticCell::new();
    let uart_service = uart_service::DefaultService::default_smbusespi(relay).unwrap();
    let uart_service = UART_SERVICE.init(uart_service);

    let Err(e) = uart_service::task::uart_service(uart_service, uart).await;
    panic!("uart-service error: {:?}", e);
}

/// Self-test task that drives each mock service directly and asserts
/// responses fall within the expected mock ranges. Enabled via the
/// `test-runner` cargo feature, in which case it replaces the UART service.
#[cfg(feature = "test-runner")]
#[embassy_executor::task]
async fn test_runner_task(services: MockServices) -> ! {
    platform_common::mock::test_runner::run(services).await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_imxrt::init(Default::default());
    let board = crate::board::Board::init(p);

    let services = platform_common::mock::init(spawner).await;

    #[cfg(not(feature = "test-runner"))]
    spawner.spawn(uart_service(board.uart, services.relay).expect("Failed to spawn UART service task"));

    #[cfg(feature = "test-runner")]
    {
        let _ = board.uart;
        spawner.spawn(test_runner_task(services).expect("Failed to spawn test runner task"));
    }
}
