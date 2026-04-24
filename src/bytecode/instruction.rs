/// Bytecode instruction set for the Syma VM.
///
/// Each `Instruction` is a typed enum that the compiler emits and the
/// VM dispatches on.
///
/// Discriminant for every valid instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    Halt = 0x00,
    LoadNull = 0x10,
    LoadTrue = 0x11,
    LoadFalse = 0x12,
    Mov = 0x20,
    Neg = 0x21,
    Not = 0x22,
    Add = 0x30,
    Sub = 0x31,
    Mul = 0x32,
    Div = 0x33,
    Pow = 0x34,
    IntAdd = 0x35,
    IntSub = 0x36,
    IntMul = 0x37,
    RealAdd = 0x38,
    RealSub = 0x39,
    RealMul = 0x3A,
    RealDiv = 0x3B,
    Eq = 0x3C,
    Neq = 0x3D,
    Lt = 0x3E,
    Gt = 0x3F,
    Le = 0x40,
    Ge = 0x41,
    And = 0x42,
    Or = 0x43,
    MakeList = 0x50,
    MakeAssoc = 0x51,
    Apply = 0x52,
    Call = 0x53,
    TailCall = 0x54,
    LoadConst = 0x60,
    LoadArg = 0x61,
    LoadSym = 0x62,
    StoreSym = 0x63,
    Jump = 0x70,
    JumpIfZero = 0x71,
    JumpIfNotZero = 0x72,
    Return = 0x80,
}

/// A single bytecode instruction with decoded operands.
#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    Halt,

    // 1 register
    LoadNull(u16),
    LoadTrue(u16),
    LoadFalse(u16),

    // 2 registers
    Mov(u16, u16),
    Neg(u16, u16),
    Not(u16, u16),

    // 3 registers
    Add(u16, u16, u16),
    Sub(u16, u16, u16),
    Mul(u16, u16, u16),
    Div(u16, u16, u16),
    Pow(u16, u16, u16),
    IntAdd(u16, u16, u16),
    IntSub(u16, u16, u16),
    IntMul(u16, u16, u16),
    RealAdd(u16, u16, u16),
    RealSub(u16, u16, u16),
    RealMul(u16, u16, u16),
    RealDiv(u16, u16, u16),
    Eq(u16, u16, u16),
    Neq(u16, u16, u16),
    Lt(u16, u16, u16),
    Gt(u16, u16, u16),
    Le(u16, u16, u16),
    Ge(u16, u16, u16),
    And(u16, u16, u16),
    Or(u16, u16, u16),

    // register + u8
    MakeList(u16, u8),
    MakeAssoc(u16, u8),
    Apply(u16, u8),
    Call(u16, u8),
    TailCall(u16, u8),

    // register + u32
    LoadConst(u16, u32),
    LoadArg(u16, u32),
    LoadSym(u16, u32),
    StoreSym(u32, u16),

    // register + i32
    Jump(i32),
    JumpIfZero(u16, i32),
    JumpIfNotZero(u16, i32),

    // Return
    Return(u16),
}

/// Convenience builder for emitting instructions.
#[derive(Debug, Default)]
pub struct CodeBuilder {
    pub code: Vec<Instruction>,
}

impl CodeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn emit(&mut self, instr: Instruction) {
        self.code.push(instr);
    }

    /// Emit a placeholder Jump and return its index for later patching.
    pub fn emit_jump(&mut self) -> usize {
        let idx = self.code.len();
        self.code.push(Instruction::Jump(0));
        idx
    }

    /// Patch a jump at `label_id` to point to `target`.
    pub fn patch_jump(&mut self, label_id: usize, target: i32) {
        match &mut self.code[label_id] {
            Instruction::Jump(offset)
            | Instruction::JumpIfZero(_, offset)
            | Instruction::JumpIfNotZero(_, offset) => *offset = target,
            _ => panic!("Cannot patch non-jump instruction at {label_id}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_builder_patch() {
        let mut b = CodeBuilder::new();
        b.emit(Instruction::LoadConst(0, 0));
        let j1 = b.emit_jump();
        b.emit(Instruction::Return(0));
        b.patch_jump(j1, 2);
        assert_eq!(b.code[1], Instruction::Jump(2));
    }
}
