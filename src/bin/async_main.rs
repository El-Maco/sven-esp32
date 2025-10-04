#![no_std]
#![no_main]

use core::str::from_utf8;

use embassy_executor::Spawner;
use embassy_net::{tcp::TcpSocket, IpAddress, IpEndpoint, Runner, StackResources};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::Output;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_wifi::wifi::WifiStaDevice;
use esp_wifi::{wifi::WifiDevice, EspWifiController};
use heapless::Vec;
use log::{debug, error, info, warn};
use rust_mqtt::packet::v5::reason_codes::ReasonCode;
use rust_mqtt::{client::client::MqttClient, utils::rng_generator::CountingRng};
use serde::Deserialize;
use serde_json_core::from_slice;

use sven_esp32::gpio::PulsePin;
use sven_esp32::sven_state::{SvenPosition, SvenState, SvenStateMsg};

extern crate alloc;

macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

const SSID: &str = env!("SSID");
const PASS: &str = env!("PASSWORD");

const MQTT_HOST: &str = env!("MQTT_HOST");

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // generator version: 0.2.2

    let config: esp_hal::Config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(72 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let mut rng = Rng::new(peripherals.RNG);

    let pin_up = PulsePin::new(
        Output::new(peripherals.GPIO5, esp_hal::gpio::Level::Low),
        true,
    );
    let pin_down = PulsePin::new(
        Output::new(peripherals.GPIO7, esp_hal::gpio::Level::Low),
        true,
    );

    esp_println::logger::init_logger_from_env();

    // let timer0 = esp_hal::timer::systimer::SystemTimer::new(peripherals.SYSTIMER);

    // configure wifi
    let init = &*mk_static!(
        EspWifiController<'static>,
        esp_wifi::init(timg0.timer0, rng.clone(), peripherals.RADIO_CLK).unwrap()
    );

    let (wifi_device, wifi_controller) =
        esp_wifi::wifi::new_with_mode(&init, peripherals.WIFI, WifiStaDevice).unwrap();

    esp_hal_embassy::init(timg0.timer1);
    info!("Embassy initialized!");

    let mut config = embassy_net::Config::dhcpv4(Default::default());
    config.ipv6 = embassy_net::ConfigV6::None;
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;

    let (stack, runner) = embassy_net::new(
        wifi_device,
        config,
        mk_static!(StackResources<3>, StackResources::<3>::new()),
        seed,
    );

    spawner.spawn(connection(wifi_controller)).ok();
    spawner.spawn(net_task(runner)).ok();

    info!("Waiting for network to be ready...");
    stack.wait_config_up().await;

    info!("Waiting to get IP address...");
    if let Some(config) = stack.config_v4() {
        info!("Network configuration:");
        info!("    IP: {}", config.address);
        info!("    Gateway: {:?}", config.gateway);
        info!("    DNS servers: {:?}", config.dns_servers);
    } else {
        error!("No IPv4 configuration available!");
    }

    let mut sven_state = SvenState::new(pin_up, pin_down).await;

    loop {
        sleep(1_000).await;
        let mut rx_buffer = [0; 4096];
        let mut tx_buffer = [0; 4096];

        let mut socket: TcpSocket<'_> = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);

        socket.set_timeout(Some(embassy_time::Duration::from_secs(60)));

        let ip = str_to_ip(MQTT_HOST);
        let port = 1883;
        let remote_endpoint = IpEndpoint::new(ip, port);
        info!("Attempting to connect to {}:{}", ip, port);
        let connection = socket.connect(remote_endpoint).await;
        warn!("connection: {:?}", connection);
        match connection {
            Ok(()) => {
                info!("✓ Successfully connected to {}:{}", ip, port);
                let mut config = rust_mqtt::client::client_config::ClientConfig::new(
                    rust_mqtt::client::client_config::MqttVersion::MQTTv5,
                    CountingRng(20000),
                );
                config.add_max_subscribe_qos(
                    rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS1,
                );
                config.add_client_id("sven-esp32");
                config.max_packet_size = 100;
                let mut recv_buffer = [0; 80];
                let mut write_buffer = [0; 80];

                let mut client = MqttClient::<_, 5, _>::new(
                    socket,
                    &mut write_buffer,
                    80,
                    &mut recv_buffer,
                    80,
                    config,
                );

                match client.connect_to_broker().await {
                    Ok(()) => {
                        info!("✓ Connected to MQTT broker at {}:{}", ip, port);
                    }
                    Err(mqtt_error) => match mqtt_error {
                        ReasonCode::NetworkError => {
                            error!("MQTT Network Error: {:?}", mqtt_error);
                            continue;
                        }
                        _ => {
                            error!("Other MQTT Error: {:?}", mqtt_error);
                            continue;
                        }
                    },
                }
                // Get Sven State

                client.subscribe_to_topic("sven/state").await.ok();
                match client.receive_message().await {
                    Ok((topic, packet)) => {
                        info!("Received message from mqtt topic {topic}");
                        if topic == "sven/state" {
                            if let Some(curr_sven_state) = mqtt_packet_to_sven_state(packet).ok() {
                                info!("Setting height_mm to {}, position {:?}", curr_sven_state.height_mm, curr_sven_state.position);
                                sven_state.height_mm = curr_sven_state.height_mm;
                                sven_state.position = curr_sven_state.position;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error receiving packet: {:?}", e);
                    }
                }
                match client.unsubscribe_from_topic("sven/state").await {
                    Ok(_) => info!("Unsubscribed from topic: sven/state"),
                    Err(e) => error!("Failed to unsubscribe from topic: {:?}", e)
                }

                client.subscribe_to_topic("sven/command").await.ok();

                let sven_state_pub = SvenStateMsg::new(&sven_state);
                let sven_state_json: serde_json_core::heapless::String<128> =
                    serde_json_core::to_string(&sven_state_pub).unwrap_or_else(|e| {
                        error!("Failed to serialize SvenState: {:?}", e);
                        serde_json_core::heapless::String::from("{}")
                    });
                info!("Publishing SvenState: {:?}", sven_state_pub);
                client
                    .send_message(
                        "sven/state",
                        sven_state_json.as_bytes(),
                        rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS0,
                        true,
                    )
                    .await
                    .unwrap_or_else(|e| {
                        error!("Failed to publish SvenState: {:?}", e);
                    });

                loop {
                    info!("Waiting for incoming MQTT packets...");
                    match client.receive_message().await {
                        Ok((topic, packet)) => {
                            info!("Received packet: {topic}: {:?}", packet);
                            let text = from_utf8(packet).unwrap_or("");
                            info!("Received packet text: {}", text);
                            if let Some(command) = mqtt_packet_to_desk_command(packet).ok() {
                                info!("Parsed command: {:?}", command);
                                // Handle the desk command
                                handle_desk_command(&command, &mut sven_state).await;
                                // Publish the new sven_state after handling the command
                                let sven_state_pub = SvenStateMsg::new(&sven_state);
                                let sven_state_json: serde_json_core::heapless::String<128> =
                                    serde_json_core::to_string(&sven_state_pub).unwrap_or_else(
                                        |e| {
                                            error!("Failed to serialize SvenState: {:?}", e);
                                            serde_json_core::heapless::String::from("{}")
                                        },
                                    );
                                info!("Publishing SvenState: {:?}", sven_state_pub);
                                client
                                    .send_message("sven/state", sven_state_json.as_bytes(), rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS0, true)
                                    .await
                                    .unwrap_or_else(|e| {
                                        error!("Failed to publish SvenState: {:?}", e);
                                    });
                            } else {
                                error!("Failed to parse MQTT message");
                                continue;
                            }
                        }
                        Err(e) => {
                            error!("Error receiving packet: {:?}", e);
                            break; // Exit the loop on error
                        }
                    }
                    info!("Waiting for next packet...");
                    sleep(1000).await;
                }
            }
            Err(e) => {
                error!("✗ Failed to connect: {:?}", e);

                // Additional debugging information
                if let Some(config) = stack.config_v4() {
                    info!("Current network config still valid:");
                    info!("  Our IP: {}", config.address.address());
                    info!("  Gateway: {:?}", config.gateway);

                    // Check if we're trying to connect to something on our subnet
                    let our_ip = config.address.address().octets();
                    let target_ip: Vec<&str, 4> = MQTT_HOST.split('.').collect();
                    let target_ip: Vec<u8, 4> =
                        target_ip.iter().map(|a| a.parse().unwrap_or(0)).collect();
                    let subnet_mask = config.address.prefix_len();

                    info!("Network analysis:");
                    info!(
                        "  Our IP: {}.{}.{}.{}/{}",
                        our_ip[0], our_ip[1], our_ip[2], our_ip[3], subnet_mask
                    );
                    info!(
                        "  Target IP: {}.{}.{}.{}",
                        target_ip[0], target_ip[1], target_ip[2], target_ip[3]
                    );

                    // Simple same-subnet check (assuming /24 network)
                    if our_ip[0] == target_ip[0]
                        && our_ip[1] == target_ip[1]
                        && our_ip[2] == target_ip[2]
                    {
                        info!("  ✓ Target appears to be on same subnet");
                    } else {
                        info!("  ! Target appears to be on different subnet - routing through gateway");
                    }
                    continue;
                } else {
                    error!("Network configuration lost!");
                }
            }
        }
    }

    //////////////////////////
    // MQTT:
    // refer to https://github.com/JurajSadel/esp32s3-no-std-async-mqtt-demo/blob/main/src/main.rs
    //////////////////////////
    // Refer to https://github.com/JurajSadel/esp32s3-no-std-async-mqtt-demo
    // Refer to https://github.com/esp-rs/esp-wifi-sys/blob/68dc11bbb2c0efa29c4acbbf134d6f142441065e/examples-esp32c3/examples/embassy_dhcp.rs

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/v0.23.1/examples/src/bin
}

#[embassy_executor::task]
async fn connection(mut controller: esp_wifi::wifi::WifiController<'static>) {
    info!("start connection task");
    debug!("Device capabilities: {:?}", controller.capabilities());
    loop {
        match esp_wifi::wifi::wifi_state() {
            esp_wifi::wifi::WifiState::StaConnected => {
                // wait until we're no longer connected
                controller
                    .wait_for_event(esp_wifi::wifi::WifiEvent::StaDisconnected)
                    .await;
                sleep(5000).await
            }
            _ => {}
        }
        if !matches!(controller.is_started(), Ok(true)) {
            let client_config =
                esp_wifi::wifi::Configuration::Client(esp_wifi::wifi::ClientConfiguration {
                    ssid: SSID.try_into().unwrap(),
                    password: PASS.try_into().unwrap(),
                    ..Default::default()
                });
            controller.set_configuration(&client_config).unwrap();
            info!("Starting wifi");
            controller.start_async().await.unwrap();
            info!("Wifi started!");
        }
        info!("About to connect...");

        match controller.connect_async().await {
            Ok(_) => info!("Wifi connected!"),
            Err(e) => {
                error!("Failed to connect to wifi: {e:?}");
                sleep(5000).await
            }
        }
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static, esp_wifi::wifi::WifiStaDevice>>) {
    runner.run().await
}

pub async fn sleep(millis: u32) {
    embassy_time::Timer::after(embassy_time::Duration::from_millis(millis as u64)).await;
}

fn str_to_ip(ip: &str) -> IpAddress {
    let split_ip: Vec<&str, 4> = ip.split('.').collect();
    IpAddress::v4(
        split_ip[0].parse().unwrap_or(0),
        split_ip[1].parse().unwrap_or(0),
        split_ip[2].parse().unwrap_or(0),
        split_ip[3].parse().unwrap_or(0),
    )
}

#[derive(Deserialize, Debug)]
pub enum SvenCommand {
    UpDuration,     // value: ms
    DownDuration,   // value: ms
    UpRelative,     // value: mm
    DownRelative,   // value: mm
    AbsoluteHeight, // value: mm
    Position,       // value: SvenPosition
}

#[derive(Deserialize, Debug)]
pub struct DeskCommand {
    pub command: SvenCommand,
    pub value: u32,
}

fn mqtt_packet_to_sven_state(data: &[u8]) -> Result<SvenStateMsg, serde_json_core::de::Error> {
    match from_slice::<SvenStateMsg>(data) {
        Ok((sven_state, _)) => {
            info!("Received SvenState: {:?}", sven_state);
            Ok(sven_state)
        }
        Err(e) => {
            panic!("Failed to parse message to SvenState: {:?}", e);
        }
    }
}

fn mqtt_packet_to_desk_command(data: &[u8]) -> Result<DeskCommand, serde_json_core::de::Error> {
    match from_slice::<DeskCommand>(data) {
        Ok((command, _)) => {
            info!("Received command: {:?}", command);
            Ok(command)
        }
        Err(e) => {
            panic!("Failed to parse message: {:?}", e);
        }
    }
}

async fn handle_desk_command<'d>(command: &DeskCommand, sven_state: &mut SvenState<'d>) {
    match command.command {
        SvenCommand::UpDuration => {
            info!("Moving up for {} ms", command.value);
            sven_state.move_up(command.value).await;
        }
        SvenCommand::DownDuration => {
            info!("Moving down for {} ms", command.value);
            sven_state.move_down(command.value).await;
        }
        SvenCommand::UpRelative => {
            info!("Moving up by {} mm", command.value);
            sven_state.move_up_relative(command.value).await;
        }
        SvenCommand::DownRelative => {
            info!("Moving down by {} mm", command.value);
            sven_state.move_down_relative(command.value).await;
        }
        SvenCommand::AbsoluteHeight => {
            info!("Setting absolute height to {} mm", command.value);
            sven_state.move_to_height(command.value).await;
        }
        SvenCommand::Position => {
            info!("Setting position to {:?}", command.value);
            let sven_position =
                SvenPosition::try_from(command.value).unwrap_or(SvenPosition::Armrest);
            sven_state.move_to_position(sven_position).await;
        }
    }
}
