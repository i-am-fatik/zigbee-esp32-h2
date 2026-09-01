mod common;

use common::{at, command, deliver, drain, joined_device, transmissions};
use zigbee::{Device, APPLICATION, CLUSTER_LEVEL_CONTROL};

const STEP: u8 = 0x02;
const UP: u8 = 0x00;

const ACK_REQUEST: u8 = 0x40;

fn light_at(level: u8) -> Device {
    let mut device = joined_device();
    device.set_level(level);
    drain(&mut device);
    device
}

fn step_up(by: u8) -> Vec<u8> {
    command(CLUSTER_LEVEL_CONTROL, 0x62, STEP, &[UP, by, 0x00, 0x00])
}

fn step_up_asking_for_an_ack(by: u8, counter: u8) -> Vec<u8> {
    let mut frame = vec![ACK_REQUEST, APPLICATION.endpoint];
    frame.extend_from_slice(&CLUSTER_LEVEL_CONTROL.to_le_bytes());
    frame.extend_from_slice(&APPLICATION.profile.to_le_bytes());
    frame.extend_from_slice(&[0x01, counter]);
    frame.extend_from_slice(&[0x01, 0x62, STEP, UP, by, 0x00, 0x00]);
    deliver(&frame)
}

#[test]
fn the_same_frame_twice_moves_the_brightness_once() {
    let mut device = light_at(100);
    let step = step_up(30);

    device.receive(&step, at(0));
    assert_eq!(device.level(), 130);

    device.receive(&step, at(50));
    assert_eq!(device.level(), 130, "the retransmission was acted on again");
}

#[test]
fn two_different_frames_both_move_the_brightness() {
    let mut device = light_at(100);

    device.receive(&step_up(10), at(0));
    device.receive(&step_up(10), at(50));

    assert_eq!(device.level(), 120);
}

#[test]
fn a_repeat_is_acknowledged_again_because_the_first_answer_was_lost() {
    let mut device = light_at(100);
    let step = step_up_asking_for_an_ack(30, 0x11);

    device.receive(&step, at(0));
    drain(&mut device);

    device.receive(&step, at(50));

    assert_eq!(device.level(), 130);
    assert!(
        !transmissions(&mut device).is_empty(),
        "a sender still waiting for an acknowledgement never gets one"
    );
}
