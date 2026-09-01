mod common;

use common::*;
use zigbee::Event;

const FIXTURE_NETWORK_KEY: [u8; 16] = [
    0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
];

#[test]
fn the_join_fixture_carries_the_key_every_test_asserts_against() {
    let mut device = device();
    device.tick(at(0));
    device.receive(&beacon(true), at(10));
    device.tick(at(260));
    device.receive(ASSOCIATION_RESPONSE, at(270));
    device.receive(&deliver(TRANSPORT_KEY), at(280));

    let credentials = events(&mut device)
        .into_iter()
        .find_map(|event| match event {
            Event::CredentialsChanged(credentials) => Some(credentials),
            _ => None,
        })
        .expect("joining emits credentials");

    let stored = credentials.to_bytes();
    assert!(
        stored
            .windows(FIXTURE_NETWORK_KEY.len())
            .any(|window| window == FIXTURE_NETWORK_KEY),
        "every join assertion downstream reads the key this fixture carries"
    );
}
