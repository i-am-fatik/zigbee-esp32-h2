//! What every integration test needs to put a frame in front of the stack.

#![allow(dead_code)]

use zigbee::{Config, Device, Event, Instant, APPLICATION, CLUSTER_GROUPS, CLUSTER_SCENES};

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

/// The association response an EmberZNet coordinator sent this device.
pub const ASSOCIATION_RESPONSE: &[u8] = &[
    0x63, 0xcc, 0xce, 0x69, 0xa2, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0x00, 0xff, 0xee, 0xdd,
    0xcc, 0xbb, 0xaa, 0x99, 0x88, 0x02, 0x60, 0x45, 0x00,
];

/// The application-support command carrying the network key, encrypted by the
/// trust centre under the key derived from the well known link key.
pub const TRANSPORT_KEY: &[u8] = &[
    0x21, 0xbd, 0x30, 0x00, 0x30, 0x00, 0x00, 0xff, 0xee, 0xdd, 0xcc, 0xbb, 0xaa, 0x99, 0x88, 0xb5,
    0x2c, 0x03, 0x1e, 0x47, 0xb4, 0xff, 0x2f, 0x20, 0xa6, 0xb3, 0x1c, 0xd2, 0x0d, 0x4c, 0x32, 0xea,
    0x81, 0x49, 0xd2, 0x06, 0xe2, 0x65, 0xab, 0xa2, 0x24, 0xd9, 0x5b, 0xa3, 0xf9, 0x70, 0x85, 0xaa,
    0x06, 0x5a, 0x89, 0x8c, 0xfc, 0x6d,
];

pub fn beacon(permit_join: bool) -> Vec<u8> {
    let superframe: u16 = if permit_join { 0xcfff } else { 0x4fff };
    let mut frame = vec![0x00, 0x80, 0x01];
    frame.extend_from_slice(&PAN.to_le_bytes());
    frame.extend_from_slice(&0x0000u16.to_le_bytes());
    frame.extend_from_slice(&superframe.to_le_bytes());
    frame.extend_from_slice(&[0x00, 0x00]);
    frame.extend_from_slice(&[0x00, 0x22, 0x84]);
    frame.extend_from_slice(&0x57c3_ec80_bbff_440au64.to_le_bytes());
    frame.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
    frame
}

pub fn transmissions(device: &mut Device) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    while let Some(outgoing) = device.next_transmission() {
        frames.push(outgoing.frame.to_vec());
    }
    frames
}

/// A device that has been through a real join, so it holds a network key and
/// will actually put its replies on the air.
pub fn joined_device() -> Device {
    let mut device = device();
    join(&mut device);
    device
}

/// Puts a device through a real join, whatever configuration it was built with.
pub fn join(device: &mut Device) {
    device.tick(at(0));
    device.receive(&beacon(true), at(10));
    device.tick(at(260));
    device.receive(ASSOCIATION_RESPONSE, at(270));
    device.receive(&deliver(TRANSPORT_KEY), at(280));
    drain(device);
}

/// One cluster-specific command sent to a group rather than to this device,
/// which is how a coordinator drives every member of a group at once.
pub fn group_command(group: u16, cluster: u16, seq: u8, id: u8, arguments: &[u8]) -> Vec<u8> {
    let mut request = vec![0x01, seq, id];
    request.extend_from_slice(arguments);

    let mut application = vec![3 << 2];
    application.extend_from_slice(&group.to_le_bytes());
    application.extend_from_slice(&cluster.to_le_bytes());
    application.extend_from_slice(&APPLICATION.profile.to_le_bytes());
    application.extend_from_slice(&[0x01, 0x07]);
    application.extend_from_slice(&request);

    let mut frame = vec![0x61, 0x88, 0x12];
    frame.extend_from_slice(&PAN.to_le_bytes());
    frame.extend_from_slice(&0xffffu16.to_le_bytes());
    frame.extend_from_slice(&0x0000u16.to_le_bytes());
    frame.extend_from_slice(&[0x08, 0x00]);
    frame.extend_from_slice(&0xfffdu16.to_le_bytes());
    frame.extend_from_slice(&0x0000u16.to_le_bytes());
    frame.extend_from_slice(&[30, 0x43]);
    frame.extend_from_slice(&application);
    frame
}

/// A network-layer command frame, unsecured, which is how these tests reach the
/// commands a trust centre normally sends under the network key.
pub fn network_command(payload: &[u8]) -> Vec<u8> {
    let mut frame = vec![0x61, 0x88, 0x13];
    frame.extend_from_slice(&PAN.to_le_bytes());
    frame.extend_from_slice(&OUR_SHORT.to_le_bytes());
    frame.extend_from_slice(&0x0000u16.to_le_bytes());
    frame.extend_from_slice(&[0x09, 0x00]);
    frame.extend_from_slice(&OUR_SHORT.to_le_bytes());
    frame.extend_from_slice(&0x0000u16.to_le_bytes());
    frame.extend_from_slice(&[30, 0x44]);
    frame.extend_from_slice(payload);
    frame
}

/// An unsecured Transport Key carrying a network key, which is what a trust
/// centre sends both to let a device in and to move it to a new key.
pub fn transport_network_key(key: &[u8; 16], sequence: u8, to: u64) -> Vec<u8> {
    let mut application = vec![0x01, 0x08, 0x05, 0x01];
    application.extend_from_slice(key);
    application.push(sequence);
    application.extend_from_slice(&to.to_le_bytes());
    deliver(&application)
}

const GROUP_ADD: u8 = 0x00;
const SCENE_STORE: u8 = 0x04;
const SCENE_RECALL: u8 = 0x05;

/// Puts the device in a group, which is what every group and scene test needs
/// before a group means anything to it.
pub fn join_group(device: &mut Device, group: u16, now: u32) {
    let mut arguments = group.to_le_bytes().to_vec();
    arguments.push(0);
    device.receive(
        &command(CLUSTER_GROUPS, 0xa0, GROUP_ADD, &arguments),
        at(now),
    );
}

pub fn store_scene(device: &mut Device, group: u16, scene: u8, now: u32) {
    let mut arguments = group.to_le_bytes().to_vec();
    arguments.push(scene);
    device.receive(
        &command(CLUSTER_SCENES, 0xa1, SCENE_STORE, &arguments),
        at(now),
    );
}

pub fn recall_scene(device: &mut Device, group: u16, scene: u8, now: u32) {
    let mut arguments = group.to_le_bytes().to_vec();
    arguments.push(scene);
    device.receive(
        &command(CLUSTER_SCENES, 0xa2, SCENE_RECALL, &arguments),
        at(now),
    );
}
