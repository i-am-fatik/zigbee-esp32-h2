# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.5 - 2026-09-02

### Added

- `Device::receive_with_quality`, which takes the link quality the radio
  measured alongside the frame. `Device::receive` stands and picks a parent on
  hop count alone.
- `Credentials::parent`, the router the device joined through.

### Fixed

- A scan settles on the parent it hears best rather than the first one to
  answer, so a device no longer attaches across a link too weak to carry a
  command and pay the coordinator's retry timer on every one.
- A restored device asks its stored parent to take it back, so a parent that
  dropped it while it was off no longer swallows everything sent to it.
- A firmware query nobody answers backs off instead of repeating every few
  seconds for as long as the device is awake.

### Changed

- The package ships the library alone, without the test directory.

## 0.1.4 - 2026-08-25

### Added

- Identify takes Trigger Effect, so blink, breathe, okay and channel change
  each hold the light's attention for as long as they are named for, and stop
  ends whichever is running.
- The colour cluster takes Move Hue, which is the command a bridge sends for a
  colour loop. The hue turns at the given rate and wraps, and either a hold or
  a stop leaves it where it was caught.

## 0.1.3 - 2026-08-25

### Added

- The light takes `StartUpOnOff` and `StartUpColorTemperatureMireds`, kept in
  the tables and applied when it comes back up. A bridge offers both controls
  on any generated light definition, and writing either was refused before.

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
