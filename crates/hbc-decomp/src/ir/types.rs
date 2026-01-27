// Core types for the IR.

use std::fmt;
use serde::{Serialize, Deserialize};

// Unique identifier for a basic block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BlockId(pub u32);

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "B{}", self.0)
    }
}

// Unique identifier for a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FunctionId(pub u32);

impl fmt::Display for FunctionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "F{}", self.0)
    }
}

// A constant value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Constant {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Integer(i32),
    String(String),
    /// BigInt literal (stored as string representation).
    BigInt(String),
}

impl fmt::Display for Constant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Constant::Undefined => write!(f, "undefined"),
            Constant::Null => write!(f, "null"),
            Constant::Bool(b) => write!(f, "{b}"),
            Constant::Number(n) if n.is_nan() => write!(f, "NaN"),
            Constant::Number(n) if n.is_infinite() => {
                write!(f, "{}Infinity", if n.is_sign_negative() { "-" } else { "" })
            }
            Constant::Number(n) => write!(f, "{n}"),
            Constant::Integer(i) => write!(f, "{i}"),
            Constant::String(s) => write!(f, "\"{}\"", escape_string(s)),
            Constant::BigInt(s) => write!(f, "{s}n"),
        }
    }
}

// A value reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Register(u32),
    Variable(String),
    Constant(Constant),
    This,
    Global,
    Parameter(u32),
    // Closure variable from parent scope (env level, slot index)
    ClosureVar { level: u32, slot: u32 },
    // The arguments object
    Arguments,
    // new.target meta-property
    NewTarget,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Register(r) => write!(f, "r{r}"),
            Value::Variable(name) => {
                if name.chars().all(|c| c.is_ascii_digit()) {
                    write!(f, "v{name}")
                } else {
                    write!(f, "{name}")
                }
            }
            Value::Constant(c) => write!(f, "{c}"),
            Value::This => write!(f, "this"),
            Value::Global => write!(f, "globalThis"),
            Value::Parameter(i) => write!(f, "arg{i}"),
            Value::ClosureVar { level, slot } => {
                if *level == 0 {
                    write!(f, "closure_{slot}")
                } else {
                    write!(f, "outer{level}_{slot}")
                }
            }
            Value::Arguments => write!(f, "arguments"),
            Value::NewTarget => write!(f, "new.target"),
        }
    }
}

fn escape_string(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\\' => "\\\\".to_string(),
            '"' => "\\\"".to_string(),
            '\n' => "\\n".to_string(),
            '\r' => "\\r".to_string(),
            '\t' => "\\t".to_string(),
            c if c.is_control() => format!("\\u{:04x}", c as u32),
            c => c.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_display() {
        assert_eq!(format!("{}", Constant::Undefined), "undefined");
        assert_eq!(format!("{}", Constant::Bool(true)), "true");
        assert_eq!(format!("{}", Constant::Integer(42)), "42");
    }
}
