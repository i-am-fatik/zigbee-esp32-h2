#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! A Zigbee end device stack for `no_std` targets, written sans-io.
//!
//! The stack owns no radio, no clock and no storage. It is handed the frames a
//! radio received and the time they arrived, and it hands back the frames to
//! transmit, the events worth acting on, and the credentials worth keeping.
//! Everything the device needs to say is produced as bytes, so the same crate
//! runs on any IEEE 802.15.4 radio and in a test with no radio at all.
//!
//! # Example
//!
//! ```
//! use zigbee::{Config, Device, Event, Instant};
//!
//! let mut device = Device::new(Config::new(0x0011_2233_4455_6677));
//! let mut elapsed = 0;
//!
//! // A real caller drives this from a radio and a monotonic clock.
//! device.tick(Instant::from_millis(elapsed));
//! while let Some(outgoing) = device.next_transmission() {
//!     let _ = (outgoing.frame, outgoing.request_cca);
//! }
//! elapsed += 10;
//!
//! device.receive(&[0x02, 0x00, 0x01], Instant::from_millis(elapsed));
//! while let Some(event) = device.next_event() {
//!     match event {
//!         Event::OnOffChanged(on) => assert!(on || !on),
//!         _ => {}
//!     }
//! }
//! ```

mod aps;
mod buf;
mod crypto;
mod device;
mod mac;
mod nwk;
mod zcl;
mod zdo;

pub use device::{Config, Credentials, Device, Event, RadioConfig, Transmission};

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

/// The On/Off light this crate ships.
pub const APPLICATION: Application = Application {
    endpoint: zdo::ENDPOINT,
    profile: aps::PROFILE_HOME_AUTOMATION,
    device_id: zdo::DEVICE_ID_ON_OFF_LIGHT,
    clusters: &zdo::INPUT_CLUSTERS,
};

/// The Basic cluster, which every device serves and no extend describes.
pub const CLUSTER_BASIC: u16 = zdo::CLUSTER_BASIC;
/// The Identify cluster.
pub const CLUSTER_IDENTIFY: u16 = zdo::CLUSTER_IDENTIFY;
/// The On/Off cluster.
pub const CLUSTER_ON_OFF: u16 = zdo::CLUSTER_ON_OFF;
