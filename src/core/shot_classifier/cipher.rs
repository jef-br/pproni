//! ChaCha20 keystream (RFC 8439) used only to obfuscate the embedded model at rest.
//!
//! Deliberately *not* a security boundary: the key ships inside the binary. XOR-symmetric, so the
//! same routine encrypts (in `build.rs`) and decrypts (at load). No MAC — integrity is not a goal,
//! and a corrupted model simply fails the session build in `core::preflight`.
//! See docs/architecture/05-single-exe.md.

const CONSTANTS: [u32; 4] = [0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574];

#[inline]
fn quarter_round(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]); s[d] ^= s[a]; s[d] = s[d].rotate_left(16);
    s[c] = s[c].wrapping_add(s[d]); s[b] ^= s[c]; s[b] = s[b].rotate_left(12);
    s[a] = s[a].wrapping_add(s[b]); s[d] ^= s[a]; s[d] = s[d].rotate_left(8);
    s[c] = s[c].wrapping_add(s[d]); s[b] ^= s[c]; s[b] = s[b].rotate_left(7);
}

/// One 64-byte keystream block for `counter`.
fn block(key: &[u8; 32], nonce: &[u8; 12], counter: u32, out: &mut [u8; 64]) {
    let mut state = [0u32; 16];
    state[0..4].copy_from_slice(&CONSTANTS);
    for i in 0..8 {
        state[4 + i] = u32::from_le_bytes([
            key[4 * i], key[4 * i + 1], key[4 * i + 2], key[4 * i + 3],
        ]);
    }
    state[12] = counter;
    for i in 0..3 {
        state[13 + i] = u32::from_le_bytes([
            nonce[4 * i], nonce[4 * i + 1], nonce[4 * i + 2], nonce[4 * i + 3],
        ]);
    }

    let mut w = state;
    for _ in 0..10 {
        // Column rounds.
        quarter_round(&mut w, 0, 4, 8, 12);
        quarter_round(&mut w, 1, 5, 9, 13);
        quarter_round(&mut w, 2, 6, 10, 14);
        quarter_round(&mut w, 3, 7, 11, 15);
        // Diagonal rounds.
        quarter_round(&mut w, 0, 5, 10, 15);
        quarter_round(&mut w, 1, 6, 11, 12);
        quarter_round(&mut w, 2, 7, 8, 13);
        quarter_round(&mut w, 3, 4, 9, 14);
    }
    for i in 0..16 {
        let word = w[i].wrapping_add(state[i]);
        out[4 * i..4 * i + 4].copy_from_slice(&word.to_le_bytes());
    }
}

/// XOR `data` in place with the ChaCha20 keystream. Symmetric: the same call encrypts and decrypts.
/// The 32-bit block counter starts at 0; the model is far under the 256 GB it could stream.
pub fn xor_keystream(data: &mut [u8], key: &[u8; 32], nonce: &[u8; 12]) {
    let mut ks = [0u8; 64];
    let mut counter: u32 = 0;
    for chunk in data.chunks_mut(64) {
        block(key, nonce, counter, &mut ks);
        for (b, k) in chunk.iter_mut().zip(ks.iter()) {
            *b ^= *k;
        }
        counter = counter.wrapping_add(1);
    }
}

/// Decrypt `enc` into a fresh buffer. Encryption is the identical operation; `build.rs` calls
/// `xor_keystream` directly on the plaintext.
pub fn decrypt(enc: &[u8], key: &[u8; 32], nonce: &[u8; 12]) -> Vec<u8> {
    let mut out = enc.to_vec();
    xor_keystream(&mut out, key, nonce);
    out
}
