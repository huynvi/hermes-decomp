use crate::ir::{Statement, Expression, Value, AssignTarget};

/// The type of destructuring pattern.
#[derive(Debug, Clone)]
pub enum DestructuringPattern {
    Object(Vec<String>),  // property names
    Array(usize),         // element count
}

/// Information about a destructured parameter.
#[derive(Debug, Clone)]
pub struct DestructuredParam {
    pub param_index: usize,
    pub statement_index: usize,
    pub pattern: DestructuringPattern,
}

/// Detect function parameter destructuring patterns.
pub fn detect_parameter_destructuring(stmts: &[Statement], param_count: usize) -> Vec<DestructuredParam> {
    let mut result = Vec::new();

    for (i, stmt) in stmts.iter().enumerate() {
        // Only check the first few statements for parameter destructuring
        if i >= param_count * 2 {
            break;
        }

        // Look for destructuring at the start of the function
        if let Statement::Assign { target, value } = stmt {
            // Check if value is a parameter (arg0, arg1, etc.)
            if let Expression::Value(Value::Variable(var_name)) = value {
                if let Some(param_idx) = var_name.strip_prefix("arg").and_then(|s| s.parse::<usize>().ok()) {
                    if param_idx < param_count {
                        match target {
                            AssignTarget::DestructuringObject(props) => {
                                result.push(DestructuredParam {
                                    param_index: param_idx,
                                    statement_index: i,
                                    pattern: DestructuringPattern::Object(
                                        props.iter().map(|(k, _)| k.clone()).collect()
                                    ),
                                });
                            }
                            AssignTarget::DestructuringArray(elements) => {
                                result.push(DestructuredParam {
                                    param_index: param_idx,
                                    statement_index: i,
                                    pattern: DestructuringPattern::Array(elements.len()),
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    result
}
