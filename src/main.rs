mod config;
mod http;
mod motor;
mod stepper;
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

// Pin assignments:
//
// Stepper (ULN2003AN, full-step wave drive):
//   GPIO10  — IN1
//   GPIO3   — IN2
//   GPIO8   — IN3
//   GPIO9   — IN4
//
// Motors (LEDC timer0, 1 kHz, 8-bit):
//   GPIO4   — left motor PWM   (LEDC channel0)
//   GPIO5   — right motor PWM  (LEDC channel1)
//   GPIO6   — left motor enable  (digital output)
//   GPIO7   — right motor enable (digital output)

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // Stepper setup (4 digital outputs to ULN2003AN IN1–IN4)
    let in1 = PinDriver::output(peripherals.pins.gpio10)?;
    let in2 = PinDriver::output(peripherals.pins.gpio3)?;
    let in3 = PinDriver::output(peripherals.pins.gpio8)?;
    let in4 = PinDriver::output(peripherals.pins.gpio9)?;
    let stepper = Arc::new(Mutex::new(stepper::Stepper::new(in1, in2, in3, in4)));

    // Motor setup (GPIO4 = left PWM, GPIO5 = right PWM, GPIO6/7 = enable)
    let motor_timer = LedcTimerDriver::new(
        peripherals.ledc.timer0,
        &TimerConfig::default().frequency(Hertz(1_000)).resolution(Resolution::Bits8),
    )?;
    let ch_l = LedcDriver::new(peripherals.ledc.channel0, &motor_timer, peripherals.pins.gpio4)?;
    let ch_r = LedcDriver::new(peripherals.ledc.channel1, &motor_timer, peripherals.pins.gpio5)?;
    let en_l = PinDriver::output(peripherals.pins.gpio6)?;
    let en_r = PinDriver::output(peripherals.pins.gpio7)?;
    let motors = Arc::new(Mutex::new(motor::Motors::new(en_l, en_r, ch_l, ch_r)?));

    // Wi-Fi
    let _wifi = wifi::connect(peripherals.modem, sys_loop, nvs)?;

    // HTTP server
    let mut server = EspHttpServer::new(&esp_idf_svc::http::server::Configuration::default())?;
    http::register_handlers(&mut server, stepper, motors)?;

    loop {
        FreeRtos::delay_ms(1000);
    }
}
