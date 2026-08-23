//! Pins the promises the compiler makes on the crate's behalf without ever
//! writing them down. Nothing here runs; instantiating the generics is the
//! assertion, so a lost promise is a build failure rather than a bug report.

use zigbee::{Application, Config, Credentials, Device, Event, Instant, RadioConfig, Transmission};

fn shareable<T: Send + Sync + Sized + Unpin>() {}
fn portable<T: Clone + Copy + core::fmt::Debug>() {}

#[test]
fn the_device_can_cross_a_thread_boundary() {
    shareable::<Device>();
    shareable::<Config>();
    shareable::<Credentials>();
    shareable::<Event>();
    shareable::<RadioConfig>();
    shareable::<Application>();
    shareable::<Transmission<'static>>();
}

#[test]
fn the_values_a_caller_holds_on_to_stay_cheap_to_copy() {
    portable::<Config>();
    portable::<Credentials>();
    portable::<Event>();
    portable::<RadioConfig>();
    portable::<Instant>();
}
