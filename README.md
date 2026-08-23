# zigbee-esp32-h2

A Zigbee end device on the ESP32-H2, in bare-metal Rust. No ESP-IDF, no `std`,
no vendor Zigbee SDK.

Two crates. [`zigbee/`](zigbee) is the protocol stack, sans-io and portable.
[`firmware/`](firmware) is the ESP32-H2 light that uses it.

## Run it

```sh
cargo build --release
espflash flash --chip esp32h2 --port /dev/cu.usbmodem101 \
    target/riscv32imac-unknown-none-elf/release/zigbee-h2 --monitor
```

The device scans channels 11 to 26, joins the first Zigbee PRO network that
accepts it, and answers the coordinator as an On/Off light. Credentials go to
flash, so a reboot or a firmware update rejoins without pairing again.

The BOOT button on GPIO9 toggles the light locally, debounced over 25 ms. The
coordinator hears about it through the same attribute report it would get from
a switch command, so the two ways of changing the light are indistinguishable
from outside.

A parent that stops answering is the other way to lose the network, and it is
recovered without a reboot. The device polls its parent once a minute, and three
refused frames in a row send it looking for another router on the same network.
That search needs no permit-join, because a device holding the network key is a
member rather than a stranger. Only when no router answers does it fall back to
scanning for a new network.

```sh
cargo test -p zigbee-rs --target aarch64-apple-darwin
```

The stack has no hardware in its dependencies, so its tests run on the host.
They replay a full join exchange, including a trust centre's encrypted
encrypted network key.

## The library

The stack owns no radio, no clock and no storage. It takes received bytes and a
millisecond count, and produces frames to transmit, events to act on, and
credentials to persist. Nothing in its public API comes from another crate.

The loop that drives it is documented on [`zigbee::Device`](zigbee/src/lib.rs),
where the example is compiled and run as a doc test. A real one is
[`firmware/src/main.rs`](firmware/src/main.rs).

| Layer | Covers |
| --- | --- |
| IEEE 802.15.4 MAC | frame control, addressing, beacons, association, data requests |
| Security | AES-CCM\*, AES-MMO, HMAC, key-transport key derivation |
| Network | headers, auxiliary security, frame counters, extended nonces |
| Application support | data, command and ack frames, APS security, transport key |
| Device object | announce, descriptors, endpoints, address requests, bind |
| Clusters | Basic, Identify, On/Off, including attribute reporting |

## What the LED says

| Colour | Meaning |
| --- | --- |
| blue, blinking | looking for a network |
| green, dim | joined, light off |
| warm white | joined, light on |

## Zigbee2MQTT

`H2.NoStd.Light` is not in the Zigbee2MQTT device database, so the bridge pairs
the device and then reports it as unsupported. [`zigbee2mqtt/`](zigbee2mqtt)
holds the definition that closes that gap.

| Step | Why |
| --- | --- |
| `enable_external_js: true` | from 2.11.0 a new installation ignores `external_converters/` without it |
| Copy `h2-nostd-light.mjs` into `external_converters/` | or publish it to `zigbee2mqtt/bridge/request/converter/save` |
| Check `zigbee2mqtt/bridge/converters` | the folder proves nothing about what loaded |

Both extends are there because the device earned them. `onOff()` was proven by
round trip. `identify()` was added only after the debug log said
`No converter available for 'identify'`, which is the line that licenses a rung
on the ladder.

The file is generated rather than written:

```sh
cargo run -p zigbee-rs --example zigbee2mqtt --target aarch64-apple-darwin -- \
    --model H2.NoStd.Light --vendor esp-rs \
    --description "ESP32-H2 no_std Rust Zigbee light" \
    > zigbee2mqtt/h2-nostd-light.mjs
```

The extends come from the clusters the stack serves, so they cannot drift from
the firmware. The identity comes from the arguments, because `zigbeeModel` has
to be the string the interview reported. Read it off the device page or
`zigbee2mqtt/bridge/devices`, never off `zcl.rs` - a definition keyed on a
string the device does not report is a file that silently never matches.

Anything served but unmapped is reported on stderr. Nothing is claimed about
whether an extend works, which is still settled by exercising it.

## To forget a network

```sh
espflash erase-region --port /dev/cu.usbmodem101 0x9000 0x1000
```

## Pinned versions

`esp-hal` 1.1.2 does not build against the `esp32h2` peripheral crate 0.19.3, so
`Cargo.lock` holds it at 0.19.2. Keep that pin when updating dependencies.

The radio's analog front end is undocumented, so `esp-radio` links Espressif's
PHY calibration blob. Everything above the PHY is Rust in this repository.
