// Final cleanup pass for cleaner output.

use crate::ir::{Statement, Expression, AssignTarget, Value, Constant};

// Apply final cleanup transformations.
pub fn cleanup_statements(stmts: Vec<Statement>) -> Vec<Statement> {
    let stmts = remove_undefined_initializations(stmts);
    let stmts = remove_redundant_assignments(stmts);
    let stmts = fold_chain_assignments(stmts);
    
    ensure_return(stmts)
}

// Fold chain assignments: `r0 = x; y = r0` → `y = x`
// Only when r0 is used exactly once immediately after.
fn fold_chain_assignments(stmts: Vec<Statement>) -> Vec<Statement> {
    let mut result = Vec::new();
    let mut iter = stmts.into_iter().peekable();

    while let Some(stmt) = iter.next() {
        match &stmt {
            Statement::Assign {
                target: AssignTarget::Register(r),
                value,
            } if !expr_has_side_effects(value) => {
                // Check if next statement uses this register as its value
                if let Some(Statement::Assign {
                    target: next_target,
                    value: Expression::Value(Value::Register(r2)),
                }) = iter.peek()
                {
                    if r == r2 && !matches!(next_target, AssignTarget::Register(_)) {
                        // Fold: skip current, modify next
                        let Some(next) = iter.next() else { continue };
                        if let Statement::Assign { target: t, .. } = next {
                            result.push(Statement::Assign {
                                target: t,
                                value: value.clone(),
                            });
                            continue;
                        }
                    }
                }
                result.push(stmt);
            }
            Statement::If { condition, then_body, else_body } => {
                result.push(Statement::If {
                    condition: condition.clone(),
                    then_body: fold_chain_assignments(then_body.clone()),
                    else_body: fold_chain_assignments(else_body.clone()),
                });
            }
            Statement::While { condition, body } => {
                result.push(Statement::While {
                    condition: condition.clone(),
                    body: fold_chain_assignments(body.clone()),
                });
            }
            Statement::Block(inner) => {
                result.push(Statement::Block(fold_chain_assignments(inner.clone())));
            }
            _ => result.push(stmt),
        }
    }

    result
}

fn expr_has_side_effects(expr: &Expression) -> bool {
    match expr {
        Expression::Call { .. } | Expression::New { .. } => true,
        Expression::Binary { left, right, .. } => {
            expr_has_side_effects(left) || expr_has_side_effects(right)
        }
        Expression::Unary { operand, .. } => expr_has_side_effects(operand),
        Expression::Member { object, .. } => expr_has_side_effects(object),
        Expression::Conditional { condition, then_expr, else_expr } => {
            expr_has_side_effects(condition)
                || expr_has_side_effects(then_expr)
                || expr_has_side_effects(else_expr)
        }
        _ => false,
    }
}

// Remove `r = undefined` when followed by another assignment to r.
fn remove_undefined_initializations(stmts: Vec<Statement>) -> Vec<Statement> {
    let mut result = Vec::new();
    let mut iter = stmts.into_iter().peekable();

    while let Some(stmt) = iter.next() {
        // Check if this is `r = undefined`
        if let Statement::Assign {
            target: AssignTarget::Register(r),
            value,
        } = &stmt
        {
            if is_undefined(value) {
                // Check if next statement assigns to same register
                if let Some(next) = iter.peek() {
                    if assigns_to_register(next, *r) {
                        // Skip this undefined assignment
                        continue;
                    }
                }
            }
        }

        // Recurse into nested structures
        let cleaned = match stmt {
            Statement::If { condition, then_body, else_body } => Statement::If {
                condition,
                then_body: remove_undefined_initializations(then_body),
                else_body: remove_undefined_initializations(else_body),
            },
            Statement::While { condition, body } => Statement::While {
                condition,
                body: remove_undefined_initializations(body),
            },
            Statement::Block(inner) => Statement::Block(remove_undefined_initializations(inner)),
            _ => stmt,
        };

        result.push(cleaned);
    }

    result
}

// Remove assignments like `r0 = r0` (self-assignment).
fn remove_redundant_assignments(stmts: Vec<Statement>) -> Vec<Statement> {
    stmts
        .into_iter()
        .filter(|stmt| {
            if let Statement::Assign {
                target: AssignTarget::Register(r),
                value: Expression::Value(Value::Register(r2)),
            } = stmt
            {
                return r != r2;
            }
            true
        })
        .map(|stmt| match stmt {
            Statement::If { condition, then_body, else_body } => Statement::If {
                condition,
                then_body: remove_redundant_assignments(then_body),
                else_body: remove_redundant_assignments(else_body),
            },
            Statement::While { condition, body } => Statement::While {
                condition,
                body: remove_redundant_assignments(body),
            },
            Statement::Block(inner) => Statement::Block(remove_redundant_assignments(inner)),
            _ => stmt,
        })
        .collect()
}

// Ensure function ends with a return statement.
fn ensure_return(mut stmts: Vec<Statement>) -> Vec<Statement> {
    if !stmts.is_empty() {
        // Check if last statement is already a return
        let needs_return = !ends_with_return(&stmts);

        if needs_return {
            stmts.push(Statement::Return(None));
        }
    }
    stmts
}

fn ends_with_return(stmts: &[Statement]) -> bool {
    if let Some(last) = stmts.last() {
        match last {
            Statement::Return(_) => true,
            Statement::Throw(_) => true,
            Statement::If { then_body, else_body, .. } => {
                ends_with_return(then_body) && ends_with_return(else_body)
            }
            Statement::While { .. } => false, // Loops may not return
            _ => false,
        }
    } else {
        false
    }
}

fn is_undefined(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::Value(Value::Constant(Constant::Undefined))
    )
}

fn assigns_to_register(stmt: &Statement, reg: u32) -> bool {
    matches!(stmt, Statement::Assign { target: AssignTarget::Register(r), .. } if *r == reg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_undefined_init() {
        let stmts = vec![
            Statement::assign_reg(0, Expression::constant(Constant::Undefined)),
            Statement::assign_reg(0, Expression::constant(Constant::Integer(42))),
        ];

        let result = remove_undefined_initializations(stmts);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_remove_self_assignment() {
        let stmts = vec![
            Statement::assign_reg(0, Expression::Value(Value::Register(0))),
            Statement::assign_reg(1, Expression::constant(Constant::Integer(42))),
        ];

        let result = remove_redundant_assignments(stmts);
        assert_eq!(result.len(), 1);
    }
}
