// Jump target analysis for basic block construction.

use std::collections::BTreeSet;
use crate::{BytecodeFormat, Instruction, BytecodeFile};
use crate::opcode::OperandType;

// Analyze instructions to find all jump targets (block boundaries).
pub fn find_block_starts(insts: &[Instruction], format: &BytecodeFormat, file: &BytecodeFile) -> BTreeSet<u32> {
    let mut targets = BTreeSet::new();

    // First instruction is always a block start
    if let Some(first) = insts.first() {
        targets.insert(first.offset);
    }

    for inst in insts {
        let def = match format.definitions.get(inst.opcode as usize) {
            Some(d) => d,
            None => continue,
        };

        let name = def.name.as_str();

        // Instructions that end a block - next instruction starts a new block
        if matches!(name, "Ret" | "Throw" | "Jmp" | "JmpLong") {
            // The instruction after this one starts a new block (if it exists)
            let next_offset = inst.offset + inst.length;
            targets.insert(next_offset);
        }

        // Jump instructions - their targets are block starts
        if def.is_jump {
            for operand in &inst.operands {
                if matches!(operand.ty, OperandType::Addr8 | OperandType::Addr32) {
                    if let Some(rel) = operand.value.as_i32() {
                        let target = inst.offset as i32 + rel;
                        if target >= 0 {
                            targets.insert(target as u32);
                        }
                    }
                }
            }
            // Conditional jumps also have a fall-through to next instruction
            if is_conditional_jump(name) {
                targets.insert(inst.offset + inst.length);
            }
        }

        // Handle SwitchImm specially
        if name == "SwitchImm" {
             if let (Some(default_op), Some(min_op), Some(max_op)) = (
                inst.operands.get(1),
                inst.operands.get(2),
                inst.operands.get(3)
            ) {
                if let (Some(default_offset), Some(min_val), Some(max_val)) = (
                    default_op.value.as_i32(),
                    min_op.value.as_u32(),
                    max_op.value.as_u32()
                ) {
                    // Default target
                    let default_target = (inst.offset as i32 + default_offset) as u32;
                    targets.insert(default_target);

                    // Read jump table
                    let end_of_inst = inst.offset as usize + inst.length as usize;
                    let table_start = (end_of_inst + 3) & !3;
                    let count = (max_val - min_val + 1) as usize;

                    if table_start + count * 4 <= file.instructions.len() {
                        use crate::io::ByteReader;
                        let mut reader = ByteReader::new(&file.instructions[table_start..]);
                        for _ in 0..count {
                             if let Ok(rel_offset) = reader.read_i32() {
                                let target = (inst.offset as i32 + rel_offset) as u32;
                                targets.insert(target);
                            }
                        }
                    }
                }
            }
        }
    }

    targets
}

// Check if an instruction is a conditional jump.
pub fn is_conditional_jump(name: &str) -> bool {
    matches!(
        name,
        "JmpTrue" | "JmpTrueLong" | "JmpFalse" | "JmpFalseLong"
            | "JEqual" | "JNotEqual" | "JStrictEqual" | "JStrictNotEqual"
            | "JLess" | "JLessEqual" | "JGreater" | "JGreaterEqual"
            | "JLessN" | "JLessEqualN" | "JGreaterN" | "JGreaterEqualN"
            | "JNotLess" | "JNotLessEqual" | "JNotGreater" | "JNotGreaterEqual"
            | "JNotLessN" | "JNotLessEqualN" | "JNotGreaterN" | "JNotGreaterEqualN"
    )
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_conditional_jump() {
        assert!(is_conditional_jump("JmpTrue"));
        assert!(is_conditional_jump("JStrictEqual"));
        assert!(!is_conditional_jump("Jmp"));
        assert!(!is_conditional_jump("Ret"));
    }
}
