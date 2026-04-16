use embedded_sensors_hal_async::sensor as sensor_traits;
use embedded_sensors_hal_async::temperature::{DegreesCelsius, TemperatureSensor, TemperatureThresholdSet};
use thermal_service::sensor;

#[derive(Copy, Clone, Debug)]
pub struct VtsError;
impl sensor_traits::Error for VtsError {
    fn kind(&self) -> sensor_traits::ErrorKind {
        sensor_traits::ErrorKind::Other
    }
}

impl<const N: usize> sensor_traits::ErrorType for Vts<N> {
    type Error = VtsError;
}

pub struct Vts<const N: usize> {
    pts_list: [&'static sensor::Device; N],
}

impl<const N: usize> Vts<N> {
    pub fn new(pts: [&'static sensor::Device; N]) -> Self {
        Self { pts_list: pts }
    }
}

// This example VTS just requests the temperature of all underlying physical sensors then computes the average
// An alternative is to not register the physical sensors with thermal service and have VTS own them
// This would allow VTS to directly sample each physical sensor possibly reducing latency
// Depends on what kind of flexibility you want
impl<const N: usize> TemperatureSensor for Vts<N> {
    async fn temperature(&mut self) -> Result<DegreesCelsius, Self::Error> {
        let mut temp_sum = 0.0;

        for pts in self.pts_list {
            match pts.execute_request(sensor::Request::GetTemp).await {
                Ok(sensor::ResponseData::Temp(temp)) => temp_sum += temp,
                _ => return Err(VtsError),
            }
        }

        Ok(temp_sum / (N as f32))
    }
}

// Setting a threshold for VTS here doesn't make sense so immediately return error
impl<const N: usize> TemperatureThresholdSet for Vts<N> {
    async fn set_temperature_threshold_low(&mut self, _threshold: DegreesCelsius) -> Result<(), Self::Error> {
        Err(VtsError)
    }

    async fn set_temperature_threshold_high(&mut self, _threshold: DegreesCelsius) -> Result<(), Self::Error> {
        Err(VtsError)
    }
}

impl<const N: usize> sensor::CustomRequestHandler for Vts<N> {}
impl<const N: usize> sensor::Controller for Vts<N> {}
