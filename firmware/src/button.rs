use esp_hal::gpio::{Input, InputConfig, InputPin, Pull};
use zigbee::Instant;

const DEBOUNCE_MS: u32 = 25;

pub struct Button<'d> {
    pin: Input<'d>,
    down: bool,
    steady_since: Instant,
}

impl<'d> Button<'d> {
    pub fn new(pin: impl InputPin + 'd, now: Instant) -> Self {
        Self {
            pin: Input::new(pin, InputConfig::default().with_pull(Pull::Up)),
            down: false,
            steady_since: now,
        }
    }

    pub fn was_pressed(&mut self, now: Instant) -> bool {
        let down = self.pin.is_low();
        if down == self.down {
            self.steady_since = now;
            return false;
        }
        if now.millis_since(self.steady_since) < DEBOUNCE_MS {
            return false;
        }
        self.down = down;
        down
    }
}
