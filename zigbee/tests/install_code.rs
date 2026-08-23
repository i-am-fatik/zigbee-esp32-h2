//! The join is the one moment a Zigbee network hands out its key, and without
//! an install code it hands it out under a key printed in the specification.

mod common;

use common::{at, drain, events, ASSOCIATION_RESPONSE, OUR_IEEE, TRANSPORT_KEY};
use zigbee::{install_code_label, Config, Device, Event};

/// The worked example from the specification, used here as a code this device
/// could have been given rather than as a value under test.
const CODE: [u8; 16] = [
    0x83, 0xfe, 0xd3, 0x40, 0x7a, 0x93, 0x97, 0x23, 0xa5, 0xc6, 0x39, 0xb2, 0x69, 0x16, 0xd5, 0x05,
];

fn joining(config: Config) -> Device {
    let mut device = Device::new(config);
    device.tick(at(0));
    device.receive(&common::beacon(true), at(10));
    device.receive(ASSOCIATION_RESPONSE, at(20));
    drain(&mut device);
    device
}

#[test]
fn without_a_code_the_join_is_readable_by_anyone() {
    let mut device = joining(Config::new(OUR_IEEE));

    device.receive(&common::deliver(TRANSPORT_KEY), at(30));

    assert!(
        device.joined(),
        "the well known link key opens the transport key"
    );
}

#[test]
fn with_a_code_the_same_join_no_longer_opens() {
    let mut device = joining(Config::new(OUR_IEEE).with_install_code(CODE));

    device.receive(&common::deliver(TRANSPORT_KEY), at(30));

    assert!(
        !device.joined(),
        "a key meant for the published link key must not decrypt under another"
    );
    assert!(events(&mut device)
        .iter()
        .all(|event| !matches!(event, Event::Joined { .. })));
}

#[test]
fn two_codes_are_two_different_locks() {
    let mut other = CODE;
    other[0] ^= 0xff;

    assert_ne!(install_code_label(&CODE), install_code_label(&other));
}

#[test]
fn the_label_carries_the_code_and_two_octets_more() {
    let label = install_code_label(&CODE);

    assert_eq!(label.len(), CODE.len() + 2);
    assert_eq!(&label[..CODE.len()], &CODE);
}

#[test]
fn the_debug_output_never_carries_the_install_code() {
    let with = format!("{:?}", Config::new(OUR_IEEE).with_install_code(CODE));
    let without = format!("{:?}", Config::new(OUR_IEEE));

    assert_eq!(
        with, without,
        "holding a secret changed what a log line would print"
    );
}
