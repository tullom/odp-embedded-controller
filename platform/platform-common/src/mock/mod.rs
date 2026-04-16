//! Provides mock hardware for development platforms lacking hardware.
//! Additionally, provides common setup and initialization if the platform doesn't need anything special.
//!
//! This allows for easy testing of host to EC comms.
pub mod battery;
pub mod thermal;
pub mod time_alarm;

/// Initialize mock embedded services.
pub async fn init(spawner: embassy_executor::Spawner) -> super::OdpRelayHandler<'static> {
    embedded_services::info!("Initializing mock services...");
    embedded_services::init().await;

    let thermal = thermal::init(spawner).await;
    let battery = battery::init(spawner).await;
    let tas = time_alarm::init(spawner).await;

    super::OdpRelayHandler::new(battery, thermal, tas)
}
