/// AST-to-bytecode compiler.
///
/// Walks `Expr` AST and emits register-based bytecode.  Patterns that
/// are too complex for the simple decision-tree compiler fall back to
/// a `Call` instruction that delegates to the tree-walk evaluator.
///
use std::collections::HashMap;

use rug::Integer;

use crate::ast::Expr;
use crate::ast::IteratorSpec;
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
                // Sequence: collect remaining args into a list
                Expr::BlankSequence {
                    name: Some(name), ..
                }
                | Expr::BlankNullSequence {
                    name: Some(name), ..
                } => {
                    let seq_reg = c.regs.alloc()?;
                    c.emit(Instruction::MakeSeq(seq_reg, i as u16));
                    c.param_regs.insert(name.clone(), seq_reg);
                }
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
            Expr::Bool(true) => {
                let dst = self.regs.alloc()?;
                self.emit(Instruction::LoadTrue(dst));
                Ok(dst)
            }
            Expr::Bool(false) => {
                let dst = self.regs.alloc()?;
                self.emit(Instruction::LoadFalse(dst));
                Ok(dst)
            }
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
            // ── Desugar Map to Call["Map", ...] ──
            Expr::Map { func, list } => self.compile_expr(&Expr::Call {
                head: Box::new(Expr::Symbol("Map".to_string())),
                args: vec![*func.clone(), *list.clone()],
            }),
            // ── Desugar Pipe[expr, func] to Call[func, expr] ──
            Expr::Pipe { expr, func } => self.compile_expr(&Expr::Call {
                head: func.clone(),
                args: vec![*expr.clone()],
            }),
            // ── Desugar Prefix[func, arg] to Call[func, arg] ──
            Expr::Prefix { func, arg } => self.compile_expr(&Expr::Call {
                head: func.clone(),
                args: vec![*arg.clone()],
            }),
            // ── Desugar Apply[func, expr] to Call["Apply", func, expr] ──
            Expr::Apply { func, expr } => self.compile_expr(&Expr::Call {
                head: Box::new(Expr::Symbol("Apply".to_string())),
                args: vec![*func.clone(), *expr.clone()],
            }),
            // ── Which[cond1, val1, cond2, val2, ...] ──
            Expr::Which { pairs } => self.compile_which(pairs),
            // ── Switch[expr, pat1, val1, pat2, val2, ...] ──
            Expr::Switch { expr, cases } => self.compile_switch(expr, cases),
            // ── ReleaseHold — unwrap and compile inner expr ──
            Expr::ReleaseHold(inner) => self.compile_expr(inner),
            // ── Complex literal ──
            Expr::Complex { re, im } => self.emit_const(Value::Complex { re: *re, im: *im }),
            // ── For loop ──
            Expr::For { init, condition, step, body } => self.compile_for(init, condition, step, body),
            // ── Do loop (range iterator only) ──
            Expr::Do { body, iterator } => self.compile_do(body, iterator),
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
            // ── Comparisons ──
            "Equal" => {
                self.emit(Instruction::Eq(dst, a_reg, b_reg));
            }
            "Unequal" => {
                self.emit(Instruction::Neq(dst, a_reg, b_reg));
            }
            "Less" => {
                self.emit(Instruction::Lt(dst, a_reg, b_reg));
            }
            "Greater" => {
                self.emit(Instruction::Gt(dst, a_reg, b_reg));
            }
            "LessEqual" => {
                self.emit(Instruction::Le(dst, a_reg, b_reg));
            }
            "GreaterEqual" => {
                self.emit(Instruction::Ge(dst, a_reg, b_reg));
            }
            // ── Logical ──
            "And" => {
                self.emit(Instruction::And(dst, a_reg, b_reg));
            }
            "Or" => {
                self.emit(Instruction::Or(dst, a_reg, b_reg));
            }
            // ── Division ──
            "Divide" => {
                if a_is_real && b_is_real {
                    self.emit(Instruction::RealDiv(dst, a_reg, b_reg));
                } else {
                    self.emit(Instruction::Div(dst, a_reg, b_reg));
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

        // Optimize: If[Not[x], ...] uses JumpIfNotZero instead of JumpIfZero
        let (cond_reg, else_label) = if let Expr::Call { head, args } = condition
            && let Expr::Symbol(name) = head.as_ref()
            && name == "Not"
            && args.len() == 1
        {
            let inner_reg = self.compile_expr(&args[0])?;
            let lbl = self.emit(Instruction::JumpIfNotZero(inner_reg, 0));
            (inner_reg, lbl)
        } else {
            let cr = self.compile_expr(condition)?;
            let lbl = self.emit(Instruction::JumpIfZero(cr, 0));
            (cr, lbl)
        };
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

        let body_reg = self.compile_expr(body)?;
        self.regs._free(body_reg);
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

    /// Compile a For loop: For[init, condition, step, body]
    fn compile_for(
        &mut self,
        init: &Expr,
        condition: &Expr,
        step: &Expr,
        body: &Expr,
    ) -> Result<u16, CompileError> {
        // Init (e.g., i = 1)
        let init_reg = self.compile_expr(init)?;
        self.regs._free(init_reg);

        let loop_start = self.code.len() as i32;

        // Condition check
        let cond_reg = self.compile_expr(condition)?;
        let exit_jz = self.emit(Instruction::JumpIfZero(cond_reg, 0));
        self.regs._free(cond_reg);

        // Body
        let body_reg = self.compile_expr(body)?;
        self.regs._free(body_reg);

        // Step (e.g., i = i + 1)
        let step_reg = self.compile_expr(step)?;
        self.regs._free(step_reg);

        // Jump back to condition
        let jump_back = self.code.len();
        self.emit(Instruction::Jump(loop_start - jump_back as i32 - 1));

        self.patch_jump(exit_jz, self.code.len() as i32);
        let dst = self.regs.alloc()?;
        self.emit(Instruction::LoadNull(dst));
        Ok(dst)
    }

    /// Compile a Do loop with range iterator: Do[body, {var, min, max}]
    fn compile_do(&mut self, body: &Expr, iterator: &IteratorSpec) -> Result<u16, CompileError> {
        match iterator {
            IteratorSpec::Range { var, min, max } => {
                let var_expr = Expr::Symbol(var.clone());

                // Construct: var = min
                let init = Expr::Assign {
                    lhs: Box::new(var_expr.clone()),
                    rhs: min.clone(),
                };

                // Construct: var <= max
                let condition = Expr::Call {
                    head: Box::new(Expr::Symbol("LessEqual".to_string())),
                    args: vec![var_expr.clone(), *max.clone()],
                };

                // Construct: var = var + 1
                let step_rhs = Expr::Call {
                    head: Box::new(Expr::Symbol("Plus".to_string())),
                    args: vec![var_expr.clone(), Expr::Integer(Integer::from(1))],
                };
                let step = Expr::Assign {
                    lhs: Box::new(var_expr.clone()),
                    rhs: Box::new(step_rhs),
                };

                // Compile as For-equivalent loop
                self.compile_for(&init, &condition, &step, body)
            }
            IteratorSpec::List { .. } => Err(CompileError::UnsupportedFeature(
                "Do loop with list iterator".to_string(),
            )),
        }
    }

    /// Compile a Which expression: Which[cond1, val1, cond2, val2, ...]
    fn compile_which(&mut self, pairs: &[(Expr, Expr)]) -> Result<u16, CompileError> {
        let result_reg = self.regs.alloc()?;
        // Track JumpIfZero instructions (next case) and Jump instructions (past LoadNull)
        let mut next_labels = Vec::new();
        let mut end_jumps = Vec::new();

        for (i, (cond, val)) in pairs.iter().enumerate() {
            let cond_reg = self.compile_expr(cond)?;
            let next_label = self.emit(Instruction::JumpIfZero(cond_reg, 0));
            self.regs._free(cond_reg);

            let val_reg = self.compile_expr(val)?;
            if val_reg != result_reg {
                self.emit(Instruction::Mov(result_reg, val_reg));
            }
            self.regs._free(val_reg);

            // Every case needs a Jump past the LoadNull so matched values
            // don't get overwritten by the "no match" result.
            let end_jump = self.emit(Instruction::Jump(0));

            if i < pairs.len() - 1 {
                // Non-last case: JumpIfZero skips past the value + end_jump
                next_labels.push(next_label);
                end_jumps.push(end_jump);
            } else {
                // Last case: JumpIfZero goes to LoadNull (no match)
                self.patch_jump(next_label, self.code.len() as i32);
                end_jumps.push(end_jump);
            }
        }

        // No match — return Null
        self.emit(Instruction::LoadNull(result_reg));

        for (&next_label, &end_jump) in next_labels.iter().zip(end_jumps.iter()) {
            // JumpIfZero → instruction past the corresponding Jump(end)
            self.patch_jump(next_label, end_jump as i32 + 1);
        }
        for &end_jump in &end_jumps {
            // Jump(end) → past LoadNull
            self.patch_jump(end_jump, self.code.len() as i32);
        }

        Ok(result_reg)
    }

    /// Compile a Switch expression: Switch[expr, pat1, val1, pat2, val2, ...]
    fn compile_switch(&mut self, expr: &Expr, cases: &[(Expr, Expr)]) -> Result<u16, CompileError> {
        let switch_reg = self.compile_expr(expr)?;
        let result_reg = self.regs.alloc()?;
        let mut patch_labels = Vec::new();

        for (i, (pat, val)) in cases.iter().enumerate() {
            let pat_reg = self.compile_expr(pat)?;

            // Compare switch value with pattern
            let cmp_reg = self.regs.alloc()?;
            self.emit(Instruction::Eq(cmp_reg, switch_reg, pat_reg));
            self.regs._free(pat_reg);

            let next_label = self.emit(Instruction::JumpIfZero(cmp_reg, 0));
            self.regs._free(cmp_reg);

            let val_reg = self.compile_expr(val)?;
            if val_reg != result_reg {
                self.emit(Instruction::Mov(result_reg, val_reg));
            }
            self.regs._free(val_reg);

            if i < cases.len() - 1 {
                let end_jump = self.emit(Instruction::Jump(0));
                patch_labels.push((next_label, end_jump));
            } else {
                self.patch_jump(next_label, self.code.len() as i32);
            }
        }

        // No match — return Null
        self.emit(Instruction::LoadNull(result_reg));

        for (next_label, end_jump) in patch_labels {
            self.patch_jump(next_label, end_jump as i32 + 1);
            self.patch_jump(end_jump, self.code.len() as i32);
        }

        self.regs._free(switch_reg);
        Ok(result_reg)
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
    use crate::ast::{Expr, IteratorSpec};

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

    #[test]
    fn test_compile_for() {
        // For[Null, Symbol("x"), Null, Null] should produce a loop structure
        let body = Expr::For {
            init: Box::new(Expr::Null),
            condition: Box::new(Expr::Symbol("x".to_string())),
            step: Box::new(Expr::Null),
            body: Box::new(Expr::Null),
        };
        let bc = compile(vec![], body);
        assert!(bc.instructions.iter().any(|i| matches!(i, Instruction::JumpIfZero(..))),
            "For loop should contain JumpIfZero");
        assert!(bc.instructions.iter().any(|i| matches!(i, Instruction::Jump(..))),
            "For loop should contain Jump");
    }

    #[test]
    fn test_compile_do_range() {
        // Do[Null, {x, 1, 10}] should compile (range iterator)
        let body = Expr::Do {
            body: Box::new(Expr::Null),
            iterator: IteratorSpec::Range {
                var: "x".to_string(),
                min: Box::new(Expr::Integer(1.into())),
                max: Box::new(Expr::Integer(10.into())),
            },
        };
        let bc = compile(vec![], body);
        assert!(bc.instructions.iter().any(|i| matches!(i, Instruction::JumpIfZero(..))),
            "Do loop should contain JumpIfZero");
    }

    #[test]
    fn test_compile_for_registers() {
        // For[Null, Symbol("x"), Null, Null] should compile without errors
        let body = Expr::For {
            init: Box::new(Expr::Null),
            condition: Box::new(Expr::Symbol("x".to_string())),
            step: Box::new(Expr::Null),
            body: Box::new(Expr::Null),
        };
        let bc = compile(vec![], body);
        assert!(bc.nregs > 0, "Should allocate at least one register");
        assert!(bc.nparams == 0, "Should have 0 parameters");
    }

    #[test]
    fn test_compile_map_desugar() {
        // Map[Length, {1, 2}] desugars to Call["Map", Length, {1, 2}]
        let body = Expr::Map {
            func: Box::new(Expr::Symbol("Length".to_string())),
            list: Box::new(Expr::List(vec![
                Expr::Integer(1.into()),
                Expr::Integer(2.into()),
            ])),
        };
        let bc = compile(vec![], body);
        assert!(bc.instructions.iter().any(|i| matches!(i, Instruction::Apply(..))),
            "Map desugaring should produce Apply instruction");
    }

    #[test]
    fn test_compile_pipe_desugar() {
        let body = Expr::Pipe {
            expr: Box::new(Expr::Integer(10.into())),
            func: Box::new(Expr::Symbol("f".to_string())),
        };
        let bc = compile(vec![], body);
        assert!(bc.instructions.iter().any(|i| matches!(i, Instruction::Apply(..))),
            "Pipe desugaring should produce Apply instruction");
    }

    #[test]
    fn test_compile_prefix_desugar() {
        let body = Expr::Prefix {
            func: Box::new(Expr::Symbol("f".to_string())),
            arg: Box::new(Expr::Integer(10.into())),
        };
        let bc = compile(vec![], body);
        assert!(bc.instructions.iter().any(|i| matches!(i, Instruction::Apply(..))),
            "Prefix desugaring should produce Apply instruction");
    }

    #[test]
    fn test_compile_apply_desugar() {
        // Apply @@ desugars to Call["Apply", ...]
        let body = Expr::Apply {
            func: Box::new(Expr::Symbol("f".to_string())),
            expr: Box::new(Expr::Symbol("x".to_string())),
        };
        let bc = compile(
            vec![Expr::NamedBlank {
                name: "x".to_string(),
                type_constraint: None,
            }],
            body,
        );
        assert!(
            bc.instructions
                .iter()
                .any(|i| matches!(i, Instruction::Apply(..))),
            "Apply desugaring should produce Apply instruction"
        );
    }

    #[test]
    fn test_compile_which() {
        // Which[x > 0, x, True, 0]
        let body = Expr::Which {
            pairs: vec![
                (
                    Expr::Call {
                        head: Box::new(Expr::Symbol("Greater".to_string())),
                        args: vec![Expr::Symbol("x".to_string()), Expr::Integer(0.into())],
                    },
                    Expr::Symbol("x".to_string()),
                ),
                (Expr::Bool(true), Expr::Integer(0.into())),
            ],
        };
        let bc = compile(
            vec![Expr::NamedBlank {
                name: "x".to_string(),
                type_constraint: None,
            }],
            body,
        );
        assert!(
            bc.instructions
                .iter()
                .any(|i| matches!(i, Instruction::JumpIfZero(..))),
            "Which should contain JumpIfZero"
        );
        assert!(
            bc.instructions
                .iter()
                .any(|i| matches!(i, Instruction::Jump(..))),
            "Which should contain Jump"
        );
    }

    #[test]
    fn test_compile_switch() {
        // Switch[x, 1, 10, 2, 20]
        let body = Expr::Switch {
            expr: Box::new(Expr::Symbol("x".to_string())),
            cases: vec![
                (Expr::Integer(1.into()), Expr::Integer(10.into())),
                (Expr::Integer(2.into()), Expr::Integer(20.into())),
            ],
        };
        let bc = compile(
            vec![Expr::NamedBlank {
                name: "x".to_string(),
                type_constraint: None,
            }],
            body,
        );
        assert!(
            bc.instructions
                .iter()
                .any(|i| matches!(i, Instruction::Eq(..))),
            "Switch should contain Eq"
        );
        assert!(
            bc.instructions
                .iter()
                .any(|i| matches!(i, Instruction::JumpIfZero(..))),
            "Switch should contain JumpIfZero"
        );
    }

    #[test]
    fn test_compile_release_hold() {
        // ReleaseHold[42] should compile to a constant
        let body = Expr::ReleaseHold(Box::new(Expr::Integer(42.into())));
        let bc = compile(vec![], body);
        assert_eq!(bc.constants.len(), 1);
        assert_eq!(bc.constants[0], Value::Integer(42.into()));
    }

    #[test]
    fn test_compile_complex() {
        let body = Expr::Complex { re: 1.0, im: 2.0 };
        let bc = compile(vec![], body);
        assert!(
            bc.constants
                .iter()
                .any(|v| matches!(v, Value::Complex { re, im } if *re == 1.0 && *im == 2.0)),
            "Complex constant should be in the constant pool"
        );
    }

    #[test]
    fn test_compile_blank_sequence() {
        // f[x__] := x -- should compile with MakeSeq instruction
        let params = vec![Expr::BlankSequence {
            name: Some("x".to_string()),
            type_constraint: None,
        }];
        let body = Expr::Symbol("x".to_string());
        let bc = BytecodeCompiler::compile_function(&params, &body, "f").unwrap();
        assert!(
            bc.instructions
                .iter()
                .any(|i| matches!(i, Instruction::MakeSeq(..))),
            "BlankSequence should emit MakeSeq instruction"
        );
    }
}
