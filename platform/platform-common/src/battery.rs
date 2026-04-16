use battery_service::{
    device::{Device, DeviceId},
    wrapper::Wrapper,
};
use bq40z50_rx::{BQ40Z50Error, Bq40z50R5};
use embassy_sync::mutex::Mutex;
use embassy_time::Timer;
use embedded_batteries_async::{
    acpi, charger,
    smart_battery::{self, SmartBattery},
};
use embedded_services::{debug, error, info, trace, GlobalRawMutex};
use static_cell::StaticCell;

pub type BatteryService = battery_service::Service<'static, 1>;
pub type FgWrapper<I2C> = battery_service::wrapper::Wrapper<'static, Bq40z50R5Controller<I2C>>;

pub enum BatteryError {
    /// Generic failure
    Failed,
}

impl<I2cError> From<BQ40Z50Error<I2cError>> for BatteryError {
    fn from(_value: BQ40Z50Error<I2cError>) -> Self {
        BatteryError::Failed
    }
}

static BATTERY_DEVICE: StaticCell<Device> = StaticCell::new();

pub async fn service_init<I2C: embedded_hal_async::i2c::I2c>(
    spawner: &embassy_executor::Spawner,
    bus: I2C,
) -> (&'static BatteryService, Wrapper<'static, Bq40z50R5Controller<I2C>>) {
    let device = BATTERY_DEVICE.init(Device::new(DeviceId(0)));

    let service = odp_service_common::spawn_service!(
        spawner,
        BatteryService,
        battery_service::InitParams {
            devices: [device],
            config: battery_service::context::Config::default(),
        }
    )
    .expect("Failed to initialize battery service");

    static BATTERY_SERVICE: StaticCell<BatteryService> = StaticCell::new();
    let service = BATTERY_SERVICE.init(service);

    let battery_wrapper = Wrapper::new(device, Bq40z50R5Controller::new(bus));

    (service, battery_wrapper)
}

pub async fn fg_init(battery_service: &'static BatteryService) {
    if let Err(e) = battery_service
        .execute_event(battery_service::context::BatteryEvent {
            event: battery_service::context::BatteryEventInner::DoInit,
            device_id: DeviceId(0),
        })
        .await
    {
        error!("Fuel gauge init error: {:?}", e);
    }

    if let Err(e) = battery_service
        .execute_event(battery_service::context::BatteryEvent {
            event: battery_service::context::BatteryEventInner::PollStaticData,
            device_id: DeviceId(0),
        })
        .await
    {
        error!("Fuel gauge static data error: {:?}", e);
    }

    if let Err(e) = battery_service
        .execute_event(battery_service::context::BatteryEvent {
            event: battery_service::context::BatteryEventInner::PollDynamicData,
            device_id: DeviceId(0),
        })
        .await
    {
        error!("Fuel gauge dynamic data error: {:?}", e);
    }
}

pub struct Bq40z50R5Controller<I2C: embedded_hal_async::i2c::I2c> {
    driver: Mutex<GlobalRawMutex, Bq40z50R5<I2C, embassy_time::Delay>>,
    dynamic_data: battery_service::device::DynamicBatteryMsgs,
    static_data: battery_service::device::StaticBatteryMsgs,
}

impl<I2C: embedded_hal_async::i2c::I2c> Bq40z50R5Controller<I2C> {
    pub fn new(bus: I2C) -> Self {
        Self {
            driver: Mutex::new(Bq40z50R5::new(bus, embassy_time::Delay)),
            dynamic_data: battery_service::device::DynamicBatteryMsgs { ..Default::default() },
            static_data: battery_service::device::StaticBatteryMsgs { ..Default::default() },
        }
    }
}

impl<I2C: embedded_hal_async::i2c::I2c> battery_service::controller::Controller for Bq40z50R5Controller<I2C> {
    type ControllerError = BatteryError;

    async fn initialize(&mut self) -> Result<(), Self::ControllerError> {
        self.driver
            .get_mut()
            // Milliamps
            .set_battery_mode(smart_battery::BatteryModeFields::with_capacity_mode(
                smart_battery::BatteryModeFields::new(),
                false,
            ))
            .await
            .inspect_err(|_| error!("FG: failed to initialize"))?;

        info!("FG: initialized");
        Ok(())
    }

    async fn ping(&mut self) -> Result<(), Self::ControllerError> {
        if let Err(e) = self.driver.get_mut().voltage().await {
            error!("FG: failed to ping");
            Err(e.into())
        } else {
            info!("FG: ping success");
            Ok(())
        }
    }

    async fn get_dynamic_data(&mut self) -> Result<battery_service::device::DynamicBatteryMsgs, Self::ControllerError> {
        let new_msgs = battery_service::device::DynamicBatteryMsgs {
            average_current_ma: self.average_current().await?,
            battery_status: self.battery_status().await?.into(),
            max_power_mw: self
                .driver
                .get_mut()
                .device
                .max_turbo_power()
                .read_async()
                .await?
                .max_turbo_power()
                .unsigned_abs()
                .into(),
            battery_temp_dk: self.temperature().await?,
            sus_power_mw: self
                .driver
                .get_mut()
                .device
                .sus_turbo_power()
                .read_async()
                .await?
                .sus_turbo_power()
                .unsigned_abs()
                .into(),
            turbo_rhf_effective_mohm: 0,
            turbo_vload_mv: 0,
            charging_current_ma: self.charging_current().await?,
            charging_voltage_mv: self.charging_voltage().await?,
            voltage_mv: self.voltage().await?,
            current_ma: self.current().await?,
            full_charge_capacity_mwh: match self.full_charge_capacity().await? {
                smart_battery::CapacityModeValue::CentiWattUnsigned(_) => 0xDEADBEEF,
                smart_battery::CapacityModeValue::MilliAmpUnsigned(capacity) => capacity.into(),
            },
            remaining_capacity_mwh: match self.remaining_capacity().await? {
                smart_battery::CapacityModeValue::CentiWattUnsigned(_) => 0xDEADBEEF,
                smart_battery::CapacityModeValue::MilliAmpUnsigned(capacity) => capacity.into(),
            },
            relative_soc_pct: self.relative_state_of_charge().await?.into(),
            cycle_count: self.cycle_count().await?,
            max_error_pct: self.max_error().await?.into(),
            bmd_status: acpi::BmdStatusFlags::default(),
        };
        self.dynamic_data = new_msgs;
        debug!("{:?}", self.dynamic_data);
        Ok(self.dynamic_data)
    }

    async fn get_static_data(&mut self) -> Result<battery_service::device::StaticBatteryMsgs, Self::ControllerError> {
        let mut buf = [0u8; 21];
        self.static_data.design_capacity_mwh = match self.design_capacity().await? {
            smart_battery::CapacityModeValue::CentiWattUnsigned(_) => 0xDEADBEEF,
            smart_battery::CapacityModeValue::MilliAmpUnsigned(design_capacity) => design_capacity.into(),
        };
        self.static_data.design_voltage_mv = self.design_voltage().await?;

        let buf_len = self.static_data.device_chemistry.len();
        self.device_chemistry(&mut buf[..buf_len]).await?;
        self.static_data.device_chemistry.copy_from_slice(&buf[..buf_len]);

        info!("{:?}", buf);

        info!("{:?}", self.static_data);

        Ok(self.static_data)
    }

    async fn get_device_event(&mut self) -> battery_service::controller::ControllerEvent {
        // TODO: Loop forever till we figure out what we want to do here
        loop {
            Timer::after_secs(1000000).await;
        }
    }

    fn set_timeout(&mut self, _duration: embassy_time::Duration) {}
}

#[embassy_executor::task]
pub async fn update_data_task(battery_service: &'static BatteryService) -> ! {
    let mut failures: u32 = 0;
    let mut count: usize = 0;
    loop {
        Timer::after_secs(1).await;
        if count.is_multiple_of(const { 60 * 60 * 60 }) {
            if let Err(e) = battery_service
                .execute_event(battery_service::context::BatteryEvent {
                    event: battery_service::context::BatteryEventInner::PollStaticData,
                    device_id: DeviceId(0),
                })
                .await
            {
                failures += 1;
                error!("Fuel gauge static data error: {:#?}", e);
            }
        }
        if let Err(e) = battery_service
            .execute_event(battery_service::context::BatteryEvent {
                event: battery_service::context::BatteryEventInner::PollDynamicData,
                device_id: DeviceId(0),
            })
            .await
        {
            failures += 1;
            error!("Fuel gauge dynamic data error: {:#?}", e);
        }

        if failures > 10 {
            failures = 0;
            count = 0;
            error!("FG: Too many errors, timing out and starting recovery...");
            loop {
                match battery_service
                    .execute_event(battery_service::context::BatteryEvent {
                        event: battery_service::context::BatteryEventInner::Timeout,
                        device_id: DeviceId(0),
                    })
                    .await
                {
                    Ok(_) => {
                        info!("FG recovered!");
                        break;
                    }
                    Err(e) => match e {
                        battery_service::context::ContextError::StateError(e) => match e {
                            battery_service::context::StateMachineError::DeviceTimeout => {
                                trace!("Recovery failed, trying again after a backoff period");
                                Timer::after_secs(10).await;
                            }
                            battery_service::context::StateMachineError::NoOpRecoveryFailed => {
                                error!("Couldn't recover, reinit needed");
                                break;
                            }
                            _ => debug!("Unexpected error"),
                        },
                        _ => debug!("Unexpected error"),
                    },
                }
            }
        }

        count = count.wrapping_add(1);
    }
}

impl<I2C: embedded_hal_async::i2c::I2c> smart_battery::ErrorType for Bq40z50R5Controller<I2C> {
    type Error = <Bq40z50R5<I2C, embassy_time::Delay> as smart_battery::ErrorType>::Error;
}

impl<I2C: embedded_hal_async::i2c::I2c> smart_battery::SmartBattery for Bq40z50R5Controller<I2C> {
    async fn absolute_state_of_charge(&mut self) -> Result<smart_battery::Percent, Self::Error> {
        self.driver.lock().await.absolute_state_of_charge().await
    }

    async fn at_rate(&mut self) -> Result<smart_battery::CapacityModeSignedValue, Self::Error> {
        self.driver.lock().await.at_rate().await
    }

    async fn at_rate_ok(&mut self) -> Result<bool, Self::Error> {
        self.driver.lock().await.at_rate_ok().await
    }

    async fn at_rate_time_to_empty(&mut self) -> Result<smart_battery::Minutes, Self::Error> {
        self.driver.lock().await.at_rate_time_to_empty().await
    }

    async fn at_rate_time_to_full(&mut self) -> Result<smart_battery::Minutes, Self::Error> {
        self.driver.lock().await.at_rate_time_to_full().await
    }

    async fn average_current(&mut self) -> Result<smart_battery::MilliAmpsSigned, Self::Error> {
        self.driver.lock().await.average_current().await
    }

    async fn average_time_to_empty(&mut self) -> Result<smart_battery::Minutes, Self::Error> {
        self.driver.lock().await.average_time_to_empty().await
    }

    async fn average_time_to_full(&mut self) -> Result<smart_battery::Minutes, Self::Error> {
        self.driver.lock().await.average_time_to_full().await
    }

    async fn battery_mode(&mut self) -> Result<smart_battery::BatteryModeFields, Self::Error> {
        self.driver.lock().await.battery_mode().await
    }

    async fn battery_status(&mut self) -> Result<smart_battery::BatteryStatusFields, Self::Error> {
        self.driver.lock().await.battery_status().await
    }

    async fn charging_current(&mut self) -> Result<charger::MilliAmps, Self::Error> {
        self.driver.lock().await.charging_current().await
    }

    async fn charging_voltage(&mut self) -> Result<charger::MilliVolts, Self::Error> {
        self.driver.lock().await.charging_voltage().await
    }

    async fn current(&mut self) -> Result<smart_battery::MilliAmpsSigned, Self::Error> {
        self.driver.lock().await.current().await
    }

    async fn cycle_count(&mut self) -> Result<smart_battery::Cycles, Self::Error> {
        self.driver.lock().await.cycle_count().await
    }

    async fn design_capacity(&mut self) -> Result<smart_battery::CapacityModeValue, Self::Error> {
        self.driver.lock().await.design_capacity().await
    }

    async fn design_voltage(&mut self) -> Result<charger::MilliVolts, Self::Error> {
        self.driver.lock().await.design_voltage().await
    }

    async fn device_chemistry(&mut self, chemistry: &mut [u8]) -> Result<(), Self::Error> {
        self.driver.lock().await.device_chemistry(chemistry).await
    }

    async fn device_name(&mut self, name: &mut [u8]) -> Result<(), Self::Error> {
        self.driver.lock().await.device_name(name).await
    }

    async fn full_charge_capacity(&mut self) -> Result<smart_battery::CapacityModeValue, Self::Error> {
        self.driver.lock().await.full_charge_capacity().await
    }

    async fn manufacture_date(&mut self) -> Result<smart_battery::ManufactureDate, Self::Error> {
        self.driver.lock().await.manufacture_date().await
    }

    async fn manufacturer_name(&mut self, name: &mut [u8]) -> Result<(), Self::Error> {
        self.driver.lock().await.manufacturer_name(name).await
    }

    async fn max_error(&mut self) -> Result<smart_battery::Percent, Self::Error> {
        self.driver.lock().await.max_error().await
    }

    async fn relative_state_of_charge(&mut self) -> Result<smart_battery::Percent, Self::Error> {
        self.driver.lock().await.relative_state_of_charge().await
    }

    async fn remaining_capacity(&mut self) -> Result<smart_battery::CapacityModeValue, Self::Error> {
        self.driver.lock().await.remaining_capacity().await
    }

    async fn remaining_capacity_alarm(&mut self) -> Result<smart_battery::CapacityModeValue, Self::Error> {
        self.driver.lock().await.remaining_capacity_alarm().await
    }

    async fn remaining_time_alarm(&mut self) -> Result<smart_battery::Minutes, Self::Error> {
        self.driver.lock().await.remaining_time_alarm().await
    }

    async fn run_time_to_empty(&mut self) -> Result<smart_battery::Minutes, Self::Error> {
        self.driver.lock().await.run_time_to_empty().await
    }

    async fn serial_number(&mut self) -> Result<u16, Self::Error> {
        self.driver.lock().await.serial_number().await
    }

    async fn set_at_rate(&mut self, rate: smart_battery::CapacityModeSignedValue) -> Result<(), Self::Error> {
        self.driver.lock().await.set_at_rate(rate).await
    }

    async fn set_battery_mode(&mut self, flags: smart_battery::BatteryModeFields) -> Result<(), Self::Error> {
        self.driver.lock().await.set_battery_mode(flags).await
    }

    async fn set_remaining_capacity_alarm(
        &mut self,
        capacity: smart_battery::CapacityModeValue,
    ) -> Result<(), Self::Error> {
        self.driver.lock().await.set_remaining_capacity_alarm(capacity).await
    }

    async fn set_remaining_time_alarm(&mut self, time: smart_battery::Minutes) -> Result<(), Self::Error> {
        self.driver.lock().await.set_remaining_time_alarm(time).await
    }

    async fn specification_info(&mut self) -> Result<smart_battery::SpecificationInfoFields, Self::Error> {
        self.driver.lock().await.specification_info().await
    }

    async fn temperature(&mut self) -> Result<smart_battery::DeciKelvin, Self::Error> {
        self.driver.lock().await.temperature().await
    }

    async fn voltage(&mut self) -> Result<charger::MilliVolts, Self::Error> {
        self.driver.lock().await.voltage().await
    }
}
