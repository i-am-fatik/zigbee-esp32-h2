use crate::buf::{Reader, Writer};
use crate::Instant;

pub const CLUSTER: u16 = 0x0019;

pub const IMAGE_NOTIFY: u8 = 0x00;
pub const QUERY_NEXT_IMAGE_REQUEST: u8 = 0x01;
pub const QUERY_NEXT_IMAGE_RESPONSE: u8 = 0x02;
pub const IMAGE_BLOCK_REQUEST: u8 = 0x03;
pub const IMAGE_BLOCK_RESPONSE: u8 = 0x05;
pub const UPGRADE_END_REQUEST: u8 = 0x06;
pub const UPGRADE_END_RESPONSE: u8 = 0x07;

const STATUS_SUCCESS: u8 = 0x00;
const STATUS_ABORT: u8 = 0x95;
const STATUS_WAIT_FOR_DATA: u8 = 0x97;

/// The most image bytes that fit beside a block response's own header in one
/// secured frame, with room to spare rather than room exactly.
pub const BLOCK: usize = 48;

/// Enough of the file to hold the upgrade header and the sub-element header
/// that follows it, which together say where the firmware itself begins.
const PREAMBLE: usize = 128;

const FILE_MAGIC: u32 = 0x0bee_f11e;
const TAG_UPGRADE_IMAGE: u16 = 0x0000;
const SUB_ELEMENT: u32 = 6;

const RETRY_MS: u32 = 5_000;

/// Which firmware the device is running, which is what a server needs to decide
/// whether it has anything newer.
#[derive(Clone, Copy, Debug)]
pub struct Identity {
    pub manufacturer: u16,
    pub image_type: u16,
    pub version: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Querying,
    Downloading,
    Finishing,
}

pub struct State {
    phase: Phase,
    server: u16,
    due_at: Instant,
    offered: u32,
    file_size: u32,
    file_offset: u32,
    preamble: [u8; PREAMBLE],
    preamble_len: usize,
    image_at: Option<u32>,
    image_size: u32,
    taken: u32,
}

/// What the stack wants said next, decided by [`State::due`] and encoded by the
/// matching builder.
pub enum Wanted {
    Query,
    Block,
    End(u8),
}

impl Default for State {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            server: 0x0000,
            due_at: Instant::from_millis(0),
            offered: 0,
            file_size: 0,
            file_offset: 0,
            preamble: [0u8; PREAMBLE],
            preamble_len: 0,
            image_at: None,
            image_size: 0,
            taken: 0,
        }
    }
}

impl State {
    pub const fn server(&self) -> u16 {
        self.server
    }

    pub const fn running(&self) -> bool {
        !matches!(self.phase, Phase::Idle)
    }

    /// Asks for an image, which is what a device does once it is on a network
    /// and again whenever a server says something is available.
    pub fn ask(&mut self, now: Instant) {
        if self.running() {
            return;
        }
        self.phase = Phase::Querying;
        self.due_at = now;
    }

    pub fn stop(&mut self) {
        self.phase = Phase::Idle;
        self.image_at = None;
        self.preamble_len = 0;
        self.taken = 0;
    }

    /// What to send now, if anything. A block is only asked for once the caller
    /// has taken the one before it, so an undrained caller stalls the download
    /// rather than losing part of it.
    pub fn due(&mut self, now: Instant, block_pending: bool) -> Option<Wanted> {
        if !now.reached(self.due_at) {
            return None;
        }
        self.due_at = now.plus_millis(RETRY_MS);
        match self.phase {
            Phase::Idle => None,
            Phase::Querying => Some(Wanted::Query),
            Phase::Downloading if block_pending => None,
            Phase::Downloading => Some(Wanted::Block),
            Phase::Finishing => Some(Wanted::End(STATUS_SUCCESS)),
        }
    }

    pub fn build(&self, out: &mut Writer, seq: u8, wanted: &Wanted, identity: &Identity) {
        match wanted {
            Wanted::Query => {
                header(out, seq, QUERY_NEXT_IMAGE_REQUEST);
                out.u8(0x00);
                out.u16(identity.manufacturer);
                out.u16(identity.image_type);
                out.u32(identity.version);
            }
            Wanted::Block => {
                header(out, seq, IMAGE_BLOCK_REQUEST);
                out.u8(0x00);
                out.u16(identity.manufacturer);
                out.u16(identity.image_type);
                out.u32(self.offered);
                out.u32(self.file_offset);
                out.u8(BLOCK as u8);
            }
            Wanted::End(status) => {
                header(out, seq, UPGRADE_END_REQUEST);
                out.u8(*status);
                out.u16(identity.manufacturer);
                out.u16(identity.image_type);
                out.u32(self.offered);
            }
        }
    }
}

/// What handling a server's command produced, beyond the state change itself.
pub enum Outcome {
    Nothing,
    Offered { version: u32, size: u32 },
    Ready,
    Abandoned,
}

impl State {
    pub fn receive(
        &mut self,
        source: u16,
        request: &crate::zcl::Incoming,
        block: &mut Block,
        now: Instant,
    ) -> Outcome {
        if !request.from_server {
            return Outcome::Nothing;
        }
        let mut r = Reader::new(request.payload);

        match request.command {
            IMAGE_NOTIFY => {
                self.server = source;
                self.ask(now);
                Outcome::Nothing
            }
            QUERY_NEXT_IMAGE_RESPONSE => self.on_offer(source, &mut r, now),
            IMAGE_BLOCK_RESPONSE => self.on_block(&mut r, block, now),
            UPGRADE_END_RESPONSE if self.phase == Phase::Finishing => {
                self.phase = Phase::Idle;
                Outcome::Ready
            }
            _ => Outcome::Nothing,
        }
    }

    fn on_offer(&mut self, source: u16, r: &mut Reader, now: Instant) -> Outcome {
        if self.phase != Phase::Querying {
            return Outcome::Nothing;
        }
        let Some(STATUS_SUCCESS) = r.u8() else {
            self.stop();
            return Outcome::Nothing;
        };
        let (Some(_manufacturer), Some(_image_type), Some(version), Some(size)) =
            (r.u16(), r.u16(), r.u32(), r.u32())
        else {
            self.stop();
            return Outcome::Nothing;
        };

        self.server = source;
        self.offered = version;
        self.file_size = size;
        self.file_offset = 0;
        self.preamble_len = 0;
        self.image_at = None;
        self.taken = 0;
        self.phase = Phase::Downloading;
        self.due_at = now;
        Outcome::Offered { version, size }
    }

    fn on_block(&mut self, r: &mut Reader, block: &mut Block, now: Instant) -> Outcome {
        if self.phase != Phase::Downloading {
            return Outcome::Nothing;
        }
        let Some(status) = r.u8() else {
            return Outcome::Nothing;
        };
        if status == STATUS_WAIT_FOR_DATA {
            self.due_at = now.plus_millis(RETRY_MS);
            return Outcome::Nothing;
        }
        if status != STATUS_SUCCESS {
            self.stop();
            return Outcome::Abandoned;
        }

        let (Some(_manufacturer), Some(_image_type), Some(_version), Some(offset), Some(len)) =
            (r.u16(), r.u16(), r.u32(), r.u32(), r.u8())
        else {
            return Outcome::Nothing;
        };
        let Some(data) = r.take(len as usize) else {
            return Outcome::Nothing;
        };
        if offset != self.file_offset {
            return Outcome::Nothing;
        }

        self.file_offset += len as u32;
        self.due_at = now;
        self.absorb(offset, data, block);

        if self.taken >= self.image_size && self.image_at.is_some() {
            self.phase = Phase::Finishing;
            self.due_at = now;
        }
        Outcome::Nothing
    }

    /// Splits an arriving block into the part that is still upgrade header and
    /// the part that is firmware, and hands only the second to the caller.
    fn absorb(&mut self, offset: u32, data: &[u8], block: &mut Block) {
        if self.image_at.is_none() {
            let room = PREAMBLE - self.preamble_len;
            let keep = data.len().min(room);
            self.preamble[self.preamble_len..self.preamble_len + keep]
                .copy_from_slice(&data[..keep]);
            self.preamble_len += keep;

            let Some(start) = image_start(&self.preamble[..self.preamble_len]) else {
                return;
            };
            self.image_at = Some(start.at);
            self.image_size = start.size;

            let from = start.at as usize;
            if from < self.preamble_len {
                block.put(0, &self.preamble[from..self.preamble_len]);
                self.taken = (self.preamble_len - from) as u32;
            }
            return;
        }

        let Some(image_at) = self.image_at else {
            return;
        };
        let end = image_at + self.image_size;
        let from = offset.max(image_at);
        let to = (offset + data.len() as u32).min(end);
        if to <= from {
            return;
        }
        let slice = &data[(from - offset) as usize..(to - offset) as usize];
        block.put(from - image_at, slice);
        self.taken = to - image_at;
    }
}

/// The one block of firmware waiting for the caller to write it down.
pub struct Block {
    pub offset: u32,
    pub buffer: [u8; PREAMBLE],
    pub len: usize,
}

impl Default for Block {
    fn default() -> Self {
        Self {
            offset: 0,
            buffer: [0u8; PREAMBLE],
            len: 0,
        }
    }
}

impl Block {
    fn put(&mut self, offset: u32, data: &[u8]) {
        let len = data.len().min(PREAMBLE);
        self.buffer[..len].copy_from_slice(&data[..len]);
        self.offset = offset;
        self.len = len;
    }

    pub const fn pending(&self) -> bool {
        self.len > 0
    }
}

struct ImageStart {
    at: u32,
    size: u32,
}

/// Reads the upgrade file header and the sub-element that follows it, which is
/// the only way to know where the firmware inside the file begins.
fn image_start(preamble: &[u8]) -> Option<ImageStart> {
    let mut r = Reader::new(preamble);
    if r.u32()? != FILE_MAGIC {
        return None;
    }
    r.u16()?;
    let header_len = r.u16()? as u32;
    if header_len as usize + SUB_ELEMENT as usize > preamble.len() {
        return None;
    }

    let mut r = Reader::new(preamble.get(header_len as usize..)?);
    if r.u16()? != TAG_UPGRADE_IMAGE {
        return None;
    }
    Some(ImageStart {
        at: header_len + SUB_ELEMENT,
        size: r.u32()?,
    })
}

fn header(out: &mut Writer, seq: u8, command: u8) {
    out.u8(0x11);
    out.u8(seq);
    out.u8(command);
}

pub const fn abort() -> u8 {
    STATUS_ABORT
}
