use crate::buf::{Reader, Writer};
use crate::crypto::{ccm_star_decrypt, KEY_LEN, MIC_LEN, NONCE_LEN};

pub const FRAME_TYPE_DATA: u8 = 0;
pub const FRAME_TYPE_COMMAND: u8 = 1;
pub const FRAME_TYPE_ACK: u8 = 2;

pub const DELIVERY_UNICAST: u8 = 0;
pub const DELIVERY_BROADCAST: u8 = 2;

pub const CMD_TRANSPORT_KEY: u8 = 0x05;

pub const KEY_TYPE_STANDARD_NETWORK: u8 = 0x01;

pub const PROFILE_ZDO: u16 = 0x0000;
pub const PROFILE_HOME_AUTOMATION: u16 = 0x0104;

const FC_SECURITY: u8 = 1 << 5;
const FC_ACK_REQUEST: u8 = 1 << 6;
const FC_EXTENDED_HEADER: u8 = 1 << 7;

const SECURITY_LEVEL: u8 = 0x05;
const EXTENDED_NONCE: u8 = 1 << 5;

pub struct DataHeader {
    pub dst_endpoint: u8,
    pub src_endpoint: u8,
    pub cluster: u16,
    pub profile: u16,
    pub counter: u8,
    pub ack_request: bool,
    pub broadcast: bool,
}

pub fn build_data(out: &mut Writer, header: DataHeader, payload: &[u8]) {
    let delivery = if header.broadcast {
        DELIVERY_BROADCAST
    } else {
        DELIVERY_UNICAST
    };
    let mut fc = FRAME_TYPE_DATA | (delivery << 2);
    if header.ack_request {
        fc |= FC_ACK_REQUEST;
    }
    out.u8(fc);
    out.u8(header.dst_endpoint);
    out.u16(header.cluster);
    out.u16(header.profile);
    out.u8(header.src_endpoint);
    out.u8(header.counter);
    out.bytes(payload);
}

pub fn build_ack(out: &mut Writer, to: &Data) {
    out.u8(FRAME_TYPE_ACK | (DELIVERY_UNICAST << 2));
    out.u8(to.src_endpoint);
    out.u16(to.cluster);
    out.u16(to.profile);
    out.u8(to.dst_endpoint);
    out.u8(to.counter);
}

pub struct Data<'a> {
    pub dst_endpoint: u8,
    pub src_endpoint: u8,
    pub cluster: u16,
    pub profile: u16,
    pub counter: u8,
    pub ack_request: bool,
    pub payload: &'a [u8],
}

pub struct Command {
    pub secured: bool,
    pub header_len: usize,
}

pub enum Frame<'a> {
    Data(Data<'a>),
    Command(Command),
    Ack,
}

pub fn parse(input: &[u8]) -> Option<Frame<'_>> {
    let mut r = Reader::new(input);
    let fc = r.u8()?;

    match fc & 0b11 {
        FRAME_TYPE_DATA => {
            let dst_endpoint = r.u8()?;
            let cluster = r.u16()?;
            let profile = r.u16()?;
            let src_endpoint = r.u8()?;
            let counter = r.u8()?;
            if fc & FC_EXTENDED_HEADER != 0 {
                r.skip(2)?;
            }
            Some(Frame::Data(Data {
                dst_endpoint,
                src_endpoint,
                cluster,
                profile,
                counter,
                ack_request: fc & FC_ACK_REQUEST != 0,
                payload: r.rest(),
            }))
        }
        FRAME_TYPE_COMMAND => {
            r.u8()?;
            Some(Frame::Command(Command {
                secured: fc & FC_SECURITY != 0,
                header_len: 2,
            }))
        }
        FRAME_TYPE_ACK => Some(Frame::Ack),
        _ => None,
    }
}

fn nonce(source: u64, counter: u32, security_control: u8) -> [u8; NONCE_LEN] {
    let mut nonce = [0u8; NONCE_LEN];
    nonce[0..8].copy_from_slice(&source.to_le_bytes());
    nonce[8..12].copy_from_slice(&counter.to_le_bytes());
    nonce[12] = security_control;
    nonce
}

pub struct Unsecured {
    pub offset: usize,
    pub len: usize,
}

/// Decrypts a secured application-support frame in place. `frame` holds the
/// whole APS frame and `aux_start` points at its auxiliary security header.
pub fn unsecure(frame: &mut [u8], aux_start: usize, key: &[u8; KEY_LEN]) -> Option<Unsecured> {
    let mut r = Reader::new(frame.get(aux_start..)?);
    let security_control = r.u8()? | SECURITY_LEVEL;
    let counter = r.u32()?;
    let source = if security_control & EXTENDED_NONCE != 0 {
        r.u64()?
    } else {
        return None;
    };
    if (security_control >> 3) & 0b11 == 1 {
        r.u8()?;
    }
    let aux_len = frame[aux_start..].len() - r.remaining();

    let authenticated_len = aux_start + aux_len;
    let mut authenticated = [0u8; 32];
    authenticated[..authenticated_len].copy_from_slice(&frame[..authenticated_len]);
    authenticated[aux_start] = security_control;

    if frame.len() < authenticated_len + MIC_LEN {
        return None;
    }
    let payload_end = frame.len() - MIC_LEN;
    let mut mic = [0u8; MIC_LEN];
    mic.copy_from_slice(&frame[payload_end..]);

    let ok = ccm_star_decrypt(
        key,
        &nonce(source, counter, security_control),
        &authenticated[..authenticated_len],
        &mut frame[authenticated_len..payload_end],
        &mic,
    );
    if !ok {
        return None;
    }

    Some(Unsecured {
        offset: authenticated_len,
        len: payload_end - authenticated_len,
    })
}

pub struct TransportKey {
    pub key_type: u8,
    pub key: [u8; KEY_LEN],
    pub key_seq: u8,
    pub destination: u64,
}

pub fn parse_transport_key(body: &[u8]) -> Option<TransportKey> {
    let mut r = Reader::new(body);
    if r.u8()? != CMD_TRANSPORT_KEY {
        return None;
    }
    let key_type = r.u8()?;
    let key = r.array::<KEY_LEN>()?;
    let key_seq = if key_type == KEY_TYPE_STANDARD_NETWORK {
        r.u8()?
    } else {
        0
    };
    Some(TransportKey {
        key_type,
        key,
        key_seq,
        destination: r.u64()?,
    })
}
