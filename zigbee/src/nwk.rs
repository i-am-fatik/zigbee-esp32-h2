use crate::buf::{Reader, Writer};
use crate::crypto::{ccm_star_decrypt, ccm_star_encrypt, KEY_LEN, MIC_LEN, NONCE_LEN};

pub const BROADCAST_RX_ON_WHEN_IDLE: u16 = 0xfffd;
pub const COORDINATOR: u16 = 0x0000;

pub const FRAME_TYPE_DATA: u16 = 0;
pub const FRAME_TYPE_COMMAND: u16 = 1;

pub const CMD_LEAVE: u8 = 0x04;
pub const CMD_SWITCH_KEY: u8 = 0x05;
pub const CMD_REJOIN_REQUEST: u8 = 0x06;
pub const CMD_REJOIN_RESPONSE: u8 = 0x07;

pub const DEFAULT_RADIUS: u8 = 30;

const PROTOCOL_VERSION: u16 = 2;
const FCF_MULTICAST: u16 = 1 << 8;
const FCF_SECURITY: u16 = 1 << 9;
const FCF_SOURCE_ROUTE: u16 = 1 << 10;
const FCF_DEST_IEEE: u16 = 1 << 11;
const FCF_SOURCE_IEEE: u16 = 1 << 12;

/// Security level 5 (encryption plus a 32 bit integrity code) is the only level
/// Zigbee PRO uses. It is stripped from the octet that goes on the air and
/// restored by the receiver before the integrity code is checked.
const SECURITY_LEVEL: u8 = 0x05;
const KEY_ID_NETWORK: u8 = 1 << 3;
const EXTENDED_NONCE: u8 = 1 << 5;

const AUX_LEN: usize = 1 + 4 + 8 + 1;
const HEADER_LEN: usize = 2 + 2 + 2 + 1 + 1;
pub const MAX_PAYLOAD_LEN: usize =
    crate::mac::MAX_PAYLOAD_LEN - HEADER_LEN - AUX_LEN - crate::crypto::MIC_LEN;

pub struct Header {
    pub frame_type: u16,
    pub dst: u16,
    pub src: u16,
    pub radius: u8,
    pub seq: u8,
    pub src_ieee: Option<u64>,
}

pub struct Frame {
    pub frame_type: u16,
    pub dst: u16,
    pub src: u16,
    pub secured: bool,
    pub header_len: usize,
}

fn nonce(source: u64, counter: u32, security_control: u8) -> [u8; NONCE_LEN] {
    let mut nonce = [0u8; NONCE_LEN];
    nonce[0..8].copy_from_slice(&source.to_le_bytes());
    nonce[8..12].copy_from_slice(&counter.to_le_bytes());
    nonce[12] = security_control;
    nonce
}

pub fn build_secured(
    out: &mut Writer,
    header: Header,
    key: &[u8; KEY_LEN],
    key_sequence: u8,
    counter: u32,
    source_ieee: u64,
    payload: &[u8],
) {
    let start = out.len();

    let mut fcf = header.frame_type | (PROTOCOL_VERSION << 2) | FCF_SECURITY;
    if header.src_ieee.is_some() {
        fcf |= FCF_SOURCE_IEEE;
    }
    out.u16(fcf);
    out.u16(header.dst);
    out.u16(header.src);
    out.u8(header.radius);
    out.u8(header.seq);
    if let Some(ieee) = header.src_ieee {
        out.u64(ieee);
    }

    let aux_start = out.len();
    let security_control = SECURITY_LEVEL | KEY_ID_NETWORK | EXTENDED_NONCE;
    out.u8(security_control);
    out.u32(counter);
    out.u64(source_ieee);
    out.u8(key_sequence);

    let mut authenticated = [0u8; 64];
    let authenticated_len = out.len() - start;
    let Some(written) = out.written() else {
        return;
    };
    authenticated[..authenticated_len].copy_from_slice(&written[start..]);

    let mut body = [0u8; MAX_PAYLOAD_LEN];
    body[..payload.len()].copy_from_slice(payload);
    let body = &mut body[..payload.len()];

    let Some(mic) = ccm_star_encrypt(
        key,
        &nonce(source_ieee, counter, security_control),
        &authenticated[..authenticated_len],
        body,
    ) else {
        return;
    };

    out.set(aux_start, security_control & !0x07);
    out.bytes(body);
    out.bytes(&mic);
}

pub fn parse(input: &[u8]) -> Option<Frame> {
    let mut r = Reader::new(input);
    let fcf = r.u16()?;
    let dst = r.u16()?;
    let src = r.u16()?;
    r.u8()?;
    r.u8()?;
    if fcf & FCF_DEST_IEEE != 0 {
        r.skip(8)?;
    }
    if fcf & FCF_SOURCE_IEEE != 0 {
        r.skip(8)?;
    }
    if fcf & FCF_MULTICAST != 0 {
        r.skip(1)?;
    }
    if fcf & FCF_SOURCE_ROUTE != 0 {
        let relay_count = r.u8()? as usize;
        r.skip(1 + relay_count * 2)?;
    }

    let header_len = input.len() - r.remaining();
    Some(Frame {
        frame_type: fcf & 0b11,
        dst,
        src,
        secured: fcf & FCF_SECURITY != 0,
        header_len,
    })
}

/// The sequence number of the key a frame was secured with, read out of the
/// auxiliary header so a receiver can pick between the key it is using and one
/// the trust centre has sent but not switched to yet.
pub fn key_sequence(frame: &[u8], aux_start: usize) -> Option<u8> {
    frame.get(aux_start + AUX_LEN - 1).copied()
}

/// The sequence number the trust centre is telling the network to move to.
pub fn parse_switch_key(body: &[u8]) -> Option<u8> {
    let mut r = Reader::new(body);
    if r.u8()? != CMD_SWITCH_KEY {
        return None;
    }
    r.u8()
}

pub struct RejoinResponse {
    pub short_address: u16,
    pub status: u8,
}

pub fn parse_rejoin_response(body: &[u8]) -> Option<RejoinResponse> {
    let mut r = Reader::new(body);
    if r.u8()? != CMD_REJOIN_RESPONSE {
        return None;
    }
    Some(RejoinResponse {
        short_address: r.u16()?,
        status: r.u8()?,
    })
}

pub struct Unsecured {
    pub offset: usize,
    pub len: usize,
}

/// Decrypts a secured network frame in place. `frame` holds the whole network
/// frame and `aux_start` points at its auxiliary security header.
pub fn unsecure(frame: &mut [u8], aux_start: usize, key: &[u8; KEY_LEN]) -> Option<Unsecured> {
    if frame.len() < aux_start + AUX_LEN + MIC_LEN {
        return None;
    }

    let mut r = Reader::new(&frame[aux_start..]);
    let security_control = r.u8()? | SECURITY_LEVEL;
    let counter = r.u32()?;
    if security_control & EXTENDED_NONCE == 0 || security_control & KEY_ID_NETWORK == 0 {
        return None;
    }
    let source = r.u64()?;

    let authenticated_len = aux_start + AUX_LEN;
    let mut authenticated = [0u8; crate::mac::MAX_FRAME_LEN];
    authenticated[..authenticated_len].copy_from_slice(&frame[..authenticated_len]);
    authenticated[aux_start] = security_control;

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
