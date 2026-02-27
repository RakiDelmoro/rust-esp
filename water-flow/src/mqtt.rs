use crate::config::{MQTT_PASSWORD, MQTT_URL, MQTT_USERNAME};
use esp_idf_svc::mqtt::client::{EspMqttClient, EventPayload, MqttClientConfiguration};
use log::info;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub fn setup_mqtt() -> anyhow::Result<(
    EspMqttClient<'static>,
    esp_idf_svc::mqtt::client::EspMqttConnection,
)> {
    let mqtt_config = MqttClientConfiguration {
        client_id: Some("esp-water-flow"),
        username: Some(MQTT_USERNAME),
        password: Some(MQTT_PASSWORD),
        ..Default::default()
    };
    let (mqtt_client, mqtt_event_loop) = EspMqttClient::new(MQTT_URL, &mqtt_config)?;

    anyhow::Ok((mqtt_client, mqtt_event_loop))
}

pub fn run_mqtt_loop(
    wifi_connected: Arc<AtomicBool>,
    mqtt_connected: Arc<AtomicBool>,
    mqtt_client: Arc<Mutex<Option<EspMqttClient<'static>>>>,
) -> anyhow::Result<()> {
    loop {
        // Wait for WiFi to be connected before attempting MQTT
        if !wifi_connected.load(Ordering::Relaxed) {
            // Clear MQTT state if WiFi is down
            if mqtt_connected.load(Ordering::Relaxed) {
                info!("WiFi disconnected - clearing MQTT state");
                mqtt_connected.store(false, Ordering::Relaxed);
                if let Ok(mut guard) = mqtt_client.lock() {
                    *guard = None;
                }
            }
            thread::sleep(Duration::from_millis(100));
            continue;
        }

        // WiFi is connected - try to establish MQTT connection
        info!("WiFi ready - initializing MQTT...");
        match setup_mqtt() {
            Ok((client, mut connection)) => {
                info!("MQTT client created, waiting for connection...");

                // Store client immediately so main thread can use it once connected
                // Use a flag to track if we've confirmed connection
                let mut confirmed_connected = false;

                // Store client in shared state BEFORE event loop
                // This is safe because we only mark as 'connected' after confirmed
                if let Ok(mut guard) = mqtt_client.lock() {
                    *guard = Some(client);
                }

                // Run event loop - client is now in shared state
                loop {
                    match connection.next() {
                        Ok(event) => {
                            match event.payload() {
                                EventPayload::Connected(_) => {
                                    if !confirmed_connected {
                                        info!("MQTT connected!");
                                        mqtt_connected.store(true, Ordering::Relaxed);
                                        confirmed_connected = true;
                                    }
                                }
                                EventPayload::Disconnected => {
                                    info!("MQTT Disconnected");
                                    break; // Exit inner loop to reconnect
                                }
                                _ => {}
                            }
                        }
                        Err(e) => {
                            info!("MQTT event error: {:?}", e);
                            break; // Exit inner loop to reconnect
                        }
                    }
                }

                // Clean up before reconnecting
                info!("MQTT disconnected - will reconnect when ready");
                mqtt_connected.store(false, Ordering::Relaxed);
                if let Ok(mut guard) = mqtt_client.lock() {
                    *guard = None;
                }

                // Add delay before reconnection attempt
                thread::sleep(Duration::from_secs(1));
            }
            Err(e) => {
                info!("Failed to setup MQTT: {:?}, will retry...", e);
                thread::sleep(Duration::from_secs(1));
            }
        }
    }
}
