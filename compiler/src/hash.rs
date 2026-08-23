//! SHA-256, for content identity.
//!
//! Hand-rolled to keep the crate dependency-free — the same house convention as the JSON
//! emitter, and for the same reason: a dependency tree is a real cost for the people this
//! is built for (see `primitives/README.md`). SHA-256 earns the exception because it is
//! completely specified and has published test vectors, so correctness here is *verified*
//! rather than hoped for; the tests below check the official NIST vectors.
//!
//! **This is a content-identity function, not a security boundary.** Collision resistance
//! matters; side-channel resistance does not, and nothing here attempts constant time. If a
//! hash ever becomes a trust boundary — signed releases, a shared public store — swap in a
//! reviewed crate at that point. RFC-0003 §2.2 records this limit deliberately.

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// A 256-bit content hash.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Hash([u8; 32]);

impl Hash {
    /// The full 64-character lowercase hex form.
    pub fn to_hex(self) -> String {
        let mut s = String::with_capacity(64);
        for b in self.0 {
            s.push(hex_digit(b >> 4));
            s.push(hex_digit(b & 0x0f));
        }
        s
    }

    /// The short form used in diagnostics and paths: the first 12 hex characters.
    ///
    /// 48 bits of prefix — collision-free at any realistic project size, and short enough
    /// to sit inside a diagnostic without spending the density budget (RFC-0001 §4.2).
    pub fn short(self) -> String {
        self.to_hex()[..12].to_string()
    }

    /// The raw bytes, for callers that need to embed a hash in another hash's input.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        _ => (b'a' + nibble - 10) as char,
    }
}

/// Hash a byte slice.
pub fn sha256(input: &[u8]) -> Hash {
    let mut h = H0;

    // Padding: the message, a 0x80 byte, zeroes, then the bit length as a big-endian u64.
    let mut message = input.to_vec();
    let bit_len = (input.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in message.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ ((!v[4]) & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);

            v[7] = v[6];
            v[6] = v[5];
            v[5] = v[4];
            v[4] = v[3].wrapping_add(t1);
            v[3] = v[2];
            v[2] = v[1];
            v[1] = v[0];
            v[0] = t1.wrapping_add(t2);
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    Hash(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The published NIST vectors. Hand-rolled crypto is only defensible when it is checked
    // against the specification's own answers, so these are the load-bearing tests.
    #[test]
    fn nist_vector_empty_string() {
        assert_eq!(
            sha256(b"").to_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn nist_vector_abc() {
        assert_eq!(
            sha256(b"abc").to_hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn nist_vector_two_block_message() {
        assert_eq!(
            sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq").to_hex(),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn nist_vector_million_a() {
        // Exercises multi-chunk padding and the length field at a size where an off-by-one
        // in the padding loop would show up.
        let input = vec![b'a'; 1_000_000];
        assert_eq!(
            sha256(&input).to_hex(),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn a_message_exactly_at_a_block_boundary_pads_into_a_new_block() {
        // 56 bytes is the worst case: the 0x80 byte leaves no room for the length field,
        // so a whole extra block must be added. A naive padding loop gets this wrong.
        let input = vec![b'x'; 56];
        let hex = sha256(&input).to_hex();
        assert_eq!(hex.len(), 64);
        // Differs from 55 and 57 bytes — i.e. the boundary is handled, not swallowed.
        assert_ne!(hex, sha256(&[b'x'; 55]).to_hex());
        assert_ne!(hex, sha256(&[b'x'; 57]).to_hex());
    }

    #[test]
    fn the_short_form_is_twelve_hex_characters() {
        let h = sha256(b"abc");
        assert_eq!(h.short().len(), 12);
        assert!(h.to_hex().starts_with(&h.short()));
    }

    #[test]
    fn hashing_is_deterministic() {
        assert_eq!(sha256(b"component button"), sha256(b"component button"));
        assert_ne!(sha256(b"component button"), sha256(b"component buttons"));
    }
}
