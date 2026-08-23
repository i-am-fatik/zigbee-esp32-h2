//! Drives the brightness the way a coordinator's slider does, through the one
//! door the radio has.

mod common;

use common::{at, command, device, events};
use zigbee::{Device, Event, CLUSTER_LEVEL_CONTROL, MAX_LEVEL};

const MOVE_TO_LEVEL: u8 = 0x00;
const MOVE_TO_LEVEL_WITH_ON_OFF: u8 = 0x04;
const MOVE: u8 = 0x01;
const STEP: u8 = 0x02;
const STOP: u8 = 0x03;
const MOVE_WITH_ON_OFF: u8 = 0x05;
const STEP_WITH_ON_OFF: u8 = 0x06;
const UP: u8 = 0x00;
const DOWN: u8 = 0x01;

fn slider(id: u8, level: u8) -> Vec<u8> {
    command(CLUSTER_LEVEL_CONTROL, 0x60, id, &[level, 0x00, 0x00])
}

fn ramp(id: u8, direction: u8, rate: u8) -> Vec<u8> {
    command(CLUSTER_LEVEL_CONTROL, 0x61, id, &[direction, rate])
}

fn step(id: u8, direction: u8, size: u8) -> Vec<u8> {
    command(CLUSTER_LEVEL_CONTROL, 0x62, id, &[direction, size, 0x00, 0x00])
}

fn halt() -> Vec<u8> {
    command(CLUSTER_LEVEL_CONTROL, 0x63, STOP, &[])
}

fn light_at(level: u8) -> Device {
    let mut device = device();
    device.receive(&slider(MOVE_TO_LEVEL, level), at(0));
    events(&mut device);
    device
}

#[test]
fn a_fresh_light_is_at_full_brightness() {
    assert_eq!(device().level(), MAX_LEVEL);
}

#[test]
fn the_slider_moves_the_brightness() {
    let mut device = device();
    device.receive(&slider(MOVE_TO_LEVEL, 128), at(0));

    assert_eq!(device.level(), 128);
    assert!(events(&mut device)
        .iter()
        .any(|event| matches!(event, Event::LevelChanged(128))));
}

#[test]
fn plain_move_to_level_leaves_the_switch_alone() {
    let mut device = device();
    device.receive(&slider(MOVE_TO_LEVEL, 0), at(0));

    assert_eq!(device.level(), 0);
    assert!(!device.on_off(), "it was off to begin with");

    device.set_on_off(true);
    device.receive(&slider(MOVE_TO_LEVEL, 200), at(10));
    assert!(device.on_off(), "moving the level must not switch anything");
}

#[test]
fn brightness_above_zero_switches_the_light_on() {
    let mut device = device();
    assert!(!device.on_off());

    device.receive(&slider(MOVE_TO_LEVEL_WITH_ON_OFF, 80), at(0));

    assert!(device.on_off());
    assert_eq!(device.level(), 80);
    let events = events(&mut device);
    assert!(events
        .iter()
        .any(|event| matches!(event, Event::OnOffChanged(true))));
    assert!(events
        .iter()
        .any(|event| matches!(event, Event::LevelChanged(80))));
}

#[test]
fn brightness_zero_switches_the_light_off() {
    let mut device = device();
    device.set_on_off(true);
    events(&mut device);

    device.receive(&slider(MOVE_TO_LEVEL_WITH_ON_OFF, 0), at(0));

    assert!(!device.on_off());
    assert!(events(&mut device)
        .iter()
        .any(|event| matches!(event, Event::OnOffChanged(false))));
}

#[test]
fn an_undefined_level_is_clamped_to_the_brightest() {
    let mut device = device();
    device.receive(&slider(MOVE_TO_LEVEL, 0xff), at(0));

    assert_eq!(device.level(), MAX_LEVEL, "0xff means undefined, not brighter");
}

#[test]
fn the_level_survives_being_switched_off() {
    let mut device = device();
    device.receive(&slider(MOVE_TO_LEVEL_WITH_ON_OFF, 60), at(0));
    device.set_on_off(false);

    assert_eq!(device.level(), 60, "a dark light still knows how bright it was");
}

#[test]
fn setting_the_level_locally_announces_it_once() {
    let mut device = device();
    device.set_level(42);
    device.set_level(42);

    let announced = events(&mut device)
        .iter()
        .filter(|event| matches!(event, Event::LevelChanged(42)))
        .count();
    assert_eq!(announced, 1, "an unchanged level is not news");
}

#[test]
fn a_move_ramps_the_brightness_as_time_passes() {
    let mut device = light_at(100);
    device.receive(&ramp(MOVE, UP, 50), at(1_000));

    device.tick(at(1_000));
    assert_eq!(device.level(), 100, "no time has passed yet");

    device.tick(at(2_000));
    assert_eq!(device.level(), 150);

    device.tick(at(3_500));
    assert_eq!(device.level(), 225);
}

#[test]
fn a_move_ends_when_it_runs_into_the_top() {
    let mut device = light_at(10);
    device.receive(&ramp(MOVE, UP, 100), at(0));

    device.tick(at(10_000));
    assert_eq!(device.level(), MAX_LEVEL);

    device.tick(at(600_000));
    assert_eq!(device.level(), MAX_LEVEL, "a finished ramp stays finished");
}

#[test]
fn a_move_down_with_on_off_switches_the_light_off_at_the_bottom() {
    let mut device = light_at(50);
    device.set_on_off(true);
    events(&mut device);

    device.receive(&ramp(MOVE_WITH_ON_OFF, DOWN, 100), at(0));
    device.tick(at(1_000));

    assert_eq!(device.level(), 0);
    assert!(!device.on_off());
    assert!(events(&mut device)
        .iter()
        .any(|event| matches!(event, Event::OnOffChanged(false))));
}

#[test]
fn stop_leaves_the_brightness_where_the_ramp_reached() {
    let mut device = light_at(100);
    device.receive(&ramp(MOVE, UP, 50), at(0));
    device.tick(at(1_000));
    assert_eq!(device.level(), 150);

    device.receive(&halt(), at(1_000));
    device.tick(at(9_000));

    assert_eq!(device.level(), 150);
}

#[test]
fn a_step_moves_the_brightness_at_once() {
    let mut device = light_at(100);
    device.receive(&step(STEP, UP, 30), at(0));

    assert_eq!(device.level(), 130, "a step needs no tick");
    assert!(events(&mut device)
        .iter()
        .any(|event| matches!(event, Event::LevelChanged(130))));
}

#[test]
fn a_step_past_an_end_of_the_range_stops_at_it() {
    let mut device = light_at(100);
    device.receive(&step(STEP, DOWN, 200), at(0));
    assert_eq!(device.level(), 0);

    device.receive(&step(STEP, UP, 255), at(10));
    assert_eq!(device.level(), MAX_LEVEL);
}

#[test]
fn a_step_with_on_off_switches_the_light_off_when_it_lands_on_zero() {
    let mut device = light_at(20);
    device.set_on_off(true);
    events(&mut device);

    device.receive(&step(STEP_WITH_ON_OFF, DOWN, 20), at(0));

    assert_eq!(device.level(), 0);
    assert!(!device.on_off());
}

#[test]
fn a_move_at_no_rate_is_refused() {
    let mut device = light_at(100);
    device.receive(&ramp(MOVE, UP, 0), at(0));

    device.tick(at(60_000));
    assert_eq!(device.level(), 100, "a rate of zero is not a move");
}

#[test]
fn a_move_in_no_known_direction_is_refused() {
    let mut device = light_at(100);
    device.receive(&ramp(MOVE, 0x07, 50), at(0));

    device.tick(at(5_000));
    assert_eq!(device.level(), 100);
}

#[test]
fn a_new_level_command_cancels_the_ramp() {
    let mut device = light_at(100);
    device.receive(&ramp(MOVE, UP, 50), at(0));
    device.tick(at(1_000));

    device.receive(&slider(MOVE_TO_LEVEL, 20), at(1_000));
    device.tick(at(9_000));

    assert_eq!(device.level(), 20);
}

#[test]
fn setting_the_level_locally_cancels_the_ramp() {
    let mut device = light_at(100);
    device.receive(&ramp(MOVE, UP, 50), at(0));
    device.tick(at(1_000));

    device.set_level(77);
    device.tick(at(9_000));

    assert_eq!(device.level(), 77);
}
