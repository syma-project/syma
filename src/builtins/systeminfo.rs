//! System information builtins.
//!
//! Provides Wolfram Language-style system variables: `$System`, `$Version`,
//! `$Machine`, `$OperatingSystem`, `$User`, `$TimeZone`, `$MachineName`,
//! `$ProcessorType`, `$ReleaseDate`, `$SystemId`, `$CommandLine`, `$InputLine`,
//! `$Language`.

use crate::env::Env;
use crate::value::Value;
use std::env::consts::OS;

pub const SYMBOLS: &[&str] = &[
    "$System",
    "$Version",
    "$ReleaseDate",
    "$Machine",
    "$MachineName",
    "$OperatingSystem",
    "$ProcessorType",
    "$User",
    "$TimeZone",
    "$SystemId",
    "$Language",
    "$CommandLine",
    "$InputLine",
];

/// Get the OS name normalized to Wolfram style (MacOS, Linux, Windows).
fn os_name() -> &'static str {
    match OS {
        "macos" => "MacOS",
        "linux" => "Linux",
        "windows" => "Windows",
        other => other,
    }
}

/// Get the architecture normalized with dash (x86-64, aarch64).
fn arch_name() -> String {
    std::env::consts::ARCH.replace("x86_64", "x86-64")
}

/// Get the hostname. Falls back to "Unknown" if not available.
#[cfg(unix)]
fn hostname() -> String {
    unsafe {
        let mut buf = [0u8; 256];
        if libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) == 0
            && let Some(null_pos) = buf.iter().position(|&b| b == 0) {
                return String::from_utf8_lossy(&buf[..null_pos]).to_string();
            }
    }
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "Unknown".to_string())
}

#[cfg(not(unix))]
fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "Unknown".to_string())
}

/// Get the local timezone offset from UTC in hours.
#[cfg(unix)]
fn timezone_offset() -> f64 {
    unsafe {
        use std::time::SystemTime;

        let mut tm = std::mem::zeroed::<libc::tm>();
        let now: libc::time_t = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as libc::time_t)
            .unwrap_or(0);
        libc::localtime_r(&now, &mut tm);
        tm.tm_gmtoff as f64 / 3600.0
    }
}

#[cfg(not(unix))]
fn timezone_offset() -> f64 {
    0.0
}

pub fn register(env: &Env) {
    let os = os_name();
    let arch = arch_name();
    let system = format!("{}-{}", os, arch);
    let version = format!("{} for {} (2026-04-28)", env!("CARGO_PKG_VERSION"), system,);

    let tz_offset = timezone_offset();

    env.set("$System".to_string(), Value::Str(system));
    env.set("$Version".to_string(), Value::Str(version));
    env.set(
        "$ReleaseDate".to_string(),
        Value::Str("2026-04-28".to_string()),
    );
    env.set("$Machine".to_string(), Value::Str(arch.clone()));
    env.set("$MachineName".to_string(), Value::Str(hostname()));
    env.set("$OperatingSystem".to_string(), Value::Str(os.to_string()));
    env.set("$ProcessorType".to_string(), Value::Str(arch));
    env.set(
        "$User".to_string(),
        Value::Str(
            std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .unwrap_or_else(|_| "Unknown".to_string()),
        ),
    );
    env.set(
        "$TimeZone".to_string(),
        Value::Real(rug::Float::with_val(
            crate::value::DEFAULT_PRECISION,
            tz_offset,
        )),
    );
    env.set("$SystemId".to_string(), Value::Str(os.to_string()));
    env.set("$Language".to_string(), Value::Str("English".to_string()));
    env.set("$CommandLine".to_string(), Value::Bool(true));
    env.set("$InputLine".to_string(), Value::Null);
}
