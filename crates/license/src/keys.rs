//! The public keys licenses are signed with, and the licenses taken back.
//!
//! Only public keys live here. The secret that goes with a production key is kept by the
//! issuing service and offline by the owner; it never enters this repository or its CI.
//! To add one, run `cargo run -p sheafly-license --features issue --bin sheafly-keygen --
//! new-key <kid>` on a trusted machine, store the secret it prints, and add the public
//! line it prints to [`PRODUCTION`].

/// Signing keys a release accepts, by id. Keep retired ones while their licenses live.
pub const PRODUCTION: &[(&str, [u8; 32])] = &[
    ("k2026", [0x6d, 0x12, 0x17, 0x00, 0x0e, 0xb8, 0xe8, 0x26, 0x3f, 0x8b, 0x70, 0x52, 0xd5, 0x06, 0xa1, 0x5f, 0xae, 0xe4, 0xe1, 0xd8, 0x43, 0x02, 0xe9, 0xed, 0x9d, 0x71, 0x09, 0x38, 0x4a, 0xeb, 0xf9, 0x6d]),
];

/// The development key. Its secret is in this repository (`dev-secret.txt`), so only
/// debug builds accept it: `pnpm tauri dev` and tests can use made-up licenses, while a
/// release (`--release`, no `debug_assertions`) refuses them.
pub const DEV_KID: &str = "dev";
pub const DEV_PUBLIC: [u8; 32] = [
    0xe6, 0x04, 0xb7, 0x43, 0x26, 0x36, 0x53, 0xe2, 0xf0, 0xbf, 0x9f, 0x85, 0x79, 0xcf, 0xf9, 0xab, 0x7f, 0x52, 0x1c, 0x0f, 0x3f, 0xb0, 0xb9, 0x94, 0x6a, 0x82, 0x5f, 0x67, 0x7a, 0x30, 0xbc, 0xdf,
];

/// License ids taken back (refunds, keys shared in public). Each release ships the list.
pub const REVOKED: &[&str] = &[];

/// The keys and revocations a verification uses.
#[derive(Clone, Copy, Debug)]
pub struct Keys<'a> {
    pub signing: &'a [(&'a str, [u8; 32])],
    pub revoked: &'a [&'a str],
}

impl<'a> Keys<'a> {
    pub fn public_key(&self, kid: &str) -> Option<&'a [u8; 32]> {
        self.signing.iter().find(|(id, _)| *id == kid).map(|(_, key)| key)
    }

    pub fn is_revoked(&self, id: &str) -> bool {
        self.revoked.contains(&id)
    }
}

/// What this build accepts: the production keys, plus the development key in a debug
/// build. The production keys come first, so a production `kid` is never shadowed.
pub fn app_keys() -> Keys<'static> {
    #[cfg(debug_assertions)]
    {
        static WITH_DEV: std::sync::OnceLock<Vec<(&'static str, [u8; 32])>> = std::sync::OnceLock::new();
        let signing = WITH_DEV.get_or_init(|| {
            PRODUCTION.iter().copied().chain([(DEV_KID, DEV_PUBLIC)]).collect()
        });
        Keys { signing, revoked: REVOKED }
    }
    #[cfg(not(debug_assertions))]
    {
        Keys { signing: PRODUCTION, revoked: REVOKED }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_keys_have_unique_ids_and_none_is_the_dev_key() {
        let mut ids: Vec<&str> = PRODUCTION.iter().map(|(id, _)| *id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PRODUCTION.len());
        assert!(!ids.contains(&DEV_KID));
        assert!(PRODUCTION.iter().all(|(_, key)| *key != DEV_PUBLIC));
    }
}
