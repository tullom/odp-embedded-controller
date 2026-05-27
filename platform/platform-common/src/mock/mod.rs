//! Provides mock hardware for development platforms lacking hardware.
//! Additionally, provides common setup and initialization if the platform doesn't need anything special.
//!
//! This allows for easy testing of host to EC comms.
pub mod battery;
pub mod test_runner;
pub mod thermal;
pub mod time_alarm;

crate::impl_relay_handler!(
    MockOdpRelayHandler,
    battery_service::Service<'static, 1>,
    crate::mock::thermal::ThermalService
);

/// Handles to the mock services and their MCTP relay.
///
/// Platforms typically pass [`MockServices::relay`] to the UART service, but
/// the underlying service handles are also exposed so that platform code can
/// drive them directly (for example, the built-in self-test in
/// [`test_runner`]).
pub struct MockServices {
    /// MCTP relay that aggregates the mock services for UART transport.
    pub relay: MockOdpRelayHandler,
    /// Battery service handle.
    pub battery: battery_service::Service<'static, 1>,
    /// Thermal service handle.
    pub thermal: crate::mock::thermal::ThermalService,
    /// Time-alarm service handle.
    pub time_alarm: time_alarm_service::Service<'static>,
}

/// Initialize mock embedded services.
pub async fn init(spawner: embassy_executor::Spawner) -> MockServices {
    embedded_services::info!("Initializing mock services...");
    embedded_services::init().await;

    let thermal = thermal::init(spawner).await;
    let battery = battery::init(spawner).await;
    let time_alarm = time_alarm::init(spawner).await;

    let relay = MockOdpRelayHandler::new(
        battery_service_relay::BatteryServiceRelayHandler::new(battery),
        thermal_service_relay::ThermalServiceRelayHandler::new(thermal),
        time_alarm_service_relay::TimeAlarmServiceRelayHandler::new(time_alarm),
    );

    MockServices {
        relay,
        battery,
        thermal,
        time_alarm,
    }
}
