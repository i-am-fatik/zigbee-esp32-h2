use core::sync::atomic::{AtomicBool, Ordering};

use esp_hal::time::{Duration, Instant};
use esp_radio::ieee802154::{CcaMode, Config, Ieee802154, RawReceived};
use zigbee::RadioConfig;
use zigbee::MAX_FRAME_LEN;

/// The two trailing octets the PHY reserves for the checksum. Hardware fills
/// them on transmit and overwrites them with signal quality on receive.
const FCS_LEN: usize = 2;

/// The radio owns a single transmit buffer, so a frame has to be on the air
/// before the next one may be handed over.
static TRANSMIT_DONE: AtomicBool = AtomicBool::new(false);
static TRANSMIT_FAILED: AtomicBool = AtomicBool::new(false);
const TRANSMIT_TIMEOUT: Duration = Duration::from_millis(260);

/// A clear channel assessment loses against a busy network, so a frame that
/// was refused is offered again, finally without asking the channel first.
/// Measured on a network of eleven routers, each further attempt clears about
/// three quarters of what the previous one left.
const TRANSMIT_ATTEMPTS: u8 = 6;

/// Retrying into the same collision just collides again, so each attempt waits
/// a different, short while first.
fn back_off(attempt: u8) {
    let jitter = (Instant::now().duration_since_epoch().as_micros() as u32) & 0x0f;
    let millis = (1u32 << attempt) + jitter;
    let deadline = Instant::now() + Duration::from_millis(millis as u64);
    while Instant::now() < deadline {}
}

enum Attempt {
    Delivered,
    Refused,
    Unanswered,
}

fn transmit_done() {
    TRANSMIT_DONE.store(true, Ordering::Release);
}

fn transmit_failed() {
    TRANSMIT_FAILED.store(true, Ordering::Release);
    TRANSMIT_DONE.store(true, Ordering::Release);
}

pub struct Radio<'a> {
    driver: Ieee802154<'a>,
    config: Config,
}

impl<'a> Radio<'a> {
    pub fn new(driver: Ieee802154<'a>, wanted: RadioConfig, ext_addr: u64) -> Self {
        let config = Config {
            auto_ack_tx: true,
            auto_ack_rx: true,
            rx_when_idle: true,
            promiscuous: false,
            coordinator: false,
            enhance_ack_tx: false,
            txpower: 20,
            channel: wanted.channel,
            cca_mode: CcaMode::Carrier,
            pan_id: Some(wanted.pan_id),
            short_addr: Some(wanted.short_address),
            ext_addr: Some(ext_addr),
            rx_queue_size: 20,
            ..Config::default()
        };
        let mut radio = Self { driver, config };
        radio.driver.set_tx_done_callback_fn(transmit_done);
        radio.driver.set_tx_failed_callback_fn(transmit_failed);
        radio.apply();
        radio.driver.start_receive();
        radio
    }

    fn apply(&mut self) {
        self.driver.set_config(self.config);
    }

    pub fn tune(&mut self, wanted: RadioConfig) {
        self.config.channel = wanted.channel;
        self.config.pan_id = Some(wanted.pan_id);
        self.config.short_addr = Some(wanted.short_address);
        self.apply();
        self.driver.start_receive();
    }

    pub fn send(&mut self, frame: &[u8], cca: bool) -> bool {
        let mut padded = [0u8; 128];
        padded[..frame.len()].copy_from_slice(frame);
        let frame = &padded[..frame.len() + FCS_LEN];

        for attempt in 0..TRANSMIT_ATTEMPTS {
            let last_attempt = attempt + 1 == TRANSMIT_ATTEMPTS;
            match self.offer(frame, cca && !last_attempt) {
                Attempt::Delivered | Attempt::Unanswered => return true,
                Attempt::Refused => back_off(attempt),
            }
        }
        false
    }

    fn offer(&mut self, frame: &[u8], cca: bool) -> Attempt {
        TRANSMIT_DONE.store(false, Ordering::Release);
        TRANSMIT_FAILED.store(false, Ordering::Release);

        let _ = self.driver.transmit_raw(frame, cca);

        let deadline = Instant::now() + TRANSMIT_TIMEOUT;
        while !TRANSMIT_DONE.load(Ordering::Acquire) && Instant::now() < deadline {
            core::hint::spin_loop();
        }
        self.driver.start_receive();

        if !TRANSMIT_DONE.load(Ordering::Acquire) {
            Attempt::Unanswered
        } else if TRANSMIT_FAILED.load(Ordering::Acquire) {
            Attempt::Refused
        } else {
            Attempt::Delivered
        }
    }

    pub fn receive(&mut self, into: &mut [u8; MAX_FRAME_LEN]) -> Option<usize> {
        let RawReceived { data, .. } = self.driver.raw_received()?;
        let psdu_len = data[0] as usize;
        if psdu_len < FCS_LEN || psdu_len > data.len() - 1 {
            return None;
        }
        let len = psdu_len - FCS_LEN;
        into[..len].copy_from_slice(&data[1..1 + len]);
        Some(len)
    }
}
