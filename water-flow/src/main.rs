use std::sync::atomic::{Atomic, Ordering};

use anyhow;
use esp_idf_hal::sys::EspError;
use esp_idf_svc::systime::EspSystemTime;
use log::info;
use serde_json::json;
use heapless::String;
use esp_idf_hal::adc::ADC1;
use esp_idf_hal::modem::Modem;
use esp_idf_hal::gpio::{PinDriver, Pins};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::adc::oneshot::config::AdcChannelConfig;
use esp_idf_hal::adc::oneshot::{AdcChannelDriver, AdcDriver};
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::mqtt::client::{EspMqttClient, MqttClientConfiguration, EventPayload, QoS};
use esp_idf_svc::wifi::{AuthMethod, ClientConfiguration, Configuration, EspWifi, ScanMethod};

use std::sync::atomic::AtomicU32;

// WiFi Configurations
const WIFI_SSID: &str = "Zoltu Staff";
const WIFI_PASSWORD: &str = "cats and dogs";

// Mqtt Configurations
const MQTT_TOPIC: &str = "esp/water-flow";
const MQTT_USERNAME: &str = "mqtt_indicator_1";
const MQTT_PASSWORD: &str = "mqtt";
const MQTT_URL: &str = "";

static PULSE_COUNT: AtomicU32 = AtomicU32::new(0);

fn time_now_in_micro_seconds() -> u64 {
    unsafe { esp_idf_svc::sys::esp_timer_get_time() as u64 }
}

fn main() {
    // It is necessary to call this function once. Otherwise, some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();
    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take().expect("Failed to take event loop");
    let nvs = EspDefaultNvsPartition::take().expect("Failed to take NVS");

    // WiFi Setup
    let mut wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs)).expect("Failed to initialize WiFi");
    let wifi_config = ClientConfiguration {ssid: ssid_as_heap_string, password: password_as_heap_string, auth_method: AuthMethod::WPA2Personal, channel: Some(40), scan_method: ScanMethod::FastScan, ..Default::default()};
    wifi.set_configuration(&Configuration::Client(wifi_config)).expect("Failed to set WiFi configurations");
    
    wifi.start().expect("Failed to start WiFi");
    wifi.connect().expect("Failed to initiate WiFi connect");
    wifi.wait_netif_up()?; // This is same as looping until wifi is ready for MQTT work.
    info!("WiFi Connected");
    
    // MQTT Setup
    let mqtt_config = MqttClientConfiguration{client_id: Some("esp-water-flow"), username: Some(MQTT_USERNAME), password: Some(MQTT_PASSWORD), ..Default::default()};
    let (mqtt_client, mut mqtt_event_loop) = EspMqttClient::new(MQTT_URL, &mqtt_config)?;

    let mut flow_pin = PinDriver::input(peripherals.pins.gpio25)?;
    flow_pin.set_pull(Pull::Up)?;
    flow_pin.set_interrupt_type(InterruptType::PosEdge)?;

    unsafe {
        flow_pin.subscribe(|| {
            PULSE_COUNT.fetch_add(1, Ordering::Relaxed);
        })?;
    }

    let mut last_sample_time = time_now_in_micro_seconds();
    let mut last_pulse_count: u32 = 0;

    loop {
        let now = time_now_in_micro_seconds();
        if now - last_sample_time < 1000 {
            continue;
        }

    }

    log::info!("Hello, world!");
}
