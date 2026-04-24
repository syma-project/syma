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
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::UnsupportedFeature(msg) => {
                write!(f, "unsupported feature: {msg}")
            }
        }
    }
}

impl std::error::Error for CompileError {}

// ── Register Allocator ─────────────────────────────────────────────────────────

struct RegAlloc {
    next: u8,
    _freed: Vec<u8>,
}

impl RegAlloc {
    fn new() -> Self {
        Self {
            next: 0,
            _freed: Vec::new(),
        }
    }

    fn alloc(&mut self) -> u8 {
        if let Some(r) = self._freed.pop() {
            return r;
        }
        let r = self.next;
        self.next += 1;
        r
    }

    fn _free(&mut self, reg: u8) {
        self._freed.push(reg);
    }

    fn used(&self) -> u16 {
        self.next as u16 - self._freed.len() as u16
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
            let reg = c.regs.alloc();
            c.emit(Instruction::LoadArg(reg, i as u32));

            match param {
                // Simple blanks always match
                Expr::NamedBlank { .. } | Expr::Blank { .. } => {}
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

    fn compile_expr(&mut self, expr: &Expr) -> Result<u8, CompileError> {
        match expr {
            Expr::Integer(n) => Ok(self.emit_const(Value::Integer(n.clone()))),
            Expr::Real(r) => Ok(self.emit_const(Value::Real(r.clone()))),
            Expr::Str(s) => Ok(self.emit_const(Value::Str(s.clone()))),
            Expr::Bool(b) => Ok(self.emit_const(Value::Bool(*b))),
            Expr::Null => {
                let dst = self.regs.alloc();
                self.emit(Instruction::LoadNull(dst));
                Ok(dst)
            }
            Expr::Symbol(s) => {
                let dst = self.regs.alloc();
                let idx = self.symbol_idx(s);
                self.emit(Instruction::LoadSym(dst, idx));
                Ok(dst)
            }
            Expr::List(items) => {
                let mut regs = Vec::with_capacity(items.len());
                for item in items {
                    regs.push(self.compile_expr(item)?);
                }
                let dst = self.regs.alloc();
                self.emit(Instruction::MakeList(dst, regs.len() as u8));
                for r in regs {
                    self.regs._free(r);
                }
                Ok(dst)
            }
            Expr::Call { head, args } => self.compile_call(head, args),
            Expr::If { condition, then_branch, else_branch } => {
                self.compile_if(condition, then_branch, else_branch)
            }
            Expr::While { condition, body } => self.compile_while(condition, body),
            Expr::Sequence(stmts) => self.compile_sequence(stmts),
            _ => Err(CompileError::UnsupportedFeature(format!(
                "expression: {expr:?}"
            ))),
        }
    }

    fn compile_call(&mut self, head: &Expr, args: &[Expr]) -> Result<u8, CompileError> {
        let func_reg = self.compile_expr(head)?;
        let mut arg_regs = Vec::with_capacity(args.len());
        for arg in args {
            arg_regs.push(self.compile_expr(arg)?);
        }
        let dst = self.regs.alloc();
        self.emit(Instruction::Apply(dst, args.len() as u8));
        self.regs._free(func_reg);
        for r in arg_regs {
            self.regs._free(r);
        }
        Ok(dst)
    }

    fn compile_if(
        &mut self,
        condition: &Expr,
        then_branch: &Expr,
        else_branch: &Option<Box<Expr>>,
    ) -> Result<u8, CompileError> {
        let result_reg = self.regs.alloc();
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

    fn compile_while(&mut self, condition: &Expr, body: &Expr) -> Result<u8, CompileError> {
        let loop_start = self.code.len() as i32;
        let cond_reg = self.compile_expr(condition)?;
        let exit_jz = self.emit(Instruction::JumpIfZero(cond_reg, 0));
        self.regs._free(cond_reg);

        self.compile_expr(body)?;
        let jump_back = self.code.len();
        self.emit(Instruction::Jump(loop_start - jump_back as i32 - 1));

        self.patch_jump(exit_jz, self.code.len() as i32);
        let dst = self.regs.alloc();
        self.emit(Instruction::LoadNull(dst));
        Ok(dst)
    }

    fn compile_sequence(&mut self, stmts: &[Expr]) -> Result<u8, CompileError> {
        let mut last_reg = self.regs.alloc();
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
        // Safe: we're matching on a mutable reference to an Instruction
        // and replacing the offset field.
        let instr = &mut self.code[instr_idx];
        match instr {
            Instruction::Jump(o)
            | Instruction::JumpIfZero(_, o)
            | Instruction::JumpIfNotZero(_, o) => *o = target,
            _ => panic!("Cannot patch non-jump at {instr_idx}"),
        }
    }

    fn emit_const(&mut self, val: Value) -> u8 {
        let dst = self.regs.alloc();
        let idx = self.pool.insert(val);
        self.emit(Instruction::LoadConst(dst, idx));
        dst
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

    #[test]
    fn test_compile_identity() {
        let params = vec![Expr::NamedBlank {
            name: "x".to_string(),
            type_constraint: None,
        }];
        let body = Expr::Symbol("x".to_string());
        let bc = BytecodeCompiler::compile_function(&params, &body, "f").unwrap();
        assert_eq!(bc.nparams, 1);
        assert!(bc.nregs > 0);
    }

    #[test]
    fn test_compile_constant() {
        let params = vec![];
        let body = Expr::Integer(42.into());
        let bc = BytecodeCompiler::compile_function(&params, &body, "f").unwrap();
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
        let bc = BytecodeCompiler::compile_function(&params, &body, "f").unwrap();
        assert_eq!(bc.nparams, 2);
    }
}
