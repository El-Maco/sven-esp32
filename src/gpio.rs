use embassy_time::{Duration, Timer};
use esp_hal::gpio::Output;
use log::info;

pub struct PulsePin<'d> {
    pin: Output<'d>,
    active_high: bool,
}

impl<'d> PulsePin<'d> {
    pub fn new(pin: Output<'d>, active_high: bool) -> Self {
        Self { pin, active_high }
    }

    pub async fn pulse(&mut self, duration: u32) {
        let duration = Duration::from_millis(duration as u64);
        if self.active_high {
            self.pin.set_high();
        } else {
            self.pin.set_low();
        }
        Timer::after(duration).await;
        if self.active_high {
            self.pin.set_low();
        } else {
            self.pin.set_high();
        }
    }
}
