pub mod events;
pub mod fan;
pub mod prochot;
pub mod sensor;

use embassy_time::Timer;
use embedded_services::{error, info};
use thermal_service as ts;

#[embassy_executor::task]
pub async fn monitor(period_secs: u64, thermal_service: &'static ts::Service<'static>) -> ! {
    loop {
        match thermal_service
            .execute_sensor_request(sensor::SKIN_TMP_ID, ts::sensor::Request::GetTemp)
            .await
        {
            Ok(ts::sensor::ResponseData::Temp(temp)) => info!("Skin temp: {} C", temp),
            _ => error!("Failed to monitor skin temp"),
        }
        match thermal_service
            .execute_fan_request(fan::SOC_FAN_ID, ts::fan::Request::GetRpm)
            .await
        {
            Ok(ts::fan::ResponseData::Rpm(rpm)) => info!("SOC Fan RPM: {}", rpm),
            _ => error!("Failed to monitor SOC fan RPM"),
        }
        match thermal_service
            .execute_fan_request(fan::GPU_FAN_ID, ts::fan::Request::GetRpm)
            .await
        {
            Ok(ts::fan::ResponseData::Rpm(rpm)) => info!("GPU Fan RPM: {}", rpm),
            _ => error!("Failed to monitor GPU fan RPM"),
        }
        Timer::after_secs(period_secs).await;
    }
}
