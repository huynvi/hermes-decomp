use crate::file::LiteralValue;
use crate::opcode::OperandValue;
use crate::util::{escape_js_string, is_valid_identifier};

pub fn format_f64(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value.is_infinite() {
        if value.is_sign_negative() {
            "-Infinity".to_string()
        } else {
            "Infinity".to_string()
        }
    } else {
        value.to_string()
    }
}

pub fn format_operand_value(value: &OperandValue) -> String {
    match value {
        OperandValue::U8(v) => v.to_string(),
        OperandValue::U16(v) => v.to_string(),
        OperandValue::U32(v) => v.to_string(),
        OperandValue::I8(v) => v.to_string(),
        OperandValue::I32(v) => v.to_string(),
        OperandValue::F64(v) => format_f64(*v),
    }
}

pub fn render_literal_value(value: &LiteralValue) -> String {
    match value {
        LiteralValue::Null => "null".to_string(),
        LiteralValue::Bool(true) => "true".to_string(),
        LiteralValue::Bool(false) => "false".to_string(),
        LiteralValue::Number(value) => format_f64(*value),
        LiteralValue::Integer(value) => value.to_string(),
        LiteralValue::String(value) => escape_js_string(value),
        LiteralValue::Undefined => "undefined".to_string(),
    }
}

pub fn render_object_key(value: &LiteralValue) -> String {
    match value {
        LiteralValue::String(value) => {
            if is_valid_identifier(value) {
                value.clone()
            } else {
                escape_js_string(value)
            }
        }
        LiteralValue::Integer(value) => value.to_string(),
        LiteralValue::Number(value) => format_f64(*value),
        _ => format!("[{}]", render_literal_value(value)),
    }
}
