#![no_std]
#![no_main]

extern crate alloc;

mod led;
mod radio;
mod store;

use esp_backtrace as _;
use esp_hal::main;
use esp_hal::time::Instant as HalInstant;
use esp_println::println;
use esp_radio::ieee802154::Ieee802154;
use zigbee::{Config, Device, Event, Instant};

use led::{AddressableLed, Rgb};
use radio::Radio;
use store::Store;

esp_bootloader_esp_idf::esp_app_desc!();

/// What the board's LED says about the device: searching, joined and dark, or
/// switched on by the coordinator.
const SEARCHING: Rgb = Rgb::new(0, 0, 60);
const IDLE_ON_NETWORK: Rgb = Rgb::new(0, 40, 0);
const LIGHT_ON: Rgb = Rgb::new(255, 170, 70);

const IDENTIFYING: Rgb = Rgb::new(255, 255, 255);

const BLINK_MS: u32 = 300;
const IDENTIFY_BLINK_MS: u32 = 120;

fn our_extended_address() -> u64 {
    let mac = esp_hal::efuse::base_mac_address();
    let mac = mac.as_bytes();
    u64::from_be_bytes([
        mac[0], mac[1], mac[2], 0xff, 0xfe, mac[3], mac[4], mac[5],
    ])
}

fn now() -> Instant {
    Instant::from_millis(HalInstant::now().duration_since_epoch().as_millis() as u32)
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    esp_alloc::heap_allocator!(size: 32 * 1024);

    let ieee = our_extended_address();
    let config = Config::new(ieee)
        .with_manufacturer("esp-rs")
        .with_model("H2.NoStd.Light")
        .with_software_build(env!("CARGO_PKG_VERSION"));

    let mut store = Store::new(peripherals.FLASH);
    let mut device = match store.load() {
        Some(credentials) => {
            println!(
                "store: rejoining as 0x{:04x} on channel {}",
                credentials.short_address(),
                credentials.channel()
            );
            Device::restore(config, credentials)
        }
        None => Device::new(config),
    };
    println!("boot: zigbee end device, eui64 {:016x}", ieee);

    let mut led = AddressableLed::new(peripherals.RMT, peripherals.GPIO8);
    let mut tuning = device.radio();
    let mut radio = Radio::new(Ieee802154::new(peripherals.IEEE802154), tuning, ieee);

    let mut blink_at = now();
    let mut blink_lit = false;

    loop {
        let mut received = [0u8; 128];
        while let Some(len) = radio.receive(&mut received) {
            device.receive(&received[..len], now());
        }

        device.tick(now());

        let wanted = device.radio();
        if wanted != tuning {
            radio.tune(wanted);
            tuning = wanted;
        }

        #[expect(
            clippy::while_let_loop,
            reason = "a while let would hold the borrow across transmission_failed"
        )]
        loop {
            let delivered = match device.next_transmission() {
                Some(outgoing) => radio.send(outgoing.frame, outgoing.request_cca),
                None => break,
            };
            if delivered {
                device.transmission_delivered();
            } else {
                println!("radio: gave up transmitting");
                device.transmission_failed(now());
            }
        }

        while let Some(event) = device.next_event() {
            match event {
                Event::Joined { short_address } => {
                    println!("join: on network, short 0x{:04x}", short_address)
                }
                Event::Left => {
                    println!("join: left the network");
                    store.forget();
                }
                Event::OnOffChanged(on) => {
                    println!("zcl: light is now {}", if on { "ON" } else { "OFF" })
                }
                Event::CredentialsChanged(credentials) => {
                    store.save(&credentials);
                }
                _ => {}
            }
        }

        let period = if device.identifying() {
            IDENTIFY_BLINK_MS
        } else {
            BLINK_MS
        };
        if now().millis_since(blink_at) >= period {
            blink_at = now();
            blink_lit = !blink_lit;
        }

        let colour = if device.identifying() {
            if blink_lit {
                IDENTIFYING.dim(160)
            } else {
                Rgb::OFF
            }
        } else if !device.joined() {
            if blink_lit {
                SEARCHING
            } else {
                Rgb::OFF
            }
        } else if device.on_off() {
            LIGHT_ON.dim(140)
        } else {
            IDLE_ON_NETWORK.dim(20)
        };
        if let Some(led) = led.as_mut() {
            led.show(colour);
        }
    }
}
