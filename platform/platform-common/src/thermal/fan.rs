use embedded_services::info;
use thermal_service as ts;
use ts::fan;

pub const SOC_FAN_ID: ts::fan::DeviceId = ts::fan::DeviceId(1);
pub const GPU_FAN_ID: ts::fan::DeviceId = ts::fan::DeviceId(0);

pub async fn auto_control_enable(thermal_service: &ts::Service<'_>) -> fan::Response {
    // Enable auto control of fans
    thermal_service
        .execute_fan_request(SOC_FAN_ID, fan::Request::EnableAutoControl)
        .await?;
    thermal_service
        .execute_fan_request(GPU_FAN_ID, fan::Request::EnableAutoControl)
        .await?;

    info!("Fans enabled");
    Ok(fan::ResponseData::Success)
}

pub async fn auto_control_disable(thermal_service: &ts::Service<'_>) -> fan::Response {
    // Stop fans and disable auto control
    thermal_service
        .execute_fan_request(SOC_FAN_ID, fan::Request::Stop)
        .await?;
    thermal_service
        .execute_fan_request(GPU_FAN_ID, fan::Request::Stop)
        .await?;

    info!("Fans disabled");
    Ok(fan::ResponseData::Success)
}

/// An S-curve fan response which shows how ODP default linear response can be overridden.
pub async fn s_curve_response<F: embedded_fans_async::Fan>(
    fan: &mut F,
    profile: &fan::Profile,
    temp: embedded_sensors_hal_async::temperature::DegreesCelsius,
) -> Result<(), F::Error> {
    let s = {
        let x = (temp - profile.ramp_temp) / (profile.max_temp - profile.ramp_temp);
        let x = x.clamp(0.0, 1.0);
        3.0 * x * x - 2.0 * x * x * x
    };

    let min_rpm = fan.min_start_rpm() as f32;
    let max_rpm = fan.max_rpm() as f32;
    let rpm = min_rpm + s * (max_rpm - min_rpm);

    fan.set_speed_rpm(rpm as u16).await?;
    Ok(())
}
