use aes::cipher::{BlockCipherEncrypt, KeyInit};
use aes::Aes128;
use ccm::aead::inout::InOutBuf;
use ccm::aead::AeadInOut;
use ccm::consts::{U13, U4};
use ccm::Ccm;

pub const KEY_LEN: usize = 16;
pub const MIC_LEN: usize = 4;
pub const NONCE_LEN: usize = 13;

const BLOCK: usize = 16;

/// CCM* as Zigbee uses it: a 13 octet nonce, which leaves a 2 octet length
/// field, and a 32 bit integrity code over both the header and the payload.
type ZigbeeCcm = Ccm<Aes128, U4, U13>;

fn encrypt_block(cipher: &Aes128, block: &mut [u8; BLOCK]) {
    cipher.encrypt_block(block.into());
}

fn xor_into(target: &mut [u8], source: &[u8]) {
    for (t, s) in target.iter_mut().zip(source) {
        *t ^= s;
    }
}

pub fn ccm_star_encrypt(
    key: &[u8; KEY_LEN],
    nonce: &[u8; NONCE_LEN],
    header: &[u8],
    payload: &mut [u8],
) -> Option<[u8; MIC_LEN]> {
    let tag = ZigbeeCcm::new(key.into())
        .encrypt_inout_detached(nonce.into(), header, InOutBuf::from(payload))
        .ok()?;
    Some(tag.into())
}

pub fn ccm_star_decrypt(
    key: &[u8; KEY_LEN],
    nonce: &[u8; NONCE_LEN],
    header: &[u8],
    payload: &mut [u8],
    mic: &[u8; MIC_LEN],
) -> bool {
    ZigbeeCcm::new(key.into())
        .decrypt_inout_detached(nonce.into(), header, InOutBuf::from(payload), mic.into())
        .is_ok()
}

/// The Matyas-Meyer-Oseas hash Zigbee builds out of AES-128, used to turn a
/// link key into the specialised keys that protect key transport.
pub fn aes_mmo(message: &[u8]) -> [u8; KEY_LEN] {
    let mut digest = [0u8; KEY_LEN];

    let absorb = |digest: &mut [u8; KEY_LEN], block: &[u8]| {
        let cipher = Aes128::new((&*digest).into());
        let mut encrypted = [0u8; BLOCK];
        encrypted.copy_from_slice(block);
        encrypt_block(&cipher, &mut encrypted);
        xor_into(&mut encrypted, block);
        digest.copy_from_slice(&encrypted);
    };

    let whole_blocks = message.len() / BLOCK;
    for block in message[..whole_blocks * BLOCK].chunks(BLOCK) {
        absorb(&mut digest, block);
    }

    let tail = &message[whole_blocks * BLOCK..];
    let mut padding = [0u8; 2 * BLOCK];
    padding[..tail.len()].copy_from_slice(tail);
    padding[tail.len()] = 0x80;

    let bit_length = (message.len() as u16) * 8;
    let padded_len = if tail.len() + 1 + 2 <= BLOCK {
        BLOCK
    } else {
        2 * BLOCK
    };
    padding[padded_len - 2..padded_len].copy_from_slice(&bit_length.to_be_bytes());

    for block in padding[..padded_len].chunks(BLOCK) {
        absorb(&mut digest, block);
    }
    digest
}

fn hmac_aes_mmo(key: &[u8; KEY_LEN], message: &[u8]) -> [u8; KEY_LEN] {
    let mut inner = [0u8; BLOCK + 1];
    for (slot, byte) in inner[..BLOCK].iter_mut().zip(key) {
        *slot = byte ^ 0x36;
    }
    inner[BLOCK..BLOCK + message.len()].copy_from_slice(message);
    let inner_digest = aes_mmo(&inner[..BLOCK + message.len()]);

    let mut outer = [0u8; 2 * BLOCK];
    for (slot, byte) in outer[..BLOCK].iter_mut().zip(key) {
        *slot = byte ^ 0x5c;
    }
    outer[BLOCK..].copy_from_slice(&inner_digest);
    aes_mmo(&outer)
}

/// Key used by the trust centre to protect a transported network key.
pub fn key_transport_key(link_key: &[u8; KEY_LEN]) -> [u8; KEY_LEN] {
    hmac_aes_mmo(link_key, &[0x00])
}
