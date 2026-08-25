use crate::buf::{Reader, Writer};
use crate::zdo::{
    CLUSTER_BASIC, CLUSTER_COLOUR_CONTROL, CLUSTER_GROUPS, CLUSTER_IDENTIFY, CLUSTER_LEVEL_CONTROL,
    CLUSTER_ON_OFF, CLUSTER_SCENES,
};
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

pub const GROUP_ADD: u8 = 0x00;
pub const GROUP_VIEW: u8 = 0x01;
pub const GROUP_GET_MEMBERSHIP: u8 = 0x02;
pub const GROUP_REMOVE: u8 = 0x03;
pub const GROUP_REMOVE_ALL: u8 = 0x04;
pub const GROUP_ADD_IF_IDENTIFYING: u8 = 0x05;
pub const ATTR_GROUP_NAME_SUPPORT: u16 = 0x0000;

/// Group zero addresses nobody, which makes it the empty slot in the table and
/// the way a scene says it belongs to no group at all.
const NO_GROUP: u16 = 0x0000;

pub const SCENE_ADD: u8 = 0x00;
pub const SCENE_VIEW: u8 = 0x01;
pub const SCENE_REMOVE: u8 = 0x02;
pub const SCENE_REMOVE_ALL: u8 = 0x03;
pub const SCENE_STORE: u8 = 0x04;
pub const SCENE_RECALL: u8 = 0x05;
pub const SCENE_GET_MEMBERSHIP: u8 = 0x06;
pub const ATTR_SCENE_COUNT: u16 = 0x0000;
pub const ATTR_CURRENT_SCENE: u16 = 0x0001;
pub const ATTR_CURRENT_GROUP: u16 = 0x0002;
pub const ATTR_SCENE_VALID: u16 = 0x0003;
pub const ATTR_SCENE_NAME_SUPPORT: u16 = 0x0004;

/// How many groups and scenes a device with no allocator can hold. Both tables
/// live in memory, so a restart starts them empty.
pub const MAX_GROUPS: usize = 4;
pub const MAX_SCENES: usize = 8;

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
pub const ATTR_CURRENT_LEVEL: u16 = 0x0000;

const LEVEL_UP: u8 = 0x00;
const LEVEL_DOWN: u8 = 0x01;

/// The brightest a Level Control light goes. 0xff is reserved for "undefined",
/// so the usable range stops one short of it.
pub const MAX_LEVEL: u8 = 0xfe;

/// The rate a coordinator sends when it wants the light to move as fast as it
/// can rather than at a stated number of units per second.
const RATE_UNSTATED: u8 = 0xff;

pub const COLOUR_MOVE_TO_HUE: u8 = 0x00;
pub const COLOUR_STEP_HUE: u8 = 0x02;
pub const COLOUR_MOVE_TO_SATURATION: u8 = 0x03;
pub const COLOUR_STEP_SATURATION: u8 = 0x05;
pub const COLOUR_MOVE_TO_HUE_AND_SATURATION: u8 = 0x06;
pub const COLOUR_MOVE_TO_XY: u8 = 0x07;
pub const COLOUR_MOVE_TO_TEMPERATURE: u8 = 0x0a;
pub const COLOUR_STOP: u8 = 0x47;
pub const ATTR_CURRENT_HUE: u16 = 0x0000;
pub const ATTR_CURRENT_SATURATION: u16 = 0x0001;
pub const ATTR_CURRENT_X: u16 = 0x0003;
pub const ATTR_CURRENT_Y: u16 = 0x0004;
pub const ATTR_COLOUR_TEMPERATURE: u16 = 0x0007;
pub const ATTR_COLOUR_MODE: u16 = 0x0008;
pub const ATTR_ENHANCED_COLOUR_MODE: u16 = 0x4001;
pub const ATTR_COLOUR_CAPABILITIES: u16 = 0x400a;
pub const ATTR_TEMPERATURE_MIN_MIREDS: u16 = 0x400b;
pub const ATTR_TEMPERATURE_MAX_MIREDS: u16 = 0x400c;

pub const COLOUR_MODE_HUE_SATURATION: u8 = 0x00;
pub const COLOUR_MODE_XY: u8 = 0x01;
pub const COLOUR_MODE_TEMPERATURE: u8 = 0x02;

const COLOUR_UP: u8 = 0x01;
const COLOUR_DOWN: u8 = 0x03;

pub const MAX_HUE: u8 = 0xfe;
pub const MAX_SATURATION: u8 = 0xfe;

/// The full circle hue travels around, which is one more than the brightest
/// hue because the range starts at zero.
const HUE_STEPS: u16 = MAX_HUE as u16 + 1;

/// A mired is a million over the colour temperature in kelvin, so the smaller
/// number is the cooler light. This range is roughly 6500 K down to 2000 K.
pub const COOLEST_MIREDS: u16 = 153;
pub const WARMEST_MIREDS: u16 = 500;

/// Hue and saturation, the XY space, and colour temperature. Not the enhanced
/// hue, so a bridge that wants that converts on its own side.
const COLOUR_CAPABILITIES: u16 = 0x0019;

const WHITE_X: u16 = 0x616b;
const WHITE_Y: u16 = 0x607d;

const TYPE_BOOL: u8 = 0x10;
const TYPE_BITMAP8: u8 = 0x18;
const TYPE_BITMAP16: u8 = 0x19;
const TYPE_UINT8: u8 = 0x20;
const TYPE_UINT16: u8 = 0x21;
const TYPE_ENUM8: u8 = 0x30;
const TYPE_STRING: u8 = 0x42;

const STATUS_SUCCESS: u8 = 0x00;
const STATUS_UNSUP_CLUSTER_COMMAND: u8 = 0x81;
const STATUS_INVALID_FIELD: u8 = 0x85;
const STATUS_UNSUPPORTED_ATTRIBUTE: u8 = 0x86;
const STATUS_INSUFFICIENT_SPACE: u8 = 0x89;
const STATUS_DUPLICATE_EXISTS: u8 = 0x8a;
const STATUS_NOT_FOUND: u8 = 0x8b;

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
        (CLUSTER_BASIC, 0x0000) => {
            out.u8(TYPE_UINT8).u8(3);
        }
        (CLUSTER_BASIC, 0x0001) => {
            out.u8(TYPE_UINT8).u8(1);
        }
        (CLUSTER_BASIC, 0x0002) => {
            out.u8(TYPE_UINT8).u8(2);
        }
        (CLUSTER_BASIC, 0x0003) => {
            out.u8(TYPE_UINT8).u8(1);
        }
        (CLUSTER_BASIC, 0x0004) => string(out, identity.manufacturer),
        (CLUSTER_BASIC, 0x0005) => string(out, identity.model),
        (CLUSTER_BASIC, 0x0007) => {
            out.u8(TYPE_ENUM8).u8(0x01);
        }
        (CLUSTER_BASIC, 0x4000) => string(out, identity.software_build),
        (CLUSTER_IDENTIFY, ATTR_IDENTIFY_TIME) => {
            out.u8(TYPE_UINT16).u16(state.identify_remaining(now));
        }
        (CLUSTER_GROUPS, ATTR_GROUP_NAME_SUPPORT) | (CLUSTER_SCENES, ATTR_SCENE_NAME_SUPPORT) => {
            out.u8(TYPE_BITMAP8).u8(0x00);
        }
        (CLUSTER_SCENES, ATTR_SCENE_COUNT) => {
            out.u8(TYPE_UINT8).u8(state.scene_count());
        }
        (CLUSTER_SCENES, ATTR_CURRENT_SCENE) => {
            out.u8(TYPE_UINT8).u8(state.current_scene);
        }
        (CLUSTER_SCENES, ATTR_CURRENT_GROUP) => {
            out.u8(TYPE_UINT16).u16(state.current_group);
        }
        (CLUSTER_SCENES, ATTR_SCENE_VALID) => {
            out.u8(TYPE_BOOL).u8(state.scene_valid as u8);
        }
        (CLUSTER_ON_OFF, ATTR_ON_OFF) => {
            out.u8(TYPE_BOOL).u8(state.on as u8);
        }
        (CLUSTER_LEVEL_CONTROL, ATTR_CURRENT_LEVEL) => {
            out.u8(TYPE_UINT8).u8(state.level);
        }
        (CLUSTER_COLOUR_CONTROL, ATTR_CURRENT_HUE) => {
            out.u8(TYPE_UINT8).u8(state.hue);
        }
        (CLUSTER_COLOUR_CONTROL, ATTR_CURRENT_SATURATION) => {
            out.u8(TYPE_UINT8).u8(state.saturation);
        }
        (CLUSTER_COLOUR_CONTROL, ATTR_CURRENT_X) => {
            out.u8(TYPE_UINT16).u16(state.x);
        }
        (CLUSTER_COLOUR_CONTROL, ATTR_CURRENT_Y) => {
            out.u8(TYPE_UINT16).u16(state.y);
        }
        (CLUSTER_COLOUR_CONTROL, ATTR_COLOUR_TEMPERATURE) => {
            out.u8(TYPE_UINT16).u16(state.mireds);
        }
        (CLUSTER_COLOUR_CONTROL, ATTR_COLOUR_MODE)
        | (CLUSTER_COLOUR_CONTROL, ATTR_ENHANCED_COLOUR_MODE) => {
            out.u8(TYPE_ENUM8).u8(state.colour_mode);
        }
        (CLUSTER_COLOUR_CONTROL, ATTR_COLOUR_CAPABILITIES) => {
            out.u8(TYPE_BITMAP16).u16(COLOUR_CAPABILITIES);
        }
        (CLUSTER_COLOUR_CONTROL, ATTR_TEMPERATURE_MIN_MIREDS) => {
            out.u8(TYPE_UINT16).u16(COOLEST_MIREDS);
        }
        (CLUSTER_COLOUR_CONTROL, ATTR_TEMPERATURE_MAX_MIREDS) => {
            out.u8(TYPE_UINT16).u16(WARMEST_MIREDS);
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
    pub colour: bool,
    pub tables: bool,
}

impl Changed {
    pub const NONE: Self = Self {
        on_off: false,
        level: false,
        colour: false,
        tables: false,
    };

    pub const TABLES: Self = Self {
        tables: true,
        ..Self::NONE
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

/// One remembered light setting, recalled by its group and scene together.
#[derive(Clone, Copy)]
pub struct Scene {
    pub group: u16,
    pub id: u8,
    pub on: bool,
    pub level: u8,
    pub hue: u8,
    pub saturation: u8,
    pub x: u16,
    pub y: u16,
    pub mireds: u16,
    pub colour_mode: u8,
}

pub struct State {
    pub on: bool,
    pub level: u8,
    pub hue: u8,
    pub saturation: u8,
    pub x: u16,
    pub y: u16,
    pub mireds: u16,
    pub colour_mode: u8,
    pub identify_until: Option<Instant>,
    pub on_off_report: Reportable,
    pub level_report: Reportable,
    pub groups: [u16; MAX_GROUPS],
    pub scenes: [Option<Scene>; MAX_SCENES],
    pub current_group: u16,
    pub current_scene: u8,
    pub scene_valid: bool,
    ramp: Option<Ramp>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            on: false,
            level: MAX_LEVEL,
            hue: 0,
            saturation: 0,
            x: WHITE_X,
            y: WHITE_Y,
            mireds: 370,
            colour_mode: COLOUR_MODE_TEMPERATURE,
            identify_until: None,
            on_off_report: Reportable::default(),
            level_report: Reportable::default(),
            groups: [NO_GROUP; MAX_GROUPS],
            scenes: [None; MAX_SCENES],
            current_group: NO_GROUP,
            current_scene: 0,
            scene_valid: false,
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
            level: self.level != level,
            ..Changed::NONE
        };
        self.level = level;

        if with_on_off {
            let on = level > 0;
            changed.on_off = self.on != on;
            self.on = on;
        }
        if changed.level || changed.on_off {
            self.scene_valid = false;
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
            rate: if rate == RATE_UNSTATED {
                MAX_LEVEL
            } else {
                rate
            },
            with_on_off,
            started_at: now,
            from: self.level,
        });
    }

    /// Abandons any move under way, leaving the brightness where it reached.
    pub fn stop(&mut self) {
        self.ramp = None;
    }

    fn set_hue_and_saturation(&mut self, hue: u8, saturation: u8) -> Changed {
        let hue = hue.min(MAX_HUE);
        let saturation = saturation.min(MAX_SATURATION);
        let changed = Changed {
            colour: self.hue != hue
                || self.saturation != saturation
                || self.colour_mode != COLOUR_MODE_HUE_SATURATION,
            ..Changed::NONE
        };
        self.hue = hue;
        self.saturation = saturation;
        self.colour_mode = COLOUR_MODE_HUE_SATURATION;
        if changed.colour {
            self.scene_valid = false;
        }
        changed
    }

    fn set_xy(&mut self, x: u16, y: u16) -> Changed {
        let changed = Changed {
            colour: self.x != x || self.y != y || self.colour_mode != COLOUR_MODE_XY,
            ..Changed::NONE
        };
        self.x = x;
        self.y = y;
        self.colour_mode = COLOUR_MODE_XY;
        if changed.colour {
            self.scene_valid = false;
        }
        changed
    }

    fn set_mireds(&mut self, mireds: u16) -> Changed {
        let mireds = mireds.clamp(COOLEST_MIREDS, WARMEST_MIREDS);
        let changed = Changed {
            colour: self.mireds != mireds || self.colour_mode != COLOUR_MODE_TEMPERATURE,
            ..Changed::NONE
        };
        self.mireds = mireds;
        self.colour_mode = COLOUR_MODE_TEMPERATURE;
        if changed.colour {
            self.scene_valid = false;
        }
        changed
    }

    /// Hue is a circle, so a step off either end comes back on the other.
    fn step_hue(&mut self, up: bool, size: u8) -> Changed {
        let size = size as u16 % HUE_STEPS;
        let moved = if up {
            self.hue as u16 + size
        } else {
            self.hue as u16 + HUE_STEPS - size
        };
        self.set_hue_and_saturation((moved % HUE_STEPS) as u8, self.saturation)
    }

    fn step_saturation(&mut self, up: bool, size: u8) -> Changed {
        let moved = if up {
            self.saturation.saturating_add(size)
        } else {
            self.saturation.saturating_sub(size)
        };
        self.set_hue_and_saturation(self.hue, moved)
    }

    pub fn in_group(&self, group: u16) -> bool {
        group != NO_GROUP && self.groups.contains(&group)
    }

    fn join_group(&mut self, group: u16) -> u8 {
        if group == NO_GROUP {
            return STATUS_INVALID_FIELD;
        }
        if self.in_group(group) {
            return STATUS_DUPLICATE_EXISTS;
        }
        match self.groups.iter_mut().find(|slot| **slot == NO_GROUP) {
            Some(slot) => {
                *slot = group;
                STATUS_SUCCESS
            }
            None => STATUS_INSUFFICIENT_SPACE,
        }
    }

    fn leave_group(&mut self, group: u16) -> u8 {
        match self.groups.iter_mut().find(|slot| **slot == group) {
            Some(slot) if group != NO_GROUP => {
                *slot = NO_GROUP;
                self.forget_scenes_of(group);
                STATUS_SUCCESS
            }
            _ => STATUS_NOT_FOUND,
        }
    }

    fn leave_every_group(&mut self) {
        for group in self.groups {
            self.forget_scenes_of(group);
        }
        self.groups = [NO_GROUP; MAX_GROUPS];
    }

    /// A scene belongs to a group, so losing the group loses the scene with it.
    fn forget_scenes_of(&mut self, group: u16) {
        for slot in self.scenes.iter_mut() {
            if slot.is_some_and(|scene| scene.group == group) {
                *slot = None;
            }
        }
    }

    fn group_capacity(&self) -> u8 {
        self.groups.iter().filter(|slot| **slot == NO_GROUP).count() as u8
    }

    fn scene_capacity(&self) -> u8 {
        self.scenes.iter().filter(|slot| slot.is_none()).count() as u8
    }

    fn scene_count(&self) -> u8 {
        (MAX_SCENES as u8) - self.scene_capacity()
    }

    fn find_scene(&self, group: u16, id: u8) -> Option<usize> {
        self.scenes
            .iter()
            .position(|slot| slot.is_some_and(|scene| scene.group == group && scene.id == id))
    }

    /// A scene may sit in a group only if the device is in that group, and the
    /// groupless scene zero is always allowed.
    fn may_hold_scenes_for(&self, group: u16) -> bool {
        group == NO_GROUP || self.in_group(group)
    }

    fn put_scene(&mut self, scene: Scene) -> u8 {
        if !self.may_hold_scenes_for(scene.group) {
            return STATUS_INVALID_FIELD;
        }
        if let Some(index) = self.find_scene(scene.group, scene.id) {
            self.scenes[index] = Some(scene);
            return STATUS_SUCCESS;
        }
        match self.scenes.iter_mut().find(|slot| slot.is_none()) {
            Some(slot) => {
                *slot = Some(scene);
                STATUS_SUCCESS
            }
            None => STATUS_INSUFFICIENT_SPACE,
        }
    }

    fn store_scene(&mut self, group: u16, id: u8) -> u8 {
        self.put_scene(Scene {
            group,
            id,
            on: self.on,
            level: self.level,
            hue: self.hue,
            saturation: self.saturation,
            x: self.x,
            y: self.y,
            mireds: self.mireds,
            colour_mode: self.colour_mode,
        })
    }

    fn remove_scene(&mut self, group: u16, id: u8) -> u8 {
        match self.find_scene(group, id) {
            Some(index) => {
                self.scenes[index] = None;
                STATUS_SUCCESS
            }
            None => STATUS_NOT_FOUND,
        }
    }

    fn remove_scenes_of(&mut self, group: u16) -> u8 {
        if !self.may_hold_scenes_for(group) {
            return STATUS_INVALID_FIELD;
        }
        self.forget_scenes_of(group);
        STATUS_SUCCESS
    }

    fn recall_scene(&mut self, group: u16, id: u8) -> Option<Changed> {
        let index = self.find_scene(group, id)?;
        let scene = self.scenes[index]?;

        let mut changed = self.settle(scene.level, false);
        if self.on != scene.on {
            changed.on_off = true;
            self.on = scene.on;
        }
        let colour = match scene.colour_mode {
            COLOUR_MODE_TEMPERATURE => self.set_mireds(scene.mireds),
            COLOUR_MODE_XY => self.set_xy(scene.x, scene.y),
            _ => self.set_hue_and_saturation(scene.hue, scene.saturation),
        };
        changed.colour = colour.colour;

        self.current_group = scene.group;
        self.current_scene = scene.id;
        self.scene_valid = true;
        Some(changed)
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
                let record_at = out.len();
                out.u16(attribute);
                let status_at = out.len();
                out.u8(STATUS_SUCCESS);
                if !write_attribute(out, cluster, attribute, state, identity, now) {
                    out.set(status_at, STATUS_UNSUPPORTED_ATTRIBUTE);
                }
                if out.overflowed() {
                    out.truncate(record_at);
                    break;
                }
            }
            replied(Changed::NONE)
        }
        CMD_CONFIGURE_REPORTING => {
            header(out, false, request.seq, CMD_CONFIGURE_REPORTING_RESPONSE);
            configure_reporting(out, cluster, request.payload, state);
            replied(Changed::NONE)
        }
        CMD_WRITE_ATTRIBUTES => {
            header(out, false, request.seq, CMD_WRITE_ATTRIBUTES_RESPONSE);
            write_attributes(out, cluster, request.payload, state, now);
            replied(Changed::NONE)
        }
        CMD_DISCOVER_ATTRIBUTES => {
            header(out, false, request.seq, CMD_DISCOVER_ATTRIBUTES_RESPONSE);
            out.u8(0x01);
            replied(Changed::NONE)
        }
        CMD_DEFAULT_RESPONSE => NOTHING,
        _ => {
            default_response(
                out,
                request.seq,
                request.command,
                STATUS_UNSUP_CLUSTER_COMMAND,
            );
            replied(Changed::NONE)
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
        let (Some(data_type), Some(min_interval), Some(max_interval)) = (r.u8(), r.u16(), r.u16())
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
        if cluster == CLUSTER_ON_OFF && attribute == ATTR_ON_OFF {
            state.on_off_report.reporting = Some(wanted);
            continue;
        }
        if cluster == CLUSTER_LEVEL_CONTROL && attribute == ATTR_CURRENT_LEVEL {
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
        let writable = cluster == CLUSTER_IDENTIFY && attribute == ATTR_IDENTIFY_TIME;
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

/// The values a scene remembers, written the way the specification lays them
/// out per cluster.
fn read_extension_fields(payload: &[u8], scene: &mut Scene) {
    let mut r = Reader::new(payload);
    while let (Some(cluster), Some(len)) = (r.u16(), r.u8()) {
        let Some(fields) = r.take(len as usize) else {
            return;
        };
        let mut f = Reader::new(fields);
        match cluster {
            CLUSTER_ON_OFF => {
                if let Some(on) = f.u8() {
                    scene.on = on != 0;
                }
            }
            CLUSTER_LEVEL_CONTROL => {
                if let Some(level) = f.u8() {
                    scene.level = level.min(MAX_LEVEL);
                }
            }
            CLUSTER_COLOUR_CONTROL => read_colour_fields(&mut f, scene),
            _ => {}
        }
    }
}

/// The colour field set opens with the XY space, the hue and the saturation
/// are taken from further in, and a temperature that is not zero is what says
/// the scene wanted a white rather than a colour.
fn read_colour_fields(f: &mut Reader, scene: &mut Scene) {
    let (Some(x), Some(y)) = (f.u16(), f.u16()) else {
        return;
    };
    scene.x = x;
    scene.y = y;

    let (Some(enhanced_hue), Some(saturation)) = (f.u16(), f.u8()) else {
        return;
    };
    scene.hue = (enhanced_hue >> 8) as u8;
    scene.saturation = saturation.min(MAX_SATURATION);
    scene.colour_mode = COLOUR_MODE_HUE_SATURATION;

    if f.skip(4).is_none() {
        return;
    }
    if let Some(mireds) = f.u16().filter(|mireds| *mireds != 0) {
        scene.mireds = mireds.clamp(COOLEST_MIREDS, WARMEST_MIREDS);
        scene.colour_mode = COLOUR_MODE_TEMPERATURE;
    }
}

fn write_extension_fields(out: &mut Writer, scene: &Scene) {
    out.u16(CLUSTER_ON_OFF).u8(1).u8(scene.on as u8);
    out.u16(CLUSTER_LEVEL_CONTROL).u8(1).u8(scene.level);
    out.u16(CLUSTER_COLOUR_CONTROL).u8(13);
    out.u16(scene.x).u16(scene.y);
    out.u16((scene.hue as u16) << 8).u8(scene.saturation);
    out.u8(0).u8(0).u16(0);
    out.u16(match scene.colour_mode {
        COLOUR_MODE_TEMPERATURE => scene.mireds,
        _ => 0,
    });
}

fn handle_group_command(
    out: &mut Writer,
    request: &Incoming,
    state: &mut State,
    now: Instant,
) -> Outcome {
    let mut r = Reader::new(request.payload);
    let group = r.u16();

    match (request.command, group) {
        (GROUP_ADD, Some(group)) => {
            let status = state.join_group(group);
            header(out, true, request.seq, GROUP_ADD);
            out.u8(status).u16(group);
            replied(Changed::TABLES)
        }
        (GROUP_ADD_IF_IDENTIFYING, Some(group)) => {
            if state.identify_remaining(now) > 0 {
                state.join_group(group);
            }
            finish(out, request, STATUS_SUCCESS, Changed::TABLES)
        }
        (GROUP_VIEW, Some(group)) => {
            let status = if state.in_group(group) {
                STATUS_SUCCESS
            } else {
                STATUS_NOT_FOUND
            };
            header(out, true, request.seq, GROUP_VIEW);
            out.u8(status).u16(group).u8(0);
            replied(Changed::NONE)
        }
        (GROUP_REMOVE, Some(group)) => {
            let status = state.leave_group(group);
            header(out, true, request.seq, GROUP_REMOVE);
            out.u8(status).u16(group);
            replied(Changed::TABLES)
        }
        (GROUP_GET_MEMBERSHIP, _) => {
            let mut r = Reader::new(request.payload);
            let wanted = r.u8().unwrap_or(0);
            header(out, true, request.seq, GROUP_GET_MEMBERSHIP);
            out.u8(state.group_capacity());
            let count_at = out.len();
            out.u8(0);

            let mut count = 0;
            if wanted == 0 {
                for group in state.groups.iter().filter(|slot| **slot != NO_GROUP) {
                    out.u16(*group);
                    count += 1;
                }
            } else {
                for _ in 0..wanted {
                    let Some(group) = r.u16() else { break };
                    if state.in_group(group) {
                        out.u16(group);
                        count += 1;
                    }
                }
            }
            out.set(count_at, count);
            replied(Changed::NONE)
        }
        (GROUP_REMOVE_ALL, _) => {
            state.leave_every_group();
            finish(out, request, STATUS_SUCCESS, Changed::TABLES)
        }
        (_, None) => malformed(out, request),
        _ => finish(out, request, STATUS_UNSUP_CLUSTER_COMMAND, Changed::NONE),
    }
}

fn handle_scene_command(out: &mut Writer, request: &Incoming, state: &mut State) -> Outcome {
    let mut r = Reader::new(request.payload);
    let Some(group) = r.u16() else {
        return malformed(out, request);
    };

    match request.command {
        SCENE_ADD => {
            let (Some(id), Some(_transition), Some(name_len)) = (r.u8(), r.u16(), r.u8()) else {
                return malformed(out, request);
            };
            if r.skip(name_len as usize).is_none() {
                return malformed(out, request);
            }
            let mut scene = Scene {
                group,
                id,
                on: state.on,
                level: state.level,
                hue: state.hue,
                saturation: state.saturation,
                x: state.x,
                y: state.y,
                mireds: state.mireds,
                colour_mode: state.colour_mode,
            };
            read_extension_fields(r.rest(), &mut scene);
            let status = state.put_scene(scene);
            header(out, true, request.seq, SCENE_ADD);
            out.u8(status).u16(group).u8(id);
            replied(Changed::TABLES)
        }
        SCENE_VIEW => {
            let Some(id) = r.u8() else {
                return malformed(out, request);
            };
            header(out, true, request.seq, SCENE_VIEW);
            match state.find_scene(group, id).and_then(|at| state.scenes[at]) {
                Some(scene) => {
                    out.u8(STATUS_SUCCESS).u16(group).u8(id).u16(0).u8(0);
                    write_extension_fields(out, &scene);
                }
                None => {
                    out.u8(STATUS_NOT_FOUND).u16(group).u8(id);
                }
            }
            replied(Changed::NONE)
        }
        SCENE_REMOVE => {
            let Some(id) = r.u8() else {
                return malformed(out, request);
            };
            let status = state.remove_scene(group, id);
            header(out, true, request.seq, SCENE_REMOVE);
            out.u8(status).u16(group).u8(id);
            replied(Changed::TABLES)
        }
        SCENE_REMOVE_ALL => {
            let status = state.remove_scenes_of(group);
            header(out, true, request.seq, SCENE_REMOVE_ALL);
            out.u8(status).u16(group);
            replied(Changed::TABLES)
        }
        SCENE_STORE => {
            let Some(id) = r.u8() else {
                return malformed(out, request);
            };
            let status = state.store_scene(group, id);
            header(out, true, request.seq, SCENE_STORE);
            out.u8(status).u16(group).u8(id);
            replied(Changed::TABLES)
        }
        SCENE_RECALL => {
            let Some(id) = r.u8() else {
                return malformed(out, request);
            };
            match state.recall_scene(group, id) {
                Some(changed) => finish(out, request, STATUS_SUCCESS, changed),
                None => finish(out, request, STATUS_NOT_FOUND, Changed::NONE),
            }
        }
        SCENE_GET_MEMBERSHIP => {
            header(out, true, request.seq, SCENE_GET_MEMBERSHIP);
            if state.may_hold_scenes_for(group) {
                out.u8(STATUS_SUCCESS).u8(state.scene_capacity()).u16(group);
                let count_at = out.len();
                out.u8(0);
                let mut count = 0;
                for scene in state.scenes.iter().flatten().filter(|s| s.group == group) {
                    out.u8(scene.id);
                    count += 1;
                }
                out.set(count_at, count);
            } else {
                out.u8(STATUS_INVALID_FIELD)
                    .u8(state.scene_capacity())
                    .u16(group);
            }
            replied(Changed::NONE)
        }
        _ => finish(out, request, STATUS_UNSUP_CLUSTER_COMMAND, Changed::NONE),
    }
}

/// A reply the command wrote for itself, rather than the generic one.
const fn replied(changed: Changed) -> Outcome {
    Outcome {
        has_reply: true,
        changed,
    }
}

fn malformed(out: &mut Writer, request: &Incoming) -> Outcome {
    finish(out, request, STATUS_INVALID_FIELD, Changed::NONE)
}

/// Either the default response the specification asks for, or nothing when the
/// requester said it did not want one and there was nothing to report.
fn finish(out: &mut Writer, request: &Incoming, status: u8, changed: Changed) -> Outcome {
    if request.disable_default_response && status == STATUS_SUCCESS {
        return Outcome {
            has_reply: false,
            changed,
        };
    }
    default_response(out, request.seq, request.command, status);
    replied(changed)
}

fn handle_cluster_command(
    out: &mut Writer,
    cluster: u16,
    request: &Incoming,
    state: &mut State,
    now: Instant,
) -> Outcome {
    match cluster {
        CLUSTER_GROUPS => return handle_group_command(out, request, state, now),
        CLUSTER_SCENES => return handle_scene_command(out, request, state),
        _ => {}
    }

    if cluster == CLUSTER_IDENTIFY && request.command == IDENTIFY_QUERY {
        let remaining = state.identify_remaining(now);
        if remaining == 0 {
            return NOTHING;
        }
        header(out, true, request.seq, IDENTIFY_QUERY_RESPONSE);
        out.u16(remaining);
        return replied(Changed::NONE);
    }

    let mut changed = Changed::NONE;

    let status = match (cluster, request.command) {
        (CLUSTER_ON_OFF, ON_OFF_OFF) => {
            changed.on_off = state.on;
            state.on = false;
            STATUS_SUCCESS
        }
        (CLUSTER_ON_OFF, ON_OFF_ON) => {
            changed.on_off = !state.on;
            state.on = true;
            STATUS_SUCCESS
        }
        (CLUSTER_ON_OFF, ON_OFF_TOGGLE) => {
            state.on = !state.on;
            changed.on_off = true;
            STATUS_SUCCESS
        }
        (CLUSTER_LEVEL_CONTROL, LEVEL_MOVE_TO_LEVEL | LEVEL_MOVE_TO_LEVEL_WITH_ON_OFF) => {
            match Reader::new(request.payload).u8() {
                Some(level) => {
                    let with_on_off = request.command == LEVEL_MOVE_TO_LEVEL_WITH_ON_OFF;
                    changed = state.move_to_level(level, with_on_off);
                    STATUS_SUCCESS
                }
                None => STATUS_INVALID_FIELD,
            }
        }
        (CLUSTER_LEVEL_CONTROL, LEVEL_MOVE | LEVEL_MOVE_WITH_ON_OFF) => {
            let mut r = Reader::new(request.payload);
            match (r.u8(), r.u8()) {
                (Some(direction), Some(rate))
                    if matches!(direction, LEVEL_UP | LEVEL_DOWN) && rate != 0 =>
                {
                    let with_on_off = request.command == LEVEL_MOVE_WITH_ON_OFF;
                    state.start_ramp(direction == LEVEL_UP, rate, with_on_off, now);
                    STATUS_SUCCESS
                }
                _ => STATUS_INVALID_FIELD,
            }
        }
        (CLUSTER_LEVEL_CONTROL, LEVEL_STEP | LEVEL_STEP_WITH_ON_OFF) => {
            let mut r = Reader::new(request.payload);
            match (r.u8(), r.u8()) {
                (Some(direction), Some(size)) if matches!(direction, LEVEL_UP | LEVEL_DOWN) => {
                    let with_on_off = request.command == LEVEL_STEP_WITH_ON_OFF;
                    changed = state.step(direction == LEVEL_UP, size, with_on_off);
                    STATUS_SUCCESS
                }
                _ => STATUS_INVALID_FIELD,
            }
        }
        (CLUSTER_LEVEL_CONTROL, LEVEL_STOP | LEVEL_STOP_WITH_ON_OFF) => {
            state.stop();
            STATUS_SUCCESS
        }
        (CLUSTER_COLOUR_CONTROL, COLOUR_MOVE_TO_HUE) => match Reader::new(request.payload).u8() {
            Some(hue) => {
                changed = state.set_hue_and_saturation(hue, state.saturation);
                STATUS_SUCCESS
            }
            None => STATUS_INVALID_FIELD,
        },
        (CLUSTER_COLOUR_CONTROL, COLOUR_MOVE_TO_SATURATION) => {
            match Reader::new(request.payload).u8() {
                Some(saturation) => {
                    changed = state.set_hue_and_saturation(state.hue, saturation);
                    STATUS_SUCCESS
                }
                None => STATUS_INVALID_FIELD,
            }
        }
        (CLUSTER_COLOUR_CONTROL, COLOUR_MOVE_TO_HUE_AND_SATURATION) => {
            let mut r = Reader::new(request.payload);
            match (r.u8(), r.u8()) {
                (Some(hue), Some(saturation)) => {
                    changed = state.set_hue_and_saturation(hue, saturation);
                    STATUS_SUCCESS
                }
                _ => STATUS_INVALID_FIELD,
            }
        }
        (CLUSTER_COLOUR_CONTROL, COLOUR_MOVE_TO_XY) => {
            let mut r = Reader::new(request.payload);
            match (r.u16(), r.u16()) {
                (Some(x), Some(y)) => {
                    changed = state.set_xy(x, y);
                    STATUS_SUCCESS
                }
                _ => STATUS_INVALID_FIELD,
            }
        }
        (CLUSTER_COLOUR_CONTROL, COLOUR_MOVE_TO_TEMPERATURE) => {
            match Reader::new(request.payload).u16() {
                Some(mireds) => {
                    changed = state.set_mireds(mireds);
                    STATUS_SUCCESS
                }
                None => STATUS_INVALID_FIELD,
            }
        }
        (CLUSTER_COLOUR_CONTROL, COLOUR_STEP_HUE | COLOUR_STEP_SATURATION) => {
            let mut r = Reader::new(request.payload);
            match (r.u8(), r.u8()) {
                (Some(mode), Some(size)) if matches!(mode, COLOUR_UP | COLOUR_DOWN) => {
                    let up = mode == COLOUR_UP;
                    changed = if request.command == COLOUR_STEP_HUE {
                        state.step_hue(up, size)
                    } else {
                        state.step_saturation(up, size)
                    };
                    STATUS_SUCCESS
                }
                _ => STATUS_INVALID_FIELD,
            }
        }
        (CLUSTER_COLOUR_CONTROL, COLOUR_STOP) => STATUS_SUCCESS,
        (CLUSTER_IDENTIFY, IDENTIFY) => {
            let seconds = Reader::new(request.payload).u16().unwrap_or(0);
            state.identify_for(seconds, now);
            STATUS_SUCCESS
        }
        _ => STATUS_UNSUP_CLUSTER_COMMAND,
    };

    finish(out, request, status, changed)
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
        run(CLUSTER_IDENTIFY, &identify_command(10), &mut state, 0);

        assert_eq!(state.identify_remaining(Instant::from_millis(0)), 10);
        assert_eq!(state.identify_remaining(Instant::from_millis(4_000)), 6);
        assert_eq!(state.identify_remaining(Instant::from_millis(10_000)), 0);
        assert_eq!(state.identify_remaining(Instant::from_millis(99_000)), 0);
    }

    #[test]
    fn identify_with_zero_seconds_stops_identifying() {
        let mut state = State::default();
        run(CLUSTER_IDENTIFY, &identify_command(10), &mut state, 0);
        run(CLUSTER_IDENTIFY, &identify_command(0), &mut state, 1_000);

        assert_eq!(state.identify_remaining(Instant::from_millis(1_000)), 0);
    }

    #[test]
    fn a_query_while_identifying_answers_with_the_remaining_time() {
        let mut state = State::default();
        run(CLUSTER_IDENTIFY, &identify_command(30), &mut state, 0);

        let query = vec![0x01, 0x43, IDENTIFY_QUERY];
        let (outcome, reply) = run(CLUSTER_IDENTIFY, &query, &mut state, 5_000);

        assert!(outcome.has_reply);
        assert_eq!(reply[2], IDENTIFY_QUERY_RESPONSE);
        assert_eq!(u16::from_le_bytes([reply[3], reply[4]]), 25);
    }

    #[test]
    fn a_query_while_idle_stays_silent() {
        let mut state = State::default();
        let query = vec![0x01, 0x43, IDENTIFY_QUERY];
        let (outcome, _) = run(CLUSTER_IDENTIFY, &query, &mut state, 0);

        assert!(
            !outcome.has_reply,
            "the spec asks an idle device not to answer"
        );
    }

    #[test]
    fn writing_the_attribute_starts_identifying_too() {
        let mut state = State::default();
        let mut write = vec![0x00, 0x44, CMD_WRITE_ATTRIBUTES];
        write.extend_from_slice(&ATTR_IDENTIFY_TIME.to_le_bytes());
        write.push(TYPE_UINT16);
        write.extend_from_slice(&7u16.to_le_bytes());

        let (outcome, reply) = run(CLUSTER_IDENTIFY, &write, &mut state, 0);

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

        let (_, reply) = run(CLUSTER_IDENTIFY, &write, &mut state, 0);

        assert_eq!(reply[3], STATUS_UNSUPPORTED_ATTRIBUTE);
    }

    #[test]
    fn a_move_to_level_without_a_level_is_refused() {
        let mut state = State::default();
        let truncated = vec![0x01, 0x61, LEVEL_MOVE_TO_LEVEL];

        let (outcome, reply) = run(CLUSTER_LEVEL_CONTROL, &truncated, &mut state, 0);

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

        let (_, reply) = run(CLUSTER_LEVEL_CONTROL, &configure, &mut state, 0);

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
        let (_, reply) = run(CLUSTER_LEVEL_CONTROL, &read, &mut state, 0);

        assert_eq!(reply[5], STATUS_SUCCESS);
        assert_eq!(reply[6], TYPE_UINT8);
        assert_eq!(reply[7], 33);
    }

    fn read(cluster: u16, attribute: u16, state: &mut State) -> Vec<u8> {
        let mut request = vec![0x00, 0x70, CMD_READ_ATTRIBUTES];
        request.extend_from_slice(&attribute.to_le_bytes());
        let (_, reply) = run(cluster, &request, state, 0);
        reply
    }

    #[test]
    fn the_capabilities_claim_the_xy_space_a_bridge_needs_to_offer_colour() {
        let mut state = State::default();
        let reply = read(CLUSTER_COLOUR_CONTROL, ATTR_COLOUR_CAPABILITIES, &mut state);

        assert_eq!(reply[5], STATUS_SUCCESS);
        assert_eq!(reply[6], TYPE_BITMAP16);
        assert_eq!(u16::from_le_bytes([reply[7], reply[8]]), 0x0019);
    }

    #[test]
    fn a_move_to_a_point_in_the_xy_space_is_taken_and_read_back() {
        let mut state = State::default();
        let mut request = vec![0x01, 0x71, COLOUR_MOVE_TO_XY];
        request.extend_from_slice(&0x2710u16.to_le_bytes());
        request.extend_from_slice(&0x4e20u16.to_le_bytes());
        request.extend_from_slice(&0u16.to_le_bytes());

        let (outcome, _) = run(CLUSTER_COLOUR_CONTROL, &request, &mut state, 0);

        assert!(outcome.changed.colour);
        assert_eq!(state.colour_mode, COLOUR_MODE_XY);

        let x = read(CLUSTER_COLOUR_CONTROL, ATTR_CURRENT_X, &mut state);
        let y = read(CLUSTER_COLOUR_CONTROL, ATTR_CURRENT_Y, &mut state);
        assert_eq!(x[6], TYPE_UINT16);
        assert_eq!(u16::from_le_bytes([x[7], x[8]]), 0x2710);
        assert_eq!(u16::from_le_bytes([y[7], y[8]]), 0x4e20);
    }

    #[test]
    fn a_move_to_a_point_without_both_axes_is_refused() {
        let mut state = State::default();
        let mut truncated = vec![0x01, 0x72, COLOUR_MOVE_TO_XY];
        truncated.extend_from_slice(&0x2710u16.to_le_bytes());

        let (_, reply) = run(CLUSTER_COLOUR_CONTROL, &truncated, &mut state, 0);

        assert_eq!(reply[4], STATUS_INVALID_FIELD);
        assert_eq!(state.colour_mode, COLOUR_MODE_TEMPERATURE);
    }

    #[test]
    fn the_mired_range_is_readable_so_a_bridge_need_not_assume_one() {
        let mut state = State::default();

        let coolest = read(
            CLUSTER_COLOUR_CONTROL,
            ATTR_TEMPERATURE_MIN_MIREDS,
            &mut state,
        );
        assert_eq!(u16::from_le_bytes([coolest[7], coolest[8]]), COOLEST_MIREDS);

        let warmest = read(
            CLUSTER_COLOUR_CONTROL,
            ATTR_TEMPERATURE_MAX_MIREDS,
            &mut state,
        );
        assert_eq!(u16::from_le_bytes([warmest[7], warmest[8]]), WARMEST_MIREDS);
    }

    #[test]
    fn the_colour_mode_says_which_of_the_three_is_live() {
        let mut state = State::default();
        let white = read(CLUSTER_COLOUR_CONTROL, ATTR_COLOUR_MODE, &mut state);
        assert_eq!(white[6], TYPE_ENUM8);
        assert_eq!(white[7], COLOUR_MODE_TEMPERATURE);

        state.set_hue_and_saturation(100, 200);
        let wheel = read(CLUSTER_COLOUR_CONTROL, ATTR_COLOUR_MODE, &mut state);
        assert_eq!(wheel[7], COLOUR_MODE_HUE_SATURATION);

        state.set_xy(0x2710, 0x4e20);
        let point = read(CLUSTER_COLOUR_CONTROL, ATTR_COLOUR_MODE, &mut state);
        assert_eq!(point[7], COLOUR_MODE_XY);
    }

    #[test]
    fn a_scene_remembers_the_point_it_was_captured_at() {
        let mut state = State::default();
        state.set_xy(0x2710, 0x4e20);
        assert_eq!(state.store_scene(0, 3), STATUS_SUCCESS);

        state.set_mireds(300);
        assert!(state.recall_scene(0, 3).is_some());

        assert_eq!(state.colour_mode, COLOUR_MODE_XY);
        assert_eq!((state.x, state.y), (0x2710, 0x4e20));
    }

    #[test]
    fn a_colour_attribute_is_not_reportable_and_says_so() {
        let mut state = State::default();
        let mut configure = vec![0x00, 0x71, CMD_CONFIGURE_REPORTING, DIRECTION_REPORT];
        configure.extend_from_slice(&ATTR_CURRENT_HUE.to_le_bytes());
        configure.push(TYPE_UINT8);
        configure.extend_from_slice(&1u16.to_le_bytes());
        configure.extend_from_slice(&60u16.to_le_bytes());
        configure.push(1);

        let (_, reply) = run(CLUSTER_COLOUR_CONTROL, &configure, &mut state, 0);

        assert_eq!(reply[3], STATUS_UNSUPPORTED_ATTRIBUTE);
    }

    #[test]
    fn the_membership_response_lists_every_group_and_the_room_left() {
        let mut state = State::default();
        run(
            CLUSTER_GROUPS,
            &[0x01, 0x80, GROUP_ADD, 0x07, 0x00, 0x00],
            &mut state,
            0,
        );
        run(
            CLUSTER_GROUPS,
            &[0x01, 0x81, GROUP_ADD, 0x09, 0x00, 0x00],
            &mut state,
            0,
        );

        let (_, reply) = run(
            CLUSTER_GROUPS,
            &[0x01, 0x82, GROUP_GET_MEMBERSHIP, 0],
            &mut state,
            0,
        );

        assert_eq!(reply[2], GROUP_GET_MEMBERSHIP);
        assert_eq!(reply[3], (MAX_GROUPS - 2) as u8, "two of four slots taken");
        assert_eq!(reply[4], 2);
        assert_eq!(u16::from_le_bytes([reply[5], reply[6]]), 7);
        assert_eq!(u16::from_le_bytes([reply[7], reply[8]]), 9);
    }

    #[test]
    fn a_second_add_of_the_same_group_says_it_is_already_there() {
        let mut state = State::default();
        run(
            CLUSTER_GROUPS,
            &[0x01, 0x83, GROUP_ADD, 0x07, 0x00, 0x00],
            &mut state,
            0,
        );

        let (_, reply) = run(
            CLUSTER_GROUPS,
            &[0x01, 0x84, GROUP_ADD, 0x07, 0x00, 0x00],
            &mut state,
            0,
        );

        assert_eq!(reply[3], STATUS_DUPLICATE_EXISTS);
    }

    #[test]
    fn viewing_a_scene_gives_back_the_setting_it_holds() {
        let mut state = State::default();
        state.settle(200, true);
        state.set_hue_and_saturation(60, 180);
        run(
            CLUSTER_SCENES,
            &[0x01, 0x85, SCENE_STORE, 0x00, 0x00, 0x01],
            &mut state,
            0,
        );

        let (_, reply) = run(
            CLUSTER_SCENES,
            &[0x01, 0x86, SCENE_VIEW, 0x00, 0x00, 0x01],
            &mut state,
            0,
        );

        assert_eq!(reply[3], STATUS_SUCCESS);
        assert_eq!(reply[6], 0x01, "the scene it was asked about");
        assert_eq!(reply[9], 0, "no name, because names are not supported");

        let fields = &reply[10..];
        assert_eq!(&fields[..4], &[0x06, 0x00, 0x01, 0x01], "on");
        assert_eq!(&fields[4..8], &[0x08, 0x00, 0x01, 200], "the brightness");
        assert_eq!(
            &fields[8..11],
            &[0x00, 0x03, 0x0d],
            "thirteen octets of colour"
        );
        assert_eq!(
            u16::from_le_bytes([fields[15], fields[16]]) >> 8,
            60,
            "the hue"
        );
        assert_eq!(fields[17], 180, "the saturation");
    }

    #[test]
    fn the_attributes_say_which_scene_is_live_and_whether_it_still_is() {
        let mut state = State::default();
        run(
            CLUSTER_SCENES,
            &[0x01, 0x87, SCENE_STORE, 0x00, 0x00, 0x04],
            &mut state,
            0,
        );

        let count = read(CLUSTER_SCENES, ATTR_SCENE_COUNT, &mut state);
        assert_eq!(count[7], 1);

        run(
            CLUSTER_SCENES,
            &[0x01, 0x88, SCENE_RECALL, 0x00, 0x00, 0x04],
            &mut state,
            0,
        );
        assert_eq!(read(CLUSTER_SCENES, ATTR_CURRENT_SCENE, &mut state)[7], 4);
        assert_eq!(read(CLUSTER_SCENES, ATTR_SCENE_VALID, &mut state)[7], 1);

        state.settle(3, false);
        assert_eq!(
            read(CLUSTER_SCENES, ATTR_SCENE_VALID, &mut state)[7],
            0,
            "moving the light by hand leaves the scene behind"
        );
    }

    #[test]
    fn reading_the_attribute_reports_the_remaining_time() {
        let mut state = State::default();
        run(CLUSTER_IDENTIFY, &identify_command(60), &mut state, 0);

        let mut read = vec![0x00, 0x46, CMD_READ_ATTRIBUTES];
        read.extend_from_slice(&ATTR_IDENTIFY_TIME.to_le_bytes());
        let (_, reply) = run(CLUSTER_IDENTIFY, &read, &mut state, 20_000);

        assert_eq!(reply[5], STATUS_SUCCESS);
        assert_eq!(reply[6], TYPE_UINT16);
        assert_eq!(u16::from_le_bytes([reply[7], reply[8]]), 40);
    }
    #[test]
    fn a_command_that_wanted_an_answer_gets_the_default_response() {
        let mut state = State::default();
        let off = vec![0x01, 0x42, ON_OFF_OFF];
        let (outcome, reply) = run(CLUSTER_ON_OFF, &off, &mut state, 0);

        assert!(outcome.has_reply);
        assert_eq!(
            reply,
            vec![
                FC_FROM_SERVER | FC_DISABLE_DEFAULT_RESPONSE,
                0x42,
                CMD_DEFAULT_RESPONSE,
                ON_OFF_OFF,
                STATUS_SUCCESS
            ]
        );
    }

    #[test]
    fn a_command_that_refused_an_answer_gets_none() {
        let mut state = State::default();
        let off = vec![0x01 | FC_DISABLE_DEFAULT_RESPONSE, 0x43, ON_OFF_OFF];
        let (outcome, _) = run(CLUSTER_ON_OFF, &off, &mut state, 0);

        assert!(!outcome.has_reply);
    }

    #[test]
    fn a_command_that_failed_answers_even_when_an_answer_was_refused() {
        let mut state = State::default();
        let truncated = vec![
            0x01 | FC_DISABLE_DEFAULT_RESPONSE,
            0x44,
            LEVEL_MOVE_TO_LEVEL,
        ];
        let (outcome, reply) = run(CLUSTER_LEVEL_CONTROL, &truncated, &mut state, 0);

        assert!(
            outcome.has_reply,
            "a failure is reported whatever was asked"
        );
        assert_eq!(reply[2], CMD_DEFAULT_RESPONSE);
        assert_eq!(reply[4], STATUS_INVALID_FIELD);
    }
}
