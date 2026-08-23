use crate::buf::Writer;
use crate::crypto::{install_code_link_key, key_transport_key, KEY_LEN};
use crate::{aps, mac, nwk, ota, zcl, zdo, Instant, CHANNELS, INSTALL_CODE_LEN};

/// The link key every Zigbee device is allowed to fall back to when it joins a
/// centralised network without an install code.
const DEFAULT_TRUST_CENTRE_LINK_KEY: [u8; KEY_LEN] = *b"ZigBeeAlliance09";

fn link_key(config: &Config) -> [u8; KEY_LEN] {
    match config.install_code {
        Some(code) => install_code_link_key(&code),
        None => DEFAULT_TRUST_CENTRE_LINK_KEY,
    }
}

const ASSOCIATION_ACCEPTED: u8 = 0x00;
const STACK_PROFILE_ZIGBEE_PRO: u8 = 2;

const SCAN_DWELL_MS: u32 = 250;
const POLL_INTERVAL_MS: u32 = 300;
const ASSOCIATION_TIMEOUT_MS: u32 = 6_000;
const KEY_TIMEOUT_MS: u32 = 12_000;

/// How often a joined device checks that its parent is still listening.
const KEEPALIVE_MS: u32 = 60_000;
const FAILURES_BEFORE_REJOIN: u8 = 3;
const REJOIN_TIMEOUT_MS: u32 = 20_000;
const REJOIN_RETRY_MS: u32 = 2_000;

const REJOIN_ACCEPTED: u8 = 0x00;

/// How far ahead of the live counter a stored counter runs, so a power cut can
/// never replay a frame counter the coordinator has already accepted.
const COUNTER_MARGIN: u32 = 1024;

const COORDINATOR_ENDPOINT: u8 = 1;

const UNASSIGNED: u16 = 0xffff;
const OUTBOX_CAPACITY: usize = 4;
const EVENT_CAPACITY: usize = 4;

/// What the device tells the world about itself during the interview.
#[derive(Clone, Copy)]
pub struct Config {
    ieee: u64,
    manufacturer: &'static str,
    model: &'static str,
    software_build: &'static str,
    firmware: Option<ota::Identity>,
    install_code: Option<[u8; INSTALL_CODE_LEN]>,
}

impl core::fmt::Debug for Config {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Config")
            .field("ieee", &self.ieee)
            .field("manufacturer", &self.manufacturer)
            .field("model", &self.model)
            .field("software_build", &self.software_build)
            .field("firmware", &self.firmware)
            .finish_non_exhaustive()
    }
}

impl Config {
    /// Starts from the device's EUI-64, which is the only value with no
    /// sensible default.
    pub const fn new(ieee: u64) -> Self {
        Self {
            ieee,
            manufacturer: "unknown",
            model: "zigbee-rs",
            software_build: "0.1.0",
            firmware: None,
            install_code: None,
        }
    }

    /// Sets the manufacturer name reported by the Basic cluster.
    pub const fn with_manufacturer(mut self, manufacturer: &'static str) -> Self {
        self.manufacturer = manufacturer;
        self
    }

    /// Sets the model identifier reported by the Basic cluster.
    ///
    /// A coordinator matches its device definition against this string, so it
    /// is the one field worth choosing deliberately.
    pub const fn with_model(mut self, model: &'static str) -> Self {
        self.model = model;
        self
    }

    /// Sets the software build identifier reported by the Basic cluster.
    pub const fn with_software_build(mut self, software_build: &'static str) -> Self {
        self.software_build = software_build;
        self
    }

    /// Takes the install code printed on this device, so the trust centre can
    /// send the network key under a key nobody else knows.
    ///
    /// Without one the join is protected by the link key published in the
    /// specification, which means anyone listening at the moment of pairing
    /// reads the network key. With one, the coordinator has to be told the same
    /// code out of band before it will let the device in.
    ///
    /// The code is a secret and the `Debug` output omits it. Its printed form,
    /// checksum included, comes from [`crate::install_code_label`].
    pub const fn with_install_code(mut self, code: [u8; INSTALL_CODE_LEN]) -> Self {
        self.install_code = Some(code);
        self
    }

    /// Says which firmware is running, which is what turns over-the-air
    /// updating on.
    ///
    /// Without this the device serves the upgrade cluster and never asks for an
    /// image, because a server cannot answer a device that will not say what it
    /// is already running.
    pub const fn with_firmware(mut self, manufacturer: u16, image_type: u16, version: u32) -> Self {
        self.firmware = Some(ota::Identity {
            manufacturer,
            image_type,
            version,
        });
        self
    }

    /// The device's EUI-64.
    pub const fn ieee(&self) -> u64 {
        self.ieee
    }
}

/// Everything a device needs to rejoin the network it already belongs to.
///
/// The contents are deliberately opaque. Persist the bytes from
/// [`Credentials::to_bytes`] and hand them back through
/// [`Credentials::from_bytes`] after a restart.
///
/// Those bytes carry the network key, so storage that anyone else can read is
/// storage that hands them the network. The `Debug` output omits the key.
#[derive(Clone, Copy)]
pub struct Credentials {
    pan_id: u16,
    short_address: u16,
    parent: u16,
    channel: u8,
    key_sequence: u8,
    key: [u8; KEY_LEN],
    counter: u32,
}

impl Credentials {
    /// The number of bytes [`Credentials::to_bytes`] produces.
    pub const LEN: usize = 32;

    const MAGIC: u32 = 0x5a42_4832;

    /// Encodes the credentials for storage.
    ///
    /// ```
    /// # use zigbee::{Config, Credentials, Device, Event, Instant};
    /// # fn persist(device: &mut Device, flash: &mut [u8; Credentials::LEN]) {
    /// while let Some(event) = device.next_event() {
    ///     if let Event::CredentialsChanged(saved) = event {
    ///         *flash = saved.to_bytes();
    ///     }
    /// }
    /// # }
    /// ```
    pub fn to_bytes(&self) -> [u8; Self::LEN] {
        let mut record = [0u8; Self::LEN];
        record[0..4].copy_from_slice(&Self::MAGIC.to_le_bytes());
        record[4] = self.channel;
        record[5] = self.key_sequence;
        record[6..8].copy_from_slice(&self.pan_id.to_le_bytes());
        record[8..10].copy_from_slice(&self.short_address.to_le_bytes());
        record[10..12].copy_from_slice(&self.parent.to_le_bytes());
        record[12..28].copy_from_slice(&self.key);
        record[28..32].copy_from_slice(&self.counter.to_le_bytes());
        record
    }

    /// Decodes credentials produced by [`Credentials::to_bytes`], rejecting
    /// anything that is not a record this version wrote.
    ///
    /// Blank flash, flash written by an older layout, and flash holding
    /// something else all read back as `None`, which is the signal to join
    /// from scratch instead.
    ///
    /// ```
    /// # use zigbee::{Config, Credentials, Device};
    /// # let flash = [0u8; Credentials::LEN];
    /// # let config = Config::new(0x0011_2233_4455_6677);
    /// let device = match Credentials::from_bytes(&flash) {
    ///     Some(saved) => Device::restore(config, saved),
    ///     None => Device::new(config),
    /// };
    /// # let _ = device;
    /// ```
    pub fn from_bytes(record: &[u8; Self::LEN]) -> Option<Self> {
        if u32::from_le_bytes([record[0], record[1], record[2], record[3]]) != Self::MAGIC {
            return None;
        }
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(&record[12..28]);
        Some(Self {
            channel: record[4],
            key_sequence: record[5],
            pan_id: u16::from_le_bytes([record[6], record[7]]),
            short_address: u16::from_le_bytes([record[8], record[9]]),
            parent: u16::from_le_bytes([record[10], record[11]]),
            key,
            counter: u32::from_le_bytes([record[28], record[29], record[30], record[31]]),
        })
    }

    /// The short address the coordinator allocated.
    pub const fn short_address(&self) -> u16 {
        self.short_address
    }

    /// The channel the network runs on.
    pub const fn channel(&self) -> u8 {
        self.channel
    }
}

impl core::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Credentials")
            .field("pan_id", &self.pan_id)
            .field("short_address", &self.short_address)
            .field("parent", &self.parent)
            .field("channel", &self.channel)
            .finish_non_exhaustive()
    }
}

/// How the radio must be tuned and addressed for the stack to hear its traffic.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub struct RadioConfig {
    /// The IEEE 802.15.4 channel to listen on.
    pub channel: u8,
    /// The PAN the radio should accept frames for.
    pub pan_id: u16,
    /// The short address the radio should accept frames for.
    pub short_address: u16,
}

/// A piece of a new firmware image, to be written down and not acted on until
/// [`Event::FirmwareReady`] says the whole image arrived.
#[derive(Debug)]
#[non_exhaustive]
pub struct FirmwareBlock<'a> {
    /// Where these bytes belong in the image, counted from its first byte.
    pub offset: u32,
    /// The bytes to write.
    pub data: &'a [u8],
}

/// A frame the caller should put on the air.
#[derive(Debug)]
#[non_exhaustive]
pub struct Transmission<'a> {
    /// The MAC frame, without the physical length byte and without the checksum.
    pub frame: &'a [u8],
    /// Whether to assess the channel before transmitting.
    pub request_cca: bool,
}

const SCENE_RECORD: usize = 10;
const SCENES_AT: usize = 4 + 2 * zcl::MAX_GROUPS + 1;
const PACKED: usize = SCENES_AT + SCENE_RECORD * zcl::MAX_SCENES;

/// The groups the light belongs to and the scenes it can put back, in the form
/// to hand to storage.
///
/// These outlive a restart only if the caller writes them down. They carry no
/// secret, unlike [`Credentials`], so where they are kept matters less.
#[derive(Clone, Copy)]
pub struct Tables([u8; Tables::LEN]);

impl core::fmt::Debug for Tables {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Tables").finish_non_exhaustive()
    }
}

impl Tables {
    /// The number of bytes [`Tables::to_bytes`] produces.
    ///
    /// It is rounded up to a whole number of 32 bit words, because a NOR flash
    /// writes words and would refuse a record that ends part way through one.
    pub const LEN: usize = PACKED.next_multiple_of(4);

    const MAGIC: u32 = 0x5a54_424c;

    /// Encodes the tables for storage.
    ///
    /// ```
    /// # use zigbee::{Device, Event, Tables};
    /// # fn persist(device: &mut Device, flash: &mut [u8; Tables::LEN]) {
    /// while let Some(event) = device.next_event() {
    ///     if let Event::TablesChanged(saved) = event {
    ///         *flash = saved.to_bytes();
    ///     }
    /// }
    /// # }
    /// ```
    pub fn to_bytes(&self) -> [u8; Self::LEN] {
        self.0
    }

    /// Decodes tables produced by [`Tables::to_bytes`], rejecting anything that
    /// is not a record this version wrote.
    ///
    /// Blank flash, an older layout and somebody else's data all read back as
    /// `None`, which is the signal to start out belonging to nothing.
    pub fn from_bytes(record: &[u8; Self::LEN]) -> Option<Self> {
        let magic = u32::from_le_bytes([record[0], record[1], record[2], record[3]]);
        (magic == Self::MAGIC).then_some(Self(*record))
    }

    fn of(state: &zcl::State) -> Self {
        let mut record = [0u8; Self::LEN];
        record[0..4].copy_from_slice(&Self::MAGIC.to_le_bytes());

        for (slot, group) in record[4..].chunks_mut(2).zip(state.groups) {
            slot.copy_from_slice(&group.to_le_bytes());
        }

        let occupied = SCENES_AT - 1;
        for (index, scene) in state.scenes.iter().enumerate() {
            let Some(scene) = scene else { continue };
            record[occupied] |= 1 << index;
            let at = SCENES_AT + index * SCENE_RECORD;
            record[at..at + 2].copy_from_slice(&scene.group.to_le_bytes());
            record[at + 2] = scene.id;
            record[at + 3] = scene.on as u8;
            record[at + 4] = scene.level;
            record[at + 5] = scene.hue;
            record[at + 6] = scene.saturation;
            record[at + 7..at + 9].copy_from_slice(&scene.mireds.to_le_bytes());
            record[at + 9] = scene.colour_mode;
        }
        Self(record)
    }

    fn apply(&self, state: &mut zcl::State) {
        for (group, slot) in state.groups.iter_mut().zip(self.0[4..].chunks(2)) {
            *group = u16::from_le_bytes([slot[0], slot[1]]);
        }

        let occupied = self.0[SCENES_AT - 1];
        for (index, scene) in state.scenes.iter_mut().enumerate() {
            if occupied & (1 << index) == 0 {
                *scene = None;
                continue;
            }
            let at = SCENES_AT + index * SCENE_RECORD;
            *scene = Some(zcl::Scene {
                group: u16::from_le_bytes([self.0[at], self.0[at + 1]]),
                id: self.0[at + 2],
                on: self.0[at + 3] != 0,
                level: self.0[at + 4],
                hue: self.0[at + 5],
                saturation: self.0[at + 6],
                mireds: u16::from_le_bytes([self.0[at + 7], self.0[at + 8]]),
                colour_mode: self.0[at + 9],
            });
        }
    }
}

/// How the light was last told to colour itself.
///
/// The two are alternatives rather than layers: setting one replaces the other,
/// and [`Device::colour`] reports whichever the coordinator asked for last.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum Colour {
    /// A point on the colour wheel.
    HueSaturation {
        /// Position around the wheel, 0 to 254.
        hue: u8,
        /// How far from white, 0 being white, up to 254.
        saturation: u8,
    },
    /// A white, stated as a colour temperature.
    Temperature {
        /// Mireds, a million over kelvin, so the smaller number is the cooler
        /// light. From 153, about 6500 K, to 500, about 2000 K.
        mireds: u16,
    },
}

/// Something happened that the caller may want to act on.
///
/// The enum grows by variant, so match it with a wildcard arm. The variants
/// themselves are frozen.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum Event {
    /// The device is on a network and has the network key.
    Joined {
        /// The short address the coordinator allocated.
        short_address: u16,
    },
    /// The device left, or was told to leave, and is scanning again.
    Left,
    /// The light was switched, by the coordinator or by the caller.
    OnOffChanged(bool),
    /// The brightness moved, from 0 to [`crate::MAX_LEVEL`].
    LevelChanged(u8),
    /// The colour moved, or switched which way it is stated.
    ColourChanged(Colour),
    /// Credentials worth writing to storage, superseding any earlier ones.
    CredentialsChanged(Credentials),
    /// A group was joined or left, or a scene was written or forgotten. Worth
    /// writing to storage, superseding any earlier tables.
    TablesChanged(Tables),
    /// A server has an image and the download has started. The blocks arrive
    /// through [`Device::next_firmware_block`].
    FirmwareOffered {
        /// The version the image carries.
        version: u32,
        /// The size of the whole upgrade file, header included.
        size: u32,
    },
    /// Every byte of the image arrived and the server agreed it may be used.
    FirmwareReady,
    /// The download stopped without finishing, so whatever was written down is
    /// incomplete and must not be booted.
    FirmwareAbandoned,
}

enum Phase {
    Scanning {
        listen_until: Instant,
    },
    Associating {
        candidate: Candidate,
        give_up_at: Instant,
        next_poll: Instant,
    },
    WaitingForKey {
        give_up_at: Instant,
        next_poll: Instant,
    },
    /// Holding the network key but no longer heard by the old parent, looking
    /// for another router on the same network. Unlike a first join this needs
    /// no permit-join, because the device is already a member.
    Rejoining {
        give_up_at: Instant,
        next_attempt: Instant,
    },
    Joined,
}

#[derive(Clone, Copy)]
struct Candidate {
    channel: u8,
    pan_id: u16,
    parent: mac::Addr,
}

struct Queue<T, const N: usize> {
    slots: [T; N],
    head: usize,
    len: usize,
}

impl<T: Copy, const N: usize> Queue<T, N> {
    fn new(empty: T) -> Self {
        Self {
            slots: [empty; N],
            head: 0,
            len: 0,
        }
    }

    fn push(&mut self, value: T) {
        if self.len == N {
            self.head = (self.head + 1) % N;
            self.len -= 1;
        }
        self.slots[(self.head + self.len) % N] = value;
        self.len += 1;
    }

    fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        let value = self.slots[self.head];
        self.head = (self.head + 1) % N;
        self.len -= 1;
        Some(value)
    }
}

#[derive(Clone, Copy)]
struct Outgoing {
    frame: [u8; mac::MAX_FRAME_LEN],
    len: u8,
    request_cca: bool,
}

/// A Zigbee end device: a colour light that joins a network, answers the
/// coordinator's interview, and reports what it was told to do.
pub struct Device {
    config: Config,
    radio: RadioConfig,
    parent: u16,
    network_key: Option<[u8; KEY_LEN]>,
    key_sequence: u8,
    next_network_key: Option<([u8; KEY_LEN], u8)>,
    phase: Phase,
    application: zcl::State,
    transport_key: [u8; KEY_LEN],
    counter: u32,
    counter_persisted: u32,
    mac_seq: u8,
    nwk_seq: u8,
    aps_counter: u8,
    zdo_seq: u8,
    identifying: bool,
    firmware: ota::State,
    firmware_block: ota::Block,
    consecutive_failures: u8,
    next_keepalive: Instant,
    outbox: Queue<Outgoing, OUTBOX_CAPACITY>,
    events: Queue<Option<Event>, EVENT_CAPACITY>,
}

impl core::fmt::Debug for Device {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Device")
            .field("radio", &self.radio)
            .field("joined", &self.joined())
            .field("on", &self.application.on)
            .finish_non_exhaustive()
    }
}

impl Device {
    /// Builds a device that has to find and join a network.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            radio: RadioConfig {
                channel: *CHANNELS.start(),
                pan_id: UNASSIGNED,
                short_address: UNASSIGNED,
            },
            parent: nwk::COORDINATOR,
            network_key: None,
            key_sequence: 0,
            next_network_key: None,
            phase: Phase::Scanning {
                listen_until: Instant::from_millis(0),
            },
            application: zcl::State::default(),
            transport_key: key_transport_key(&link_key(&config)),
            counter: 0,
            counter_persisted: 0,
            mac_seq: 0,
            nwk_seq: 0,
            aps_counter: 0,
            zdo_seq: 0,
            identifying: false,
            firmware: ota::State::default(),
            firmware_block: ota::Block::default(),
            consecutive_failures: 0,
            next_keepalive: Instant::from_millis(KEEPALIVE_MS),
            outbox: Queue::new(Outgoing {
                frame: [0; mac::MAX_FRAME_LEN],
                len: 0,
                request_cca: false,
            }),
            events: Queue::new(None),
        }
    }

    /// Builds a device that already belongs to a network and announces itself
    /// on the next [`Device::tick`] rather than scanning.
    pub fn restore(config: Config, credentials: Credentials) -> Self {
        let mut device = Self::new(config);
        device.ask_for_firmware(Instant::from_millis(0));
        device.radio = RadioConfig {
            channel: credentials.channel,
            pan_id: credentials.pan_id,
            short_address: credentials.short_address,
        };
        device.parent = credentials.parent;
        device.network_key = Some(credentials.key);
        device.key_sequence = credentials.key_sequence;
        device.counter = credentials.counter;
        device.counter_persisted = credentials.counter;
        device.phase = Phase::Joined;
        device.announce();
        device
    }

    /// Puts back groups and scenes read from storage.
    ///
    /// Unlike [`Device::restore`] this is not part of joining a network, so it
    /// can be called on a device built either way.
    pub fn restore_tables(&mut self, tables: Tables) {
        tables.apply(&mut self.application);
    }

    /// Lets time pass: drives scanning, association, polling and reporting.
    pub fn tick(&mut self, now: Instant) {
        self.identifying = self.application.identify_remaining(now) > 0;
        let moved = self.application.advance(now);
        self.publish_change(moved);

        if self.joined() {
            self.honour_reporting(now);
            self.drive_firmware(now);
            if now.reached(self.next_keepalive) {
                self.next_keepalive = now.plus_millis(KEEPALIVE_MS);
                let parent = mac::Addr::Short(self.parent);
                self.send_data_request(self.radio.pan_id, parent);
            }
            return;
        }

        match self.phase {
            Phase::Scanning { listen_until } => {
                if !now.reached(listen_until) {
                    return;
                }
                self.radio.channel = if self.radio.channel >= *CHANNELS.end() {
                    *CHANNELS.start()
                } else {
                    self.radio.channel + 1
                };
                self.send_beacon_request();
                self.phase = Phase::Scanning {
                    listen_until: now.plus_millis(SCAN_DWELL_MS),
                };
            }
            Phase::Associating {
                candidate,
                give_up_at,
                next_poll,
            } => {
                if now.reached(give_up_at) {
                    self.restart_scan(now);
                    return;
                }
                if now.reached(next_poll) {
                    self.send_data_request(candidate.pan_id, candidate.parent);
                    self.phase = Phase::Associating {
                        candidate,
                        give_up_at,
                        next_poll: now.plus_millis(POLL_INTERVAL_MS),
                    };
                }
            }
            Phase::WaitingForKey {
                give_up_at,
                next_poll,
            } => {
                if now.reached(give_up_at) {
                    self.restart_scan(now);
                    return;
                }
                if now.reached(next_poll) {
                    let parent = mac::Addr::Short(self.parent);
                    self.send_data_request(self.radio.pan_id, parent);
                    self.phase = Phase::WaitingForKey {
                        give_up_at,
                        next_poll: now.plus_millis(POLL_INTERVAL_MS),
                    };
                }
            }
            Phase::Rejoining {
                give_up_at,
                next_attempt,
            } => {
                if now.reached(give_up_at) {
                    self.restart_scan(now);
                    return;
                }
                if now.reached(next_attempt) {
                    self.send_beacon_request();
                    self.phase = Phase::Rejoining {
                        give_up_at,
                        next_attempt: now.plus_millis(REJOIN_RETRY_MS),
                    };
                }
            }
            Phase::Joined => {}
        }
    }

    /// Hands the stack a MAC frame the radio received, without the physical
    /// length byte and without the checksum.
    pub fn receive(&mut self, frame: &[u8], now: Instant) {
        if frame.len() > mac::MAX_FRAME_LEN {
            return;
        }
        let Some(parsed) = mac::parse(frame) else {
            return;
        };

        match parsed.frame_type {
            mac::FRAME_TYPE_BEACON => {
                if let Some(beacon) = mac::parse_beacon(&parsed) {
                    self.on_beacon(beacon, now);
                }
            }
            mac::FRAME_TYPE_COMMAND => {
                if let Some(response) = mac::parse_association_response(&parsed) {
                    self.on_association_response(response, now);
                }
            }
            mac::FRAME_TYPE_DATA => {
                let payload_len = parsed.payload.len();
                let mut network = [0u8; mac::MAX_FRAME_LEN];
                network[..payload_len].copy_from_slice(parsed.payload);
                self.on_network_frame(&mut network[..payload_len], now);
            }
            _ => {}
        }
    }

    /// Takes the next frame to put on the air, if any.
    pub fn next_transmission(&mut self) -> Option<Transmission<'_>> {
        if self.outbox.len == 0 {
            return None;
        }
        let index = self.outbox.head;
        let len = self.outbox.slots[index].len as usize;
        let request_cca = self.outbox.slots[index].request_cca;
        self.outbox.head = (index + 1) % OUTBOX_CAPACITY;
        self.outbox.len -= 1;
        Some(Transmission {
            frame: &self.outbox.slots[index].frame[..len],
            request_cca,
        })
    }

    /// Takes the next event the caller has to act on, if any.
    pub fn next_event(&mut self) -> Option<Event> {
        self.events.pop().flatten()
    }

    /// Tells the stack a frame reached its parent, clearing the run of
    /// failures that would otherwise start a rejoin. See
    /// [`Device::transmission_failed`] for the shape a caller writes.
    pub fn transmission_delivered(&mut self) {
        self.consecutive_failures = 0;
    }

    /// Tells the stack a frame could not be delivered.
    ///
    /// A radio reports this when it exhausts its attempts. Enough of them in a
    /// row means the parent has stopped listening, and the device goes looking
    /// for another one rather than talking into an empty room.
    ///
    /// A caller that never reports delivery keeps a device talking to a parent
    /// that stopped answering, so report both outcomes of every send.
    ///
    /// ```
    /// # use zigbee::{Device, Instant};
    /// # fn send(_frame: &[u8], _request_cca: bool) -> bool { true }
    /// # fn drive(device: &mut Device, now: Instant) {
    /// while let Some(outgoing) = device.next_transmission() {
    ///     let delivered = send(outgoing.frame, outgoing.request_cca);
    ///     if delivered {
    ///         device.transmission_delivered();
    ///     } else {
    ///         device.transmission_failed(now);
    ///     }
    /// }
    /// # }
    /// ```
    pub fn transmission_failed(&mut self, now: Instant) {
        if !self.joined() {
            return;
        }
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if self.consecutive_failures < FAILURES_BEFORE_REJOIN {
            return;
        }
        self.start_rejoin(now);
    }

    /// How the radio must currently be tuned. Apply it whenever it changes.
    pub const fn radio(&self) -> RadioConfig {
        self.radio
    }

    /// Whether the device is on a network and holds the network key.
    pub const fn joined(&self) -> bool {
        matches!(self.phase, Phase::Joined)
    }

    /// The state of the On/Off application.
    pub const fn on_off(&self) -> bool {
        self.application.on
    }

    /// The brightness the Level Control cluster holds, from 0 to
    /// [`crate::MAX_LEVEL`].
    ///
    /// It is independent of [`Device::on_off`]: a light switched off keeps the
    /// level it will come back on at.
    pub const fn level(&self) -> u8 {
        self.application.level
    }

    /// The colour the light was last told to be.
    ///
    /// It is independent of [`Device::on_off`] and [`Device::level`], which
    /// say whether the light is lit and how brightly.
    pub const fn colour(&self) -> Colour {
        match self.application.colour_mode {
            zcl::COLOUR_MODE_TEMPERATURE => Colour::Temperature {
                mireds: self.application.mireds,
            },
            _ => Colour::HueSaturation {
                hue: self.application.hue,
                saturation: self.application.saturation,
            },
        }
    }

    /// Whether the coordinator asked the device to make itself recognisable.
    ///
    /// A caller is expected to do something visible while this holds, which is
    /// the whole point of the Identify cluster.
    pub const fn identifying(&self) -> bool {
        self.identifying
    }

    /// Switches the On/Off application locally, which the device then reports
    /// to the coordinator if reporting was configured.
    pub fn set_on_off(&mut self, on: bool) {
        if self.application.on == on {
            return;
        }
        self.application.on = on;
        self.application.on_off_report.pending = true;
        self.emit(Event::OnOffChanged(on));
    }

    /// Moves the brightness locally, which the device then reports to the
    /// coordinator if reporting was configured. Anything above
    /// [`crate::MAX_LEVEL`] is clamped to it, and a ramp under way is
    /// abandoned, because a local decision outranks one already in flight.
    pub fn set_level(&mut self, level: u8) {
        self.application.stop();
        let level = level.min(crate::MAX_LEVEL);
        if self.application.level == level {
            return;
        }
        self.application.level = level;
        self.application.level_report.pending = true;
        self.emit(Event::LevelChanged(level));
    }

    /// Takes the next piece of a new firmware image, if one arrived.
    ///
    /// Write it at its offset and keep going. Nothing is safe to boot until
    /// [`Event::FirmwareReady`], and an image that stops part way through
    /// arrives as [`Event::FirmwareAbandoned`] instead.
    pub fn next_firmware_block(&mut self) -> Option<FirmwareBlock<'_>> {
        if !self.firmware_block.pending() {
            return None;
        }
        let len = self.firmware_block.len;
        self.firmware_block.len = 0;
        Some(FirmwareBlock {
            offset: self.firmware_block.offset,
            data: &self.firmware_block.buffer[..len],
        })
    }

    /// Gives up on a download, which is what a caller does when it cannot write
    /// a block down. The server is told, so it stops sending.
    pub fn abandon_firmware(&mut self) {
        if !self.firmware.running() {
            return;
        }
        self.send_firmware_request(&ota::Wanted::End(ota::STATUS_ABORT));
        self.firmware.stop();
        self.firmware_block.len = 0;
        self.emit(Event::FirmwareAbandoned);
    }
}

impl Device {
    fn emit(&mut self, event: Event) {
        self.events.push(Some(event));
    }

    fn enqueue(&mut self, frame: &[u8], request_cca: bool) {
        let mut outgoing = Outgoing {
            frame: [0; mac::MAX_FRAME_LEN],
            len: frame.len() as u8,
            request_cca,
        };
        outgoing.frame[..frame.len()].copy_from_slice(frame);
        self.outbox.push(outgoing);
    }

    fn credentials(&self, counter: u32) -> Credentials {
        Credentials {
            pan_id: self.radio.pan_id,
            short_address: self.radio.short_address,
            parent: self.parent,
            channel: self.radio.channel,
            key_sequence: self.key_sequence,
            key: self.network_key.unwrap_or_default(),
            counter,
        }
    }

    fn remember(&mut self) {
        let counter = self.counter.wrapping_add(COUNTER_MARGIN);
        self.counter_persisted = counter;
        let credentials = self.credentials(counter);
        self.emit(Event::CredentialsChanged(credentials));
    }

    fn next_mac_seq(&mut self) -> u8 {
        self.mac_seq = self.mac_seq.wrapping_add(1);
        self.mac_seq
    }

    fn next_aps_counter(&mut self) -> u8 {
        self.aps_counter = self.aps_counter.wrapping_add(1);
        self.aps_counter
    }

    fn next_zdo_seq(&mut self) -> u8 {
        self.zdo_seq = self.zdo_seq.wrapping_add(1);
        self.zdo_seq
    }

    fn send_beacon_request(&mut self) {
        let mut buffer = [0u8; mac::MAX_FRAME_LEN];
        let seq = self.next_mac_seq();
        let mut out = Writer::new(&mut buffer);
        mac::beacon_request(&mut out, seq);
        let Some(frame) = out.written() else {
            return;
        };
        self.enqueue(frame, true);
    }

    fn send_association_request(&mut self, candidate: &Candidate) {
        let mut buffer = [0u8; mac::MAX_FRAME_LEN];
        let seq = self.next_mac_seq();
        let ieee = self.config.ieee;
        let mut out = Writer::new(&mut buffer);
        mac::association_request(&mut out, seq, candidate.pan_id, candidate.parent, ieee);
        let Some(frame) = out.written() else {
            return;
        };
        self.enqueue(frame, true);
    }

    fn send_data_request(&mut self, pan_id: u16, parent: mac::Addr) {
        let mut buffer = [0u8; mac::MAX_FRAME_LEN];
        let seq = self.next_mac_seq();
        let us = if self.radio.short_address == UNASSIGNED {
            mac::Addr::Extended(self.config.ieee)
        } else {
            mac::Addr::Short(self.radio.short_address)
        };
        let mut out = Writer::new(&mut buffer);
        mac::data_request(&mut out, seq, pan_id, parent, us);
        let Some(frame) = out.written() else {
            return;
        };
        self.enqueue(frame, true);
    }

    fn send_application(&mut self, nwk_dst: u16, aps_frame: &[u8]) {
        let Some(key) = self.network_key else {
            return;
        };
        let broadcast = nwk_dst >= 0xfff8;
        let mac_dst = if broadcast {
            mac::Addr::Short(0xffff)
        } else {
            mac::Addr::Short(self.parent)
        };

        let mut buffer = [0u8; mac::MAX_FRAME_LEN];
        let mac_seq = self.next_mac_seq();
        let ieee = self.config.ieee;
        let mut out = Writer::new(&mut buffer);
        mac::data(
            &mut out,
            mac_seq,
            self.radio.pan_id,
            mac_dst,
            mac::Addr::Short(self.radio.short_address),
            !broadcast,
        );

        self.nwk_seq = self.nwk_seq.wrapping_add(1);
        self.counter = self.counter.wrapping_add(1);
        nwk::build_secured(
            &mut out,
            nwk::Header {
                frame_type: nwk::FRAME_TYPE_DATA,
                dst: nwk_dst,
                src: self.radio.short_address,
                radius: nwk::DEFAULT_RADIUS,
                seq: self.nwk_seq,
                src_ieee: broadcast.then_some(ieee),
            },
            &key,
            self.key_sequence,
            self.counter,
            ieee,
            aps_frame,
        );

        let Some(frame) = out.written() else {
            return;
        };
        self.enqueue(frame, !broadcast);

        if self.counter >= self.counter_persisted {
            self.remember();
        }
    }

    fn send_aps_data(
        &mut self,
        nwk_dst: u16,
        dst_endpoint: u8,
        src_endpoint: u8,
        cluster: u16,
        profile: u16,
        payload: &[u8],
    ) {
        let mut frame = [0u8; 96];
        let counter = self.next_aps_counter();
        let mut out = Writer::new(&mut frame);
        aps::build_data(
            &mut out,
            aps::DataHeader {
                dst_endpoint,
                src_endpoint,
                cluster,
                profile,
                counter,
                ack_request: false,
                broadcast: false,
            },
            payload,
        );
        let Some(aps_frame) = out.written() else {
            return;
        };
        self.send_application(nwk_dst, aps_frame);
    }

    fn acknowledge(&mut self, nwk_dst: u16, request: &aps::Data) {
        let mut frame = [0u8; 32];
        let mut out = Writer::new(&mut frame);
        aps::build_ack(&mut out, request);
        let Some(aps_frame) = out.written() else {
            return;
        };
        self.send_application(nwk_dst, aps_frame);
    }

    fn announce(&mut self) {
        let mut payload = [0u8; 32];
        let seq = self.next_zdo_seq();
        let short = self.radio.short_address;
        let ieee = self.config.ieee;
        let mut body = Writer::new(&mut payload);
        zdo::device_announce(
            &mut body,
            seq,
            short,
            ieee,
            mac::CAPABILITY_MAINS_END_DEVICE,
        );
        let Some(announcement) = body.written() else {
            return;
        };

        let mut frame = [0u8; 64];
        let counter = self.next_aps_counter();
        let mut out = Writer::new(&mut frame);
        aps::build_data(
            &mut out,
            aps::DataHeader {
                dst_endpoint: 0,
                src_endpoint: 0,
                cluster: zdo::DEVICE_ANNCE,
                profile: aps::PROFILE_ZDO,
                counter,
                ack_request: false,
                broadcast: true,
            },
            announcement,
        );
        let Some(aps_frame) = out.written() else {
            return;
        };
        self.send_application(nwk::BROADCAST_RX_ON_WHEN_IDLE, aps_frame);
    }

    fn publish_change(&mut self, changed: zcl::Changed) {
        if changed.on_off {
            self.application.on_off_report.pending = true;
            let on = self.application.on;
            self.emit(Event::OnOffChanged(on));
        }
        if changed.level {
            self.application.level_report.pending = true;
            let level = self.application.level;
            self.emit(Event::LevelChanged(level));
        }
        if changed.colour {
            let colour = self.colour();
            self.emit(Event::ColourChanged(colour));
        }
        if changed.tables {
            let tables = Tables::of(&self.application);
            self.emit(Event::TablesChanged(tables));
        }
    }

    fn send_firmware_request(&mut self, wanted: &ota::Wanted) {
        let mut payload = [0u8; 32];
        let seq = self.next_zdo_seq();
        let mut out = Writer::new(&mut payload);
        let Some(identity) = self.config.firmware else {
            return;
        };
        self.firmware.build(&mut out, seq, wanted, &identity);
        let Some(body) = out.written() else {
            return;
        };
        let server = self.firmware.server();
        self.send_aps_data(
            server,
            COORDINATOR_ENDPOINT,
            zdo::ENDPOINT,
            ota::CLUSTER,
            aps::PROFILE_HOME_AUTOMATION,
            body,
        );
    }

    fn ask_for_firmware(&mut self, now: Instant) {
        if self.config.firmware.is_some() {
            self.firmware.ask(now);
        }
    }

    fn drive_firmware(&mut self, now: Instant) {
        let pending = self.firmware_block.pending();
        let Some(wanted) = self.firmware.due(now, pending) else {
            return;
        };
        self.send_firmware_request(&wanted);
    }

    fn report(&mut self, cluster: u16, body: &[u8]) {
        self.send_aps_data(
            nwk::COORDINATOR,
            COORDINATOR_ENDPOINT,
            zdo::ENDPOINT,
            cluster,
            aps::PROFILE_HOME_AUTOMATION,
            body,
        );
    }

    fn report_on_off(&mut self) {
        let mut payload = [0u8; 16];
        let seq = self.next_zdo_seq();
        let on = self.application.on;
        let mut body = Writer::new(&mut payload);
        zcl::report_on_off(&mut body, seq, on);
        if let Some(report) = body.written() {
            self.report(zdo::CLUSTER_ON_OFF, report);
        }
    }

    fn report_level(&mut self) {
        let mut payload = [0u8; 16];
        let seq = self.next_zdo_seq();
        let level = self.application.level;
        let mut body = Writer::new(&mut payload);
        zcl::report_level(&mut body, seq, level);
        if let Some(report) = body.written() {
            self.report(zdo::CLUSTER_LEVEL_CONTROL, report);
        }
    }

    fn honour_reporting(&mut self, now: Instant) {
        if self.application.on_off_report.due(now) {
            self.report_on_off();
            self.application.on_off_report.sent(now);
        }
        if self.application.level_report.due(now) {
            self.report_level();
            self.application.level_report.sent(now);
        }
    }

    /// Keeps the key, the network and the channel, and goes looking for a
    /// router on that network willing to take the device as a child.
    fn start_rejoin(&mut self, now: Instant) {
        self.consecutive_failures = 0;
        self.phase = Phase::Rejoining {
            give_up_at: now.plus_millis(REJOIN_TIMEOUT_MS),
            next_attempt: now,
        };
    }

    fn send_rejoin_request(&mut self, parent: mac::Addr) {
        let Some(key) = self.network_key else {
            return;
        };
        let parent_short = match parent {
            mac::Addr::Short(short) => short,
            _ => return,
        };

        let mut buffer = [0u8; mac::MAX_FRAME_LEN];
        let mac_seq = self.next_mac_seq();
        let ieee = self.config.ieee;
        let mut out = Writer::new(&mut buffer);
        mac::data(
            &mut out,
            mac_seq,
            self.radio.pan_id,
            parent,
            mac::Addr::Short(self.radio.short_address),
            true,
        );

        self.nwk_seq = self.nwk_seq.wrapping_add(1);
        self.counter = self.counter.wrapping_add(1);
        let payload = [nwk::CMD_REJOIN_REQUEST, mac::CAPABILITY_MAINS_END_DEVICE];
        nwk::build_secured(
            &mut out,
            nwk::Header {
                frame_type: nwk::FRAME_TYPE_COMMAND,
                dst: parent_short,
                src: self.radio.short_address,
                radius: 1,
                seq: self.nwk_seq,
                src_ieee: Some(ieee),
            },
            &key,
            self.key_sequence,
            self.counter,
            ieee,
            &payload,
        );

        let Some(frame) = out.written() else {
            return;
        };
        self.enqueue(frame, true);
    }

    fn on_rejoin_response(&mut self, response: nwk::RejoinResponse, source: u16, now: Instant) {
        if !matches!(self.phase, Phase::Rejoining { .. }) {
            return;
        }
        if response.status != REJOIN_ACCEPTED {
            return;
        }

        self.radio.short_address = response.short_address;
        self.parent = source;
        self.consecutive_failures = 0;
        self.next_keepalive = now.plus_millis(KEEPALIVE_MS);
        self.phase = Phase::Joined;
        self.remember();
        self.announce();
        let short_address = self.radio.short_address;
        self.ask_for_firmware(now);
        self.emit(Event::Joined { short_address });
    }

    fn restart_scan(&mut self, now: Instant) {
        self.radio.pan_id = UNASSIGNED;
        self.radio.short_address = UNASSIGNED;
        self.network_key = None;
        self.counter = 0;
        self.counter_persisted = 0;
        self.phase = Phase::Scanning { listen_until: now };
        self.emit(Event::Left);
    }
}

impl Device {
    fn on_beacon(&mut self, beacon: mac::Beacon, now: Instant) {
        if matches!(self.phase, Phase::Rejoining { .. }) {
            if beacon.pan_id == self.radio.pan_id && beacon.end_device_capacity {
                self.send_rejoin_request(beacon.source);
            }
            return;
        }
        if !matches!(self.phase, Phase::Scanning { .. }) {
            return;
        }
        if !beacon.association_permit || !beacon.end_device_capacity {
            return;
        }
        if beacon.stack_profile != STACK_PROFILE_ZIGBEE_PRO {
            return;
        }

        let candidate = Candidate {
            channel: self.radio.channel,
            pan_id: beacon.pan_id,
            parent: beacon.source,
        };
        self.radio.pan_id = candidate.pan_id;
        self.send_association_request(&candidate);
        self.phase = Phase::Associating {
            candidate,
            give_up_at: now.plus_millis(ASSOCIATION_TIMEOUT_MS),
            next_poll: now.plus_millis(POLL_INTERVAL_MS),
        };
    }

    fn on_association_response(&mut self, response: mac::AssociationResponse, now: Instant) {
        let Phase::Associating { candidate, .. } = self.phase else {
            return;
        };
        if response.status != ASSOCIATION_ACCEPTED {
            self.restart_scan(now);
            return;
        }

        self.radio.short_address = response.short_addr;
        self.radio.pan_id = candidate.pan_id;
        self.radio.channel = candidate.channel;
        self.parent = match candidate.parent {
            mac::Addr::Short(short) => short,
            _ => nwk::COORDINATOR,
        };
        self.phase = Phase::WaitingForKey {
            give_up_at: now.plus_millis(KEY_TIMEOUT_MS),
            next_poll: now,
        };
    }

    fn on_network_frame(&mut self, frame: &mut [u8], now: Instant) {
        let Some(parsed) = nwk::parse(frame) else {
            return;
        };
        let (frame_type, source, secured, header_len, broadcast) = (
            parsed.frame_type,
            parsed.src,
            parsed.secured,
            parsed.header_len,
            parsed.dst >= 0xfff8,
        );

        let payload_range = if secured {
            let Some(key) = self.key_for(nwk::key_sequence(frame, header_len)) else {
                return;
            };
            let Some(unsecured) = nwk::unsecure(frame, header_len, &key) else {
                return;
            };
            unsecured.offset..unsecured.offset + unsecured.len
        } else {
            header_len..frame.len()
        };
        let Some(payload) = frame.get_mut(payload_range) else {
            return;
        };

        if frame_type == nwk::FRAME_TYPE_COMMAND {
            match payload.first() {
                Some(&nwk::CMD_LEAVE) => self.restart_scan(now),
                Some(&nwk::CMD_SWITCH_KEY) => self.on_switch_key(payload),
                Some(&nwk::CMD_REJOIN_RESPONSE) => {
                    if let Some(response) = nwk::parse_rejoin_response(payload) {
                        self.on_rejoin_response(response, source, now);
                    }
                }
                _ => {}
            }
            return;
        }

        self.on_application_frame(source, broadcast, payload, now);
    }

    fn on_application_frame(
        &mut self,
        source: u16,
        broadcast: bool,
        frame: &mut [u8],
        now: Instant,
    ) {
        let Some(parsed) = aps::parse(frame) else {
            return;
        };

        match parsed {
            aps::Frame::Ack => {}
            aps::Frame::Command(command) => {
                let (secured, header_len) = (command.secured, command.header_len);
                let body_range = if secured {
                    let key = self.transport_key;
                    let Some(unsecured) = aps::unsecure(frame, header_len, &key) else {
                        return;
                    };
                    unsecured.offset..unsecured.offset + unsecured.len
                } else {
                    header_len..frame.len()
                };
                let Some(body) = frame.get(body_range) else {
                    return;
                };
                self.on_transport_key(body, now);
            }
            aps::Frame::Data(data) => {
                self.on_application_data(source, broadcast, &data, now);
            }
        }
    }

    /// Picks the key a secured frame was made with. A trust centre hands out
    /// the next network key before it tells anyone to use it, so for a while
    /// both are live and the sequence number in the frame says which.
    fn key_for(&self, sequence: Option<u8>) -> Option<[u8; KEY_LEN]> {
        match (sequence, self.next_network_key) {
            (Some(sequence), Some((key, held))) if sequence == held => Some(key),
            _ => self.network_key,
        }
    }

    /// A network key that arrives while already on a network is a rotation, so
    /// it is held until the trust centre says to move to it. Using it early
    /// would make the device deaf to everything still sent under the old one.
    fn on_switch_key(&mut self, body: &[u8]) {
        let Some(sequence) = nwk::parse_switch_key(body) else {
            return;
        };
        let Some((key, held)) = self.next_network_key else {
            return;
        };
        if held != sequence {
            return;
        }

        self.network_key = Some(key);
        self.key_sequence = sequence;
        self.next_network_key = None;
        self.counter = 0;
        self.counter_persisted = 0;
        self.remember();
    }

    fn on_transport_key(&mut self, body: &[u8], now: Instant) {
        let Some(transport) = aps::parse_transport_key(body) else {
            return;
        };
        if transport.key_type != aps::KEY_TYPE_STANDARD_NETWORK {
            return;
        }
        if transport.destination != self.config.ieee && transport.destination != 0 {
            return;
        }
        if self.joined() {
            self.next_network_key = Some((transport.key, transport.key_sequence));
            return;
        }

        self.network_key = Some(transport.key);
        self.key_sequence = transport.key_sequence;
        self.phase = Phase::Joined;
        self.remember();
        self.announce();
        let short_address = self.radio.short_address;
        self.ask_for_firmware(now);
        self.emit(Event::Joined { short_address });
    }

    fn on_application_data(
        &mut self,
        source: u16,
        broadcast: bool,
        request: &aps::Data,
        now: Instant,
    ) {
        if let Some(group) = request.group {
            if self.application.in_group(group) {
                self.on_cluster_request(source, false, request, now);
            }
            return;
        }

        if request.ack_request && !broadcast {
            self.acknowledge(source, request);
        }

        match request.profile {
            aps::PROFILE_ZDO => {
                let mut reply = [0u8; 96];
                let mut out = Writer::new(&mut reply);
                let Some(response) = zdo::respond(
                    &mut out,
                    request.cluster,
                    request.payload,
                    self.radio.short_address,
                    self.config.ieee,
                    mac::CAPABILITY_MAINS_END_DEVICE,
                    broadcast,
                ) else {
                    return;
                };
                let Some(body) = out.written() else {
                    return;
                };
                self.send_aps_data(source, 0, 0, response.cluster, aps::PROFILE_ZDO, body);
            }
            aps::PROFILE_HOME_AUTOMATION if !broadcast => {
                self.on_cluster_request(source, true, request, now);
            }
            aps::PROFILE_HOME_AUTOMATION if request.cluster == ota::CLUSTER => {
                self.on_cluster_request(source, false, request, now);
            }
            _ => {}
        }
    }

    /// Runs one cluster command. A group frame reaches every member at once, so
    /// nobody answers it.
    fn on_cluster_request(&mut self, source: u16, answer: bool, request: &aps::Data, now: Instant) {
        if request.cluster == ota::CLUSTER {
            self.on_firmware_frame(source, request.payload, now);
            return;
        }

        let mut reply = [0u8; 96];
        let mut out = Writer::new(&mut reply);
        let identity = zcl::Identity {
            manufacturer: self.config.manufacturer,
            model: self.config.model,
            software_build: self.config.software_build,
        };
        let outcome = zcl::handle(
            &mut out,
            request.cluster,
            request.payload,
            &mut self.application,
            &identity,
            now,
        );

        if let (true, true, Some(body)) = (answer, outcome.has_reply, out.written()) {
            self.send_aps_data(
                source,
                request.src_endpoint,
                zdo::ENDPOINT,
                request.cluster,
                aps::PROFILE_HOME_AUTOMATION,
                body,
            );
        }
        self.publish_change(outcome.changed);
    }

    fn on_firmware_frame(&mut self, source: u16, payload: &[u8], now: Instant) {
        let Some(incoming) = zcl::parse(payload) else {
            return;
        };
        match self
            .firmware
            .receive(source, &incoming, &mut self.firmware_block, now)
        {
            ota::Outcome::Nothing => {}
            ota::Outcome::Offered { version, size } => {
                self.emit(Event::FirmwareOffered { version, size })
            }
            ota::Outcome::Ready => self.emit(Event::FirmwareReady),
            ota::Outcome::Abandoned => self.emit(Event::FirmwareAbandoned),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_never_carries_the_network_key() {
        let key = [0xa5u8; KEY_LEN];
        let credentials = Credentials {
            pan_id: 0xa269,
            short_address: 0x4560,
            parent: 0x0000,
            channel: 15,
            key_sequence: 0,
            key,
            counter: 7,
        };

        let rendered = std::format!("{credentials:?}");
        assert!(rendered.contains("Credentials"));
        assert!(rendered.contains(&std::format!("{}", 0xa269)));
        assert!(!rendered.contains("key"));
        assert!(!rendered.contains(&std::format!("{key:?}")));
    }

    fn rejoining() -> Device {
        let mut device = Device::new(Config::new(0x0011_2233_4455_6677));
        device.network_key = Some([0x11; KEY_LEN]);
        device.radio.pan_id = 0xa269;
        device.radio.short_address = 0x4560;
        device.radio.channel = 20;
        device.parent = 0x1234;
        device.phase = Phase::Joined;
        device.start_rejoin(Instant::from_millis(0));
        device
    }

    #[test]
    fn an_accepted_rejoin_adopts_the_new_address_and_parent() {
        let mut device = rejoining();

        device.on_rejoin_response(
            nwk::RejoinResponse {
                short_address: 0x7ace,
                status: REJOIN_ACCEPTED,
            },
            0x5678,
            Instant::from_millis(100),
        );

        assert!(device.joined());
        assert_eq!(device.radio().short_address, 0x7ace);
        assert_eq!(device.parent, 0x5678);
    }

    #[test]
    fn a_refused_rejoin_keeps_looking() {
        let mut device = rejoining();

        device.on_rejoin_response(
            nwk::RejoinResponse {
                short_address: 0x7ace,
                status: 0x01,
            },
            0x5678,
            Instant::from_millis(100),
        );

        assert!(!device.joined());
        assert_eq!(device.radio().short_address, 0x4560);
    }

    #[test]
    fn an_accepted_rejoin_offers_credentials_worth_keeping() {
        let mut device = rejoining();

        device.on_rejoin_response(
            nwk::RejoinResponse {
                short_address: 0x7ace,
                status: REJOIN_ACCEPTED,
            },
            0x5678,
            Instant::from_millis(100),
        );

        let mut saved = None;
        while let Some(event) = device.next_event() {
            if let Event::CredentialsChanged(credentials) = event {
                saved = Some(credentials);
            }
        }
        let saved = saved.expect("a new parent has to survive a reboot");
        assert_eq!(saved.short_address(), 0x7ace);
        assert_eq!(saved.channel(), 20);
    }

    #[test]
    fn a_rejoin_response_outside_a_rejoin_is_ignored() {
        let mut device = Device::new(Config::new(1));

        device.on_rejoin_response(
            nwk::RejoinResponse {
                short_address: 0x7ace,
                status: REJOIN_ACCEPTED,
            },
            0x5678,
            Instant::from_millis(0),
        );

        assert!(!device.joined());
    }
}
