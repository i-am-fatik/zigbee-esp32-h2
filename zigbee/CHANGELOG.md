# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.2 - 2026-08-25

### Added

- The colour cluster serves the XY space: `currentX`, `currentY` and Move to
  Color, reported through the new `Colour::Xy`. A bridge that generates its own
  definition offers no colour control at all without it, so a light claiming
  only hue and saturation came out white-only.

## 0.1.1 - 2026-08-24

### Fixed

- A reply too big for one 802.15.4 frame was dropped without a word, so a
  coordinator reading many attributes at once heard silence and timed out.
  A read now answers with the records that fit and stops there.

## 0.1.0 - 2026-08-23

First release.

### Added

- A sans-io Zigbee end device stack. The caller owns the radio, the clock and
  the storage, and the crate hands back the frames to transmit, the events
  worth acting on and the credentials worth keeping.
- Joining by association, and recovery from a lost parent by rejoining, which
  needs no permit-join because a device holding the network key is already a
  member.
- `Config::with_install_code`, which derives the join key from sixteen secret
  octets instead of the link key printed in the specification.
- Network key rotation. Both keys are live between the transport and the
  switch, so a re-key costs no rejoin.
- A colour light on one endpoint, serving Basic, Identify, Groups, Scenes,
  On/Off, Level Control and Colour Control, and acting as a client of
  Over-the-Air Upgrade.
- Group and scene tables that survive a restart. Every change offers the bytes
  to write down, and `Device::restore_tables` puts them back.

What the crate deliberately does not do, and which half of its cryptography
has never been audited, are stated in the README.
