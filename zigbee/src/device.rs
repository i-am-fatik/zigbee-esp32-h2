use crate::buf::Writer;
use crate::crypto::{key_transport_key, KEY_LEN};
use crate::{aps, mac, nwk, zcl, zdo, Instant, CHANNELS};

/// The link key every Zigbee device is allowed to fall back to when it joins a
/// centralised network without an install code.
const DEFAULT_TRUST_CENTRE_LINK_KEY: [u8; KEY_LEN] = *b"ZigBeeAlliance09";

const ASSOCIATION_ACCEPTED: u8 = 0x00;
const STACK_PROFILE_ZIGBEE_PRO: u8 = 2;

const SCAN_DWELL_MS: u32 = 250;
const POLL_INTERVAL_MS: u32 = 300;
const ASSOCIATION_TIMEOUT_MS: u32 = 6_000;
const KEY_TIMEOUT_MS: u32 = 12_000;

/// How far ahead of the live counter a stored counter runs, so a power cut can
/// never replay a frame counter the coordinator has already accepted.
const COUNTER_MARGIN: u32 = 1024;

const UNASSIGNED: u16 = 0xffff;
const FRAME_CAPACITY: usize = 127;
const OUTBOX_CAPACITY: usize = 4;
const EVENT_CAPACITY: usize = 4;

/// What the device tells the world about itself during the interview.
#[derive(Clone, Copy)]
pub struct Config {
    ieee: u64,
    manufacturer: &'static str,
    model: &'static str,
    software_build: &'static str,
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
    pub const SIZE: usize = 32;

    const MAGIC: u32 = 0x5a42_4832;

    /// Encodes the credentials for storage.
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut record = [0u8; Self::SIZE];
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
    pub fn from_bytes(record: &[u8; Self::SIZE]) -> Option<Self> {
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

/// How the radio must be tuned and addressed for the stack to hear its traffic.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RadioConfig {
    /// The IEEE 802.15.4 channel to listen on.
    pub channel: u8,
    /// The PAN the radio should accept frames for.
    pub pan_id: u16,
    /// The short address the radio should accept frames for.
    pub short_address: u16,
}

/// A frame the caller should put on the air.
pub struct Transmission<'a> {
    /// The MAC frame, without the physical length byte and without the checksum.
    pub frame: &'a [u8],
    /// Whether to assess the channel before transmitting.
    pub request_cca: bool,
}

/// Something happened that the caller may want to act on.
#[derive(Clone, Copy)]
#[non_exhaustive]
pub enum Event {
    /// The device is on a network and has the network key.
    Joined {
        /// The short address the coordinator allocated.
        short_address: u16,
    },
    /// The device left, or was told to leave, and is scanning again.
    Left,
    /// The coordinator switched the On/Off application.
    OnOffChanged(bool),
    /// Credentials worth writing to storage, superseding any earlier ones.
    CredentialsChanged(Credentials),
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
    frame: [u8; FRAME_CAPACITY],
    len: u8,
    request_cca: bool,
}

/// A Zigbee end device: an On/Off application that joins a network, answers the
/// coordinator's interview, and reports what it was told to do.
pub struct Device {
    config: Config,
    radio: RadioConfig,
    parent: u16,
    network_key: Option<[u8; KEY_LEN]>,
    key_sequence: u8,
    phase: Phase,
    application: zcl::State,
    transport_key: [u8; KEY_LEN],
    counter: u32,
    counter_persisted: u32,
    mac_seq: u8,
    nwk_seq: u8,
    aps_counter: u8,
    zdo_seq: u8,
    report_pending: bool,
    last_report_at: Instant,
    identifying: bool,
    outbox: Queue<Outgoing, OUTBOX_CAPACITY>,
    events: Queue<Option<Event>, EVENT_CAPACITY>,
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
            phase: Phase::Scanning {
                listen_until: Instant::from_millis(0),
            },
            application: zcl::State::default(),
            transport_key: key_transport_key(&DEFAULT_TRUST_CENTRE_LINK_KEY),
            counter: 0,
            counter_persisted: 0,
            mac_seq: 0,
            nwk_seq: 0,
            aps_counter: 0,
            zdo_seq: 0,
            report_pending: false,
            last_report_at: Instant::from_millis(0),
            identifying: false,
            outbox: Queue::new(Outgoing {
                frame: [0; FRAME_CAPACITY],
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
        self.report_pending = true;
        self.emit(Event::OnOffChanged(on));
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

    /// Takes the next event, if any.
    pub fn next_event(&mut self) -> Option<Event> {
        self.events.pop().flatten()
    }

    /// Hands the stack a MAC frame the radio received, without the physical
    /// length byte and without the checksum.
    pub fn receive(&mut self, frame: &[u8], now: Instant) {
        let Some(parsed) = mac::parse(frame) else {
            return;
        };
        let frame_type = parsed.frame_type;
        let payload_start = frame.len() - parsed.payload.len();

        match frame_type {
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
                let mut network = [0u8; 128];
                let len = frame.len() - payload_start;
                network[..len].copy_from_slice(&frame[payload_start..]);
                self.on_network_frame(&mut network, len, now);
            }
            _ => {}
        }
    }

    /// Lets time pass: drives scanning, association, polling and reporting.
    pub fn tick(&mut self, now: Instant) {
        self.identifying = self.application.identify_remaining(now) > 0;

        if self.joined() {
            self.honour_reporting(now);
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
            Phase::Joined => {}
        }
    }
}

impl Device {
    const fn after(now: Instant, millis: u32) -> Instant {
        now.plus_millis(millis)
    }

    fn emit(&mut self, event: Event) {
        self.events.push(Some(event));
    }

    fn enqueue(&mut self, frame: &[u8], request_cca: bool) {
        let mut outgoing = Outgoing {
            frame: [0; FRAME_CAPACITY],
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
        let mut buffer = [0u8; FRAME_CAPACITY];
        let seq = self.next_mac_seq();
        let mut out = Writer::new(&mut buffer);
        mac::beacon_request(&mut out, seq);
        let len = out.len();
        self.enqueue(&buffer[..len], true);
    }

    fn send_association_request(&mut self, candidate: &Candidate) {
        let mut buffer = [0u8; FRAME_CAPACITY];
        let seq = self.next_mac_seq();
        let ieee = self.config.ieee;
        let mut out = Writer::new(&mut buffer);
        mac::association_request(&mut out, seq, candidate.pan_id, candidate.parent, ieee);
        let len = out.len();
        self.enqueue(&buffer[..len], true);
    }

    fn send_data_request(&mut self, pan_id: u16, parent: mac::Addr) {
        let mut buffer = [0u8; FRAME_CAPACITY];
        let seq = self.next_mac_seq();
        let us = if self.radio.short_address == UNASSIGNED {
            mac::Addr::Extended(self.config.ieee)
        } else {
            mac::Addr::Short(self.radio.short_address)
        };
        let mut out = Writer::new(&mut buffer);
        mac::data_request(&mut out, seq, pan_id, parent, us);
        let len = out.len();
        self.enqueue(&buffer[..len], true);
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

        let mut buffer = [0u8; FRAME_CAPACITY];
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

        let len = out.len();
        self.enqueue(&buffer[..len], !broadcast);

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
        let len = out.len();
        self.send_application(nwk_dst, &frame[..len]);
    }

    fn acknowledge(&mut self, nwk_dst: u16, request: &aps::Data) {
        let mut frame = [0u8; 32];
        let mut out = Writer::new(&mut frame);
        aps::build_ack(&mut out, request);
        let len = out.len();
        self.send_application(nwk_dst, &frame[..len]);
    }

    fn announce(&mut self) {
        let mut payload = [0u8; 32];
        let seq = self.next_zdo_seq();
        let short = self.radio.short_address;
        let ieee = self.config.ieee;
        let mut body = Writer::new(&mut payload);
        zdo::device_announce(&mut body, seq, short, ieee, mac::CAPABILITY_MAINS_END_DEVICE);
        let body_len = body.len();

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
            &payload[..body_len],
        );
        let len = out.len();
        self.send_application(nwk::BROADCAST_RX_ON_WHEN_IDLE, &frame[..len]);
    }

    fn report_on_off(&mut self) {
        let mut payload = [0u8; 16];
        let seq = self.next_zdo_seq();
        let on = self.application.on;
        let mut body = Writer::new(&mut payload);
        zcl::report_on_off(&mut body, seq, on);
        let body_len = body.len();
        self.send_aps_data(
            nwk::COORDINATOR,
            1,
            zdo::ENDPOINT,
            zdo::CLUSTER_ON_OFF,
            aps::PROFILE_HOME_AUTOMATION,
            &payload[..body_len],
        );
    }

    fn honour_reporting(&mut self, now: Instant) {
        let Some(reporting) = self.application.on_off_reporting else {
            return;
        };
        let quiet_for = now.millis_since(self.last_report_at);
        let may_report = quiet_for >= reporting.min_interval as u32 * 1000;
        let periodic =
            reporting.max_interval != 0 && reporting.max_interval != zcl::INTERVAL_NEVER;
        let must_report = periodic && quiet_for >= reporting.max_interval as u32 * 1000;

        let due = must_report || (self.report_pending && may_report);
        if !due {
            return;
        }
        self.report_on_off();
        self.report_pending = false;
        self.last_report_at = now;
    }

    fn restart_scan(&mut self, now: Instant) {
        self.radio.pan_id = UNASSIGNED;
        self.radio.short_address = UNASSIGNED;
        self.network_key = None;
        self.counter = 0;
        self.counter_persisted = 0;
        self.phase = Phase::Scanning {
            listen_until: now,
        };
        self.emit(Event::Left);
    }
}

impl Device {
    fn on_beacon(&mut self, beacon: mac::Beacon, now: Instant) {
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
            give_up_at: Self::after(now, ASSOCIATION_TIMEOUT_MS),
            next_poll: Self::after(now, POLL_INTERVAL_MS),
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
            give_up_at: Self::after(now, KEY_TIMEOUT_MS),
            next_poll: now,
        };
    }

    fn on_network_frame(&mut self, frame: &mut [u8; 128], len: usize, now: Instant) {
        let Some(parsed) = nwk::parse(&frame[..len]) else {
            return;
        };
        let (frame_type, source, secured, header_len, broadcast) = (
            parsed.frame_type,
            parsed.src,
            parsed.secured,
            parsed.header_len,
            parsed.dst >= 0xfff8,
        );

        let (offset, payload_len) = if secured {
            let Some(key) = self.network_key else {
                return;
            };
            let Some(unsecured) = nwk::unsecure(&mut frame[..len], header_len, &key) else {
                return;
            };
            (unsecured.offset, unsecured.len)
        } else {
            (header_len, len - header_len)
        };

        if frame_type == nwk::FRAME_TYPE_COMMAND {
            if frame[offset] == nwk::CMD_LEAVE {
                self.restart_scan(now);
            }
            return;
        }

        let mut application = [0u8; 128];
        application[..payload_len].copy_from_slice(&frame[offset..offset + payload_len]);
        self.on_application_frame(source, broadcast, &mut application, payload_len, now);
    }

    fn on_application_frame(
        &mut self,
        source: u16,
        broadcast: bool,
        frame: &mut [u8; 128],
        len: usize,
        now: Instant,
    ) {
        let Some(parsed) = aps::parse(&frame[..len]) else {
            return;
        };

        match parsed {
            aps::Frame::Ack => {}
            aps::Frame::Command(command) => {
                let (secured, header_len) = (command.secured, command.header_len);
                let body_range = if secured {
                    let key = self.transport_key;
                    let Some(unsecured) = aps::unsecure(&mut frame[..len], header_len, &key) else {
                        return;
                    };
                    unsecured.offset..unsecured.offset + unsecured.len
                } else {
                    header_len..len
                };
                let mut body = [0u8; 64];
                let body_len = body_range.len();
                body[..body_len].copy_from_slice(&frame[body_range]);
                self.on_transport_key(&body[..body_len]);
            }
            aps::Frame::Data(data) => {
                let mut payload = [0u8; 96];
                let payload_len = data.payload.len();
                payload[..payload_len].copy_from_slice(data.payload);
                let request = aps::Data {
                    payload: &[],
                    ..data
                };
                self.on_application_data(source, broadcast, &request, &payload[..payload_len], now);
            }
        }
    }

    fn on_transport_key(&mut self, body: &[u8]) {
        let Some(transport) = aps::parse_transport_key(body) else {
            return;
        };
        if transport.key_type != aps::KEY_TYPE_STANDARD_NETWORK {
            return;
        }
        if transport.destination != self.config.ieee && transport.destination != 0 {
            return;
        }

        self.network_key = Some(transport.key);
        self.key_sequence = transport.key_seq;
        self.phase = Phase::Joined;
        self.remember();
        self.announce();
        let short_address = self.radio.short_address;
        self.emit(Event::Joined { short_address });
    }

    fn on_application_data(
        &mut self,
        source: u16,
        broadcast: bool,
        request: &aps::Data,
        payload: &[u8],
        now: Instant,
    ) {
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
                    payload,
                    self.radio.short_address,
                    self.config.ieee,
                    mac::CAPABILITY_MAINS_END_DEVICE,
                    broadcast,
                ) else {
                    return;
                };
                let len = out.len();
                self.send_aps_data(
                    source,
                    0,
                    0,
                    response.cluster,
                    aps::PROFILE_ZDO,
                    &reply[..len],
                );
            }
            aps::PROFILE_HOME_AUTOMATION if !broadcast => {
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
                    payload,
                    &mut self.application,
                    &identity,
                    now,
                );
                let len = out.len();

                if outcome.has_reply {
                    self.send_aps_data(
                        source,
                        request.src_endpoint,
                        zdo::ENDPOINT,
                        request.cluster,
                        aps::PROFILE_HOME_AUTOMATION,
                        &reply[..len],
                    );
                }
                if outcome.state_changed {
                    self.report_pending = true;
                    let on = self.application.on;
                    self.emit(Event::OnOffChanged(on));
                }
            }
            _ => {}
        }
    }
}
