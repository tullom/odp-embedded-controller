use embedded_services::{error, info, warn};
use thermal_service as ts;

/// Represents some signal (typically a GPIO pin) that can be used to assert or deassert prochot to host.
pub trait Prochot {
    /// Assert prochot signal.
    fn assert(&mut self) -> impl core::future::Future<Output = ()>;
    /// Deassert prochot signal.
    fn deassert(&mut self) -> impl core::future::Future<Output = ()>;
}

/// Monitors an active low prochot signal and sets fan to maximum speed while prochot is asserted.
/// If prochot is deasserted, fans resume automatic control.
///
/// If prochot out and in share the same signal, asserting prochot from EC will also trigger fan response.
pub async fn monitor<P: embedded_hal_async::digital::Wait + embedded_hal::digital::InputPin>(
    mut pin: P,
    thermal_service: &'static ts::Service<'_>,
) -> Result<(), P::Error> {
    loop {
        pin.wait_for_falling_edge().await?;

        warn!("Prochot falling edge detected! Setting fans to maximum speed.");
        for fan in thermal_service.fans() {
            if fan.execute_request(ts::fan::Request::SetDuty(100)).await.is_err() {
                error!("Error setting fan {} to max speed", fan.id().0);
            }
        }

        // Wait for procot to clear
        pin.wait_for_high().await?;

        info!("Prochot rising edge detected. Resuming auto control of fans.");
        for fan in thermal_service.fans() {
            if fan.execute_request(ts::fan::Request::EnableAutoControl).await.is_err() {
                error!("Error setting fan {} to auto control", fan.id().0);
            }
        }
    }
}
