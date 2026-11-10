//! System information builtins.
//!
//! Provides Wolfram Language-style system variables: `$System`, `$Version`,
//! `$Machine`, `$OperatingSystem`, `$User`, `$TimeZone`, `$MachineName`,
//! `$ProcessorType`, `$ReleaseDate`, `$SystemId`, `$CommandLine`, `$InputLine`,
//! `$Language`, plus the `SystemInformation` query function.

use crate::env::Env;
use crate::value::{EvalError, Value};
use rug::Integer;
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
            && let Some(null_pos) = buf.iter().position(|&b| b == 0)
        {
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

    // Protection builtins live in `clearing`; register them here because this
    // registrar is one of the module registrars wired up by `builtins::mod`.
    crate::builtins::clearing::register(env);
    super::register_builtin_env(env, "SystemInformation", builtin_system_information);
}

// ── SystemInformation ──

/// Top-level property groups reported by `SystemInformation`.
const KERNEL_PROPERTIES: &[&str] = &[
    "Version",
    "ReleaseDate",
    "System",
    "SystemID",
    "Machine",
    "MachineName",
    "ProcessorType",
    "OperatingSystem",
    "Language",
];

/// All scalar properties `SystemInformation` can report.
const SCALAR_PROPERTIES: &[&str] = &[
    "Version",
    "ReleaseDate",
    "System",
    "SystemID",
    "Machine",
    "MachineName",
    "ProcessorType",
    "OperatingSystem",
    "Language",
    "Memory",
    "Processors",
    "Devices",
    "Packages",
];

fn rule(name: &str, value: Value) -> Value {
    Value::Rule {
        lhs: Box::new(Value::Str(name.to_string())),
        rhs: Box::new(value),
        delayed: false,
    }
}

fn memory_bytes() -> Value {
    if let Ok(contents) = std::fs::read_to_string("/proc/meminfo") {
        for line in contents.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:")
                && let Some(kb) = rest
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<i64>().ok())
            {
                return Value::Integer(Integer::from(kb) * 1024);
            }
        }
    }
    Value::Null
}

fn processor_count() -> Value {
    let n = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    Value::Integer(Integer::from(n as i64))
}

fn packages(env: &Env) -> Value {
    let registry = env.registry.lock().unwrap();
    let mut names: Vec<String> = registry.keys().cloned().collect();
    names.sort();
    Value::List(names.into_iter().map(Value::Str).collect())
}

fn scalar_property(name: &str, env: &Env) -> Option<Value> {
    let value = match name {
        "Version" => env.get("$Version")?,
        "ReleaseDate" => env.get("$ReleaseDate")?,
        "System" => env.get("$System")?,
        "SystemID" => env.get("$SystemId")?,
        "Machine" => env.get("$Machine")?,
        "MachineName" => env.get("$MachineName")?,
        "ProcessorType" => env.get("$ProcessorType")?,
        "OperatingSystem" => env.get("$OperatingSystem")?,
        "Language" => env.get("$Language")?,
        "Memory" => memory_bytes(),
        "Processors" => processor_count(),
        "Devices" => Value::List(vec![]),
        "Packages" => packages(env),
        _ => return None,
    };
    Some(value)
}

/// `SystemInformation[]` / `SystemInformation[prop]` / `SystemInformation[prop, key]`.
///
/// Returns a list of `prop -> value` rules for the requested (group of)
/// properties, an `Unknown`-free unevaluated call for unrecognized properties.
pub fn builtin_system_information(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() > 2 {
        return Ok(Value::Call {
            head: "SystemInformation".to_string(),
            args: args.to_vec(),
        });
    }

    let some_str = |v: &Value| match v {
        Value::Str(s) | Value::Symbol(s) => Some(s.clone()),
        _ => None,
    };

    // No argument: every scalar property as a rule.
    if args.is_empty() {
        let rules: Vec<Value> = SCALAR_PROPERTIES
            .iter()
            .filter_map(|name| scalar_property(name, env).map(|v| rule(name, v)))
            .collect();
        return Ok(Value::List(rules));
    }

    let Some(property) = some_str(&args[0]) else {
        return Ok(Value::Call {
            head: "SystemInformation".to_string(),
            args: args.to_vec(),
        });
    };

    // `SystemInformation[prop, key]` — return the single requested value.
    if let Some(key) = args.get(1).and_then(some_str) {
        return Ok(scalar_property(&key, env).unwrap_or(Value::Null));
    }

    // Property groups.
    let group: Option<&[&str]> = match property.as_str() {
        "Kernel" | "System" => Some(KERNEL_PROPERTIES),
        "Properties" => {
            let names: Vec<Value> = SCALAR_PROPERTIES
                .iter()
                .map(|s| Value::Str((*s).to_string()))
                .collect();
            return Ok(Value::List(names));
        }
        _ => None,
    };

    if let Some(names) = group {
        let rules: Vec<Value> = names
            .iter()
            .filter_map(|name| scalar_property(name, env).map(|v| rule(name, v)))
            .collect();
        return Ok(Value::List(rules));
    }

    // A single scalar property.
    if let Some(value) = scalar_property(&property, env) {
        return Ok(value);
    }

    Ok(Value::Call {
        head: "SystemInformation".to_string(),
        args: args.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_env() -> Env {
        let env = Env::new();
        register(&env);
        env
    }

    #[test]
    fn test_system_variables_present() {
        let env = test_env();
        assert!(matches!(env.get("$Version"), Some(Value::Str(_))));
        assert!(matches!(env.get("$System"), Some(Value::Str(_))));
        assert!(matches!(env.get("$MachineName"), Some(Value::Str(_))));
    }

    #[test]
    fn test_system_information_version_rule() {
        let env = test_env();
        let result =
            builtin_system_information(&[Value::Str("Version".to_string())], &env).unwrap();
        assert!(matches!(result, Value::Str(_)));
    }

    #[test]
    fn test_system_information_kernel_is_rule_list() {
        let env = test_env();
        let result = builtin_system_information(&[Value::Str("Kernel".to_string())], &env).unwrap();
        match result {
            Value::List(items) => {
                assert!(!items.is_empty());
                assert!(items.iter().all(|v| matches!(v, Value::Rule { .. })));
            }
            other => panic!("expected rule list, got {other:?}"),
        }
    }

    #[test]
    fn test_system_information_empty_lists_properties() {
        let env = test_env();
        let result = builtin_system_information(&[], &env).unwrap();
        match result {
            Value::List(items) => assert!(items.iter().all(|v| matches!(v, Value::Rule { .. }))),
            other => panic!("expected rule list, got {other:?}"),
        }
    }

    #[test]
    fn test_system_information_unknown_stays_unevaluated() {
        let env = test_env();
        let result =
            builtin_system_information(&[Value::Str("NoSuchThing".to_string())], &env).unwrap();
        assert!(matches!(result, Value::Call { ref head, .. } if head == "SystemInformation"));
    }

    #[test]
    fn test_system_information_subkey() {
        let env = test_env();
        let result = builtin_system_information(
            &[
                Value::Str("Kernel".to_string()),
                Value::Str("Version".to_string()),
            ],
            &env,
        )
        .unwrap();
        assert!(matches!(result, Value::Str(_)));
    }
}
