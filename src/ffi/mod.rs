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
    use std::ffi::CString;

    unsafe extern "C" {
        fn dlopen(filename: *const libc_c_char, flag: i32) -> usize;
        fn dlsym(handle: usize, symbol: *const libc_c_char) -> usize;
        fn dlclose(handle: usize) -> i32;
        fn dlerror() -> *const libc_c_char;
    }

    #[allow(non_camel_case_types)]
    type libc_c_char = i8;
    const RTLD_NOW: i32 = 2;
    #[cfg(target_os = "macos")]
    const RTLD_LOCAL: i32 = 4;
    #[cfg(not(target_os = "macos"))]
    const RTLD_LOCAL: i32 = 0;

    pub fn open(path: &str) -> Result<usize, String> {
        let cpath = CString::new(path).map_err(|e| format!("invalid path: {e}"))?;
        let handle = unsafe { dlopen(cpath.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
        if handle == 0 {
            let msg = unsafe {
                let p = dlerror();
                if p.is_null() {
                    "unknown error".to_string()
                } else {
                    std::ffi::CStr::from_ptr(p)
                        .to_string_lossy()
                        .into_owned()
                }
            };
            Err(format!("dlopen({path}): {msg}"))
        } else {
            Ok(handle)
        }
    }

    pub fn sym(handle: usize, name: &str) -> Option<usize> {
        let cname = CString::new(name).ok()?;
        let ptr = unsafe { dlsym(handle, cname.as_ptr()) };
        if ptr == 0 { None } else { Some(ptr) }
    }

    pub fn close(handle: usize) {
        unsafe { dlclose(handle); }
    }
}

#[cfg(windows)]
mod platform {
    use std::ffi::CString;

    extern "system" {
        fn LoadLibraryA(lp_lib_file_name: *const u8) -> usize;
        fn GetProcAddress(h_module: usize, lp_proc_name: *const u8) -> usize;
        fn FreeLibrary(h_lib_module: usize) -> i32;
    }

    pub fn open(path: &str) -> Result<usize, String> {
        let cpath = CString::new(path).map_err(|e| format!("invalid path: {e}"))?;
        let handle = unsafe { LoadLibraryA(cpath.as_ptr() as *const u8) };
        if handle == 0 {
            Err(format!("LoadLibraryA({path}) failed"))
        } else {
            Ok(handle)
        }
    }

    pub fn sym(handle: usize, name: &str) -> Option<usize> {
        let cname = CString::new(name).ok()?;
        let ptr = unsafe { GetProcAddress(handle, cname.as_ptr() as *const u8) };
        if ptr == 0 { None } else { Some(ptr) }
    }

    pub fn close(handle: usize) {
        unsafe { FreeLibrary(handle); }
    }
}
