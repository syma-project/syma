/// Bytecode → native code compiler (Cranelift).
///
/// Translates Syma bytecode to native machine code via the Cranelift
/// code generator. All operations are delegated to `extern "C"` runtime
/// helpers; the JIT wins by eliminating interpreter dispatch overhead.
use std::collections::{BTreeSet, HashMap};

use cranelift::codegen::binemit::Reloc;
use cranelift::codegen::control::ControlPlane;
use cranelift::codegen::entity::EntityRef;
use cranelift::codegen::ir;
use cranelift::codegen::ir::condcodes::IntCC;
use cranelift::codegen::ir::{
    AbiParam, Block, ExtFuncData, ExternalName, FuncRef, Signature, UserExternalNameRef,
    UserFuncName, types,
};
use cranelift::codegen::isa;
use cranelift::codegen::settings::{self, Flags};
use cranelift::codegen::{Context, FinalizedMachReloc, FinalizedRelocTarget};
use cranelift::frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift::prelude::InstBuilder;
use target_lexicon::Triple;

use crate::bytecode::CompiledBytecode;
use crate::bytecode::instruction::Instruction;
use crate::jit::runtime::{
    JIT_OP_EQUAL, JIT_OP_GREATER, JIT_OP_GREATEREQUAL, JIT_OP_LESS, JIT_OP_LESSEQUAL, JIT_OP_PLUS,
    JIT_OP_POWER, JIT_OP_TIMES, JIT_OP_UNEQUAL,
};

/// Error during native compilation.
#[derive(Debug)]
pub enum JitCompileError {
    UnsupportedInstruction(String),
    Cranelift(String),
    NoMachineCode,
}

/// A set of imported `extern "C"` runtime helper function references.
struct JitHelpers {
    load_const: FuncRef,
    load_arg: FuncRef,
    load_true: FuncRef,
    load_false: FuncRef,
    load_null: FuncRef,
    mov: FuncRef,
    neg: FuncRef,
    not_: FuncRef,
    and_: FuncRef,
    or_: FuncRef,
    binop: FuncRef,
    sub_: FuncRef,
    div_: FuncRef,
    make_list: FuncRef,
    make_assoc: FuncRef,
    apply: FuncRef,
    make_seq: FuncRef,
    load_sym: FuncRef,
    store_sym: FuncRef,
    is_truthy: FuncRef,
}

impl JitHelpers {
    /// Import all runtime helpers into the Cranelift function.
    fn new(builder: &mut FunctionBuilder) -> Self {
        // Helper: create a void SystemV signature.
        let void_sig = |params: &[types::Type]| -> Signature {
            let mut sig = Signature::new(isa::CallConv::SystemV);
            for &p in params {
                sig.params.push(AbiParam::new(p));
            }
            sig
        };

        // Helper: create a signature with a return value.
        let ret_sig = |params: &[types::Type], ret: types::Type| -> Signature {
            let mut sig = Signature::new(isa::CallConv::SystemV);
            for &p in params {
                sig.params.push(AbiParam::new(p));
            }
            sig.returns.push(AbiParam::new(ret));
            sig
        };

        // Helper: import a function into the Cranelift IR.
        let import = |builder: &mut FunctionBuilder, name: &str, sig: Signature| -> FuncRef {
            let sig_ref = builder.import_signature(sig);
            builder.import_function(ExtFuncData {
                name: ExternalName::user(UserExternalNameRef::new(
                    name.as_bytes().iter().map(|&b| b as usize).sum(),
                )),
                signature: sig_ref,
                colocated: true,
            })
        };

        Self {
            load_const: import(
                builder,
                "jit_load_const",
                void_sig(&[types::I64, types::I32, types::I32]),
            ),
            load_arg: import(
                builder,
                "jit_load_arg",
                void_sig(&[types::I64, types::I32, types::I32]),
            ),
            load_true: import(
                builder,
                "jit_load_true",
                void_sig(&[types::I64, types::I32]),
            ),
            load_false: import(
                builder,
                "jit_load_false",
                void_sig(&[types::I64, types::I32]),
            ),
            load_null: import(
                builder,
                "jit_load_null",
                void_sig(&[types::I64, types::I32]),
            ),
            mov: import(
                builder,
                "jit_mov",
                void_sig(&[types::I64, types::I32, types::I32]),
            ),
            neg: import(
                builder,
                "jit_neg",
                void_sig(&[types::I64, types::I32, types::I32]),
            ),
            not_: import(
                builder,
                "jit_not",
                void_sig(&[types::I64, types::I32, types::I32]),
            ),
            and_: import(
                builder,
                "jit_and",
                void_sig(&[types::I64, types::I32, types::I32, types::I32]),
            ),
            or_: import(
                builder,
                "jit_or",
                void_sig(&[types::I64, types::I32, types::I32, types::I32]),
            ),
            binop: import(
                builder,
                "jit_binop",
                void_sig(&[types::I64, types::I32, types::I32, types::I32, types::I32]),
            ),
            sub_: import(
                builder,
                "jit_sub",
                void_sig(&[types::I64, types::I32, types::I32, types::I32]),
            ),
            div_: import(
                builder,
                "jit_div",
                void_sig(&[types::I64, types::I32, types::I32, types::I32]),
            ),
            make_list: import(
                builder,
                "jit_make_list",
                void_sig(&[types::I64, types::I32, types::I32]),
            ),
            make_assoc: import(
                builder,
                "jit_make_assoc",
                void_sig(&[types::I64, types::I32, types::I32]),
            ),
            apply: import(
                builder,
                "jit_apply",
                void_sig(&[types::I64, types::I32, types::I32]),
            ),
            make_seq: import(
                builder,
                "jit_make_seq",
                void_sig(&[types::I64, types::I32, types::I32]),
            ),
            load_sym: import(
                builder,
                "jit_load_sym",
                void_sig(&[types::I64, types::I32, types::I32]),
            ),
            store_sym: import(
                builder,
                "jit_store_sym",
                void_sig(&[types::I64, types::I32, types::I32]),
            ),
            is_truthy: import(
                builder,
                "jit_is_truthy",
                ret_sig(&[types::I64, types::I32], types::I8),
            ),
        }
    }
}

/// Compile bytecode into a native function pointer.
///
/// Returns a pointer to executable machine code (cast to `*mut ()`).
pub fn compile(bc: &CompiledBytecode, _name: &str) -> Result<*mut (), JitCompileError> {
    // ── Set up target ISA ──────────────────────────────────────────
    let triple = Triple::host();
    let isa = isa::lookup(triple)
        .map_err(|e| JitCompileError::Cranelift(format!("ISA lookup: {e:?}")))?
        .finish(Flags::new(settings::builder()))
        .map_err(|e| JitCompileError::Cranelift(format!("ISA finish: {e:?}")))?;

    // ── JIT function signature: fn(ctx: *mut JitContext) -> () ─────
    let mut sig = Signature::new(isa::CallConv::SystemV);
    sig.params.push(AbiParam::new(types::I64));

    let mut ctx = Context::new();
    ctx.func = ir::Function::with_name_signature(UserFuncName::user(0, 0), sig);

    let mut func_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);

    // ── Pre-scan for block boundaries ──────────────────────────────
    let block_starts = find_block_starts(&bc.instructions);
    let block_starts_set: BTreeSet<usize> = block_starts.iter().copied().collect();
    let block_starts_sorted: Vec<usize> = block_starts_set.iter().copied().collect();

    // ── Create Cranelift blocks ────────────────────────────────────
    let mut block_map: HashMap<usize, Block> = HashMap::new();
    for &start in &block_starts_sorted {
        let block = builder.create_block();
        block_map.insert(start, block);
        // Every block has one I64 block parameter (the ctx pointer).
        // For the entry block this comes from the function signature;
        // for others we must add it explicitly.
        builder.append_block_param(block, types::I64);
    }

    // ── Import runtime helpers ─────────────────────────────────────
    let helpers = JitHelpers::new(&mut builder);

    // ── Process instructions linearly ──────────────────────────────
    let entry_block = block_map[&0];
    builder.switch_to_block(entry_block);
    let mut current_block = entry_block;
    let mut ctx_arg = builder.block_params(current_block)[0];
    let mut terminated = false; // true → current block has a terminator

    let mut i = 0;
    while i < bc.instructions.len() {
        // Handle block switching.
        if i > 0 && block_starts_set.contains(&i) {
            builder.seal_block(current_block);
            current_block = block_map[&i];
            builder.switch_to_block(current_block);
            ctx_arg = builder.block_params(current_block)[0];
            terminated = false;
        }

        let instr = &bc.instructions[i];

        // ── Terminator instructions (end the current block) ────
        match instr {
            Instruction::Halt => {
                let d0 = i32_val(&mut builder, 0);
                builder.ins().call(helpers.load_null, &[ctx_arg, d0]);
                builder.ins().return_(&[]);
                terminated = true;
                i += 1;
                // Skip any dead code after this terminator.
                while i < bc.instructions.len() && !block_starts_set.contains(&i) {
                    i += 1;
                }
                continue;
            }

            Instruction::Return(reg) => {
                if *reg != 0 {
                    let d0 = i32_val(&mut builder, 0);
                    let src = i32_val(&mut builder, *reg as u32);
                    builder.ins().call(helpers.mov, &[ctx_arg, d0, src]);
                }
                builder.ins().return_(&[]);
                terminated = true;
                i += 1;
                while i < bc.instructions.len() && !block_starts_set.contains(&i) {
                    i += 1;
                }
                continue;
            }

            Instruction::Jump(offset) => {
                let target = jump_target(i, *offset);
                let target_block = block_map[&target];
                builder.ins().jump(target_block, &[ctx_arg]);
                terminated = true;
                i += 1;
                while i < bc.instructions.len() && !block_starts_set.contains(&i) {
                    i += 1;
                }
                continue;
            }

            Instruction::JumpIfZero(reg, offset) => {
                let target = jump_target(i, *offset);
                let target_block = block_map[&target];
                let fallthrough = block_map[&(i + 1)];
                let reg_val = i32_val(&mut builder, *reg as u32);
                let call_inst = builder.ins().call(helpers.is_truthy, &[ctx_arg, reg_val]);
                let truthy = builder.inst_results(call_inst)[0];
                let zero = builder.ins().iconst(types::I8, 0);
                let cond = builder.ins().icmp(IntCC::Equal, truthy, zero);
                builder
                    .ins()
                    .brif(cond, target_block, &[ctx_arg], fallthrough, &[ctx_arg]);
                terminated = true;
                i += 1;
                continue;
            }

            Instruction::JumpIfNotZero(reg, offset) => {
                let target = jump_target(i, *offset);
                let target_block = block_map[&target];
                let fallthrough = block_map[&(i + 1)];
                let reg_val = i32_val(&mut builder, *reg as u32);
                let call_inst = builder.ins().call(helpers.is_truthy, &[ctx_arg, reg_val]);
                let truthy = builder.inst_results(call_inst)[0];
                let zero = builder.ins().iconst(types::I8, 0);
                let cond = builder.ins().icmp(IntCC::NotEqual, truthy, zero);
                builder
                    .ins()
                    .brif(cond, target_block, &[ctx_arg], fallthrough, &[ctx_arg]);
                terminated = true;
                i += 1;
                continue;
            }

            _ => {}
        }

        // ── Non-terminator instructions (accumulate values) ───
        // Each instruction calls the appropriate runtime helper.
        emit_op(&mut builder, instr, ctx_arg, &helpers);
        i += 1;
    }

    // If the last block was not terminated, add an implicit return.
    if !terminated {
        builder.ins().return_(&[]);
    }

    // ── Seal all blocks and finalize ───────────────────────────────
    for block in block_map.values() {
        builder.seal_block(*block);
    }
    builder.finalize();

    // ── Compile to machine code ────────────────────────────────────
    let mut ctrl_plane = ControlPlane::default();
    let compiled = ctx
        .compile(&*isa, &mut ctrl_plane)
        .map_err(|e| JitCompileError::Cranelift(format!("compile: {e:?}")))?;

    let code_bytes = compiled.code_buffer();
    if code_bytes.is_empty() {
        return Err(JitCompileError::NoMachineCode);
    }

    // ── Allocate executable memory and copy code ───────────────────
    let code_base = unsafe { make_executable(code_bytes) };
    if code_base.is_null() {
        return Err(JitCompileError::NoMachineCode);
    }

    // ── Apply relocations ──────────────────────────────────────────
    // Build a map from ExternalName user-index → helper name (for resolution).
    // The imported functions use ExternalName::user(0, hash) where hash
    // is the sum of the name's byte values. We need to map these back
    // to actual function pointers.
    let mut name_to_fn: HashMap<usize, usize> = HashMap::new();
    for &(name, addr) in &helper_addresses() {
        let hash = name.as_bytes().iter().map(|&b| b as usize).sum();
        name_to_fn.insert(hash, addr);
    }

    apply_relocations(
        unsafe { std::slice::from_raw_parts_mut(code_base, code_bytes.len()) },
        code_base as usize,
        compiled.buffer.relocs(),
        &name_to_fn,
    );

    Ok(code_base as *mut ())
}

/// Build a map of runtime helper names to their function addresses.
fn helper_addresses() -> Vec<(&'static str, usize)> {
    // We use direct as-casting: extern "C" fn pointer → usize.
    // This works because all extern "C" function pointers have the same
    // representation regardless of parameter count.
    macro_rules! fn_addr {
        ($f:expr) => {
            $f as *const () as usize
        };
    }
    vec![
        (
            "jit_load_const",
            fn_addr!(crate::jit::runtime::jit_load_const),
        ),
        ("jit_load_arg", fn_addr!(crate::jit::runtime::jit_load_arg)),
        (
            "jit_load_true",
            fn_addr!(crate::jit::runtime::jit_load_true),
        ),
        (
            "jit_load_false",
            fn_addr!(crate::jit::runtime::jit_load_false),
        ),
        (
            "jit_load_null",
            fn_addr!(crate::jit::runtime::jit_load_null),
        ),
        ("jit_mov", fn_addr!(crate::jit::runtime::jit_mov)),
        ("jit_neg", fn_addr!(crate::jit::runtime::jit_neg)),
        ("jit_not", fn_addr!(crate::jit::runtime::jit_not)),
        ("jit_and", fn_addr!(crate::jit::runtime::jit_and)),
        ("jit_or", fn_addr!(crate::jit::runtime::jit_or)),
        ("jit_binop", fn_addr!(crate::jit::runtime::jit_binop)),
        ("jit_sub", fn_addr!(crate::jit::runtime::jit_sub)),
        ("jit_div", fn_addr!(crate::jit::runtime::jit_div)),
        (
            "jit_make_list",
            fn_addr!(crate::jit::runtime::jit_make_list),
        ),
        (
            "jit_make_assoc",
            fn_addr!(crate::jit::runtime::jit_make_assoc),
        ),
        ("jit_apply", fn_addr!(crate::jit::runtime::jit_apply)),
        ("jit_make_seq", fn_addr!(crate::jit::runtime::jit_make_seq)),
        ("jit_load_sym", fn_addr!(crate::jit::runtime::jit_load_sym)),
        (
            "jit_store_sym",
            fn_addr!(crate::jit::runtime::jit_store_sym),
        ),
        (
            "jit_is_truthy",
            fn_addr!(crate::jit::runtime::jit_is_truthy),
        ),
    ]
}

// ── Pre-scan ──────────────────────────────────────────────────────────────

/// Find all instruction indices that start a basic block.
fn find_block_starts(instructions: &[Instruction]) -> Vec<usize> {
    let mut starts: BTreeSet<usize> = BTreeSet::new();
    starts.insert(0); // entry block

    for (i, instr) in instructions.iter().enumerate() {
        match instr {
            Instruction::Jump(offset) => {
                let target = jump_target(i, *offset);
                starts.insert(target);
            }
            Instruction::JumpIfZero(_, offset) | Instruction::JumpIfNotZero(_, offset) => {
                let target = jump_target(i, *offset);
                starts.insert(target);
                if i + 1 < instructions.len() {
                    starts.insert(i + 1); // fall-through
                }
            }
            Instruction::Halt | Instruction::Return(_) => {
                // No fall-through after these.
            }
            _ => {}
        }
    }

    starts.into_iter().collect()
}

/// Compute the absolute instruction index from a relative jump offset.
///
/// The VM stores `offset = target - instr_idx - 1`, so
/// `target = instr_idx + 1 + offset`.
fn jump_target(instr_idx: usize, offset: i32) -> usize {
    ((instr_idx as isize) + 1 + (offset as isize)) as usize
}

// ── Instruction emission ──────────────────────────────────────────────────

/// Emit a non-terminator instruction as a call to the appropriate runtime helper.
fn emit_op(builder: &mut FunctionBuilder, instr: &Instruction, ctx: ir::Value, h: &JitHelpers) {
    match instr {
        Instruction::LoadNull(d) => {
            let d_v = i32_val(builder, *d as u32);
            builder.ins().call(h.load_null, &[ctx, d_v]);
        }
        Instruction::LoadTrue(d) => {
            let d_v = i32_val(builder, *d as u32);
            builder.ins().call(h.load_true, &[ctx, d_v]);
        }
        Instruction::LoadFalse(d) => {
            let d_v = i32_val(builder, *d as u32);
            builder.ins().call(h.load_false, &[ctx, d_v]);
        }
        Instruction::Mov(d, s) => {
            let d_v = i32_val(builder, *d as u32);
            let s_v = i32_val(builder, *s as u32);
            builder.ins().call(h.mov, &[ctx, d_v, s_v]);
        }
        Instruction::Neg(d, s) => {
            let d_v = i32_val(builder, *d as u32);
            let s_v = i32_val(builder, *s as u32);
            builder.ins().call(h.neg, &[ctx, d_v, s_v]);
        }
        Instruction::Not(d, s) => {
            let d_v = i32_val(builder, *d as u32);
            let s_v = i32_val(builder, *s as u32);
            builder.ins().call(h.not_, &[ctx, d_v, s_v]);
        }

        // Generic arithmetic (full Value operations)
        Instruction::Add(d, a, b) => binop(builder, ctx, h, *d, *a, *b, JIT_OP_PLUS),
        Instruction::Sub(d, a, b) => {
            let d_v = i32_val(builder, *d as u32);
            let a_v = i32_val(builder, *a as u32);
            let b_v = i32_val(builder, *b as u32);
            builder.ins().call(h.sub_, &[ctx, d_v, a_v, b_v]);
        }
        Instruction::Mul(d, a, b) => binop(builder, ctx, h, *d, *a, *b, JIT_OP_TIMES),
        Instruction::Div(d, a, b) => {
            let d_v = i32_val(builder, *d as u32);
            let a_v = i32_val(builder, *a as u32);
            let b_v = i32_val(builder, *b as u32);
            builder.ins().call(h.div_, &[ctx, d_v, a_v, b_v]);
        }
        Instruction::Pow(d, a, b) => binop(builder, ctx, h, *d, *a, *b, JIT_OP_POWER),

        // Type-specialized arithmetic
        Instruction::IntAdd(d, a, b) => binop(builder, ctx, h, *d, *a, *b, JIT_OP_PLUS),
        Instruction::IntSub(d, a, b) => {
            let d_v = i32_val(builder, *d as u32);
            let a_v = i32_val(builder, *a as u32);
            let b_v = i32_val(builder, *b as u32);
            builder.ins().call(h.sub_, &[ctx, d_v, a_v, b_v]);
        }
        Instruction::IntMul(d, a, b) => binop(builder, ctx, h, *d, *a, *b, JIT_OP_TIMES),
        Instruction::RealAdd(d, a, b) => binop(builder, ctx, h, *d, *a, *b, JIT_OP_PLUS),
        Instruction::RealSub(d, a, b) => {
            let d_v = i32_val(builder, *d as u32);
            let a_v = i32_val(builder, *a as u32);
            let b_v = i32_val(builder, *b as u32);
            builder.ins().call(h.sub_, &[ctx, d_v, a_v, b_v]);
        }
        Instruction::RealMul(d, a, b) => binop(builder, ctx, h, *d, *a, *b, JIT_OP_TIMES),
        Instruction::RealDiv(d, a, b) => {
            let d_v = i32_val(builder, *d as u32);
            let a_v = i32_val(builder, *a as u32);
            let b_v = i32_val(builder, *b as u32);
            builder.ins().call(h.div_, &[ctx, d_v, a_v, b_v]);
        }

        // Comparisons
        Instruction::Eq(d, a, b) => binop(builder, ctx, h, *d, *a, *b, JIT_OP_EQUAL),
        Instruction::Neq(d, a, b) => binop(builder, ctx, h, *d, *a, *b, JIT_OP_UNEQUAL),
        Instruction::Lt(d, a, b) => binop(builder, ctx, h, *d, *a, *b, JIT_OP_LESS),
        Instruction::Gt(d, a, b) => binop(builder, ctx, h, *d, *a, *b, JIT_OP_GREATER),
        Instruction::Le(d, a, b) => binop(builder, ctx, h, *d, *a, *b, JIT_OP_LESSEQUAL),
        Instruction::Ge(d, a, b) => binop(builder, ctx, h, *d, *a, *b, JIT_OP_GREATEREQUAL),

        // Logical
        Instruction::And(d, a, b) => {
            let d_v = i32_val(builder, *d as u32);
            let a_v = i32_val(builder, *a as u32);
            let b_v = i32_val(builder, *b as u32);
            builder.ins().call(h.and_, &[ctx, d_v, a_v, b_v]);
        }
        Instruction::Or(d, a, b) => {
            let d_v = i32_val(builder, *d as u32);
            let a_v = i32_val(builder, *a as u32);
            let b_v = i32_val(builder, *b as u32);
            builder.ins().call(h.or_, &[ctx, d_v, a_v, b_v]);
        }

        // Collections
        Instruction::MakeList(d, n) => {
            let d_v = i32_val(builder, *d as u32);
            let n_v = i32_val(builder, *n as u32);
            builder.ins().call(h.make_list, &[ctx, d_v, n_v]);
        }
        Instruction::MakeAssoc(d, n) => {
            let d_v = i32_val(builder, *d as u32);
            let n_v = i32_val(builder, *n as u32);
            builder.ins().call(h.make_assoc, &[ctx, d_v, n_v]);
        }
        Instruction::MakeSeq(d, start) => {
            let d_v = i32_val(builder, *d as u32);
            let s_v = i32_val(builder, *start as u32);
            builder.ins().call(h.make_seq, &[ctx, d_v, s_v]);
        }

        // Function application
        Instruction::Apply(d, n) | Instruction::Call(d, n) | Instruction::TailCall(d, n) => {
            let d_v = i32_val(builder, *d as u32);
            let n_v = i32_val(builder, *n as u32);
            builder.ins().call(h.apply, &[ctx, d_v, n_v]);
        }

        // Constants / args / symbols
        Instruction::LoadConst(d, idx) => {
            let d_v = i32_val(builder, *d as u32);
            let idx_v = i32_val(builder, *idx);
            builder.ins().call(h.load_const, &[ctx, d_v, idx_v]);
        }
        Instruction::LoadArg(d, idx) => {
            let d_v = i32_val(builder, *d as u32);
            let idx_v = i32_val(builder, *idx);
            builder.ins().call(h.load_arg, &[ctx, d_v, idx_v]);
        }
        Instruction::LoadSym(d, idx) => {
            let d_v = i32_val(builder, *d as u32);
            let idx_v = i32_val(builder, *idx);
            builder.ins().call(h.load_sym, &[ctx, d_v, idx_v]);
        }
        Instruction::StoreSym(idx, s) => {
            let idx_v = i32_val(builder, *idx);
            let s_v = i32_val(builder, *s as u32);
            builder.ins().call(h.store_sym, &[ctx, idx_v, s_v]);
        }

        // Halt / Return / Jump / JumpIf* are terminators — handled in the loop.
        Instruction::Halt
        | Instruction::Return(_)
        | Instruction::Jump(_)
        | Instruction::JumpIfZero(..)
        | Instruction::JumpIfNotZero(..) => {
            // Should not be reached — these are handled as terminators.
        }
    }
}

/// Emit a call to `jit_binop` with the given opcode.
fn binop(
    builder: &mut FunctionBuilder,
    ctx: ir::Value,
    h: &JitHelpers,
    d: u16,
    a: u16,
    b: u16,
    op: u32,
) {
    let d_v = i32_val(builder, d as u32);
    let a_v = i32_val(builder, a as u32);
    let b_v = i32_val(builder, b as u32);
    let op_v = i32_val(builder, op);
    builder.ins().call(h.binop, &[ctx, d_v, a_v, b_v, op_v]);
}

/// Create an I32 constant value in Cranelift IR.
fn i32_val(builder: &mut FunctionBuilder, v: u32) -> ir::Value {
    builder.ins().iconst(types::I32, v as i64)
}

// ── Relocation ────────────────────────────────────────────────────────────

/// Apply relocations to the emitted machine code.
///
/// Each relocation is a reference to an external symbol (a runtime helper).
/// We resolve the symbol to its actual address and patch the code.
fn apply_relocations(
    code: &mut [u8],
    code_base: usize,
    relocs: &[FinalizedMachReloc],
    name_map: &HashMap<usize, usize>,
) {
    for reloc in relocs {
        let target_addr = match &reloc.target {
            FinalizedRelocTarget::ExternalName(ExternalName::User(index)) => {
                match name_map.get(&index.index()) {
                    Some(&addr) => addr,
                    None => continue,
                }
            }
            _ => continue,
        };

        let offset = reloc.offset as usize;
        match reloc.kind {
            Reloc::X86CallPCRel4 | Reloc::X86PCRel4 => {
                // PC-relative: delta = target - (code_base + offset + 4) + addend
                let delta =
                    (target_addr as i64) - (code_base as i64 + offset as i64 + 4) + reloc.addend;
                if offset + 4 <= code.len() {
                    code[offset..offset + 4].copy_from_slice(&(delta as i32).to_le_bytes());
                }
            }
            Reloc::Abs4 if offset + 4 <= code.len() => {
                code[offset..offset + 4].copy_from_slice(
                    &((target_addr as u32).wrapping_add(reloc.addend as u32)).to_le_bytes(),
                );
            }
            Reloc::Abs8 if offset + 8 <= code.len() => {
                code[offset..offset + 8].copy_from_slice(
                    &(target_addr.wrapping_add(reloc.addend as usize)).to_le_bytes(),
                );
            }
            _ => {
                // Unsupported relocation kind — skip.
            }
        }
    }
}

// ── Executable memory ─────────────────────────────────────────────────────

/// Allocate executable memory and copy code into it.
///
/// Returns a pointer to the executable code, or null on failure.
unsafe fn make_executable(code: &[u8]) -> *mut u8 {
    #[cfg(unix)]
    {
        use std::alloc::{Layout, alloc};
        let page_size = 4096;
        let size = code.len().div_ceil(page_size) * page_size;
        let layout = Layout::from_size_align(size, page_size).unwrap();
        // SAFETY: layout is page-aligned and sized correctly.
        let ptr = unsafe { alloc(layout) };
        if ptr.is_null() {
            return std::ptr::null_mut();
        }
        // SAFETY: ptr points to at least code.len() bytes of allocated memory,
        // and the source/destination don't overlap.
        unsafe {
            std::ptr::copy_nonoverlapping(code.as_ptr(), ptr, code.len());
        }
        // SAFETY: ptr is page-aligned, size is a multiple of page_size,
        // and the memory was just allocated so making it executable is safe.
        unsafe {
            libc::mprotect(
                ptr as *mut libc::c_void,
                size,
                libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
            );
        }
        ptr
    }
    #[cfg(not(unix))]
    {
        code.to_vec().leak().as_mut_ptr()
    }
}
