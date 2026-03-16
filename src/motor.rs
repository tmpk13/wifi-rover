use anyhow::Result;
use esp_idf_hal::{
    gpio::{Output, PinDriver},
    ledc::LedcDriver,
};

/// Duty cycle percentage, clamped to 0–100.
pub type DutyCycle = u8;

pub struct Motors<'d> {
    en_l: PinDriver<'d, Output>,
    en_r: PinDriver<'d, Output>,
    ch_l: LedcDriver<'d>,
    ch_r: LedcDriver<'d>,
}

impl<'d> Motors<'d> {
    /// Takes pre-constructed enable pin drivers and LEDC channel drivers.
    /// Set up the timer and channels in `main` (same pattern as the servo).
    pub fn new(
        mut en_l: PinDriver<'d, Output>,
        mut en_r: PinDriver<'d, Output>,
        ch_l: LedcDriver<'d>,
        ch_r: LedcDriver<'d>,
    ) -> Result<Self> {
        en_l.set_high()?;
        en_r.set_high()?;
        Ok(Self { en_l, en_r, ch_l, ch_r })
    }

    /// Set left and right motor speeds as a percentage (0–100).
    pub fn drive(&mut self, left: DutyCycle, right: DutyCycle) -> Result<()> {
        self.ch_l.set_duty(pct_to_duty(left, self.ch_l.get_max_duty()))?;
        self.ch_r.set_duty(pct_to_duty(right, self.ch_r.get_max_duty()))?;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        self.drive(0, 0)
    }
}

fn pct_to_duty(pct: DutyCycle, max: u32) -> u32 {
    (pct.min(100) as u32 * max) / 100
}
