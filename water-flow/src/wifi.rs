use crate::config::{WIFI_PASSWORD, WIFI_SSID};
use esp_idf_hal::modem::Modem;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{AuthMethod, ClientConfiguration, Configuration, EspWifi, ScanMethod};
use heapless::String;
use log::info;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn setup_wifi(modem: Modem) -> anyhow::Result<EspWifi<'static>> {
    let ssid_as_heap_string: String<32> = String::try_from(WIFI_SSID).expect("SSID too long");
    let password_as_heap_string: String<64> =
        String::try_from(WIFI_PASSWORD).expect("Password too long");

    let sysloop = EspSystemEventLoop::take().expect("Failed to take event loop");
    let nvs = EspDefaultNvsPartition::take().expect("Failed to take NVS");

    let mut wifi =
        EspWifi::new(modem, sysloop.clone(), Some(nvs)).expect("Failed to initialize WiFi");
    let wifi_config = ClientConfiguration {
        ssid: ssid_as_heap_string,
        password: password_as_heap_string,
        auth_method: AuthMethod::WPA2Personal,
        channel: Some(40),
        scan_method: ScanMethod::FastScan,
        ..Default::default()
    };
    wifi.set_configuration(&Configuration::Client(wifi_config))
        .expect("Failed to set WiFi");

    wifi.start()?;
    anyhow::Ok(wifi)
}

pub fn run_wifi_loop(
    mut wifi: EspWifi<'static>,
    wifi_connected: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    loop {
        let is_ready = wifi.is_connected()? && wifi.is_up()?;
        match is_ready {
            true => {
                if !wifi_connected.load(Ordering::Relaxed) {
                    info!("WiFi connected!");
                    wifi_connected.store(true, Ordering::Relaxed);
                }
            }
            false => {
                if wifi_connected.load(Ordering::Relaxed) {
                    info!("WiFi disconnected!");
                    wifi_connected.store(false, Ordering::Relaxed);
                }
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
