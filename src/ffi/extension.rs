/// Tier 3: Native Syma extension packages.
///
/// An extension is a compiled Rust (or C) dynamic library that exports a
/// single C-ABI function:
///
/// ```c
/// void syma_init(SymaExtensionContext *ctx);
/// ```
///
/// Inside `syma_init` the extension calls `ctx->register_fn` to register
/// one or more builtins.  Those builtins then appear in the environment as
/// ordinary `Value::Builtin` entries.
///
/// Arguments and results are exchanged as null-terminated JSON strings so
/// that the `Value` binary layout never leaks through the ABI.
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};

use crate::env::Env;
use crate::ffi::marshal::{json_to_value, values_to_json};
use crate::ffi::{lib_open, lib_sym};
use crate::value::{EvalError, Value};

// ── Stable C ABI ──────────────────────────────────────────────────────────────

/// Passed to `syma_init` by the loader.  The extension calls `register_fn`
/// for each function it wants to expose.
///
/// This struct is `repr(C)` and designed for forward compatibility:
/// fields are never removed, only appended at the end.
#[repr(C)]
pub struct SymaExtensionContext {
    /// Version of this ABI.  Currently 1.
    pub abi_version: u32,
    /// Called by the extension to register a builtin.
    pub register_fn:
        unsafe extern "C" fn(ctx: *mut SymaExtensionContext, name: *const c_char, f: SymaBuiltinFn),
    /// Opaque back-pointer to the Rust `Env` (do not use from extension code).
    pub env_opaque: *mut c_void,
}

/// The C-callable signature that every extension builtin must implement.
///
/// `args_json` — pointer to a null-terminated JSON array of arguments.
/// `out_buf`   — caller-provided buffer for the JSON result.
/// `out_len`   — size of `out_buf`.
/// `err_buf`   — caller-provided buffer for an error message.
/// `err_len`   — size of `err_buf`.
///
/// Returns 0 on success (result in `out_buf`), non-zero on error (message in `err_buf`).
pub type SymaBuiltinFn = unsafe extern "C" fn(
    args_json: *const c_char,
    out_buf: *mut c_char,
    out_len: usize,
    err_buf: *mut c_char,
    err_len: usize,
) -> c_int;

// ── Loader ────────────────────────────────────────────────────────────────────

/// Load a Syma extension from a dynamic library and register its builtins.
pub fn load_extension(path: &str, env: &Env) -> Result<(), EvalError> {
    let handle = lib_open(path).map_err(|e| EvalError::FfiError(format!("LoadExtension: {e}")))?;

    let init_ptr = lib_sym(&handle, "syma_init").ok_or_else(|| {
        EvalError::FfiError(format!(
            "LoadExtension: symbol \"syma_init\" not found in \"{path}\""
        ))
    })?;

    // The env pointer is stable for the lifetime of this call.
    // We box it so we have a stable address.
    let env_clone = env.clone();
    let env_box: Box<Env> = Box::new(env_clone);
    let env_raw = Box::into_raw(env_box) as *mut c_void;

    let mut ctx = SymaExtensionContext {
        abi_version: 1,
        register_fn,
        env_opaque: env_raw,
    };

    // SAFETY: the extension must implement `syma_init` with the documented signature.
    unsafe {
        let init: unsafe extern "C" fn(*mut SymaExtensionContext) = std::mem::transmute(init_ptr);
        init(&mut ctx);
    }

    // Reclaim the boxed env to avoid a leak.
    let _ = unsafe { Box::from_raw(ctx.env_opaque as *mut Env) };

    // Store the handle in the registry so it stays live.
    env.register_native_lib(path.to_string(), handle);

    Ok(())
}

// ── Callback registered with extensions ──────────────────────────────────────

#[allow(unsafe_op_in_unsafe_fn)]
unsafe extern "C" fn register_fn(
    ctx: *mut SymaExtensionContext,
    name_ptr: *const c_char,
    f: SymaBuiltinFn,
) {
    if ctx.is_null() || name_ptr.is_null() {
        return;
    }
    let env = &*((*ctx).env_opaque as *const Env);
    let name = CStr::from_ptr(name_ptr).to_string_lossy().into_owned();

    // Because `BuiltinFn` is `fn(&[Value]) -> ...` (a bare fn pointer, not a closure),
    // we cannot directly capture `f`.  We store the mapping `name -> f` in a global
    // registry and look it up at call time via the trampoline in apply_function.
    register_ext_fn(name.clone(), f);

    // Register a builtin whose name matches the key we just stored.
    env.set(name.clone(), ext_builtin_stub(&name));
}

// ── Global extension function registry ───────────────────────────────────────
// Maps extension function name -> C function pointer.
// This is the simplest approach given that `BuiltinFn` is a bare fn pointer.

use std::collections::HashMap;
use std::sync::Mutex;

static EXT_FN_REGISTRY: std::sync::OnceLock<Mutex<HashMap<String, usize>>> =
    std::sync::OnceLock::new();

fn ext_registry() -> &'static Mutex<HashMap<String, usize>> {
    EXT_FN_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_ext_fn(name: String, f: SymaBuiltinFn) {
    ext_registry().lock().unwrap().insert(name, f as usize);
}

/// Look up a registered extension function by name and call it.
pub fn call_ext_fn(name: &str, args: &[Value]) -> Result<Value, EvalError> {
    let fn_ptr = { ext_registry().lock().unwrap().get(name).copied() };

    let fn_ptr = fn_ptr.ok_or_else(|| {
        EvalError::FfiError(format!(
            "extension function \"{name}\" not found in registry"
        ))
    })?;

    let f: SymaBuiltinFn = unsafe { std::mem::transmute(fn_ptr) };

    let args_json = values_to_json(args)?;
    let args_cstr = CString::new(args_json)
        .map_err(|e| EvalError::FfiError(format!("JSON arg serialisation: {e}")))?;

    const BUF_SIZE: usize = 65536;
    let mut out_buf = vec![0i8; BUF_SIZE];
    let mut err_buf = vec![0i8; BUF_SIZE];

    let rc = unsafe {
        f(
            args_cstr.as_ptr(),
            out_buf.as_mut_ptr(),
            BUF_SIZE,
            err_buf.as_mut_ptr(),
            BUF_SIZE,
        )
    };

    if rc != 0 {
        let msg = unsafe {
            CStr::from_ptr(err_buf.as_ptr())
                .to_string_lossy()
                .into_owned()
        };
        return Err(EvalError::FfiError(format!(
            "extension \"{name}\" error: {msg}"
        )));
    }

    let out_str = unsafe {
        CStr::from_ptr(out_buf.as_ptr())
            .to_string_lossy()
            .into_owned()
    };
    json_to_value(&out_str)
}

// ── Stub generator ────────────────────────────────────────────────────────────

/// Generate a `Value` that, when called, looks up `name` in the extension registry.
/// We can't close over `name` in a bare `fn` pointer, so instead we store the name
/// as a `Value::Symbol` and rely on the evaluator's symbol→Builtin lookup.
///
/// The actual dispatch happens in `builtins/ffi.rs`'s `builtin_ext_call`, which
/// checks whether the callee name is in the ext registry.
fn ext_builtin_stub(name: &str) -> Value {
    // We register the name in the environment as a Builtin value whose function pointer
    // is the generic `ext_dispatch` trampoline.  The trampoline itself can't know which
    // name it was called under, so we use a thread-local to pass the name.
    //
    // Since BuiltinFn is fn(&[Value]) -> Result<Value, EvalError> (no closure),
    // we rely on the Env binding: store the name as a Symbol, not as a Builtin.
    // The evaluator's Symbol→lookup will find it via `add_values_public` once the
    // actual Builtin(name, fn_ptr) is stored there.
    //
    // Simpler approach: just register as `Value::Symbol(name)` and let the evaluator
    // fall through to `call_ext_fn`. But `Value::Symbol` would just return unevaluated.
    //
    // Cleanest approach given BuiltinFn = bare fn: register a *named* builtin using
    // a match in `apply_function` that checks the ext registry.  The check is O(1)
    // hash-map lookup and happens only for unknown builtins.
    //
    // We register under `name` as a `Value::Builtin(name, _EXT_DISPATCH)`.
    // `_EXT_DISPATCH` is an alias for the generic trampoline below.
    Value::Builtin(name.to_string(), _ext_dispatch)
}

fn _ext_dispatch(args: &[Value]) -> Result<Value, EvalError> {
    // This trampoline is called by `apply_function` for any `Value::Builtin` whose
    // `f` pointer is `_ext_dispatch`.  But we need the *name* — which is the first
    // field of `Value::Builtin(name, _)`.  The name is passed to us only indirectly
    // via the special-case in `apply_function` (see eval.rs changes).
    //
    // For now return an error; the actual dispatch is done in eval.rs by checking
    // whether a Builtin's function pointer equals `_ext_dispatch` and re-routing
    // through `call_ext_fn(name, args)`.
    let _ = args;
    Err(EvalError::FfiError(
        "extension dispatch stub called directly — this is a bug".to_string(),
    ))
}

/// The trampoline function pointer used as the marker for extension builtins.
pub const EXT_DISPATCH_FN: crate::value::BuiltinFn = _ext_dispatch;
