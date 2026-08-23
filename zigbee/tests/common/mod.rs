//! What every integration test needs to put a frame in front of the stack.

#![allow(dead_code)]

use zigbee::{Config, Device, Event, Instant, APPLICATION};

pub const OUR_IEEE: u64 = 0x0011_2233_4455_6677;
pub const PAN: u16 = 0xa269;
pub const OUR_SHORT: u16 = 0x4560;

pub fn device() -> Device {
    Device::new(Config::new(OUR_IEEE))
}

pub fn at(millis: u32) -> Instant {
    Instant::from_millis(millis)
}

pub fn events(device: &mut Device) -> Vec<Event> {
    let mut collected = Vec::new();
    while let Some(event) = device.next_event() {
        collected.push(event);
    }
    collected
}

pub fn drain(device: &mut Device) {
    while device.next_transmission().is_some() {}
    while device.next_event().is_some() {}
}

/// A MAC data frame carrying an unsecured network frame, which is how a
/// coordinator talks to a device that does not hold the network key yet.
pub fn deliver(application: &[u8]) -> Vec<u8> {
    let mut frame = vec![0x61, 0x88, 0x11];
    frame.extend_from_slice(&PAN.to_le_bytes());
    frame.extend_from_slice(&OUR_SHORT.to_le_bytes());
    frame.extend_from_slice(&0x0000u16.to_le_bytes());
    frame.extend_from_slice(&[0x08, 0x00]);
    frame.extend_from_slice(&OUR_SHORT.to_le_bytes());
    frame.extend_from_slice(&0x0000u16.to_le_bytes());
    frame.extend_from_slice(&[30, 0x42]);
    frame.extend_from_slice(application);
    frame
}

pub fn application_frame(cluster: u16, payload: &[u8]) -> Vec<u8> {
    let mut frame = vec![0x00, APPLICATION.endpoint];
    frame.extend_from_slice(&cluster.to_le_bytes());
    frame.extend_from_slice(&APPLICATION.profile.to_le_bytes());
    frame.extend_from_slice(&[0x01, 0x07]);
    frame.extend_from_slice(payload);
    frame
}

/// One cluster-specific command, wrapped in everything below it and ready for
/// [`zigbee::Device::receive`].
pub fn command(cluster: u16, seq: u8, id: u8, arguments: &[u8]) -> Vec<u8> {
    let mut request = vec![0x01, seq, id];
    request.extend_from_slice(arguments);
    deliver(&application_frame(cluster, &request))
}
