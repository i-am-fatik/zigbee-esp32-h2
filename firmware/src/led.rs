#[cfg(not(feature = "xiao-esp32c6"))]
pub use addressable::AddressableLed;
#[cfg(feature = "xiao-esp32c6")]
pub use plain::PlainLed;

#[cfg(not(feature = "xiao-esp32c6"))]
use zigbee::{COLOUR_TEMPERATURE_MIREDS, MAX_HUE};

#[cfg(not(feature = "xiao-esp32c6"))]
/// The whole hue circle, which is one step past the brightest hue because the
/// range starts at zero.
const WHEEL: u16 = MAX_HUE as u16 + 1;

#[cfg(not(feature = "xiao-esp32c6"))]
const XY_ONE: i64 = 65_536;

#[cfg(not(feature = "xiao-esp32c6"))]
const DAYLIGHT: Rgb = Rgb::new(201, 226, 255);
#[cfg(not(feature = "xiao-esp32c6"))]
const CANDLE: Rgb = Rgb::new(255, 157, 63);

#[cfg(not(feature = "xiao-esp32c6"))]
const fn mix(from: u8, to: u8, towards: u8) -> u8 {
    ((from as u16 * (255 - towards as u16) + to as u16 * towards as u16) / 255) as u8
}

#[cfg(not(feature = "xiao-esp32c6"))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

#[cfg(not(feature = "xiao-esp32c6"))]
impl Rgb {
    pub const OFF: Rgb = Rgb::new(0, 0, 0);

    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    pub const fn dim(self, numerator: u8) -> Self {
        Self {
            red: (self.red as u16 * numerator as u16 / 255) as u8,
            green: (self.green as u16 * numerator as u16 / 255) as u8,
            blue: (self.blue as u16 * numerator as u16 / 255) as u8,
        }
    }

    const fn blend(self, other: Rgb, towards: u8) -> Self {
        Self {
            red: mix(self.red, other.red, towards),
            green: mix(self.green, other.green, towards),
            blue: mix(self.blue, other.blue, towards),
        }
    }

    pub fn from_hue_and_saturation(hue: u8, saturation: u8, value: u8) -> Self {
        if saturation == 0 {
            return Rgb::new(value, value, value);
        }

        let scaled = hue as u16 * 6;
        let sector = scaled / WHEEL;
        let along = scaled % WHEEL;

        let shade =
            |towards: u16| (value as u16 * (255 - saturation as u16 * towards / 255) / 255) as u8;
        let bottom = shade(255);
        let falling = shade(along);
        let rising = shade(255 - along);

        match sector {
            0 => Rgb::new(value, rising, bottom),
            1 => Rgb::new(falling, value, bottom),
            2 => Rgb::new(bottom, value, rising),
            3 => Rgb::new(bottom, falling, value),
            4 => Rgb::new(rising, bottom, value),
            _ => Rgb::new(value, bottom, falling),
        }
    }

    pub fn from_xy(x: u16, y: u16, value: u8) -> Self {
        if y == 0 {
            return Rgb::new(value, value, value);
        }

        let tristimulus_x = XY_ONE * x as i64 / y as i64;
        let tristimulus_z = XY_ONE * (XY_ONE - x as i64 - y as i64) / y as i64;
        let srgb_primary = |from_x: i64, from_y: i64, from_z: i64| {
            (from_x * tristimulus_x + from_y * XY_ONE + from_z * tristimulus_z) / 10_000
        };

        let red = srgb_primary(32_406, -15_372, -4_986);
        let green = srgb_primary(-9_689, 18_758, 415);
        let blue = srgb_primary(557, -2_040, 10_570);

        let brightest = red.max(green).max(blue).max(1);
        let scaled_to_full = |primary: i64| (primary.max(0) * 255 / brightest) as u8;
        Rgb::new(
            scaled_to_full(red),
            scaled_to_full(green),
            scaled_to_full(blue),
        )
        .dim(value)
    }

    /// A white from a colour temperature, mixed between the two ends of what
    /// this LED can pretend to be rather than computed from a black body.
    pub fn from_mireds(mireds: u16, value: u8) -> Self {
        let coolest = *COLOUR_TEMPERATURE_MIREDS.start();
        let warmest = *COLOUR_TEMPERATURE_MIREDS.end();
        let along =
            (mireds.clamp(coolest, warmest) - coolest) as u32 * 255 / (warmest - coolest) as u32;
        DAYLIGHT.blend(CANDLE, along as u8).dim(value)
    }
}

#[cfg(not(feature = "xiao-esp32c6"))]
mod addressable {
    use esp_hal::gpio::{Level, OutputPin};
    use esp_hal::peripherals::RMT;
    use esp_hal::rmt::{Channel, PulseCode, Rmt, Tx, TxChannelConfig, TxChannelCreator};
    use esp_hal::time::Rate;
    use esp_hal::Blocking;

    use super::Rgb;

    /// The RMT source clock differs per chip, so the bit timings are kept in
    /// nanoseconds and counted into ticks below.
    #[cfg(feature = "esp32h2")]
    const CLOCK_MHZ: u32 = 32;
    #[cfg(feature = "esp32c6")]
    const CLOCK_MHZ: u32 = 80;
    const CLOCK: Rate = Rate::from_mhz(CLOCK_MHZ);

    const fn ticks(nanoseconds: u32) -> u16 {
        (nanoseconds * CLOCK_MHZ / 1000) as u16
    }

    const T0_HIGH: u16 = ticks(350);
    const T0_LOW: u16 = ticks(900);
    const T1_HIGH: u16 = ticks(700);
    const T1_LOW: u16 = ticks(600);

    /// A low period long enough for the LED to latch the colour it just received.
    const RESET: u16 = ticks(300_000);

    const BITS: usize = 24;

    fn grb(colour: Rgb) -> u32 {
        (colour.green as u32) << 16 | (colour.red as u32) << 8 | colour.blue as u32
    }

    /// The single WS2812 style LED soldered to the development board.
    pub struct AddressableLed<'a> {
        channel: Option<Channel<'a, Blocking, Tx>>,
        shown: Option<Rgb>,
    }

    impl<'a> AddressableLed<'a> {
        pub fn new(rmt: RMT<'a>, pin: impl OutputPin + 'a) -> Option<Self> {
            let rmt = Rmt::new(rmt, CLOCK).ok()?;
            let channel = rmt
                .channel0
                .configure_tx(
                    &TxChannelConfig::default()
                        .with_clk_divider(1)
                        .with_idle_output(true)
                        .with_idle_output_level(Level::Low),
                )
                .ok()?
                .with_pin(pin);

            Some(Self {
                channel: Some(channel),
                shown: None,
            })
        }

        pub fn show(&mut self, colour: Rgb) {
            if self.shown == Some(colour) {
                return;
            }

            let mut codes = [PulseCode::default(); BITS + 2];
            let grb = grb(colour);
            for (index, code) in codes[..BITS].iter_mut().enumerate() {
                let bit_set = grb & (1 << (BITS - 1 - index)) != 0;
                *code = if bit_set {
                    PulseCode::new(Level::High, T1_HIGH, Level::Low, T1_LOW)
                } else {
                    PulseCode::new(Level::High, T0_HIGH, Level::Low, T0_LOW)
                };
            }
            codes[BITS] = PulseCode::new(Level::Low, RESET, Level::Low, 0);

            let Some(channel) = self.channel.take() else {
                return;
            };
            self.channel = match channel.transmit(&codes) {
                Ok(transaction) => match transaction.wait() {
                    Ok(channel) => {
                        self.shown = Some(colour);
                        Some(channel)
                    }
                    Err((_, channel)) => Some(channel),
                },
                Err((_, channel)) => Some(channel),
            };
        }
    }
}

#[cfg(feature = "xiao-esp32c6")]
mod plain {
    extern crate alloc;

    use alloc::boxed::Box;
    use esp_hal::gpio::{DriveMode, OutputPin};
    use esp_hal::ledc::channel::{self, ChannelIFace};
    use esp_hal::ledc::timer::{self, TimerIFace};
    use esp_hal::ledc::{LSGlobalClkSource, Ledc, LowSpeed};
    use esp_hal::peripherals::LEDC;
    use esp_hal::time::Rate;

    const FULL: u32 = zigbee::MAX_LEVEL as u32;
    const FAINTEST_VISIBLE_PERCENT: u8 = 1;

    /// The single-colour LED the XIAO boards carry, lit by pulling its pin
    /// low. It is dimmed with PWM, so it follows the brightness of the light
    /// and not only whether it is on.
    pub struct PlainLed<'a> {
        channel: channel::Channel<'a, LowSpeed>,
        shown: Option<u8>,
    }

    impl<'a> PlainLed<'a> {
        pub fn new(ledc: LEDC<'a>, pin: impl OutputPin + 'a) -> Option<Self> {
            let mut ledc = Ledc::new(ledc);
            ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);
            let ledc: &'a Ledc<'a> = Box::leak(Box::new(ledc));
            let mut timer = ledc.timer::<LowSpeed>(timer::Number::Timer0);
            timer
                .configure(timer::config::Config {
                    duty: timer::config::Duty::Duty8Bit,
                    clock_source: timer::LSClockSource::APBClk,
                    frequency: Rate::from_khz(2),
                })
                .ok()?;
            let timer: &'a timer::Timer<'a, LowSpeed> = Box::leak(Box::new(timer));
            let mut channel = ledc.channel(channel::Number::Channel0, pin);
            channel
                .configure(channel::config::Config {
                    timer,
                    duty_pct: 100,
                    drive_mode: DriveMode::PushPull,
                })
                .ok()?;
            Some(Self {
                channel,
                shown: None,
            })
        }

        /// Shows a brightness from 0 (dark) to `MAX_LEVEL`; the eye sees
        /// duty squared as even steps, so the level is squared first.
        pub fn show(&mut self, level: u8) {
            if self.shown == Some(level) {
                return;
            }
            let level = level as u32;
            let mut lit_percent = (level * level * 100 / (FULL * FULL)) as u8;
            if level > 0 {
                lit_percent = lit_percent.max(FAINTEST_VISIBLE_PERCENT);
            }
            if self.channel.set_duty(100 - lit_percent).is_ok() {
                self.shown = Some(level as u8);
            }
        }
    }
}
