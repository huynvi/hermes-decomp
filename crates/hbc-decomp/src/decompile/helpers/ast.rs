use crate::error::{Error, Result};
use crate::file::{BytecodeFile, Instruction};
use crate::opcode::{Operand, OperandType};
use super::super::DecompileOptions;
use super::context::{reg_from_operand, expr_from_operand, prop_from_operand, operand_to_u32};
use super::format::{render_literal_value, render_object_key};

/// Helper functions for rendering bytecode instructions into AST-like string representations.
/// 
/// Note: These functions generate "C-style" string snippets (e.g., `r0 = r1 + r2;`).
/// This is used for the "Disassembler" view or fallback debugging, NOT for the high-level Decompiler (which uses IR).
/// The actual Decompiler builds a CFG and structural tree, then emits JS.
/// These helpers are for the `dump` command or fast-path rendering.

pub fn load_const(insn: &Instruction, value: &str) -> Result<Option<String>> {
    let dst = reg_from_operand(&insn.operands[0])?;
    Ok(Some(format!("{dst} = {value};")))
}

pub fn render_binary_op(
    name: &str,
    insn: &Instruction,
    file: &BytecodeFile,
    options: &DecompileOptions,
) -> Result<Option<String>> {
    let dst = reg_from_operand(&insn.operands[0])?;
    let left = expr_from_operand(file, &insn.operands[1], options);
    let right = expr_from_operand(file, &insn.operands[2], options);
    let op = match name {
        "Add" | "AddN" => "+",
        "Sub" | "SubN" => "-",
        "Mul" | "MulN" => "*",
        "Div" | "DivN" => "/",
        "Mod" => "%",
        "BitAnd" => "&",
        "BitOr" => "|",
        "BitXor" => "^",
        "Shl" => "<<",
        "Shr" => ">>",
        "UShr" => ">>>",
        "Eq" => "==",
        "StrictEq" => "===",
        "Neq" => "!=",
        "StrictNeq" => "!==",
        "Less" => "<",
        "LessEq" => "<=",
        "Greater" => ">",
        "GreaterEq" => ">=",
        _ => "?",
    };
    Ok(Some(format!("{dst} = {left} {op} {right};")))
}

pub fn render_unary_op(
    name: &str,
    insn: &Instruction,
    file: &BytecodeFile,
    options: &DecompileOptions,
) -> Result<Option<String>> {
    let dst = reg_from_operand(&insn.operands[0])?;
    let value = expr_from_operand(file, &insn.operands[1], options);
    let op = match name {
        "Negate" => "-",
        "Not" => "!",
        "BitNot" => "~",
        "TypeOf" => "typeof ",
        _ => "",
    };
    let expr = format!("{op}{value}");
    Ok(Some(format!("{dst} = {expr};")))
}

pub fn render_get_by_id(
    insn: &Instruction,
    file: &BytecodeFile,
    options: &DecompileOptions,
) -> Result<Option<String>> {
    let dst = reg_from_operand(
        insn.operands.first()
            .ok_or_else(|| Error::Parse("missing destination register".to_string()))?,
    )?;
    let obj = expr_from_operand(
        file,
        insn.operands
            .get(1)
            .ok_or_else(|| Error::Parse("missing object operand".to_string()))?,
        options,
    );
    let prop_operand = insn
        .operands
        .last()
        .ok_or_else(|| Error::Parse("missing property operand".to_string()))?;
    let prop = prop_from_operand(file, prop_operand, options);
    Ok(Some(format!("{dst} = {obj}{prop};")))
}

pub fn render_put_by_id(
    insn: &Instruction,
    file: &BytecodeFile,
    options: &DecompileOptions,
) -> Result<Option<String>> {
    // PutById(obj, val, "prop") -> obj.prop = val;
    let obj = expr_from_operand(
        file,
        insn.operands.first()
            .ok_or_else(|| Error::Parse("missing object operand".to_string()))?,
        options,
    );
    let value = expr_from_operand(
        file,
        insn.operands
            .get(1)
            .ok_or_else(|| Error::Parse("missing value operand".to_string()))?,
        options,
    );
    let prop_operand = insn
        .operands
        .last()
        .ok_or_else(|| Error::Parse("missing property operand".to_string()))?;
    let prop = prop_from_operand(file, prop_operand, options);
    Ok(Some(format!("{obj}{prop} = {value};")))
}

/// Renders `NewArrayWithBuffer` instruction.
/// This instruction initializes an array with static constants (often numbers/strings) from a side buffer.
/// Optimizes `x = [1, 2, 3]` instead of lots of separate assignments.
pub fn render_new_array_with_buffer(
    name: &str,
    insn: &Instruction,
    file: &BytecodeFile,
    options: &DecompileOptions,
) -> Result<Option<String>> {
    let dst = reg_from_operand(&insn.operands[0])?;
    let count = operand_to_u32(insn.operands.get(2));
    let offset = operand_to_u32(insn.operands.get(3));

    if let (Some(count), Some(offset)) = (count, offset) {
        if let Ok(values) = file.read_array_buffer_series(offset, count) {
            let rendered: Vec<String> = values.iter().map(render_literal_value).collect();
            return Ok(Some(format!("{} = [{}];", dst, rendered.join(", "))));
        }
    }

    Ok(Some(render_fallback(name, insn, file, options)))
}

pub fn render_new_object_with_buffer(
    name: &str,
    insn: &Instruction,
    file: &BytecodeFile,
    options: &DecompileOptions,
) -> Result<Option<String>> {
    let dst = reg_from_operand(&insn.operands[0])?;
    let (count, key_offset, value_offset) = if insn.operands.len() >= 5 {
        let count = operand_to_u32(insn.operands.get(2));
        let key_offset = operand_to_u32(insn.operands.get(3));
        let value_offset = operand_to_u32(insn.operands.get(4));
        (count, key_offset, value_offset)
    } else if insn.operands.len() >= 3 {
        let shape_id = operand_to_u32(insn.operands.get(1));
        let value_offset = operand_to_u32(insn.operands.get(2));
        if let (Some(shape_id), Some(value_offset)) = (shape_id, value_offset) {
            if let Some(shape) = file.shape_at(shape_id) {
                (Some(shape.num_props), Some(shape.key_buffer_offset), Some(value_offset))
            } else {
                (None, None, None)
            }
        } else {
            (None, None, None)
        }
    } else {
        (None, None, None)
    };

    if let (Some(count), Some(key_offset), Some(value_offset)) = (count, key_offset, value_offset) {
        if let (Ok(keys), Ok(values)) = (
            file.read_key_buffer_series(key_offset, count),
            file.read_value_buffer_series(value_offset, count),
        ) {
            let mut parts = Vec::with_capacity(count as usize);
            for (key, value) in keys.iter().zip(values.iter()) {
                let key = render_object_key(key);
                let value = render_literal_value(value);
                parts.push(format!("{key}: {value}"));
            }
            return Ok(Some(format!("{} = {{ {} }};", dst, parts.join(", "))));
        }
    }

    Ok(Some(render_fallback(name, insn, file, options)))
}

pub fn render_call_fixed(
    insn: &Instruction,
    file: &BytecodeFile,
    options: &DecompileOptions,
) -> Result<Option<String>> {
    let dst = reg_from_operand(&insn.operands[0])?;
    let callee = expr_from_operand(file, &insn.operands[1], options);
    let args: Vec<String> = insn.operands[2..]
        .iter()
        .map(|op| expr_from_operand(file, op, options))
        .collect();
    Ok(Some(format!("{} = {}({});", dst, callee, args.join(", "))))
}

pub fn render_call_var(
    name: &str,
    insn: &Instruction,
    file: &BytecodeFile,
    options: &DecompileOptions,
) -> Result<Option<String>> {
    let dst = reg_from_operand(&insn.operands[0])?;
    let callee = expr_from_operand(file, &insn.operands[1], options);
    let argc = insn.operands.get(2).map(|op| expr_from_operand(file, op, options));
    let note = argc.map(|v| format!(" /* argc: {v} */")).unwrap_or_default();
    let call = match name {
        "Construct" => format!("new {callee}(){note}"),
        _ => format!("{callee}(){note}"),
    };
    Ok(Some(format!("{dst} = {call};")))
}

pub fn render_jump(
    name: &str,
    insn: &Instruction,
    file: &BytecodeFile,
    options: &DecompileOptions,
) -> Result<Option<String>> {
    let target = jump_target(insn);
    let non_addr: Vec<&Operand> = insn
        .operands
        .iter()
        .filter(|op| op.ty != OperandType::Addr8 && op.ty != OperandType::Addr32)
        .collect();

    if name == "Jmp" || name == "JmpLong" {
        if let Some(target) = target {
            return Ok(Some(format!("goto L{target};")));
        }
    }

    if name == "JmpTrue" || name == "JmpTrueLong" {
        if let (Some(cond), Some(target)) = (non_addr.first(), target) {
            let cond = expr_from_operand(file, cond, options);
            return Ok(Some(format!("if ({cond}) goto L{target};")));
        }
    }

    if name == "JmpFalse" || name == "JmpFalseLong" {
        if let (Some(cond), Some(target)) = (non_addr.first(), target) {
            let cond = expr_from_operand(file, cond, options);
            return Ok(Some(format!("if (!{cond}) goto L{target};")));
        }
    }

    if name == "JmpUndefined" || name == "JmpUndefinedLong" {
        if let (Some(cond), Some(target)) = (non_addr.first(), target) {
            let cond = expr_from_operand(file, cond, options);
            return Ok(Some(format!("if ({cond} === undefined) goto L{target};")));
        }
    }

    if name == "JmpBuiltinIs" || name == "JmpBuiltinIsLong" {
        if let (Some(target), Some(builtin), Some(value)) = (target, non_addr.first(), non_addr.get(1)) {
            let builtin_id = expr_from_operand(file, builtin, options);
            let value = expr_from_operand(file, value, options);
            return Ok(Some(format!(
                "if (builtin_is({value}, {builtin_id})) goto L{target};"
            )));
        }
    }

    if name == "JmpBuiltinIsNot" || name == "JmpBuiltinIsNotLong" {
        if let (Some(target), Some(builtin), Some(value)) = (target, non_addr.first(), non_addr.get(1)) {
            let builtin_id = expr_from_operand(file, builtin, options);
            let value = expr_from_operand(file, value, options);
            return Ok(Some(format!(
                "if (!builtin_is({value}, {builtin_id})) goto L{target};"
            )));
        }
    }

    if name == "JmpTypeOfIs" {
        if let (Some(target), Some(value), Some(type_mask)) = (target, non_addr.first(), non_addr.get(1)) {
            let value = expr_from_operand(file, value, options);
            let type_mask = expr_from_operand(file, type_mask, options);
            return Ok(Some(format!(
                "if (typeof {value} is {type_mask}) goto L{target};"
            )));
        }
    }

    let base = normalize_jump_name(name);
    let op = match base.as_str() {
        "Equal" => "==",
        "NotEqual" => "!=",
        "StrictEqual" => "===",
        "StrictNotEqual" => "!==",
        "Less" => "<",
        "LessEqual" => "<=",
        "Greater" => ">",
        "GreaterEqual" => ">=",
        "NotLess" => ">=",
        "NotLessEqual" => ">",
        "NotGreater" => "<=",
        "NotGreaterEqual" => "<",
        _ => "?",
    };

    if non_addr.len() >= 2 {
        let left = expr_from_operand(file, non_addr[0], options);
        let right = expr_from_operand(file, non_addr[1], options);
        if let Some(target) = target {
            return Ok(Some(format!("if ({left} {op} {right}) goto L{target};")));
        }
    }

    Ok(Some(render_fallback(name, insn, file, options)))
}

pub fn normalize_jump_name(name: &str) -> String {
    let mut base = name.trim_start_matches('J').to_string();
    for suffix in ["Long", "N"].iter() {
        if base.ends_with(suffix) {
            base = base.trim_end_matches(suffix).to_string();
        }
    }
    base
}

pub fn jump_target(insn: &Instruction) -> Option<u32> {
    for operand in &insn.operands {
        if operand.ty == OperandType::Addr8 || operand.ty == OperandType::Addr32 {
            if let Some(rel) = operand.value.as_i32() {
                let target = insn.offset as i32 + rel;
                if target >= 0 {
                    return Some(target as u32);
                }
            }
        }
    }
    None
}

pub fn render_fallback(
    name: &str,
    insn: &Instruction,
    file: &BytecodeFile,
    options: &DecompileOptions,
) -> String {
    let operands: Vec<String> = insn
        .operands
        .iter()
        .map(|operand| expr_from_operand(file, operand, options))
        .collect();
    if operands.is_empty() {
        format!("// {name}")
    } else {
        format!("// {} {}", name, operands.join(", "))
    }
}
