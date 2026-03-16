use esp_idf_hal::ledc::LedcDriver;

pub fn set_angle(servo: &mut LedcDriver<'_>, angle: u32) {
    let max_duty = servo.get_max_duty();
    let pulse_us = 500 + (angle * 2000) / 180;
    let duty = (pulse_us * max_duty) / 20_000;
    let _ = servo.set_duty(duty);
}
