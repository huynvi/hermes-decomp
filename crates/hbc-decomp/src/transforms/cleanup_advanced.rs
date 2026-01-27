// Advanced cleanup transformations.
//
// This module provides more aggressive cleanup passes to improve code quality:
// This module provides more aggressive cleanup passes to improve code quality:
// 1. **Redundant Assignment Elimination**: `x = y; x = z;` -> `x = z;` (if `x` unused in `z`).
// 2. **Inline Single-Use Temporaries**: `t = expr; use(t);` -> `use(expr);` (avoids clutter).
// 3. **Dead Assignment Elimination**: `x = expr;` where `x` is never read -> remove (if no side effects).
//
// These passes are essential after SSA/Structure recovery generated verbose code.

use crate::ir::{Statement, Expression, Value, AssignTarget};
use std::collections::{HashMap, HashSet};

/// Apply advanced cleanup transformations.
pub fn cleanup_advanced(stmts: Vec<Statement>) -> Vec<Statement> {
    let mut stmts = stmts;

    // Pass 1: Remove redundant consecutive assignments to same register
    remove_redundant_assignments(&mut stmts);

    // Pass 2: Inline single-use temporaries
    inline_single_use(&mut stmts);

    // Pass 3: Remove dead assignments (assigned but never read)
    remove_dead_assignments(&mut stmts);

    stmts
}

/// Remove redundant assignments where same target is assigned multiple times
/// without the value being used in between.
fn remove_redundant_assignments(stmts: &mut Vec<Statement>) {
    let mut i = 0;
    while i < stmts.len() {
        // Look for pattern: r = x; r = y; (where r is not used in y)
        if i + 1 < stmts.len() {
            if let (
                Statement::Assign { target: t1, value: _ },
                Statement::Assign { target: t2, value: v2 }
            ) = (&stmts[i], &stmts[i + 1]) {
                if targets_equal(t1, t2) && !expr_uses_target(v2, t1) {
                    // The first assignment is dead, remove it
                    stmts.remove(i);
                    continue; // Don't increment, check new position
                }
            }
        }
        i += 1;
    }
}

/// Inline temporaries that are only used once.
fn inline_single_use(stmts: &mut Vec<Statement>) {
    // Count uses of each register
    let mut use_count: HashMap<u32, usize> = HashMap::new();
    let mut def_value: HashMap<u32, Expression> = HashMap::new();
    let mut def_index: HashMap<u32, usize> = HashMap::new();

    // First pass: collect definitions and count uses
    for (idx, stmt) in stmts.iter().enumerate() {
        match stmt {
            Statement::Assign { target: AssignTarget::Register(r), value } => {
                def_value.insert(*r, value.clone());
                def_index.insert(*r, idx);
                count_uses(value, &mut use_count);
            }
            Statement::Assign { target: _, value } => {
                count_uses(value, &mut use_count);
            }
            Statement::Return(Some(e)) => count_uses(e, &mut use_count),
            Statement::Throw(e) => count_uses(e, &mut use_count),
            Statement::Expr(e) => count_uses(e, &mut use_count),
            Statement::If { condition, then_body, else_body } => {
                count_uses(condition, &mut use_count);
                for s in then_body {
                    count_uses_stmt(s, &mut use_count);
                }
                for s in else_body {
                    count_uses_stmt(s, &mut use_count);
                }
            }
            _ => {}
        }
    }

    // Find registers used exactly once and defined with simple expressions
    let mut to_inline: HashSet<u32> = HashSet::new();
    for (reg, count) in &use_count {
        if *count == 1 {
            if let Some(value) = def_value.get(reg) {
                // Only inline simple values (not complex expressions that might have side effects)
                if is_simple_value(value) {
                    to_inline.insert(*reg);
                }
            }
        }
    }

    // Second pass: inline and mark definitions for removal
    let mut to_remove: HashSet<usize> = HashSet::new();

    for (reg, idx) in &def_index {
        if to_inline.contains(reg) {
            to_remove.insert(*idx);
        }
    }

    // Apply inlining
    for stmt in stmts.iter_mut() {
        inline_in_stmt(stmt, &to_inline, &def_value);
    }

    // Remove inlined definitions (in reverse order to preserve indices)
    let mut indices: Vec<usize> = to_remove.into_iter().collect();
    indices.sort_by(|a, b| b.cmp(a));
    for idx in indices {
        if idx < stmts.len() {
            stmts.remove(idx);
        }
    }
}

/// Remove assignments where the value is never used.
fn remove_dead_assignments(stmts: &mut Vec<Statement>) {
    // Collect all used registers
    let mut used: HashSet<u32> = HashSet::new();

    for stmt in stmts.iter() {
        collect_used_registers(stmt, &mut used);
    }

    // Remove assignments to unused registers (but keep side-effectful expressions)
    stmts.retain(|stmt| {
        if let Statement::Assign { target: AssignTarget::Register(r), value } = stmt {
            if !used.contains(r) && !has_side_effects(value) {
                return false;
            }
        }
        true
    });
}

fn targets_equal(t1: &AssignTarget, t2: &AssignTarget) -> bool {
    match (t1, t2) {
        (AssignTarget::Register(r1), AssignTarget::Register(r2)) => r1 == r2,
        (AssignTarget::Variable(v1), AssignTarget::Variable(v2)) => v1 == v2,
        _ => false,
    }
}

fn expr_uses_target(expr: &Expression, target: &AssignTarget) -> bool {
    match target {
        AssignTarget::Register(r) => expr_uses_reg(expr, *r),
        AssignTarget::Variable(v) => expr_uses_var(expr, v),
        _ => true, // Conservative: assume it might be used
    }
}

fn expr_uses_reg(expr: &Expression, reg: u32) -> bool {
    match expr {
        Expression::Value(Value::Register(r)) => *r == reg,
        Expression::Binary { left, right, .. } => {
            expr_uses_reg(left, reg) || expr_uses_reg(right, reg)
        }
        Expression::Unary { operand, .. } => expr_uses_reg(operand, reg),
        Expression::Call { callee, arguments } => {
            expr_uses_reg(callee, reg) || arguments.iter().any(|a| expr_uses_reg(a, reg))
        }
        Expression::Member { object, .. } => expr_uses_reg(object, reg),
        Expression::Array { elements } => {
            elements.iter().any(|e| e.as_ref().is_some_and(|e| expr_uses_reg(e, reg)))
        }
        Expression::Object { properties } => {
            properties.iter().any(|p| expr_uses_reg(&p.value, reg))
        }
        Expression::Conditional { condition, then_expr, else_expr } => {
            expr_uses_reg(condition, reg) || expr_uses_reg(then_expr, reg) || expr_uses_reg(else_expr, reg)
        }
        _ => false,
    }
}

fn expr_uses_var(expr: &Expression, var: &str) -> bool {
    match expr {
        Expression::Value(Value::Variable(v)) => v == var,
        Expression::Binary { left, right, .. } => {
            expr_uses_var(left, var) || expr_uses_var(right, var)
        }
        Expression::Unary { operand, .. } => expr_uses_var(operand, var),
        Expression::Call { callee, arguments } => {
            expr_uses_var(callee, var) || arguments.iter().any(|a| expr_uses_var(a, var))
        }
        Expression::Member { object, .. } => expr_uses_var(object, var),
        _ => false,
    }
}

fn count_uses(expr: &Expression, counts: &mut HashMap<u32, usize>) {
    match expr {
        Expression::Value(Value::Register(r)) => {
            *counts.entry(*r).or_insert(0) += 1;
        }
        Expression::Binary { left, right, .. } => {
            count_uses(left, counts);
            count_uses(right, counts);
        }
        Expression::Unary { operand, .. } => count_uses(operand, counts),
        Expression::Call { callee, arguments } => {
            count_uses(callee, counts);
            for arg in arguments {
                count_uses(arg, counts);
            }
        }
        Expression::Member { object, .. } => count_uses(object, counts),
        Expression::Array { elements } => {
            for elem in elements.iter().flatten() {
                count_uses(elem, counts);
            }
        }
        Expression::Object { properties } => {
            for prop in properties {
                count_uses(&prop.value, counts);
            }
        }
        Expression::Conditional { condition, then_expr, else_expr } => {
            count_uses(condition, counts);
            count_uses(then_expr, counts);
            count_uses(else_expr, counts);
        }
        _ => {}
    }
}

fn count_uses_stmt(stmt: &Statement, counts: &mut HashMap<u32, usize>) {
    match stmt {
        Statement::Assign { value, .. } => count_uses(value, counts),
        Statement::Return(Some(e)) => count_uses(e, counts),
        Statement::Throw(e) => count_uses(e, counts),
        Statement::Expr(e) => count_uses(e, counts),
        _ => {}
    }
}

fn is_simple_value(expr: &Expression) -> bool {
    matches!(expr,
        Expression::Value(Value::Constant(_)) |
        Expression::Value(Value::Register(_)) |
        Expression::Value(Value::Variable(_)) |
        Expression::Value(Value::This) |
        Expression::Value(Value::Global)
    )
}

fn inline_in_stmt(stmt: &mut Statement, to_inline: &HashSet<u32>, values: &HashMap<u32, Expression>) {
    match stmt {
        Statement::Assign { value, .. } => {
            *value = inline_in_expr(value.clone(), to_inline, values);
        }
        Statement::Return(Some(e)) => {
            *e = inline_in_expr(e.clone(), to_inline, values);
        }
        Statement::Throw(e) => {
            *e = inline_in_expr(e.clone(), to_inline, values);
        }
        Statement::Expr(e) => {
            *e = inline_in_expr(e.clone(), to_inline, values);
        }
        Statement::If { condition, then_body, else_body } => {
            *condition = inline_in_expr(condition.clone(), to_inline, values);
            for s in then_body {
                inline_in_stmt(s, to_inline, values);
            }
            for s in else_body {
                inline_in_stmt(s, to_inline, values);
            }
        }
        _ => {}
    }
}

fn inline_in_expr(expr: Expression, to_inline: &HashSet<u32>, values: &HashMap<u32, Expression>) -> Expression {
    match expr {
        Expression::Value(Value::Register(r)) if to_inline.contains(&r) => {
            values.get(&r).cloned().unwrap_or(Expression::Value(Value::Register(r)))
        }
        Expression::Binary { op, left, right } => Expression::Binary {
            op,
            left: Box::new(inline_in_expr(*left, to_inline, values)),
            right: Box::new(inline_in_expr(*right, to_inline, values)),
        },
        Expression::Unary { op, operand } => Expression::Unary {
            op,
            operand: Box::new(inline_in_expr(*operand, to_inline, values)),
        },
        Expression::Call { callee, arguments } => Expression::Call {
            callee: Box::new(inline_in_expr(*callee, to_inline, values)),
            arguments: arguments.into_iter().map(|a| inline_in_expr(a, to_inline, values)).collect(),
        },
        Expression::Member { object, property, optional } => Expression::Member {
            object: Box::new(inline_in_expr(*object, to_inline, values)),
            property,
            optional,
        },
        Expression::Conditional { condition, then_expr, else_expr } => Expression::Conditional {
            condition: Box::new(inline_in_expr(*condition, to_inline, values)),
            then_expr: Box::new(inline_in_expr(*then_expr, to_inline, values)),
            else_expr: Box::new(inline_in_expr(*else_expr, to_inline, values)),
        },
        other => other,
    }
}

fn collect_used_registers(stmt: &Statement, used: &mut HashSet<u32>) {
    match stmt {
        Statement::Assign { target, value } => {
            // The target register is being defined, not used
            // But if it's a member/index, the base is used
            match target {
                AssignTarget::Member { object, .. } => collect_used_in_expr(object, used),
                AssignTarget::Index { object, key } => {
                    collect_used_in_expr(object, used);
                    collect_used_in_expr(key, used);
                }
                _ => {}
            }
            collect_used_in_expr(value, used);
        }
        Statement::Return(Some(e)) => collect_used_in_expr(e, used),
        Statement::Throw(e) => collect_used_in_expr(e, used),
        Statement::Expr(e) => collect_used_in_expr(e, used),
        Statement::If { condition, then_body, else_body } => {
            collect_used_in_expr(condition, used);
            for s in then_body {
                collect_used_registers(s, used);
            }
            for s in else_body {
                collect_used_registers(s, used);
            }
        }
        Statement::While { condition, body } => {
            collect_used_in_expr(condition, used);
            for s in body {
                collect_used_registers(s, used);
            }
        }
        Statement::For { init, condition, update, body } => {
            if let Some(s) = init {
                collect_used_registers(s, used);
            }
            if let Some(e) = condition {
                collect_used_in_expr(e, used);
            }
            if let Some(s) = update {
                collect_used_registers(s, used);
            }
            for s in body {
                collect_used_registers(s, used);
            }
        }
        _ => {}
    }
}

fn collect_used_in_expr(expr: &Expression, used: &mut HashSet<u32>) {
    match expr {
        Expression::Value(Value::Register(r)) => { used.insert(*r); }
        Expression::Binary { left, right, .. } => {
            collect_used_in_expr(left, used);
            collect_used_in_expr(right, used);
        }
        Expression::Unary { operand, .. } => collect_used_in_expr(operand, used),
        Expression::Call { callee, arguments } => {
            collect_used_in_expr(callee, used);
            for arg in arguments {
                collect_used_in_expr(arg, used);
            }
        }
        Expression::Member { object, .. } => collect_used_in_expr(object, used),
        Expression::Array { elements } => {
            for elem in elements.iter().flatten() {
                collect_used_in_expr(elem, used);
            }
        }
        Expression::Object { properties } => {
            for prop in properties {
                collect_used_in_expr(&prop.value, used);
            }
        }
        Expression::Conditional { condition, then_expr, else_expr } => {
            collect_used_in_expr(condition, used);
            collect_used_in_expr(then_expr, used);
            collect_used_in_expr(else_expr, used);
        }
        _ => {}
    }
}

fn has_side_effects(expr: &Expression) -> bool {
    match expr {
        Expression::Call { .. } => true,
        Expression::New { .. } => true,
        Expression::Assignment { .. } => true,
        Expression::Binary { left, right, .. } => {
            has_side_effects(left) || has_side_effects(right)
        }
        Expression::Unary { operand, .. } => has_side_effects(operand),
        Expression::Conditional { condition, then_expr, else_expr } => {
            has_side_effects(condition) || has_side_effects(then_expr) || has_side_effects(else_expr)
        }
        _ => false,
    }
}
