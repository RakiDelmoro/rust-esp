use heapless::String;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::mqtt::client::{EspMqttClient, MqttClientConfiguration, QoS};
use esp_idf_svc::wifi::{EspWifi, Configuration, ClientConfiguration, AuthMethod};

const WIFI_SSID: &str = "";
const WIFI_PASSWORD: &str = "";

const MQTT_USERNAME: &str = "";
const MQTT_PASSWORD: &str = "";
const MQTT_URL: &str = "";

fn main() {
    esp_idf_svc::sys::link_patches();

    // WiFi configuration expects: ssid -> heapless String<32>, password heapless String<64>
    let mut ssid_as_heap_string: String<32> = String::new();
    ssid_as_heap_string.push_str(WIFI_SSID).unwrap();
    let mut password_as_heap_string: String<64> = String::new();
    password_as_heap_string.push_str(WIFI_PASSWORD).unwrap();

    let system_event_loop = EspSystemEventLoop::take().unwrap();
    let non_volatile_storage = EspDefaultNvsPartition::take().unwrap();
    let peripherals = Peripherals::take().unwrap();

    let mut wifi = EspWifi::new(peripherals.modem, system_event_loop.clone(), Some(non_volatile_storage)).unwrap();

    let client_config = ClientConfiguration{ssid: ssid_as_heap_string, password: password_as_heap_string, auth_method: AuthMethod::WPA2Personal, ..Default::default()};
    wifi.set_configuration(&Configuration::Client(client_config)).unwrap();

    wifi.start().unwrap();
    wifi.connect().unwrap();

    while !wifi.is_connected().unwrap() {
        let config = wifi.get_configuration();
        println!("Connecting to network: {:?}", config);
        FreeRtos::delay_ms(100);
    }
    println!("WiFi-Connected");

    let mqtt_config = MqttClientConfiguration {client_id: Some("esp32-rust-client"), username: Some(MQTT_USERNAME), password: Some(MQTT_PASSWORD), ..Default::default()};
    let (mut mqtt_client, mut mqtt_connection) = EspMqttClient::new(MQTT_URL, &mqtt_config).unwrap();

    let topic = "esp-rust/test";
    let payload = b"Alive";

    // Run MQTT event loop in a separate thread so it can process events
    // (connect, publish, disconnect) without blocking the main loop.
    std::thread::spawn(move || {
    while let Ok(_event) = mqtt_connection.next() {
            // TODO: handle mqtt events
        }
        println!("MQTT connection disrupted.");
    });

    loop {
        mqtt_client.publish(topic, QoS::AtLeastOnce, false, payload).unwrap();
        FreeRtos::delay_ms(1000);
    }
}
