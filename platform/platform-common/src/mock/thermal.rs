use embassy_sync::once_lock::OnceLock;
use embedded_services::info;
use static_cell::StaticCell;
use thermal_service as ts;
use ts::mock::{TsMockFan, TsMockSensor};

pub async fn init(spawner: embassy_executor::Spawner) -> &'static ts::Service<'static> {
    info!("Initializing thermal service...");

    static SENSOR: StaticCell<TsMockSensor> = StaticCell::new();
    let sensor = SENSOR.init(ts::mock::new_sensor());

    static FAN: StaticCell<TsMockFan> = StaticCell::new();
    let fan = FAN.init(ts::mock::new_fan());

    static SENSORS: StaticCell<[&'static ts::sensor::Device; 1]> = StaticCell::new();
    let sensors = SENSORS.init([sensor.device()]);

    static FANS: StaticCell<[&'static ts::fan::Device; 1]> = StaticCell::new();
    let fans = FANS.init([fan.device()]);

    static STORAGE: OnceLock<ts::Service<'static>> = OnceLock::new();
    let service = ts::Service::init(&STORAGE, sensors, fans).await;

    type MockSensorService = ts::sensor::Service<'static, ts::mock::sensor::MockSensor, 16>;
    odp_service_common::spawn_service!(
        spawner,
        MockSensorService,
        ts::sensor::InitParams {
            sensor,
            thermal_service: service,
        }
    )
    .expect("Failed to spawn mock sensor service");

    type MockFanService = ts::fan::Service<'static, ts::mock::fan::MockFan, 16>;
    odp_service_common::spawn_service!(
        spawner,
        MockFanService,
        ts::fan::InitParams {
            fan,
            thermal_service: service,
        }
    )
    .expect("Failed to spawn mock fan service");

    service
}
