use anyhow;
use esp_idf_hal::gpio::{InterruptType, PinDriver, Pull};
use esp_idf_hal::modem::Modem;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::mqtt::client::{
    EspMqttClient, EspMqttConnection, EventPayload, MqttClientConfiguration, QoS,
};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{AuthMethod, ClientConfiguration, Configuration, EspWifi, ScanMethod};
use heapless::String;
use log::info;
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

// WiFi Configurations
const WIFI_SSID: &str = "";
const WIFI_PASSWORD: &str = "";

// Mqtt Configurations
const MQTT_TOPIC: &str = "esp/water-flow";
const MQTT_USERNAME: &str = "";
const MQTT_PASSWORD: &str = "";
const MQTT_URL: &str = "";

// `static` creates a single global value with a fixed memory address.
// Unlike `const`, it is not inlined and can be mutated (here safely via `AtomicU32`).
static PULSE_COUNT: AtomicU32 = AtomicU32::new(0);

fn time_now_in_millis() -> u64 {
    unsafe { (esp_idf_svc::sys::esp_timer_get_time() / 1000) as u64 }
}

fn setup_wifi(modem: Modem) -> anyhow::Result<EspWifi<'static>> {
    let ssid_as_heap_string: String<32> = String::try_from(WIFI_SSID).expect("SSID too long");
    let password_as_heap_string: String<64> = String::try_from(WIFI_PASSWORD).expect("Password too long");

    let sysloop = EspSystemEventLoop::take().expect("Failed to take event loop");
    let nvs = EspDefaultNvsPartition::take().expect("Failed to take NVS");

    let mut wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs)).expect("Failed to initialize WiFi");
    let wifi_config = ClientConfiguration {ssid: ssid_as_heap_string, password: password_as_heap_string, auth_method: AuthMethod::WPA2Personal, channel: Some(40), scan_method: ScanMethod::FastScan, ..Default::default()};
    wifi.set_configuration(&Configuration::Client(wifi_config)).expect("Failed to set WiFi");

    wifi.start()?;
    anyhow::Ok(wifi)
}

fn wifi_connection_event(mut wifi: EspWifi<'static>, wifi_connected: Arc<AtomicBool>,) -> anyhow::Result<()> {
    loop {
        let is_ready = wifi.is_connected()? && wifi.is_up()?;
        // Check current status
        match is_ready {
            true => {
                info!("WiFi connected!");
                wifi_connected.store(true, Ordering::Relaxed);
            }
            false => {
                wifi_connected.store(false, Ordering::Relaxed);
                match wifi.connect() {
                    Ok(_) => {
                        info!("WiFi reconnection initiated");
                    }
                    Err(e) => {
                        info!("WiFi reconnection failed: {:?}, retrying...", e);
                    }
                }
            }
        }
    }
}

fn setup_mqtt() -> anyhow::Result<(EspMqttClient<'static>, esp_idf_svc::mqtt::client::EspMqttConnection)> {
    let mqtt_config = MqttClientConfiguration {client_id: Some("esp-water-flow"), username: Some(MQTT_USERNAME), password: Some(MQTT_PASSWORD), ..Default::default()};
    let (mqtt_client, mqtt_event_loop) = EspMqttClient::new(MQTT_URL, &mqtt_config)?;

    anyhow::Ok((mqtt_client, mqtt_event_loop))
}

fn mqtt_connection_event(mut mqtt_connection: EspMqttConnection, mqtt_connected: Arc<AtomicBool>) -> anyhow::Result<()> {
    loop {
        if let Ok(event) = mqtt_connection.next() {
            match event.payload() {
                EventPayload::Connected(_) => {
                    info!("MQTT connected!");
                    mqtt_connected.store(true, Ordering::Relaxed);
                }
                EventPayload::Disconnected => {
                    println!("MQTT Disconnected, retrying...");
                }
                _ => {}
            }
        }
    }
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    info!("=== DEVICE POWERED ON - Starting initialization ===");
    info!("Device is powered by water flow - will run until water stops");

    let peripherals = Peripherals::take().expect("Failed to take peripherals");

    // Wrap in Arc so they can be shared across threads
    let wifi_connected = Arc::new(AtomicBool::new(false));
    let mqtt_connected = Arc::new(AtomicBool::new(false));

    // Clone Arc references for the threads
    let wifi_connected_clone = Arc::clone(&wifi_connected);
    let mqtt_connected_clone = Arc::clone(&mqtt_connected);

    let mut flow_pin = PinDriver::input(peripherals.pins.gpio25)?;
    flow_pin.set_pull(Pull::Up)?;
    flow_pin.set_interrupt_type(InterruptType::AnyEdge)?;
    unsafe {flow_pin.subscribe(|| {PULSE_COUNT.fetch_add(1, Ordering::Relaxed);})?;}
    info!("Flow sensor reading started on GPIO 25 - counting pulses immediately");

    // Initialize WiFi and MQTT
    let wifi = setup_wifi(peripherals.modem)?;
    let (mut mqtt_client, mqtt_event) = setup_mqtt()?;

    let _wifi_thread = std::thread::Builder::new().stack_size(8192).spawn(move || {
            if let Err(e) = wifi_connection_event(wifi, wifi_connected_clone) {
                info!("WiFi connection thread error: {:?}", e);
            }
        })?;

    let _mqtt_thread = std::thread::Builder::new().stack_size(8192).spawn(move || {
            if let Err(e) = mqtt_connection_event(mqtt_event, mqtt_connected_clone) {
                info!("MQTT connection thread error: {:?}", e);
            }
        })?;

    info!("=== Initialization complete - entering main loop ===");
    info!("Sensor reading continues regardless of WiFi/MQTT state");

    let mut last_sample_time = time_now_in_millis();
    let last_pulse_count: u32 = PULSE_COUNT.load(Ordering::Relaxed);
    loop {
        flow_pin.enable_interrupt()?;
        if time_now_in_millis() - last_sample_time < 1_000 {continue;}

        let now = time_now_in_millis();
        let pulses = PULSE_COUNT.load(Ordering::Relaxed);

        if !wifi_connected.load(Ordering::Relaxed) || !mqtt_connected.load(Ordering::Relaxed) {continue;}

        let time_delta = now - last_sample_time;
        let pulse_delta = pulses.saturating_sub(last_pulse_count);
        let payload = json!({"total_pulses": pulse_delta, "Time_ms": time_delta});
        
        match mqtt_client.publish(MQTT_TOPIC,QoS::AtLeastOnce,false, payload.to_string().as_bytes()) {
                Ok(_) => {

                    last_sample_time = now;
                }
                Err(e) => {
                    info!("Failed to publish data: {:?}", e);
            }
        }
    }
}

