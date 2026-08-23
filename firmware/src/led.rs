use esp_hal::gpio::{Level, OutputPin};
use esp_hal::peripherals::RMT;
use esp_hal::rmt::{Channel, PulseCode, Rmt, Tx, TxChannelConfig, TxChannelCreator};
use esp_hal::Blocking;
use esp_hal::time::Rate;
use zigbee::{COLOUR_TEMPERATURE_MIREDS, MAX_HUE};

/// The RMT source clock the ESP32-H2 offers. One tick is 31.25 ns, which is
/// what the bit timings below are counted in.
const CLOCK: Rate = Rate::from_mhz(32);

const T0_HIGH: u16 = 11;
const T0_LOW: u16 = 29;
const T1_HIGH: u16 = 22;
const T1_LOW: u16 = 19;

/// A low period long enough for the LED to latch the colour it just received.
const RESET: u16 = 9600;

const BITS: usize = 24;

/// The whole hue circle, which is one step past the brightest hue because the
/// range starts at zero.
const WHEEL: u16 = MAX_HUE as u16 + 1;

const DAYLIGHT: Rgb = Rgb::new(201, 226, 255);
const CANDLE: Rgb = Rgb::new(255, 157, 63);

const fn mix(from: u8, to: u8, towards: u8) -> u8 {
    ((from as u16 * (255 - towards as u16) + to as u16 * towards as u16) / 255) as u8
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

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

        let shade = |towards: u16| (value as u16 * (255 - saturation as u16 * towards / 255) / 255) as u8;
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

    /// A white from a colour temperature, mixed between the two ends of what
    /// this LED can pretend to be rather than computed from a black body.
    pub fn from_mireds(mireds: u16, value: u8) -> Self {
        let coolest = *COLOUR_TEMPERATURE_MIREDS.start();
        let warmest = *COLOUR_TEMPERATURE_MIREDS.end();
        let along = (mireds.clamp(coolest, warmest) - coolest) as u32 * 255
            / (warmest - coolest) as u32;
        DAYLIGHT.blend(CANDLE, along as u8).dim(value)
    }

    fn grb(self) -> u32 {
        (self.green as u32) << 16 | (self.red as u32) << 8 | self.blue as u32
    }
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
        let grb = colour.grb();
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
