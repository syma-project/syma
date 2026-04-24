//! LocalSymbol — persistent key-value storage.
//!
//! `LocalSymbol["name"]` reads a value from `~/.syma/LocalSymbols/`.
//! `LocalSymbol["name"] = value` writes a value (via the `Set` special form in `eval`).
//! `LocalSymbol["name", default]` returns `default` when the key doesn't exist.
//!
//! Values are persisted as individual JSON files using the existing
//! `value_to_json` / `json_to_value` marshalling from `ffi/marshal.rs`.

use crate::ffi::marshal::{json_to_value, value_to_json};
use crate::value::{EvalError, Value};
use std::path::PathBuf;

/// Return the Syma home directory.
///
/// Respects `$SYMA_HOME` environment variable; falls back to `~/.syma`.
pub(crate) fn syma_home() -> PathBuf {
    if let Ok(home) = std::env::var("SYMA_HOME") {
        PathBuf::from(home)
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".syma")
    } else {
        PathBuf::from(".syma")
    }
}

/// Return the LocalSymbols storage directory, creating it if necessary.
pub(crate) fn local_symbols_dir() -> PathBuf {
    let dir = syma_home().join("LocalSymbols");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Sanitize a symbol name for use as a filename.
///
/// Replaces path separators and other unsafe characters to prevent
/// directory traversal. Only alphanumeric, underscore, hyphen, and
/// dot are allowed; everything else becomes underscore.
pub(crate) fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// LocalSymbol["name"] — read a persisted value.
///
/// Returns the stored value, or `Null` if the key doesn't exist.
/// `LocalSymbol["name", default]` returns `default` instead of `Null`.
pub fn builtin_local_symbol(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "LocalSymbol requires 1 or 2 arguments: LocalSymbol[\"name\"] or LocalSymbol[\"name\", default]"
                .to_string(),
        ));
    }

    let name = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };

    let dir = local_symbols_dir();
    let sanitized = sanitize_name(&name);
    let path = dir.join(format!("{}.json", sanitized));

    match std::fs::read_to_string(&path) {
        Ok(contents) => match json_to_value(&contents) {
            Ok(val) => Ok(val),
            Err(e) => Err(EvalError::Error(format!(
                "LocalSymbol: failed to parse '{}': {}",
                name, e
            ))),
        },
        Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Key not found — return default or Null
            if args.len() >= 2 {
                Ok(args[1].clone())
            } else {
                Ok(Value::Null)
            }
        }
        Err(e) => Err(EvalError::Error(format!(
            "LocalSymbol: failed to read '{}': {}",
            name, e
        ))),
    }
}

/// Write a value to a LocalSymbol. Called from eval.rs Set special form.
///
/// Returns the written value on success.
pub(crate) fn write_local_symbol(name: &str, value: &Value) -> Result<Value, EvalError> {
    let dir = local_symbols_dir();
    let sanitized = sanitize_name(name);
    let path = dir.join(format!("{}.json", sanitized));

    let json = value_to_json(value).map_err(|e| {
        EvalError::Error(format!(
            "LocalSymbol: value type '{}' is not persistable: {}",
            value.type_name(),
            e
        ))
    })?;

    let json_str = serde_json::to_string_pretty(&json)
        .map_err(|e| EvalError::Error(format!("LocalSymbol: serialization failed: {}", e)))?;

    std::fs::write(&path, &json_str).map_err(|e| {
        EvalError::Error(format!("LocalSymbol: failed to write '{}': {}", name, e))
    })?;

    Ok(value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);
    /// Serialises tests that modify the global `SYMA_HOME` env var.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// A minimal temp directory that cleans up on drop.
    /// Uses a unique counter suffix so parallel tests don't collide.
    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            let count = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
            let mut path = std::env::temp_dir();
            path.push(format!("syma_test_{name}_{count}"));
            let _ = std::fs::create_dir_all(&path);
            TestDir { path }
        }

        fn path(&self) -> &std::path::Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("hello"), "hello");
        assert_eq!(sanitize_name("hello_world"), "hello_world");
        assert_eq!(sanitize_name("a/b"), "a_b");
        assert_eq!(sanitize_name("a\\b"), "a_b");
        assert_eq!(sanitize_name("../foo"), "___foo");
        assert_eq!(sanitize_name("a b"), "a_b");
        assert_eq!(sanitize_name("my-key"), "my-key");
        assert_eq!(sanitize_name(""), "");
    }

    #[test]
    fn test_syma_home_respects_env() {
        let home = syma_home();
        assert!(!home.as_os_str().is_empty());
    }

    #[test]
    fn test_local_symbols_dir_creates() {
        let dir = local_symbols_dir();
        assert!(dir.exists());
        assert!(dir.to_string_lossy().contains("LocalSymbols"));
    }

    #[test]
    fn test_read_write_roundtrip() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = TestDir::new("rwp");
        unsafe { std::env::set_var("SYMA_HOME", tmp.path()); }

        let val = Value::Integer(rug::Integer::from(42));
        let result = write_local_symbol("test_key", &val).unwrap();
        assert_eq!(result, val);

        let read_back = builtin_local_symbol(&[Value::Str("test_key".to_string())]).unwrap();
        assert_eq!(read_back, val);

        unsafe { std::env::remove_var("SYMA_HOME"); }
    }

    #[test]
    fn test_read_missing_returns_null() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = TestDir::new("null");
        unsafe { std::env::set_var("SYMA_HOME", tmp.path()); }

        let result = builtin_local_symbol(&[Value::Str("nonexistent".to_string())]).unwrap();
        assert_eq!(result, Value::Null);

        unsafe { std::env::remove_var("SYMA_HOME"); }
    }

    #[test]
    fn test_read_missing_with_default() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = TestDir::new("default");
        unsafe { std::env::set_var("SYMA_HOME", tmp.path()); }

        let default = Value::Str("default_value".to_string());
        let result =
            builtin_local_symbol(&[Value::Str("nonexistent".to_string()), default.clone()])
                .unwrap();
        assert_eq!(result, default);

        unsafe { std::env::remove_var("SYMA_HOME"); }
    }

    #[test]
    fn test_write_and_read_list() {
        let _lock = ENV_LOCK.lock().unwrap();
        let tmp = TestDir::new("list");
        unsafe { std::env::set_var("SYMA_HOME", tmp.path()); }

        let list = Value::List(vec![
            Value::Integer(rug::Integer::from(1)),
            Value::Integer(rug::Integer::from(2)),
            Value::Integer(rug::Integer::from(3)),
        ]);
        write_local_symbol("mylist", &list).unwrap();

        let read_back = builtin_local_symbol(&[Value::Str("mylist".to_string())]).unwrap();
        assert_eq!(read_back, list);

        unsafe { std::env::remove_var("SYMA_HOME"); }
    }
}
