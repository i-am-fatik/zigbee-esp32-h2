#![no_std]
#![no_main]

extern crate alloc;

mod button;
mod led;
mod radio;
mod store;

use esp_backtrace as _;
use esp_hal::main;
use esp_hal::time::Instant as HalInstant;
use esp_println::{print, println};
use esp_radio::ieee802154::Ieee802154;
use zigbee::{Colour, Config, Device, Event, Instant};

use button::Button;
use led::{AddressableLed, Rgb};
use radio::Radio;
use store::Store;

esp_bootloader_esp_idf::esp_app_desc!();

/// What the board's LED says about the device: searching, joined and dark, or
/// switched on by the coordinator.
const SEARCHING: Rgb = Rgb::new(0, 0, 60);
const IDLE_ON_NETWORK: Rgb = Rgb::new(0, 40, 0);

const IDENTIFYING: Rgb = Rgb::new(255, 255, 255);

/// What the upgrade server is told this device is running. Bump the version
/// when releasing an image, or a server with a newer one will not offer it.
const MANUFACTURER_CODE: u16 = 0x1037;
const IMAGE_TYPE: u16 = 0x0001;
const FIRMWARE_VERSION: u32 = 0x0000_0001;

const BLINK_MS: u32 = 300;
const IDENTIFY_BLINK_MS: u32 = 120;

fn our_extended_address() -> u64 {
    let mac = esp_hal::efuse::base_mac_address();
    let mac = mac.as_bytes();
    u64::from_be_bytes([
        mac[0], mac[1], mac[2], 0xff, 0xfe, mac[3], mac[4], mac[5],
    ])
}

fn lit(colour: Colour, level: u8) -> Rgb {
    match colour {
        Colour::HueSaturation { hue, saturation } => {
            Rgb::from_hue_and_saturation(hue, saturation, level)
        }
        Colour::Temperature { mireds } => Rgb::from_mireds(mireds, level),
        _ => Rgb::from_hue_and_saturation(0, 0, level),
    }
}

fn now() -> Instant {
    Instant::from_millis(HalInstant::now().duration_since_epoch().as_millis() as u32)
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    esp_alloc::heap_allocator!(size: 32 * 1024);

    let mut store = Store::new(peripherals.FLASH);

    let ieee = our_extended_address();
    let mut config = Config::new(ieee)
        .with_manufacturer("esp-rs")
        .with_model("H2.NoStd.Light")
        .with_software_build(env!("CARGO_PKG_VERSION"))
        .with_firmware(MANUFACTURER_CODE, IMAGE_TYPE, FIRMWARE_VERSION);

    if let Some(code) = store.load_install_code() {
        config = config.with_install_code(code);
        print!("store: install code ");
        for octet in zigbee::install_code_label(&code) {
            print!("{:02X}", octet);
        }
        println!();
    }
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
    if let Some(tables) = store.load_tables() {
        device.restore_tables(tables);
        println!("store: groups and scenes came back from flash");
    }
    println!("boot: zigbee end device, eui64 {:016x}", ieee);

    let mut led = AddressableLed::new(peripherals.RMT, peripherals.GPIO8);
    let mut button = Button::new(peripherals.GPIO9, now());
    let mut tuning = device.radio();
    let mut radio = Radio::new(Ieee802154::new(peripherals.IEEE802154), tuning, ieee);

    let mut blink_at = now();
    let mut blink_lit = false;

    loop {
        let mut received = [0u8; 128];
        while let Some(len) = radio.receive(&mut received) {
            device.receive(&received[..len], now());
        }

        if button.was_pressed(now()) {
            device.set_on_off(!device.on_off());
        }

        device.tick(now());

        #[expect(
            clippy::while_let_loop,
            reason = "a while let would hold the borrow across abandon_firmware"
        )]
        loop {
            let refused = match device.next_firmware_block() {
                Some(block) => !store.write_firmware(block.offset, block.data),
                None => break,
            };
            if refused {
                println!("ota: the slot refused a block, giving up");
                device.abandon_firmware();
                break;
            }
        }

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
                Event::LevelChanged(level) => println!("zcl: brightness {}", level),
                Event::ColourChanged(colour) => println!("zcl: colour {:?}", colour),
                Event::FirmwareOffered { version, size } => {
                    println!("ota: image 0x{:08x}, {} octets", version, size);
                    store.begin_firmware();
                }
                Event::FirmwareReady => {
                    if store.activate_firmware() {
                        println!("ota: written, restarting into it");
                        esp_hal::system::software_reset();
                    }
                    println!("ota: the new image could not be activated");
                }
                Event::FirmwareAbandoned => {
                    println!("ota: update abandoned, the old image stands");
                    store.abandon_firmware();
                }
                Event::CredentialsChanged(credentials) => {
                    store.save(&credentials);
                }
                Event::TablesChanged(tables) => {
                    store.save_tables(&tables);
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
            lit(device.colour(), device.level())
        } else {
            IDLE_ON_NETWORK.dim(20)
        };
        if let Some(led) = led.as_mut() {
            led.show(colour);
        }
    }
}
