use std::process::Command;

fn main() {
    println!("cargo:rustc-env=SHEAFLY_BUILD_DATE={}", build_date());
    tauri_build::build()
}

/// The day licenses compare their end of updates with: `SHEAFLY_BUILD_DATE` when set
/// (`YYYY-MM-DD`), else the date of the commit being built, so the same commit always has
/// the same date. Without either (no git, a source archive) a local build warns and takes
/// any license's updates as covering it; a CI build (`CI` set) fails instead.
fn build_date() -> String {
    println!("cargo:rerun-if-env-changed=SHEAFLY_BUILD_DATE");
    if let Ok(date) = std::env::var("SHEAFLY_BUILD_DATE") {
        if is_day(&date) {
            return date;
        }
        println!("cargo:warning=SHEAFLY_BUILD_DATE {date:?} is not YYYY-MM-DD; using the commit date");
    }
    // A new commit or checkout moves the reflog, so the date is read again.
    if let Some(reflog) = git(&["rev-parse", "--git-path", "logs/HEAD"]) {
        println!("cargo:rerun-if-changed={reflog}");
    }
    if let Some(date) = git(&["log", "-1", "--format=%cs"]).filter(|date| is_day(date)) {
        return date;
    }
    if std::env::var_os("CI").is_some() {
        panic!("no build date: set SHEAFLY_BUILD_DATE (YYYY-MM-DD) or build from a git checkout");
    }
    println!("cargo:warning=no build date (no git commit, no SHEAFLY_BUILD_DATE): licenses will not check their end of updates");
    "1970-01-01".into()
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// A real calendar day written `YYYY-MM-DD` (the same rule as `sheafly_license::Day::parse`).
fn is_day(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let number = |from: usize, to: usize| text[from..to].bytes().all(|b| b.is_ascii_digit()).then(|| text[from..to].parse::<u32>().ok()).flatten();
    let (Some(year), Some(month), Some(day)) = (number(0, 4), number(5, 7), number(8, 10)) else { return false };
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1..=12 => 31,
        _ => 0,
    };
    year >= 1 && day >= 1 && day <= days
}
