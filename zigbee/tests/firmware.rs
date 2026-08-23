//! Runs a whole over-the-air update past the stack, from the server's offer to
//! the last block, and checks that what comes out is the firmware rather than
//! the file it was wrapped in.

mod common;

use common::{at, deliver, drain, events, join, transmissions};
use zigbee::{Config, Device, Event, APPLICATION, CLUSTER_OTA};

const MANUFACTURER: u16 = 0x1037;
const IMAGE_TYPE: u16 = 0x0001;
const RUNNING: u32 = 0x0000_0001;
const OFFERED: u32 = 0x0000_0002;

const QUERY_NEXT_IMAGE_RESPONSE: u8 = 0x02;
const IMAGE_BLOCK_RESPONSE: u8 = 0x05;
const UPGRADE_END_RESPONSE: u8 = 0x07;
const IMAGE_NOTIFY: u8 = 0x00;

const BLOCK: usize = 48;

/// A device on a network that has been told which firmware it is running, which
/// is the only kind that asks a server for anything.
fn joined_updatable() -> Device {
    let mut device =
        Device::new(Config::new(common::OUR_IEEE).with_firmware(MANUFACTURER, IMAGE_TYPE, RUNNING));
    join(&mut device);
    device
}

/// A minimal upgrade file: the 56 octet header, one sub-element tag, then the
/// firmware itself.
fn upgrade_file(firmware: &[u8]) -> Vec<u8> {
    let mut file = Vec::new();
    file.extend_from_slice(&0x0bee_f11eu32.to_le_bytes());
    file.extend_from_slice(&0x0100u16.to_le_bytes());
    file.extend_from_slice(&56u16.to_le_bytes());
    file.extend_from_slice(&0x0000u16.to_le_bytes());
    file.extend_from_slice(&MANUFACTURER.to_le_bytes());
    file.extend_from_slice(&IMAGE_TYPE.to_le_bytes());
    file.extend_from_slice(&OFFERED.to_le_bytes());
    file.extend_from_slice(&0x0002u16.to_le_bytes());
    file.extend_from_slice(&[0u8; 32]);
    file.extend_from_slice(&((56 + 6 + firmware.len()) as u32).to_le_bytes());
    assert_eq!(file.len(), 56);

    file.extend_from_slice(&0x0000u16.to_le_bytes());
    file.extend_from_slice(&(firmware.len() as u32).to_le_bytes());
    file.extend_from_slice(firmware);
    file
}

fn server_says(id: u8, arguments: &[u8]) -> Vec<u8> {
    let mut request = vec![0x19, 0x40, id];
    request.extend_from_slice(arguments);

    let mut application = vec![0x00, APPLICATION.endpoint];
    application.extend_from_slice(&CLUSTER_OTA.to_le_bytes());
    application.extend_from_slice(&APPLICATION.profile.to_le_bytes());
    application.extend_from_slice(&[0x01, 0x07]);
    application.extend_from_slice(&request);
    deliver(&application)
}

fn offer(size: u32) -> Vec<u8> {
    let mut arguments = vec![0x00];
    arguments.extend_from_slice(&MANUFACTURER.to_le_bytes());
    arguments.extend_from_slice(&IMAGE_TYPE.to_le_bytes());
    arguments.extend_from_slice(&OFFERED.to_le_bytes());
    arguments.extend_from_slice(&size.to_le_bytes());
    server_says(QUERY_NEXT_IMAGE_RESPONSE, &arguments)
}

fn block_at(file: &[u8], offset: u32) -> Vec<u8> {
    let from = offset as usize;
    let to = (from + BLOCK).min(file.len());
    let mut arguments = vec![0x00];
    arguments.extend_from_slice(&MANUFACTURER.to_le_bytes());
    arguments.extend_from_slice(&IMAGE_TYPE.to_le_bytes());
    arguments.extend_from_slice(&OFFERED.to_le_bytes());
    arguments.extend_from_slice(&offset.to_le_bytes());
    arguments.push((to - from) as u8);
    arguments.extend_from_slice(&file[from..to]);
    server_says(IMAGE_BLOCK_RESPONSE, &arguments)
}

fn ended() -> Vec<u8> {
    let mut arguments = MANUFACTURER.to_le_bytes().to_vec();
    arguments.extend_from_slice(&IMAGE_TYPE.to_le_bytes());
    arguments.extend_from_slice(&OFFERED.to_le_bytes());
    arguments.extend_from_slice(&0u32.to_le_bytes());
    arguments.extend_from_slice(&0u32.to_le_bytes());
    server_says(UPGRADE_END_RESPONSE, &arguments)
}

#[test]
fn a_light_that_has_not_said_what_it_runs_never_asks_for_an_image() {
    let mut device = common::joined_device();
    for tick in 0..5 {
        device.tick(at(1_000 + tick * 100));
    }

    assert!(transmissions(&mut device).is_empty());
}

#[test]
fn a_light_that_has_asks() {
    let mut device = joined_updatable();
    device.tick(at(1_000));

    assert!(
        !transmissions(&mut device).is_empty(),
        "a device that knows its own version queries once it is on a network"
    );
}

#[test]
fn a_whole_image_arrives_stripped_of_the_file_it_came_in() {
    let firmware: Vec<u8> = (0..300u32).map(|byte| byte as u8).collect();
    let file = upgrade_file(&firmware);

    let mut device = joined_updatable();
    device.tick(at(1_000));
    drain(&mut device);
    device.receive(&offer(file.len() as u32), at(1_100));

    let mut written = vec![0u8; firmware.len()];
    let mut offset = 0u32;
    let mut clock = 1_200;
    let mut ready = false;

    for _ in 0..200 {
        device.tick(at(clock));
        while let Some(block) = device.next_firmware_block() {
            let at = block.offset as usize;
            written[at..at + block.data.len()].copy_from_slice(block.data);
        }
        while let Some(event) = device.next_event() {
            if matches!(event, Event::FirmwareReady) {
                ready = true;
            }
        }
        while device.next_transmission().is_some() {}

        clock += 100;
        if offset < file.len() as u32 {
            device.receive(&block_at(&file, offset), at(clock));
            offset = (offset + BLOCK as u32).min(file.len() as u32);
        } else if !ready {
            device.receive(&ended(), at(clock));
        } else {
            break;
        }
        clock += 100;
    }

    assert!(ready, "the update never finished");
    assert_eq!(written, firmware, "the image is not what the file carried");
}

#[test]
fn an_offer_is_announced_with_what_it_holds() {
    let file = upgrade_file(&[0xaa; 64]);
    let mut device = joined_updatable();
    device.tick(at(1_000));
    drain(&mut device);

    device.receive(&offer(file.len() as u32), at(1_100));

    assert!(events(&mut device).iter().any(|event| matches!(
        event,
        Event::FirmwareOffered { version, size }
            if *version == OFFERED && *size == file.len() as u32
    )));
}

#[test]
fn a_server_with_nothing_to_offer_leaves_the_light_alone() {
    let mut device = joined_updatable();
    device.tick(at(1_000));
    drain(&mut device);

    device.receive(&server_says(QUERY_NEXT_IMAGE_RESPONSE, &[0x98]), at(1_100));

    assert!(device.next_firmware_block().is_none());
    assert!(events(&mut device).is_empty());
}

#[test]
fn giving_up_tells_the_caller_and_stops_the_download() {
    let file = upgrade_file(&[0xaa; 200]);
    let mut device = joined_updatable();
    device.tick(at(1_000));
    drain(&mut device);
    device.receive(&offer(file.len() as u32), at(1_100));
    device.receive(&block_at(&file, 0), at(1_200));
    events(&mut device);

    device.abandon_firmware();

    assert!(events(&mut device)
        .iter()
        .any(|event| matches!(event, Event::FirmwareAbandoned)));

    device.receive(&block_at(&file, 48), at(1_300));
    assert!(
        device.next_firmware_block().is_none(),
        "a block arriving after the abort is not written down"
    );
}

#[test]
fn a_block_out_of_order_is_dropped_rather_than_written_at_the_wrong_place() {
    let file = upgrade_file(&[0x11; 200]);
    let mut device = joined_updatable();
    device.tick(at(1_000));
    drain(&mut device);
    device.receive(&offer(file.len() as u32), at(1_100));

    device.receive(&block_at(&file, 96), at(1_200));

    assert!(device.next_firmware_block().is_none());
}

#[test]
fn a_notify_from_a_server_restarts_the_asking() {
    let mut device = joined_updatable();
    device.tick(at(1_000));
    drain(&mut device);
    device.receive(&server_says(QUERY_NEXT_IMAGE_RESPONSE, &[0x98]), at(1_100));
    drain(&mut device);

    device.receive(&server_says(IMAGE_NOTIFY, &[0x00, 100]), at(2_000));
    device.tick(at(2_100));

    assert!(
        !transmissions(&mut device).is_empty(),
        "a notify is how a server wakes a light that gave up"
    );
}

#[test]
fn a_file_that_is_not_an_upgrade_file_yields_nothing() {
    let mut device = joined_updatable();
    device.tick(at(1_000));
    drain(&mut device);
    device.receive(&offer(200), at(1_100));

    let rubbish: Vec<u8> = (0..200u32).map(|byte| byte as u8).collect();
    let mut offset = 0u32;
    for round in 0..4 {
        device.receive(&block_at(&rubbish, offset), at(1_200 + round * 100));
        offset += BLOCK as u32;
    }

    assert!(device.next_firmware_block().is_none());
}

#[test]
fn the_upgrade_cluster_is_advertised_the_other_way_round() {
    assert_eq!(APPLICATION.outputs, &[CLUSTER_OTA]);
    assert!(
        !APPLICATION.clusters.contains(&CLUSTER_OTA),
        "the light is the client, not the server"
    );
}
