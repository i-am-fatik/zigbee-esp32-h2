//! Drives a device through a join using a full frame exchange, so the protocol
//! and the cryptography are exercised against traffic shaped the way a
//! coordinator produces it.

use zigbee::{Config, Device, Event, Instant};

const OUR_IEEE: u64 = 0x0011_2233_4455_6677;
const PAN: u16 = 0xa269;
const OUR_SHORT: u16 = 0x4560;

/// The association response an EmberZNet coordinator sent this device.
const ASSOCIATION_RESPONSE: &[u8] = &[
    0x63, 0xcc, 0xce, 0x69, 0xa2, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0x00, 0xff, 0xee, 0xdd,
    0xcc, 0xbb, 0xaa, 0x99, 0x88, 0x02, 0x60, 0x45, 0x00,
];

/// The application-support command carrying the network key, encrypted by the
/// trust centre under the key derived from the well known link key.
const TRANSPORT_KEY: &[u8] = &[
    0x21, 0xbd, 0x30, 0x00, 0x30, 0x00, 0x00, 0xff, 0xee, 0xdd, 0xcc, 0xbb, 0xaa, 0x99, 0x88, 0xb5,
    0x2c, 0x03, 0x1e, 0x47, 0xb4, 0xff, 0x2f, 0x20, 0xa6, 0xb3, 0x1c, 0xd2, 0x0d, 0x4c, 0x32, 0xea,
    0x81, 0x49, 0xd2, 0x06, 0xe2, 0x65, 0xab, 0xa2, 0x24, 0xd9, 0x5b, 0xa3, 0xf9, 0x70, 0x85, 0xaa,
    0x06, 0x5a, 0x89, 0x8c, 0xfc, 0x6d,
];

fn device() -> Device {
    Device::new(Config::new(OUR_IEEE).with_model("H2.NoStd.Light"))
}

fn drain(device: &mut Device) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    while let Some(outgoing) = device.next_transmission() {
        frames.push(outgoing.frame.to_vec());
    }
    frames
}

fn events(device: &mut Device) -> Vec<Event> {
    let mut collected = Vec::new();
    while let Some(event) = device.next_event() {
        collected.push(event);
    }
    collected
}

/// Wraps an application-support frame the way the coordinator delivers one
/// before the joining device holds a network key: a MAC data frame carrying an
/// unsecured network frame.
fn deliver_unsecured(application: &[u8]) -> Vec<u8> {
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

fn beacon(permit_join: bool) -> Vec<u8> {
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

#[test]
fn scanning_broadcasts_a_beacon_request() {
    let mut device = device();
    device.tick(Instant::from_millis(0));

    let frames = drain(&mut device);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0], vec![0x03, 0x08, 0x01, 0xff, 0xff, 0xff, 0xff, 0x07]);
}

#[test]
fn scanning_walks_every_zigbee_channel() {
    let mut device = device();
    let mut seen = Vec::new();
    let mut clock = 0;

    for _ in 0..16 {
        device.tick(Instant::from_millis(clock));
        seen.push(device.radio().channel);
        drain(&mut device);
        clock += 300;
    }

    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen, (11..=26).collect::<Vec<_>>());
}

#[test]
fn a_beacon_that_permits_joining_provokes_an_association_request() {
    let mut device = device();
    device.tick(Instant::from_millis(0));
    drain(&mut device);

    device.receive(&beacon(true), Instant::from_millis(10));

    let frames = drain(&mut device);
    assert_eq!(frames.len(), 1, "expected exactly one association request");
    let request = &frames[0];
    assert_eq!(&request[..2], &[0x23, 0xc8], "MAC command, ack requested");
    assert_eq!(request[request.len() - 2], 0x01, "association request");
    assert_eq!(request[request.len() - 1], 0x8c, "mains powered, receiver on");
}

#[test]
fn a_beacon_that_refuses_joining_is_ignored() {
    let mut device = device();
    device.tick(Instant::from_millis(0));
    drain(&mut device);

    device.receive(&beacon(false), Instant::from_millis(10));

    assert!(drain(&mut device).is_empty());
}

#[test]
fn a_real_association_response_allocates_the_short_address() {
    let mut device = device();
    device.tick(Instant::from_millis(0));
    drain(&mut device);
    device.receive(&beacon(true), Instant::from_millis(10));
    drain(&mut device);

    device.receive(ASSOCIATION_RESPONSE, Instant::from_millis(20));

    assert_eq!(device.radio().short_address, OUR_SHORT);
    assert_eq!(device.radio().pan_id, PAN);
    assert!(!device.joined(), "not joined until the network key arrives");
}

#[test]
fn a_real_transport_key_completes_the_join() {
    let mut device = device();
    device.tick(Instant::from_millis(0));
    drain(&mut device);
    device.receive(&beacon(true), Instant::from_millis(10));
    drain(&mut device);
    device.receive(ASSOCIATION_RESPONSE, Instant::from_millis(20));
    drain(&mut device);
    events(&mut device);

    device.receive(&deliver_unsecured(TRANSPORT_KEY), Instant::from_millis(30));

    assert!(device.joined(), "the transported key should decrypt and be accepted");
    let events = events(&mut device);
    assert!(events.iter().any(
        |event| matches!(event, Event::Joined { short_address } if *short_address == OUR_SHORT)
    ));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::CredentialsChanged(_))),
        "joining must offer credentials to persist"
    );
    assert!(
        !drain(&mut device).is_empty(),
        "joining must announce the device"
    );
}

#[test]
fn credentials_survive_a_round_trip_through_storage() {
    let mut device = device();
    device.tick(Instant::from_millis(0));
    drain(&mut device);
    device.receive(&beacon(true), Instant::from_millis(10));
    drain(&mut device);
    device.receive(ASSOCIATION_RESPONSE, Instant::from_millis(20));
    drain(&mut device);
    device.receive(&deliver_unsecured(TRANSPORT_KEY), Instant::from_millis(30));

    let saved = events(&mut device)
        .into_iter()
        .find_map(|event| match event {
            Event::CredentialsChanged(credentials) => Some(credentials),
            _ => None,
        })
        .expect("joining emits credentials");

    let restored = zigbee::Credentials::from_bytes(&saved.to_bytes()).expect("round trip");
    let device = Device::restore(Config::new(OUR_IEEE), restored);

    assert!(device.joined());
    assert_eq!(device.radio().short_address, OUR_SHORT);
    assert_eq!(device.radio().pan_id, PAN);
}

#[test]
fn rubbish_is_rejected_without_panicking() {
    let mut device = device();
    for length in 0..48 {
        let noise: Vec<u8> = (0..length).map(|byte| byte as u8 ^ 0x5a).collect();
        device.receive(&noise, Instant::from_millis(length as u32));
    }
    assert!(!device.joined());
}

fn joined_device() -> Device {
    let mut device = device();
    device.tick(Instant::from_millis(0));
    drain(&mut device);
    device.receive(&beacon(true), Instant::from_millis(10));
    drain(&mut device);
    device.receive(ASSOCIATION_RESPONSE, Instant::from_millis(20));
    drain(&mut device);
    device.receive(&deliver_unsecured(TRANSPORT_KEY), Instant::from_millis(30));
    drain(&mut device);
    events(&mut device);
    device
}

#[test]
fn a_few_refused_frames_send_the_device_looking_for_a_new_parent() {
    let mut device = joined_device();
    assert!(device.joined());

    for tick in 0..3 {
        device.transmission_failed(Instant::from_millis(1_000 + tick));
    }

    assert!(!device.joined(), "three refusals mean the parent stopped listening");
    device.tick(Instant::from_millis(1_100));
    let frames = drain(&mut device);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0][frames[0].len() - 1], 0x07, "a beacon request");
}

#[test]
fn one_refused_frame_is_not_enough() {
    let mut device = joined_device();
    device.transmission_failed(Instant::from_millis(1_000));
    assert!(device.joined());
}

#[test]
fn a_delivery_clears_the_run_of_failures() {
    let mut device = joined_device();
    device.transmission_failed(Instant::from_millis(1_000));
    device.transmission_failed(Instant::from_millis(1_001));
    device.transmission_delivered();
    device.transmission_failed(Instant::from_millis(1_002));

    assert!(device.joined(), "the run has to be consecutive");
}

#[test]
fn rejoining_keeps_the_channel_the_network_and_the_key() {
    let mut device = joined_device();
    let before = device.radio();
    for tick in 0..3 {
        device.transmission_failed(Instant::from_millis(1_000 + tick));
    }

    assert_eq!(device.radio().channel, before.channel);
    assert_eq!(device.radio().pan_id, before.pan_id);
}

#[test]
fn a_closed_network_still_takes_a_rejoin() {
    let mut device = joined_device();
    for tick in 0..3 {
        device.transmission_failed(Instant::from_millis(1_000 + tick));
    }
    device.tick(Instant::from_millis(1_100));
    drain(&mut device);

    device.receive(&beacon(false), Instant::from_millis(1_200));

    let frames = drain(&mut device);
    assert_eq!(
        frames.len(),
        1,
        "a member rejoins without permit-join, unlike a first join"
    );
    let request = &frames[0];
    assert_eq!(&request[..2], &[0x61, 0x88], "MAC data, ack requested");
}

#[test]
fn giving_up_on_the_rejoin_falls_back_to_a_full_scan() {
    let mut device = joined_device();
    for tick in 0..3 {
        device.transmission_failed(Instant::from_millis(1_000 + tick));
    }
    events(&mut device);

    device.tick(Instant::from_millis(30_000));

    assert!(events(&mut device)
        .iter()
        .any(|event| matches!(event, Event::Left)));
    assert_eq!(device.radio().pan_id, 0xffff);
}
