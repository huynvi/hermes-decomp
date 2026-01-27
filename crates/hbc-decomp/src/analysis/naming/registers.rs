use std::collections::{HashMap, HashSet};
use crate::ir::{Statement, Expression, AssignTarget, Value, PropertyKey, Constant};

// Information about a register's usage.
#[derive(Debug, Clone, Default)]
pub struct RegisterInfo {
    // Inferred type/role
    pub role: RegisterRole,
    // Properties accessed on this register
    pub accessed_props: HashSet<String>,
    // Methods called on this register
    pub called_methods: HashSet<String>,
    // If assigned from a parameter
    pub from_param: Option<u32>,
    // If assigned from a property access
    pub from_property: Option<String>,
    // Number of uses
    pub use_count: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum RegisterRole {
    #[default]
    Unknown,
    Array,
    Object,
    Function,
    String,
    Number,
    Boolean,
    BigInt,
    Iterator,
    Promise,
    This,
    Undefined,
    Null,
}

// Analyze statements to infer register roles and generate names.
pub fn analyze_registers(stmts: &[Statement]) -> HashMap<u32, RegisterInfo> {
    let mut info: HashMap<u32, RegisterInfo> = HashMap::new();

    for stmt in stmts {
        analyze_stmt(stmt, &mut info);
    }

    info
}

fn analyze_stmt(stmt: &Statement, info: &mut HashMap<u32, RegisterInfo>) {
    match stmt {
        Statement::Assign { target, value } => {
            // Track what's assigned to the register
            if let AssignTarget::Register(r) = target {
                let entry = info.entry(*r).or_default();
                infer_role_from_value(value, entry);
            }
            // Track uses in the value
            analyze_expr(value, info);
            analyze_target(target, info);
        }
        Statement::Expr(e) => analyze_expr(e, info),
        Statement::Return(Some(e)) => analyze_expr(e, info),
        Statement::Throw(e) => analyze_expr(e, info),
        Statement::If { condition, then_body, else_body } => {
            analyze_expr(condition, info);
            for s in then_body {
                analyze_stmt(s, info);
            }
            for s in else_body {
                analyze_stmt(s, info);
            }
        }
        Statement::While { condition, body } => {
            analyze_expr(condition, info);
            for s in body {
                analyze_stmt(s, info);
            }
        }
        Statement::Block(inner) => {
            for s in inner {
                analyze_stmt(s, info);
            }
        }
        _ => {}
    }
}

fn analyze_target(target: &AssignTarget, info: &mut HashMap<u32, RegisterInfo>) {
    match target {
        AssignTarget::Member { object, .. } => analyze_expr(object, info),
        AssignTarget::Index { object, key } => {
            analyze_expr(object, info);
            analyze_expr(key, info);
        }
        _ => {}
    }
}

fn analyze_expr(expr: &Expression, info: &mut HashMap<u32, RegisterInfo>) {
    match expr {
        Expression::Value(Value::Register(r)) => {
            info.entry(*r).or_default().use_count += 1;
        }
        Expression::Member { object, property, .. } => {
            // Track property access
            if let Expression::Value(Value::Register(r)) = object.as_ref() {
                let entry = info.entry(*r).or_default();
                if let PropertyKey::Ident(name) = property {
                    entry.accessed_props.insert(name.clone());
                    // Infer type from property
                    infer_role_from_property(name, entry);
                }
            }
            analyze_expr(object, info);
        }
        Expression::Call { callee, arguments } => {
            // Track method calls
            if let Expression::Member { object, property: PropertyKey::Ident(method), .. } = callee.as_ref() {
                if let Expression::Value(Value::Register(r)) = object.as_ref() {
                    info.entry(*r).or_default().called_methods.insert(method.clone());
                }
            }
            analyze_expr(callee, info);
            for arg in arguments {
                analyze_expr(arg, info);
            }
        }
        Expression::Binary { left, right, .. } => {
            analyze_expr(left, info);
            analyze_expr(right, info);
        }
        Expression::Unary { operand, .. } => analyze_expr(operand, info),
        Expression::New { callee, arguments } => {
            analyze_expr(callee, info);
            for arg in arguments {
                analyze_expr(arg, info);
            }
        }
        Expression::Array { elements } => {
            for elem in elements.iter().flatten() {
                analyze_expr(elem, info);
            }
        }
        Expression::Object { properties } => {
            for prop in properties {
                analyze_expr(&prop.value, info);
            }
        }
        Expression::Conditional { condition, then_expr, else_expr } => {
            analyze_expr(condition, info);
            analyze_expr(then_expr, info);
            analyze_expr(else_expr, info);
        }
        _ => {}
    }
}

fn infer_role_from_value(value: &Expression, info: &mut RegisterInfo) {
    match value {
        Expression::Array { .. } => info.role = RegisterRole::Array,
        Expression::Object { .. } => info.role = RegisterRole::Object,
        Expression::Function { .. } => info.role = RegisterRole::Function,
        Expression::Value(Value::Constant(c)) => {
            info.role = match c {
                Constant::String(_) => RegisterRole::String,
                Constant::Integer(_) | Constant::Number(_) => RegisterRole::Number,
                Constant::Bool(_) => RegisterRole::Boolean,
                Constant::BigInt(_) => RegisterRole::BigInt,
                Constant::Null => RegisterRole::Null,
                Constant::Undefined => RegisterRole::Undefined,
            };
        }
        Expression::Value(Value::This) => info.role = RegisterRole::This,
        Expression::Member { property: PropertyKey::Ident(name), .. } => {
            info.from_property = Some(name.clone());
        }
        _ => {}
    }
}

fn infer_role_from_property(prop: &str, info: &mut RegisterInfo) {
    match prop {
        "length" | "push" | "pop" | "shift" | "unshift" | "splice" | "slice"
        | "map" | "filter" | "reduce" | "forEach" | "find" | "indexOf" => {
            if info.role == RegisterRole::Unknown {
                info.role = RegisterRole::Array;
            }
        }
        "then" | "catch" | "finally" => {
            if info.role == RegisterRole::Unknown {
                info.role = RegisterRole::Promise;
            }
        }
        "next" | "done" | "value" => {
            if info.role == RegisterRole::Unknown {
                info.role = RegisterRole::Iterator;
            }
        }
        "toString" | "charAt" | "substring" | "substr" | "split" | "trim"
        | "toLowerCase" | "toUpperCase" | "replace" | "match" => {
            if info.role == RegisterRole::Unknown {
                info.role = RegisterRole::String;
            }
        }
        _ => {}
    }
}
