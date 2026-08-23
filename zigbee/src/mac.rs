use crate::buf::{Reader, Writer};

/// The longest MAC frame IEEE 802.15.4 allows, counted the way this crate
/// counts frames: without the physical length byte and without the checksum.
pub const MAX_FRAME: usize = 127;

pub const BROADCAST_PAN: u16 = 0xffff;
pub const BROADCAST_SHORT: u16 = 0xffff;

pub const FRAME_TYPE_BEACON: u16 = 0;
pub const FRAME_TYPE_DATA: u16 = 1;
pub const FRAME_TYPE_COMMAND: u16 = 3;

const FCF_SECURITY: u16 = 1 << 3;
const FCF_ACK_REQUEST: u16 = 1 << 5;
const FCF_PAN_COMPRESSION: u16 = 1 << 6;

const ADDR_MODE_NONE: u16 = 0;
const ADDR_MODE_SHORT: u16 = 2;
const ADDR_MODE_EXTENDED: u16 = 3;

pub const CMD_ASSOCIATION_REQUEST: u8 = 0x01;
pub const CMD_ASSOCIATION_RESPONSE: u8 = 0x02;
pub const CMD_DATA_REQUEST: u8 = 0x04;
pub const CMD_BEACON_REQUEST: u8 = 0x07;

/// Capability information advertised in an association request: an end device
/// that is mains powered, keeps its receiver on and wants an allocated address.
pub const CAPABILITY_MAINS_END_DEVICE: u8 = 0x8c;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Addr {
    None,
    Short(u16),
    Extended(u64),
}

impl Addr {
    fn mode(&self) -> u16 {
        match self {
            Addr::None => ADDR_MODE_NONE,
            Addr::Short(_) => ADDR_MODE_SHORT,
            Addr::Extended(_) => ADDR_MODE_EXTENDED,
        }
    }

    fn write(&self, out: &mut Writer) {
        match self {
            Addr::None => {}
            Addr::Short(v) => {
                out.u16(*v);
            }
            Addr::Extended(v) => {
                out.u64(*v);
            }
        }
    }

    fn read(input: &mut Reader, mode: u16) -> Option<Addr> {
        match mode {
            ADDR_MODE_NONE => Some(Addr::None),
            ADDR_MODE_SHORT => Some(Addr::Short(input.u16()?)),
            ADDR_MODE_EXTENDED => Some(Addr::Extended(input.u64()?)),
            _ => None,
        }
    }
}

pub struct Header {
    pub frame_type: u16,
    pub ack_request: bool,
    pub security: bool,
    pub seq: u8,
    pub dst_pan: Option<u16>,
    pub dst: Addr,
    pub src_pan: Option<u16>,
    pub src: Addr,
}

impl Header {
    pub fn write(&self, out: &mut Writer) {
        let mut fcf = self.frame_type;
        if self.security {
            fcf |= FCF_SECURITY;
        }
        if self.ack_request {
            fcf |= FCF_ACK_REQUEST;
        }
        if self.src_pan.is_none() && self.src.mode() != ADDR_MODE_NONE {
            fcf |= FCF_PAN_COMPRESSION;
        }
        fcf |= self.dst.mode() << 10;
        fcf |= self.src.mode() << 14;

        out.u16(fcf);
        out.u8(self.seq);
        if let Some(pan) = self.dst_pan {
            out.u16(pan);
        }
        self.dst.write(out);
        if let Some(pan) = self.src_pan {
            out.u16(pan);
        }
        self.src.write(out);
    }
}

pub struct Frame<'a> {
    pub frame_type: u16,
    pub dst_pan: Option<u16>,
    pub src_pan: Option<u16>,
    pub src: Addr,
    pub payload: &'a [u8],
}

pub fn parse(input: &[u8]) -> Option<Frame<'_>> {
    let mut r = Reader::new(input);
    let fcf = r.u16()?;
    r.u8()?;

    let dst_mode = (fcf >> 10) & 0b11;
    let src_mode = (fcf >> 14) & 0b11;
    let pan_compressed = fcf & FCF_PAN_COMPRESSION != 0;

    let dst_pan = if dst_mode != ADDR_MODE_NONE {
        Some(r.u16()?)
    } else {
        None
    };
    Addr::read(&mut r, dst_mode)?;

    let src_pan = if src_mode != ADDR_MODE_NONE && !pan_compressed {
        Some(r.u16()?)
    } else {
        dst_pan.filter(|_| pan_compressed)
    };
    let src = Addr::read(&mut r, src_mode)?;

    Some(Frame {
        frame_type: fcf & 0b111,
        dst_pan,
        src_pan,
        src,
        payload: r.rest(),
    })
}

/// Everything a joining device needs out of a beacon: who answered, on which
/// network, and whether that network is currently accepting new devices.
pub struct Beacon {
    pub pan_id: u16,
    pub source: Addr,
    pub association_permit: bool,
    pub stack_profile: u8,
    pub end_device_capacity: bool,
}

pub fn parse_beacon(frame: &Frame) -> Option<Beacon> {
    let mut r = Reader::new(frame.payload);
    let superframe = r.u16()?;
    let gts_spec = r.u8()?;
    if gts_spec & 0b111 != 0 {
        return None;
    }
    let pending = r.u8()?;
    let pending_short = (pending & 0b111) as usize;
    let pending_extended = ((pending >> 4) & 0b111) as usize;
    r.skip(pending_short * 2 + pending_extended * 8)?;

    let protocol_id = r.u8()?;
    if protocol_id != 0 {
        return None;
    }
    let profile_and_version = r.u8()?;
    let capacity = r.u8()?;
    Some(Beacon {
        pan_id: frame.src_pan.or(frame.dst_pan)?,
        source: frame.src,
        association_permit: superframe & (1 << 15) != 0,
        stack_profile: profile_and_version & 0x0f,
        end_device_capacity: capacity & 0x80 != 0,
    })
}

pub fn beacon_request(out: &mut Writer, seq: u8) {
    Header {
        frame_type: FRAME_TYPE_COMMAND,
        ack_request: false,
        security: false,
        seq,
        dst_pan: Some(BROADCAST_PAN),
        dst: Addr::Short(BROADCAST_SHORT),
        src_pan: None,
        src: Addr::None,
    }
    .write(out);
    out.u8(CMD_BEACON_REQUEST);
}

pub fn association_request(out: &mut Writer, seq: u8, pan_id: u16, coordinator: Addr, us: u64) {
    Header {
        frame_type: FRAME_TYPE_COMMAND,
        ack_request: true,
        security: false,
        seq,
        dst_pan: Some(pan_id),
        dst: coordinator,
        src_pan: Some(BROADCAST_PAN),
        src: Addr::Extended(us),
    }
    .write(out);
    out.u8(CMD_ASSOCIATION_REQUEST);
    out.u8(CAPABILITY_MAINS_END_DEVICE);
}

pub fn data_request(out: &mut Writer, seq: u8, pan_id: u16, coordinator: Addr, us: Addr) {
    Header {
        frame_type: FRAME_TYPE_COMMAND,
        ack_request: true,
        security: false,
        seq,
        dst_pan: Some(pan_id),
        dst: coordinator,
        src_pan: None,
        src: us,
    }
    .write(out);
    out.u8(CMD_DATA_REQUEST);
}

pub struct AssociationResponse {
    pub short_addr: u16,
    pub status: u8,
}

pub fn parse_association_response(frame: &Frame) -> Option<AssociationResponse> {
    let mut r = Reader::new(frame.payload);
    if r.u8()? != CMD_ASSOCIATION_RESPONSE {
        return None;
    }
    Some(AssociationResponse {
        short_addr: r.u16()?,
        status: r.u8()?,
    })
}

pub fn data(out: &mut Writer, seq: u8, pan_id: u16, dst: Addr, src: Addr, ack: bool) {
    Header {
        frame_type: FRAME_TYPE_DATA,
        ack_request: ack,
        security: false,
        seq,
        dst_pan: Some(pan_id),
        dst,
        src_pan: None,
        src,
    }
    .write(out);
}
