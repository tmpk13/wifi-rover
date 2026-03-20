use anyhow::Result;
use embedded_svc::wifi::{AuthMethod, ClientConfiguration, Configuration};
use esp_idf_hal::modem::Modem;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    nvs::EspDefaultNvsPartition,
    wifi::{BlockingWifi, EspWifi},
};

use crate::config::{WIFI_PASS, WIFI_SSID};

pub fn connect(
    modem: Modem<'static>,
    sys_loop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> Result<BlockingWifi<EspWifi<'static>>> {
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(modem, sys_loop.clone(), Some(nvs))?,
        sys_loop,
    )?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: WIFI_SSID.try_into().unwrap(),
        password: WIFI_PASS.try_into().unwrap(),
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    }))?;

    log::info!("Connecting to Wi-Fi...");
    wifi.start()?;

    loop {
        match wifi.connect() {
            Ok(_) => match wifi.wait_netif_up() {
                Ok(_) => break,
                Err(e) => log::warn!("Waiting for netif failed: {e}, retrying..."),
            },
            Err(e) => log::warn!("Wi-Fi connect failed: {e}, retrying..."),
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    let ip_info = wifi.wifi().sta_netif().get_ip_info()?;
    log::info!("-------------------------------------------------");
    log::info!("✅ Connected! Go to: http://{}", ip_info.ip);
    log::info!("-------------------------------------------------");

    Ok(wifi)
}
