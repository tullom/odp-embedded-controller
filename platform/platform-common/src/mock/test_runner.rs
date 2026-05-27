//! Self-test runner for mock services.
//!
//! Sends a fixed sequence of requests to each mock service (battery, thermal,
//! time-alarm) and asserts that the responses fall within the expected ranges
//! produced by the mock drivers. This is intended to run instead of the UART
//! service on development platforms to validate end-to-end service plumbing
//! without requiring a host connection.
//!
//! The actual test cases live in the `tests/*.expect` files next to this
//! module and are pulled in via `include!`. Each file is a flat list of
//! `"name" : <call>, <verb> <args>;` statements written against the
//! [`crate::test`] DSL and references locals set up by the surrounding
//! function (e.g. `runner`, `battery`, `sensor`, `fan`, `tas`).

use battery_service_interface::{BatteryService, DeviceId};
use defmt::info;
use embassy_time::{Duration, Timer};
use thermal_service_interface::fan::FanService;
use thermal_service_interface::sensor::SensorService;
use thermal_service_interface::ThermalService;
use time_alarm_service_interface::TimeAlarmService;

use super::MockServices;
use crate::test::TestRunner;
use crate::{battery_tests, fan_tests, sensor_tests, time_alarm_tests};

const BAT: DeviceId = DeviceId(0);
const UNKNOWN_BAT: DeviceId = DeviceId(99);

/// Run the full mock self-test suite and then idle. Never returns.
pub async fn run(services: MockServices) -> ! {
    // Let the battery state machine finish initializing and run a few poll
    // cycles so the dynamic data cache is populated.
    info!("[test] waiting for services to settle...");
    Timer::after(Duration::from_secs(3)).await;

    info!("[test] starting test suite");
    let mut runner = TestRunner::new();

    test_battery(&services, &mut runner).await;
    test_thermal(&services, &mut runner).await;
    test_time_alarm(&services, &mut runner).await;

    runner.summary();

    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

async fn test_battery(services: &MockServices, runner: &mut TestRunner) {
    info!("[test] --- battery service ---");
    let battery = &services.battery;
    include!("tests/battery.expect");
}

async fn test_thermal(services: &MockServices, runner: &mut TestRunner) {
    info!("[test] --- thermal service ---");
    let Some(sensor) = services.thermal.sensor(0) else {
        runner.record("sensor 0 registered", false);
        return;
    };
    runner.record("sensor 0 registered", true);

    let Some(fan) = services.thermal.fan(0) else {
        runner.record("fan 0 registered", false);
        return;
    };
    runner.record("fan 0 registered", true);

    // Target rpm for the fan round-trip check in the .expect file.
    const TARGET: u16 = 2500;
    include!("tests/thermal.expect");
}

async fn test_time_alarm(services: &MockServices, runner: &mut TestRunner) {
    info!("[test] --- time-alarm service ---");
    let tas = &services.time_alarm;

    let Ok(first) = tas.get_real_time() else {
        runner.record("get_real_time initial ok", false);
        return;
    };
    runner.record("get_real_time initial ok", true);

    // Sleep so the .expect file can assert that the running clock advanced.
    Timer::after(Duration::from_secs(2)).await;

    // Round-trip a timer value: the .expect file reads this constant.
    const REQUESTED: u32 = 300;
    include!("tests/time_alarm.expect");
}
