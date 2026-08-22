use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockEncrypt, KeyInit};
use aes::Aes128;

pub const KEY_LEN: usize = 16;
pub const MIC_LEN: usize = 4;
pub const NONCE_LEN: usize = 13;

const BLOCK: usize = 16;

fn encrypt_block(cipher: &Aes128, block: &mut [u8; BLOCK]) {
    let mut b = GenericArray::clone_from_slice(block);
    cipher.encrypt_block(&mut b);
    block.copy_from_slice(&b);
}

fn xor_into(target: &mut [u8], source: &[u8]) {
    for (t, s) in target.iter_mut().zip(source) {
        *t ^= s;
    }
}

/// CCM* as Zigbee uses it: a 13 octet nonce, a 2 octet length field and a
/// 32 bit integrity code over both the header and the payload.
struct CcmStar {
    cipher: Aes128,
    nonce: [u8; NONCE_LEN],
}

impl CcmStar {
    fn new(key: &[u8; KEY_LEN], nonce: &[u8; NONCE_LEN]) -> Self {
        Self {
            cipher: Aes128::new(GenericArray::from_slice(key)),
            nonce: *nonce,
        }
    }

    fn keystream_block(&self, counter: u16) -> [u8; BLOCK] {
        let mut block = [0u8; BLOCK];
        block[0] = 0x01;
        block[1..1 + NONCE_LEN].copy_from_slice(&self.nonce);
        block[14..16].copy_from_slice(&counter.to_be_bytes());
        encrypt_block(&self.cipher, &mut block);
        block
    }

    fn apply_keystream(&self, payload: &mut [u8]) {
        for (index, chunk) in payload.chunks_mut(BLOCK).enumerate() {
            let keystream = self.keystream_block(index as u16 + 1);
            xor_into(chunk, &keystream);
        }
    }

    fn integrity_code(&self, header: &[u8], payload: &[u8]) -> [u8; MIC_LEN] {
        let mut state = [0u8; BLOCK];
        state[0] = 0x49;
        state[1..1 + NONCE_LEN].copy_from_slice(&self.nonce);
        state[14..16].copy_from_slice(&(payload.len() as u16).to_be_bytes());
        encrypt_block(&self.cipher, &mut state);

        let mut prefixed_header = [0u8; BLOCK];
        prefixed_header[..2].copy_from_slice(&(header.len() as u16).to_be_bytes());
        let split = core::cmp::min(header.len(), BLOCK - 2);
        prefixed_header[2..2 + split].copy_from_slice(&header[..split]);
        xor_into(&mut state, &prefixed_header);
        encrypt_block(&self.cipher, &mut state);

        for chunk in header[split..].chunks(BLOCK) {
            xor_into(&mut state, chunk);
            encrypt_block(&self.cipher, &mut state);
        }
        for chunk in payload.chunks(BLOCK) {
            xor_into(&mut state, chunk);
            encrypt_block(&self.cipher, &mut state);
        }

        let mut mic = [0u8; MIC_LEN];
        mic.copy_from_slice(&state[..MIC_LEN]);
        xor_into(&mut mic, &self.keystream_block(0));
        mic
    }
}

pub fn ccm_star_encrypt(
    key: &[u8; KEY_LEN],
    nonce: &[u8; NONCE_LEN],
    header: &[u8],
    payload: &mut [u8],
) -> [u8; MIC_LEN] {
    let ccm = CcmStar::new(key, nonce);
    let mic = ccm.integrity_code(header, payload);
    ccm.apply_keystream(payload);
    mic
}

pub fn ccm_star_decrypt(
    key: &[u8; KEY_LEN],
    nonce: &[u8; NONCE_LEN],
    header: &[u8],
    payload: &mut [u8],
    mic: &[u8; MIC_LEN],
) -> bool {
    let ccm = CcmStar::new(key, nonce);
    ccm.apply_keystream(payload);
    &ccm.integrity_code(header, payload) == mic
}

/// The Matyas-Meyer-Oseas hash Zigbee builds out of AES-128, used to turn a
/// link key into the specialised keys that protect key transport.
pub fn aes_mmo(message: &[u8]) -> [u8; KEY_LEN] {
    let mut digest = [0u8; KEY_LEN];

    let absorb = |digest: &mut [u8; KEY_LEN], block: &[u8]| {
        let cipher = Aes128::new(GenericArray::from_slice(digest));
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
