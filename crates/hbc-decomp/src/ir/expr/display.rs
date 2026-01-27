// Display implementations for expressions.

use std::fmt;
use super::{Expression, PropertyKey};

impl fmt::Display for PropertyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PropertyKey::Ident(name) => write!(f, ".{name}"),
            PropertyKey::Computed(expr) => write!(f, "[{expr}]"),
            PropertyKey::String(s) => write!(f, "[\"{s}\"]"),
            PropertyKey::Index(i) => write!(f, "[{i}]"),
        }
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expression::Value(v) => write!(f, "{v}"),

            Expression::Binary { op, left, right } => {
                let prec = op.precedence();
                write_with_parens(f, left, prec)?;
                write!(f, " {op} ")?;
                write_with_parens(f, right, prec)
            }

            Expression::Unary { op, operand } => {
                write!(f, "{op}{operand}")
            }

            Expression::Conditional { condition, then_expr, else_expr } => {
                write!(f, "{condition} ? {then_expr} : {else_expr}")
            }

            Expression::Member { object, property, optional } => {
                write!(f, "{}{}{}", object, if *optional { "?" } else { "" }, property)
            }

            Expression::Call { callee, arguments } => {
                if let Some((first, rest)) = arguments.split_first() {
                    // Check if first arg is 'undefined'
                    let is_undefined_this = matches!(first, Expression::Value(crate::ir::Value::Constant(crate::ir::Constant::Undefined)));
                    
                    if is_undefined_this {
                         // Standard call: callee(args...)
                         write!(f, "{}({})", callee, join_exprs(rest))
                    } else {
                        // Check for method call pattern: obj.method(obj, args...)
                         let is_method_call = if let Expression::Member { object, .. } = &**callee {
                             // Compare object expression with first argument
                             // Simple string comparison for now (safe enough for display)
                             object.to_string() == first.to_string()
                         } else {
                             false
                         };

                         if is_method_call {
                              write!(f, "{}({})", callee, join_exprs(rest))
                         } else {
                              // Explicit .call for context mismatch
                              write!(f, "{}.call({})", callee, join_exprs(arguments))
                         }
                    }
                } else {
                    write!(f, "{}({})", callee, join_exprs(arguments))
                }
            }

            Expression::New { callee, arguments } => {
                write!(f, "new {}({})", callee, join_exprs(arguments))
            }

            Expression::Array { elements } => {
                let elems: Vec<String> = elements.iter()
                    .map(|e| e.as_ref().map(|e| e.to_string()).unwrap_or_default())
                    .collect();
                write!(f, "[{}]", elems.join(", "))
            }

            Expression::Object { properties } => {
                if properties.is_empty() {
                    write!(f, "{{}}")
                } else {
                    let props: Vec<String> = properties.iter()
                        .map(format_property)
                        .collect();
                    write!(f, "{{ {} }}", props.join(", "))
                }
            }

            Expression::Function { id, name, is_arrow, is_async, is_generator } => {
                let async_prefix = if *is_async { "async " } else { "" };
                let gen_star = if *is_generator { "*" } else { "" };
                match (is_arrow, name) {
                    (true, Some(n)) => write!(f, "/* F{} */ {}({}) => {{ ... }}", id.0, async_prefix, n),
                    (true, None) => write!(f, "/* F{} */ {}() => {{ ... }}", id.0, async_prefix),
                    (false, Some(n)) => write!(f, "/* F{} */ {}function{} {}() {{ ... }}", id.0, async_prefix, gen_star, n),
                    (false, None) => write!(f, "/* F{} */ {}function{}() {{ ... }}", id.0, async_prefix, gen_star),
                }
            }

            Expression::Assignment { target, value } => {
                write!(f, "{target} = {value}")
            }

            Expression::Spread(inner) => {
                write!(f, "...{inner}")
            }

            Expression::TemplateLiteral { quasis, expressions } => {
                write!(f, "`")?;
                for (i, quasi) in quasis.iter().enumerate() {
                    write!(f, "{quasi}")?;
                    if let Some(expr) = expressions.get(i) {
                        write!(f, "${{{expr}}}")?;
                    }
                }
                write!(f, "`")
            }

            Expression::RegExp { pattern, flags } => {
                write!(f, "/{pattern}/{flags}")
            }

            Expression::Yield { value, delegate } => {
                if *delegate {
                    write!(f, "yield* {value}")
                } else {
                    write!(f, "yield {value}")
                }
            }

            Expression::Await(value) => {
                write!(f, "await {value}")
            }

            Expression::Unknown { opcode, operands } => {
                write!(f, "/* {} {} */", opcode, operands.join(", "))
            }
        }
    }
}

fn write_with_parens(f: &mut fmt::Formatter<'_>, expr: &Expression, parent_prec: u8) -> fmt::Result {
    let needs_parens = match expr {
        Expression::Binary { op, .. } => op.precedence() < parent_prec,
        _ => false,
    };
    if needs_parens {
        write!(f, "({expr})")
    } else {
        write!(f, "{expr}")
    }
}

fn join_exprs(exprs: &[Expression]) -> String {
    exprs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", ")
}

fn format_key(key: &PropertyKey) -> String {
    match key {
        PropertyKey::Ident(name) => {
            if name.chars().all(|c| c.is_ascii_digit()) {
                format!("\"{}\"", name)
            } else {
                name.clone()
            }
        }
        PropertyKey::String(s) => format!("\"{s}\""),
        PropertyKey::Computed(e) => format!("[{e}]"),
        PropertyKey::Index(i) => i.to_string(),
    }
}

/// Format an object property, using shorthand syntax when applicable.
/// E.g., `{ x }` instead of `{ x: x }` when key equals value name.
fn format_property(prop: &super::ObjectProperty) -> String {
    use crate::ir::Value;

    // Check for shorthand: key is Ident and value is a Variable with the same name
    if let PropertyKey::Ident(key_name) = &prop.key {
        if let Expression::Value(Value::Variable(var_name)) = &prop.value {
            if key_name == var_name {
                return key_name.clone();
            }
        }
    }

    // Also check for method shorthand (function values)
    if let PropertyKey::Ident(key_name) = &prop.key {
        if let Expression::Function { name: Some(fn_name), .. } = &prop.value {
            if key_name == fn_name {
                // Method shorthand - just show the function
                return format!("{}", prop.value);
            }
        }
    }

    // Default: key: value
    format!("{}: {}", format_key(&prop.key), prop.value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Constant, Value, BinaryOp};

    #[test]
    fn test_binary_display() {
        let expr = Expression::binary(
            BinaryOp::Add,
            Expression::Value(Value::Register(0)),
            Expression::Value(Value::Constant(Constant::Integer(1))),
        );
        assert_eq!(format!("{expr}"), "r0 + 1");
    }

    #[test]
    fn test_precedence_parens() {
        let expr = Expression::binary(
            BinaryOp::Mul,
            Expression::binary(
                BinaryOp::Add,
                Expression::register(0),
                Expression::register(1),
            ),
            Expression::register(2),
        );
        assert_eq!(format!("{expr}"), "(r0 + r1) * r2");
    }
}
