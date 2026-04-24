/// Runtime support for JIT-compiled code.
///
/// Defines the `JitContext` that a compiled function receives, and
/// `extern "C"` helpers that compiled code can call for complex
/// operations (env lookup, function calls, list construction, etc.).

use crate::bytecode::CompiledBytecode;
use crate::env::Env;
use crate::eval;
use crate::value::{EvalError, Value};

/// Context pointer passed to every JIT-compiled function.
///
/// The compiled function reads/writes registers, reads the constant
/// pool, and delegates to runtime helpers via this context.
#[derive(Debug)]
#[repr(C)]
pub struct JitContext {
    /// Virtual register file.
    pub regs: *mut Value,
    /// Number of registers available.
    pub nregs: usize,
    /// Constant pool pointer.
    pub constants: *const Value,
    /// Number of constants.
    pub nconstants: u32,
    /// Argument values passed to the function.
    pub args: *const Value,
    /// Number of arguments.
    pub nargs: u32,
    /// Evaluation environment.
    pub env: *const Env,
    /// Scratch space for temporary values.
    pub scratch: Value,
}

// SAFETY: JitContext is only accessed from the thread that created it.
unsafe impl Send for JitContext {}
unsafe impl Sync for JitContext {}

impl JitContext {
    /// Build a context from bytecode + runtime args.
    pub fn new(
        bc: &CompiledBytecode,
        args: &[Value],
        env: &Env,
        regs: &mut [Value],
    ) -> Self {
        Self {
            regs: regs.as_mut_ptr(),
            nregs: regs.len(),
            constants: bc.constants.as_ptr(),
            nconstants: bc.constants.len() as u32,
            args: args.as_ptr(),
            nargs: args.len() as u32,
            env: env as *const Env,
            scratch: Value::Null,
        }
    }
}

// ── Runtime helpers (extern "C" callable from compiled code) ────────────

/// Load a constant from the pool.
#[no_mangle]
pub extern "C" fn jit_load_const(ctx: &mut JitContext, dst: u8, idx: u32) {
    if (idx as usize) < ctx.nconstants as usize {
        unsafe {
            let val = (*ctx.constances.add(idx as usize)).clone();
            *ctx.regs.add(dst as usize) = val;
        }
    }
}

/// Load an argument into a register.
#[no_mangle]
pub extern "C" fn jit_load_arg(ctx: &mut JitContext, dst: u8, idx: u32) {
    if (idx as usize) < ctx.nargs as usize {
        unsafe {
            let val = (*ctx.args.add(idx as usize)).clone();
            *ctx.regs.add(dst as usize) = val;
        }
    }
}

/// Perform a binary builtin operation (e.g. Plus, Times).
#[no_mangle]
pub extern "C" fn jit_builtin2(
    ctx: &mut JitContext,
    name: *const u8,
    name_len: usize,
    a: &Value,
    b: &Value,
    dst: u8,
) {
    let s = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(name, name_len)) };
    let env = unsafe { &*ctx.env };
    let func = env.get(s).unwrap_or_else(|| Value::Symbol(s.to_string()));
    match eval::apply_function(&func, &[a.clone(), b.clone()], env) {
        Ok(val) => unsafe { *ctx.regs.add(dst as usize) = val },
        Err(_) => {}
    }
}
