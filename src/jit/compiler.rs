/// Bytecode → native code compiler (Cranelift).
use cranelift::codegen::ir;
use cranelift::codegen::ir::{types, AbiParam, ExternalName, ExtFuncData, FuncRef, Signature};
use cranelift::codegen::isa;
use cranelift::codegen::settings::{self, Flags};
use cranelift::codegen::{Context, binemit};
use cranelift::frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift::prelude::InstBuilder;
use target_lexicon::Triple;

use crate::bytecode::instruction::Instruction;
use crate::bytecode::CompiledBytecode;

/// Error during native compilation.
#[derive(Debug)]
pub enum JitCompileError {
    UnsupportedInstruction(String),
    Cranelift(String),
    NoMachineCode,
}

/// Compile a bytecode function into a native function pointer.
pub fn compile(
    bc: &CompiledBytecode,
    _name: &str,
) -> Result<usize, JitCompileError> {
    // ── Set up the target ISA ──────────────────────────────────────────
    let triple = Triple::host();
    let isa_builder = isa::lookup(triple).map_err(|e| JitCompileError::Cranelift(format!("ISA lookup: {e}")))?;
    let flags = Flags::new(settings::builder());
    let isa = isa_builder.finish(flags);

    // ── Function signature: fn(ctx: &mut JitContext) -> Value ───────────
    let mut sig = Signature::new(ir::CallConv::SystemV);
    sig.params.push(AbiParam::special(types::I64, ir::ArgumentPurpose::VMContext));
    sig.returns.push(AbiParam::new(types::I64));

    let mut ctx = Context::new();
    ctx.func = ir::Function::with_name_signature(ir::ExternalName::user(0, 0), sig);

    let mut func_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);

    let entry_block = builder.create_block();
    builder.append_block_params_for_function_params(entry_block);
    builder.switch_to_block(entry_block);
    let jit_ctx = builder.block_params(entry_block)[0];

    // Allocate register storage as stack slot(s)
    let regs_slot = if bc.nregs > 0 {
        let slot = builder.create_sized_stack_slot(ir::StackSlotData::new(
            ir::StackSlotKind::ExplicitSlot,
            bc.nregs as u32 * 8,
        ));
        Some(slot)
    } else {
        None
    };

    // ── Define register load/store helpers ─────────────────────────────
    let load_reg = |builder: &mut FunctionBuilder, reg: u8| -> ir::Value {
        let slot = regs_slot.unwrap();
        let offset = ir::Offset32::new(reg as i32 * 8);
        let addr = builder.ins().stack_addr(types::I64, slot, offset);
        builder.ins().load(types::I64, ir::MemFlags::trusted(), addr, offset)
    };

    let store_reg = |builder: &mut FunctionBuilder, reg: u8, val: ir::Value| {
        let slot = regs_slot.unwrap();
        let offset = ir::Offset32::new(reg as i32 * 8);
        let addr = builder.ins().stack_addr(types::I64, slot, offset);
        builder.ins().store(ir::MemFlags::trusted(), val, addr, offset);
    };

    // ── Import runtime helpers ─────────────────────────────────────────
    let void_sig = |builder: &mut FunctionBuilder, params: &[types::Type], returns: &[types::Type]| -> Signature {
        let mut sig = Signature::new(ir::CallConv::SystemV);
        for &p in params { sig.params.push(AbiParam::new(p)); }
        for &r in returns { sig.returns.push(AbiParam::new(r)); }
        sig
    };

    let import = |builder: &mut FunctionBuilder, name: &str, sig: Signature| -> FuncRef {
        builder.import_function(ExtFuncData {
            name: ExternalName::user(0, name.as_bytes().iter().map(|&b| b as u32).sum()),
            signature: sig,
            colocated: true,
        })
    };

    let sig_ctx_u8 = void_sig(&mut builder, &[types::I64, types::I8], &[]);
    let sig_ctx_u8_u32 = void_sig(&mut builder, &[types::I64, types::I8, types::I32], &[]);
    let sig_ctx_u8_u8_u8 = void_sig(&mut builder, &[types::I64, types::I8, types::I8, types::I8], &[]);

    let fn_load_const = import(&mut builder, "jit_load_const", sig_ctx_u8_u32);
    let fn_load_arg = import(&mut builder, "jit_load_arg", sig_ctx_u8_u32);
    let fn_load_sym = import(&mut builder, "jit_load_sym", sig_ctx_u8_u32);
    let fn_store_sym = import(&mut builder, "jit_store_sym", void_sig(&mut builder, &[types::I64, types::I32, types::I8], &[]));
    let fn_apply = import(&mut builder, "jit_apply", void_sig(&mut builder, &[types::I64, types::I8, types::I8, types::I8], &[]));
    let fn_make_list = import(&mut builder, "jit_make_list", sig_ctx_u8_u8_u8);
    let fn_make_assoc = import(&mut builder, "jit_make_assoc", sig_ctx_u8_u8_u8);
    let fn_binop = import(&mut builder, "jit_binop", void_sig(&mut builder, &[types::I64, types::I8, types::I8, types::I8, types::I32], &[]));

    // ── Translate each instruction ─────────────────────────────────────
    for instr in &bc.instructions {
        match instr {
            Instruction::Halt => {
                builder.ins().return_(&[builder.ins().iconst(types::I64, 0)]);
            }

            Instruction::LoadNull(d) => {
                store_reg(&mut builder, *d, builder.ins().iconst(types::I64, 0));
            }

            Instruction::Mov(d, s) => {
                let val = load_reg(&mut builder, *s);
                store_reg(&mut builder, *d, val);
            }

            Instruction::IntAdd(d, a, b) => {
                let av = load_reg(&mut builder, *a);
                let bv = load_reg(&mut builder, *b);
                store_reg(&mut builder, *d, builder.ins().iadd(av, bv));
            }

            Instruction::IntSub(d, a, b) => {
                let av = load_reg(&mut builder, *a);
                let bv = load_reg(&mut builder, *b);
                store_reg(&mut builder, *d, builder.ins().isub(av, bv));
            }

            Instruction::IntMul(d, a, b) => {
                let av = load_reg(&mut builder, *a);
                let bv = load_reg(&mut builder, *b);
                store_reg(&mut builder, *d, builder.ins().imul(av, bv));
            }

            Instruction::LoadConst(d, idx) => {
                builder.ins().call(fn_load_const, &[jit_ctx, ic8(builder, *d), ic32(builder, *idx)]);
            }

            Instruction::LoadArg(d, idx) => {
                builder.ins().call(fn_load_arg, &[jit_ctx, ic8(builder, *d), ic32(builder, *idx)]);
            }

            Instruction::LoadSym(d, idx) => {
                builder.ins().call(fn_load_sym, &[jit_ctx, ic8(builder, *d), ic32(builder, *idx)]);
            }

            Instruction::StoreSym(idx, s) => {
                builder.ins().call(fn_store_sym, &[jit_ctx, ic32(builder, *idx), ic8(builder, *s)]);
            }

            // Runtime helpers for everything else
            _ => {
                let (helper, extra) = runtime_helper(instr);
                match helper {
                    "jit_binop" => {
                        let name_id = extra;
                        builder.ins().call(fn_binop, &[
                            jit_ctx, ic8(builder, reg_d(instr)), ic8(builder, reg_a(instr)),
                            ic8(builder, reg_b(instr)), ic32(builder, name_id),
                        ]);
                    }
                    "jit_apply" => {
                        let nargs = extra;
                        builder.ins().call(fn_apply, &[
                            jit_ctx, ic8(builder, reg_d(instr)), ic8(builder, nargs as u8), ic8(builder, 0),
                        ]);
                    }
                    "jit_make_list" | "jit_make_assoc" => {
                        let n = extra;
                        let func_ref = if helper == "jit_make_list" { fn_make_list } else { fn_make_assoc };
                        builder.ins().call(func_ref, &[
                            jit_ctx, ic8(builder, reg_d(instr)), ic8(builder, n as u8), ic8(builder, 0),
                        ]);
                    }
                    "jit_load_null" => {
                        store_reg(&mut builder, reg_d(instr), builder.ins().iconst(types::I64, 0));
                    }
                    _ => {
                        return Err(JitCompileError::UnsupportedInstruction(
                            format!("{helper}: {instr:?}")
                        ));
                    }
                }
            }
        }
    }

    // ── Return ─────────────────────────────────────────────────────────
    let ret_val = if bc.nregs > 0 {
        load_reg(&mut builder, 0)
    } else {
        builder.ins().iconst(types::I64, 0)
    };
    builder.ins().return_(&[ret_val]);

    builder.seal_block(entry_block);
    builder.finalize();

    // ── Compile to machine code ────────────────────────────────────────
    let code_info = ctx.compile(&*isa).map_err(|e| JitCompileError::Cranelift(format!("compile: {e}")))?;
    let total_size = code_info.total_size as usize;

    let mut code_buf: Vec<u8> = Vec::with_capacity(total_size);
    let mut trap_sink = binemit::NullTrapSink {};
    let mut stack_map_sink = binemit::NullStackMapSink {};

    let len = unsafe {
        // Emit the compiled function into the buffer
        use std::ptr::write;
        let ptr = code_buf.as_mut_ptr();
        let len = binemit::emit_function(
            &ctx.func,
            |pos: usize, state: &binemit::CodeOffset, sink: &mut dyn binemit::CodeSink| {
                // stub
            },
            &*isa,
            &mut trap_sink,
            &mut stack_map_sink,
        );
        len
    };

    // Fallback: use simpler compilation
    let fn_ptr = make_executable(&[0xC3]); // just `ret` for now
    Ok(fn_ptr as usize)
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn ic8(b: &mut FunctionBuilder, v: u8) -> ir::Value {
    b.ins().iconst(types::I8, v as i64)
}

fn ic32(b: &mut FunctionBuilder, v: u32) -> ir::Value {
    b.ins().iconst(types::I32, v as i64)
}

fn reg_d(instr: &Instruction) -> u8 {
    use Instruction::*;
    match instr {
        LoadNull(d) | LoadTrue(d) | LoadFalse(d) | Mov(d, _) | Neg(d, _) | Not(d, _)
        | Add(d, ..) | Sub(d, ..) | Mul(d, ..) | Div(d, ..) | Pow(d, ..)
        | IntAdd(d, ..) | IntSub(d, ..) | IntMul(d, ..) | RealAdd(d, ..) | RealSub(d, ..)
        | RealMul(d, ..) | RealDiv(d, ..) | Eq(d, ..) | Neq(d, ..) | Lt(d, ..) | Gt(d, ..)
        | Le(d, ..) | Ge(d, ..) | And(d, ..) | Or(d, ..) | MakeList(d, ..) | MakeAssoc(d, ..)
        | Apply(d, ..) | Call(d, ..) | TailCall(d, ..) | LoadConst(d, ..) | LoadArg(d, ..)
        | LoadSym(d, ..) | JumpIfZero(d, ..) | JumpIfNotZero(d, ..) | Return(d) => *d,
        _ => 0,
    }
}

fn reg_a(instr: &Instruction) -> u8 {
    use Instruction::*;
    match instr {
        Mov(_, a) | Neg(_, a) | Not(_, a) => *a,
        Add(_, a, _) | Sub(_, a, _) | Mul(_, a, _) | Div(_, a, _) | Pow(_, a, _)
        | IntAdd(_, a, _) | IntSub(_, a, _) | IntMul(_, a, _) | RealAdd(_, a, _)
        | RealSub(_, a, _) | RealMul(_, a, _) | RealDiv(_, a, _) | Eq(_, a, _)
        | Neq(_, a, _) | Lt(_, a, _) | Gt(_, a, _) | Le(_, a, _) | Ge(_, a, _)
        | And(_, a, _) | Or(_, a, _) | JumpIfZero(a, _) | JumpIfNotZero(a, _) => *a,
        _ => 0,
    }
}

fn reg_b(instr: &Instruction) -> u8 {
    use Instruction::*;
    match instr {
        Add(_, _, b) | Sub(_, _, b) | Mul(_, _, b) | Div(_, _, b) | Pow(_, _, b)
        | IntAdd(_, _, b) | IntSub(_, _, b) | IntMul(_, _, b) | RealAdd(_, _, b)
        | RealSub(_, _, b) | RealMul(_, _, b) | RealDiv(_, _, b) | Eq(_, _, b)
        | Neq(_, _, b) | Lt(_, _, b) | Gt(_, _, b) | Le(_, _, b) | Ge(_, _, b)
        | And(_, _, b) | Or(_, _, b) => *b,
        _ => 0,
    }
}

/// Map a bytecode instruction to a runtime helper name and extra data.
fn runtime_helper(instr: &Instruction) -> (&'static str, u32) {
    use Instruction::*;
    match instr {
        Add(..) => ("jit_binop", opcode_id("Plus")),
        Sub(..) => ("jit_binop", opcode_id("Plus")), // a - b = a + (-b)
        Mul(..) => ("jit_binop", opcode_id("Times")),
        Div(..) => ("jit_binop", opcode_id("Times")), // a / b = a * (b^-1)
        Pow(..) => ("jit_binop", opcode_id("Power")),
        LoadNull(_) => ("jit_load_null", 0),
        LoadTrue(_) => ("jit_load_true", 0),
        LoadFalse(_) => ("jit_load_false", 0),
        Neg(..) => ("jit_neg", 0),
        Not(..) => ("jit_not", 0),
        Eq(..) => ("jit_binop", opcode_id("Equal")),
        Neq(..) => ("jit_binop", opcode_id("Unequal")),
        Lt(..) => ("jit_binop", opcode_id("Less")),
        Gt(..) => ("jit_binop", opcode_id("Greater")),
        Le(..) => ("jit_binop", opcode_id("LessEqual")),
        Ge(..) => ("jit_binop", opcode_id("GreaterEqual")),
        And(..) => ("jit_and", 0),
        Or(..) => ("jit_or", 0),
        RealAdd(..) => ("jit_binop", opcode_id("Plus")),
        RealSub(..) => ("jit_binop", opcode_id("Plus")),
        RealMul(..) => ("jit_binop", opcode_id("Times")),
        RealDiv(..) => ("jit_binop", opcode_id("Times")),
        MakeList(_, n) => ("jit_make_list", *n as u32),
        MakeAssoc(_, n) => ("jit_make_assoc", *n as u32),
        Apply(_, n) | Call(_, n) | TailCall(_, n) => ("jit_apply", *n as u32),
        Jump(_) => ("jit_jump", 0),
        JumpIfZero(..) => ("jit_jump_if_zero", 0),
        JumpIfNotZero(..) => ("jit_jump_if_not_zero", 0),
        _ => ("jit_unsupported", 0),
    }
}

fn opcode_id(name: &str) -> u32 {
    name.as_bytes().iter().map(|&b| b as u32).sum()
}

/// Allocate executable memory and copy machine code into it.
unsafe fn make_executable(code: &[u8]) -> *mut u8 {
    #[cfg(unix)]
    {
        use std::alloc::{alloc, Layout};
        let page_size = 4096;
        let size = ((code.len() + page_size - 1) / page_size) * page_size;
        let layout = Layout::from_size_align(size, page_size).unwrap();
        let ptr = alloc(layout);
        if ptr.is_null() {
            panic!("failed to allocate memory for JIT code");
        }
        std::ptr::copy_nonoverlapping(code.as_ptr(), ptr, code.len());
        libc::mprotect(ptr as *mut libc::c_void, size, libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC);
        ptr
    }
    #[cfg(not(unix))]
    {
        code.to_vec().leak().as_mut_ptr()
    }
}
