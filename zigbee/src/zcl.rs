use crate::buf::{Reader, Writer};
use crate::Instant;

pub const CMD_READ_ATTRIBUTES: u8 = 0x00;
pub const CMD_READ_ATTRIBUTES_RESPONSE: u8 = 0x01;
pub const CMD_WRITE_ATTRIBUTES: u8 = 0x02;
pub const CMD_WRITE_ATTRIBUTES_RESPONSE: u8 = 0x04;
pub const CMD_CONFIGURE_REPORTING: u8 = 0x06;
pub const CMD_CONFIGURE_REPORTING_RESPONSE: u8 = 0x07;
pub const CMD_REPORT_ATTRIBUTES: u8 = 0x0a;
pub const CMD_DEFAULT_RESPONSE: u8 = 0x0b;
pub const CMD_DISCOVER_ATTRIBUTES: u8 = 0x0c;
pub const CMD_DISCOVER_ATTRIBUTES_RESPONSE: u8 = 0x0d;

pub const IDENTIFY: u8 = 0x00;
pub const IDENTIFY_QUERY: u8 = 0x01;
pub const IDENTIFY_QUERY_RESPONSE: u8 = 0x00;

pub const ATTR_IDENTIFY_TIME: u16 = 0x0000;

pub const ON_OFF_OFF: u8 = 0x00;
pub const ON_OFF_ON: u8 = 0x01;
pub const ON_OFF_TOGGLE: u8 = 0x02;

pub const ATTR_ON_OFF: u16 = 0x0000;

pub const LEVEL_MOVE_TO_LEVEL: u8 = 0x00;
pub const LEVEL_MOVE: u8 = 0x01;
pub const LEVEL_STEP: u8 = 0x02;
pub const LEVEL_STOP: u8 = 0x03;
pub const LEVEL_MOVE_TO_LEVEL_WITH_ON_OFF: u8 = 0x04;
pub const LEVEL_MOVE_WITH_ON_OFF: u8 = 0x05;
pub const LEVEL_STEP_WITH_ON_OFF: u8 = 0x06;
pub const LEVEL_STOP_WITH_ON_OFF: u8 = 0x07;

const DIRECTION_UP: u8 = 0x00;
const DIRECTION_DOWN: u8 = 0x01;

/// The rate a coordinator sends when it wants the light to move as fast as it
/// can rather than at a stated number of units per second.
const RATE_UNSTATED: u8 = 0xff;

pub const ATTR_CURRENT_LEVEL: u16 = 0x0000;

/// The brightest a Level Control light goes. 0xff is reserved for "undefined",
/// so the usable range stops one short of it.
pub const MAX_LEVEL: u8 = 0xfe;

const TYPE_BOOL: u8 = 0x10;
const TYPE_UINT8: u8 = 0x20;
const TYPE_UINT16: u8 = 0x21;
const TYPE_ENUM8: u8 = 0x30;
const TYPE_STRING: u8 = 0x42;

const STATUS_SUCCESS: u8 = 0x00;
const STATUS_UNSUPPORTED_ATTRIBUTE: u8 = 0x86;
const STATUS_INVALID_FIELD: u8 = 0x85;
const STATUS_UNSUP_CLUSTER_COMMAND: u8 = 0x81;

const DIRECTION_REPORT: u8 = 0x00;

/// The coordinator disables periodic reporting by asking for the longest
/// possible interval, leaving only change-driven reports.
pub const INTERVAL_NEVER: u16 = 0xffff;

const FC_CLUSTER_SPECIFIC: u8 = 0x01;
const FC_MANUFACTURER_SPECIFIC: u8 = 0x04;
const FC_FROM_SERVER: u8 = 0x08;
const FC_DISABLE_DEFAULT_RESPONSE: u8 = 0x10;

/// The strings the Basic cluster reports about the hardware.
pub struct Identity<'a> {
    pub manufacturer: &'a str,
    pub model: &'a str,
    pub software_build: &'a str,
}

pub struct Incoming<'a> {
    pub cluster_specific: bool,
    pub from_server: bool,
    pub disable_default_response: bool,
    pub seq: u8,
    pub command: u8,
    pub payload: &'a [u8],
}

pub fn parse(input: &[u8]) -> Option<Incoming<'_>> {
    let mut r = Reader::new(input);
    let fc = r.u8()?;
    if fc & FC_MANUFACTURER_SPECIFIC != 0 {
        r.skip(2)?;
    }
    let seq = r.u8()?;
    let command = r.u8()?;
    Some(Incoming {
        cluster_specific: fc & FC_CLUSTER_SPECIFIC != 0,
        from_server: fc & FC_FROM_SERVER != 0,
        disable_default_response: fc & FC_DISABLE_DEFAULT_RESPONSE != 0,
        seq,
        command,
        payload: r.rest(),
    })
}

fn header(out: &mut Writer, cluster_specific: bool, seq: u8, command: u8) {
    let mut fc = FC_FROM_SERVER | FC_DISABLE_DEFAULT_RESPONSE;
    if cluster_specific {
        fc |= FC_CLUSTER_SPECIFIC;
    }
    out.u8(fc);
    out.u8(seq);
    out.u8(command);
}

fn string(out: &mut Writer, value: &str) {
    out.u8(TYPE_STRING);
    out.u8(value.len() as u8);
    out.bytes(value.as_bytes());
}

fn write_attribute(
    out: &mut Writer,
    cluster: u16,
    attribute: u16,
    state: &State,
    identity: &Identity,
    now: Instant,
) -> bool {
    match (cluster, attribute) {
        (super::zdo::CLUSTER_BASIC, 0x0000) => {
            out.u8(TYPE_UINT8).u8(3);
        }
        (super::zdo::CLUSTER_BASIC, 0x0001) => {
            out.u8(TYPE_UINT8).u8(1);
        }
        (super::zdo::CLUSTER_BASIC, 0x0002) => {
            out.u8(TYPE_UINT8).u8(2);
        }
        (super::zdo::CLUSTER_BASIC, 0x0003) => {
            out.u8(TYPE_UINT8).u8(1);
        }
        (super::zdo::CLUSTER_BASIC, 0x0004) => string(out, identity.manufacturer),
        (super::zdo::CLUSTER_BASIC, 0x0005) => string(out, identity.model),
        (super::zdo::CLUSTER_BASIC, 0x0007) => {
            out.u8(TYPE_ENUM8).u8(0x01);
        }
        (super::zdo::CLUSTER_BASIC, 0x4000) => string(out, identity.software_build),
        (super::zdo::CLUSTER_IDENTIFY, ATTR_IDENTIFY_TIME) => {
            out.u8(TYPE_UINT16).u16(state.identify_remaining(now));
        }
        (super::zdo::CLUSTER_ON_OFF, ATTR_ON_OFF) => {
            out.u8(TYPE_BOOL).u8(state.on as u8);
        }
        (super::zdo::CLUSTER_LEVEL_CONTROL, ATTR_CURRENT_LEVEL) => {
            out.u8(TYPE_UINT8).u8(state.level);
        }
        _ => return false,
    }
    true
}

/// What the coordinator asked to be told about the On/Off attribute: how soon
/// after a change it may hear, and how long it will wait without one.
#[derive(Clone, Copy)]
pub struct Reporting {
    pub min_interval: u16,
    pub max_interval: u16,
}

#[derive(Clone, Copy)]
pub struct Reportable {
    pub reporting: Option<Reporting>,
    pub pending: bool,
    pub last_at: Instant,
}

impl Default for Reportable {
    fn default() -> Self {
        Self {
            reporting: None,
            pending: false,
            last_at: Instant::from_millis(0),
        }
    }
}

impl Reportable {
    pub fn due(&self, now: Instant) -> bool {
        let Some(reporting) = self.reporting else {
            return false;
        };
        let quiet_for = now.millis_since(self.last_at);
        let may = quiet_for >= reporting.min_interval as u32 * 1000;
        let periodic = reporting.max_interval != 0 && reporting.max_interval != INTERVAL_NEVER;
        let must = periodic && quiet_for >= reporting.max_interval as u32 * 1000;
        must || (self.pending && may)
    }

    pub fn sent(&mut self, now: Instant) {
        self.pending = false;
        self.last_at = now;
    }
}

#[derive(Clone, Copy)]
pub struct Changed {
    pub on_off: bool,
    pub level: bool,
}

impl Changed {
    pub const NONE: Self = Self {
        on_off: false,
        level: false,
    };
}

/// A brightness move already under way, held as its starting point rather than
/// its current one so repeated ticks cannot accumulate rounding error.
#[derive(Clone, Copy)]
struct Ramp {
    up: bool,
    rate: u8,
    with_on_off: bool,
    started_at: Instant,
    from: u8,
}

pub struct State {
    pub on: bool,
    pub level: u8,
    pub identify_until: Option<Instant>,
    pub on_off_report: Reportable,
    pub level_report: Reportable,
    ramp: Option<Ramp>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            on: false,
            level: MAX_LEVEL,
            identify_until: None,
            on_off_report: Reportable::default(),
            level_report: Reportable::default(),
            ramp: None,
        }
    }
}

impl State {
    /// Seconds of identifying left, which is what the attribute reports.
    pub fn identify_remaining(&self, now: Instant) -> u16 {
        let Some(until) = self.identify_until else {
            return 0;
        };
        if now.reached(until) {
            return 0;
        }
        (until.millis_since(now) / 1000) as u16
    }

    fn identify_for(&mut self, seconds: u16, now: Instant) {
        self.identify_until = (seconds > 0).then(|| now.plus_millis(seconds as u32 * 1000));
    }

    /// Lands on a brightness, and with the on-off variant lets a level of zero
    /// switch the light off and any other level switch it on.
    fn settle(&mut self, level: u8, with_on_off: bool) -> Changed {
        let level = level.min(MAX_LEVEL);
        let mut changed = Changed {
            on_off: false,
            level: self.level != level,
        };
        self.level = level;

        if with_on_off {
            let on = level > 0;
            changed.on_off = self.on != on;
            self.on = on;
        }
        changed
    }

    fn move_to_level(&mut self, level: u8, with_on_off: bool) -> Changed {
        self.stop();
        self.settle(level, with_on_off)
    }

    fn step(&mut self, up: bool, size: u8, with_on_off: bool) -> Changed {
        self.stop();
        let level = if up {
            self.level.saturating_add(size)
        } else {
            self.level.saturating_sub(size)
        };
        self.settle(level, with_on_off)
    }

    fn start_ramp(&mut self, up: bool, rate: u8, with_on_off: bool, now: Instant) {
        self.ramp = Some(Ramp {
            up,
            rate: if rate == RATE_UNSTATED { MAX_LEVEL } else { rate },
            with_on_off,
            started_at: now,
            from: self.level,
        });
    }

    /// Abandons any move under way, leaving the brightness where it reached.
    pub fn stop(&mut self) {
        self.ramp = None;
    }

    /// Carries a move under way forward to where it should be by now, and ends
    /// it once the brightness runs into either end of the range.
    pub fn advance(&mut self, now: Instant) -> Changed {
        let Some(ramp) = self.ramp else {
            return Changed::NONE;
        };

        let elapsed = now.millis_since(ramp.started_at) as u64;
        let travelled = (ramp.rate as u64 * elapsed / 1000).min(u8::MAX as u64) as u8;
        let level = if ramp.up {
            ramp.from.saturating_add(travelled).min(MAX_LEVEL)
        } else {
            ramp.from.saturating_sub(travelled)
        };

        if level == 0 || level == MAX_LEVEL {
            self.stop();
        }
        self.settle(level, ramp.with_on_off)
    }
}

pub struct Outcome {
    pub has_reply: bool,
    pub changed: Changed,
}

const NOTHING: Outcome = Outcome {
    has_reply: false,
    changed: Changed::NONE,
};

pub fn handle(
    out: &mut Writer,
    cluster: u16,
    input: &[u8],
    state: &mut State,
    identity: &Identity,
    now: Instant,
) -> Outcome {
    let Some(request) = parse(input) else {
        return NOTHING;
    };
    if request.from_server {
        return NOTHING;
    }

    if request.cluster_specific {
        return handle_cluster_command(out, cluster, &request, state, now);
    }

    match request.command {
        CMD_READ_ATTRIBUTES => {
            header(out, false, request.seq, CMD_READ_ATTRIBUTES_RESPONSE);
            let mut r = Reader::new(request.payload);
            while let Some(attribute) = r.u16() {
                out.u16(attribute);
                let status_at = out.len();
                out.u8(STATUS_SUCCESS);
                if !write_attribute(out, cluster, attribute, state, identity, now) {
                    out.set(status_at, STATUS_UNSUPPORTED_ATTRIBUTE);
                }
            }
            Outcome {
                has_reply: true,
                changed: Changed::NONE,
            }
        }
        CMD_CONFIGURE_REPORTING => {
            header(out, false, request.seq, CMD_CONFIGURE_REPORTING_RESPONSE);
            configure_reporting(out, cluster, request.payload, state);
            Outcome {
                has_reply: true,
                changed: Changed::NONE,
            }
        }
        CMD_WRITE_ATTRIBUTES => {
            header(out, false, request.seq, CMD_WRITE_ATTRIBUTES_RESPONSE);
            write_attributes(out, cluster, request.payload, state, now);
            Outcome {
                has_reply: true,
                changed: Changed::NONE,
            }
        }
        CMD_DISCOVER_ATTRIBUTES => {
            header(out, false, request.seq, CMD_DISCOVER_ATTRIBUTES_RESPONSE);
            out.u8(0x01);
            Outcome {
                has_reply: true,
                changed: Changed::NONE,
            }
        }
        CMD_DEFAULT_RESPONSE => NOTHING,
        _ => {
            default_response(out, request.seq, request.command, STATUS_UNSUP_CLUSTER_COMMAND);
            Outcome {
                has_reply: true,
                changed: Changed::NONE,
            }
        }
    }
}

/// The width of the reportable change field, which analog attributes carry and
/// discrete ones omit. An unrecognised type ends the parse, because without the
/// width the rest of the records cannot be located.
fn reportable_change_width(data_type: u8) -> Option<usize> {
    match data_type {
        TYPE_BOOL | TYPE_ENUM8 => Some(0),
        TYPE_UINT8 => Some(1),
        TYPE_UINT16 => Some(2),
        _ => None,
    }
}

fn configure_reporting(out: &mut Writer, cluster: u16, payload: &[u8], state: &mut State) {
    let accepted_at = out.len();
    out.u8(STATUS_SUCCESS);
    let mut refused = false;

    let mut r = Reader::new(payload);
    while let Some(direction) = r.u8() {
        let Some(attribute) = r.u16() else { break };
        if direction != DIRECTION_REPORT {
            r.u16();
            continue;
        }
        let (Some(data_type), Some(min_interval), Some(max_interval)) =
            (r.u8(), r.u16(), r.u16())
        else {
            break;
        };
        let Some(change_width) = reportable_change_width(data_type) else {
            break;
        };
        r.skip(change_width);

        let wanted = Reporting {
            min_interval,
            max_interval,
        };
        if cluster == super::zdo::CLUSTER_ON_OFF && attribute == ATTR_ON_OFF {
            state.on_off_report.reporting = Some(wanted);
            continue;
        }
        if cluster == super::zdo::CLUSTER_LEVEL_CONTROL && attribute == ATTR_CURRENT_LEVEL {
            state.level_report.reporting = Some(wanted);
            continue;
        }

        if !refused {
            refused = true;
            out.truncate(accepted_at);
        }
        out.u8(STATUS_UNSUPPORTED_ATTRIBUTE);
        out.u8(direction);
        out.u16(attribute);
    }
}

fn write_attributes(
    out: &mut Writer,
    cluster: u16,
    payload: &[u8],
    state: &mut State,
    now: Instant,
) {
    let accepted_at = out.len();
    out.u8(STATUS_SUCCESS);

    let mut r = Reader::new(payload);
    while let Some(attribute) = r.u16() {
        let Some(data_type) = r.u8() else { break };
        let writable =
            cluster == super::zdo::CLUSTER_IDENTIFY && attribute == ATTR_IDENTIFY_TIME;
        if writable && data_type == TYPE_UINT16 {
            let Some(seconds) = r.u16() else { break };
            state.identify_for(seconds, now);
            continue;
        }
        out.truncate(accepted_at);
        out.u8(STATUS_UNSUPPORTED_ATTRIBUTE);
        out.u16(attribute);
        break;
    }
}

fn handle_cluster_command(
    out: &mut Writer,
    cluster: u16,
    request: &Incoming,
    state: &mut State,
    now: Instant,
) -> Outcome {
    if cluster == super::zdo::CLUSTER_IDENTIFY && request.command == IDENTIFY_QUERY {
        let remaining = state.identify_remaining(now);
        if remaining == 0 {
            return NOTHING;
        }
        header(out, true, request.seq, IDENTIFY_QUERY_RESPONSE);
        out.u16(remaining);
        return Outcome {
            has_reply: true,
            changed: Changed::NONE,
        };
    }

    let mut changed = Changed::NONE;

    let status = match (cluster, request.command) {
        (super::zdo::CLUSTER_ON_OFF, ON_OFF_OFF) => {
            changed.on_off = state.on;
            state.on = false;
            STATUS_SUCCESS
        }
        (super::zdo::CLUSTER_ON_OFF, ON_OFF_ON) => {
            changed.on_off = !state.on;
            state.on = true;
            STATUS_SUCCESS
        }
        (super::zdo::CLUSTER_ON_OFF, ON_OFF_TOGGLE) => {
            state.on = !state.on;
            changed.on_off = true;
            STATUS_SUCCESS
        }
        (
            super::zdo::CLUSTER_LEVEL_CONTROL,
            LEVEL_MOVE_TO_LEVEL | LEVEL_MOVE_TO_LEVEL_WITH_ON_OFF,
        ) => match Reader::new(request.payload).u8() {
            Some(level) => {
                let with_on_off = request.command == LEVEL_MOVE_TO_LEVEL_WITH_ON_OFF;
                changed = state.move_to_level(level, with_on_off);
                STATUS_SUCCESS
            }
            None => STATUS_INVALID_FIELD,
        },
        (super::zdo::CLUSTER_LEVEL_CONTROL, LEVEL_MOVE | LEVEL_MOVE_WITH_ON_OFF) => {
            let mut r = Reader::new(request.payload);
            match (r.u8(), r.u8()) {
                (Some(direction), Some(rate)) if direction <= DIRECTION_DOWN && rate != 0 => {
                    let with_on_off = request.command == LEVEL_MOVE_WITH_ON_OFF;
                    state.start_ramp(direction == DIRECTION_UP, rate, with_on_off, now);
                    STATUS_SUCCESS
                }
                _ => STATUS_INVALID_FIELD,
            }
        }
        (super::zdo::CLUSTER_LEVEL_CONTROL, LEVEL_STEP | LEVEL_STEP_WITH_ON_OFF) => {
            let mut r = Reader::new(request.payload);
            match (r.u8(), r.u8()) {
                (Some(direction), Some(size)) if direction <= DIRECTION_DOWN => {
                    let with_on_off = request.command == LEVEL_STEP_WITH_ON_OFF;
                    changed = state.step(direction == DIRECTION_UP, size, with_on_off);
                    STATUS_SUCCESS
                }
                _ => STATUS_INVALID_FIELD,
            }
        }
        (super::zdo::CLUSTER_LEVEL_CONTROL, LEVEL_STOP | LEVEL_STOP_WITH_ON_OFF) => {
            state.stop();
            STATUS_SUCCESS
        }
        (super::zdo::CLUSTER_IDENTIFY, IDENTIFY) => {
            let seconds = Reader::new(request.payload).u16().unwrap_or(0);
            state.identify_for(seconds, now);
            STATUS_SUCCESS
        }
        _ => STATUS_UNSUP_CLUSTER_COMMAND,
    };

    if request.disable_default_response && status == STATUS_SUCCESS {
        return Outcome {
            has_reply: false,
            changed,
        };
    }

    default_response(out, request.seq, request.command, status);
    Outcome {
        has_reply: true,
        changed,
    }
}

fn default_response(out: &mut Writer, seq: u8, command: u8, status: u8) {
    header(out, false, seq, CMD_DEFAULT_RESPONSE);
    out.u8(command);
    out.u8(status);
}

pub fn report_on_off(out: &mut Writer, seq: u8, on: bool) {
    header(out, false, seq, CMD_REPORT_ATTRIBUTES);
    out.u16(ATTR_ON_OFF);
    out.u8(TYPE_BOOL);
    out.u8(on as u8);
}

pub fn report_level(out: &mut Writer, seq: u8, level: u8) {
    header(out, false, seq, CMD_REPORT_ATTRIBUTES);
    out.u16(ATTR_CURRENT_LEVEL);
    out.u8(TYPE_UINT8);
    out.u8(level);
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDENTIFY_CLUSTER: u16 = super::super::zdo::CLUSTER_IDENTIFY;

    fn identity() -> Identity<'static> {
        Identity {
            manufacturer: "esp-rs",
            model: "H2.NoStd.Light",
            software_build: "0.1.0",
        }
    }

    fn run(cluster: u16, request: &[u8], state: &mut State, now: u32) -> (Outcome, Vec<u8>) {
        let mut buffer = [0u8; 96];
        let mut out = Writer::new(&mut buffer);
        let outcome = handle(
            &mut out,
            cluster,
            request,
            state,
            &identity(),
            Instant::from_millis(now),
        );
        let len = out.len();
        (outcome, buffer[..len].to_vec())
    }

    fn identify_command(seconds: u16) -> Vec<u8> {
        let mut frame = vec![0x01, 0x42, IDENTIFY];
        frame.extend_from_slice(&seconds.to_le_bytes());
        frame
    }

    #[test]
    fn identify_command_starts_a_countdown() {
        let mut state = State::default();
        run(IDENTIFY_CLUSTER, &identify_command(10), &mut state, 0);

        assert_eq!(state.identify_remaining(Instant::from_millis(0)), 10);
        assert_eq!(state.identify_remaining(Instant::from_millis(4_000)), 6);
        assert_eq!(state.identify_remaining(Instant::from_millis(10_000)), 0);
        assert_eq!(state.identify_remaining(Instant::from_millis(99_000)), 0);
    }

    #[test]
    fn identify_with_zero_seconds_stops_identifying() {
        let mut state = State::default();
        run(IDENTIFY_CLUSTER, &identify_command(10), &mut state, 0);
        run(IDENTIFY_CLUSTER, &identify_command(0), &mut state, 1_000);

        assert_eq!(state.identify_remaining(Instant::from_millis(1_000)), 0);
    }

    #[test]
    fn a_query_while_identifying_answers_with_the_remaining_time() {
        let mut state = State::default();
        run(IDENTIFY_CLUSTER, &identify_command(30), &mut state, 0);

        let query = vec![0x01, 0x43, IDENTIFY_QUERY];
        let (outcome, reply) = run(IDENTIFY_CLUSTER, &query, &mut state, 5_000);

        assert!(outcome.has_reply);
        assert_eq!(reply[2], IDENTIFY_QUERY_RESPONSE);
        assert_eq!(u16::from_le_bytes([reply[3], reply[4]]), 25);
    }

    #[test]
    fn a_query_while_idle_stays_silent() {
        let mut state = State::default();
        let query = vec![0x01, 0x43, IDENTIFY_QUERY];
        let (outcome, _) = run(IDENTIFY_CLUSTER, &query, &mut state, 0);

        assert!(!outcome.has_reply, "the spec asks an idle device not to answer");
    }

    #[test]
    fn writing_the_attribute_starts_identifying_too() {
        let mut state = State::default();
        let mut write = vec![0x00, 0x44, CMD_WRITE_ATTRIBUTES];
        write.extend_from_slice(&ATTR_IDENTIFY_TIME.to_le_bytes());
        write.push(TYPE_UINT16);
        write.extend_from_slice(&7u16.to_le_bytes());

        let (outcome, reply) = run(IDENTIFY_CLUSTER, &write, &mut state, 0);

        assert!(outcome.has_reply);
        assert_eq!(reply[3], STATUS_SUCCESS);
        assert_eq!(state.identify_remaining(Instant::from_millis(2_000)), 5);
    }

    #[test]
    fn writing_an_attribute_we_do_not_own_is_refused() {
        let mut state = State::default();
        let mut write = vec![0x00, 0x45, CMD_WRITE_ATTRIBUTES];
        write.extend_from_slice(&0x1234u16.to_le_bytes());
        write.push(TYPE_UINT16);
        write.extend_from_slice(&1u16.to_le_bytes());

        let (_, reply) = run(IDENTIFY_CLUSTER, &write, &mut state, 0);

        assert_eq!(reply[3], STATUS_UNSUPPORTED_ATTRIBUTE);
    }

    const LEVEL_CLUSTER: u16 = super::super::zdo::CLUSTER_LEVEL_CONTROL;

    #[test]
    fn a_move_to_level_without_a_level_is_refused() {
        let mut state = State::default();
        let truncated = vec![0x01, 0x61, LEVEL_MOVE_TO_LEVEL];

        let (outcome, reply) = run(LEVEL_CLUSTER, &truncated, &mut state, 0);

        assert!(outcome.has_reply);
        assert_eq!(reply[4], STATUS_INVALID_FIELD);
        assert_eq!(state.level, MAX_LEVEL, "a refused command changes nothing");
    }

    #[test]
    fn the_coordinator_can_ask_to_hear_about_the_brightness() {
        let mut state = State::default();
        let mut configure = vec![0x00, 0x62, CMD_CONFIGURE_REPORTING, DIRECTION_REPORT];
        configure.extend_from_slice(&ATTR_CURRENT_LEVEL.to_le_bytes());
        configure.push(TYPE_UINT8);
        configure.extend_from_slice(&1u16.to_le_bytes());
        configure.extend_from_slice(&60u16.to_le_bytes());
        configure.push(1);

        let (_, reply) = run(LEVEL_CLUSTER, &configure, &mut state, 0);

        assert_eq!(reply[3], STATUS_SUCCESS);
        let reporting = state.level_report.reporting.expect("accepted");
        assert_eq!(reporting.min_interval, 1);
        assert_eq!(reporting.max_interval, 60);
    }

    #[test]
    fn reading_the_current_level_answers_with_it() {
        let mut state = State {
            level: 33,
            ..State::default()
        };

        let mut read = vec![0x00, 0x63, CMD_READ_ATTRIBUTES];
        read.extend_from_slice(&ATTR_CURRENT_LEVEL.to_le_bytes());
        let (_, reply) = run(LEVEL_CLUSTER, &read, &mut state, 0);

        assert_eq!(reply[5], STATUS_SUCCESS);
        assert_eq!(reply[6], TYPE_UINT8);
        assert_eq!(reply[7], 33);
    }

    #[test]
    fn reading_the_attribute_reports_the_remaining_time() {
        let mut state = State::default();
        run(IDENTIFY_CLUSTER, &identify_command(60), &mut state, 0);

        let mut read = vec![0x00, 0x46, CMD_READ_ATTRIBUTES];
        read.extend_from_slice(&ATTR_IDENTIFY_TIME.to_le_bytes());
        let (_, reply) = run(IDENTIFY_CLUSTER, &read, &mut state, 20_000);

        assert_eq!(reply[5], STATUS_SUCCESS);
        assert_eq!(reply[6], TYPE_UINT16);
        assert_eq!(u16::from_le_bytes([reply[7], reply[8]]), 40);
    }
}
