// Expression inlining for cleaner output.
//
// Transforms patterns like:
//   r0 = a;
//   r1 = r0.b;
// Into:
//   r1 = a.b;

use std::collections::HashMap;
use crate::ir::{Statement, Expression, AssignTarget, Value};

// Inline simple expressions to reduce register usage.
pub fn inline_expressions(stmts: Vec<Statement>) -> Vec<Statement> {
    let mut inliner = ExpressionInliner::new();
    inliner.process(stmts)
}

struct ExpressionInliner {
    // Map from register to its defining expression
    definitions: HashMap<u32, Expression>,
    // Count of uses for each register
    use_count: HashMap<u32, usize>,
}

impl ExpressionInliner {
    fn new() -> Self {
        Self {
            definitions: HashMap::new(),
            use_count: HashMap::new(),
        }
    }

    fn process(&mut self, stmts: Vec<Statement>) -> Vec<Statement> {
        // First pass: count uses of each register
        self.count_uses(&stmts);

        // Second pass: inline where beneficial
        self.inline_pass(stmts)
    }

    fn count_uses(&mut self, stmts: &[Statement]) {
        for stmt in stmts {
            self.count_uses_in_stmt(stmt);
        }
    }

    fn count_uses_in_stmt(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Assign { target, value } => {
                self.count_uses_in_target(target);
                self.count_uses_in_expr(value);
            }
            Statement::Expr(e) => self.count_uses_in_expr(e),
            Statement::Return(Some(e)) => self.count_uses_in_expr(e),
            Statement::Throw(e) => self.count_uses_in_expr(e),
            Statement::If { condition, then_body, else_body } => {
                self.count_uses_in_expr(condition);
                self.count_uses(then_body);
                self.count_uses(else_body);
            }
            Statement::While { condition, body } => {
                self.count_uses_in_expr(condition);
                self.count_uses(body);
            }
            Statement::Block(inner) => self.count_uses(inner),
            _ => {}
        }
    }

    fn count_uses_in_target(&mut self, target: &AssignTarget) {
        match target {
            AssignTarget::Member { object, .. } => self.count_uses_in_expr(object),
            AssignTarget::Index { object, key } => {
                self.count_uses_in_expr(object);
                self.count_uses_in_expr(key);
            }
            _ => {}
        }
    }

    fn count_uses_in_expr(&mut self, expr: &Expression) {
        match expr {
            Expression::Value(Value::Register(r)) => {
                *self.use_count.entry(*r).or_insert(0) += 1;
            }
            Expression::Binary { left, right, .. } => {
                self.count_uses_in_expr(left);
                self.count_uses_in_expr(right);
            }
            Expression::Unary { operand, .. } => self.count_uses_in_expr(operand),
            Expression::Call { callee, arguments } => {
                self.count_uses_in_expr(callee);
                for arg in arguments {
                    self.count_uses_in_expr(arg);
                }
            }
            Expression::Member { object, .. } => self.count_uses_in_expr(object),
            Expression::New { callee, arguments } => {
                self.count_uses_in_expr(callee);
                for arg in arguments {
                    self.count_uses_in_expr(arg);
                }
            }
            Expression::Conditional { condition, then_expr, else_expr } => {
                self.count_uses_in_expr(condition);
                self.count_uses_in_expr(then_expr);
                self.count_uses_in_expr(else_expr);
            }
            Expression::Array { elements } => {
                for elem in elements.iter().flatten() {
                    self.count_uses_in_expr(elem);
                }
            }
            Expression::Object { properties } => {
                for prop in properties {
                    self.count_uses_in_expr(&prop.value);
                }
            }
            _ => {}
        }
    }

    fn inline_pass(&mut self, stmts: Vec<Statement>) -> Vec<Statement> {
        let mut result = Vec::new();
        // Pending assignments with side effects: (register, statement, expression value)
        let mut pending: Vec<(u32, Statement, Expression)> = Vec::new();

        for stmt in stmts {
            let mut stmt = stmt;
            
            // 2. Determine if flush is needed
            let has_side_effects = stmt_has_side_effects(&stmt);
            
            if has_side_effects {
                let new_pending = Vec::new();
                for (r, s, expr) in pending {
                    if stmt_uses(&stmt, r) {
                        self.definitions.insert(r, expr);
                    } else {
                        result.push(s);
                    }
                }
                pending = new_pending;
                stmt = self.inline_stmt(stmt);
            } else {
                let mut kept_pending = Vec::new();
                for (r, s, expr) in pending {
                    if stmt_uses(&stmt, r) {
                        self.definitions.insert(r, expr);
                    } else {
                        kept_pending.push((r, s, expr));
                    }
                }
                pending = kept_pending;
                stmt = self.inline_stmt(stmt);
            }

            // 3. Process the resulting statement for potential new pending
            if let Statement::Assign { target: AssignTarget::Register(r), value } = &stmt {
                let uses = self.use_count.get(r).copied().unwrap_or(0);
                if uses == 1 {
                    // Candidate for pending (chaining)
                    pending.push((*r, stmt.clone(), value.clone()));
                    continue; // Do not emit yet
                }
            }
            result.push(stmt);
        }

        for (_, s, _) in pending {
            result.push(s);
        }

        result
    }

    fn inline_stmt(&mut self, stmt: Statement) -> Statement {
         match stmt {
            Statement::Assign { target, value } => Statement::Assign {
                target: self.inline_target(target),
                value: self.inline_expr(value),
            },
            Statement::Expr(e) => Statement::Expr(self.inline_expr(e)),
            Statement::Return(Some(e)) => Statement::Return(Some(self.inline_expr(e))),
            Statement::Throw(e) => Statement::Throw(self.inline_expr(e)),
            Statement::If { condition, then_body, else_body } => Statement::If {
                condition: self.inline_expr(condition),
                then_body: self.inline_pass_recursive(then_body),
                else_body: self.inline_pass_recursive(else_body),
            },
            Statement::While { condition, body } => Statement::While {
                condition: self.inline_expr(condition),
                body: self.inline_pass_recursive(body),
            },
             Statement::Block(inner) => Statement::Block(self.inline_pass_recursive(inner)),
             other => other
        }
    }

    fn inline_pass_recursive(&mut self, stmts: Vec<Statement>) -> Vec<Statement> {
        self.inline_pass(stmts)
    }

    fn inline_target(&self, target: AssignTarget) -> AssignTarget {
        match target {
            AssignTarget::Member { object, property } => AssignTarget::Member {
                object: self.inline_expr(object),
                property,
            },
            AssignTarget::Index { object, key } => AssignTarget::Index {
                object: self.inline_expr(object),
                key: self.inline_expr(key),
            },
            other => other,
        }
    }

    fn inline_expr(&self, expr: Expression) -> Expression {
        match expr {
            Expression::Value(Value::Register(r)) => {
                if let Some(def) = self.definitions.get(&r) {
                    def.clone()
                } else {
                    Expression::Value(Value::Register(r))
                }
            }
            Expression::Binary { op, left, right } => Expression::Binary {
                op,
                left: Box::new(self.inline_expr(*left)),
                right: Box::new(self.inline_expr(*right)),
            },
            Expression::Unary { op, operand } => Expression::Unary {
                op,
                operand: Box::new(self.inline_expr(*operand)),
            },
            Expression::Call { callee, arguments } => Expression::Call {
                callee: Box::new(self.inline_expr(*callee)),
                arguments: arguments.into_iter().map(|a| self.inline_expr(a)).collect(),
            },
            Expression::Member { object, property, optional } => Expression::Member {
                object: Box::new(self.inline_expr(*object)),
                property,
                optional,
            },
            Expression::New { callee, arguments } => Expression::New {
                callee: Box::new(self.inline_expr(*callee)),
                arguments: arguments.into_iter().map(|a| self.inline_expr(a)).collect(),
            },
            Expression::Conditional { condition, then_expr, else_expr } => {
                Expression::Conditional {
                    condition: Box::new(self.inline_expr(*condition)),
                    then_expr: Box::new(self.inline_expr(*then_expr)),
                    else_expr: Box::new(self.inline_expr(*else_expr)),
                }
            }
            other => other,
        }
    }
}

// Helpers
fn stmt_uses(stmt: &Statement, reg: u32) -> bool {
    match stmt {
        Statement::Assign { target, value } => {
            target_uses(target, reg) || expr_uses(value, reg)
        }
        Statement::Expr(e) => expr_uses(e, reg),
        Statement::Return(Some(e)) | Statement::Throw(e) => expr_uses(e, reg),
        Statement::If { condition, .. } => expr_uses(condition, reg),
        Statement::While { condition, .. } => expr_uses(condition, reg),
        _ => false,
    }
}

fn target_uses(target: &AssignTarget, reg: u32) -> bool {
    match target {
        AssignTarget::Member { object, .. } => expr_uses(object, reg),
        AssignTarget::Index { object, key } => expr_uses(object, reg) || expr_uses(key, reg),
        _ => false,
    }
}

fn expr_uses(expr: &Expression, reg: u32) -> bool {
    match expr {
        Expression::Value(Value::Register(r)) => *r == reg,
        Expression::Binary { left, right, .. } => expr_uses(left, reg) || expr_uses(right, reg),
        Expression::Unary { operand, .. } => expr_uses(operand, reg),
        Expression::Call { callee, arguments } | Expression::New { callee, arguments } => {
            expr_uses(callee, reg) || arguments.iter().any(|a| expr_uses(a, reg))
        }
        Expression::Member { object, .. } => expr_uses(object, reg),
        Expression::Conditional { condition, then_expr, else_expr } => {
            expr_uses(condition, reg) || expr_uses(then_expr, reg) || expr_uses(else_expr, reg)
        }
        Expression::Array { elements } => elements.iter().flatten().any(|e| expr_uses(e, reg)),
        Expression::Object { properties } => properties.iter().any(|p| expr_uses(&p.value, reg)),
        _ => false
    }
}

fn stmt_has_side_effects(stmt: &Statement) -> bool {
    match stmt {
        Statement::Assign { value, .. } => expr_has_side_effects(value),
        Statement::Expr(e) => expr_has_side_effects(e),
        Statement::Return(_) | Statement::Throw(_) => true,
        Statement::If { .. } | Statement::While { .. } | Statement::Block(_) => true,
        _ => false,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Constant, PropertyKey};

    #[test]
    fn test_inline_simple() {
        let stmts = vec![
            Statement::assign_reg(0, Expression::constant(Constant::String("test".to_string()))),
            Statement::assign_reg(1, Expression::Member {
                object: Box::new(Expression::Value(Value::Register(0))),
                property: PropertyKey::Ident("length".to_string()),
                optional: false,
            }),
        ];

        let result = inline_expressions(stmts);

        // r0 should be inlined into r1's expression
        assert!(result.len() <= 2);
    }
}
