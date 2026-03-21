mod config;
mod http;
mod motor;
mod servo;
mod wifi;

use anyhow::Result;
use esp_idf_hal::{
    delay::FreeRtos,
    gpio::PinDriver,
    ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver, Resolution},
    peripherals::Peripherals,
    units::Hertz,
};
use esp_idf_svc::{eventloop::EspSystemEventLoop, http::server::EspHttpServer, nvs::EspDefaultNvsPartition};
use std::sync::{Arc, Mutex};

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // Servo setup (two servos on same timer, mirrored)
    let servo_timer = LedcTimerDriver::new(
        peripherals.ledc.timer0,
        &TimerConfig::default().frequency(Hertz(50)).resolution(Resolution::Bits14),
    )?;
    let servo1 = Arc::new(Mutex::new(LedcDriver::new(
        peripherals.ledc.channel0,
        &servo_timer,
        peripherals.pins.gpio10,
    )?));
    let servo2 = Arc::new(Mutex::new(LedcDriver::new(
        peripherals.ledc.channel3,
        &servo_timer,
        peripherals.pins.gpio3,
    )?));

    // Motor setup (GPIO4 = left PWM, GPIO5 = right PWM, GPIO6/7 = enable)
    let motor_timer = LedcTimerDriver::new(
        peripherals.ledc.timer1,
        &TimerConfig::default().frequency(Hertz(1_000)).resolution(Resolution::Bits8),
    )?;
    let ch_l = LedcDriver::new(peripherals.ledc.channel1, &motor_timer, peripherals.pins.gpio4)?;
    let ch_r = LedcDriver::new(peripherals.ledc.channel2, &motor_timer, peripherals.pins.gpio5)?;
    let en_l = PinDriver::output(peripherals.pins.gpio6)?;
    let en_r = PinDriver::output(peripherals.pins.gpio7)?;
    let motors = Arc::new(Mutex::new(motor::Motors::new(en_l, en_r, ch_l, ch_r)?));

    // Wi-Fi
    let _wifi = wifi::connect(peripherals.modem, sys_loop, nvs)?;

    // HTTP server
    let mut server = EspHttpServer::new(&esp_idf_svc::http::server::Configuration::default())?;
    http::register_handlers(&mut server, servo1, servo2, motors)?;

    loop {
        FreeRtos::delay_ms(1000);
    }
}
