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
answers the coordinator's interview, and serves a dimmable light on one
endpoint. A parent that stops answering is recovered by rejoining, which needs
no permit-join because a device holding the network key is already a member.

The application is fixed: this crate is a dimmable light, not a framework for
building arbitrary Zigbee devices. [`APPLICATION`] describes exactly what a
coordinator will find.

| Cluster | What a coordinator can do |
| --- | --- |
| Basic | read the manufacturer, model, versions and build |
| Identify | make the light announce itself, and ask how long is left |
| On/Off | on, off, toggle, and hear about every change |
| Level Control | set the brightness, step it, or ramp it until told to stop |

## What it does not do

No routing, no coordinator role, no sleepy end device, no install codes, no
green power, no over-the-air updates. No colour and no scenes. A stated
transition time is parsed and ignored, so a brightness the coordinator asks
for arrives at once rather than fading in. A ramp started by Move does take
time, and it advances on `tick` like everything else in the stack.
Frames it does not understand are dropped in silence.

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
