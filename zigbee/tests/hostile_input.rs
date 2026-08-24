//! Feeds the stack the frames a hostile or broken neighbour can put on the air
//! and asserts it stays alive. Everything here goes in through `receive`, the
//! one door the radio has.

mod common;

use common::{application_frame, at, deliver, device, drain, joined_device, transmissions};
use zigbee::{CLUSTER_BASIC, MAX_FRAME_LEN};

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
            device.receive(&frame, at(0));
            device.tick(at(1));
            drain(&mut device);
            assert!(!device.joined());
        }
    }
}

#[test]
fn a_well_formed_frame_with_a_mangled_tail_is_survivable() {
    let mut seed = 7;
    let template = deliver(&application_frame(
        CLUSTER_BASIC,
        &[0x00, 0x01, 0x00, 0x05, 0x00],
    ));
    for cut in 0..template.len() {
        for _round in 0..30 {
            let mut frame = template[..cut].to_vec();
            frame.extend((0..template.len() - cut).map(|_| next(&mut seed)));
            let mut device = device();
            device.receive(&frame, at(0));
            device.tick(at(1));
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
        device.receive(&frame, at(0));
        device.tick(at(1));
        drain(&mut device);
    }
}

#[test]
fn a_reply_too_big_for_one_frame_is_cut_down_rather_than_dropped() {
    let mut read = vec![0x00, 0x01, 0x00];
    for attribute in 0..40u16 {
        read.extend_from_slice(&attribute.to_le_bytes());
    }
    let request = deliver(&application_frame(CLUSTER_BASIC, &read));
    assert!(
        request.len() <= MAX_FRAME_LEN,
        "the ask has to be legal too"
    );

    let mut device = joined_device();
    device.receive(&request, at(100));

    let replies = transmissions(&mut device);
    assert_eq!(replies.len(), 1, "silence reads as a dead device");
    assert_eq!(
        replies[0].len(),
        MAX_FRAME_LEN,
        "the reply fills the frame it is allowed and stops"
    );
}
