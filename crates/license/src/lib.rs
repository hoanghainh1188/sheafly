//! Sheafly Pro license keys, checked offline.
//!
//! A key is `SHEAFLY-` and the Crockford base32 of a small JSON payload followed by its
//! ed25519 signature (64 bytes), in groups of five. The payload names the signing key
//! (`kid`), so keys can be rotated while older licenses keep working. Nothing here talks
//! to a network: a key is good when its signature is right, it is not revoked, a trial
//! has not expired by the computer's clock, and this build was made on or before the
//! day its updates end (a license keeps every release made before then, for good).
//!
//! The engine knows nothing of licenses: what a license grants is decided in the app
//! shell, never in scheduling or replay.

mod base32;
mod day;
#[cfg(feature = "issue")]
pub mod issue;
pub mod keys;

use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

pub use day::Day;
pub use keys::Keys;

/// Every key starts with this; reading forgives its letter case.
pub const PREFIX: &str = "SHEAFLY-";
/// The payload format this build reads.
pub const FORMAT: u32 = 1;
const SIGNATURE_LEN: usize = 64;
/// Far longer than any real key, so a pasted file is refused before it is decoded.
const MAX_KEY_LEN: usize = 4096;

/// The plan a license is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Plan {
    Pro,
    Business,
    Trial,
}

/// What a plan unlocks. Features a newer issuer names that this build does not know are
/// left out, so an older app reads a newer key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    /// Pulling from and pushing to an external task tracker.
    Trackers,
    /// Capacity planning: the productivity factor of the project and of each resource.
    Capacity,
    /// Executive reporting, corporate branding, and multi-sheet Excel export.
    Reporting,
    /// AI recommendations: calls to the AI provider with the user's own key.
    Ai,
    /// The schedule health check (DCMA's 14 points).
    Health,
    /// The cash flow by period: money in from payments, money out from costs, the balance.
    Cashflow,
    /// Project version comparison and visual variance audit diff.
    Diff,
}

impl Feature {
    pub const ALL: &'static [Feature] = &[
        Feature::Trackers,
        Feature::Capacity,
        Feature::Reporting,
        Feature::Ai,
        Feature::Health,
        Feature::Cashflow,
        Feature::Diff,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Feature::Trackers => "trackers",
            Feature::Capacity => "capacity",
            Feature::Reporting => "reporting",
            Feature::Ai => "ai",
            Feature::Health => "health",
            Feature::Cashflow => "cashflow",
            Feature::Diff => "diff",
        }
    }

    pub fn from_code(code: &str) -> Option<Feature> {
        Feature::ALL.iter().copied().find(|feature| feature.code() == code)
    }
}

/// The signed part of a key, as it travels.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payload {
    pub v: u32,
    pub kid: String,
    pub id: String,
    pub name: String,
    pub email: String,
    pub plan: Plan,
    pub seats: u32,
    /// Empty: everything the plan has.
    #[serde(default)]
    pub features: Vec<String>,
    pub issued: String,
    pub updates_until: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
}

/// A key whose signature checked out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct License {
    pub id: String,
    pub name: String,
    pub email: String,
    pub plan: Plan,
    pub seats: u32,
    /// The features the key names that this build knows; `None` when it names none, which
    /// means every feature of the plan.
    pub features: Option<Vec<Feature>>,
    pub issued: Day,
    pub updates_until: Day,
    pub expires: Option<Day>,
}

impl License {
    pub fn grants(&self, feature: Feature) -> bool {
        self.features.as_ref().is_none_or(|named| named.contains(&feature))
    }

    /// Whether the license works on a build made on `build`, today being `today`.
    pub fn check(&self, build: Day, today: Day) -> Result<(), Problem> {
        if let Some(expires) = self.expires {
            if today > expires {
                return Err(Problem::Expired(expires));
            }
        }
        if build > self.updates_until {
            return Err(Problem::UpdatesEnded(self.updates_until));
        }
        Ok(())
    }
}

/// Why a key does not unlock Pro on this build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    /// Not a key at all: the prefix, a character, the length or the payload is wrong.
    Malformed,
    /// Made by a newer issuer, in a format this build cannot read.
    NewerFormat,
    /// Signed with a key this build does not know.
    UnknownKey,
    /// The signature does not match: the key was changed or mistyped.
    BadSignature,
    Revoked,
    /// A trial or subscription that ended on that day.
    Expired(Day),
    /// The license covers releases made up to that day; this one is newer.
    UpdatesEnded(Day),
}

impl Problem {
    /// A stable code for the frontend, which words it in the user's language.
    pub fn code(self) -> &'static str {
        match self {
            Problem::Malformed => "malformed",
            Problem::NewerFormat => "newer_format",
            Problem::UnknownKey => "unknown_key",
            Problem::BadSignature => "bad_signature",
            Problem::Revoked => "revoked",
            Problem::Expired(_) => "expired",
            Problem::UpdatesEnded(_) => "updates_ended",
        }
    }
}

/// Read a key and check its signature against `keys`; dates are not looked at.
pub fn read(text: &str, keys: &Keys) -> Result<License, Problem> {
    let bytes = decode(text).ok_or(Problem::Malformed)?;
    if bytes.len() <= SIGNATURE_LEN {
        return Err(Problem::Malformed);
    }
    let (payload_bytes, signature_bytes) = bytes.split_at(bytes.len() - SIGNATURE_LEN);
    let format: FormatOnly = serde_json::from_slice(payload_bytes).map_err(|_| Problem::Malformed)?;
    if format.v != FORMAT {
        return Err(if format.v > FORMAT { Problem::NewerFormat } else { Problem::Malformed });
    }
    let payload: Payload = serde_json::from_slice(payload_bytes).map_err(|_| Problem::Malformed)?;
    let public = keys.public_key(&payload.kid).ok_or(Problem::UnknownKey)?;
    let public = VerifyingKey::from_bytes(public).map_err(|_| Problem::UnknownKey)?;
    let signature = Signature::from_slice(signature_bytes).map_err(|_| Problem::Malformed)?;
    public
        .verify_strict(payload_bytes, &signature)
        .map_err(|_| Problem::BadSignature)?;
    if keys.is_revoked(&payload.id) {
        return Err(Problem::Revoked);
    }
    license_of(payload).ok_or(Problem::Malformed)
}

/// What a key means on this build: the license when it works, else why not, with the
/// license when its signature is good (so the reason can name its dates).
pub fn verify(text: &str, keys: &Keys, build: Day, today: Day) -> Result<License, Rejected> {
    let license = read(text, keys).map_err(|problem| Rejected { problem, license: None })?;
    match license.check(build, today) {
        Ok(()) => Ok(license),
        Err(problem) => Err(Rejected { problem, license: Some(Box::new(license)) }),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rejected {
    pub problem: Problem,
    pub license: Option<Box<License>>,
}

/// Only the format number, read before the rest so a newer payload is told apart from a
/// broken one.
#[derive(Deserialize)]
struct FormatOnly {
    v: u32,
}

fn license_of(payload: Payload) -> Option<License> {
    let expires = match &payload.expires {
        Some(day) => Some(Day::parse(day)?),
        None => None,
    };
    Some(License {
        id: payload.id,
        name: payload.name,
        email: payload.email,
        plan: payload.plan,
        seats: payload.seats.max(1),
        features: (!payload.features.is_empty())
            .then(|| payload.features.iter().filter_map(|code| Feature::from_code(code)).collect()),
        issued: Day::parse(&payload.issued)?,
        updates_until: Day::parse(&payload.updates_until)?,
        expires,
    })
}

/// The bytes of a key, forgiving spaces, line breaks, dashes and letter case.
fn decode(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if text.len() > MAX_KEY_LEN || text.len() < PREFIX.len() {
        return None;
    }
    let (prefix, body) = text.split_at_checked(PREFIX.len())?;
    if !prefix.eq_ignore_ascii_case(PREFIX) {
        return None;
    }
    let body: String = body.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    base32::decode(&body)
}

/// The key text of signed bytes: the prefix, then groups of five.
#[cfg(feature = "issue")]
fn encode(bytes: &[u8]) -> String {
    let body = base32::encode(bytes);
    let groups: Vec<&str> = body
        .as_bytes()
        .chunks(5)
        .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
        .collect();
    format!("{PREFIX}{}", groups.join("-"))
}
