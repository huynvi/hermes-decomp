use crate::ir::{Statement, Expression, AssignTarget, PropertyKey, Value, Constant};

/// Extract property access pattern from a statement.
pub fn extract_property_access(stmt: &Statement) -> Option<(Expression, PropertyKey, AssignTarget)> {
    match stmt {
        Statement::Assign { target, value } => {
            if let Expression::Member { object, property, optional: false } = value {
                return Some((*object.clone(), property.clone(), target.clone()));
            }
            None
        }
        Statement::Let { name, value, .. } => {
            if let Expression::Member { object, property, optional: false } = value {
                return Some((*object.clone(), property.clone(), AssignTarget::Variable(name.clone())));
            }
            None
        }
        _ => None,
    }
}

/// Get integer index from a property key.
pub fn get_index(key: &PropertyKey) -> Option<i64> {
    match key {
        PropertyKey::Index(idx) => Some(*idx),
        PropertyKey::Computed(expr) => {
            match expr.as_ref() {
                Expression::Value(Value::Constant(Constant::Integer(i))) => Some(*i as i64),
                Expression::Value(Value::Constant(Constant::Number(n))) => {
                    if n.fract() == 0.0 && *n >= 0.0 && *n < i64::MAX as f64 {
                        Some(*n as i64)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Check if two expressions are structurally equal.
pub fn exprs_equal(a: &Expression, b: &Expression) -> bool {
    match (a, b) {
        (Expression::Value(v1), Expression::Value(v2)) => v1 == v2,
        (
            Expression::Member { object: o1, property: p1, optional: opt1 },
            Expression::Member { object: o2, property: p2, optional: opt2 }
        ) => opt1 == opt2 && property_keys_equal(p1, p2) && exprs_equal(o1, o2),
        (
            Expression::Call { callee: c1, arguments: args1 },
            Expression::Call { callee: c2, arguments: args2 }
        ) => {
            exprs_equal(c1, c2) && args1.len() == args2.len()
                && args1.iter().zip(args2.iter()).all(|(a, b)| exprs_equal(a, b))
        }
        (
            Expression::Binary { op: op1, left: l1, right: r1 },
            Expression::Binary { op: op2, left: l2, right: r2 }
        ) => op1 == op2 && exprs_equal(l1, l2) && exprs_equal(r1, r2),
        (
            Expression::Unary { op: op1, operand: o1 },
            Expression::Unary { op: op2, operand: o2 }
        ) => op1 == op2 && exprs_equal(o1, o2),
        _ => false,
    }
}

/// Check if two property keys are equal.
pub fn property_keys_equal(a: &PropertyKey, b: &PropertyKey) -> bool {
    match (a, b) {
        (PropertyKey::Ident(s1), PropertyKey::Ident(s2)) => s1 == s2,
        (PropertyKey::String(s1), PropertyKey::String(s2)) => s1 == s2,
        (PropertyKey::Index(i1), PropertyKey::Index(i2)) => i1 == i2,
        (PropertyKey::Computed(e1), PropertyKey::Computed(e2)) => exprs_equal(e1, e2),
        _ => false,
    }
}
