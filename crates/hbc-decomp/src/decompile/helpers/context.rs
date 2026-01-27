use crate::error::{Error, Result};
use crate::file::BytecodeFile;
use crate::opcode::{Operand, OperandType, OperandValue};
use crate::util::{escape_js_string, is_valid_identifier};
use super::super::DecompileOptions;
use super::format::{format_operand_value};

pub fn reg_from_operand(operand: &Operand) -> Result<String> {
    match operand.value {
        OperandValue::U8(v) => Ok(format!("r{v}")),
        OperandValue::U16(v) => Ok(format!("r{v}")),
        OperandValue::U32(v) => Ok(format!("r{v}")),
        _ => Err(Error::Parse("expected register operand".to_string())),
    }
}

pub fn expr_from_operand(file: &BytecodeFile, operand: &Operand, options: &DecompileOptions) -> String {
    match operand.ty {
        OperandType::Reg8 | OperandType::Reg32 => match operand.value {
            OperandValue::U8(v) => format!("r{v}"),
            OperandValue::U16(v) => format!("r{v}"),
            OperandValue::U32(v) => format!("r{v}"),
            _ => "r?".to_string(),
        },
        OperandType::UInt8S | OperandType::UInt16S | OperandType::UInt32S => {
            if options.resolve_strings {
                if let Some(id) = operand.value.as_u32() {
                    if let Some(entry) = file.string_at(id) {
                        return escape_js_string(&entry.value);
                    }
                }
            }
            format_operand_value(&operand.value)
        }
        OperandType::Addr8 | OperandType::Addr32 => format_operand_value(&operand.value),
        _ => format_operand_value(&operand.value),
    }
}

pub fn prop_from_operand(file: &BytecodeFile, operand: &Operand, options: &DecompileOptions) -> String {
    if let OperandType::UInt8S | OperandType::UInt16S | OperandType::UInt32S = operand.ty {
        if options.resolve_strings {
            if let Some(id) = operand.value.as_u32() {
                if let Some(entry) = file.string_at(id) {
                    if entry.is_identifier && is_valid_identifier(&entry.value) {
                        return format!(".{}", entry.value);
                    }
                    return format!("[{}]", escape_js_string(&entry.value));
                }
            }
        }
    }

    let value = expr_from_operand(file, operand, options);
    format!("[{value}]")
}

pub fn render_declare_global_var(
    file: &BytecodeFile,
    operand: &Operand,
    options: &DecompileOptions,
) -> String {
    if options.resolve_strings {
        if let Some(id) = operand.value.as_u32() {
            if let Some(entry) = file.string_at(id) {
                if is_valid_identifier(&entry.value) {
                    return format!("var {};", entry.value);
                }
                return format!("globalThis[{}] = undefined;", escape_js_string(&entry.value));
            }
        }
    }

    let name = expr_from_operand(file, operand, options);
    format!("globalThis[{name}] = undefined;")
}

pub fn operand_to_u32(operand: Option<&Operand>) -> Option<u32> {
    match operand?.value {
        OperandValue::U8(value) => Some(value as u32),
        OperandValue::U16(value) => Some(value as u32),
        OperandValue::U32(value) => Some(value),
        _ => None,
    }
}
