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

#[cfg(test)]
mod diff_tests;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::bytecode::BytecodeFunctionDef;
use crate::env::Env;
use crate::value::{EvalError, Value};

pub use runtime::JitContext;

/// A function compiled to native code.
#[derive(Debug, Clone)]
pub struct JITFunction {
    /// Human-readable name.
    pub name: String,
    /// Pointer to the compiled native code.
    pub fn_ptr: usize,
    /// Size of the executable memory allocation (for JitModule tracking).
    pub alloc_size: usize,
    /// How many times this function was called.
    pub call_count: Arc<AtomicU64>,
}

/// Compile a bytecode function into a JIT function.
///
/// Returns `None` if compilation fails (e.g. unsupported bytecode).
pub fn compile_jit(bc_def: &BytecodeFunctionDef) -> Option<JITFunction> {
    let (fn_ptr, alloc_size) = compiler::compile(&bc_def.bytecode, &bc_def.name).ok()?;
    Some(JITFunction {
        name: bc_def.name.clone(),
        fn_ptr: fn_ptr as usize,
        alloc_size,
        call_count: bc_def.call_count.clone(),
    })
}

/// Execute a JIT-compiled function and return its result.
///
/// Mirrors [`crate::bytecode::vm::execute_bytecode`]: a fresh child scope is
/// created and the parameters are bound by name (so closures created in the
/// body capture them), the arguments are loaded into the register file, and a
/// propagated `Return` is unwrapped to the function result. Errors recorded by
/// runtime helpers are surfaced here rather than being swallowed.
///
/// # Panics
/// Panics only if called while `bc_def.jit_fn_ptr` is null or the failure
/// sentinel; the caller is expected to compile first.
pub fn invoke(bc_def: &BytecodeFunctionDef, args: &[Value], env: &Env) -> Result<Value, EvalError> {
    const JIT_FAILED: usize = 1;
    debug_assert!({
        let p = bc_def.jit_fn_ptr.load(Ordering::Relaxed);
        !p.is_null() && p as usize != JIT_FAILED
    });

    let bytecode = &bc_def.bytecode;
    let nregs = bytecode.nregs.max(args.len() as u16).max(1) as usize;
    let mut regs = vec![Value::Null; nregs];
    for (i, arg) in args.iter().enumerate() {
        if i < nregs {
            regs[i] = arg.clone();
        }
    }

    // Match the interpreter's per-call frame so parameter-binding semantics
    // (and hence closure capture) are identical on both tiers.
    let local = env.child();
    for (i, name) in bytecode.param_names.iter().enumerate() {
        if !name.is_empty() {
            local.set(name.clone(), args.get(i).cloned().unwrap_or(Value::Null));
        }
    }

    let mut ctx = JitContext::new(bytecode, args, &local, &mut regs);
    let jit_ptr = bc_def.jit_fn_ptr.load(Ordering::Relaxed);
    let jit_fn: extern "C" fn(*mut JitContext) = unsafe { std::mem::transmute(jit_ptr) };
    jit_fn(&mut ctx);

    if let Some(e) = ctx.take_error() {
        return match e {
            EvalError::Return(v) => Ok(*v),
            other => Err(other),
        };
    }
    Ok(unsafe { (*ctx.regs).clone() })
}
