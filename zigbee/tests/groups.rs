//! A group is how one command reaches many lights, so what is tested here is
//! whether the light acts on a frame that was never addressed to it.

mod common;

use common::{at, command, device, group_command, join_group, joined_device, transmissions};
use zigbee::{Device, CLUSTER_GROUPS, CLUSTER_ON_OFF};

const REMOVE: u8 = 0x03;
const REMOVE_ALL: u8 = 0x04;
const ADD_IF_IDENTIFYING: u8 = 0x05;

const ON: u8 = 0x01;

fn switch_group_on(device: &mut Device, group: u16) {
    device.receive(&group_command(group, CLUSTER_ON_OFF, 0x81, ON, &[]), at(10));
}

#[test]
fn a_light_in_no_group_ignores_a_group_command() {
    let mut device = device();
    switch_group_on(&mut device, 7);

    assert!(!device.on_off());
}

#[test]
fn a_light_in_the_group_acts_on_the_command() {
    let mut device = device();
    join_group(&mut device, 7, 0);

    switch_group_on(&mut device, 7);

    assert!(device.on_off());
}

#[test]
fn a_light_ignores_a_group_it_is_not_in() {
    let mut device = device();
    join_group(&mut device, 7, 0);

    switch_group_on(&mut device, 9);

    assert!(!device.on_off());
}

#[test]
fn a_group_command_is_never_answered() {
    let mut device = joined_device();
    join_group(&mut device, 7, 0);
    transmissions(&mut device);

    switch_group_on(&mut device, 7);

    assert!(device.on_off(), "it acted on the command");
    assert!(
        transmissions(&mut device).is_empty(),
        "every member answering at once is what a group must not do"
    );
}

#[test]
fn the_same_light_still_answers_a_command_addressed_to_it() {
    let mut device = joined_device();
    transmissions(&mut device);

    device.receive(&command(CLUSTER_ON_OFF, 0x82, ON, &[]), at(10));

    assert!(
        !transmissions(&mut device).is_empty(),
        "a unicast command is answered, which is what makes the group case meaningful"
    );
}

#[test]
fn leaving_a_group_stops_the_commands() {
    let mut device = device();
    join_group(&mut device, 7, 0);
    device.receive(&command(CLUSTER_GROUPS, 0x83, REMOVE, &[7, 0]), at(5));

    switch_group_on(&mut device, 7);

    assert!(!device.on_off());
}

#[test]
fn leaving_every_group_stops_them_all() {
    let mut device = device();
    join_group(&mut device, 7, 0);
    join_group(&mut device, 8, 0);
    device.receive(&command(CLUSTER_GROUPS, 0x84, REMOVE_ALL, &[]), at(5));

    switch_group_on(&mut device, 7);
    switch_group_on(&mut device, 8);

    assert!(!device.on_off());
}

#[test]
fn the_table_holds_four_groups_and_refuses_the_fifth() {
    let mut device = device();
    for group in 1..=5u16 {
        join_group(&mut device, group, group as u32);
    }

    switch_group_on(&mut device, 5);
    assert!(!device.on_off(), "the fifth group never fitted");

    switch_group_on(&mut device, 4);
    assert!(device.on_off(), "the first four did");
}

#[test]
fn group_zero_addresses_nobody_and_is_refused() {
    let mut device = device();
    join_group(&mut device, 0, 0);

    switch_group_on(&mut device, 0);

    assert!(!device.on_off());
}

#[test]
fn adding_a_group_only_while_identifying_waits_for_the_identify() {
    let mut device = device();
    device.receive(
        &command(CLUSTER_GROUPS, 0x85, ADD_IF_IDENTIFYING, &[7, 0, 0]),
        at(0),
    );
    switch_group_on(&mut device, 7);
    assert!(!device.on_off(), "it was not identifying");

    device.receive(
        &command(zigbee::CLUSTER_IDENTIFY, 0x86, 0x00, &[30, 0]),
        at(20),
    );
    device.receive(
        &command(CLUSTER_GROUPS, 0x87, ADD_IF_IDENTIFYING, &[7, 0, 0]),
        at(30),
    );
    switch_group_on(&mut device, 7);
    assert!(device.on_off());
}
