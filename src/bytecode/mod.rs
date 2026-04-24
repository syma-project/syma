/// Syma Bytecode VM (Phase 2 of the JIT pipeline).
///
/// Hot user-defined functions are compiled from AST to a register-based
/// bytecode instruction stream and executed by the VM in [`vm`].  The
/// bytecode is designed to feed into the Cranelift native compiler
/// (Phase 3, behind the `"jit"` feature flag).
///
pub mod compiler;
pub mod instruction;
pub mod vm;

use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, AtomicU64};

use crate::value::Value;

/// The type of a JIT-compiled function: `extern "C" fn(ctx: *mut JitContext)`.
pub type JitFnPtr = AtomicPtr<()>;

/// A function whose body has been compiled to Syma bytecode.
#[derive(Debug, Clone)]
pub struct BytecodeFunctionDef {
    /// The name of the function.
    pub name: String,
    /// The compiled bytecode body.
    pub bytecode: CompiledBytecode,
    /// How many times this function has been called
    /// (used for Phase 3 promotion).
    pub call_count: Arc<AtomicU64>,
    /// Pointer to JIT-compiled native code (null = not compiled yet).
    pub jit_fn_ptr: Arc<JitFnPtr>,
}

/// Compiled bytecode for a single function body.
#[derive(Debug, Clone)]
pub struct CompiledBytecode {
    /// Instructions in linear order.
    pub instructions: Vec<instruction::Instruction>,
    /// Constant pool — literal values referenced by `LoadConst`.
    pub constants: Vec<Value>,
    /// Number of virtual registers this function needs.
    pub nregs: u16,
    /// Number of parameters the function expects.
    pub nparams: u8,
}
