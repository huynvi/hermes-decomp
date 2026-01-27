// Expression and statement simplification.

use crate::ir::{Expression, Statement, Constant, Value, BinaryOp, UnaryOp};

// Simplify an expression (constant folding, identity elimination).
pub fn simplify_expr(expr: &Expression) -> Expression {
    match expr {
        Expression::Binary { op, left, right } => {
            let left = simplify_expr(left);
            let right = simplify_expr(right);
            simplify_binary(*op, left, right)
        }
        Expression::Unary { op, operand } => {
            let operand = simplify_expr(operand);
            simplify_unary(*op, operand)
        }
        Expression::Conditional { condition, then_expr, else_expr } => {
            let cond = simplify_expr(condition);
            match &cond {
                Expression::Value(Value::Constant(Constant::Bool(true))) => {
                    simplify_expr(then_expr)
                }
                Expression::Value(Value::Constant(Constant::Bool(false))) => {
                    simplify_expr(else_expr)
                }
                _ => Expression::Conditional {
                    condition: Box::new(cond),
                    then_expr: Box::new(simplify_expr(then_expr)),
                    else_expr: Box::new(simplify_expr(else_expr)),
                },
            }
        }
        Expression::Yield { value, delegate } => Expression::Yield {
            value: Box::new(simplify_expr(value)),
            delegate: *delegate,
        },
        Expression::Await(value) => Expression::Await(Box::new(simplify_expr(value))),
        Expression::Call { callee, arguments } => Expression::Call {
            callee: Box::new(simplify_expr(callee)),
            arguments: arguments.iter().map(simplify_expr).collect(),
        },
        Expression::Member { object, property, optional } => Expression::Member {
            object: Box::new(simplify_expr(object)),
            property: property.clone(),
            optional: *optional,
        },
        _ => expr.clone(),
    }
}

// Simplify a statement.
pub fn simplify_stmt(stmt: &Statement) -> Statement {
    match stmt {
        Statement::Expr(e) => Statement::Expr(simplify_expr(e)),
        Statement::Let { name, value, kind } => Statement::Let {
            name: name.clone(),
            value: simplify_expr(value),
            kind: *kind,
        },
        Statement::Assign { target, value } => Statement::Assign {
            target: target.clone(),
            value: simplify_expr(value),
        },
        Statement::Return(Some(e)) => Statement::Return(Some(simplify_expr(e))),
        Statement::Throw(e) => Statement::Throw(simplify_expr(e)),
        Statement::If { condition, then_body, else_body } => Statement::If {
            condition: simplify_expr(condition),
            then_body: then_body.iter().map(simplify_stmt).collect(),
            else_body: else_body.iter().map(simplify_stmt).collect(),
        },
        Statement::While { condition, body } => Statement::While {
            condition: simplify_expr(condition),
            body: body.iter().map(simplify_stmt).collect(),
        },
        Statement::Block(stmts) => Statement::Block(stmts.iter().map(simplify_stmt).collect()),
        _ => stmt.clone(),
    }
}

fn simplify_binary(op: BinaryOp, left: Expression, right: Expression) -> Expression {
    // Constant folding for integers
    if let (
        Expression::Value(Value::Constant(Constant::Integer(l))),
        Expression::Value(Value::Constant(Constant::Integer(r))),
    ) = (&left, &right) {
        if let Some(result) = fold_int_binary(op, *l, *r) {
            return Expression::constant(Constant::Integer(result));
        }
    }

    // Identity simplifications
    match op {
        BinaryOp::Add if is_zero(&right) => return left,
        BinaryOp::Add if is_zero(&left) => return right,
        BinaryOp::Sub if is_zero(&right) => return left,
        BinaryOp::Mul if is_one(&right) => return left,
        BinaryOp::Mul if is_one(&left) => return right,
        BinaryOp::Mul if is_zero(&left) || is_zero(&right) => {
            return Expression::constant(Constant::Integer(0));
        }
        BinaryOp::Div if is_one(&right) => return left,
        _ => {}
    }

    Expression::binary(op, left, right)
}

fn simplify_unary(op: UnaryOp, operand: Expression) -> Expression {
    // Double negation
    if op == UnaryOp::Not {
        if let Expression::Unary { op: UnaryOp::Not, operand: inner } = operand {
            return *inner;
        }
    }

    // Constant folding
    if let Expression::Value(Value::Constant(c)) = &operand {
        match (op, c) {
            (UnaryOp::Not, Constant::Bool(b)) => {
                return Expression::constant(Constant::Bool(!b));
            }
            (UnaryOp::Neg, Constant::Integer(i)) => {
                return Expression::constant(Constant::Integer(-i));
            }
            _ => {}
        }
    }

    Expression::unary(op, operand)
}

fn fold_int_binary(op: BinaryOp, l: i32, r: i32) -> Option<i32> {
    match op {
        BinaryOp::Add => l.checked_add(r),
        BinaryOp::Sub => l.checked_sub(r),
        BinaryOp::Mul => l.checked_mul(r),
        BinaryOp::Div if r != 0 => l.checked_div(r),
        BinaryOp::Mod if r != 0 => l.checked_rem(r),
        BinaryOp::BitAnd => Some(l & r),
        BinaryOp::BitOr => Some(l | r),
        BinaryOp::BitXor => Some(l ^ r),
        BinaryOp::Shl => Some(l << (r & 31)),
        BinaryOp::Shr => Some(l >> (r & 31)),
        _ => None,
    }
}

fn is_zero(expr: &Expression) -> bool {
    match expr {
        Expression::Value(Value::Constant(Constant::Integer(0))) => true,
        Expression::Value(Value::Constant(Constant::Number(n))) => *n == 0.0,
        _ => false,
    }
}

fn is_one(expr: &Expression) -> bool {
    match expr {
        Expression::Value(Value::Constant(Constant::Integer(1))) => true,
        Expression::Value(Value::Constant(Constant::Number(n))) => *n == 1.0,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_folding() {
        let expr = Expression::binary(
            BinaryOp::Add,
            Expression::constant(Constant::Integer(2)),
            Expression::constant(Constant::Integer(3)),
        );
        let result = simplify_expr(&expr);
        assert_eq!(result, Expression::constant(Constant::Integer(5)));
    }

    #[test]
    fn test_identity_elimination() {
        let expr = Expression::binary(
            BinaryOp::Add,
            Expression::register(0),
            Expression::constant(Constant::Integer(0)),
        );
        let result = simplify_expr(&expr);
        assert_eq!(result, Expression::register(0));
    }
}
