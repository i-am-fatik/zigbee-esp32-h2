//! Feeds the stack the frames a hostile or broken neighbour can put on the air
//! and asserts it stays alive. Everything here goes in through `receive`, the
//! one door the radio has.

use zigbee::{Config, Device, Instant, APPLICATION, CLUSTER_BASIC};

const OUR_IEEE: u64 = 0x0011_2233_4455_6677;
const PAN: u16 = 0xa269;
const OUR_SHORT: u16 = 0x4560;

fn device() -> Device {
    Device::new(Config::new(OUR_IEEE))
}

fn drain(device: &mut Device) {
    while device.next_transmission().is_some() {}
    while device.next_event().is_some() {}
}

/// A MAC data frame carrying an unsecured network frame, which is how a
/// coordinator talks to a device that does not hold the network key yet.
fn deliver(application: &[u8]) -> Vec<u8> {
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

fn application_frame(cluster: u16, payload: &[u8]) -> Vec<u8> {
    let mut frame = vec![0x00, APPLICATION.endpoint];
    frame.extend_from_slice(&cluster.to_le_bytes());
    frame.extend_from_slice(&APPLICATION.profile.to_le_bytes());
    frame.extend_from_slice(&[0x01, 0x07]);
    frame.extend_from_slice(payload);
    frame
}

fn next(seed: &mut u32) -> u8 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    (*seed >> 16) as u8
}

#[test]
fn nonsense_of_every_length_is_survivable() {
    let mut seed = 1;
    for len in 0..300usize {
        for _round in 0..30 {
            let frame: Vec<u8> = (0..len).map(|_| next(&mut seed)).collect();
            let mut device = device();
            device.receive(&frame, Instant::from_millis(0));
            device.tick(Instant::from_millis(1));
            drain(&mut device);
            assert!(!device.joined());
        }
    }
}

#[test]
fn a_well_formed_frame_with_a_mangled_tail_is_survivable() {
    let mut seed = 7;
    let template = deliver(&application_frame(CLUSTER_BASIC, &[0x00, 0x01, 0x00, 0x05, 0x00]));
    for cut in 0..template.len() {
        for _round in 0..30 {
            let mut frame = template[..cut].to_vec();
            frame.extend((0..template.len() - cut).map(|_| next(&mut seed)));
            let mut device = device();
            device.receive(&frame, Instant::from_millis(0));
            device.tick(Instant::from_millis(1));
            drain(&mut device);
        }
    }
}

/// A read of many attributes at once answers with a record per attribute, so a
/// single legal frame asks for a reply several times its own size.
#[test]
fn a_reply_larger_than_the_frame_it_answers_is_survivable() {
    for attributes in 1..50u16 {
        let mut read = vec![0x00, 0x01, 0x00];
        for id in 0..attributes {
            read.extend_from_slice(&id.to_le_bytes());
        }
        let frame = deliver(&application_frame(CLUSTER_BASIC, &read));
        if frame.len() > 127 {
            break;
        }
        let mut device = device();
        device.receive(&frame, Instant::from_millis(0));
        device.tick(Instant::from_millis(1));
        drain(&mut device);
    }
}
