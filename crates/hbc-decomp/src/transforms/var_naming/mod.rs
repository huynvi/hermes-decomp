mod suggestions;
mod state;
mod analysis;
mod renaming;

use crate::ir::Statement;
use state::VariableNamer;

use analysis::analyze_stmt;
use renaming::rename_stmt;

/// Infer and apply better variable names.
/// 
/// This is a two-pass transformation:
/// 1. Analysis: Visit all statements to find "naming hints".
///    - Hints come from property accesses (`obj.length` -> `len`), 
///    - Initializers (`fetch(...)` -> `response`), 
///    - and object keys (`{ email: r0 }` -> `r0` is `email`).
/// 2. Renaming: Apply the best found names to variables, replacing generic names like `r0`, `val`.
pub fn infer_variable_names(stmts: Vec<Statement>) -> Vec<Statement> {
    let mut namer = VariableNamer::new();

    // First pass: analyze to infer names
    for stmt in &stmts {
        analyze_stmt(&mut namer, stmt);
    }

    // Second pass: apply inferred names
    stmts.into_iter().map(|s| rename_stmt(&namer, s)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Expression, Value, AssignTarget, PropertyKey};

    #[test]
    fn test_fetch_naming() {
        // r0 = fetch(url) → response = fetch(url)
        let stmts = vec![Statement::Assign {
            target: AssignTarget::Register(0),
            value: Expression::Call {
                callee: Box::new(Expression::Value(Value::Variable("fetch".to_string()))),
                arguments: vec![Expression::Value(Value::Variable("url".to_string()))],
            },
        }];

        let result = infer_variable_names(stmts);

        if let Statement::Assign { target, .. } = &result[0] {
            assert!(matches!(target, AssignTarget::Variable(n) if n == "response"));
        } else {
            panic!("Expected assign statement");
        }
    }

    #[test]
    fn test_property_naming() {
        // r0 = obj.length → len = obj.length
        let stmts = vec![Statement::Assign {
            target: AssignTarget::Register(0),
            value: Expression::Member {
                object: Box::new(Expression::Value(Value::Variable("obj".to_string()))),
                property: PropertyKey::Ident("length".to_string()),
                optional: false,
            },
        }];

        let result = infer_variable_names(stmts);

        if let Statement::Assign { target, .. } = &result[0] {
            assert!(matches!(target, AssignTarget::Variable(n) if n == "len"));
        } else {
            panic!("Expected assign statement");
        }
    }

    #[test]
    fn test_new_instance_naming() {
        // r0 = new Date() → date = new Date()
        let stmts = vec![Statement::Assign {
            target: AssignTarget::Register(0),
            value: Expression::New {
                callee: Box::new(Expression::Value(Value::Variable("Date".to_string()))),
                arguments: vec![],
            },
        }];

        let result = infer_variable_names(stmts);

        if let Statement::Assign { target, .. } = &result[0] {
            assert!(matches!(target, AssignTarget::Variable(n) if n == "date"));
        } else {
            panic!("Expected assign statement");
        }
    }

    #[test]
    fn test_unique_names() {
        // Two fetch calls should get unique names
        let stmts = vec![
            Statement::Assign {
                target: AssignTarget::Register(0),
                value: Expression::Call {
                    callee: Box::new(Expression::Value(Value::Variable("fetch".to_string()))),
                    arguments: vec![],
                },
            },
            Statement::Assign {
                target: AssignTarget::Register(1),
                value: Expression::Call {
                    callee: Box::new(Expression::Value(Value::Variable("fetch".to_string()))),
                    arguments: vec![],
                },
            },
        ];

        let result = infer_variable_names(stmts);

        let names: Vec<_> = result.iter().filter_map(|s| {
            if let Statement::Assign { target: AssignTarget::Variable(n), .. } = s {
                Some(n.clone())
            } else {
                None
            }
        }).collect();

        assert_eq!(names.len(), 2);
        assert_ne!(names[0], names[1]);
    }
}
