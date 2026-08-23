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
answers the coordinator's interview, and serves an On/Off light on one
endpoint. A parent that stops answering is recovered by rejoining, which needs
no permit-join because a device holding the network key is already a member.

The application is fixed: this crate is an On/Off light, not a framework for
building arbitrary Zigbee devices. [`APPLICATION`] describes exactly what a
coordinator will find.

## What it does not do

No routing, no coordinator role, no sleepy end device, no install codes, no
green power, no over-the-air updates. Frames it does not understand are
dropped in silence.

## The name

The crate is `zigbee-rs` on the registry because `zigbee` was already taken.
The library it builds is `zigbee`, so every example here compiles as written.

```toml
zigbee-rs = "0.1"
```

## Minimum supported Rust version

1.76, checked by building against that toolchain.
