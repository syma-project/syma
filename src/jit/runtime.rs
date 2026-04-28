/// Runtime support for JIT-compiled code.
///
/// Defines the `JitContext` that a compiled function receives, and
/// `extern "C"` helpers that compiled code can call for complex
/// operations (env lookup, function calls, list construction, etc.).
use std::collections::HashMap;

use crate::bytecode::CompiledBytecode;
use crate::bytecode::vm::Truthy;
use crate::env::Env;
use crate::eval;
use crate::value::Value;

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
    pub nregs: u32,
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
}

// SAFETY: JitContext is only accessed from the thread that created it.
// It is Send because it may be moved to another thread before use,
// but it is NOT Sync — concurrent access through shared references is
// undefined behavior because the register file and env pointer are
// mutated by JIT-compiled code on a single thread.
unsafe impl Send for JitContext {}

impl JitContext {
    /// Build a context from bytecode + runtime args.
    pub fn new(bc: &CompiledBytecode, args: &[Value], env: &Env, regs: &mut [Value]) -> Self {
        Self {
            regs: regs.as_mut_ptr(),
            nregs: regs.len() as u32,
            constants: bc.constants.as_ptr(),
            nconstants: bc.constants.len() as u32,
            args: args.as_ptr(),
            nargs: args.len() as u32,
            env: env as *const Env,
        }
    }
}

// ── Opcode IDs for jit_binop ──────────────────────────────────────────────

pub const JIT_OP_PLUS: u32 = 1;
pub const JIT_OP_TIMES: u32 = 2;
pub const JIT_OP_POWER: u32 = 3;
pub const JIT_OP_EQUAL: u32 = 4;
pub const JIT_OP_UNEQUAL: u32 = 5;
pub const JIT_OP_LESS: u32 = 6;
pub const JIT_OP_GREATER: u32 = 7;
pub const JIT_OP_LESSEQUAL: u32 = 8;
pub const JIT_OP_GREATEREQUAL: u32 = 9;

// ── Runtime helpers (extern "C" callable from compiled code) ───────────────

#[unsafe(no_mangle)]
pub extern "C" fn jit_load_const(ctx: &mut JitContext, dst: u32, idx: u32) {
    if (idx as usize) < ctx.nconstants as usize {
        unsafe {
            let val = (*ctx.constants.add(idx as usize)).clone();
            *ctx.regs.add(dst as usize) = val;
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_load_arg(ctx: &mut JitContext, dst: u32, idx: u32) {
    if (idx as usize) < ctx.nargs as usize {
        unsafe {
            let val = (*ctx.args.add(idx as usize)).clone();
            *ctx.regs.add(dst as usize) = val;
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_load_true(ctx: &mut JitContext, dst: u32) {
    if (dst as usize) < ctx.nregs as usize {
        unsafe {
            *ctx.regs.add(dst as usize) = Value::Bool(true);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_load_false(ctx: &mut JitContext, dst: u32) {
    if (dst as usize) < ctx.nregs as usize {
        unsafe {
            *ctx.regs.add(dst as usize) = Value::Bool(false);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_load_null(ctx: &mut JitContext, dst: u32) {
    if (dst as usize) < ctx.nregs as usize {
        unsafe {
            *ctx.regs.add(dst as usize) = Value::Null;
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_mov(ctx: &mut JitContext, dst: u32, src: u32) {
    if (dst as usize) < ctx.nregs as usize && (src as usize) < ctx.nregs as usize {
        unsafe {
            *ctx.regs.add(dst as usize) = (*ctx.regs.add(src as usize)).clone();
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_neg(ctx: &mut JitContext, dst: u32, src: u32) {
    if (dst as usize) >= ctx.nregs as usize || (src as usize) >= ctx.nregs as usize {
        return;
    }
    let val = unsafe { (*ctx.regs.add(src as usize)).clone() };
    let env = unsafe { &*ctx.env };
    if let Some(func) = env.get("Minus")
        && let Ok(result) = eval::apply_function(&func, &[val], env)
    {
        unsafe {
            *ctx.regs.add(dst as usize) = result;
        }
        return;
    }
    unsafe {
        *ctx.regs.add(dst as usize) = Value::Null;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_not(ctx: &mut JitContext, dst: u32, src: u32) {
    if (dst as usize) < ctx.nregs as usize && (src as usize) < ctx.nregs as usize {
        let val = unsafe { &*ctx.regs.add(src as usize) };
        unsafe {
            *ctx.regs.add(dst as usize) = Value::Bool(!val.is_truthy());
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_and(ctx: &mut JitContext, dst: u32, a: u32, b: u32) {
    if (dst as usize) < ctx.nregs as usize
        && (a as usize) < ctx.nregs as usize
        && (b as usize) < ctx.nregs as usize
    {
        let va = unsafe { &*ctx.regs.add(a as usize) };
        let vb = unsafe { &*ctx.regs.add(b as usize) };
        unsafe {
            *ctx.regs.add(dst as usize) = Value::Bool(va.is_truthy() && vb.is_truthy());
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_or(ctx: &mut JitContext, dst: u32, a: u32, b: u32) {
    if (dst as usize) < ctx.nregs as usize
        && (a as usize) < ctx.nregs as usize
        && (b as usize) < ctx.nregs as usize
    {
        let va = unsafe { &*ctx.regs.add(a as usize) };
        let vb = unsafe { &*ctx.regs.add(b as usize) };
        unsafe {
            *ctx.regs.add(dst as usize) = Value::Bool(va.is_truthy() || vb.is_truthy());
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_is_truthy(ctx: &mut JitContext, reg: u32) -> u8 {
    if (reg as usize) < ctx.nregs as usize {
        let val = unsafe { &*ctx.regs.add(reg as usize) };
        val.is_truthy() as u8
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_sub(ctx: &mut JitContext, dst: u32, a: u32, b: u32) {
    if (dst as usize) >= ctx.nregs as usize
        || (a as usize) >= ctx.nregs as usize
        || (b as usize) >= ctx.nregs as usize
    {
        return;
    }
    let va = unsafe { (*ctx.regs.add(a as usize)).clone() };
    let vb = unsafe { (*ctx.regs.add(b as usize)).clone() };
    let env = unsafe { &*ctx.env };
    // Sub(a, b) = Plus[a, Minus[b]]
    if let Some(minus_fn) = env.get("Minus")
        && let Ok(neg_b) = eval::apply_function(&minus_fn, &[vb], env)
        && let Some(plus_fn) = env.get("Plus")
        && let Ok(result) = eval::apply_function(&plus_fn, &[va, neg_b], env)
    {
        unsafe {
            *ctx.regs.add(dst as usize) = result;
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_div(ctx: &mut JitContext, dst: u32, a: u32, b: u32) {
    if (dst as usize) >= ctx.nregs as usize
        || (a as usize) >= ctx.nregs as usize
        || (b as usize) >= ctx.nregs as usize
    {
        return;
    }
    let va = unsafe { (*ctx.regs.add(a as usize)).clone() };
    let vb = unsafe { (*ctx.regs.add(b as usize)).clone() };
    let env = unsafe { &*ctx.env };
    // Div(a, b) = Times[a, Power[b, -1]]
    let minus_one = Value::Integer((-1).into());
    if let Some(power_fn) = env.get("Power")
        && let Ok(inv_b) = eval::apply_function(&power_fn, &[vb, minus_one], env)
        && let Some(times_fn) = env.get("Times")
        && let Ok(result) = eval::apply_function(&times_fn, &[va, inv_b], env)
    {
        unsafe {
            *ctx.regs.add(dst as usize) = result;
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_binop(ctx: &mut JitContext, dst: u32, a: u32, b: u32, op: u32) {
    if (dst as usize) >= ctx.nregs as usize
        || (a as usize) >= ctx.nregs as usize
        || (b as usize) >= ctx.nregs as usize
    {
        return;
    }
    let va = unsafe { (*ctx.regs.add(a as usize)).clone() };
    let vb = unsafe { (*ctx.regs.add(b as usize)).clone() };
    let env = unsafe { &*ctx.env };
    let name = match op {
        JIT_OP_PLUS => "Plus",
        JIT_OP_TIMES => "Times",
        JIT_OP_POWER => "Power",
        JIT_OP_EQUAL => "Equal",
        JIT_OP_UNEQUAL => "Unequal",
        JIT_OP_LESS => "Less",
        JIT_OP_GREATER => "Greater",
        JIT_OP_LESSEQUAL => "LessEqual",
        JIT_OP_GREATEREQUAL => "GreaterEqual",
        _ => return,
    };
    if let Some(func) = env.get(name)
        && let Ok(result) = eval::apply_function(&func, &[va, vb], env)
    {
        unsafe {
            *ctx.regs.add(dst as usize) = result;
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_make_list(ctx: &mut JitContext, dst: u32, n: u32) {
    if (dst as usize) >= ctx.nregs as usize {
        return;
    }
    let mut items = Vec::with_capacity(n as usize);
    for i in 1..=n {
        if ((dst + i) as usize) < ctx.nregs as usize {
            items.push(unsafe { (*ctx.regs.add((dst + i) as usize)).clone() });
        }
    }
    unsafe {
        *ctx.regs.add(dst as usize) = Value::List(items);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_make_assoc(ctx: &mut JitContext, dst: u32, n: u32) {
    if (dst as usize) >= ctx.nregs as usize {
        return;
    }
    let mut pairs = Vec::with_capacity(n as usize);
    for i in 0..n {
        let key_idx = (dst + 1 + 2 * i) as usize;
        let val_idx = (dst + 2 + 2 * i) as usize;
        if key_idx < ctx.nregs as usize && val_idx < ctx.nregs as usize {
            let key = unsafe { (*ctx.regs.add(key_idx)).clone() };
            let val = unsafe { (*ctx.regs.add(val_idx)).clone() };
            pairs.push((key, val));
        }
    }
    let mut map = HashMap::new();
    for (key, val) in pairs {
        if let Value::Str(k) = key {
            map.insert(k, val);
        }
    }
    unsafe {
        *ctx.regs.add(dst as usize) = Value::Assoc(map);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_make_seq(ctx: &mut JitContext, dst: u32, start: u32) {
    if (dst as usize) >= ctx.nregs as usize {
        return;
    }
    let count = if (start as usize) < ctx.nargs as usize {
        ctx.nargs - start
    } else {
        0
    };
    let mut items = Vec::with_capacity(count as usize);
    for i in 0..count {
        items.push(unsafe { (*ctx.args.add((start + i) as usize)).clone() });
    }
    unsafe {
        *ctx.regs.add(dst as usize) = Value::List(items);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_load_sym(ctx: &mut JitContext, dst: u32, idx: u32) {
    if (idx as usize) >= ctx.nconstants as usize || (dst as usize) >= ctx.nregs as usize {
        return;
    }
    let name_const = unsafe { &*ctx.constants.add(idx as usize) };
    if let Value::Str(name) = name_const {
        let env = unsafe { &*ctx.env };
        if let Some(val) = env.get(name) {
            unsafe {
                *ctx.regs.add(dst as usize) = val.clone();
            }
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_store_sym(ctx: &mut JitContext, idx: u32, src: u32) {
    if (idx as usize) >= ctx.nconstants as usize || (src as usize) >= ctx.nregs as usize {
        return;
    }
    let name_const = unsafe { &*ctx.constants.add(idx as usize) };
    if let Value::Str(name) = name_const {
        let val = unsafe { (*ctx.regs.add(src as usize)).clone() };
        let env = unsafe { &*ctx.env };
        env.set(name.clone(), val);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_apply(ctx: &mut JitContext, dst: u32, nargs: u32) {
    if (dst as usize) >= ctx.nregs as usize {
        return;
    }
    let func = unsafe { (*ctx.regs.add(dst as usize)).clone() };
    let mut args = Vec::with_capacity(nargs as usize);
    for i in 0..nargs as usize {
        let arg_idx = dst as usize + 1 + i;
        if arg_idx < ctx.nregs as usize {
            args.push(unsafe { (*ctx.regs.add(arg_idx)).clone() });
        }
    }
    let env = unsafe { &*ctx.env };
    match eval::apply_function(&func, &args, env) {
        Ok(result) => unsafe { *ctx.regs.add(dst as usize) = result },
        Err(_) => unsafe { *ctx.regs.add(dst as usize) = Value::Null },
    }
}
