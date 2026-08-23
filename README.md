# zigbee-esp32-h2

A Zigbee end device on the ESP32-H2, in bare-metal Rust. No ESP-IDF, no `std`,
no vendor Zigbee SDK.

Two crates. [`zigbee/`](zigbee) is the protocol stack, sans-io and portable.
[`firmware/`](firmware) is the ESP32-H2 light that uses it.

## Run it

```sh
cargo build --release
espflash flash --chip esp32h2 --port /dev/cu.usbmodem101 \
    --partition-table firmware/partitions.csv \
    target/riscv32imac-unknown-none-elf/release/zigbee-h2 --monitor
```

The partition table is not the default one. It carries two application slots so
an image arriving over the air is written to the slot that is not running, and
booted only once every byte of it landed. Passing it is required on every flash,
because a device flashed with the default single-slot table cannot update over
the air at all.

The device scans channels 11 to 26, joins the first Zigbee PRO network that
accepts it, and answers the coordinator as a colour light. Credentials go to
flash, so a reboot or a firmware update rejoins without pairing again. Groups
and scenes go to the next sector along, so they come back too.

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
| Network | headers, auxiliary security, frame counters, extended nonces, key rotation |
| Application support | data, command and ack frames, group addressing, APS security, transport key |
| Device object | announce, descriptors, endpoints, address requests, bind |
| Upgrade | queries a server, downloads an image, hands it over for writing |
| Clusters | Basic, Identify, On/Off, Level Control, Colour Control, including attribute reporting |
| Groups and scenes | group addressing on receive, four groups and eight scenes, kept in flash |

## What the LED says

| Colour | Meaning |
| --- | --- |
| blue, blinking | looking for a network |
| green, dim | joined, light off |
| the colour asked for | joined, light on, at the hue, saturation and brightness the coordinator set |

## Zigbee2MQTT

`H2.NoStd.Light` is not in the Zigbee2MQTT device database, so the bridge pairs
the device and then reports it as unsupported. [`zigbee2mqtt/`](zigbee2mqtt)
holds the definition that closes that gap.

| Step | Why |
| --- | --- |
| `enable_external_js: true` | from 2.11.0 a new installation ignores `external_converters/` without it |
| Copy `h2-nostd-light.mjs` into `external_converters/` | or publish it to `zigbee2mqtt/bridge/request/converter/save` |
| Check `zigbee2mqtt/bridge/converters` | the folder proves nothing about what loaded |

Both extends are there because the device earned them. `identify()` was added
only after the debug log said `No converter available for 'identify'`, which is
the line that licenses a rung on the ladder. `light()` replaced the earlier
`onOff()` when the Level Control cluster arrived, because one extend carries
the switch, the slider and the colour wheel between them, and its arguments are
read off the cluster list rather than written down.

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

## Updating over the air

Bump `FIRMWARE_VERSION` in `firmware/src/main.rs`, build, and wrap the binary in
a Zigbee upgrade file for the coordinator to serve. The device asks on every
join and whenever a server sends an Image Notify, downloads at 48 octets a
block, and restarts into the new slot only after the server agrees the image is
complete.

An update that is interrupted leaves the running slot untouched, because the
bootloader is only pointed at the new one after the last block.

## Pairing without letting the neighbours listen

The device joins under the link key printed in the Zigbee specification, so a
radio listening during pairing reads the network key and keeps it. An install
code closes that, and it lives in flash rather than in this repository, because
a secret in a public repository is not one.

```sh
openssl rand 16 > install-code.bin
espflash write-bin --port /dev/cu.usbmodem101 0xb000 install-code.bin
```

The device prints the code and its checksum on the console at every boot:

```
store: install code 83FED3407A939723A5C639B26916D505C3B5
```

Give that whole string to the bridge before pairing, by publishing to
`zigbee2mqtt/bridge/request/install_code/add`. The same value comes out of a
host without the board attached:

```sh
cargo run -p zigbee-rs --example install-code --target aarch64-apple-darwin -- \
    $(xxd -p install-code.bin)
```

A device whose code the coordinator does not know never finishes joining, so add
it to the bridge first. Erasing the sector puts the device back to joining the
old way, and `espflash erase-region` on the network sectors leaves the code
alone, because it belongs to the device rather than to any one network.

| Sector | Holds |
| --- | --- |
| 0x9000 | network credentials, erased on leaving |
| 0xa000 | groups and scenes, erased on leaving |
| 0xb000 | install code, never erased from firmware |

## To forget a network

```sh
espflash erase-region --port /dev/cu.usbmodem101 0x9000 0x1000
```

## Pinned versions

`esp-hal` 1.1.2 does not build against the `esp32h2` peripheral crate 0.19.3, so
`Cargo.lock` holds it at 0.19.2. Keep that pin when updating dependencies.

The radio's analog front end is undocumented, so `esp-radio` links Espressif's
PHY calibration blob. Everything above the PHY is Rust in this repository.

## Licence

Either [Apache 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.
