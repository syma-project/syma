use crate::bytecode::CompiledBytecode;
/// Register-based bytecode VM.
///
/// Executes `CompiledBytecode` by dispatching on each `Instruction`.
/// All function calls (`Apply`) delegate to the tree-walk evaluator's
/// `apply_function`, which returns the call back here for compiled callees.
///
use crate::bytecode::instruction::*;
use crate::env::Env;
use crate::eval;
use crate::value::*;

/// Execute a compiled bytecode function with the given arguments and environment.
pub fn execute_bytecode(
    bytecode: &CompiledBytecode,
    args: &[Value],
    env: &Env,
) -> Result<Value, EvalError> {
    // Validate register bounds before execution
    validate_bytecode_regs(bytecode)?;

    let nregs = bytecode.nregs.max(args.len() as u16).max(1) as usize;
    let mut regs = vec![Value::Null; nregs];
    for (i, arg) in args.iter().enumerate() {
        if i < nregs {
            regs[i] = arg.clone();
        }
    }

    // Run the body in a fresh child scope, binding parameters by name. This
    // mirrors the tree-walk evaluator's per-call scope: local assignments do not
    // leak to the caller, and `Function`/`PureFunction` values created in the
    // body capture the parameters through their call-site environment.
    let local = env.child();
    for (i, name) in bytecode.param_names.iter().enumerate() {
        if !name.is_empty() {
            local.set(name.clone(), args.get(i).cloned().unwrap_or(Value::Null));
        }
    }

    let mut vm = VmState {
        regs,
        pc: 0,
        code: &bytecode.instructions,
        constants: &bytecode.constants,
        args,
        env: &local,
        instr_count: 0,
        ctrl: 0,
    };
    // `Return[expr]` raises `EvalError::Return`; at a function boundary it must
    // yield `expr` rather than escape to the caller (matching the tree-walk
    // `Function`/`PureFunction` handling).
    vm.execute_loop().or_else(|e| match e {
        EvalError::Return(v) => Ok(*v),
        other => Err(other),
    })
}

const INSTRUCTION_LIMIT: u64 = 10_000_000;

struct VmState<'a> {
    regs: Vec<Value>,
    pc: usize,
    code: &'a [Instruction],
    constants: &'a [Value],
    args: &'a [Value],
    env: &'a Env,
    instr_count: u64,
    /// Pending loop control captured by `EvalTreeLoop` (0 none, 1 break,
    /// 2 continue). Consumed by `JumpIfBreak`/`JumpIfContinue`.
    ctrl: u8,
}

const CTRL_BREAK: u8 = 1;
const CTRL_CONTINUE: u8 = 2;

impl VmState<'_> {
    fn execute_loop(&mut self) -> Result<Value, EvalError> {
        loop {
            if self.instr_count >= INSTRUCTION_LIMIT {
                return Err(EvalError::Error(
                    "bytecode: instruction limit (10M) exceeded".to_string(),
                ));
            }
            self.instr_count += 1;

            if self.pc >= self.code.len() {
                return Err(EvalError::Error(
                    "bytecode: program counter past end of code".to_string(),
                ));
            }

            let instr = &self.code[self.pc];
            self.pc += 1;

            match instr {
                Instruction::Halt => return Ok(Value::Null),
                Instruction::Break => return Err(EvalError::Break),
                Instruction::Continue => return Err(EvalError::Continue),

                Instruction::LoadNull(d) => self.regs[*d as usize] = Value::Null,
                Instruction::LoadTrue(d) => self.regs[*d as usize] = Value::Bool(true),
                Instruction::LoadFalse(d) => self.regs[*d as usize] = Value::Bool(false),

                Instruction::Mov(d, s) => {
                    self.regs[*d as usize] = self.regs[*s as usize].clone();
                }
                Instruction::Neg(d, s) => {
                    self.regs[*d as usize] = self.negate(&self.regs[*s as usize])?;
                }
                Instruction::Not(d, s) => {
                    let v = self.regs[*s as usize].clone();
                    self.regs[*d as usize] = crate::builtins::logical::not_value(v);
                }

                // General arithmetic via builtins
                Instruction::Add(d, a, b) => {
                    self.regs[*d as usize] =
                        self.builtin2("Plus", &self.regs[*a as usize], &self.regs[*b as usize])?;
                }
                Instruction::Sub(d, a, b) => {
                    let neg_b = self.negate(&self.regs[*b as usize])?;
                    self.regs[*d as usize] =
                        self.builtin2("Plus", &self.regs[*a as usize], &neg_b)?;
                }
                Instruction::Mul(d, a, b) => {
                    self.regs[*d as usize] =
                        self.builtin2("Times", &self.regs[*a as usize], &self.regs[*b as usize])?;
                }
                Instruction::Div(d, a, b) => {
                    // Use the `Divide` builtin so symbolic/degenerate cases
                    // (e.g. `1/0` → `ComplexInfinity`) match the tree-walker
                    // rather than being recomputed as `Times[a, Power[b, -1]]`.
                    self.regs[*d as usize] =
                        self.builtin2("Divide", &self.regs[*a as usize], &self.regs[*b as usize])?;
                }
                Instruction::Pow(d, a, b) => {
                    self.regs[*d as usize] =
                        self.builtin2("Power", &self.regs[*a as usize], &self.regs[*b as usize])?;
                }

                // Fast integer paths
                Instruction::IntAdd(d, a, b) => {
                    self.int_binop(*d, *a, *b, |x, y| Value::Integer(x + y), "Plus")?;
                }
                Instruction::IntSub(d, a, b) => {
                    let av = &self.regs[*a as usize];
                    let bv = &self.regs[*b as usize];
                    match (av, bv) {
                        (Value::Integer(ai), Value::Integer(bi)) => {
                            self.regs[*d as usize] = Value::Integer(ai.clone() - bi);
                        }
                        _ => {
                            let neg_b = self.negate(bv)?;
                            self.regs[*d as usize] = self.builtin2("Plus", av, &neg_b)?;
                        }
                    }
                }
                Instruction::IntMul(d, a, b) => {
                    self.int_binop(*d, *a, *b, |x, y| Value::Integer(x * y), "Times")?;
                }

                // Fast real paths
                Instruction::RealAdd(d, a, b) => {
                    let av = &self.regs[*a as usize];
                    let bv = &self.regs[*b as usize];
                    match (av, bv) {
                        (Value::Real(af), Value::Real(bf)) => {
                            let mut r = af.clone();
                            r += bf.to_f64();
                            self.regs[*d as usize] = Value::Real(r);
                        }
                        _ => self.regs[*d as usize] = self.builtin2("Plus", av, bv)?,
                    }
                }
                Instruction::RealSub(d, a, b) => {
                    let av = &self.regs[*a as usize];
                    let bv = &self.regs[*b as usize];
                    match (av, bv) {
                        (Value::Real(af), Value::Real(bf)) => {
                            let mut r = af.clone();
                            r -= bf.to_f64();
                            self.regs[*d as usize] = Value::Real(r);
                        }
                        _ => {
                            let neg_b = self.negate(bv)?;
                            self.regs[*d as usize] = self.builtin2("Plus", av, &neg_b)?;
                        }
                    }
                }
                Instruction::RealMul(d, a, b) => {
                    self.real_binop(
                        *d,
                        *a,
                        *b,
                        |x, y| {
                            let mut r = x;
                            r *= y.to_f64();
                            Value::Real(r)
                        },
                        "Times",
                    )?;
                }
                Instruction::RealDiv(d, a, b) => {
                    // Delegate to `Divide` for the same reason as `Div`:
                    // `1.0/0.0` must yield `ComplexInfinity`.
                    self.regs[*d as usize] =
                        self.builtin2("Divide", &self.regs[*a as usize], &self.regs[*b as usize])?;
                }

                // Comparison
                Instruction::Eq(d, a, b) => {
                    self.regs[*d as usize] =
                        self.builtin2("Equal", &self.regs[*a as usize], &self.regs[*b as usize])?;
                }
                Instruction::Neq(d, a, b) => {
                    self.regs[*d as usize] =
                        self.builtin2("Unequal", &self.regs[*a as usize], &self.regs[*b as usize])?;
                }
                Instruction::Lt(d, a, b) => {
                    self.regs[*d as usize] =
                        self.builtin2("Less", &self.regs[*a as usize], &self.regs[*b as usize])?;
                }
                Instruction::Gt(d, a, b) => {
                    self.regs[*d as usize] =
                        self.builtin2("Greater", &self.regs[*a as usize], &self.regs[*b as usize])?;
                }
                Instruction::Le(d, a, b) => {
                    self.regs[*d as usize] = self.builtin2(
                        "LessEqual",
                        &self.regs[*a as usize],
                        &self.regs[*b as usize],
                    )?;
                }
                Instruction::Ge(d, a, b) => {
                    self.regs[*d as usize] = self.builtin2(
                        "GreaterEqual",
                        &self.regs[*a as usize],
                        &self.regs[*b as usize],
                    )?;
                }
                Instruction::And(d, a, b) => {
                    // Match `builtin_and`: short-circuit on False, drop True,
                    // and form a symbolic call for two unknown operands.
                    let av = self.regs[*a as usize].clone();
                    let bv = self.regs[*b as usize].clone();
                    self.regs[*d as usize] = crate::builtins::logical::and_values(av, bv);
                }
                Instruction::Or(d, a, b) => {
                    // Match `builtin_or`: short-circuit on True, drop False,
                    // and form a symbolic call for two unknown operands.
                    let av = self.regs[*a as usize].clone();
                    let bv = self.regs[*b as usize].clone();
                    self.regs[*d as usize] = crate::builtins::logical::or_values(av, bv);
                }

                // List/Assoc
                Instruction::MakeList(d, n) => {
                    let n = *n as usize;
                    let mut items = Vec::with_capacity(n);
                    for i in 0..n {
                        items.push(self.regs[*d as usize + 1 + i].clone());
                    }
                    self.regs[*d as usize] = Value::List(items);
                }
                Instruction::MakeAssoc(d, n) => {
                    let n = *n as usize;
                    let mut map = std::collections::HashMap::new();
                    for i in 0..n {
                        let key = self.val_str(&self.regs[*d as usize + 1 + i * 2]);
                        let val = self.regs[*d as usize + 1 + i * 2 + 1].clone();
                        map.insert(key, val);
                    }
                    self.regs[*d as usize] = Value::Assoc(map);
                }

                // Function calls — always delegate to tree-walk evaluator
                Instruction::Apply(d, nargs)
                | Instruction::Call(d, nargs)
                | Instruction::TailCall(d, nargs) => {
                    let func = self.regs[*d as usize].clone();
                    let n = *nargs as usize;
                    let mut args = Vec::with_capacity(n);
                    for i in 0..n {
                        args.push(self.regs[*d as usize + 1 + i].clone());
                    }
                    let result = eval::apply_function(&func, &args, self.env)?;
                    if matches!(instr, Instruction::TailCall(..)) {
                        return Ok(result);
                    }
                    self.regs[*d as usize] = result;
                }

                // Load/Store
                Instruction::LoadConst(d, idx) => {
                    self.regs[*d as usize] = self
                        .constants
                        .get(*idx as usize)
                        .cloned()
                        .unwrap_or(Value::Null);
                }
                Instruction::LoadArg(d, idx) => {
                    self.regs[*d as usize] =
                        self.args.get(*idx as usize).cloned().unwrap_or(Value::Null);
                }
                Instruction::LoadSym(d, idx) => {
                    let name = self.const_str(*idx);
                    let val = self.env.get(&name).unwrap_or_else(|| Value::Symbol(name));
                    self.regs[*d as usize] = val;
                }
                Instruction::StoreSym(idx, s) => {
                    // `Set` propagates to the scope where the symbol is already
                    // bound (matching `eval_assign`'s `set_propagate`), so
                    // assigning a global from inside a function updates the
                    // global rather than shadowing it.
                    let name = self.const_str(*idx);
                    self.env.set_propagate(name, self.regs[*s as usize].clone());
                }
                Instruction::EvalTree(d, idx) => {
                    let expr = match self.constants.get(*idx as usize) {
                        Some(Value::Pattern(e)) => e.clone(),
                        Some(other) => crate::eval::table::value_to_expr(other),
                        None => crate::ast::Expr::Null,
                    };
                    self.regs[*d as usize] = eval::eval(&expr, self.env)?;
                }
                Instruction::EvalTreeLoop(d, idx) => {
                    let expr = match self.constants.get(*idx as usize) {
                        Some(Value::Pattern(e)) => e.clone(),
                        Some(other) => crate::eval::table::value_to_expr(other),
                        None => crate::ast::Expr::Null,
                    };
                    match eval::eval(&expr, self.env) {
                        Ok(v) => self.regs[*d as usize] = v,
                        Err(EvalError::Break) => {
                            self.ctrl = CTRL_BREAK;
                            self.regs[*d as usize] = Value::Null;
                        }
                        Err(EvalError::Continue) => {
                            self.ctrl = CTRL_CONTINUE;
                            self.regs[*d as usize] = Value::Null;
                        }
                        Err(e) => return Err(e),
                    }
                }

                // Control flow
                Instruction::Jump(offset) => {
                    self.pc = signed_offset(self.pc, *offset);
                }
                Instruction::JumpIfZero(r, offset) => {
                    if !self.regs[*r as usize].is_truthy() {
                        self.pc = signed_offset(self.pc, *offset);
                    }
                }
                Instruction::JumpIfNotZero(r, offset) => {
                    if self.regs[*r as usize].is_truthy() {
                        self.pc = signed_offset(self.pc, *offset);
                    }
                }
                Instruction::JumpIfBreak(offset) => {
                    if self.ctrl & CTRL_BREAK != 0 {
                        self.ctrl = 0;
                        self.pc = signed_offset(self.pc, *offset);
                    }
                }
                Instruction::JumpIfContinue(offset) => {
                    if self.ctrl & CTRL_CONTINUE != 0 {
                        self.ctrl = 0;
                        self.pc = signed_offset(self.pc, *offset);
                    }
                }

                Instruction::MakeSeq(d, start) => {
                    let start = *start as usize;
                    let count = self.args.len().saturating_sub(start);
                    let mut items = Vec::with_capacity(count);
                    for i in 0..count {
                        items.push(self.args[start + i].clone());
                    }
                    self.regs[*d as usize] = Value::List(items);
                }

                Instruction::Return(s) => return Ok(self.regs[*s as usize].clone()),
            }
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────

    fn int_binop(
        &mut self,
        d: u16,
        a: u16,
        b: u16,
        int_op: fn(rug::Integer, &rug::Integer) -> Value,
        fallback: &str,
    ) -> Result<(), EvalError> {
        match (&self.regs[a as usize], &self.regs[b as usize]) {
            (Value::Integer(ai), Value::Integer(bi)) => {
                self.regs[d as usize] = int_op(ai.clone(), bi);
            }
            _ => {
                self.regs[d as usize] =
                    self.builtin2(fallback, &self.regs[a as usize], &self.regs[b as usize])?;
            }
        }
        Ok(())
    }

    fn real_binop(
        &mut self,
        d: u16,
        a: u16,
        b: u16,
        real_op: fn(rug::Float, rug::Float) -> Value,
        fallback: &str,
    ) -> Result<(), EvalError> {
        match (&self.regs[a as usize], &self.regs[b as usize]) {
            (Value::Real(af), Value::Real(bf)) => {
                self.regs[d as usize] = real_op(af.clone(), bf.clone());
            }
            _ => {
                self.regs[d as usize] =
                    self.builtin2(fallback, &self.regs[a as usize], &self.regs[b as usize])?;
            }
        }
        Ok(())
    }

    fn negate(&self, val: &Value) -> Result<Value, EvalError> {
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
                let func = self
                    .env
                    .get("Times")
                    .unwrap_or_else(|| Value::Symbol("Times".to_string()));
                eval::apply_function(&func, &[minus_one, val.clone()], self.env)
            }
        }
    }

    fn builtin2(&self, name: &str, a: &Value, b: &Value) -> Result<Value, EvalError> {
        let func = self
            .env
            .get(name)
            .unwrap_or_else(|| Value::Symbol(name.to_string()));
        eval::apply_function(&func, &[a.clone(), b.clone()], self.env)
    }

    fn const_str(&self, idx: u32) -> String {
        match self.constants.get(idx as usize) {
            Some(Value::Symbol(s) | Value::Str(s)) => s.clone(),
            Some(other) => format!("{other}"),
            None => String::new(),
        }
    }

    fn val_str(&self, val: &Value) -> String {
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
}

fn signed_offset(pc: usize, offset: i32) -> usize {
    if offset >= 0 {
        pc.wrapping_add(offset as usize)
    } else {
        pc.wrapping_sub((-offset) as usize)
    }
}

// ── is_truthy on Value ─────────────────────────────────────────────────────────

pub(crate) trait Truthy {
    fn is_truthy(&self) -> bool;
}

impl Truthy for Value {
    /// Delegate to [`Value::to_bool`] so bytecode conditions agree with the
    /// tree-walk evaluator (notably, an unevaluated `Call` such as a symbolic
    /// relation is *falsey*, like it is for `If`/`And`/`Or` in `eval`).
    fn is_truthy(&self) -> bool {
        self.to_bool()
    }
}

/// Validate that all register references in the bytecode are within bounds.
fn validate_bytecode_regs(bc: &CompiledBytecode) -> Result<(), EvalError> {
    let nregs = bc.nregs.max(1) as usize;
    for instr in &bc.instructions {
        let max_reg = max_reg_index(instr);
        if max_reg >= nregs {
            return Err(EvalError::Error(format!(
                "bytecode: register {max_reg} out of bounds (nregs={nregs})"
            )));
        }
    }
    Ok(())
}

/// Return the maximum register index referenced by an instruction.
fn max_reg_index(instr: &Instruction) -> usize {
    match instr {
        Instruction::Halt
        | Instruction::Break
        | Instruction::Continue
        | Instruction::Jump(_)
        | Instruction::JumpIfBreak(_)
        | Instruction::JumpIfContinue(_) => 0,

        // 1 register
        Instruction::LoadNull(d) | Instruction::LoadTrue(d) | Instruction::LoadFalse(d) => {
            *d as usize
        }

        // 2 registers — max of both
        Instruction::Mov(d, s) | Instruction::Neg(d, s) | Instruction::Not(d, s) => {
            (*d).max(*s) as usize
        }

        // 3 registers — max of all three
        Instruction::Add(d, a, b)
        | Instruction::Sub(d, a, b)
        | Instruction::Mul(d, a, b)
        | Instruction::Div(d, a, b)
        | Instruction::Pow(d, a, b)
        | Instruction::IntAdd(d, a, b)
        | Instruction::IntSub(d, a, b)
        | Instruction::IntMul(d, a, b)
        | Instruction::RealAdd(d, a, b)
        | Instruction::RealSub(d, a, b)
        | Instruction::RealMul(d, a, b)
        | Instruction::RealDiv(d, a, b)
        | Instruction::Eq(d, a, b)
        | Instruction::Neq(d, a, b)
        | Instruction::Lt(d, a, b)
        | Instruction::Gt(d, a, b)
        | Instruction::Le(d, a, b)
        | Instruction::Ge(d, a, b)
        | Instruction::And(d, a, b)
        | Instruction::Or(d, a, b) => (*d).max(*a).max(*b) as usize,

        // reg + u8 — includes contiguous registers
        Instruction::MakeList(d, n)
        | Instruction::MakeAssoc(d, n)
        | Instruction::Apply(d, n)
        | Instruction::Call(d, n)
        | Instruction::TailCall(d, n) => {
            // d + (up to 2*n for MakeAssoc, n for others) contiguous regs
            let count = match instr {
                Instruction::MakeAssoc(_, n) => 1 + 2 * n,
                _ => 1 + n,
            };
            (*d as usize).saturating_add(count as usize - 1) // -1 because d itself is 1
        }

        // reg + u32
        Instruction::LoadConst(d, _)
        | Instruction::LoadArg(d, _)
        | Instruction::LoadSym(d, _)
        | Instruction::EvalTree(d, _)
        | Instruction::EvalTreeLoop(d, _) => *d as usize,

        // MakeSeq: dest register only
        Instruction::MakeSeq(d, _) => *d as usize,

        // StoreSym(idx, s) — s is source register
        Instruction::StoreSym(_, s) => *s as usize,

        // reg + i32
        Instruction::JumpIfZero(r, _) | Instruction::JumpIfNotZero(r, _) => *r as usize,

        // Return
        Instruction::Return(r) => *r as usize,
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Expr;
    use crate::builtins;
    use crate::bytecode::CompiledBytecode;
    use crate::bytecode::compiler::BytecodeCompiler;
    use crate::env::Env;

    fn make_env() -> Env {
        let env = Env::new();
        builtins::register_builtins(&env);
        env
    }

    fn run(bc: &CompiledBytecode, args: &[Value]) -> Result<Value, EvalError> {
        execute_bytecode(bc, args, &make_env())
    }

    #[test]
    fn test_null() {
        let bc = CompiledBytecode {
            instructions: vec![Instruction::LoadNull(0), Instruction::Return(0)],
            constants: vec![],
            nregs: 1,
            nparams: 0,
            param_names: vec![],
        };
        assert_eq!(run(&bc, &[]).unwrap(), Value::Null);
    }

    #[test]
    fn test_constant() {
        let bc = CompiledBytecode {
            instructions: vec![Instruction::LoadConst(0, 0), Instruction::Return(0)],
            constants: vec![Value::Integer(42.into())],
            nregs: 1,
            nparams: 0,
            param_names: vec![],
        };
        assert_eq!(run(&bc, &[]).unwrap(), Value::Integer(42.into()));
    }

    #[test]
    fn test_int_add() {
        let bc = CompiledBytecode {
            instructions: vec![
                Instruction::LoadConst(0, 0),
                Instruction::LoadConst(1, 1),
                Instruction::IntAdd(2, 0, 1),
                Instruction::Return(2),
            ],
            constants: vec![Value::Integer(10.into()), Value::Integer(20.into())],
            nregs: 3,
            nparams: 0,
            param_names: vec![],
        };
        assert_eq!(run(&bc, &[]).unwrap(), Value::Integer(30.into()));
    }

    #[test]
    fn test_jump() {
        let bc = CompiledBytecode {
            instructions: vec![
                Instruction::LoadConst(0, 0), // reg0 = 0
                Instruction::Jump(1),         // skip Return(0), land on LoadConst
                Instruction::Return(0),       // skipped
                Instruction::LoadConst(0, 1), // reg0 = 42
                Instruction::Return(0),
            ],
            constants: vec![Value::Integer(0.into()), Value::Integer(42.into())],
            nregs: 1,
            nparams: 0,
            param_names: vec![],
        };
        assert_eq!(run(&bc, &[]).unwrap(), Value::Integer(42.into()));
    }

    #[test]
    fn test_bounds_validation_out_of_bounds() {
        // Register 5 is out of bounds (nregs=3)
        let bc = CompiledBytecode {
            instructions: vec![Instruction::LoadConst(5, 0), Instruction::Return(5)],
            constants: vec![Value::Integer(42.into())],
            nregs: 3,
            nparams: 0,
            param_names: vec![],
        };
        let err = run(&bc, &[]).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("out of bounds"),
            "expected bounds error, got: {msg}"
        );
    }

    #[test]
    fn test_bounds_validation_make_list() {
        // MakeList at reg 0 with n=5 touches regs 0..5, but nregs=3
        let bc = CompiledBytecode {
            instructions: vec![Instruction::MakeList(0, 5), Instruction::Return(0)],
            constants: vec![],
            nregs: 3,
            nparams: 0,
            param_names: vec![],
        };
        let err = run(&bc, &[]).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("out of bounds"),
            "expected bounds error, got: {msg}"
        );
    }

    #[test]
    fn test_bounds_validation_jump_if_zero() {
        let bc = CompiledBytecode {
            instructions: vec![Instruction::JumpIfZero(99, 1), Instruction::Return(99)],
            constants: vec![],
            nregs: 3,
            nparams: 0,
            param_names: vec![],
        };
        let err = run(&bc, &[]).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("out of bounds"),
            "expected bounds error, got: {msg}"
        );
    }

    #[test]
    fn test_compile_list_literal_packs_operands() {
        // Regression: `MakeList(d, n)` reads operands from `d+1 ..= d+n`, so the
        // compiler must pack the items into a contiguous span at `d`.
        use crate::ast::Expr;
        let body = Expr::List(vec![
            Expr::Integer(1.into()),
            Expr::Integer(2.into()),
            Expr::Integer(3.into()),
        ]);
        let bc = BytecodeCompiler::compile_function(&[], &body, "f").unwrap();
        assert_eq!(
            run(&bc, &[]).unwrap(),
            Value::List(vec![
                Value::Integer(1.into()),
                Value::Integer(2.into()),
                Value::Integer(3.into())
            ])
        );
    }

    #[test]
    fn test_compile_assoc_literal_packs_operands() {
        // Regression: `MakeAssoc(d, n)` reads key/value pairs from `d+1 ..`.
        use crate::ast::Expr;
        let body = Expr::Assoc(vec![
            ("a".to_string(), Expr::Integer(1.into())),
            ("b".to_string(), Expr::Integer(2.into())),
        ]);
        let bc = BytecodeCompiler::compile_function(&[], &body, "f").unwrap();
        match run(&bc, &[]).unwrap() {
            Value::Assoc(map) => {
                assert_eq!(map.get("a"), Some(&Value::Integer(1.into())));
                assert_eq!(map.get("b"), Some(&Value::Integer(2.into())));
            }
            other => panic!("expected assoc, got {other:?}"),
        }
    }

    #[test]
    fn test_nested_call_packing() {
        // Regression: packing call arguments into `d+1 ..` must not clobber
        // source registers when `d` is reused from the free list.
        use crate::ast::Expr;
        let leaf = |s: &str| Expr::Str(s.to_string());
        let sj = |a: Expr, b: Expr| Expr::Call {
            head: Box::new(Expr::Symbol("StringJoin".into())),
            args: vec![a, b],
        };
        let mut body = sj(leaf("a"), leaf("b"));
        for _ in 0..6 {
            body = sj(body.clone(), body);
        }
        let bc = BytecodeCompiler::compile_function(&[], &body, "f").unwrap();
        assert_eq!(run(&bc, &[]).unwrap(), Value::Str("ab".repeat(64)));
    }

    fn check_bytecode_matches_treewalk(body: Expr) {
        let tree = crate::eval::eval(&body, &make_env()).unwrap();
        let bc = BytecodeCompiler::compile_function(&[], &body, "f").unwrap();
        let byte = execute_bytecode(&bc, &[], &make_env()).unwrap();
        assert_eq!(byte, tree, "bytecode diverged from tree-walk");
    }

    #[test]
    fn test_control_flow_bytecode_matches_treewalk() {
        use crate::ast::Expr;
        let int = |n: i64| Expr::Integer(n.into());
        let call = |name: &str, args: Vec<Expr>| Expr::Call {
            head: Box::new(Expr::Symbol(name.to_string())),
            args,
        };
        // If with Greater / Not conditions and both branches.
        check_bytecode_matches_treewalk(Expr::If {
            condition: Box::new(call("Greater", vec![int(3), int(2)])),
            then_branch: Box::new(int(10)),
            else_branch: Some(Box::new(int(20))),
        });
        check_bytecode_matches_treewalk(Expr::If {
            condition: Box::new(call("Not", vec![Expr::Bool(false)])),
            then_branch: Box::new(int(1)),
            else_branch: Some(Box::new(int(2))),
        });
        // Which picks the first true branch and otherwise falls through.
        check_bytecode_matches_treewalk(Expr::Which {
            pairs: vec![
                (Expr::Bool(false), int(1)),
                (Expr::Bool(true), int(2)),
                (Expr::Bool(true), int(3)),
            ],
        });
        check_bytecode_matches_treewalk(Expr::Which {
            pairs: vec![(Expr::Bool(false), int(1)), (Expr::Bool(false), int(2))],
        });
        // Switch matches a value and falls through to Null otherwise.
        check_bytecode_matches_treewalk(Expr::Switch {
            expr: Box::new(int(2)),
            cases: vec![(int(1), int(10)), (int(2), int(20))],
        });
        check_bytecode_matches_treewalk(Expr::Switch {
            expr: Box::new(int(9)),
            cases: vec![(int(1), int(10)), (int(2), int(20))],
        });
    }

    #[test]
    fn test_arithmetic_bytecode_matches_treewalk() {
        use crate::ast::Expr;
        let int = |n: i64| Expr::Integer(n.into());
        let call = |name: &str, args: Vec<Expr>| Expr::Call {
            head: Box::new(Expr::Symbol(name.to_string())),
            args,
        };
        check_bytecode_matches_treewalk(call("Plus", vec![int(2), int(3)]));
        check_bytecode_matches_treewalk(call("Times", vec![int(-1), int(7)]));
        check_bytecode_matches_treewalk(call("Divide", vec![int(12), int(4)]));
        check_bytecode_matches_treewalk(call("Power", vec![int(2), int(10)]));
        check_bytecode_matches_treewalk(call("And", vec![Expr::Bool(true), Expr::Bool(false)]));
        check_bytecode_matches_treewalk(call("Or", vec![Expr::Bool(false), Expr::Bool(true)]));

        // `Subtract` is an alias the compiler inlines; tree-walk leaves the
        // explicit head unevaluated, so assert the bytecode result directly.
        let body = call("Subtract", vec![int(10), int(4)]);
        let bc = BytecodeCompiler::compile_function(&[], &body, "f").unwrap();
        assert_eq!(run(&bc, &[]).unwrap(), Value::Integer(6.into()));
    }

    #[test]
    fn test_symbolic_condition_matches_treewalk() {
        // Regression: the VM used a truthiness rule that treated an unevaluated
        // Call (e.g. a symbolic relation) as true, diverging from the tree-walk
        // evaluator's `to_bool` semantics for `If`.
        let params = vec![Expr::NamedBlank {
            name: "x".to_string(),
            type_constraint: None,
        }];
        let body = Expr::If {
            condition: Box::new(Expr::Call {
                head: Box::new(Expr::Symbol("Greater".to_string())),
                args: vec![Expr::Symbol("x".to_string()), Expr::Integer(0.into())],
            }),
            then_branch: Box::new(Expr::Integer(1.into())),
            else_branch: Some(Box::new(Expr::Integer(2.into()))),
        };
        let arg = Value::Symbol("y".to_string());

        let tree_env = make_env();
        tree_env.set("x".to_string(), arg.clone());
        let tree = crate::eval::eval(&body, &tree_env).unwrap();

        let bc = BytecodeCompiler::compile_function(&params, &body, "f").unwrap();
        let byte = execute_bytecode(&bc, &[arg], &make_env()).unwrap();
        assert_eq!(byte, tree);
    }

    #[test]
    fn test_symbolic_and_or_matches_treewalk() {
        // `And`/`Or` yield an operand value (not a canonical Bool); the VM must
        // match the tree-walk builtins for symbolic/falsey operands.
        let params = vec![
            Expr::NamedBlank {
                name: "x".to_string(),
                type_constraint: None,
            },
            Expr::NamedBlank {
                name: "y".to_string(),
                type_constraint: None,
            },
        ];
        let combos = [
            (
                Value::Symbol("a".to_string()),
                Value::Symbol("b".to_string()),
            ),
            (Value::Bool(false), Value::Symbol("b".to_string())),
            (Value::Bool(true), Value::Symbol("b".to_string())),
            (Value::Bool(true), Value::Bool(false)),
        ];
        for head in ["And", "Or"] {
            let body = Expr::Call {
                head: Box::new(Expr::Symbol(head.to_string())),
                args: vec![Expr::Symbol("x".to_string()), Expr::Symbol("y".to_string())],
            };
            let bc = BytecodeCompiler::compile_function(&params, &body, "f").unwrap();
            for (x, y) in &combos {
                let tree_env = make_env();
                tree_env.set("x".to_string(), x.clone());
                tree_env.set("y".to_string(), y.clone());
                let tree = crate::eval::eval(&body, &tree_env).unwrap();
                let byte = execute_bytecode(&bc, &[x.clone(), y.clone()], &make_env()).unwrap();
                assert_eq!(byte, tree, "{head} mismatch for ({x:?}, {y:?})");
            }
        }
    }

    #[test]
    fn test_compiled_identity() {
        use crate::ast::Expr;
        let params = vec![Expr::NamedBlank {
            name: "x".to_string(),
            type_constraint: None,
        }];
        let body = Expr::Symbol("x".to_string());
        let bc = BytecodeCompiler::compile_function(&params, &body, "f").unwrap();
        assert_eq!(
            run(&bc, &[Value::Integer(99.into())]).unwrap(),
            Value::Integer(99.into())
        );
    }
}

/// Differential tests: compile a body with parameters and assert that the
/// bytecode VM produces exactly the same `Result` as the tree-walk evaluator.
#[cfg(test)]
mod diff_tests {
    use super::*;
    use crate::ast::Expr;
    use crate::builtins;
    use crate::bytecode::compiler::BytecodeCompiler;
    use crate::env::Env;
    use crate::lexer;
    use crate::parser;

    fn real(n: f64) -> Value {
        Value::Real(rug::Float::with_val(53, n))
    }

    fn parse_expr(src: &str) -> Expr {
        let tokens = lexer::tokenize(src).expect("lex");
        let mut ast = parser::parse(tokens).expect("parse");
        ast.pop().expect("non-empty")
    }

    fn make_env() -> Env {
        let env = Env::new();
        builtins::register_builtins(&env);
        env
    }

    fn named(name: &str) -> Expr {
        Expr::NamedBlank {
            name: name.to_string(),
            type_constraint: None,
        }
    }

    /// Compare tree-walk vs bytecode for a function `params -> body` applied to
    /// `args`. The tree-walk side goes through `apply_function` (wrapping the
    /// body in a `Function[...]`) so that non-local `Return` behaves the same on
    /// both sides.
    fn diff(body_src: &str, params: &[(&str, Value)]) {
        let body = parse_expr(body_src);
        let names: Vec<String> = params.iter().map(|(n, _)| n.to_string()).collect();
        let params_expr: Vec<Expr> = names.iter().map(|n| named(n)).collect();
        let args: Vec<Value> = params.iter().map(|(_, v)| v.clone()).collect();

        let tree_env = make_env();
        let func_expr = Expr::Function {
            params: names.clone(),
            body: Box::new(body.clone()),
        };
        let func_val = crate::eval::eval(&func_expr, &tree_env)
            .unwrap_or_else(|e| panic!("tree function build failed for {body_src}: {e:?}"));
        let tree = crate::eval::apply_function(&func_val, &args, &tree_env);

        let bc = BytecodeCompiler::compile_function(&params_expr, &body, "f")
            .unwrap_or_else(|e| panic!("compile failed for {body_src}: {e:?}"));
        let byte = execute_bytecode(&bc, &args, &make_env());

        match (&tree, &byte) {
            (Ok(a), Ok(b)) => assert_eq!(a, b, "divergence for `{body_src}` (params={params:?})"),
            (Err(_), Err(_)) => {}
            _ => panic!("ok/err divergence for `{body_src}`: tree={tree:?} byte={byte:?}"),
        }
    }

    #[test]
    fn diff_arithmetic() {
        let i = |n: i64| Value::Integer(n.into());
        diff("x + y", &[("x", i(2)), ("y", i(3))]);
        diff("x - y", &[("x", i(2)), ("y", i(3))]);
        diff("x * y", &[("x", i(2)), ("y", i(3))]);
        diff("x / y", &[("x", i(7)), ("y", i(2))]);
        diff("x^y", &[("x", i(2)), ("y", i(10))]);
        diff("-x", &[("x", i(5))]);
        diff("x + y", &[("x", real(2.5)), ("y", real(1.5))]);
        diff("x / y", &[("x", real(3.0)), ("y", real(2.0))]);
    }

    #[test]
    fn diff_comparisons() {
        let i = |n: i64| Value::Integer(n.into());
        for (src, a, b) in [
            ("x == y", 1, 1),
            ("x == y", 1, 2),
            ("x != y", 1, 2),
            ("x < y", 1, 2),
            ("x > y", 1, 2),
            ("x <= y", 2, 2),
            ("x >= y", 2, 3),
        ] {
            diff(src, &[("x", i(a)), ("y", i(b))]);
        }
    }

    #[test]
    fn diff_control_flow() {
        let i = |n: i64| Value::Integer(n.into());
        diff("If[x > 0, 1, 2]", &[("x", i(3))]);
        diff("If[x > 0, 1, 2]", &[("x", i(-3))]);
        diff("If[Not[x > 0], 1, 2]", &[("x", i(-3))]);
        diff("If[x > 0, 1]", &[("x", i(-3))]);
        diff("Which[x > 0, 1, x < 0, 2]", &[("x", i(-3))]);
        diff("Which[x > 0, 1, x < 0, 2]", &[("x", i(0))]);
    }

    #[test]
    fn diff_lists() {
        let i = |n: i64| Value::Integer(n.into());
        diff("{x, x + 1, x + 2}", &[("x", i(1))]);
        diff("Length[{x, x + 1}]", &[("x", i(1))]);
        diff("{{x, x}, {x + 1}}", &[("x", i(1))]);
    }

    #[test]
    fn diff_assignment_returns_value() {
        // Set returns the assigned value in the tree-walk evaluator.
        diff("x = 5", &[]);
        diff("x = 5; x", &[]);
        diff("{x = 5, x}", &[]);
    }

    #[test]
    fn diff_switch_patterns() {
        let i = |n: i64| Value::Integer(n.into());
        diff("Switch[x, 1, 10, 2, 20]", &[("x", i(2))]);
        diff("Switch[x, _Integer, 10, _, 20]", &[("x", i(2))]);
        diff(
            "Switch[x, _Integer, 10, _, 20]",
            &[("x", Value::Str("s".into()))],
        );
    }

    #[test]
    fn diff_closure_capture() {
        let i = |n: i64| Value::Integer(n.into());
        diff("(Function[{y}, x + y])[10]", &[("x", i(5))]);
        diff("(Function[{y}, x * y])[10]", &[("x", i(5))]);
    }

    #[test]
    fn diff_param_assignment() {
        let i = |n: i64| Value::Integer(n.into());
        // Assigning to a parameter must affect later reads.
        diff("x = x + 1; x", &[("x", i(5))]);
        diff("x = x * 2; x + 1", &[("x", i(3))]);
        diff("If[x > 0, x = 0, x = 10]; x", &[("x", i(7))]);
    }

    #[test]
    fn diff_loops() {
        let i = |n: i64| Value::Integer(n.into());
        diff("While[x < 5, x = x + 1]; x", &[("x", i(0))]);
        diff("For[i = 0, i < 3, i = i + 1, Null]; i", &[("i", i(0))]);
        diff(
            "Do[(If[x == 3, Break[]]; x = x + 1), {x, 0, 10}]; x",
            &[("x", i(0))],
        );
        diff(
            "x = 0; Do[(If[Mod[k, 2] == 0, Continue[]]; x = x + k), {k, 1, 6}]; x",
            &[],
        );
    }

    #[test]
    fn diff_switch_bindings() {
        let i = |n: i64| Value::Integer(n.into());
        diff("Switch[x, y_Integer, y + 1, _, 0]", &[("x", i(4))]);
        diff(
            "Switch[x, y_Integer, y + 1, _, 0]",
            &[("x", Value::Str("s".into()))],
        );
    }

    #[test]
    fn diff_return() {
        let i = |n: i64| Value::Integer(n.into());
        diff("If[x > 0, Return[x], Null]", &[("x", i(7))]);
        diff("Do[If[i == 3, Return[i]], {i, 1, 10}]", &[]);
    }

    #[test]
    fn diff_deep_register_pressure() {
        // Exercise the register allocator with many live temporaries.
        let i = |n: i64| Value::Integer(n.into());
        let mut src = "x".to_string();
        for _ in 0..40 {
            src = format!("({src}) + x");
        }
        diff(&src, &[("x", i(2))]);
    }

    #[test]
    fn diff_assignment_propagates_to_outer() {
        let i = |n: i64| Value::Integer(n.into());
        let body = parse_expr("g = x + 1");
        let params = vec![named("x")];

        // Tree-walk: `Set` on an existing outer binding updates the outer scope.
        let tree_env = make_env();
        tree_env.set("g".to_string(), i(1));
        let func_val = crate::eval::eval(
            &Expr::Function {
                params: vec!["x".to_string()],
                body: Box::new(body.clone()),
            },
            &tree_env,
        )
        .unwrap();
        crate::eval::apply_function(&func_val, &[i(41)], &tree_env).unwrap();
        assert_eq!(tree_env.get("g"), Some(i(42)));

        // Bytecode must agree: the global `g` is updated, not shadowed.
        let byte_env = make_env();
        byte_env.set("g".to_string(), i(1));
        let bc = BytecodeCompiler::compile_function(&params, &body, "f").unwrap();
        execute_bytecode(&bc, &[i(41)], &byte_env).unwrap();
        assert_eq!(byte_env.get("g"), Some(i(42)));
    }

    #[test]
    fn diff_catch_throw() {
        let i = |n: i64| Value::Integer(n.into());
        diff("Catch[Throw[5]]", &[]);
        diff("Catch[If[x < 0, Throw[\"neg\"], x]]", &[("x", i(3))]);
        diff("Catch[If[x < 0, Throw[\"neg\"], x]]", &[("x", i(-1))]);
        diff("Catch[x + 1]", &[("x", i(41))]);
        diff("Catch[x = 5]; x", &[("x", i(1))]);
    }

    #[test]
    fn diff_logical_short_circuit() {
        let i = |n: i64| Value::Integer(n.into());
        diff("And[x > 0, y > 0]", &[("x", i(1)), ("y", i(1))]);
        diff("Or[x > 0, y > 0]", &[("x", i(-1)), ("y", i(1))]);
        diff("And[x > 0, First[{}]]", &[("x", i(-1))]);
        diff("Or[x > 0, First[{}]]", &[("x", i(1))]);
    }

    // ── Broader coverage: loops with control flow ──────────────────────────

    fn list(xs: &[i64]) -> Value {
        Value::List(xs.iter().map(|&n| Value::Integer(n.into())).collect())
    }

    #[test]
    fn diff_break_continue_in_loops() {
        let i = |n: i64| Value::Integer(n.into());
        diff(
            "s = 0; Do[(If[k > 3, Break[]]; s = s + k), {k, 1, x}]; s",
            &[("x", i(10))],
        );
        diff(
            "s = 0; Do[(If[Mod[k, 2] == 0, Continue[]]; s = s + k), {k, 1, x}]; s",
            &[("x", i(6))],
        );
        diff(
            "s = 0; For[i = 1, i <= x, i++, s = s + i]; s",
            &[("x", i(4))],
        );
        diff("s = 0; While[s < x, s = s + 1]; s", &[("x", i(6))]);
    }

    #[test]
    fn diff_nested_loops_control() {
        let i = |n: i64| Value::Integer(n.into());
        diff(
            "s = 0; Do[Do[(If[j > i, Break[]]; s = s + 1), {j, 1, x}], {i, 1, x}]; s",
            &[("x", i(4))],
        );
    }

    #[test]
    fn diff_break_escapes_delegated_subtree() {
        // A `Break`/`Continue` inside a delegated special form must still be
        // caught by the enclosing compiled loop.
        let i = |n: i64| Value::Integer(n.into());
        diff(
            "s = 0; While[s < x, Module[{}, If[s == 2, Break[]]; s = s + 1]]; s",
            &[("x", i(6))],
        );
        diff(
            "s = 0; Do[Module[{}, If[Mod[k, 2] == 0, Continue[]]; s = s + k], {k, 1, x}]; s",
            &[("x", i(6))],
        );
        diff(
            "s = 0; Do[Catch[If[k == 3, Break[]]; s = s + k], {k, 1, x}]; s",
            &[("x", i(6))],
        );
    }

    #[test]
    fn diff_delegated_special_forms() {
        let i = |n: i64| Value::Integer(n.into());
        diff("Module[{y = x + 1}, y * 2]", &[("x", i(5))]);
        diff("Block[{y}, y = x + 1; y * 2]", &[("x", i(5))]);
        diff("With[{y = x + 1}, y * 2]", &[("x", i(5))]);
        diff("Table[i + x, {i, 3}]", &[("x", i(2))]);
        diff("Sum[k + x, {k, 1, 4}]", &[("x", i(1))]);
        diff("Product[k, {k, 1, x}]", &[("x", i(5))]);
        diff("Module[{}, If[x > 0, Return[x]]; 0]", &[("x", i(7))]);
    }

    #[test]
    fn diff_functional_and_string_ops() {
        let i = |n: i64| Value::Integer(n.into());
        diff("Map[Function[{y}, y + x], {1, 2, 3}]", &[("x", i(10))]);
        diff("Apply[Plus, {x, 1, 2}]", &[("x", i(10))]);
        diff("Select[{1, 2, 3, 4}, # > x &]", &[("x", i(2))]);
        diff("Fold[Plus, x, {1, 2, 3}]", &[("x", i(0))]);
        diff("Nest[Function[{y}, y + x], 0, 3]", &[("x", i(2))]);
        diff("(Function[{y}, x + y])[10]", &[("x", i(5))]);
        diff(
            "StringJoin[\"a\", x, \"b\"]",
            &[("x", Value::Str("X".into()))],
        );
        diff("ToUpperCase[x]", &[("x", Value::Str("ab".into()))]);
        diff("Part[x, 2]", &[("x", list(&[1, 2, 3]))]);
        diff("Take[x, 2]", &[("x", list(&[1, 2, 3]))]);
    }

    #[test]
    fn diff_error_paths() {
        let i = |n: i64| Value::Integer(n.into());
        diff("First[{}]", &[]);
        diff("Part[{}, 1]", &[]);
        diff("1/0", &[]);
        diff("1.0/0.0", &[]);
        diff("Catch[Throw[\"boom\"]]", &[]);
        diff("Catch[Throw[x, \"t\"], \"t\"]", &[("x", i(5))]);
    }

    #[test]
    fn diff_recursion() {
        let i = |n: i64| Value::Integer(n.into());
        // Recursion via a global definition set up inside the body.
        diff(
            "f = Function[{n}, If[n <= 1, 1, n * f[n - 1]]]; f[x]",
            &[("x", i(6))],
        );
    }
}
