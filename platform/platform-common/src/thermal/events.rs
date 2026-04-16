use embedded_services::{info, warn};
use thermal_service as ts;

pub async fn handle<const NUM_SENSORS: usize>(
    mut prochot: impl super::prochot::Prochot,
    thermal_service: &'static ts::Service<'_>,
) {
    let mut prochot_clear = [true; NUM_SENSORS];

    loop {
        match thermal_service.wait_event().await {
            ts::Event::ThresholdExceeded(ts::sensor::DeviceId(sensor_id), ts::sensor::ThresholdType::WarnLow, _) => {
                warn!("Sensor {} exceeded its low WARN threshold!", sensor_id);
                // TODO: Send notification to host
            }

            ts::Event::ThresholdCleared(ts::sensor::DeviceId(sensor_id), ts::sensor::ThresholdType::WarnLow) => {
                info!("Sensor {} cleared its low WARN threshold.", sensor_id);
                // TODO: Send notification to host
            }

            ts::Event::ThresholdExceeded(ts::sensor::DeviceId(sensor_id), ts::sensor::ThresholdType::WarnHigh, _) => {
                warn!("Sensor {} exceeded its high WARN threshold!", sensor_id);
                // TODO: Send notification to host
            }

            ts::Event::ThresholdCleared(ts::sensor::DeviceId(sensor_id), ts::sensor::ThresholdType::WarnHigh) => {
                info!("Sensor {} cleared its high WARN threshold.", sensor_id);
            }

            ts::Event::ThresholdExceeded(ts::sensor::DeviceId(sensor_id), ts::sensor::ThresholdType::Prochot, _) => {
                warn!("Sensor {} exceeded its PROCHOT threshold!", sensor_id);
                prochot_clear[sensor_id as usize] = false;
                prochot.assert().await;
                // TODO: Send notification to host
            }

            ts::Event::ThresholdCleared(ts::sensor::DeviceId(sensor_id), ts::sensor::ThresholdType::Prochot) => {
                info!("Sensor {} cleared its PROCHOT threshold.", sensor_id);

                // Only deassert prochot if every sensor is clear of prochot
                prochot_clear[sensor_id as usize] = true;
                if prochot_clear.iter().all(|&clear| clear) {
                    prochot.deassert().await;
                }
                // TODO: Send notification to host
            }

            ts::Event::ThresholdExceeded(ts::sensor::DeviceId(sensor_id), ts::sensor::ThresholdType::Critical, _) => {
                warn!("Sensor {} exceeded its CRITICAL threshold!", sensor_id);
                // TODO: Send notification to host
            }

            ts::Event::ThresholdCleared(ts::sensor::DeviceId(sensor_id), ts::sensor::ThresholdType::Critical) => {
                info!("Sensor {} cleared its CRITICAL threshold.", sensor_id);
            }

            ts::Event::SensorFailure(ts::sensor::DeviceId(sensor_id), e) => {
                warn!("Sensor {} encountered error: {:?}", sensor_id, e);
            }

            ts::Event::FanFailure(fan_id, e) => {
                warn!("Fan {:?} encountered error: {:?}", fan_id, e);
            }
        }
    }
}
