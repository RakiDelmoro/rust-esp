use anyhow;
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
use esp_idf_svc::wifi::{EspWifi, ScanMethod};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::mqtt::client::{EspMqttClient, MqttClientConfiguration, EventPayload, QoS};
use esp_idf_svc::wifi::{AuthMethod, ClientConfiguration, Configuration};

// WiFi Configurations
const WIFI_SSID: &str = "";
const WIFI_PASSWORD: &str = "";

// Mqtt Configurations
const MQTT_TOPIC: &str = "esp/water-level";
const MQTT_USERNAME: &str = "";
const MQTT_PASSWORD: &str = "";
const MQTT_URL: &str = "";

// Time configarutions
const TIMEOUT_DURATION: i64 = 3_000_000; // Timeout duration in microseconds
const DEEP_SLEEP_DURATION_IN_SECONDS: u64 = 1 * 5;

pub fn go_to_deep_sleep() -> ! {
    let sleep_time_duration = DEEP_SLEEP_DURATION_IN_SECONDS * 1_000_000;

    println!("Going to deep sleep for {} seconds...", DEEP_SLEEP_DURATION_IN_SECONDS);
    unsafe {esp_idf_svc::sys::esp_sleep_enable_timer_wakeup(sleep_time_duration);
            esp_idf_svc::sys::esp_deep_sleep_start();
        }
}

fn read_sensor(adc1: ADC1, pins: Pins) -> Result<(u16, u16), anyhow::Error> {
    let adc_reader = AdcDriver::new(adc1)?;

    // Water Tank 1
    let water_tank_1_level = {
        let mut water_tank_1_power_pin = PinDriver::output(pins.gpio13)?;
        water_tank_1_power_pin.set_high()?;
        FreeRtos::delay_ms(100); // Delay for smooth reading
        let mut water_tank_1_adc_pin = AdcChannelDriver::new(&adc_reader, pins.gpio39, &AdcChannelConfig::new())?;
        let tank_1_reading: u16 = adc_reader.read(&mut water_tank_1_adc_pin)?;
        FreeRtos::delay_ms(100); // Delay for smooth reading
        water_tank_1_power_pin.set_low()?;
        tank_1_reading
    };

    // Water Tank 2
    let water_tank_2_level = {
        let mut water_tank_2_power_pin = PinDriver::output(pins.gpio5)?;
        water_tank_2_power_pin.set_high()?;
        FreeRtos::delay_ms(100); // Delay for smooth reading
        let mut water_tank_2_adc_pin = AdcChannelDriver::new(&adc_reader, pins.gpio34, &AdcChannelConfig::new())?;
        let tank_2_reading: u16 = adc_reader.read(&mut water_tank_2_adc_pin)?;
        FreeRtos::delay_ms(100); // Delay for smooth reading
        water_tank_2_power_pin.set_low()?;
        tank_2_reading
    };

    anyhow::Ok((water_tank_1_level, water_tank_2_level))
}


fn wifi_setup(modem: Modem) -> anyhow::Result<EspWifi<'static>> {
    let ssid_as_heap_string: String<32> = String::try_from(WIFI_SSID).expect("SSID too long for buffer");
    let password_as_heap_string: String<64> = String::try_from(WIFI_PASSWORD).expect("PASSWORD too long for buffer");

    let sysloop = EspSystemEventLoop::take().expect("Failed to take event loop");
    let nvs = EspDefaultNvsPartition::take().expect("Failed to take NVS");

    let mut wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs)).expect("Failed to initialize WiFi");
    let wifi_config = ClientConfiguration {ssid: ssid_as_heap_string, password: password_as_heap_string, auth_method: AuthMethod::WPA2Personal, channel: Some(40), scan_method: ScanMethod::FastScan, ..Default::default()};

    wifi.set_configuration(&Configuration::Client(wifi_config)).expect("Failed to set WiFi configurations");
    wifi.start().expect("Failed to start WiFi");
    wifi.connect().expect("Failed to initiate WiFi connect");
    let start_time = unsafe {esp_idf_svc::sys::esp_timer_get_time()};
    
    loop {
        let now = unsafe {esp_idf_svc::sys::esp_timer_get_time()};
        if now - start_time > TIMEOUT_DURATION {go_to_deep_sleep();}

        if wifi.is_connected()? && wifi.is_up()?  { // both should be true before doing MQTT work.
            println!("WiFi connected and IP address acquired!"); 
            break;
        }
        FreeRtos::delay_ms(100); // Delay is unavoidable, but kept as short as possible
    }

    anyhow::Ok(wifi)
}

fn mqtt_client_event() -> anyhow::Result<EspMqttClient<'static>> {
    let mqtt_config = MqttClientConfiguration{client_id: Some("esp-water-level"), username: Some(MQTT_USERNAME), password: Some(MQTT_PASSWORD), ..Default::default()};
    let (mqtt_client, mut mqtt_event_loop) = EspMqttClient::new(MQTT_URL, &mqtt_config)?;
    let start_time = unsafe {esp_idf_svc::sys::esp_timer_get_time()}; // in microseconds

   loop {
        let now = unsafe {esp_idf_svc::sys::esp_timer_get_time()};
        if now - start_time > TIMEOUT_DURATION {go_to_deep_sleep();}

        if let Ok(event) = mqtt_event_loop.next() {
            match event.payload() {
                EventPayload::Connected(_) => {
                    println!("MQTT Connected");
                    break;
                }
                EventPayload::Disconnected => {
                    println!("MQTT Disconnected, retrying...");
                }
                _ => {}
            }
        }
    }

    anyhow::Ok(mqtt_client)
}

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise, some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();
    
    let peripherals = Peripherals::take()?;
    
    let mut wifi = wifi_setup(peripherals.modem)?;

    let (water_tank_1_level, water_tank_2_level) = read_sensor(peripherals.adc1, peripherals.pins)?;
    let payload = json!({"water_tank_1": water_tank_1_level, "water_tank_2": water_tank_2_level});
    let data = payload.to_string();

    let mut mqttclient = mqtt_client_event()?;
    mqttclient.publish(MQTT_TOPIC, QoS::AtLeastOnce, false, data.as_bytes())?;
    mqttclient.publish(MQTT_TOPIC, QoS::AtLeastOnce, false, data.as_bytes())?;
    mqttclient.publish(MQTT_TOPIC, QoS::AtLeastOnce, false, data.as_bytes())?;
    
    FreeRtos::delay_ms(100); // Ensure all tasks have completed before entering deep sleep
    wifi.disconnect()?;
    go_to_deep_sleep()
}
