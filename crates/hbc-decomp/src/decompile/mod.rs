use std::collections::BTreeSet;

use crate::error::Result;
use crate::file::{BytecodeFile, Instruction};
use crate::opcode::{BytecodeFormat, OperandType};
use crate::util::is_valid_identifier;

pub mod helpers;
pub mod instructions;

use instructions::render_instruction;

#[derive(Debug, Clone)]
pub struct DecompileOptions {
    pub show_offsets: bool,
    pub show_labels: bool,
    pub resolve_strings: bool,
}

impl Default for DecompileOptions {
    fn default() -> Self {
        Self {
            show_offsets: false,
            show_labels: true,
            resolve_strings: true,
        }
    }
}

pub fn decompile_function(
    file: &BytecodeFile,
    format: &BytecodeFormat,
    function_id: u32,
    options: &DecompileOptions,
) -> Result<String> {
    let function_name = function_label(file, function_id);

    let instructions = file.decode_function_instructions(format, function_id)?;
    let label_offsets = collect_label_offsets(&instructions, format);

    let mut out = String::new();
    out.push_str(&format!("function {function_name}() {{\n"));

    for insn in &instructions {
        if options.show_labels && label_offsets.contains(&insn.offset) {
            out.push_str(&format!("  L{}:\n", insn.offset));
        }
        if let Some(line) = render_instruction(file, format, insn, options)? {
            if options.show_offsets {
                out.push_str(&format!("  /* {:04x} */ ", insn.offset));
            } else {
                out.push_str("  ");
            }
            out.push_str(&line);
            out.push('\n');
        }
    }

    out.push_str("}\n");
    Ok(out)
}

pub fn decompile_all(
    file: &BytecodeFile,
    format: &BytecodeFormat,
    options: &DecompileOptions,
) -> Result<String> {
    let mut out = String::new();
    for header in &file.function_headers {
        out.push_str(&decompile_function(file, format, header.function_id(), options)?);
        out.push('\n');
    }
    Ok(out)
}

fn collect_label_offsets(instructions: &[Instruction], format: &BytecodeFormat) -> BTreeSet<u32> {
    let mut labels = BTreeSet::new();
    for insn in instructions {
        let def = match format.definitions.get(insn.opcode as usize) {
            Some(def) => def,
            None => continue,
        };
        if def.name == "Ret" || def.name == "Throw" || def.name == "Jmp" || def.name == "JmpLong" {
            labels.insert(insn.offset + insn.length);
        }
        if def.is_jump {
            for operand in &insn.operands {
                if operand.ty == OperandType::Addr8 || operand.ty == OperandType::Addr32 {
                    if let Some(rel) = operand.value.as_i32() {
                        let target = insn.offset as i32 + rel;
                        if target >= 0 {
                            labels.insert(target as u32);
                        }
                    }
                }
            }
        }
    }
    labels
}

fn function_label(file: &BytecodeFile, function_id: u32) -> String {
    let name = file
        .function_headers
        .get(function_id as usize)
        .and_then(|header| file.string_at(header.function_name()))
        .map(|entry| entry.value.clone());

    match name {
        Some(value) if !value.is_empty() && is_valid_identifier(&value) => value,
        _ => format!("f{function_id}"),
    }
}
