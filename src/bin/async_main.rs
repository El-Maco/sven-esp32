#![no_std]
#![no_main]

use alloc::string::ToString;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::{clock::CpuClock, time};
use esp_wifi::wifi::{
    utils::create_network_interface, AccessPointInfo, AuthMethod, ClientConfiguration,
    Configuration, WifiError, WifiStaDevice,
};
use log::info;
use smoltcp::iface::SocketStorage;

extern crate alloc;

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // generator version: 0.2.2

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(72 * 1024);

    esp_println::logger::init_logger_from_env();

    let timer0 = esp_hal::timer::systimer::SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    info!("Embassy initialized!");

    // configure wifi
    let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
    let init = esp_wifi::init(
        timg0.timer0,
        esp_hal::rng::Rng::new(peripherals.RNG),
        peripherals.RADIO_CLK,
    )
    .unwrap();

    let mut wifi = peripherals.WIFI;
    let mut socket_set_entries: [SocketStorage; 3] = Default::default();

    let (iface, device, mut controller) =
        create_network_interface(&init, &mut wifi, WifiStaDevice).unwrap();

    let ssid = env!("SSID");
    let pass = env!("PASSWORD");
    let client_config = Configuration::Client(ClientConfiguration {
        ssid: heapless::String::try_from("todo").unwrap(),
        bssid: None,
        auth_method: AuthMethod::WPAWPA2Personal,
        password: heapless::String::try_from("todo").unwrap(),
        channel: None,
    });

    let res = controller.set_configuration(&client_config);
    info!("Wi-Fi set_configuration returned {:?}", res);

    controller.start().unwrap();
    info!("Is wifi started: {:?}", controller.is_started());

    info!("Start Wifi Scan");
    let res: Result<(heapless::Vec<AccessPointInfo, 10>, usize), WifiError> = controller.scan_n();
    if let Ok((res, _count)) = res {
        for ap in res {
            info!("{:?}", ap);
        }
    }

    info!("{:?}", controller.capabilities());
    info!("Wi-Fi connect: {:?}", controller.connect());

    info!("Wait to get connected");
    loop {
        let res = controller.is_connected();
        match res {
            Ok(connected) => {
                if connected {
                    info!("Wi-Fi is CONNECTED!");
                    break;
                }
            }
            Err(err) => {
                info!("{:?}", err);
                loop {}
            }
        }
    }
    // TODO: Spawn some tasks
    let _ = spawner;

    loop {
        info!("Hello world!");
        Timer::after(Duration::from_secs(1)).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/v0.23.1/examples/src/bin
}
