/// Native JIT compiler (Phase 3).
///
/// Translates bytecode to native code via Cranelift.  Behind the `"jit"`
/// feature flag.
///
/// # Architecture
///
/// A compiled JIT function receives a `JitContext` pointer and returns no
/// value — the caller reads `ctx.regs[0]` for the result.  All operations
/// (arithmetic, env lookups, list construction) are delegated to
/// `extern "C"` runtime helpers defined in [`runtime`].
///
/// The function pointer has the ABI:
///
/// ```ignore
/// extern "C" fn(ctx: *mut JitContext)
/// ```
pub mod compiler;
pub mod runtime;

use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use crate::bytecode::BytecodeFunctionDef;

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
    let fn_ptr = compiler::compile(&bc_def.bytecode, &bc_def.name).ok()?;
    Some(JITFunction {
        name: bc_def.name.clone(),
        fn_ptr: fn_ptr as usize,
        call_count: bc_def.call_count.clone(),
    })
}
