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

use crate::value::NativeLibHandle;
use std::sync::Arc;

/// Open a dynamic library by file path. Returns the handle on success.
pub fn lib_open(path: &str) -> Result<Arc<NativeLibHandle>, String> {
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
