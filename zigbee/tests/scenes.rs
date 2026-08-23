//! A scene remembers what the light looked like and puts it back, so what is
//! tested here is whether the state that comes back is the state that went in.

mod common;

use common::{at, command, device, events, group_command, joined_device, transmissions};
use zigbee::{Colour, Device, Event, CLUSTER_GROUPS, CLUSTER_SCENES};

const ADD: u8 = 0x00;
const REMOVE: u8 = 0x02;
const REMOVE_ALL: u8 = 0x03;
const STORE: u8 = 0x04;
const RECALL: u8 = 0x05;

const LOOSE: u16 = 0x0000;

fn scene_command(seq: u8, id: u8, group: u16, scene: u8, tail: &[u8]) -> Vec<u8> {
    let mut arguments = group.to_le_bytes().to_vec();
    arguments.push(scene);
    arguments.extend_from_slice(tail);
    command(CLUSTER_SCENES, seq, id, &arguments)
}

fn store(device: &mut Device, group: u16, scene: u8, now: u32) {
    device.receive(&scene_command(0x90, STORE, group, scene, &[]), at(now));
}

fn recall(device: &mut Device, group: u16, scene: u8, now: u32) {
    device.receive(&scene_command(0x91, RECALL, group, scene, &[]), at(now));
}

/// An Add carries the light's whole setting with it, cluster by cluster, rather
/// than telling the light to look at itself the way a Store does.
fn added_scene(hue: u8, saturation: u8, level: u8, on: bool, mireds: u16) -> Vec<u8> {
    let mut tail = vec![0x00, 0x00, 0x00];
    tail.extend_from_slice(&[0x06, 0x00, 0x01, on as u8]);
    tail.extend_from_slice(&[0x08, 0x00, 0x01, level]);
    tail.extend_from_slice(&[0x00, 0x03, 0x0d, 0, 0, 0, 0]);
    tail.extend_from_slice(&((hue as u16) << 8).to_le_bytes());
    tail.push(saturation);
    tail.extend_from_slice(&[0, 0, 0, 0]);
    tail.extend_from_slice(&mireds.to_le_bytes());
    scene_command(0x92, ADD, LOOSE, 1, &tail)
}

fn join_group(device: &mut Device, group: u16) {
    let mut arguments = group.to_le_bytes().to_vec();
    arguments.push(0);
    device.receive(&command(CLUSTER_GROUPS, 0x93, 0x00, &arguments), at(0));
}

#[test]
fn storing_a_scene_and_recalling_it_puts_the_light_back() {
    let mut device = device();
    device.set_on_off(true);
    device.set_level(120);
    store(&mut device, LOOSE, 1, 10);

    device.set_level(20);
    device.set_on_off(false);

    recall(&mut device, LOOSE, 1, 20);

    assert!(device.on_off());
    assert_eq!(device.level(), 120);
}

#[test]
fn a_recall_puts_the_colour_back_too() {
    let mut device = device();
    device.receive(
        &command(zigbee::CLUSTER_COLOUR_CONTROL, 0x94, 0x06, &[90, 200, 0, 0]),
        at(0),
    );
    store(&mut device, LOOSE, 2, 10);

    device.receive(
        &command(zigbee::CLUSTER_COLOUR_CONTROL, 0x95, 0x0a, &[250, 0, 0, 0]),
        at(20),
    );
    assert_eq!(device.colour(), Colour::Temperature { mireds: 250 });

    recall(&mut device, LOOSE, 2, 30);

    assert_eq!(
        device.colour(),
        Colour::HueSaturation {
            hue: 90,
            saturation: 200
        }
    );
}

#[test]
fn an_added_scene_carries_its_own_state_rather_than_the_light_s() {
    let mut device = device();
    device.set_level(10);
    device.receive(&added_scene(60, 180, 200, true, 0), at(10));

    recall(&mut device, LOOSE, 1, 20);

    assert!(device.on_off());
    assert_eq!(device.level(), 200);
    assert_eq!(
        device.colour(),
        Colour::HueSaturation {
            hue: 60,
            saturation: 180
        }
    );
}

#[test]
fn an_added_scene_with_a_temperature_comes_back_as_a_white() {
    let mut device = device();
    device.receive(&added_scene(0, 0, 150, true, 400), at(10));

    recall(&mut device, LOOSE, 1, 20);

    assert_eq!(device.colour(), Colour::Temperature { mireds: 400 });
}

#[test]
fn recalling_a_scene_nobody_stored_changes_nothing() {
    let mut device = device();
    device.set_level(77);

    recall(&mut device, LOOSE, 9, 10);

    assert_eq!(device.level(), 77);
    assert!(!device.on_off());
}

#[test]
fn a_removed_scene_is_gone() {
    let mut device = device();
    device.set_level(120);
    store(&mut device, LOOSE, 1, 10);
    device.receive(&scene_command(0x96, REMOVE, LOOSE, 1, &[]), at(20));

    device.set_level(30);
    recall(&mut device, LOOSE, 1, 30);

    assert_eq!(device.level(), 30);
}

#[test]
fn removing_every_scene_of_a_group_takes_them_all() {
    let mut device = device();
    device.set_level(120);
    store(&mut device, LOOSE, 1, 10);
    store(&mut device, LOOSE, 2, 11);

    let arguments = LOOSE.to_le_bytes();
    device.receive(&command(CLUSTER_SCENES, 0x97, REMOVE_ALL, &arguments), at(20));

    device.set_level(30);
    recall(&mut device, LOOSE, 1, 30);
    recall(&mut device, LOOSE, 2, 31);

    assert_eq!(device.level(), 30);
}

#[test]
fn a_scene_in_a_group_needs_the_light_to_be_in_that_group() {
    let mut device = device();
    device.set_level(120);

    store(&mut device, 7, 1, 10);
    device.set_level(30);
    recall(&mut device, 7, 1, 20);
    assert_eq!(device.level(), 30, "the light was never in group 7");

    join_group(&mut device, 7);
    device.set_level(120);
    store(&mut device, 7, 1, 30);
    device.set_level(30);
    recall(&mut device, 7, 1, 40);
    assert_eq!(device.level(), 120);
}

#[test]
fn leaving_a_group_takes_its_scenes_with_it() {
    let mut device = device();
    join_group(&mut device, 7);
    device.set_level(120);
    store(&mut device, 7, 1, 10);

    device.receive(&command(CLUSTER_GROUPS, 0x98, 0x03, &[7, 0]), at(20));
    join_group(&mut device, 7);

    device.set_level(30);
    recall(&mut device, 7, 1, 30);

    assert_eq!(device.level(), 30, "the scene left with the group");
}

#[test]
fn a_recall_announces_everything_it_moved() {
    let mut device = device();
    device.set_on_off(true);
    device.set_level(200);
    store(&mut device, LOOSE, 1, 10);
    device.set_on_off(false);
    device.set_level(20);
    events(&mut device);

    recall(&mut device, LOOSE, 1, 20);

    let announced = events(&mut device);
    assert!(announced
        .iter()
        .any(|event| matches!(event, Event::OnOffChanged(true))));
    assert!(announced
        .iter()
        .any(|event| matches!(event, Event::LevelChanged(200))));
}

#[test]
fn a_scene_recalled_at_a_group_reaches_every_member() {
    let mut device = joined_device();
    join_group(&mut device, 7);
    device.set_on_off(true);
    device.set_level(180);
    store(&mut device, 7, 3, 10);
    device.set_on_off(false);
    device.set_level(10);
    transmissions(&mut device);

    device.receive(&group_command(7, CLUSTER_SCENES, 0x99, RECALL, &[7, 0, 3]), at(20));

    assert!(device.on_off());
    assert_eq!(device.level(), 180);
    assert!(
        transmissions(&mut device).is_empty(),
        "a group recall is not answered either"
    );
}

#[test]
fn the_table_holds_eight_scenes_and_refuses_the_ninth() {
    let mut device = device();
    device.set_level(120);
    for scene in 1..=9u8 {
        store(&mut device, LOOSE, scene, 10 + scene as u32);
    }

    device.set_level(30);
    recall(&mut device, LOOSE, 9, 40);
    assert_eq!(device.level(), 30, "the ninth scene never fitted");

    recall(&mut device, LOOSE, 8, 50);
    assert_eq!(device.level(), 120, "the first eight did");
}
