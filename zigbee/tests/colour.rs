//! Drives the colour wheel and the white slider the way a coordinator does.

mod common;

use common::{at, command, device, events};
use zigbee::{Colour, Device, Event, CLUSTER_COLOUR_CONTROL, COLOUR_TEMPERATURE_MIREDS};

const MOVE_TO_HUE: u8 = 0x00;
const STEP_HUE: u8 = 0x02;
const MOVE_TO_SATURATION: u8 = 0x03;
const STEP_SATURATION: u8 = 0x05;
const MOVE_TO_HUE_AND_SATURATION: u8 = 0x06;
const MOVE_TO_TEMPERATURE: u8 = 0x0a;

const UP: u8 = 0x01;
const DOWN: u8 = 0x03;

fn wheel(hue: u8, saturation: u8) -> Vec<u8> {
    command(
        CLUSTER_COLOUR_CONTROL,
        0x70,
        MOVE_TO_HUE_AND_SATURATION,
        &[hue, saturation, 0x00, 0x00],
    )
}

fn white(mireds: u16) -> Vec<u8> {
    let mut arguments = mireds.to_le_bytes().to_vec();
    arguments.extend_from_slice(&0u16.to_le_bytes());
    command(
        CLUSTER_COLOUR_CONTROL,
        0x71,
        MOVE_TO_TEMPERATURE,
        &arguments,
    )
}

fn nudge(id: u8, direction: u8, size: u8) -> Vec<u8> {
    command(CLUSTER_COLOUR_CONTROL, 0x72, id, &[direction, size, 0x00])
}

fn coloured(hue: u8, saturation: u8) -> Device {
    let mut device = device();
    device.receive(&wheel(hue, saturation), at(0));
    events(&mut device);
    device
}

#[test]
fn a_fresh_light_is_a_warm_white() {
    match device().colour() {
        Colour::Temperature { mireds } => {
            assert!(COLOUR_TEMPERATURE_MIREDS.contains(&mireds));
            assert!(mireds > 300, "warm, not daylight");
        }
        other => panic!("expected a white, got {other:?}"),
    }
}

#[test]
fn the_wheel_sets_both_hue_and_saturation() {
    let mut device = device();
    device.receive(&wheel(90, 200), at(0));

    assert_eq!(
        device.colour(),
        Colour::HueSaturation {
            hue: 90,
            saturation: 200
        }
    );
    assert!(events(&mut device)
        .iter()
        .any(|event| matches!(event, Event::ColourChanged(_))));
}

#[test]
fn moving_the_hue_leaves_the_saturation_alone() {
    let mut device = coloured(10, 180);
    device.receive(
        &command(
            CLUSTER_COLOUR_CONTROL,
            0x73,
            MOVE_TO_HUE,
            &[200, 0x00, 0x00, 0x00],
        ),
        at(10),
    );

    assert_eq!(
        device.colour(),
        Colour::HueSaturation {
            hue: 200,
            saturation: 180
        }
    );
}

#[test]
fn moving_the_saturation_leaves_the_hue_alone() {
    let mut device = coloured(10, 180);
    device.receive(
        &command(
            CLUSTER_COLOUR_CONTROL,
            0x74,
            MOVE_TO_SATURATION,
            &[40, 0x00, 0x00],
        ),
        at(10),
    );

    assert_eq!(
        device.colour(),
        Colour::HueSaturation {
            hue: 10,
            saturation: 40
        }
    );
}

#[test]
fn a_white_replaces_the_colour_rather_than_layering_on_it() {
    let mut device = coloured(90, 200);
    device.receive(&white(250), at(10));

    assert_eq!(device.colour(), Colour::Temperature { mireds: 250 });
}

#[test]
fn a_colour_replaces_the_white_the_same_way() {
    let mut device = device();
    device.receive(&white(250), at(0));
    device.receive(&wheel(30, 100), at(10));

    assert_eq!(
        device.colour(),
        Colour::HueSaturation {
            hue: 30,
            saturation: 100
        }
    );
}

#[test]
fn a_temperature_beyond_what_the_light_does_is_clamped() {
    let mut device = device();

    device.receive(&white(1), at(0));
    assert_eq!(
        device.colour(),
        Colour::Temperature {
            mireds: *COLOUR_TEMPERATURE_MIREDS.start()
        }
    );

    device.receive(&white(9_000), at(10));
    assert_eq!(
        device.colour(),
        Colour::Temperature {
            mireds: *COLOUR_TEMPERATURE_MIREDS.end()
        }
    );
}

#[test]
fn a_hue_step_comes_back_round_the_circle() {
    let mut device = coloured(10, 254);

    device.receive(&nudge(STEP_HUE, DOWN, 20), at(10));
    assert_eq!(
        device.colour(),
        Colour::HueSaturation {
            hue: 245,
            saturation: 254
        },
        "stepping below zero wraps to the far side"
    );

    device.receive(&nudge(STEP_HUE, UP, 20), at(20));
    assert_eq!(
        device.colour(),
        Colour::HueSaturation {
            hue: 10,
            saturation: 254
        }
    );
}

#[test]
fn a_saturation_step_stops_at_the_ends() {
    let mut device = coloured(10, 20);

    device.receive(&nudge(STEP_SATURATION, DOWN, 200), at(10));
    assert_eq!(
        device.colour(),
        Colour::HueSaturation {
            hue: 10,
            saturation: 0
        }
    );

    device.receive(&nudge(STEP_SATURATION, UP, 255), at(20));
    assert_eq!(
        device.colour(),
        Colour::HueSaturation {
            hue: 10,
            saturation: 254
        },
        "saturation is a line, not a circle"
    );
}

#[test]
fn a_step_in_no_known_direction_is_refused() {
    let mut device = coloured(10, 180);
    device.receive(&nudge(STEP_HUE, 0x02, 50), at(10));

    assert_eq!(
        device.colour(),
        Colour::HueSaturation {
            hue: 10,
            saturation: 180
        }
    );
}

#[test]
fn a_colour_that_did_not_move_is_not_announced() {
    let mut device = coloured(90, 200);
    device.receive(&wheel(90, 200), at(10));

    assert!(
        !events(&mut device)
            .iter()
            .any(|event| matches!(event, Event::ColourChanged(_))),
        "an unchanged colour is not news"
    );
}

#[test]
fn the_colour_is_independent_of_the_switch_and_the_brightness() {
    let mut device = coloured(90, 200);
    device.set_on_off(true);
    device.set_level(30);
    device.set_on_off(false);

    assert_eq!(
        device.colour(),
        Colour::HueSaturation {
            hue: 90,
            saturation: 200
        }
    );
}
