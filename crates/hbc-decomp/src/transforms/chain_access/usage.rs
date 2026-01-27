use crate::ir::{Statement, Expression, Value, AssignTarget, PropertyKey};
use std::collections::HashMap;

pub fn is_chain_candidate(expr: &Expression) -> bool {
    matches!(expr, Expression::Member { .. })
}

pub fn count_register_uses(stmt: &Statement, counts: &mut HashMap<u32, usize>) {
    match stmt {
        Statement::Assign { target, value } => {
            count_in_target(target, counts);
            count_in_expr(value, counts);
        }
        Statement::Let { value, .. } => count_in_expr(value, counts),
        Statement::Return(Some(e)) => count_in_expr(e, counts),
        Statement::Throw(e) => count_in_expr(e, counts),
        Statement::Expr(e) => count_in_expr(e, counts),
        Statement::If { condition, then_body, else_body } => {
            count_in_expr(condition, counts);
            for s in then_body {
                count_register_uses(s, counts);
            }
            for s in else_body {
                count_register_uses(s, counts);
            }
        }
        Statement::While { condition, body } | Statement::DoWhile { body, condition } => {
            count_in_expr(condition, counts);
            for s in body {
                count_register_uses(s, counts);
            }
        }
        Statement::For { init, condition, update, body } => {
            if let Some(s) = init {
                count_register_uses(s, counts);
            }
            if let Some(e) = condition {
                count_in_expr(e, counts);
            }
            if let Some(s) = update {
                count_register_uses(s, counts);
            }
            for s in body {
                count_register_uses(s, counts);
            }
        }
        Statement::ForOf { iterable, body, .. } => {
            count_in_expr(iterable, counts);
            for s in body {
                count_register_uses(s, counts);
            }
        }
        Statement::ForIn { object, body, .. } => {
            count_in_expr(object, counts);
            for s in body {
                count_register_uses(s, counts);
            }
        }
        Statement::Switch { discriminant, cases, default } => {
            count_in_expr(discriminant, counts);
            for (expr, stmts) in cases {
                count_in_expr(expr, counts);
                for s in stmts {
                    count_register_uses(s, counts);
                }
            }
            if let Some(stmts) = default {
                for s in stmts {
                    count_register_uses(s, counts);
                }
            }
        }
        Statement::TryCatch { try_body, catch_body, finally_body, .. } => {
            for s in try_body {
                count_register_uses(s, counts);
            }
            for s in catch_body {
                count_register_uses(s, counts);
            }
            for s in finally_body {
                count_register_uses(s, counts);
            }
        }
        Statement::Block(stmts) => {
            for s in stmts {
                count_register_uses(s, counts);
            }
        }
        _ => {}
    }
}

fn count_in_target(target: &AssignTarget, counts: &mut HashMap<u32, usize>) {
    match target {
        AssignTarget::Member { object, .. } => count_in_expr(object, counts),
        AssignTarget::Index { object, key } => {
            count_in_expr(object, counts);
            count_in_expr(key, counts);
        }
        _ => {}
    }
}

fn count_in_expr(expr: &Expression, counts: &mut HashMap<u32, usize>) {
    match expr {
        Expression::Value(Value::Register(r)) => {
            *counts.entry(*r).or_insert(0) += 1;
        }
        Expression::Binary { left, right, .. } => {
            count_in_expr(left, counts);
            count_in_expr(right, counts);
        }
        Expression::Unary { operand, .. } => count_in_expr(operand, counts),
        Expression::Call { callee, arguments } => {
            count_in_expr(callee, counts);
            for arg in arguments {
                count_in_expr(arg, counts);
            }
        }
        Expression::New { callee, arguments } => {
            count_in_expr(callee, counts);
            for arg in arguments {
                count_in_expr(arg, counts);
            }
        }
        Expression::Member { object, property, .. } => {
            count_in_expr(object, counts);
            if let PropertyKey::Computed(e) = property {
                count_in_expr(e, counts);
            }
        }
        Expression::Array { elements } => {
            for elem in elements.iter().flatten() {
                count_in_expr(elem, counts);
            }
        }
        Expression::Object { properties } => {
            for prop in properties {
                count_in_expr(&prop.value, counts);
                if let PropertyKey::Computed(e) = &prop.key {
                    count_in_expr(e, counts);
                }
            }
        }
        Expression::Conditional { condition, then_expr, else_expr } => {
            count_in_expr(condition, counts);
            count_in_expr(then_expr, counts);
            count_in_expr(else_expr, counts);
        }
        Expression::Assignment { target, value } => {
            count_in_expr(target, counts);
            count_in_expr(value, counts);
        }
        Expression::Spread(inner) => count_in_expr(inner, counts),
        Expression::Await(inner) => count_in_expr(inner, counts),
        Expression::Yield { value, .. } => {
            count_in_expr(value, counts);
        }
        Expression::TemplateLiteral { expressions, .. } => {
            for e in expressions {
                count_in_expr(e, counts);
            }
        }
        _ => {}
    }
}
