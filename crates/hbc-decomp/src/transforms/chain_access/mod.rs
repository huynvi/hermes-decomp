mod usage;
mod inlining;

use crate::ir::{Statement, Expression, AssignTarget};
use std::collections::HashMap;
use usage::{count_register_uses, is_chain_candidate};
use inlining::inline_chains_in_stmt;

/// Optimize member access chains by inlining single-use intermediate registers.
pub fn optimize_chain_access(stmts: Vec<Statement>) -> Vec<Statement> {
    // First pass: count uses of each register
    let mut use_count: HashMap<u32, usize> = HashMap::new();
    let mut def_map: HashMap<u32, (usize, Expression)> = HashMap::new();

    for (idx, stmt) in stmts.iter().enumerate() {
        count_register_uses(stmt, &mut use_count);

        // Track member access definitions
        if let Statement::Assign { target: AssignTarget::Register(r), value } = stmt {
            if is_chain_candidate(value) {
                def_map.insert(*r, (idx, value.clone()));
            }
        }
    }

    // Find registers that are:
    // 1. Defined with a member access
    // 2. Used exactly once
    // 3. The use is also a member access or a return/expression
    let mut to_inline: HashMap<u32, Expression> = HashMap::new();

    for (reg, (_, expr)) in &def_map {
        if use_count.get(reg).copied().unwrap_or(0) == 1 {
            to_inline.insert(*reg, expr.clone());
        }
    }

    // Second pass: inline the chains
    let mut result: Vec<Statement> = Vec::with_capacity(stmts.len());
    let mut to_remove: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // Mark definitions for removal if they will be inlined
    for (reg, (idx, _)) in &def_map {
        if to_inline.contains_key(reg) {
            to_remove.insert(*idx);
        }
    }

    for (idx, stmt) in stmts.into_iter().enumerate() {
        if to_remove.contains(&idx) {
            continue;
        }

        let new_stmt = inline_chains_in_stmt(stmt, &to_inline);
        result.push(new_stmt);
    }

    // Recursively process nested statements
    result.into_iter().map(process_nested_chains).collect()
}

fn process_nested_chains(stmt: Statement) -> Statement {
    match stmt {
        Statement::If { condition, then_body, else_body } => Statement::If {
            condition,
            then_body: optimize_chain_access(then_body),
            else_body: optimize_chain_access(else_body),
        },
        Statement::While { condition, body } => Statement::While {
            condition,
            body: optimize_chain_access(body),
        },
        Statement::DoWhile { body, condition } => Statement::DoWhile {
            body: optimize_chain_access(body),
            condition,
        },
        Statement::For { init, condition, update, body } => Statement::For {
            init: init.map(|s| Box::new(process_nested_chains(*s))),
            condition,
            update: update.map(|s| Box::new(process_nested_chains(*s))),
            body: optimize_chain_access(body),
        },
        Statement::ForOf { variable, iterable, body } => Statement::ForOf {
            variable,
            iterable,
            body: optimize_chain_access(body),
        },
        Statement::ForIn { variable, object, body } => Statement::ForIn {
            variable,
            object,
            body: optimize_chain_access(body),
        },
        Statement::TryCatch { try_body, catch_param, catch_body, finally_body } => Statement::TryCatch {
            try_body: optimize_chain_access(try_body),
            catch_param,
            catch_body: optimize_chain_access(catch_body),
            finally_body: optimize_chain_access(finally_body),
        },
        Statement::Switch { discriminant, cases, default } => Statement::Switch {
            discriminant,
            cases: cases.into_iter().map(|(e, stmts)| (e, optimize_chain_access(stmts))).collect(),
            default: default.map(optimize_chain_access),
        },
        Statement::Block(stmts) => Statement::Block(optimize_chain_access(stmts)),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Statement, Expression, Value, AssignTarget, PropertyKey};

    #[test]
    fn test_chain_access_inline() {
        // r0 = obj.a; r1 = r0.b; return r1;
        let obj = Expression::Value(Value::Variable("obj".to_string()));
        let stmts = vec![
            Statement::Assign {
                target: AssignTarget::Register(0),
                value: Expression::Member {
                    object: Box::new(obj),
                    property: PropertyKey::Ident("a".to_string()),
                    optional: false,
                },
            },
            Statement::Assign {
                target: AssignTarget::Register(1),
                value: Expression::Member {
                    object: Box::new(Expression::Value(Value::Register(0))),
                    property: PropertyKey::Ident("b".to_string()),
                    optional: false,
                },
            },
            Statement::Return(Some(Expression::Value(Value::Register(1)))),
        ];

        let result = optimize_chain_access(stmts);

        // Should be: return obj.a.b;
        assert_eq!(result.len(), 1);
        if let Statement::Return(Some(Expression::Member { object, property: PropertyKey::Ident(prop), .. })) = &result[0] {
            assert_eq!(prop, "b");
            if let Expression::Member { object: inner, property: PropertyKey::Ident(inner_prop), .. } = object.as_ref() {
                assert_eq!(inner_prop, "a");
                assert!(matches!(inner.as_ref(), Expression::Value(Value::Variable(v)) if v == "obj"));
            } else {
                panic!("Expected nested member access");
            }
        } else {
            panic!("Expected return with member chain, got: {:?}", result[0]);
        }
    }

    #[test]
    fn test_multi_use_not_inlined() {
        // r0 = obj.a; r1 = r0.b; r2 = r0.c; (r0 used twice)
        let obj = Expression::Value(Value::Variable("obj".to_string()));
        let stmts = vec![
            Statement::Assign {
                target: AssignTarget::Register(0),
                value: Expression::Member {
                    object: Box::new(obj),
                    property: PropertyKey::Ident("a".to_string()),
                    optional: false,
                },
            },
            Statement::Assign {
                target: AssignTarget::Register(1),
                value: Expression::Member {
                    object: Box::new(Expression::Value(Value::Register(0))),
                    property: PropertyKey::Ident("b".to_string()),
                    optional: false,
                },
            },
            Statement::Assign {
                target: AssignTarget::Register(2),
                value: Expression::Member {
                    object: Box::new(Expression::Value(Value::Register(0))),
                    property: PropertyKey::Ident("c".to_string()),
                    optional: false,
                },
            },
        ];

        let result = optimize_chain_access(stmts);

        // r0 should NOT be inlined because it's used twice
        // But r1 and r2 definitions should remain
        assert_eq!(result.len(), 3);
    }
}
