use sheafly_license::issue::{issue, public_of, secret_from_hex};
use sheafly_license::keys::{app_keys, DEV_KID, DEV_PUBLIC};
use sheafly_license::{read, verify, Day, Feature, Keys, Payload, Plan, Problem, FORMAT, PREFIX};

const SECRET: [u8; 32] = [42; 32];
const OTHER: [u8; 32] = [7; 32];

fn day(text: &str) -> Day {
    Day::parse(text).unwrap()
}

fn payload() -> Payload {
    Payload {
        v: FORMAT,
        kid: "k1".into(),
        id: "L-1".into(),
        name: "Nguyễn Văn A".into(),
        email: "a@example.com".into(),
        plan: Plan::Pro,
        seats: 3,
        features: vec![],
        issued: "2026-10-24".into(),
        updates_until: "2027-10-24".into(),
        expires: None,
    }
}

fn signing() -> Vec<(&'static str, [u8; 32])> {
    vec![("k0", public_of(&OTHER)), ("k1", public_of(&SECRET))]
}

fn keys<'a>(signing: &'a [(&'static str, [u8; 32])]) -> Keys<'a> {
    Keys { signing, revoked: &["L-gone"] }
}

#[test]
fn a_signed_key_reads_back_with_its_fields() {
    let text = issue(&payload(), &SECRET);
    assert!(text.starts_with(PREFIX));
    let signing = signing();
    let license = verify(&text, &keys(&signing), day("2026-11-01"), day("2026-11-02")).unwrap();
    assert_eq!(license.name, "Nguyễn Văn A");
    assert_eq!(license.plan, Plan::Pro);
    assert_eq!(license.seats, 3);
    assert_eq!(license.updates_until, day("2027-10-24"));
    assert!(license.grants(Feature::Trackers));
}

#[test]
fn reading_forgives_case_spaces_and_line_breaks() {
    let text = issue(&payload(), &SECRET);
    let messy = format!("  {}\n", text.to_lowercase().replace('-', " - ").replacen(" - ", "-", 1));
    let wrapped: String = text.chars().enumerate().flat_map(|(i, c)| if i % 30 == 29 { vec![c, '\n'] } else { vec![c] }).collect();
    let signing = signing();
    assert!(read(&messy, &keys(&signing)).is_ok());
    assert!(read(&wrapped, &keys(&signing)).is_ok());
}

#[test]
fn a_changed_character_breaks_the_signature_or_the_key() {
    let text = issue(&payload(), &SECRET);
    let signing = signing();
    for at in (PREFIX.len()..text.len()).step_by(7) {
        let mut chars: Vec<char> = text.chars().collect();
        if chars[at] == '-' {
            continue;
        }
        chars[at] = if chars[at] == 'A' { 'B' } else { 'A' };
        let changed: String = chars.into_iter().collect();
        assert!(read(&changed, &keys(&signing)).is_err(), "changed at {at}");
    }
}

#[test]
fn a_payload_signed_with_another_secret_fails() {
    let signing = signing();
    let forged = issue(&Payload { seats: 500, ..payload() }, &OTHER);
    assert_eq!(read(&forged, &keys(&signing)), Err(Problem::BadSignature));
}

#[test]
fn an_edited_payload_under_a_good_signature_fails() {
    // Take a genuine key apart, change one byte of the payload, keep the signature.
    let signing = signing();
    let text = issue(&payload(), &SECRET);
    let mut bytes = raw_bytes(&text);
    let at = bytes.windows(9).position(|w| w == br#""seats":3"#).unwrap() + 8;
    bytes[at] = b'9';
    let edited = format!("{PREFIX}{}", crockford(&bytes));
    assert_eq!(read(&edited, &keys(&signing)), Err(Problem::BadSignature));
}

/// The bytes behind a key text, decoded by hand (the crate keeps its decoder private).
fn raw_bytes(text: &str) -> Vec<u8> {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let body: String = text[PREFIX.len()..].chars().filter(|c| *c != '-').collect();
    let (mut out, mut buffer, mut bits) = (Vec::new(), 0u32, 0);
    for c in body.bytes() {
        buffer = (buffer << 5) | ALPHABET.iter().position(|a| *a == c).unwrap() as u32;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
        buffer &= (1 << bits) - 1;
    }
    out
}

#[test]
fn an_unknown_signing_key_is_refused() {
    let text = issue(&Payload { kid: "k9".into(), ..payload() }, &SECRET);
    let signing = signing();
    assert_eq!(read(&text, &keys(&signing)), Err(Problem::UnknownKey));
}

#[test]
fn old_keys_keep_working_after_a_rotation() {
    let old = issue(&Payload { kid: "k0".into(), ..payload() }, &OTHER);
    let signing = signing();
    assert!(read(&old, &keys(&signing)).is_ok());
}

#[test]
fn a_revoked_license_is_refused() {
    let text = issue(&Payload { id: "L-gone".into(), ..payload() }, &SECRET);
    let signing = signing();
    assert_eq!(read(&text, &keys(&signing)), Err(Problem::Revoked));
}

#[test]
fn garbage_is_malformed() {
    let signing = signing();
    for text in ["", "SHEAFLY-", "hello", "SHEAFLY-UUUUU", "SHEAFLY-AAAAA-AAAAA", &"SHEAFLY-".repeat(1000)] {
        assert_eq!(read(text, &keys(&signing)), Err(Problem::Malformed), "{text:.20}");
    }
}

#[test]
fn a_trial_ends_on_its_day() {
    let text = issue(&Payload { plan: Plan::Trial, expires: Some("2026-11-07".into()), ..payload() }, &SECRET);
    let signing = signing();
    let keys = keys(&signing);
    assert!(verify(&text, &keys, day("2026-10-24"), day("2026-11-07")).is_ok());
    let rejected = verify(&text, &keys, day("2026-10-24"), day("2026-11-08")).unwrap_err();
    assert_eq!(rejected.problem, Problem::Expired(day("2026-11-07")));
    assert_eq!(rejected.license.unwrap().plan, Plan::Trial);
}

#[test]
fn releases_up_to_the_end_of_updates_keep_working_forever() {
    let text = issue(&payload(), &SECRET);
    let signing = signing();
    let keys = keys(&signing);
    // A build from the last day of updates, used years later.
    assert!(verify(&text, &keys, day("2027-10-24"), day("2031-01-01")).is_ok());
    let rejected = verify(&text, &keys, day("2027-10-25"), day("2027-10-26")).unwrap_err();
    assert_eq!(rejected.problem, Problem::UpdatesEnded(day("2027-10-24")));
    assert!(rejected.license.is_some());
}

#[test]
fn named_features_limit_the_grant_and_unknown_ones_are_ignored() {
    let text = issue(&Payload { features: vec!["portfolio-from-the-future".into()], ..payload() }, &SECRET);
    let signing = signing();
    let license = read(&text, &keys(&signing)).unwrap();
    // An unknown name alone leaves nothing this build knows: not "everything".
    assert!(!license.grants(Feature::Trackers));
    let only_trackers = issue(&Payload { features: vec!["trackers".into()], ..payload() }, &SECRET);
    assert!(read(&only_trackers, &keys(&signing)).unwrap().grants(Feature::Trackers));
}

#[test]
fn a_newer_format_says_so() {
    // The issuer of a later format signs `v: 2`; `issue` always writes this crate's format,
    // so the bytes are signed and encoded by hand.
    let signing = signing();
    let text = issue(&payload(), &SECRET);
    assert!(read(&text, &keys(&signing)).is_ok());
    let newer = newer_key(&SECRET);
    assert_eq!(read(&newer, &keys(&signing)), Err(Problem::NewerFormat));
}

fn newer_key(secret: &[u8; 32]) -> String {
    use ed25519_dalek::{Signer, SigningKey};
    let mut bytes = br#"{"v":2,"kid":"k1","id":"L-2"}"#.to_vec();
    let signature = SigningKey::from_bytes(secret).sign(&bytes);
    bytes.extend_from_slice(&signature.to_bytes());
    format!("{PREFIX}{}", crockford(&bytes))
}

fn crockford(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let (mut out, mut buffer, mut bits) = (String::new(), 0u32, 0);
    for &byte in bytes {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buffer >> bits) & 31) as usize] as char);
        }
        buffer &= (1 << bits) - 1;
    }
    if bits > 0 {
        out.push(ALPHABET[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out
}

#[test]
fn the_dev_secret_matches_the_dev_key_and_only_debug_builds_take_it() {
    let secret = secret_from_hex(include_str!("../dev-secret.txt")).unwrap();
    assert_eq!(public_of(&secret), DEV_PUBLIC);
    let text = issue(&Payload { kid: DEV_KID.into(), ..payload() }, &secret);
    let accepted = read(&text, &app_keys()).is_ok();
    assert_eq!(accepted, cfg!(debug_assertions));
}
