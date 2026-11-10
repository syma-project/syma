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
    /// Boxed first error raised by a runtime helper, or null.
    ///
    /// Runtime helpers that delegate back to the tree-walk evaluator can fail.
    /// Rather than silently substituting `Null`, they record the error here;
    /// the compiled code checks [`jit_has_error`] after each fallible helper
    /// call and bails out, mirroring the interpreter's `?` propagation.
    pub err: *mut EvalError,
    /// Pending loop control captured by `jit_eval_tree_loop` (0 none,
    /// 1 break, 2 continue). Consumed by the `jit_take_*` helpers.
    pub ctrl: u8,
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
            err: std::ptr::null_mut(),
            ctrl: 0,
        }
    }

    /// Record `e` as the pending error, preserving the *first* one so that
    /// evaluation aborts with the same error the interpreter would report.
    #[inline]
    pub fn set_error(&mut self, e: EvalError) {
        if self.err.is_null() {
            self.err = Box::into_raw(Box::new(e));
        }
    }

    /// Take the pending error, if any (transfers ownership).
    pub fn take_error(&mut self) -> Option<EvalError> {
        if self.err.is_null() {
            None
        } else {
            let e = unsafe { *Box::from_raw(self.err) };
            self.err = std::ptr::null_mut();
            Some(e)
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
    if (dst as usize) < ctx.nregs as usize && (idx as usize) < ctx.nconstants as usize {
        unsafe {
            let val = (*ctx.constants.add(idx as usize)).clone();
            *ctx.regs.add(dst as usize) = val;
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_load_arg(ctx: &mut JitContext, dst: u32, idx: u32) {
    if (dst as usize) < ctx.nregs as usize && (idx as usize) < ctx.nargs as usize {
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
pub extern "C" fn jit_has_error(ctx: &mut JitContext) -> u8 {
    (!ctx.err.is_null()) as u8
}

/// Evaluate a held expression (constant pool index `idx`) with the tree-walk
/// evaluator. Mirrors `Instruction::EvalTree` in the VM.
#[unsafe(no_mangle)]
pub extern "C" fn jit_eval_tree(ctx: &mut JitContext, dst: u32, idx: u32) {
    if (dst as usize) >= ctx.nregs as usize || (idx as usize) >= ctx.nconstants as usize {
        return;
    }
    let expr = match unsafe { &*ctx.constants.add(idx as usize) } {
        Value::Pattern(e) => e.clone(),
        other => crate::eval::table::value_to_expr(other),
    };
    let result = {
        let env = unsafe { &*ctx.env };
        eval::eval(&expr, env)
    };
    match result {
        Ok(v) => unsafe {
            *ctx.regs.add(dst as usize) = v;
        },
        Err(e) => ctx.set_error(e),
    }
}

/// Like [`jit_eval_tree`], but capture `Break`/`Continue` into `ctx.ctrl`
/// instead of failing, so an enclosing native loop can act on them.
#[unsafe(no_mangle)]
pub extern "C" fn jit_eval_tree_loop(ctx: &mut JitContext, dst: u32, idx: u32) {
    if (dst as usize) >= ctx.nregs as usize || (idx as usize) >= ctx.nconstants as usize {
        return;
    }
    let expr = match unsafe { &*ctx.constants.add(idx as usize) } {
        Value::Pattern(e) => e.clone(),
        other => crate::eval::table::value_to_expr(other),
    };
    let result = {
        let env = unsafe { &*ctx.env };
        eval::eval(&expr, env)
    };
    match result {
        Ok(v) => unsafe {
            *ctx.regs.add(dst as usize) = v;
        },
        Err(EvalError::Break) => {
            ctx.ctrl = 1;
            unsafe {
                *ctx.regs.add(dst as usize) = Value::Null;
            }
        }
        Err(EvalError::Continue) => {
            ctx.ctrl = 2;
            unsafe {
                *ctx.regs.add(dst as usize) = Value::Null;
            }
        }
        Err(e) => ctx.set_error(e),
    }
}

/// Return the captured `Break` control and clear it.
#[unsafe(no_mangle)]
pub extern "C" fn jit_take_break(ctx: &mut JitContext) -> u8 {
    let hit = ctx.ctrl == 1;
    if hit {
        ctx.ctrl = 0;
    }
    hit as u8
}

/// Return the captured `Continue` control and clear it.
#[unsafe(no_mangle)]
pub extern "C" fn jit_take_continue(ctx: &mut JitContext) -> u8 {
    let hit = ctx.ctrl == 2;
    if hit {
        ctx.ctrl = 0;
    }
    hit as u8
}

/// Negate a value exactly like `VmState::negate`: numeric literals directly,
/// otherwise `Times[-1, val]`.
fn negate_value(val: &Value, env: &Env) -> Result<Value, EvalError> {
    match val {
        Value::Integer(n) => Ok(Value::Integer(n.clone() * -1)),
        Value::Real(r) => {
            let mut neg = r.clone();
            neg *= -1.0;
            Ok(Value::Real(neg))
        }
        Value::Complex { re, im } => Ok(Value::Complex { re: -re, im: -im }),
        _ => {
            let minus_one = Value::Integer((-1).into());
            let func = env
                .get("Times")
                .unwrap_or_else(|| Value::Symbol("Times".to_string()));
            eval::apply_function(&func, &[minus_one, val.clone()], env)
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_neg(ctx: &mut JitContext, dst: u32, src: u32) {
    if (dst as usize) >= ctx.nregs as usize || (src as usize) >= ctx.nregs as usize {
        return;
    }
    let val = unsafe { (*ctx.regs.add(src as usize)).clone() };
    let result = {
        let env = unsafe { &*ctx.env };
        negate_value(&val, env)
    };
    match result {
        Ok(v) => unsafe {
            *ctx.regs.add(dst as usize) = v;
        },
        Err(e) => ctx.set_error(e),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_not(ctx: &mut JitContext, dst: u32, src: u32) {
    if (dst as usize) < ctx.nregs as usize && (src as usize) < ctx.nregs as usize {
        let val = unsafe { (*ctx.regs.add(src as usize)).clone() };
        unsafe {
            *ctx.regs.add(dst as usize) = crate::builtins::logical::not_value(val);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_and(ctx: &mut JitContext, dst: u32, a: u32, b: u32) {
    if (dst as usize) < ctx.nregs as usize
        && (a as usize) < ctx.nregs as usize
        && (b as usize) < ctx.nregs as usize
    {
        // Match `builtin_and`.
        let va = unsafe { (*ctx.regs.add(a as usize)).clone() };
        let vb = unsafe { (*ctx.regs.add(b as usize)).clone() };
        let out = crate::builtins::logical::and_values(va, vb);
        unsafe {
            *ctx.regs.add(dst as usize) = out;
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_or(ctx: &mut JitContext, dst: u32, a: u32, b: u32) {
    if (dst as usize) < ctx.nregs as usize
        && (a as usize) < ctx.nregs as usize
        && (b as usize) < ctx.nregs as usize
    {
        // Match `builtin_or`.
        let va = unsafe { (*ctx.regs.add(a as usize)).clone() };
        let vb = unsafe { (*ctx.regs.add(b as usize)).clone() };
        let out = crate::builtins::logical::or_values(va, vb);
        unsafe {
            *ctx.regs.add(dst as usize) = out;
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
    // Sub(a, b) = Plus[a, negate(b)], matching `Instruction::Sub` in the VM.
    let result = {
        let env = unsafe { &*ctx.env };
        negate_value(&vb, env).and_then(|neg_b| {
            let plus = env
                .get("Plus")
                .unwrap_or_else(|| Value::Symbol("Plus".to_string()));
            eval::apply_function(&plus, &[va, neg_b], env)
        })
    };
    match result {
        Ok(v) => unsafe {
            *ctx.regs.add(dst as usize) = v;
        },
        Err(e) => ctx.set_error(e),
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
    // Match the VM: delegate to the `Divide` builtin so degenerate cases
    // (e.g. `1/0` → `ComplexInfinity`) agree with the tree-walker.
    let result = {
        let env = unsafe { &*ctx.env };
        let divide = env
            .get("Divide")
            .unwrap_or_else(|| Value::Symbol("Divide".to_string()));
        eval::apply_function(&divide, &[va, vb], env)
    };
    match result {
        Ok(v) => unsafe {
            *ctx.regs.add(dst as usize) = v;
        },
        Err(e) => ctx.set_error(e),
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
    let result = {
        let env = unsafe { &*ctx.env };
        let func = env
            .get(name)
            .unwrap_or_else(|| Value::Symbol(name.to_string()));
        eval::apply_function(&func, &[va, vb], env)
    };
    match result {
        Ok(v) => unsafe {
            *ctx.regs.add(dst as usize) = v;
        },
        Err(e) => ctx.set_error(e),
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
        map.insert(key_str(&key), val);
    }
    unsafe {
        *ctx.regs.add(dst as usize) = Value::Assoc(map);
    }
}

/// Format an association key exactly like `VmState::val_str`.
fn key_str(val: &Value) -> String {
    match val {
        Value::Str(s) => s.clone(),
        Value::Symbol(s) => s.clone(),
        Value::Integer(n) => n.to_string(),
        Value::Real(r) => r.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "Null".to_string(),
        other => format!("{other}"),
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
    // The bytecode compiler stores symbol names as `Value::Symbol` constants
    // (and `Value::Str` for string constants); accept both, matching the VM's
    // `const_str` behaviour.
    let name = match name_const {
        Value::Str(name) | Value::Symbol(name) => name.clone(),
        _ => return,
    };
    let env = unsafe { &*ctx.env };
    // Mirror `LoadSym` in the VM: unresolved names evaluate to themselves.
    let val = env.get(&name).unwrap_or(Value::Symbol(name));
    unsafe {
        *ctx.regs.add(dst as usize) = val;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn jit_store_sym(ctx: &mut JitContext, idx: u32, src: u32) {
    if (idx as usize) >= ctx.nconstants as usize || (src as usize) >= ctx.nregs as usize {
        return;
    }
    let name_const = unsafe { &*ctx.constants.add(idx as usize) };
    let name = match name_const {
        Value::Str(name) | Value::Symbol(name) => name.clone(),
        _ => return,
    };
    let val = unsafe { (*ctx.regs.add(src as usize)).clone() };
    let env = unsafe { &*ctx.env };
    // Mirror `StoreSym` in the VM, which propagates to the binding's
    // defining scope (`set_propagate`) rather than shadowing in the frame.
    env.set_propagate(name, val);
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
    let result = {
        let env = unsafe { &*ctx.env };
        eval::apply_function(&func, &args, env)
    };
    match result {
        Ok(result) => unsafe { *ctx.regs.add(dst as usize) = result },
        Err(e) => ctx.set_error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode::CompiledBytecode;
    use crate::env::Env;

    fn bc_with(constants: Vec<Value>, nregs: u16) -> CompiledBytecode {
        CompiledBytecode {
            instructions: vec![],
            constants,
            nregs,
            nparams: 0,
            param_names: vec![],
        }
    }

    #[test]
    fn test_jit_load_sym_reads_symbol_constant_and_falls_back() {
        // Regression: the compiler stores symbol names as `Value::Symbol`, but
        // `jit_load_sym` only matched `Value::Str` and left `dst` untouched.
        let env = Env::new();
        env.set("x".to_string(), Value::Integer(42.into()));
        let bc = bc_with(
            vec![Value::Symbol("x".into()), Value::Symbol("y".into())],
            2,
        );
        let mut regs = vec![Value::Null; 2];
        let mut ctx = JitContext::new(&bc, &[], &env, &mut regs);
        jit_load_sym(&mut ctx, 0, 0);
        jit_load_sym(&mut ctx, 1, 1);
        assert_eq!(regs[0], Value::Integer(42.into()));
        assert_eq!(regs[1], Value::Symbol("y".into()));
    }

    #[test]
    fn test_jit_store_sym_writes_symbol_constant() {
        let env = Env::new();
        let bc = bc_with(vec![Value::Symbol("bound".into())], 1);
        let mut regs = vec![Value::Integer(7.into())];
        let mut ctx = JitContext::new(&bc, &[], &env, &mut regs);
        jit_store_sym(&mut ctx, 0, 0);
        assert_eq!(env.get("bound"), Some(Value::Integer(7.into())));
    }
}
