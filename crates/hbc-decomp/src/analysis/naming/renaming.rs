use std::collections::HashMap;
use crate::ir::{Statement, Expression, AssignTarget, Value, PropertyKey};

// Rename registers in statements using the generated names.
pub fn rename_registers(stmts: Vec<Statement>, names: &HashMap<u32, String>) -> Vec<Statement> {
    stmts.into_iter().map(|s| rename_stmt(s, names)).collect()
}

fn rename_stmt(stmt: Statement, names: &HashMap<u32, String>) -> Statement {
    match stmt {
        Statement::Assign { target, value } => Statement::Assign {
            target: rename_target(target, names),
            value: rename_expr(value, names),
        },
        Statement::Expr(e) => Statement::Expr(rename_expr(e, names)),
        Statement::Return(Some(e)) => Statement::Return(Some(rename_expr(e, names))),
        Statement::Throw(e) => Statement::Throw(rename_expr(e, names)),
        Statement::If { condition, then_body, else_body } => Statement::If {
            condition: rename_expr(condition, names),
            then_body: rename_registers(then_body, names),
            else_body: rename_registers(else_body, names),
        },
        Statement::While { condition, body } => Statement::While {
            condition: rename_expr(condition, names),
            body: rename_registers(body, names),
        },
        Statement::Block(inner) => Statement::Block(rename_registers(inner, names)),
        other => other,
    }
}

fn rename_target(target: AssignTarget, names: &HashMap<u32, String>) -> AssignTarget {
    match target {
        AssignTarget::Register(r) => {
            if let Some(name) = names.get(&r) {
                AssignTarget::Variable(name.clone())
            } else {
                AssignTarget::Register(r)
            }
        }
        AssignTarget::Member { object, property } => AssignTarget::Member {
            object: rename_expr(object, names),
            property,
        },
        AssignTarget::Index { object, key } => AssignTarget::Index {
            object: rename_expr(object, names),
            key: rename_expr(key, names),
        },
        other => other,
    }
}

fn rename_expr(expr: Expression, names: &HashMap<u32, String>) -> Expression {
    match expr {
        Expression::Value(Value::Register(r)) => {
            if let Some(name) = names.get(&r) {
                Expression::Value(Value::Variable(name.clone()))
            } else {
                Expression::Value(Value::Register(r))
            }
        }
        Expression::Binary { op, left, right } => Expression::Binary {
            op,
            left: Box::new(rename_expr(*left, names)),
            right: Box::new(rename_expr(*right, names)),
        },
        Expression::Unary { op, operand } => Expression::Unary {
            op,
            operand: Box::new(rename_expr(*operand, names)),
        },
        Expression::Call { callee, arguments } => Expression::Call {
            callee: Box::new(rename_expr(*callee, names)),
            arguments: arguments.into_iter().map(|a| rename_expr(a, names)).collect(),
        },
        Expression::Member { object, property, optional } => Expression::Member {
            object: Box::new(rename_expr(*object, names)),
            property,
            optional,
        },
        Expression::New { callee, arguments } => Expression::New {
            callee: Box::new(rename_expr(*callee, names)),
            arguments: arguments.into_iter().map(|a| rename_expr(a, names)).collect(),
        },
        Expression::Conditional { condition, then_expr, else_expr } => Expression::Conditional {
            condition: Box::new(rename_expr(*condition, names)),
            then_expr: Box::new(rename_expr(*then_expr, names)),
            else_expr: Box::new(rename_expr(*else_expr, names)),
        },
        Expression::Array { elements } => Expression::Array {
            elements: elements.into_iter().map(|e| e.map(|ex| rename_expr(ex, names))).collect(),
        },
        Expression::Object { properties } => Expression::Object {
            properties: properties.into_iter().map(|mut p| {
                p.value = rename_expr(p.value, names);
                p
            }).collect(),
        },
        other => other,
    }
}

// Rename variables in statements in-place.
pub fn rename_variables_in_stmts(stmts: &mut [Statement], renames: &HashMap<String, String>) {
    for stmt in stmts {
        rename_variables_in_stmt(stmt, renames);
    }
}

fn rename_variables_in_stmt(stmt: &mut Statement, renames: &HashMap<String, String>) {
    match stmt {
        Statement::Assign { target, value } => {
            rename_variables_in_target(target, renames);
            rename_variables_in_expr(value, renames);
        }
        Statement::Expr(e) => rename_variables_in_expr(e, renames),
        Statement::Return(Some(e)) => rename_variables_in_expr(e, renames),
        Statement::Throw(e) => rename_variables_in_expr(e, renames),
        Statement::If { condition, then_body, else_body } => {
            rename_variables_in_expr(condition, renames);
            rename_variables_in_stmts(then_body, renames);
            rename_variables_in_stmts(else_body, renames);
        }
        Statement::While { condition, body } => {
            rename_variables_in_expr(condition, renames);
            rename_variables_in_stmts(body, renames);
        }
        Statement::DoWhile { body, condition } => {
             rename_variables_in_stmts(body, renames);
             rename_variables_in_expr(condition, renames);
        }
        Statement::For { init, condition, update, body } => {
             if let Some(i) = init { rename_variables_in_stmt(i, renames); }
             if let Some(c) = condition { rename_variables_in_expr(c, renames); }
             if let Some(u) = update { rename_variables_in_stmt(u, renames); }
             rename_variables_in_stmts(body, renames);
        }
        Statement::Block(inner) | Statement::ForOf { body: inner, .. } | Statement::ForIn { body: inner, .. } => {
             rename_variables_in_stmts(inner, renames);
        }
        Statement::Switch { discriminant, cases, default } => {
            rename_variables_in_expr(discriminant, renames);
            for (val, body) in cases {
                rename_variables_in_expr(val, renames);
                rename_variables_in_stmts(body, renames);
            }
            if let Some(d) = default {
                 rename_variables_in_stmts(d, renames);
            }
        }
        _ => {}
    }
}

fn rename_variables_in_target(target: &mut AssignTarget, renames: &HashMap<String, String>) {
    match target {
        AssignTarget::Variable(name) => {
            if let Some(new_name) = renames.get(name) {
                *name = new_name.clone();
            }
        }
        AssignTarget::Member { object, .. } => rename_variables_in_expr(object, renames),
        AssignTarget::Index { object, key } => {
            rename_variables_in_expr(object, renames);
            rename_variables_in_expr(key, renames);
        }
        _ => {}
    }
}

fn rename_variables_in_expr(expr: &mut Expression, renames: &HashMap<String, String>) {
    match expr {
        Expression::Value(Value::Variable(name)) => {
            if let Some(new_name) = renames.get(name) {
                *name = new_name.clone();
            }
        }
        Expression::Binary { left, right, .. } => {
            rename_variables_in_expr(left, renames);
            rename_variables_in_expr(right, renames);
        }
        Expression::Unary { operand, .. } => rename_variables_in_expr(operand, renames),
        Expression::Call { callee, arguments } => {
            rename_variables_in_expr(callee, renames);
            for arg in arguments {
                rename_variables_in_expr(arg, renames);
            }
        }
        Expression::Member { object, property, .. } => {
             rename_variables_in_expr(object, renames);
             if let PropertyKey::Computed(k) = property {
                 rename_variables_in_expr(k, renames);
             }
        }
        Expression::New { callee, arguments } => {
            rename_variables_in_expr(callee, renames);
            for arg in arguments {
                rename_variables_in_expr(arg, renames);
            }
        }
        Expression::Conditional { condition, then_expr, else_expr } => {
            rename_variables_in_expr(condition, renames);
            rename_variables_in_expr(then_expr, renames);
            rename_variables_in_expr(else_expr, renames);
        }
        Expression::Array { elements } => {
            for elem in elements.iter_mut().flatten() {
                 rename_variables_in_expr(elem, renames);
            }
        }
        Expression::Object { properties } => {
             for prop in properties {
                 rename_variables_in_expr(&mut prop.value, renames);
                 if let PropertyKey::Computed(k) = &mut prop.key {
                     rename_variables_in_expr(k, renames);
                 }
             }
        }
        Expression::Assignment { target, value } => {
            rename_variables_in_expr(target, renames);
            rename_variables_in_expr(value, renames);
        }
        _ => {}
    }
}
