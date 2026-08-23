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

pub const INSTALL_CODE_LEN: usize = 16;
pub const INSTALL_CODE_LABEL_LEN: usize = INSTALL_CODE_LEN + 2;

/// CRC-16/X-25 over an install code, which exists so that a person copying the
/// code off a label catches their own mistake rather than a failed join.
fn install_code_crc(code: &[u8; INSTALL_CODE_LEN]) -> u16 {
    let mut crc = 0xffffu16;
    for byte in code {
        crc ^= *byte as u16;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0x8408
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// The install code as it is printed on a device: the code, then its checksum
/// in the order a coordinator reads them.
pub fn install_code_label(code: &[u8; INSTALL_CODE_LEN]) -> [u8; INSTALL_CODE_LABEL_LEN] {
    let mut label = [0u8; INSTALL_CODE_LABEL_LEN];
    label[..INSTALL_CODE_LEN].copy_from_slice(code);
    label[INSTALL_CODE_LEN..].copy_from_slice(&install_code_crc(code).to_le_bytes());
    label
}

/// The link key a trust centre derives from an install code, which is what
/// takes the place of the key everybody already knows.
pub fn install_code_link_key(code: &[u8; INSTALL_CODE_LEN]) -> [u8; KEY_LEN] {
    aes_mmo(&install_code_label(code))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The worked example from the specification, which is the only way to know
    /// the checksum, its order and the hash are all right at once.
    const PUBLISHED_CODE: [u8; INSTALL_CODE_LEN] = [
        0x83, 0xfe, 0xd3, 0x40, 0x7a, 0x93, 0x97, 0x23, 0xa5, 0xc6, 0x39, 0xb2, 0x69, 0x16, 0xd5,
        0x05,
    ];

    #[test]
    fn the_published_install_code_derives_the_published_link_key() {
        assert_eq!(
            install_code_link_key(&PUBLISHED_CODE),
            [
                0x66, 0xb6, 0x90, 0x09, 0x81, 0xe1, 0xee, 0x3c, 0xa4, 0x20, 0x6b, 0x6b, 0x86, 0x1c,
                0x02, 0xbb,
            ]
        );
    }

    #[test]
    fn the_label_ends_in_the_checksum_the_specification_prints() {
        let label = install_code_label(&PUBLISHED_CODE);

        assert_eq!(&label[..INSTALL_CODE_LEN], &PUBLISHED_CODE);
        assert_eq!(&label[INSTALL_CODE_LEN..], &[0xc3, 0xb5]);
    }

    #[test]
    fn one_wrong_octet_gives_a_different_checksum_and_a_different_key() {
        let mut mistyped = PUBLISHED_CODE;
        mistyped[7] ^= 0x01;

        assert_ne!(
            install_code_label(&mistyped)[INSTALL_CODE_LEN..],
            install_code_label(&PUBLISHED_CODE)[INSTALL_CODE_LEN..]
        );
        assert_ne!(
            install_code_link_key(&mistyped),
            install_code_link_key(&PUBLISHED_CODE)
        );
    }
}
