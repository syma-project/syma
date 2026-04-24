/// Native JIT compiler (Phase 3).
///
/// Translates bytecode to native code via Cranelift.  Behind the `"jit"`
/// feature flag.
///
/// # Architecture
///
/// A compiled JIT function receives a `JitContext` pointer and returns a
/// `Value`.  Simple integer/f64 operations are inlined as native
/// instructions; everything else (env lookups, function calls, list ops)
/// calls `extern "C"` runtime helpers.
///
/// The function pointer has the ABI:
///
/// ```ignore
/// extern "C" fn(ctx: *mut JitContext) -> Value
/// ```

pub mod compiler;
pub mod runtime;

use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use crate::bytecode::BytecodeFunctionDef;
use crate::bytecode::CompiledBytecode;
use crate::value::Value;

pub use runtime::JitContext;

/// A function compiled to native code.
#[derive(Debug, Clone)]
pub struct JITFunction {
    /// Human-readable name.
    pub name: String,
    /// Pointer to the compiled native code.
    pub fn_ptr: usize,
    /// How many times this function was called.
    pub call_count: Arc<AtomicU64>,
}

/// Compile a bytecode function into a JIT function.
///
/// Returns `None` if compilation fails (e.g. unsupported bytecode).
pub fn compile_jit(bc_def: &BytecodeFunctionDef) -> Option<JITFunction> {
    compiler::compile(&bc_def.bytecode, &bc_def.name).ok()
}
