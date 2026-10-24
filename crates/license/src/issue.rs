//! Making keys: for the key tool and the issuing service, never compiled into the app.

use ed25519_dalek::{Signer, SigningKey};

use crate::{Payload, FORMAT};

/// A new signing secret from the system's random source.
pub fn new_secret() -> Result<[u8; 32], getrandom::Error> {
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret)?;
    Ok(secret)
}

/// The public key that checks what `secret` signs.
pub fn public_of(secret: &[u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(secret).verifying_key().to_bytes()
}

/// The key text for `payload`, signed with `secret`. The payload's format number is set
/// to the one this crate writes.
pub fn issue(payload: &Payload, secret: &[u8; 32]) -> String {
    let payload = Payload { v: FORMAT, ..payload.clone() };
    let mut bytes = serde_json::to_vec(&payload).expect("a payload always serializes");
    let signature = SigningKey::from_bytes(secret).sign(&bytes);
    bytes.extend_from_slice(&signature.to_bytes());
    crate::encode(&bytes)
}

/// Lowercase hex, as the key tool prints secrets and public keys.
pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Exactly 32 bytes of hex (either case, surrounding space allowed).
pub fn secret_from_hex(text: &str) -> Option<[u8; 32]> {
    let text = text.trim();
    if text.len() != 64 || !text.is_ascii() {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}
