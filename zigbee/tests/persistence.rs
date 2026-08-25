//! What a light has to write down to come back from a restart as the same
//! member of the same groups, holding the same scenes.

mod common;

use common::{
    application_frame, at, command, deliver, device, events, group_command, join_group,
    recall_scene, store_scene,
};
use zigbee::{
    Colour, Device, Event, Tables, CLUSTER_COLOUR_CONTROL, CLUSTER_GROUPS, CLUSTER_ON_OFF,
};

const GROUP_REMOVE_ALL: u8 = 0x04;
const ON: u8 = 0x01;

const WRITE_ATTRIBUTES: u8 = 0x02;
const TYPE_ENUM8: u8 = 0x30;
const TYPE_UINT16: u8 = 0x21;

fn write_attribute(cluster: u16, attribute: u16, value: &[u8]) -> Vec<u8> {
    let mut request = vec![0x00, 0x51, WRITE_ATTRIBUTES];
    request.extend_from_slice(&attribute.to_le_bytes());
    request.extend_from_slice(value);
    deliver(&application_frame(cluster, &request))
}

fn last_tables(device: &mut Device) -> Option<Tables> {
    events(device)
        .into_iter()
        .rev()
        .find_map(|event| match event {
            Event::TablesChanged(tables) => Some(tables),
            _ => None,
        })
}

/// A restart: nothing carries over but the bytes that were written down.
fn restarted(tables: Tables) -> Device {
    let mut device = device();
    let record = tables.to_bytes();
    device.restore_tables(Tables::from_bytes(&record).expect("its own record"));
    device
}

#[test]
fn joining_a_group_offers_tables_to_write_down() {
    let mut device = device();
    join_group(&mut device, 7, 0);

    assert!(last_tables(&mut device).is_some());
}

#[test]
fn a_group_survives_a_restart() {
    let mut device = device();
    join_group(&mut device, 7, 0);
    let saved = last_tables(&mut device).expect("joining a group is worth saving");

    let mut device = restarted(saved);
    device.receive(&group_command(7, CLUSTER_ON_OFF, 0x93, ON, &[]), at(10));

    assert!(device.on_off());
}

#[test]
fn a_scene_survives_a_restart_with_everything_it_held() {
    let mut device = device();
    device.set_on_off(true);
    device.set_level(180);
    device.receive(
        &command(zigbee::CLUSTER_COLOUR_CONTROL, 0x94, 0x06, &[90, 200, 0, 0]),
        at(5),
    );
    store_scene(&mut device, 0, 3, 10);
    let saved = last_tables(&mut device).expect("storing a scene is worth saving");

    let mut device = restarted(saved);
    recall_scene(&mut device, 0, 3, 20);

    assert!(device.on_off());
    assert_eq!(device.level(), 180);
    assert_eq!(
        device.colour(),
        zigbee::Colour::HueSaturation {
            hue: 90,
            saturation: 200
        }
    );
}

#[test]
fn a_full_table_survives_every_slot() {
    let mut device = device();
    for group in 1..=4u16 {
        join_group(&mut device, group, group as u32);
    }
    for scene in 1..=8u8 {
        device.set_level(scene * 20);
        store_scene(&mut device, 0, scene, 20 + scene as u32);
    }
    let saved = last_tables(&mut device).expect("a full table is worth saving");

    let mut device = restarted(saved);
    for group in 1..=4u16 {
        device.set_on_off(false);
        device.receive(&group_command(group, CLUSTER_ON_OFF, 0x95, ON, &[]), at(40));
        assert!(device.on_off(), "group {group} was lost");
    }
    for scene in 1..=8u8 {
        recall_scene(&mut device, 0, scene, 50 + scene as u32);
        assert_eq!(device.level(), scene * 20, "scene {scene} was lost");
    }
}

#[test]
fn leaving_every_group_is_written_down_too() {
    let mut device = device();
    join_group(&mut device, 7, 0);
    device.receive(
        &command(CLUSTER_GROUPS, 0x96, GROUP_REMOVE_ALL, &[]),
        at(10),
    );
    let saved = last_tables(&mut device).expect("forgetting is worth saving as well");

    let mut device = restarted(saved);
    device.receive(&group_command(7, CLUSTER_ON_OFF, 0x97, ON, &[]), at(20));

    assert!(!device.on_off(), "a restart must not resurrect the group");
}

#[test]
fn recalling_a_scene_is_not_worth_writing_down() {
    let mut device = device();
    device.set_level(180);
    store_scene(&mut device, 0, 1, 10);
    events(&mut device);

    recall_scene(&mut device, 0, 1, 20);

    assert!(
        last_tables(&mut device).is_none(),
        "a recall reads the table, it does not change it"
    );
}

#[test]
fn blank_flash_reads_back_as_nothing() {
    assert!(Tables::from_bytes(&[0u8; Tables::LEN]).is_none());
    assert!(Tables::from_bytes(&[0xffu8; Tables::LEN]).is_none());
}

#[test]
fn a_record_written_by_something_else_is_refused() {
    let mut device = device();
    join_group(&mut device, 7, 0);
    let saved = last_tables(&mut device).expect("saved");

    let mut record = saved.to_bytes();
    record[0] ^= 0xff;

    assert!(Tables::from_bytes(&record).is_none());
}

/// A NOR flash writes whole words, so a record that ends part way through one
/// is refused by the driver rather than by anything a host test would notice.
#[test]
fn both_records_are_a_whole_number_of_flash_words() {
    assert_eq!(Tables::LEN % 4, 0, "Tables::LEN is {}", Tables::LEN);
    assert_eq!(
        zigbee::Credentials::LEN % 4,
        0,
        "Credentials::LEN is {}",
        zigbee::Credentials::LEN
    );
}

#[test]
fn a_light_told_to_come_up_on_does_so_after_a_restart() {
    let mut device = device();
    device.receive(
        &write_attribute(CLUSTER_ON_OFF, 0x4003, &[TYPE_ENUM8, ON]),
        at(10),
    );
    let saved = last_tables(&mut device).expect("a startup setting is worth saving");

    let device = restarted(saved);

    assert!(device.on_off(), "the coordinator asked it to boot lit");
}

#[test]
fn a_light_told_to_come_up_at_a_temperature_does_so_after_a_restart() {
    let mut device = device();
    let mireds = 250u16;
    let mut value = vec![TYPE_UINT16];
    value.extend_from_slice(&mireds.to_le_bytes());
    device.receive(
        &write_attribute(CLUSTER_COLOUR_CONTROL, 0x4010, &value),
        at(10),
    );
    let saved = last_tables(&mut device).expect("a startup setting is worth saving");

    let device = restarted(saved);

    assert_eq!(device.colour(), Colour::Temperature { mireds });
}

#[test]
fn a_light_nobody_told_anything_comes_up_the_way_it_always_did() {
    let mut device = device();
    join_group(&mut device, 7, 0);
    let saved = last_tables(&mut device).expect("joining a group is worth saving");

    let device = restarted(saved);

    assert!(!device.on_off());
    assert_eq!(device.colour(), Colour::Temperature { mireds: 370 });
}
