//! Make signing keys and license keys.
//!
//! ```text
//! sheafly-keygen new-key <kid>
//!     A new signing key: the secret (hex, keep it offline) and the line for keys.rs.
//! sheafly-keygen issue --name N --email E [--plan pro|business|trial] [--seats 1]
//!     [--id ID] [--issued YYYY-MM-DD] [--updates-until YYYY-MM-DD] [--expires YYYY-MM-DD]
//!     [--features a,b] (--kid KID | --dev)
//!     A license key. The secret comes from SHEAFLY_LICENSE_SECRET (hex), never from an
//!     argument, so it stays out of shell history; --dev signs with the development key
//!     in dev-secret.txt, which only debug builds accept.
//! ```

use std::collections::HashMap;
use std::process::ExitCode;

use sheafly_license::issue::{issue, new_secret, public_of, secret_from_hex, to_hex};
use sheafly_license::keys::DEV_KID;
use sheafly_license::{Day, Payload, Plan, FORMAT};

const DEV_SECRET: &str = include_str!("../../dev-secret.txt");
const SECRET_VAR: &str = "SHEAFLY_LICENSE_SECRET";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("new-key") => new_key(&args[1..]),
        Some("issue") => issue_key(&args[1..]),
        _ => Err("usage: sheafly-keygen new-key <kid> | issue --name N --email E … (see the source)".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("sheafly-keygen: {message}");
            ExitCode::FAILURE
        }
    }
}

fn new_key(args: &[String]) -> Result<(), String> {
    let kid = args.first().ok_or("new-key needs a key id, such as k2026")?;
    if kid == DEV_KID || kid.is_empty() || !kid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(format!("{kid:?} is not a usable key id (letters, digits and dashes; not {DEV_KID:?})"));
    }
    let secret = new_secret().map_err(|err| format!("no random source: {err}"))?;
    let public = public_of(&secret);
    let bytes: Vec<String> = public.iter().map(|b| format!("0x{b:02x}")).collect();
    println!("secret (keep offline, give only to the issuing service): {}", to_hex(&secret));
    println!("add to PRODUCTION in crates/license/src/keys.rs:");
    println!("    (\"{kid}\", [{}]),", bytes.join(", "));
    Ok(())
}

fn issue_key(args: &[String]) -> Result<(), String> {
    let options = parse_options(args)?;
    let get = |name: &str| options.get(name).map(String::as_str);
    let dev = options.contains_key("dev");
    let (kid, secret) = if dev {
        (DEV_KID.to_string(), secret_from_hex(DEV_SECRET).ok_or("dev-secret.txt is not 32 bytes of hex")?)
    } else {
        let kid = get("kid").ok_or("issue needs --kid, or --dev for a development key")?;
        let text = std::env::var(SECRET_VAR).map_err(|_| format!("set {SECRET_VAR} to the signing secret (hex)"))?;
        (kid.to_string(), secret_from_hex(&text).ok_or_else(|| format!("{SECRET_VAR} is not 32 bytes of hex"))?)
    };
    let plan = match get("plan").unwrap_or("pro") {
        "pro" => Plan::Pro,
        "business" => Plan::Business,
        "trial" => Plan::Trial,
        other => return Err(format!("unknown plan {other:?}")),
    };
    let day = |name: &str| -> Result<Option<String>, String> {
        match get(name) {
            None => Ok(None),
            Some(text) => Day::parse(text).map(|d| Some(d.to_string())).ok_or(format!("--{name} {text:?} is not YYYY-MM-DD")),
        }
    };
    let today = Day::today_utc();
    let issued = day("issued")?.unwrap_or_else(|| today.to_string());
    let (y, m, d) = Day::parse(&issued).expect("checked above").ymd();
    let a_year_on = Day::from_ymd(y + 1, m, d.min(28)).expect("a valid day").to_string();
    let payload = Payload {
        v: FORMAT,
        kid,
        id: get("id").map(str::to_string).unwrap_or_else(|| format!("L-{}", &to_hex(&new_secret().unwrap_or([7; 32]))[..12])),
        name: get("name").ok_or("issue needs --name")?.to_string(),
        email: get("email").ok_or("issue needs --email")?.to_string(),
        plan,
        seats: get("seats").map(|s| s.parse().map_err(|_| format!("--seats {s:?} is not a number"))).transpose()?.unwrap_or(1),
        features: get("features").map(|f| f.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default(),
        issued,
        updates_until: day("updates-until")?.unwrap_or(a_year_on),
        expires: day("expires")?,
    };
    if payload.seats == 0 {
        return Err("--seats must be at least 1".into());
    }
    println!("{}", issue(&payload, &secret));
    Ok(())
}

/// `--name value` pairs; `--dev` alone.
fn parse_options(args: &[String]) -> Result<HashMap<String, String>, String> {
    let mut options = HashMap::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let name = arg.strip_prefix("--").ok_or_else(|| format!("unexpected {arg:?}"))?;
        if name == "dev" {
            options.insert("dev".to_string(), String::new());
            continue;
        }
        let value = rest.next().ok_or_else(|| format!("--{name} needs a value"))?;
        options.insert(name.to_string(), value.clone());
    }
    Ok(options)
}
