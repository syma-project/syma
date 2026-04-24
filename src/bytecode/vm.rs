/// Register-based bytecode VM.
///
/// Executes `CompiledBytecode` by dispatching on each `Instruction`.
/// All function calls (`Apply`) delegate to the tree-walk evaluator's
/// `apply_function`, which returns the call back here for compiled callees.
///
use crate::bytecode::instruction::*;
use crate::bytecode::CompiledBytecode;
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

    let mut vm = VmState {
        regs,
        pc: 0,
        code: &bytecode.instructions,
        constants: &bytecode.constants,
        args,
        env,
        instr_count: 0,
    };
    vm.execute_loop()
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
}

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
                    self.regs[*d as usize] = Value::Bool(!self.regs[*s as usize].is_truthy());
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
                    let inv = self.builtin2(
                        "Power",
                        &self.regs[*b as usize],
                        &Value::Integer((-1).into()),
                    )?;
                    self.regs[*d as usize] =
                        self.builtin2("Times", &self.regs[*a as usize], &inv)?;
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
                            self.regs[*d as usize] =
                                self.builtin2("Plus", av, &neg_b)?;
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
                    self.real_binop(*d, *a, *b, |x, y| {
                        let mut r = x;
                        r *= y.to_f64();
                        Value::Real(r)
                    }, "Times")?;
                }
                Instruction::RealDiv(d, a, b) => {
                    let av = &self.regs[*a as usize];
                    let bv = &self.regs[*b as usize];
                    match (av, bv) {
                        (Value::Real(af), Value::Real(bf)) => {
                            let mut r = af.clone();
                            r /= bf.to_f64();
                            self.regs[*d as usize] = Value::Real(r);
                        }
                        _ => {
                            let inv = self.builtin2("Power", bv, &Value::Integer((-1).into()))?;
                            self.regs[*d as usize] = self.builtin2("Times", av, &inv)?;
                        }
                    }
                }

                // Comparison
                Instruction::Eq(d, a, b) => {
                    self.regs[*d as usize] =
                        Value::Bool(self.regs[*a as usize] == self.regs[*b as usize]);
                }
                Instruction::Neq(d, a, b) => {
                    self.regs[*d as usize] =
                        Value::Bool(self.regs[*a as usize] != self.regs[*b as usize]);
                }
                Instruction::Lt(d, a, b) => {
                    self.regs[*d as usize] =
                        self.builtin2("Less", &self.regs[*a as usize], &self.regs[*b as usize])?;
                }
                Instruction::Gt(d, a, b) => {
                    self.regs[*d as usize] = self.builtin2(
                        "Greater",
                        &self.regs[*a as usize],
                        &self.regs[*b as usize],
                    )?;
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
                    let av = self.regs[*a as usize].is_truthy();
                    let bv = self.regs[*b as usize].is_truthy();
                    self.regs[*d as usize] = Value::Bool(av && bv);
                }
                Instruction::Or(d, a, b) => {
                    let av = self.regs[*a as usize].is_truthy();
                    let bv = self.regs[*b as usize].is_truthy();
                    self.regs[*d as usize] = Value::Bool(av || bv);
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
                    self.regs[*d as usize] = self
                        .args
                        .get(*idx as usize)
                        .cloned()
                        .unwrap_or(Value::Null);
                }
                Instruction::LoadSym(d, idx) => {
                    let name = self.const_str(*idx);
                    let val = self
                        .env
                        .get(&name)
                        .unwrap_or_else(|| Value::Symbol(name));
                    self.regs[*d as usize] = val;
                }
                Instruction::StoreSym(idx, s) => {
                    let name = self.const_str(*idx);
                    self.env.set(name, self.regs[*s as usize].clone());
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

trait Truthy {
    fn is_truthy(&self) -> bool;
}

impl Truthy for Value {
    fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Integer(n) => *n != 0,
            Value::Real(r) => *r != 0.0,
            Value::Null => false,
            Value::Symbol(s) => s != "False",
            _ => true,
        }
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
        | Instruction::Jump(_) => 0,

        // 1 register
        Instruction::LoadNull(d)
        | Instruction::LoadTrue(d)
        | Instruction::LoadFalse(d) => *d as usize,

        // 2 registers — max of both
        Instruction::Mov(d, s)
        | Instruction::Neg(d, s)
        | Instruction::Not(d, s) => (*d).max(*s) as usize,

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
        | Instruction::LoadSym(d, _) => *d as usize,

        // MakeSeq: dest register only
        Instruction::MakeSeq(d, _) => *d as usize,

        // StoreSym(idx, s) — s is source register
        Instruction::StoreSym(_, s) => *s as usize,

        // reg + i32
        Instruction::JumpIfZero(r, _)
        | Instruction::JumpIfNotZero(r, _) => *r as usize,

        // Return
        Instruction::Return(r) => *r as usize,
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins;
    use crate::bytecode::compiler::BytecodeCompiler;
    use crate::bytecode::CompiledBytecode;
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
        };
        let err = run(&bc, &[]).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("out of bounds"), "expected bounds error, got: {msg}");
    }

    #[test]
    fn test_bounds_validation_make_list() {
        // MakeList at reg 0 with n=5 touches regs 0..5, but nregs=3
        let bc = CompiledBytecode {
            instructions: vec![Instruction::MakeList(0, 5), Instruction::Return(0)],
            constants: vec![],
            nregs: 3,
            nparams: 0,
        };
        let err = run(&bc, &[]).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("out of bounds"), "expected bounds error, got: {msg}");
    }

    #[test]
    fn test_bounds_validation_jump_if_zero() {
        let bc = CompiledBytecode {
            instructions: vec![Instruction::JumpIfZero(99, 1), Instruction::Return(99)],
            constants: vec![],
            nregs: 3,
            nparams: 0,
        };
        let err = run(&bc, &[]).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("out of bounds"), "expected bounds error, got: {msg}");
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
