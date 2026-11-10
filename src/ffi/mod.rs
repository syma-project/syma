/// Foreign Function Interface (FFI) support for Syma.
///
/// Three tiers of foreign-language interop:
///   Tier 1 — Raw C/C++/Rust dynamic libraries via `dlopen`/`dlsym`.
///   Tier 2 — Python via subprocess JSON bridge.
///   Tier 3 — Native Syma extension packages (Rust crates with `syma_init` ABI).
pub mod extension;
pub mod loader;
pub mod marshal;
pub mod python;

// Platform-level dynamic-library primitives.
// We avoid the `libloading` crate so that the zero-external-deps property is preserved.
// On Unix we call dlopen/dlsym/dlclose via `extern "C"` declarations.
// On Windows we call LoadLibraryA/GetProcAddress/FreeLibrary.

use crate::env::Env;
use crate::value::{EvalError, NativeLibHandle, Value};
use std::sync::Arc;

/// Open a dynamic library by file path. Returns the handle on success.
pub fn lib_open(path: &str) -> Result<Arc<NativeLibHandle>, String> {
    if path.is_empty() {
        return Err("empty library path".to_string());
    }
    let raw = platform::open(path)?;
    Ok(Arc::new(NativeLibHandle { raw }))
}

/// Resolve a symbol in the library, returning its address as a raw `usize`.
pub fn lib_sym(handle: &NativeLibHandle, symbol: &str) -> Option<usize> {
    platform::sym(handle.raw, symbol)
}

/// Close the library. Called by `NativeLibHandle::Drop`.
pub(crate) fn lib_close(raw: usize) {
    platform::close(raw);
}

impl Drop for NativeLibHandle {
    fn drop(&mut self) {
        if self.raw != 0 {
            lib_close(self.raw);
        }
    }
}

// ── Platform implementations ──────────────────────────────────────────────────

#[cfg(unix)]
mod platform {
    use std::ffi::{CStr, CString, c_void};
    use std::os::raw::{c_char, c_int};

    unsafe extern "C" {
        fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dlclose(handle: *mut c_void) -> c_int;
        fn dlerror() -> *const c_char;
    }

    const RTLD_NOW: c_int = 2;
    #[cfg(target_os = "macos")]
    const RTLD_LOCAL: c_int = 4;
    #[cfg(not(target_os = "macos"))]
    const RTLD_LOCAL: c_int = 0;

    pub fn open(path: &str) -> Result<usize, String> {
        let cpath = CString::new(path).map_err(|e| format!("invalid path: {e}"))?;
        // Clear any stale error before dlopen so a failure message is ours.
        unsafe {
            dlerror();
        }
        let handle = unsafe { dlopen(cpath.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
        if handle.is_null() {
            let msg = unsafe {
                let p = dlerror();
                if p.is_null() {
                    "unknown error".to_string()
                } else {
                    CStr::from_ptr(p).to_string_lossy().into_owned()
                }
            };
            Err(format!("dlopen({path}): {msg}"))
        } else {
            Ok(handle as usize)
        }
    }

    pub fn sym(handle: usize, name: &str) -> Option<usize> {
        let cname = CString::new(name).ok()?;
        let ptr = unsafe { dlsym(handle as *mut c_void, cname.as_ptr()) };
        if ptr.is_null() {
            None
        } else {
            Some(ptr as usize)
        }
    }

    pub fn close(handle: usize) {
        if handle != 0 {
            unsafe {
                dlclose(handle as *mut c_void);
            }
        }
    }
}

#[cfg(windows)]
mod platform {
    use std::ffi::{CString, c_void};
    use std::os::raw::{c_char, c_int};

    unsafe extern "system" {
        fn LoadLibraryA(lp_lib_file_name: *const c_char) -> *mut c_void;
        fn GetProcAddress(h_module: *mut c_void, lp_proc_name: *const c_char) -> *mut c_void;
        fn FreeLibrary(h_lib_module: *mut c_void) -> c_int;
    }

    pub fn open(path: &str) -> Result<usize, String> {
        let cpath = CString::new(path).map_err(|e| format!("invalid path: {e}"))?;
        let handle = unsafe { LoadLibraryA(cpath.as_ptr()) };
        if handle.is_null() {
            Err(format!("LoadLibraryA({path}) failed"))
        } else {
            Ok(handle as usize)
        }
    }

    pub fn sym(handle: usize, name: &str) -> Option<usize> {
        let cname = CString::new(name).ok()?;
        let ptr = unsafe { GetProcAddress(handle as *mut c_void, cname.as_ptr()) };
        if ptr.is_null() {
            None
        } else {
            Some(ptr as usize)
        }
    }

    pub fn close(handle: usize) {
        if handle != 0 {
            unsafe {
                FreeLibrary(handle as *mut c_void);
            }
        }
    }
}

// ── Library search: `$LibraryPath` / `FindLibrary` ────────────────────────────

/// Directories searched by `FindLibrary`.
///
/// Honours `SYMA_LIBRARY_PATH`, `LD_LIBRARY_PATH` and `LIBRARY_PATH`, then falls
/// back to conventional system directories. Order is significant: earlier
/// entries win.
pub fn default_library_paths() -> Vec<String> {
    let mut dirs: Vec<String> = Vec::new();
    let mut push_env = |var: &str| {
        if let Ok(val) = std::env::var(var) {
            for part in std::env::split_paths(&val) {
                let s = part.to_string_lossy().into_owned();
                if !s.is_empty() && !dirs.contains(&s) {
                    dirs.push(s);
                }
            }
        }
    };
    push_env("SYMA_LIBRARY_PATH");
    push_env("LD_LIBRARY_PATH");
    push_env("LIBRARY_PATH");

    #[cfg(unix)]
    for d in [
        "/usr/local/lib",
        "/usr/lib",
        "/lib",
        "/usr/lib/x86_64-linux-gnu",
        "/lib/x86_64-linux-gnu",
        "/usr/lib/aarch64-linux-gnu",
        "/lib/aarch64-linux-gnu",
    ] {
        let s = d.to_string();
        if !dirs.contains(&s) {
            dirs.push(s);
        }
    }

    dirs
}

/// Platform-appropriate file-name candidates for a bare library name.
fn library_candidates(name: &str) -> Vec<String> {
    let mut v = vec![name.to_string()];
    if std::path::Path::new(name).extension().is_none() {
        #[cfg(target_os = "windows")]
        {
            v.push(format!("{name}.dll"));
            v.push(format!("lib{name}.dll"));
        }
        #[cfg(target_os = "macos")]
        {
            v.push(format!("lib{name}.dylib"));
            v.push(format!("{name}.dylib"));
            v.push(format!("lib{name}.so"));
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            v.push(format!("lib{name}.so"));
            v.push(format!("{name}.so"));
        }
        #[cfg(not(any(unix, target_os = "windows")))]
        {
            v.push(format!("lib{name}.so"));
        }
    }
    v
}

/// Search `search_dirs` for a library named `name` and return its full path.
///
/// `name` may be a bare name (`"m"`), a file name (`"libm.so.6"`) or an already
/// qualified path; a direct file hit is preferred over directory search.
pub fn find_library(name: &str, search_dirs: &[String]) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    if std::path::Path::new(name).is_file() {
        return Some(name.to_string());
    }
    let candidates = library_candidates(name);
    for dir in search_dirs {
        for cand in &candidates {
            let full = std::path::Path::new(dir).join(cand);
            if full.is_file() {
                return Some(full.to_string_lossy().into_owned());
            }
        }
    }
    None
}

/// Search path currently in effect for `env` (from `$LibraryPath`).
pub fn library_search_dirs(env: &Env) -> Vec<String> {
    match env.get("$LibraryPath") {
        Some(Value::List(items)) => items
            .iter()
            .filter_map(|v| match v {
                Value::Str(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => default_library_paths(),
    }
}

/// Install `$LibraryPath` into a fresh environment.
pub fn register(env: &Env) {
    if env.get("$LibraryPath").is_none() {
        let paths = default_library_paths()
            .into_iter()
            .map(Value::Str)
            .collect();
        env.set("$LibraryPath".to_string(), Value::List(paths));
    }
}

/// `FindLibrary["name"]` — locate a shared library on `$LibraryPath`.
///
/// Returns the full path as a string, or `Null` when it cannot be found.
pub fn builtin_find_library(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FindLibrary requires exactly 1 argument".to_string(),
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
    let dirs = library_search_dirs(env);
    match find_library(&name, &dirs) {
        Some(path) => Ok(Value::Str(path)),
        None => Ok(Value::Null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lib_open_empty_path_errors() {
        assert!(lib_open("").is_err());
    }

    #[test]
    fn test_lib_open_missing_path_errors() {
        let err = lib_open("/no/such/library_xyz.so").unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn test_find_library_candidate_names() {
        let c = library_candidates("m");
        assert!(c.contains(&"m".to_string()));
        #[cfg(all(unix, not(target_os = "macos")))]
        assert!(c.contains(&"libm.so".to_string()));
    }

    #[test]
    fn test_find_library_missing_is_none() {
        assert!(find_library("definitely_not_a_real_lib_xyz", &default_library_paths()).is_none());
        assert!(find_library("", &default_library_paths()).is_none());
    }

    #[test]
    fn test_find_library_absolute_existing_file() {
        // Cargo.toml always exists relative to the crate root.
        if std::path::Path::new("Cargo.toml").is_file() {
            assert_eq!(
                find_library("Cargo.toml", &[]),
                Some("Cargo.toml".to_string())
            );
        }
    }

    #[test]
    fn test_find_library_finds_libc_on_linux() {
        if cfg!(target_os = "linux") {
            // `None` on unusual distro layouts is not a hard failure.
            if let Some(p) = find_library("libc.so.6", &default_library_paths()) {
                assert!(p.ends_with("libc.so.6"));
            }
        }
    }

    #[test]
    fn test_register_sets_library_path() {
        let env = Env::new();
        register(&env);
        match env.get("$LibraryPath") {
            Some(Value::List(items)) => {
                assert!(!items.is_empty());
                assert!(items.iter().all(|v| matches!(v, Value::Str(_))));
            }
            other => panic!("expected $LibraryPath list, got {other:?}"),
        }
    }

    #[test]
    fn test_register_preserves_existing_library_path() {
        let env = Env::new();
        env.set(
            "$LibraryPath".to_string(),
            Value::List(vec![Value::Str("/custom".into())]),
        );
        register(&env);
        assert_eq!(library_search_dirs(&env), vec!["/custom".to_string()]);
    }

    #[test]
    fn test_builtin_find_library_missing_returns_null() {
        let env = Env::new();
        register(&env);
        let out = builtin_find_library(&[Value::Str("no_such_lib_zzz".into())], &env).unwrap();
        assert_eq!(out, Value::Null);
    }

    #[test]
    fn test_builtin_find_library_type_error() {
        let env = Env::new();
        assert!(builtin_find_library(&[Value::Integer(1.into())], &env).is_err());
        assert!(builtin_find_library(&[], &env).is_err());
    }
}
