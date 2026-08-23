# zigbee-rs

A Zigbee end device stack for `no_std` targets, written sans-io.

The stack owns no radio, no clock and no storage. It is handed the frames a
radio received and the time they arrived, and it hands back the frames to
transmit, the events worth acting on, and the credentials worth keeping.
Nothing in its public API comes from another crate, so the same code runs on
any IEEE 802.15.4 radio and in a test with no radio at all.

```rust
let mut device = zigbee::Device::new(zigbee::Config::new(ieee));

loop {
    device.tick(now());
    while let Some(outgoing) = device.next_transmission() {
        radio.send(outgoing.frame, outgoing.request_cca);
    }
    if let Some(frame) = radio.receive() {
        device.receive(frame, now());
    }
}
```

## What it does

Scans for a network, associates, takes the network key from the trust centre,
answers the coordinator's interview, and serves a colour light on one
endpoint. A parent that stops answering is recovered by rejoining, which needs
no permit-join because a device holding the network key is already a member.

The application is fixed: this crate is a colour light, not a framework for
building arbitrary Zigbee devices. [`APPLICATION`] describes exactly what a
coordinator will find.

| Cluster | What a coordinator can do |
| --- | --- |
| Basic | read the manufacturer, model, versions and build |
| Identify | make the light announce itself, and ask how long is left |
| Groups | join and leave groups, so one command reaches many lights at once |
| Scenes | remember a setting and put it back, on its own or for a group |
| On/Off | on, off, toggle, and hear about every change |
| Level Control | set the brightness, step it, or ramp it until told to stop |
| Colour Control | a hue and a saturation, or a colour temperature in mireds |
| Upgrade (client) | asks a server for a newer image and hands it over a block at a time |

## What it does not do

No routing, no coordinator role, no sleepy end device and no green power. Colour is hue
and saturation or a temperature, never the XY space and never an enhanced hue,
which the capabilities attribute says out loud so a bridge converts on its own
side. A stated transition time is parsed and ignored, so a colour or a
brightness the coordinator asks for arrives at once rather than fading in. A
ramp started by Move does take time, and it advances on `tick` like everything
else in the stack. Frames it does not understand are dropped in silence.

An upgrade is downloaded but never applied here: the stack strips the file
header, hands out the firmware a block at a time, and says when the last one
arrived. Writing it down and booting into it belongs to the caller, because a
stack that owns no storage cannot own a bootloader either. A device that has
not been told which firmware it runs never asks for one.

The group and scene tables hold four groups and eight scenes. Every change to
either offers a `TablesChanged` event carrying the bytes to write down, and
`Device::restore_tables` puts them back, so a light comes back from a restart
belonging to the same groups and holding the same scenes.

## Joining, and who can listen

By default the trust centre sends the network key encrypted under the link key
printed in the specification, so anyone within radio range at the moment of
pairing reads it and holds the network from then on.

`Config::with_install_code` replaces that key with one derived from sixteen
secret octets unique to the device. The same code has to reach the coordinator
out of band before it will let the device in, which is what turns a published
constant into something an eavesdropper does not have.

```rust
let config = Config::new(ieee).with_install_code(code);
let label = zigbee::install_code_label(&code);
```

A trust centre may also replace the network key at any time. The new key is
held until the switch command names it, so both keys are live in between and
frames sent under either are still read. A rotation never costs a rejoin.

The code protects the join and nothing after it. Once the device holds the
network key, its traffic is protected exactly as it was before.

## Cryptography

Zigbee mandates AES-CCM* and AES-MMO, so neither AES-GCM nor ChaCha20-Poly1305
is available as a substitute. The two halves are not equally trustworthy and
the difference is worth knowing before you depend on this.

- **CCM\*** comes from [`ccm`](https://crates.io/crates/ccm), the RustCrypto
  implementation. It compares the integrity code in constant time and zeroes
  the buffer when the check fails.
- **AES-MMO**, and the HMAC built on it, are hand-written here because no
  reviewed crate implements them. They run only during a join, deriving the key
  that protects the transported network key.

The hand-written half has never been audited. It matches the specification and
it decrypts a full transport-key exchange, which is evidence that it is
is correct and no evidence at all that it resists a side channel.

## The name

The crate is `zigbee-rs` on the registry because `zigbee` was already taken.
The library it builds is `zigbee`, so every example here compiles as written.

```toml
zigbee-rs = "0.1"
```

## Minimum supported Rust version

1.85, checked by building against that toolchain. The floor comes from the
RustCrypto dependencies, not from this crate.

## Licence

Either [Apache 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.
