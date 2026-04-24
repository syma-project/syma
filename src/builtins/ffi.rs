/// FFI builtins exposed to Syma code.
///
/// These are thin wrappers around the `ffi` module.  Builtins that need
/// access to the environment (`LibraryFunction`, `LibraryFunctionLoad`)
/// are handled as special cases in `eval.rs`; the ones here are
/// env-independent.
use crate::value::{EvalError, Value};

// ── LoadLibrary ───────────────────────────────────────────────────────────────

/// `LoadLibrary["path/to/lib.so"]` — registered as env-aware in eval.rs.
/// This stub is never called directly; it exists only for help text.
#[allow(dead_code)]
pub fn builtin_load_library(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::FfiError(
        "LoadLibrary is env-aware and must be called via eval.rs dispatch".to_string(),
    ))
}

// ── LoadExtension ─────────────────────────────────────────────────────────────

/// `LoadExtension["path/to/ext.so"]` — env-aware, handled in eval.rs.
#[allow(dead_code)]
pub fn builtin_load_extension(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::FfiError(
        "LoadExtension is env-aware and must be called via eval.rs dispatch".to_string(),
    ))
}

// ── ExternalEvaluate ──────────────────────────────────────────────────────────

/// `ExternalEvaluate["Python", opts]` — env-aware, handled in eval.rs.
#[allow(dead_code)]
pub fn builtin_external_evaluate(_args: &[Value]) -> Result<Value, EvalError> {
    Err(EvalError::FfiError(
        "ExternalEvaluate is env-aware and must be called via eval.rs dispatch".to_string(),
    ))
}
