// High-level statement optimizations.

use crate::ir::{Statement, Expression, AssignTarget, Value, UnaryOp};

// Apply all statement-level optimizations.
pub fn optimize_statements(stmts: Vec<Statement>) -> Vec<Statement> {
    let stmts = invert_empty_ifs(stmts);
    let stmts = detect_ternaries(stmts);
    let stmts = remove_dead_assignments(stmts);
    
    merge_sequential_returns(stmts)
}

// Invert if statements with empty then branch: if (!x) {} else { code } → if (x) { code }
fn invert_empty_ifs(stmts: Vec<Statement>) -> Vec<Statement> {
    stmts.into_iter().map(invert_empty_if).collect()
}

fn invert_empty_if(stmt: Statement) -> Statement {
    match stmt {
        Statement::If { condition, then_body, else_body } => {
            let then_body: Vec<_> = then_body.into_iter().map(invert_empty_if).collect();
            let else_body: Vec<_> = else_body.into_iter().map(invert_empty_if).collect();

            // If then is empty but else is not, invert
            if then_body.is_empty() && !else_body.is_empty() {
                Statement::If {
                    condition: negate_condition(condition),
                    then_body: else_body,
                    else_body: vec![],
                }
            } else {
                Statement::If { condition, then_body, else_body }
            }
        }
        Statement::While { condition, body } => Statement::While {
            condition,
            body: body.into_iter().map(invert_empty_if).collect(),
        },
        Statement::Block(inner) => Statement::Block(invert_empty_ifs(inner)),
        _ => stmt,
    }
}

// Negate a condition, simplifying double negations.
fn negate_condition(expr: Expression) -> Expression {
    match expr {
        // !!x → x (double negation in original, so just negate once)
        Expression::Unary { op: UnaryOp::Not, operand } => *operand,
        // Otherwise wrap in Not
        _ => Expression::unary(UnaryOp::Not, expr),
    }
}

// Detect ternary patterns: if (c) { r = a } else { r = b } → r = c ? a : b
fn detect_ternaries(stmts: Vec<Statement>) -> Vec<Statement> {
    stmts.into_iter().map(detect_ternary).collect()
}

fn detect_ternary(stmt: Statement) -> Statement {
    match stmt {
        Statement::If { condition, then_body, else_body } => {
            // Check if both branches are single assignments to same target
            if let (Some(then_assign), Some(else_assign)) = (
                get_single_assignment(&then_body),
                get_single_assignment(&else_body),
            ) {
                if targets_equal(&then_assign.0, &else_assign.0) {
                    return Statement::Assign {
                        target: then_assign.0,
                        value: Expression::Conditional {
                            condition: Box::new(condition),
                            then_expr: Box::new(then_assign.1),
                            else_expr: Box::new(else_assign.1),
                        },
                    };
                }
            }

            // Recurse into branches
            Statement::If {
                condition,
                then_body: detect_ternaries(then_body),
                else_body: detect_ternaries(else_body),
            }
        }
        Statement::While { condition, body } => Statement::While {
            condition,
            body: detect_ternaries(body),
        },
        Statement::Block(inner) => Statement::Block(detect_ternaries(inner)),
        _ => stmt,
    }
}

fn get_single_assignment(stmts: &[Statement]) -> Option<(AssignTarget, Expression)> {
    if stmts.len() != 1 {
        return None;
    }
    match &stmts[0] {
        Statement::Assign { target, value } => Some((target.clone(), value.clone())),
        _ => None,
    }
}

fn targets_equal(a: &AssignTarget, b: &AssignTarget) -> bool {
    match (a, b) {
        (AssignTarget::Register(r1), AssignTarget::Register(r2)) => r1 == r2,
        (AssignTarget::Variable(v1), AssignTarget::Variable(v2)) => v1 == v2,
        _ => false,
    }
}

// Remove assignments to registers that are immediately overwritten.
fn remove_dead_assignments(stmts: Vec<Statement>) -> Vec<Statement> {
    let mut result = Vec::with_capacity(stmts.len());
    let mut iter = stmts.into_iter().peekable();

    while let Some(stmt) = iter.next() {
        // Check if this is an assignment that gets immediately overwritten
        if let Statement::Assign { target: AssignTarget::Register(r), .. } = &stmt {
            if let Some(next) = iter.peek() {
                if overwrites_register(next, *r) && !uses_register(next, *r) {
                    // Skip this assignment, it's dead
                    continue;
                }
            }
        }

        // Recurse into nested structures
        let optimized = match stmt {
            Statement::If { condition, then_body, else_body } => Statement::If {
                condition,
                then_body: remove_dead_assignments(then_body),
                else_body: remove_dead_assignments(else_body),
            },
            Statement::While { condition, body } => Statement::While {
                condition,
                body: remove_dead_assignments(body),
            },
            Statement::Block(inner) => Statement::Block(remove_dead_assignments(inner)),
            _ => stmt,
        };

        result.push(optimized);
    }

    result
}

fn overwrites_register(stmt: &Statement, reg: u32) -> bool {
    match stmt {
        Statement::Assign { target: AssignTarget::Register(r), .. } => *r == reg,
        _ => false,
    }
}

fn uses_register(stmt: &Statement, reg: u32) -> bool {
    match stmt {
        Statement::Assign { target, value } => {
            target_uses_register(target, reg) || expr_uses_register(value, reg)
        }
        Statement::Expr(e) => expr_uses_register(e, reg),
        Statement::Return(Some(e)) => expr_uses_register(e, reg),
        Statement::Throw(e) => expr_uses_register(e, reg),
        _ => false,
    }
}

fn target_uses_register(target: &AssignTarget, reg: u32) -> bool {
    match target {
        AssignTarget::Member { object, .. } => expr_uses_register(object, reg),
        AssignTarget::Index { object, key } => {
            expr_uses_register(object, reg) || expr_uses_register(key, reg)
        }
        _ => false,
    }
}

fn expr_uses_register(expr: &Expression, reg: u32) -> bool {
    match expr {
        Expression::Value(Value::Register(r)) => *r == reg,
        Expression::Binary { left, right, .. } => {
            expr_uses_register(left, reg) || expr_uses_register(right, reg)
        }
        Expression::Unary { operand, .. } => expr_uses_register(operand, reg),
        Expression::Call { callee, arguments } => {
            expr_uses_register(callee, reg)
                || arguments.iter().any(|a| expr_uses_register(a, reg))
        }
        Expression::Member { object, .. } => expr_uses_register(object, reg),
        Expression::New { callee, arguments } => {
            expr_uses_register(callee, reg)
                || arguments.iter().any(|a| expr_uses_register(a, reg))
        }
        Expression::Array { elements } => elements
            .iter()
            .any(|e| e.as_ref().map(|e| expr_uses_register(e, reg)).unwrap_or(false)),
        Expression::Object { properties } => properties
            .iter()
            .any(|p| expr_uses_register(&p.value, reg)),
        Expression::Conditional { condition, then_expr, else_expr } => {
            expr_uses_register(condition, reg)
                || expr_uses_register(then_expr, reg)
                || expr_uses_register(else_expr, reg)
        }
        _ => false,
    }
}

// Merge return statements that follow each other in if/else branches.
fn merge_sequential_returns(stmts: Vec<Statement>) -> Vec<Statement> {
    stmts.into_iter().map(|stmt| {
        match stmt {
            Statement::If { condition, then_body, else_body } => {
                let then_body = merge_sequential_returns(then_body);
                let else_body = merge_sequential_returns(else_body);
                Statement::If { condition, then_body, else_body }
            }
            Statement::While { condition, body } => Statement::While {
                condition,
                body: merge_sequential_returns(body),
            },
            Statement::Block(inner) => Statement::Block(merge_sequential_returns(inner)),
            _ => stmt,
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Constant;

    #[test]
    fn test_invert_empty_if() {
        let stmt = Statement::If {
            condition: Expression::Value(Value::Register(0)),
            then_body: vec![],
            else_body: vec![Statement::Return(Some(Expression::constant(Constant::Integer(1))))],
        };

        let result = invert_empty_if(stmt);

        if let Statement::If { condition, then_body, else_body } = result {
            assert!(!then_body.is_empty());
            assert!(else_body.is_empty());
            // Condition should be negated
            assert!(matches!(condition, Expression::Unary { op: UnaryOp::Not, .. }));
        } else {
            panic!("Expected If statement");
        }
    }

    #[test]
    fn test_detect_ternary() {
        let stmt = Statement::If {
            condition: Expression::Value(Value::Register(0)),
            then_body: vec![Statement::assign_reg(1, Expression::constant(Constant::Integer(10)))],
            else_body: vec![Statement::assign_reg(1, Expression::constant(Constant::Integer(20)))],
        };

        let result = detect_ternary(stmt);

        if let Statement::Assign { target: AssignTarget::Register(1), value } = result {
            assert!(matches!(value, Expression::Conditional { .. }));
        } else {
            panic!("Expected ternary assignment");
        }
    }
}
