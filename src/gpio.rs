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
            info!("Pulsing pin high for {} ms", duration.as_millis());
            self.pin.set_high();
        } else {
            info!("Pulsing pin low for {} ms", duration.as_millis());
            self.pin.set_low();
        }
        Timer::after(duration).await;
        if self.active_high {
            info!("Setting pin low after pulse");
            self.pin.set_low();
        } else {
            info!("Setting pin high after pulse");
            self.pin.set_high();
        }
    }
}
