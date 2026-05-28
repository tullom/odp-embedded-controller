#![no_std]
#![no_main]

mod board;
mod clocks;

#[cfg(feature = "teleprobe-test")]
teleprobe_meta::target!(b"mcxa266");

use board::Board;
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
#[cfg(not(feature = "test-runner"))]
use embassy_mcxa::lpuart;
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
async fn uart_service(uart: lpuart::LpuartBbq, relay: MockOdpRelayHandler) {
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
    let mut cfg = embassy_mcxa::config::Config::default();
    cfg.clock_cfg = clocks::config();
    let p = embassy_mcxa::init(cfg);
    let board = Board::init(p);

    info!("Hello world from MCXA!");

    #[cfg(feature = "teleprobe-test")]
    cortex_m::asm::bkpt();

    let services = platform_common::mock::init(spawner).await;

    #[cfg(not(feature = "test-runner"))]
    spawner.spawn(uart_service(board.uart, services.relay).expect("Failed to spawn UART service task"));

    #[cfg(feature = "test-runner")]
    {
        let _ = board.uart;
        spawner.spawn(test_runner_task(services).expect("Failed to spawn test runner task"));
    }
}
