//! A trust centre may hand out a new network key at any time, and for a while
//! afterwards both keys are live. A device that gets this wrong goes deaf.

mod common;

use common::{at, events, joined_device, network_command, transport_network_key, OUR_IEEE};
use zigbee::{Credentials, Device, Event};

const NEXT_KEY: [u8; 16] = [0xa5; 16];
const NEXT_SEQUENCE: u8 = 1;

const SWITCH_KEY: u8 = 0x05;

fn rotating() -> Device {
    let mut device = joined_device();
    device.receive(
        &transport_network_key(&NEXT_KEY, NEXT_SEQUENCE, OUR_IEEE),
        at(1_000),
    );
    device
}

fn saved(device: &mut Device) -> Option<Credentials> {
    events(device)
        .into_iter()
        .rev()
        .find_map(|event| match event {
            Event::CredentialsChanged(saved) => Some(saved),
            _ => None,
        })
}

#[test]
fn a_new_key_arriving_is_held_rather_than_used() {
    let mut device = rotating();

    assert!(device.joined(), "a rotation is not a join");
    assert!(
        saved(&mut device).is_none(),
        "nothing changed yet, so nothing is worth writing down"
    );
}

#[test]
fn a_rotation_does_not_announce_the_device_all_over_again() {
    let mut device = joined_device();
    events(&mut device);
    while device.next_transmission().is_some() {}

    device.receive(
        &transport_network_key(&NEXT_KEY, NEXT_SEQUENCE, OUR_IEEE),
        at(1_000),
    );

    assert!(
        device.next_transmission().is_none(),
        "a device already on the network has nothing to announce"
    );
    assert!(events(&mut device)
        .iter()
        .all(|event| !matches!(event, Event::Joined { .. })));
}

#[test]
fn the_switch_command_makes_the_held_key_the_one_in_use() {
    let mut device = rotating();
    events(&mut device);

    device.receive(&network_command(&[SWITCH_KEY, NEXT_SEQUENCE]), at(2_000));

    let saved = saved(&mut device).expect("moving to a new key is worth writing down");
    let restored = Credentials::from_bytes(&saved.to_bytes()).expect("round trip");
    assert_eq!(restored.channel(), device.radio().channel);
    assert!(device.joined());
}

#[test]
fn a_switch_to_a_sequence_nobody_sent_is_ignored() {
    let mut device = rotating();
    events(&mut device);

    device.receive(&network_command(&[SWITCH_KEY, 0x7f]), at(2_000));

    assert!(
        saved(&mut device).is_none(),
        "the device must not move to a key it was never given"
    );
}

#[test]
fn a_switch_with_no_key_held_is_ignored() {
    let mut device = joined_device();
    events(&mut device);

    device.receive(&network_command(&[SWITCH_KEY, NEXT_SEQUENCE]), at(2_000));

    assert!(saved(&mut device).is_none());
    assert!(device.joined());
}

#[test]
fn the_device_stays_on_the_network_across_the_whole_rotation() {
    let mut device = rotating();
    device.receive(&network_command(&[SWITCH_KEY, NEXT_SEQUENCE]), at(2_000));
    device.tick(at(2_100));

    assert!(device.joined(), "a rotation must never cost a rejoin");
    assert_ne!(device.radio().pan_id, 0xffff);
}
