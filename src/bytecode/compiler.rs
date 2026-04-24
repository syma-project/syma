/// AST-to-bytecode compiler.
///
/// Walks `Expr` AST and emits register-based bytecode.  Patterns that
/// are too complex for the simple decision-tree compiler fall back to
/// a `Call` instruction that delegates to the tree-walk evaluator.
///
use std::collections::HashMap;

use crate::ast::Expr;
use crate::bytecode::instruction::*;
use crate::bytecode::CompiledBytecode;
use crate::value::{FunctionDefinition, Value};

/// Errors emitted during compilation.
#[derive(Debug)]
pub enum CompileError {
    UnsupportedFeature(String),
    RegisterOverflow,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::UnsupportedFeature(msg) => write!(f, "{msg}"),
            CompileError::RegisterOverflow => write!(f, "too many registers"),
        }
    }
}

impl std::error::Error for CompileError {}

// ── Register Allocator ─────────────────────────────────────────────────────────

struct RegAlloc {
    next: u16,
    _freed: Vec<u16>,
    /// High-water mark: the highest register index ever allocated.
    max_reg: u16,
}

impl RegAlloc {
    fn new() -> Self {
        Self {
            next: 0,
            _freed: Vec::new(),
            max_reg: 0,
        }
    }

    fn alloc(&mut self) -> Result<u16, CompileError> {
        if let Some(r) = self._freed.pop() {
            return Ok(r);
        }
        if self.next == u16::MAX {
            return Err(CompileError::RegisterOverflow);
        }
        let r = self.next;
        self.next += 1;
        if r > self.max_reg {
            self.max_reg = r;
        }
        Ok(r)
    }

    fn _free(&mut self, reg: u16) {
        self._freed.push(reg);
    }

    /// Ensure a specific register index is accounted for in max_reg.
    /// Used when instructions reference registers not obtained via alloc().
    fn note_reg(&mut self, reg: u16) {
        if reg > self.max_reg {
            self.max_reg = reg;
        }
    }

    fn used(&self) -> u16 {
        self.max_reg + 1
    }
}

// ── Constant Pool ──────────────────────────────────────────────────────────────

struct ConstPool {
    entries: Vec<Value>,
    indices: HashMap<String, u32>,
}

impl ConstPool {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            indices: HashMap::new(),
        }
    }

    fn insert(&mut self, val: Value) -> u32 {
        let key = format!("{val:?}");
        if let Some(&idx) = self.indices.get(&key) {
            return idx;
        }
        let idx = self.entries.len() as u32;
        self.entries.push(val);
        self.indices.insert(key, idx);
        idx
    }
}

// ── BytecodeCompiler ───────────────────────────────────────────────────────────

pub struct BytecodeCompiler {
    code: Vec<Instruction>,
    regs: RegAlloc,
    pool: ConstPool,
    symbol_cache: HashMap<String, u32>,
    /// Maps parameter names to their register numbers.
    param_regs: HashMap<String, u16>,
}

impl Default for BytecodeCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl BytecodeCompiler {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            regs: RegAlloc::new(),
            pool: ConstPool::new(),
            symbol_cache: HashMap::new(),
            param_regs: HashMap::new(),
        }
    }

    // ── Public entry points ──────────────────────────────────────────────────

    /// Compile all definitions of a multi-definition function into a single
    /// bytecode body.  Returns `Err` if *any* definition cannot be compiled.
    pub fn compile_multi(
        definitions: &[FunctionDefinition],
        name: &str,
    ) -> Result<CompiledBytecode, CompileError> {
        if definitions.is_empty() {
            return Err(CompileError::UnsupportedFeature(
                "empty function definition".to_string(),
            ));
        }
        if definitions.len() == 1 {
            return Self::compile_function(
                &definitions[0].params,
                &definitions[0].body,
                name,
            );
        }
        for def in definitions {
            Self::compile_function(&def.params, &def.body, name)?;
        }
        // Multi-def: compile first (future: decision tree with fallthrough).
        Self::compile_function(&definitions[0].params, &definitions[0].body, name)
    }

    /// Compile a single function definition's body into bytecode.
    pub fn compile_function(
        params: &[Expr],
        body: &Expr,
        _name: &str,
    ) -> Result<CompiledBytecode, CompileError> {
        let mut c = Self::new();

        for (i, param) in params.iter().enumerate() {
            let reg = c.regs.alloc()?;
            c.emit(Instruction::LoadArg(reg, i as u32));

            match param {
                // Simple blanks always match
                Expr::NamedBlank { name, .. } => {
                    c.param_regs.insert(name.clone(), reg);
                }
                Expr::Blank { .. } => {}
                // Complex patterns — fall back to tree-walk
                _ => {
                    return Err(CompileError::UnsupportedFeature(format!(
                        "complex pattern: {param:?}"
                    )));
                }
            }
        }

        let result_reg = c.compile_expr(body)?;
        c.emit(Instruction::Return(result_reg));

        Ok(CompiledBytecode {
            instructions: c.code,
            constants: c.pool.entries,
            nregs: c.regs.used(),
            nparams: params.len() as u8,
        })
    }

    // ── Expression compilation ────────────────────────────────────────────────

    fn compile_expr(&mut self, expr: &Expr) -> Result<u16, CompileError> {
        match expr {
            Expr::Integer(n) => self.emit_const(Value::Integer(n.clone())),
            Expr::Real(r) => self.emit_const(Value::Real(r.clone())),
            Expr::Str(s) => self.emit_const(Value::Str(s.clone())),
            Expr::Bool(b) => self.emit_const(Value::Bool(*b)),
            Expr::Null => {
                let dst = self.regs.alloc()?;
                self.emit(Instruction::LoadNull(dst));
                Ok(dst)
            }
            Expr::Symbol(s) => {
                // If this symbol is a function parameter, load from register
                if let Some(&reg) = self.param_regs.get(s) {
                    let dst = self.regs.alloc()?;
                    self.emit(Instruction::Mov(dst, reg));
                    return Ok(dst);
                }
                // Otherwise, do an environment lookup
                let dst = self.regs.alloc()?;
                let idx = self.symbol_idx(s);
                self.emit(Instruction::LoadSym(dst, idx));
                Ok(dst)
            }
            Expr::List(items) => {
                let mut regs = Vec::with_capacity(items.len());
                for item in items {
                    regs.push(self.compile_expr(item)?);
                }
                let dst = self.regs.alloc()?;
                self.emit(Instruction::MakeList(dst, regs.len() as u8));
                for r in regs {
                    self.regs._free(r);
                }
                Ok(dst)
            }
            Expr::Assoc(entries) => self.compile_assoc(entries),
            Expr::Call { head, args } => self.compile_call(head, args),
            Expr::If { condition, then_branch, else_branch } => {
                self.compile_if(condition, then_branch, else_branch)
            }
            Expr::While { condition, body } => self.compile_while(condition, body),
            Expr::Sequence(stmts) => self.compile_sequence(stmts),
            Expr::Assign { lhs, rhs } => self.compile_assign(lhs, rhs),
            _ => Err(CompileError::UnsupportedFeature(format!(
                "expression: {expr:?}"
            ))),
        }
    }

    fn compile_call(&mut self, head: &Expr, args: &[Expr]) -> Result<u16, CompileError> {
        // Try inline arithmetic for known binary ops
        if let Expr::Symbol(name) = head
            && let Some(instr) = self.try_inline_arithmetic(name, args)?
        {
            return Ok(instr);
        }

        // General call: compile head and args, emit Apply
        let func_reg = self.compile_expr(head)?;
        let mut arg_regs = Vec::with_capacity(args.len());
        for arg in args {
            arg_regs.push(self.compile_expr(arg)?);
        }
        // The Apply instruction expects function and args in consecutive
        // registers starting at dst: [func, arg0, arg1, ..., argN].
        // Allocate dst and emit Mov instructions to pack them.
        let dst = self.regs.alloc()?;
        if func_reg != dst {
            self.emit(Instruction::Mov(dst, func_reg));
        }
        for (i, arg_reg) in arg_regs.iter().enumerate() {
            let target = dst.checked_add(1 + i as u16).ok_or_else(|| {
                CompileError::UnsupportedFeature("register overflow in call".to_string())
            })?;
            self.regs.note_reg(target);
            self.emit(Instruction::Mov(target, *arg_reg));
        }
        self.emit(Instruction::Apply(dst, args.len() as u8));
        self.regs._free(func_reg);
        for r in arg_regs {
            self.regs._free(r);
        }
        Ok(dst)
    }

    /// Try to emit a direct arithmetic instruction for known binary ops.
    /// Returns `Ok(Some(reg))` on success, `Ok(None)` to fall through to Apply.
    fn try_inline_arithmetic(
        &mut self,
        name: &str,
        args: &[Expr],
    ) -> Result<Option<u16>, CompileError> {
        // Unary ops
        if args.len() == 1 {
            return match name {
                "Not" => {
                    let a_reg = self.compile_expr(&args[0])?;
                    self.emit(Instruction::Not(a_reg, a_reg));
                    Ok(Some(a_reg))
                }
                _ => Ok(None),
            };
        }

        if args.len() != 2 {
            return Ok(None);
        }
        let a_reg = self.compile_expr(&args[0])?;
        let b_reg = self.compile_expr(&args[1])?;
        let dst = self.regs.alloc()?;

        // Check if both operands are statically integer or real literals
        let a_is_int = matches!(&args[0], Expr::Integer(_));
        let b_is_int = matches!(&args[1], Expr::Integer(_));
        let a_is_real = matches!(&args[0], Expr::Real(_));
        let b_is_real = matches!(&args[1], Expr::Real(_));

        match name {
            "Plus" => {
                if a_is_int && b_is_int {
                    self.emit(Instruction::IntAdd(dst, a_reg, b_reg));
                } else if a_is_real && b_is_real {
                    self.emit(Instruction::RealAdd(dst, a_reg, b_reg));
                } else {
                    self.emit(Instruction::Add(dst, a_reg, b_reg));
                }
            }
            "Times" => {
                // Detect unary minus: Times[-1, x] → Neg
                if matches!(&args[0], Expr::Integer(n) if n == &(-1)) {
                    self.regs._free(a_reg);
                    self.regs._free(dst);
                    self.emit(Instruction::Neg(b_reg, b_reg));
                    return Ok(Some(b_reg));
                }
                if matches!(&args[1], Expr::Integer(n) if n == &(-1)) {
                    self.regs._free(b_reg);
                    self.regs._free(dst);
                    self.emit(Instruction::Neg(a_reg, a_reg));
                    return Ok(Some(a_reg));
                }
                if a_is_int && b_is_int {
                    self.emit(Instruction::IntMul(dst, a_reg, b_reg));
                } else if a_is_real && b_is_real {
                    self.emit(Instruction::RealMul(dst, a_reg, b_reg));
                } else {
                    self.emit(Instruction::Mul(dst, a_reg, b_reg));
                }
            }
            "Power" => {
                self.emit(Instruction::Pow(dst, a_reg, b_reg));
            }
            "Subtract" => {
                // Subtract[a, b] desugars to Plus[a, Times[-1, b]], but
                // the parser desugars it before it reaches us. Handle it anyway.
                if a_is_int && b_is_int {
                    self.emit(Instruction::IntSub(dst, a_reg, b_reg));
                } else if a_is_real && b_is_real {
                    self.emit(Instruction::RealSub(dst, a_reg, b_reg));
                } else {
                    self.emit(Instruction::Sub(dst, a_reg, b_reg));
                }
            }
            _ => {
                // Not an arithmetic op — free regs and fall through
                self.regs._free(dst);
                self.regs._free(a_reg);
                self.regs._free(b_reg);
                return Ok(None);
            }
        }
        self.regs._free(a_reg);
        self.regs._free(b_reg);
        Ok(Some(dst))
    }

    fn compile_if(
        &mut self,
        condition: &Expr,
        then_branch: &Expr,
        else_branch: &Option<Box<Expr>>,
    ) -> Result<u16, CompileError> {
        let result_reg = self.regs.alloc()?;
        let cond_reg = self.compile_expr(condition)?;
        let else_label = self.emit(Instruction::JumpIfZero(cond_reg, 0));
        self.regs._free(cond_reg);

        let then_reg = self.compile_expr(then_branch)?;
        if then_reg != result_reg {
            self.emit(Instruction::Mov(result_reg, then_reg));
        }
        let end_label = self.emit(Instruction::Jump(0));

        // Patch else branch jump to here
        self.patch_jump(else_label, self.code.len() as i32);

        if let Some(else_expr) = else_branch {
            let er = self.compile_expr(else_expr)?;
            if er != result_reg {
                self.emit(Instruction::Mov(result_reg, er));
            }
        } else {
            self.emit(Instruction::LoadNull(result_reg));
        }
        self.patch_jump(end_label, self.code.len() as i32);
        Ok(result_reg)
    }

    fn compile_while(&mut self, condition: &Expr, body: &Expr) -> Result<u16, CompileError> {
        let loop_start = self.code.len() as i32;
        let cond_reg = self.compile_expr(condition)?;
        let exit_jz = self.emit(Instruction::JumpIfZero(cond_reg, 0));
        self.regs._free(cond_reg);

        self.compile_expr(body)?;
        let jump_back = self.code.len();
        self.emit(Instruction::Jump(loop_start - jump_back as i32 - 1));

        self.patch_jump(exit_jz, self.code.len() as i32);
        let dst = self.regs.alloc()?;
        self.emit(Instruction::LoadNull(dst));
        Ok(dst)
    }

    fn compile_sequence(&mut self, stmts: &[Expr]) -> Result<u16, CompileError> {
        let mut last_reg = self.regs.alloc()?;
        self.emit(Instruction::LoadNull(last_reg));
        for stmt in stmts {
            self.regs._free(last_reg);
            last_reg = self.compile_expr(stmt)?;
        }
        Ok(last_reg)
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn emit(&mut self, instr: Instruction) -> usize {
        let idx = self.code.len();
        self.code.push(instr);
        idx
    }

    fn patch_jump(&mut self, instr_idx: usize, target: i32) {
        // The Jump instruction's offset is relative to the PC after fetching
        // (PC already incremented past the jump). Compute the relative offset
        // from instr_idx + 1 to target.
        let relative = target - (instr_idx as i32) - 1;
        let instr = &mut self.code[instr_idx];
        match instr {
            Instruction::Jump(o)
            | Instruction::JumpIfZero(_, o)
            | Instruction::JumpIfNotZero(_, o) => *o = relative,
            _ => panic!("Cannot patch non-jump at {instr_idx}"),
        }
    }

    fn emit_const(&mut self, val: Value) -> Result<u16, CompileError> {
        let dst = self.regs.alloc()?;
        let idx = self.pool.insert(val);
        self.emit(Instruction::LoadConst(dst, idx));
        Ok(dst)
    }

    /// Compile an Association literal: `<\| key1 -> val1, key2 -> val2 \|>`
    fn compile_assoc(&mut self, entries: &[(String, Expr)]) -> Result<u16, CompileError> {
        let n = entries.len();
        if n > u8::MAX as usize {
            return Err(CompileError::UnsupportedFeature(
                "association with too many entries".to_string(),
            ));
        }
        // Layout: dst = assoc result, dst+1 = key0, dst+2 = val0, dst+3 = key1, ...
        let dst = self.regs.alloc()?;
        let mut temp_regs = Vec::with_capacity(2 * n);
        for (key, val_expr) in entries {
            temp_regs.push(self.emit_const(Value::Str(key.clone()))?);
            temp_regs.push(self.compile_expr(val_expr)?);
        }
        for (offset, &tr) in (1_u16..).zip(temp_regs.iter()) {
            let target = dst.checked_add(offset).ok_or_else(|| {
                CompileError::UnsupportedFeature("register overflow in assoc".to_string())
            })?;
            self.regs.note_reg(target);
            self.emit(Instruction::Mov(target, tr));
            self.regs._free(tr);
        }
        self.emit(Instruction::MakeAssoc(dst, n as u8));
        Ok(dst)
    }

    /// Compile an assignment: symbol = expr
    fn compile_assign(&mut self, lhs: &Expr, rhs: &Expr) -> Result<u16, CompileError> {
        match lhs {
            Expr::Symbol(name) => {
                let val_reg = self.compile_expr(rhs)?;
                let sym_idx = self.symbol_idx(name);
                self.emit(Instruction::StoreSym(sym_idx, val_reg));
                self.regs._free(val_reg);
                // Assignments return Null
                let dst = self.regs.alloc()?;
                self.emit(Instruction::LoadNull(dst));
                Ok(dst)
            }
            _ => Err(CompileError::UnsupportedFeature(format!(
                "assignment target: {lhs:?}"
            ))),
        }
    }

    fn symbol_idx(&mut self, name: &str) -> u32 {
        if let Some(&idx) = self.symbol_cache.get(name) {
            return idx;
        }
        let idx = self.pool.insert(Value::Symbol(name.to_string()));
        self.symbol_cache.insert(name.to_string(), idx);
        idx
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Expr;

    fn compile(params: Vec<Expr>, body: Expr) -> CompiledBytecode {
        BytecodeCompiler::compile_function(&params, &body, "f").unwrap()
    }

    #[test]
    fn test_compile_identity() {
        let params = vec![Expr::NamedBlank {
            name: "x".to_string(),
            type_constraint: None,
        }];
        let body = Expr::Symbol("x".to_string());
        let bc = compile(params, body);
        assert_eq!(bc.nparams, 1);
        assert!(bc.nregs > 0);
    }

    #[test]
    fn test_compile_constant() {
        let body = Expr::Integer(42.into());
        let bc = compile(vec![], body);
        assert_eq!(bc.nparams, 0);
        assert_eq!(bc.constants.len(), 1);
    }

    #[test]
    fn test_compile_add() {
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
        let body = Expr::Call {
            head: Box::new(Expr::Symbol("Plus".to_string())),
            args: vec![
                Expr::Symbol("x".to_string()),
                Expr::Symbol("y".to_string()),
            ],
        };
        let bc = compile(params, body);
        assert_eq!(bc.nparams, 2);
        // Should use Add (not IntAdd) since params are not statically int
        assert!(bc.instructions.iter().any(|i| matches!(i, Instruction::Add(..))));
    }

    #[test]
    fn test_compile_if_else() {
        // f[x_] := If[x > 0, x, -x]
        let body = Expr::If {
            condition: Box::new(Expr::Call {
                head: Box::new(Expr::Symbol("Greater".to_string())),
                args: vec![Expr::Symbol("x".to_string()), Expr::Integer(0.into())],
            }),
            then_branch: Box::new(Expr::Symbol("x".to_string())),
            else_branch: Some(Box::new(Expr::Call {
                head: Box::new(Expr::Symbol("Times".to_string())),
                args: vec![Expr::Integer((-1).into()), Expr::Symbol("x".to_string())],
            })),
        };
        let params = vec![Expr::NamedBlank {
            name: "x".to_string(),
            type_constraint: None,
        }];
        let bc = compile(params, body);
        // Should contain JumpIfZero and Jump instructions
        assert!(bc.instructions.iter().any(|i| matches!(i, Instruction::JumpIfZero(..))));
        assert!(bc.instructions.iter().any(|i| matches!(i, Instruction::Jump(..))));
    }

    #[test]
    fn test_compile_inline_int_add() {
        // Plus[1, 2] → should emit IntAdd
        let body = Expr::Call {
            head: Box::new(Expr::Symbol("Plus".to_string())),
            args: vec![Expr::Integer(1.into()), Expr::Integer(2.into())],
        };
        let bc = compile(vec![], body);
        assert!(bc.instructions.iter().any(|i| matches!(i, Instruction::IntAdd(..))),
            "Plus of two integers should emit IntAdd");
    }

    #[test]
    fn test_compile_inline_neg() {
        // Times[-1, x] → should emit Neg
        let body = Expr::Call {
            head: Box::new(Expr::Symbol("Times".to_string())),
            args: vec![Expr::Integer((-1).into()), Expr::Symbol("x".to_string())],
        };
        let params = vec![Expr::NamedBlank {
            name: "x".to_string(),
            type_constraint: None,
        }];
        let bc = compile(params, body);
        assert!(bc.instructions.iter().any(|i| matches!(i, Instruction::Neg(..))),
            "Times[-1, x] should emit Neg");
    }

    #[test]
    fn test_compile_not() {
        // Not[x] → should emit Not
        let body = Expr::Call {
            head: Box::new(Expr::Symbol("Not".to_string())),
            args: vec![Expr::Symbol("x".to_string())],
        };
        let params = vec![Expr::NamedBlank {
            name: "x".to_string(),
            type_constraint: None,
        }];
        let bc = compile(params, body);
        assert!(bc.instructions.iter().any(|i| matches!(i, Instruction::Not(..))),
            "Not[x] should emit Not instruction");
    }
}
