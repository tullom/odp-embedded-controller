//! Self-test runner for mock services.
//!
//! Sends a fixed sequence of requests to each mock service (battery, thermal,
//! time-alarm) and asserts that the responses fall within the expected ranges
//! produced by the mock drivers. This is intended to run instead of the UART
//! service on development platforms to validate end-to-end service plumbing
//! without requiring a host connection.
//!
//! The test cases use the generic [`embedded_service_test::embedded_service_test!`]
//! DSL macro, with one row per trait method. The macro works for any
//! service — pass `async;` for futures-returning traits and `sync;` for
//! plain synchronous traits.

use battery_service_interface::{BatteryService, DeviceId};
use defmt::info;
use embassy_time::{Duration, Timer};
use thermal_service_interface::fan::{FanService, OnState};
use thermal_service_interface::sensor::{SensorService, Threshold};
use thermal_service_interface::ThermalService;
use time_alarm_service_interface::{AcpiTimerId, AlarmExpiredWakePolicy, AlarmTimerSeconds, TimeAlarmService};

use super::MockServices;
use embedded_service_test::{embedded_service_test, TestRunner};

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
    let battery_service = &services.battery;

    embedded_service_test!(async; runner, battery_service; {
        // ---- query methods (DeviceId only, return Result) -------------------
        battery_info(BAT)                                => ( is_ok );
        battery_info(BAT) -> design_voltage              => ( ok_eq 12_000u32 );
        battery_info(BAT) -> cycle_count                 => ( ok_eq 10_000u32 );

        battery_status(BAT)                              => ( is_ok );
        battery_status(BAT) -> battery_present_voltage   => ( ok_in_range 11_000u32..=13_500 );
        battery_status(BAT) -> |b| b.battery_present_rate
                                                         => ( ok_le 5_000u32 );

        battery_power_state(BAT)                         => ( is_ok );
        battery_power_characteristics(BAT)               => ( is_ok );
        battery_maintenance_data(BAT)                    => ( is_ok );
        is_in_use(BAT)                                   => ( is_ok );
        power_source_information(BAT)                    => ( is_ok );
        device_status(BAT)                               => ( is_ok );

        // ---- error path: unknown battery id ---------------------------------
        battery_info(UNKNOWN_BAT)                        => ( is_err );
        battery_status(UNKNOWN_BAT)                      => ( is_err );
        device_status(UNKNOWN_BAT)                       => ( is_err );
    });
}

async fn test_thermal(services: &MockServices, runner: &mut TestRunner) {
    info!("[test] --- thermal service ---");
    let Some(sensor_service) = services.thermal.sensor(0) else {
        runner.record("sensor 0 registered", false);
        return;
    };
    runner.record("sensor 0 registered", true);

    let Some(fan_service) = services.thermal.fan(0) else {
        runner.record("fan 0 registered", false);
        return;
    };
    runner.record("fan 0 registered", true);

    // Target rpm for the fan round-trip check below.
    const TARGET: u16 = 2500;

    // ---- SensorService ----------------------------------------------------
    embedded_service_test!(async; runner, sensor_service; {
        temperature()                                    => ( in_range 0.0f32..=125.0 );
        temperature_average()                            => ( in_range 0.0f32..=125.0 );
        temperature_immediate()                          => ( is_ok );
        temperature_immediate()                          => ( ok_in_range 20.0f32..=40.0 );
        set_threshold(Threshold::Critical, 95.0)         => ( eq () );
        threshold(Threshold::Critical)                   => ( eq 95.0f32 );
        set_sample_period(Duration::from_millis(500))    => ( eq () );
        enable_sampling()                                => ( eq () );
        disable_sampling()                               => ( eq () );
    });

    // ---- Sampling-disabled behaviour --------------------------------------
    // With sampling disabled the background poll task stops updating the
    // cached samples, so `temperature()` (which returns the cached recent
    // sample) must not change across a sleep. `temperature_immediate()`,
    // which bypasses the cache and hits the driver directly, must still
    // succeed. After re-enabling sampling, the cached value must update.
    embedded_service_test!(async; runner, sensor_service; {
        disable_sampling()                               => ( eq () );
        sleep Duration::from_millis(600);
        let cached = temperature();
        temperature_immediate()                          => ( is_ok );
        sleep Duration::from_millis(600);
        temperature()                                    => ( eq cached );

        // Re-enable and verify the cache resumes updating.
        enable_sampling()                                => ( eq () );
        sleep Duration::from_millis(600);
        temperature()                                    => ( ne cached );
    });

    // ---- FanService -------------------------------------------------------
    embedded_service_test!(async; runner, fan_service; {
        enable_auto_control()                            => ( is_ok );
        min_rpm()                                        => ( eq 0u16 );
        max_rpm()                                        => ( eq 6000u16 );
        rpm()                                            => ( le 6_000u16 );
        rpm_average()                                    => ( le 6_000u16 );
        rpm_immediate()                                  => ( is_ok );

        set_rpm(TARGET)                                  => ( is_ok );
        set_duty_percent(50)                             => ( is_ok );
        set_rpm_sampling_period(Duration::from_millis(250))
                                                         => ( eq () );
        set_rpm_update_period(Duration::from_millis(250))
                                                         => ( eq () );

        state_temp(OnState::Min)                         => ( in_range 0.0f32..=125.0 );
        state_temp(OnState::Ramping)                     => ( in_range 0.0f32..=125.0 );
        state_temp(OnState::Max)                         => ( in_range 0.0f32..=125.0 );
        set_state_temp(OnState::Max, 80.0)               => ( eq () );
    });

    // Round-trip check: after setting TARGET above, give the fan a moment to
    // converge then verify the reported rpm.
    Timer::after(Duration::from_millis(50)).await;
    embedded_service_test!(async; runner, fan_service; {
        rpm_immediate()
            => ( ok_in_range TARGET.saturating_sub(500)..=TARGET.saturating_add(500) );
        stop()                                           => ( is_ok );
    });
}

async fn test_time_alarm(services: &MockServices, runner: &mut TestRunner) {
    info!("[test] --- time-alarm service ---");
    let time_alarm_service = &services.time_alarm;

    // Round-trip value used by the timer-value tests below.
    const REQUESTED: u32 = 300;

    // `TimeAlarmDeviceCapabilities` and `TimerStatus` are bitfields whose
    // accessors are methods (not fields), so checks on them project via
    // `-> |x| x.foo()` and compare with `eq true` / `eq false`.
    embedded_service_test!(sync; runner, time_alarm_service; {
        // Capture the current time; on failure the macro records the row and
        // returns from this fn. `first` is then in scope for the rows below,
        // including the `set_real_time(first)` round-trip and the
        // delta-against-`first` projections.
        let first = try get_real_time();

        // Wait so the rows below can assert that the running clock advanced.
        sleep Duration::from_secs(2);

        // ---- get_capabilities ----------------------------------------------
        // Exercise every accessor on TimeAlarmDeviceCapabilities so a future
        // bitfield-layout regression is caught immediately.
        get_capabilities() ~> |c| c.ac_wake_implemented()                  => ( eq true );
        get_capabilities() ~> |c| c.dc_wake_implemented()                  => ( eq true );
        get_capabilities() ~> |c| c.realtime_implemented()                 => ( eq true );
        get_capabilities() ~> |c| c.realtime_accuracy_in_milliseconds()    => ( eq false );
        get_capabilities() ~> |c| c.get_wake_status_supported()            => ( eq true );
        get_capabilities() ~> |c| c.ac_s4_wake_supported()                 => ( eq true );
        get_capabilities() ~> |c| c.ac_s5_wake_supported()                 => ( eq true );
        get_capabilities() ~> |c| c.dc_s4_wake_supported()                 => ( eq true );
        get_capabilities() ~> |c| c.dc_s5_wake_supported()                 => ( eq true );

        // ---- get_real_time --------------------------------------------------
        get_real_time()                             => ( is_ok );
        // Clock must have advanced >=1s since `first` (captured 2s ago).
        get_real_time() -> |t| t.datetime.unix_timestamp()
                                .saturating_sub(first.datetime.unix_timestamp())
                                                    => ( ok_ge 1 );

        // ---- set_real_time --------------------------------------------------
        // Round-trip: write back the snapshot we already captured.
        set_real_time(first)                        => ( is_ok );
        // After resetting, get_real_time should still succeed and report a
        // value within a small window of what we just wrote.
        get_real_time() -> |t| t.datetime.unix_timestamp()
                                .saturating_sub(first.datetime.unix_timestamp())
                                                    => ( ok_le 5 );

        // ---- set_expired_timer_policy / get_expired_timer_policy -----------
        // AcPower round-trip
        set_expired_timer_policy(AcpiTimerId::AcPower, AlarmExpiredWakePolicy::INSTANTLY)
                                                    => ( is_ok );
        get_expired_timer_policy(AcpiTimerId::AcPower)
                                                    => ( eq AlarmExpiredWakePolicy::INSTANTLY );
        set_expired_timer_policy(AcpiTimerId::AcPower, AlarmExpiredWakePolicy::NEVER)
                                                    => ( is_ok );
        get_expired_timer_policy(AcpiTimerId::AcPower)
                                                    => ( eq AlarmExpiredWakePolicy::NEVER );
        // DcPower round-trip
        set_expired_timer_policy(AcpiTimerId::DcPower, AlarmExpiredWakePolicy::INSTANTLY)
                                                    => ( is_ok );
        get_expired_timer_policy(AcpiTimerId::DcPower)
                                                    => ( eq AlarmExpiredWakePolicy::INSTANTLY );

        // ---- set_timer_value / get_timer_value -----------------------------
        // AcPower round-trip with the shared REQUESTED constant.
        set_timer_value(AcpiTimerId::AcPower, AlarmTimerSeconds(REQUESTED))
                                                    => ( is_ok );
        get_timer_value(AcpiTimerId::AcPower) -> |w| w.0
                                                    => ( ok_in_range REQUESTED.saturating_sub(5)..=REQUESTED );
        // DcPower with a distinct value to ensure the two timers are
        // independent.
        set_timer_value(AcpiTimerId::DcPower, AlarmTimerSeconds(120))
                                                    => ( is_ok );
        get_timer_value(AcpiTimerId::DcPower) -> |w| w.0
                                                    => ( ok_in_range 115u32..=120 );
        // Round-trip with 0 (often a "disabled" sentinel).
        set_timer_value(AcpiTimerId::AcPower, AlarmTimerSeconds(0))
                                                    => ( is_ok );
        get_timer_value(AcpiTimerId::AcPower) -> |w| w.0
                                                    => ( ok_eq 0u32 );

        // ---- get_wake_status / clear_wake_status ---------------------------
        // After clearing, neither bit should be set on either timer.
        clear_wake_status(AcpiTimerId::AcPower)     => ( eq () );
        clear_wake_status(AcpiTimerId::DcPower)     => ( eq () );
        get_wake_status(AcpiTimerId::AcPower) ~> |s| s.timer_expired()        => ( eq false );
        get_wake_status(AcpiTimerId::AcPower) ~> |s| s.timer_triggered_wake() => ( eq false );
        get_wake_status(AcpiTimerId::DcPower) ~> |s| s.timer_expired()        => ( eq false );
        get_wake_status(AcpiTimerId::DcPower) ~> |s| s.timer_triggered_wake() => ( eq false );
    });
}
