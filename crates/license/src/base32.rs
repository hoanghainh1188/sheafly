//! Crockford base32, without padding: the digits and the letters except I, L, O and U,
//! so a key read aloud or typed from paper has no look-alikes. Reading forgives
//! lowercase and takes I and L as 1 and O as 0.

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

#[cfg(any(test, feature = "issue"))]
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(5) * 8);
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for &byte in bytes {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buffer >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn value(c: char) -> Option<u32> {
    let c = match c.to_ascii_uppercase() {
        'I' | 'L' => '1',
        'O' => '0',
        c => c,
    };
    ALPHABET.iter().position(|&a| a as char == c).map(|i| i as u32)
}

/// `None` on a character outside the alphabet or text `encode` would not write.
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 5 / 8);
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for c in text.chars() {
        buffer = (buffer << 5) | value(c)?;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
        buffer &= (1 << bits) - 1;
    }
    // Only the canonical spelling: a leftover of five bits or more is a stray character,
    // and unused low bits must be zero, so one key has exactly one text.
    (bits < 5 && buffer == 0).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_length() {
        let bytes: Vec<u8> = (0..=255u8).collect();
        for len in 0..40 {
            let encoded = encode(&bytes[..len]);
            assert_eq!(decode(&encoded).unwrap(), &bytes[..len]);
            assert_eq!(decode(&encoded.to_lowercase()).unwrap(), &bytes[..len]);
        }
    }

    #[test]
    fn forgives_look_alikes_and_refuses_others() {
        assert_eq!(decode("1O"), decode("i0"));
        assert_eq!(decode("L1"), decode("11"));
        assert_eq!(decode("U0"), None);
        assert_eq!(decode("A-"), None);
    }

    #[test]
    fn refuses_spellings_encode_never_writes() {
        // One byte is two characters; the second carries two unused bits.
        assert_eq!(decode("Z0"), Some(vec![0xF8]));
        assert_eq!(decode("Z1"), None, "an unused bit set");
        assert_eq!(decode("Z00"), None, "a stray character");
    }
}
