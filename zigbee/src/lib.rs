#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(missing_docs, missing_debug_implementations)]

//! A Zigbee end device stack for `no_std` targets, written sans-io.
//!
//! The stack owns no radio, no clock and no storage. It is handed the frames a
//! radio received and the time they arrived, and it hands back the frames to
//! transmit, the events worth acting on, and the credentials worth keeping.
//! Everything the device needs to say is produced as bytes, so the same crate
//! runs on any IEEE 802.15.4 radio and in a test with no radio at all.
//!
//! The application is fixed: this crate is a colour light on one endpoint,
//! not a framework for building arbitrary Zigbee devices. [`APPLICATION`] is
//! the whole of what a coordinator will find.
//!
//! # Example
//!
//! One turn of the loop a caller writes. A real one runs it forever, sending
//! what [`Device::next_transmission`] produces and feeding back what the radio
//! heard.
//!
//! ```
//! use zigbee::{Config, Device, Event, Instant};
//!
//! let mut device = Device::new(
//!     Config::new(0x0011_2233_4455_6677)
//!         .with_manufacturer("esp-rs")
//!         .with_model("H2.NoStd.Light"),
//! );
//!
//! let now = Instant::from_millis(0);
//! device.tick(now);
//!
//! while let Some(outgoing) = device.next_transmission() {
//!     radio_send(outgoing.frame, outgoing.request_cca);
//! }
//!
//! if let Some(frame) = radio_receive() {
//!     device.receive(frame, now);
//! }
//!
//! while let Some(event) = device.next_event() {
//!     match event {
//!         Event::OnOffChanged(on) => light(on),
//!         Event::LevelChanged(level) => brightness(level),
//!         Event::CredentialsChanged(saved) => flash_write(&saved.to_bytes()),
//!         _ => {}
//!     }
//! }
//! # fn radio_send(_frame: &[u8], _request_cca: bool) {}
//! # fn radio_receive() -> Option<&'static [u8]> { None }
//! # fn light(_on: bool) {}
//! # fn brightness(_level: u8) {}
//! # fn flash_write(_bytes: &[u8]) {}
//! ```
//!
//! # What the caller owes
//!
//! Drive [`Device::tick`] often enough that a timeout of a few hundred
//! milliseconds is not missed, and drain [`Device::next_transmission`] and
//! [`Device::next_event`] every turn. Both queues hold four entries and drop
//! the oldest when full. A brightness ramp advances on the same call, so the
//! tick rate is also how smoothly the light dims.
//!
//! Retune the radio whenever [`Device::radio`] changes: during a scan the
//! stack walks the channels itself, and a frame arriving on the wrong channel
//! or for the wrong PAN never reaches it.
//!
//! Persist the bytes from every [`Event::CredentialsChanged`] and hand them to
//! [`Device::restore`] after a restart. Skipping that costs a fresh join, and
//! a fresh join needs the coordinator to be permitting one.
//!
//! # What the stack promises
//!
//! [`Device::receive`] accepts any bytes at all. A frame that is too long,
//! truncated, mis-framed or hostile is dropped in silence, and no input can
//! make it panic. Nothing in this crate allocates, blocks, or performs I/O,
//! and no public type is borrowed from another crate.
//!
//! [`Device`] is a state machine with one owner. It is `Send` and `Sync`
//! because it holds only plain data, but two callers driving one device will
//! interleave its frames.
//!
mod aps;
mod buf;
mod crypto;
mod device;
mod mac;
mod nwk;
mod zcl;
mod zdo;

pub use device::{Colour, Config, Credentials, Device, Event, RadioConfig, Transmission};

/// A point on a monotonic millisecond clock.
///
/// The stack only ever asks how long ago something happened, so the value may
/// start anywhere and may wrap. Comparisons stay correct while the intervals
/// being measured are shorter than about 24 days.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Instant(u32);

impl Instant {
    /// Reads a millisecond count from whatever clock the caller has.
    pub const fn from_millis(millis: u32) -> Self {
        Self(millis)
    }

    /// Milliseconds from `earlier` to `self`.
    pub const fn millis_since(self, earlier: Self) -> u32 {
        self.0.wrapping_sub(earlier.0)
    }

    /// The instant `millis` later, wrapping with the clock.
    pub const fn plus_millis(self, millis: u32) -> Self {
        Self(self.0.wrapping_add(millis))
    }

    pub(crate) const fn reached(self, deadline: Self) -> bool {
        self.millis_since(deadline) < u32::MAX / 2
    }
}

/// The channels Zigbee uses within the 2.4 GHz IEEE 802.15.4 band.
pub const CHANNELS: core::ops::RangeInclusive<u8> = 11..=26;

/// What a coordinator finds when it interviews the device.
///
/// This is the same description the stack puts in its simple descriptor, so a
/// tool that writes coordinator-side configuration can read it here rather than
/// guessing from a datasheet.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub struct Application {
    /// The endpoint the application answers on.
    pub endpoint: u8,
    /// The application profile, Home Automation.
    pub profile: u16,
    /// The device identifier within that profile.
    pub device_id: u16,
    /// The clusters the device serves on that endpoint.
    pub clusters: &'static [u16],
}

/// The colour light this crate ships.
pub const APPLICATION: Application = Application {
    endpoint: zdo::ENDPOINT,
    profile: aps::PROFILE_HOME_AUTOMATION,
    device_id: zdo::DEVICE_ID_COLOUR_LIGHT,
    clusters: &zdo::INPUT_CLUSTERS,
};

/// The brightest [`Device::set_level`] and the Level Control cluster go. 0xff
/// is reserved for "undefined", so the usable range stops one short of it.
pub const MAX_LEVEL: u8 = zcl::MAX_LEVEL;

/// The furthest round the colour wheel a hue goes before it comes back to
/// where it started.
pub const MAX_HUE: u8 = zcl::MAX_HUE;

/// The furthest a colour gets from white.
pub const MAX_SATURATION: u8 = zcl::MAX_SATURATION;

/// The colour temperatures the light accepts, in mireds. A bridge is expected
/// to read this range off the device rather than assume one.
pub const COLOUR_TEMPERATURE_MIREDS: core::ops::RangeInclusive<u16> =
    zcl::COOLEST_MIREDS..=zcl::WARMEST_MIREDS;

/// The Basic cluster, which every device serves and no extend describes.
pub const CLUSTER_BASIC: u16 = zdo::CLUSTER_BASIC;
/// The Identify cluster.
pub const CLUSTER_IDENTIFY: u16 = zdo::CLUSTER_IDENTIFY;
/// The On/Off cluster.
pub const CLUSTER_ON_OFF: u16 = zdo::CLUSTER_ON_OFF;
/// The Level Control cluster, which carries the brightness.
pub const CLUSTER_LEVEL_CONTROL: u16 = zdo::CLUSTER_LEVEL_CONTROL;
/// The Colour Control cluster.
pub const CLUSTER_COLOUR_CONTROL: u16 = zdo::CLUSTER_COLOUR_CONTROL;
