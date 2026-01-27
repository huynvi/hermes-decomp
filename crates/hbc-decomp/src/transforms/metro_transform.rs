// Transform Metro bundle patterns into readable require calls.
//
// Handles:
// - Dependency map access: `deps[0]` -> `ID`
// - Require calls: `require(undef, ID)` -> `require(ID)`

use crate::ir::{Statement, Expression, Value, AssignTarget, Constant, PropertyKey};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
enum Origin {
    Param, // Param index
    Dep(u32),   // Module ID (resolved from dependencies[k])
}

// Transform Metro require calls in a module factory function.
pub fn transform_metro_require(stmts: &mut [Statement], dependencies: &[u32]) {
    // 1. Identify which registers hold 'require' and 'dependencyMap'
    // We look for patterns like:
    //   t1 = argN
    //   t2 = t1[k] -> Origin::Dep(dependencies[k])
    //   call(argM, ..., t2)

    let mut reg_origin: HashMap<u32, Origin> = HashMap::new();

    // Iterate through statements
    for stmt in stmts.iter_mut() {
        if let Statement::Assign { target: AssignTarget::Register(dst), value } = stmt {
            // Update origin tracking first based on the value

            let mut is_require = false;
            let mut module_id = None;

            // Check for require call
            if let Expression::Call { callee, arguments } = value {
                 // Check origin of callee (looking for param origin)
                 if let Expression::Value(Value::Register(r)) = **callee {
                     if let Some(Origin::Param) = reg_origin.get(&r) {
                         // Check args: [undef, Dep(id)]
                         if arguments.len() >= 2 {
                             // arg 0 is undefined/context
                             // arg 1 is module ID
                             if let Expression::Value(Value::Register(arg_r)) = &arguments[1] {
                                 if let Some(Origin::Dep(id)) = reg_origin.get(arg_r) {
                                     is_require = true;
                                     module_id = Some(*id);
                                 }
                             }
                         }
                     }
                 }
            }

            if is_require {
                if let Some(id) = module_id {
                    // REWRITE THIS EXPRESSION to CallRequire
                    // constructing `require(id)`
                    *value = Expression::Call {
                        callee: Box::new(Expression::Value(Value::Variable("require".to_string()))),
                        arguments: vec![Expression::Value(Value::Constant(Constant::Integer(id as i32)))],
                    };
                }
            }

            // Now update tracking for the destination register
            process_assignment(dst, value, &mut reg_origin, dependencies);
        }
    }
}

fn process_assignment(
    dst: &u32,
    expr: &Expression,
    map: &mut HashMap<u32, Origin>,
    deps: &[u32]
) {
    match expr {
        Expression::Value(Value::Variable(name)) => {
             // Handle "argN"
             if name.starts_with("arg")
                 && name[3..].parse::<u32>().is_ok() {
                     map.insert(*dst, Origin::Param);
                 }
        }
        Expression::Member { object, property, .. } => {
             // Access dependencies: Param(N)[Const(K)]
             if let Expression::Value(Value::Register(obj_reg)) = **object {
                 if let Some(Origin::Param) = map.get(&obj_reg) { 
                     if let PropertyKey::Index(k) = property {
                         // deps[k]
                         if (*k as usize) < deps.len() {
                             let dep_id = deps[*k as usize];
                             map.insert(*dst, Origin::Dep(dep_id));
                         }
                     }
                 }
             }
        }
        // Direct register copy
        Expression::Value(Value::Register(src)) => {
            if let Some(orig) = map.get(src) {
                map.insert(*dst, *orig);
            }
        }
        _ => {}
    }
}
