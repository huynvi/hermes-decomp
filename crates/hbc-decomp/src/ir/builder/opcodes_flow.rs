// Opcode handlers for control flow operations.

use crate::{BytecodeFile, BytecodeFormat, Instruction};
use crate::ir::{Expression, Statement, BinaryOp};
use crate::opcode::OperandType;
use super::opcodes_load::{get_reg, reg_expr};

// Result of processing a control flow instruction.
pub enum FlowResult {
    // Regular statement, continue in same block.
    Statement(Statement),
    // Unconditional jump to target.
    Jump { target: u32 },
    // Conditional branch.
    Branch { condition: Expression, target: u32, fallthrough: u32 },
    // Return from function.
    Return(Option<Expression>),
    // Throw exception.
    Throw(Expression),
    // No-op (e.g., environment setup).
    Noop,
    // Switch statement.
    Switch {
        value: Expression,
        default: u32,
        cases: Vec<(u32, u32)>, // (value matches min+i, target)
    },
}

// Handle unconditional jump opcodes.
pub fn handle_jmp(inst: &Instruction, format: &BytecodeFormat) -> Option<FlowResult> {
    let target = get_jump_target(inst, format)?;
    Some(FlowResult::Jump { target })
}

// Handle conditional jump opcodes (JmpTrue, JmpFalse).
// Operand order: Addr (target), Reg (condition)
pub fn handle_jmp_cond(
    name: &str,
    inst: &Instruction,
    format: &BytecodeFormat,
) -> Option<FlowResult> {
    // JmpTrue/JmpFalse have: Addr8/Addr32 first, then Reg8
    let target = get_jump_target(inst, format)?;
    let cond = reg_expr(&inst.operands, 1)?; // Register is second operand
    let fallthrough = inst.offset + inst.length;

    let condition = if name.contains("False") {
        Expression::unary(crate::ir::UnaryOp::Not, cond)
    } else {
        cond
    };

    Some(FlowResult::Branch { condition, target, fallthrough })
}

// Handle comparison jump opcodes (JEqual, JStrictEqual, etc.).
// Operand order: Addr (target), Reg (left), Reg (right)
pub fn handle_jmp_comparison(
    name: &str,
    inst: &Instruction,
    format: &BytecodeFormat,
) -> Option<FlowResult> {
    // Comparison jumps have: Addr8/Addr32 first, then Reg8, Reg8
    let target = get_jump_target(inst, format)?;
    let left = reg_expr(&inst.operands, 1)?;
    let right = reg_expr(&inst.operands, 2)?;
    let fallthrough = inst.offset + inst.length;

    // Strip "Long" suffix for matching
    let base_name = name.trim_end_matches("Long");

    let op = match base_name {
        "JEqual" => BinaryOp::Eq,
        "JNotEqual" => BinaryOp::Neq,
        "JStrictEqual" => BinaryOp::StrictEq,
        "JStrictNotEqual" => BinaryOp::StrictNeq,
        "JLess" | "JLessN" => BinaryOp::Lt,
        "JLessEqual" | "JLessEqualN" => BinaryOp::Le,
        "JGreater" | "JGreaterN" => BinaryOp::Gt,
        "JGreaterEqual" | "JGreaterEqualN" => BinaryOp::Ge,
        "JNotLess" | "JNotLessN" => BinaryOp::Ge,
        "JNotLessEqual" | "JNotLessEqualN" => BinaryOp::Gt,
        "JNotGreater" | "JNotGreaterN" => BinaryOp::Le,
        "JNotGreaterEqual" | "JNotGreaterEqualN" => BinaryOp::Lt,
        _ => return None,
    };

    let condition = Expression::binary(op, left, right);
    Some(FlowResult::Branch { condition, target, fallthrough })
}

// Handle Ret opcode.
pub fn handle_ret(inst: &Instruction) -> Option<FlowResult> {
    let value = reg_expr(&inst.operands, 0)?;
    Some(FlowResult::Return(Some(value)))
}

// Handle Throw opcode.
pub fn handle_throw(inst: &Instruction) -> Option<FlowResult> {
    let value = reg_expr(&inst.operands, 0)?;
    Some(FlowResult::Throw(value))
}

// Handle environment opcodes (mostly no-op for decompilation).
pub fn handle_create_environment(inst: &Instruction) -> Option<FlowResult> {
    let _dst = get_reg(&inst.operands, 0)?;
    // CreateEnvironment is a no-op in terms of visible JS code
    Some(FlowResult::Noop)
}

// Handle GetEnvironment opcode.
pub fn handle_get_environment(inst: &Instruction) -> Option<FlowResult> {
    let _dst = get_reg(&inst.operands, 0)?;
    let _level = inst.operands.get(1)?.value.as_u32()?;
    Some(FlowResult::Noop)
}

// Handle LoadFromEnvironment opcode.
pub fn handle_load_from_environment(inst: &Instruction) -> Option<FlowResult> {
    let dst = get_reg(&inst.operands, 0)?;
    let _env = get_reg(&inst.operands, 1)?;
    let slot = inst.operands.get(2)?.value.as_u32()?;

    // Represent as loading from a closure variable
    Some(FlowResult::Statement(Statement::Assign {
        target: crate::ir::AssignTarget::Register(dst),
        value: Expression::Value(crate::ir::Value::ClosureVar { level: 0, slot }),
    }))
}

// Handle StoreToEnvironment opcode.
pub fn handle_store_to_environment(inst: &Instruction) -> Option<FlowResult> {
    let _env = get_reg(&inst.operands, 0)?;
    let slot = inst.operands.get(1)?.value.as_u32()?;
    let value = reg_expr(&inst.operands, 2)?;

    Some(FlowResult::Statement(Statement::Assign {
        target: crate::ir::AssignTarget::ClosureVar { level: 0, slot },
        value,
    }))
}

// Handle StoreNPToEnvironment opcode.
pub fn handle_store_np_to_environment(inst: &Instruction) -> Option<FlowResult> {
    handle_store_to_environment(inst)
}

// Get jump target offset from instruction.
fn get_jump_target(inst: &Instruction, format: &BytecodeFormat) -> Option<u32> {
    let def = format.definitions.get(inst.opcode as usize)?;

    if !def.is_jump {
        return None;
    }

    for operand in &inst.operands {
        if matches!(operand.ty, OperandType::Addr8 | OperandType::Addr32) {
            if let Some(rel) = operand.value.as_i32() {
                let target = inst.offset as i32 + rel;
                if target >= 0 {
                    return Some(target as u32);
                }
            }
        }
    }
    None
}

// Handle SelectObject opcode (used in conditional expressions).
pub fn handle_select_object(inst: &Instruction) -> Option<FlowResult> {
    let dst = get_reg(&inst.operands, 0)?;
    let result = reg_expr(&inst.operands, 1)?;
    let this_obj = reg_expr(&inst.operands, 2)?;

    // SelectObject: if result is an object, use result; otherwise use this_obj
    Some(FlowResult::Statement(Statement::Assign {
        target: crate::ir::AssignTarget::Register(dst),
        value: Expression::Conditional {
            condition: Box::new(Expression::binary(
                BinaryOp::StrictEq,
                Expression::unary(crate::ir::UnaryOp::TypeOf, result.clone()),
                Expression::Value(crate::ir::Value::Constant(crate::ir::Constant::String("object".to_string()))),
            )),
            then_expr: Box::new(result),
            else_expr: Box::new(this_obj),
        },
    }))
}

// Handle Debugger opcode.
pub fn handle_debugger() -> Option<FlowResult> {
    Some(FlowResult::Statement(Statement::Debugger))
}

// Handle Catch opcode - stores the caught exception in a register.
pub fn handle_catch(inst: &Instruction) -> Option<FlowResult> {
    let dst = get_reg(&inst.operands, 0)?;
    // The Catch opcode puts the exception into a register
    // This marks the beginning of a catch block
    Some(FlowResult::Statement(Statement::Assign {
        target: crate::ir::AssignTarget::Register(dst),
        value: Expression::Value(crate::ir::Value::Variable("__exception".to_string())),
    }))
}

// Handle JmpUndefined opcode - jump if value is undefined.
pub fn handle_jmp_undefined(
    _name: &str,
    inst: &Instruction,
    format: &BytecodeFormat,
) -> Option<FlowResult> {
    let target = get_jump_target(inst, format)?;
    let val = reg_expr(&inst.operands, 1)?;
    let fallthrough = inst.offset + inst.length;

    let condition = Expression::binary(
        BinaryOp::StrictEq,
        val,
        Expression::Value(crate::ir::Value::Constant(crate::ir::Constant::Undefined)),
    );

    Some(FlowResult::Branch { condition, target, fallthrough })
}

// Handle GetNextPName opcode (for-in iteration).
pub fn handle_get_next_pname(inst: &Instruction) -> Option<FlowResult> {
    let dst = get_reg(&inst.operands, 0)?;
    let props = reg_expr(&inst.operands, 1)?;
    let _obj = reg_expr(&inst.operands, 2)?;
    let idx = reg_expr(&inst.operands, 3)?;
    let _size = reg_expr(&inst.operands, 4)?;

    // Get next property name from the property list
    Some(FlowResult::Statement(Statement::Assign {
        target: crate::ir::AssignTarget::Register(dst),
        value: Expression::Member {
            object: Box::new(props),
            property: crate::ir::PropertyKey::Computed(Box::new(idx)),
            optional: false,
        },
    }))
}

// Handle SwitchImm opcode.
pub fn handle_switch_imm(
    inst: &Instruction,
    _format: &BytecodeFormat,
    file: &BytecodeFile,
) -> Option<FlowResult> {
    let val = reg_expr(&inst.operands, 0)?;
    let default_offset = inst.operands.get(1)?.value.as_i32()?;
    let min_val = inst.operands.get(2)?.value.as_u32()?;
    let max_val = inst.operands.get(3)?.value.as_u32()?;
    
    // Convert default offset to absolute instruction offset
    // Jumps in Hermes are relative to the start of the instruction
    let default_target = (inst.offset as i32 + default_offset) as u32;

    // Read jump table
    // Jump table starts after the instruction, aligned to 4 bytes
    let end_of_inst = inst.offset as usize + inst.length as usize;
    let table_start = (end_of_inst + 3) & !3;
    
    let count = (max_val - min_val + 1) as usize;
    let mut cases = Vec::with_capacity(count);

    // Ensure we don't read past end of file
    if table_start + count * 4 > file.instructions.len() {
         return Some(FlowResult::Statement(Statement::Comment(
            format!("SwitchImm: jump table out of bounds (start={}, count={}, len={})", table_start, count, file.instructions.len())
        )));
    }

    use crate::io::ByteReader;
    let mut reader = ByteReader::new(&file.instructions[table_start..]);

    for i in 0..count {
        // Read offset (i32)
        if let Ok(rel_offset) = reader.read_i32() {
            let target = (inst.offset as i32 + rel_offset) as u32;
            let case_val = min_val + i as u32;
            cases.push((case_val, target));
        }
    }

    Some(FlowResult::Switch {
        value: val,
        default: default_target,
        cases,
    })
}

// Handle StartGenerator opcode.
pub fn handle_start_generator() -> Option<FlowResult> {
    Some(FlowResult::Statement(Statement::Comment("StartGenerator".to_string())))
}

// Handle ResumeGenerator opcode.
pub fn handle_resume_generator(inst: &Instruction) -> Option<FlowResult> {
    // ResumeGenerator dst, gen
    let dst = get_reg(&inst.operands, 0)?;
    let gen = reg_expr(&inst.operands, 1)?;
    Some(FlowResult::Statement(Statement::Assign {
        target: crate::ir::AssignTarget::Register(dst),
        value: Expression::Call {
            callee: Box::new(Expression::Member {
                object: Box::new(gen),
                property: crate::ir::PropertyKey::Ident("resume".to_string()),
                optional: false,
            }),
            arguments: vec![],
        }
    }))
}

// Handle CreateGenerator opcode.
pub fn handle_create_generator(inst: &Instruction) -> Option<FlowResult> {
    // CreateGenerator dst, env
    let dst = get_reg(&inst.operands, 0)?;
    let env = get_reg(&inst.operands, 1)?;
    Some(FlowResult::Statement(Statement::Assign {
        target: crate::ir::AssignTarget::Register(dst),
        value: Expression::Call {
            callee: Box::new(Expression::Value(crate::ir::Value::Variable("CreateGenerator".to_string()))),
            arguments: vec![Expression::Value(crate::ir::Value::Register(env))],
        }
    }))
}

// Handle CompleteGenerator opcode.
pub fn handle_complete_generator(_inst: &Instruction) -> Option<FlowResult> {
    // CompleteGenerator is effectively a return
    Some(FlowResult::Return(None))
}

// Handle SaveGenerator opcode.
// SaveGenerator saves the current state and specifies where to resume.
// The next instruction after SaveGenerator is typically a Ret that yields the value.
// Operand: Addr8 or Addr32 - the resume address (relative offset).
pub fn handle_save_generator(inst: &Instruction, _format: &BytecodeFormat) -> Option<FlowResult> {
    // Get the resume address from the operand
    let resume_offset = match inst.operands.first()?.value {
        crate::opcode::OperandValue::I8(v) => v as i32,
        crate::opcode::OperandValue::I32(v) => v,
        _ => return None,
    };

    let resume_addr = (inst.offset as i32 + resume_offset) as u32;

    // SaveGenerator creates a yield point
    // We mark this with a special statement that will be transformed later
    Some(FlowResult::Statement(Statement::Comment(format!(
        "__yield_point__:{resume_addr}"
    ))))
}
