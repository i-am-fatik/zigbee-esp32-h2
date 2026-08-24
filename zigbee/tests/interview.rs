//! What a coordinator asks the moment it meets the device. An interview that
//! goes unanswered leaves the light described by whatever was cached before it.

mod common;

use common::{at, deliver, joined_device, transmissions};

fn zdo_request(cluster: u16, payload: &[u8]) -> Vec<u8> {
    let mut frame = vec![0x00, 0x00];
    frame.extend_from_slice(&cluster.to_le_bytes());
    frame.extend_from_slice(&0x0000u16.to_le_bytes());
    frame.extend_from_slice(&[0x00, 0x07]);
    frame.extend_from_slice(payload);
    deliver(&frame)
}

#[test]
fn the_first_question_of_an_interview_is_answered() {
    let mut device = joined_device();
    device.receive(&zdo_request(0x0005, &[0x42, 0x60, 0x45]), at(100));

    assert_eq!(
        transmissions(&mut device).len(),
        1,
        "silence here fails the whole interview"
    );
}

#[test]
fn the_descriptor_that_names_the_light_is_answered_too() {
    let mut device = joined_device();
    device.receive(&zdo_request(0x0004, &[0x43, 0x60, 0x45, 0x01]), at(100));

    assert_eq!(transmissions(&mut device).len(), 1);
}
