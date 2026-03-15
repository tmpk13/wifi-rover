use anyhow::Result;
use embedded_svc::{
    http::Method,
    io::Write,
    wifi::{AuthMethod, ClientConfiguration, Configuration},
};
use esp_idf_hal::{
    delay::FreeRtos,
    ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver, Resolution},
    peripherals::Peripherals,
    units::Hertz,
};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    http::server::EspHttpServer,
    nvs::EspDefaultNvsPartition,
    wifi::{BlockingWifi, EspWifi},
};
use std::sync::{Arc, Mutex};

// --- Configuration ---
const WIFI_SSID: &str = "Zebra";     // <--- CHANGE THIS
const WIFI_PASS: &str = "Inazoo2016"; // <--- CHANGE THIS

const SERVO_OPEN: u32 = 180;
const SERVO_CLOSE: u32 = 70;

// --- Servo Logic ---
fn set_angle(servo: &mut LedcDriver<'_>, angle: u32) {
    let max_duty = servo.get_max_duty();
    let pulse_us = 500 + (angle * 2000) / 180;
    let duty = (pulse_us * max_duty) / 20_000;
    let _ = servo.set_duty(duty);
}

// --- HTML Dashboard ---
const HTML_SITE: &str = r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Wifi Rover</title>
    <style>
        body { font-family: sans-serif; text-align: center; margin-top: 50px; background: #222; color: #fff; }
        button {
            display: block; width: 80%; max-width: 300px; margin: 20px auto; padding: 20px;
            font-size: 1.5rem; border: none; border-radius: 10px; cursor: pointer;
            background: #007BFF; color: white; transition: 0.2s;
        }
        button:active { background: #0056b3; transform: scale(0.98); }
        .cycle { background: #28a745; }
    </style>
</head>
<body>
    <h1>🤖 Wifi Rover</h1>
    <button onclick="fetch('/open')">Open</button>
    <button onclick="fetch('/close')">Close</button>
    <button class="cycle" onclick="fetch('/cycle')">🌊 Wave / Cycle</button>
    <p id="status">Ready</p>
    <script>
        // Simple feedback
        document.querySelectorAll('button').forEach(b => {
            b.addEventListener('click', () => {
                document.getElementById('status').innerText = 'Command Sent...';
                setTimeout(() => document.getElementById('status').innerText = 'Ready', 1000);
            });
        });
    </script>
</body>
</html>
"#;

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // 1. Servo Setup
    let timer = LedcTimerDriver::new(
        peripherals.ledc.timer0,
        &TimerConfig::default().frequency(Hertz(50)).resolution(Resolution::Bits14),
    )?;
    let servo = Arc::new(Mutex::new(LedcDriver::new(
        peripherals.ledc.channel0,
        timer,
        peripherals.pins.gpio10,
    )?));

    // 2. Connect to Home Wi-Fi (Station Mode)
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
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
    wifi.connect()?;
    wifi.wait_netif_up()?;

    let ip_info = wifi.wifi().sta_netif().get_ip_info()?;
    
    log::info!("-------------------------------------------------");
    log::info!("✅ Connected! Go to: http://{}", ip_info.ip);
    log::info!("-------------------------------------------------");

    // 3. HTTP Server
    let mut server = EspHttpServer::new(&esp_idf_svc::http::server::Configuration::default())?;

    // Serve the Website
    server.fn_handler("/", Method::Get, |request| -> Result<()> {
        let mut response = request.into_ok_response()?;
        response.write_all(HTML_SITE.as_bytes())?;
        Ok(())
    })?;

    // API: Open
    let s_clone = servo.clone();
    server.fn_handler("/open", Method::Get, move |request| -> Result<()> {
        log::info!("Open");

        let mut s = s_clone.lock().unwrap();
        set_angle(&mut s, SERVO_OPEN);
        request.into_ok_response()?;
        Ok(())
    })?;

    // API: Close
    let s_clone = servo.clone();
    server.fn_handler("/close", Method::Get, move |request| -> Result<()> {
        let mut s = s_clone.lock().unwrap();
        set_angle(&mut s, SERVO_CLOSE);
        request.into_ok_response()?;
        Ok(())
    })?;

    // API: Cycle
    let s_clone = servo.clone();
    server.fn_handler("/cycle", Method::Get, move |request| -> Result<()> {
        let mut s = s_clone.lock().unwrap();
        set_angle(&mut s, SERVO_OPEN);
        FreeRtos::delay_ms(500);
        set_angle(&mut s, SERVO_CLOSE);
        FreeRtos::delay_ms(500);
        set_angle(&mut s, SERVO_OPEN);
        request.into_ok_response()?;
        Ok(())
    })?;

    loop {
        FreeRtos::delay_ms(1000);
    }
}
