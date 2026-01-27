use crate::ir::{Expression, Value, Constant};

pub fn is_null(expr: &Expression) -> bool {
    matches!(expr, Expression::Value(Value::Constant(Constant::Null)))
}

pub fn is_undefined(expr: &Expression) -> bool {
    matches!(expr, Expression::Value(Value::Constant(Constant::Undefined)))
}

pub fn exprs_equal(a: &Expression, b: &Expression) -> bool {
    match (a, b) {
        (Expression::Value(v1), Expression::Value(v2)) => v1 == v2,
        (
            Expression::Member { object: o1, property: p1, .. },
            Expression::Member { object: o2, property: p2, .. }
        ) => exprs_equal(o1, o2) && p1 == p2,
        _ => false,
    }
}
