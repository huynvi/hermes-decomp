use std::collections::HashMap;
use crate::ir::{Statement, Expression, AssignTarget, Value, PropertyKey};
use super::graph::CallGraph;
use super::inference::collect_param_names_from_expr;
use crate::analysis::metro::registry::MetroRegistry;

#[derive(Clone, Copy)]
enum Definition {
    Function(u32),
    Parameter(u32),
    Module(u32),
}

/// Index of function names to their IDs for resolving method calls like obj.loginWithToken()
pub type FunctionNameIndex = HashMap<String, u32>;

pub fn collect_info(
    caller_id: u32,
    stmts: &[Statement],
    graph: &mut CallGraph,
    call_sites: &mut HashMap<u32, Vec<Vec<Option<String>>>>,
    self_param_names: &mut HashMap<u32, Vec<Vec<Option<String>>>>,
    param_links: &mut Vec<((u32, u32), (u32, u32))>,
    metro_registry: &MetroRegistry,
    func_name_index: &FunctionNameIndex,
) {
    let mut defs = HashMap::new();
    for stmt in stmts {
        collect_definitions(stmt, &mut defs);
    }

    for stmt in stmts {
        visit_stmt_for_calls(stmt, caller_id, &defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index);
    }
}

fn collect_definitions(stmt: &Statement, defs: &mut HashMap<String, Definition>) {
    match stmt {
        Statement::Assign { target, value } => {
            if let Some(key) = target_to_key(target) {
                collect_value_definition(&key, value, defs);
            }
        }
        Statement::Let { name, value, .. } => {
            collect_value_definition(name, value, defs);
        }
        Statement::Block(stmts) => {
            for s in stmts {
                collect_definitions(s, defs);
            }
        }
        Statement::If { then_body, else_body, .. } => {
            for s in then_body {
                collect_definitions(s, defs);
            }
            for s in else_body {
                collect_definitions(s, defs);
            }
        }
        _ => {}
    }
}

fn collect_value_definition(key: &str, value: &Expression, defs: &mut HashMap<String, Definition>) {
    if let Some(fid) = extract_function_id(value) {
        defs.insert(key.to_string(), Definition::Function(fid));
    } else if let Some(mod_id) = extract_require_call(value) {
        defs.insert(key.to_string(), Definition::Module(mod_id));
    } else if let Expression::Value(Value::Parameter(idx)) = value {
        defs.insert(key.to_string(), Definition::Parameter(*idx));
    } else if let Expression::Member { object, property, .. } = value {
        // Check for var x = y.default where y is a module
        if let Some(base) = get_base_name(object) {
            if let Some(Definition::Module(mod_id)) = defs.get(&base) {
                if let PropertyKey::String(p) = property {
                    if p == "default" {
                        defs.insert(key.to_string(), Definition::Module(*mod_id));
                    }
                }
            }
        }
        // Also track if base is a parameter: x = arg0.value -> x comes from arg0
        if let Expression::Value(Value::Parameter(idx)) = object.as_ref() {
            defs.insert(key.to_string(), Definition::Parameter(*idx));
        }
    } else if let Expression::Call { arguments, .. } = value {
        // If call has single param argument, track it: x = fn(arg0) -> x related to arg0
        // This is weaker but can help in some cases
        if arguments.len() == 1 {
            if let Expression::Value(Value::Parameter(idx)) = &arguments[0] {
                defs.insert(key.to_string(), Definition::Parameter(*idx));
            }
        }
    }
}

fn visit_stmt_for_calls(
    stmt: &Statement,
    caller_id: u32,
    defs: &HashMap<String, Definition>,
    graph: &mut CallGraph,
    call_sites: &mut HashMap<u32, Vec<Vec<Option<String>>>>,
    self_param_names: &mut HashMap<u32, Vec<Vec<Option<String>>>>,
    param_links: &mut Vec<((u32, u32), (u32, u32))>,
    metro_registry: &MetroRegistry,
    func_name_index: &FunctionNameIndex,
) {
    match stmt {
        Statement::Assign { value, .. }
        | Statement::Expr(value)
        | Statement::Return(Some(value))
        | Statement::Throw(value) => visit_expr(value, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index),

        Statement::If { condition, then_body, else_body } => {
            visit_expr(condition, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index);
            for s in then_body { visit_stmt_for_calls(s, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index); }
            for s in else_body { visit_stmt_for_calls(s, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index); }
        }
        Statement::While { condition, body } => {
            visit_expr(condition, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index);
            for s in body { visit_stmt_for_calls(s, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index); }
        }
        Statement::DoWhile { body, condition } => {
             for s in body { visit_stmt_for_calls(s, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index); }
             visit_expr(condition, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index);
        }
        Statement::For { init, condition, update, body } => {
            if let Some(i) = init { visit_stmt_for_calls(i, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index); }
            if let Some(c) = condition { visit_expr(c, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index); }
            if let Some(u) = update { visit_stmt_for_calls(u, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index); }
            for s in body { visit_stmt_for_calls(s, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index); }
        }
        Statement::Switch { discriminant, cases, default } => {
            visit_expr(discriminant, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index);
            for (val, body) in cases {
                visit_expr(val, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index);
                for s in body { visit_stmt_for_calls(s, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index); }
            }
            if let Some(d) = default {
                 for s in d { visit_stmt_for_calls(s, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index); }
            }
        }
        Statement::Block(inner) => {
             for s in inner { visit_stmt_for_calls(s, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index); }
        }
        Statement::Class { methods, super_class, .. } => {
            if let Some(sc) = super_class { visit_expr(sc, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index); }
            for _m in methods {}
        }
        _ => {}
    }
}

fn visit_expr(
    expr: &Expression,
    caller_id: u32,
    defs: &HashMap<String, Definition>,
    graph: &mut CallGraph,
    call_sites: &mut HashMap<u32, Vec<Vec<Option<String>>>>,
    self_param_names: &mut HashMap<u32, Vec<Vec<Option<String>>>>,
    param_links: &mut Vec<((u32, u32), (u32, u32))>,
    metro_registry: &MetroRegistry,
    func_name_index: &FunctionNameIndex,
) {
    // Check for parameter names within this expression (e.g., object literal properties)
    collect_param_names_from_expr(expr, caller_id, self_param_names);

    match expr {
        Expression::Call { callee, arguments } => {
            // Process the call itself
            let callee_id = resolve_callee(callee, defs, metro_registry, func_name_index);

            
            if let Some(id) = callee_id {
                graph.add_call(caller_id, id);
                
                // Check if this is a method call: obj.method(obj, args...)
                // In Hermes IR, the first argument in the arguments list for a method call is the context object (`this`) itself.
                // We skip this first argument because it does not correspond to a parameter in the function signature we want to infer names for.
                // Example: `obj.login(email)` -> IR arguments: [`obj`, `email`] -> we only want to infer name for `email` (param index 0 of `login`).
                let is_method_call = matches!(callee.as_ref(), Expression::Member { .. });
                let args_to_process: &[Expression] = if is_method_call && !arguments.is_empty() {
                    &arguments[1..] // Skip the `this` argument
                } else {
                    arguments
                };

                // Extract argument names AND links
                let mut arg_names = Vec::new();
                for (arg_idx, arg) in args_to_process.iter().enumerate() {
                    let mut resolved_param = None;
                    
                    match arg {
                         Expression::Value(Value::Variable(name)) => {
                             // Check for alias to parameter
                             if let Some(Definition::Parameter(idx)) = defs.get(name) {
                                  resolved_param = Some(*idx);
                             }
                             arg_names.push(Some(name.clone()));
                         },
                         Expression::Value(Value::Register(r)) => {
                              let r_name = format!("r{r}");
                              if let Some(Definition::Parameter(idx)) = defs.get(&r_name) {
                                 resolved_param = Some(*idx);
                              }
                              arg_names.push(Some(r_name));
                         }, 
                         Expression::Value(Value::Parameter(src_idx)) => {
                             resolved_param = Some(*src_idx);
                             arg_names.push(None); // No name hint directly from parameter
                         },
                         Expression::Value(Value::Constant(crate::ir::Constant::String(s))) => {
                             // Use string literal as parameter name hint
                             // Sanitize: must be valid identifier-ish
                             if s.chars().all(|c| c.is_alphanumeric() || c == '_') && !s.is_empty() {
                                 arg_names.push(Some(s.clone()));
                             } else {
                                 arg_names.push(None);
                             }
                         }
                         // Extract name from member access: user.email -> "email"
                         Expression::Member { property: PropertyKey::String(prop), .. } |
                         Expression::Member { property: PropertyKey::Ident(prop), .. } => {
                             arg_names.push(Some(prop.clone()));
                         }
                         // Extract name from call: getEmail() -> "email", or filter.join() -> "filter"
                         Expression::Call { callee, .. } => {
                             if let Some(name) = extract_name_from_callee(callee) {
                                 arg_names.push(Some(name));
                             } else if let Some(name) = extract_object_name_from_method_call(callee) {
                                 // For method calls like filter.join(), use the object name
                                 arg_names.push(Some(name));
                             } else {
                                 arg_names.push(None);
                             }
                         }
                         _ => arg_names.push(None)
                    }

                    if let Some(src_idx) = resolved_param {
                        param_links.push(((caller_id, src_idx), (id, arg_idx as u32)));
                    }
                }

                call_sites.entry(id).or_default().push(arg_names);
            }

            // Promise Then/Catch Parameter Naming Inference
            if let Expression::Member { property, .. } = callee.as_ref() {
                let prop_name_opt = match property {
                    PropertyKey::String(s) => Some(s.as_str()),
                    PropertyKey::Ident(s) => Some(s.as_str()),
                    _ => None,
                };

                if let Some(prop_name) = prop_name_opt {
                    // For method calls (callee is Member), the arguments list includes 'this' at index 0.
                    // So .then(onFullfilled, onRejected) -> args: [this, onFullfilled, onRejected]
                    // We need to shift our indexing by 1 to access the callbacks.

                    if prop_name == "then" && arguments.len() >= 2 {
                         // Arg 1: onFulfilled(response) - skipping explicit 'this' (arg 0)
                         // We infer the parameter name "response" for the first argument of the success callback.
                         // This is a common convention for Promise chains (fetch, axios, graphQL mutations).
                         if let Some(fid) = resolve_callee(&arguments[1], defs, metro_registry, func_name_index) {
                             // println!("// [IPA] Resolved .then callback to function {}", fid);
                             call_sites.entry(fid).or_default().push(vec![Some("response".to_string())]);
                         }
                         
                         // Arg 2: onRejected(error)
                         if arguments.len() > 2 {
                             if let Some(fid) = resolve_callee(&arguments[2], defs, metro_registry, func_name_index) {
                                 call_sites.entry(fid).or_default().push(vec![Some("error".to_string())]);
                             }
                         }
                    } else if prop_name == "catch" && arguments.len() >= 2 {
                         // Arg 1: onRejected(error)
                         if let Some(fid) = resolve_callee(&arguments[1], defs, metro_registry, func_name_index) {
                             call_sites.entry(fid).or_default().push(vec![Some("error".to_string())]);
                         }
                    }
                }
            }

            // Recurse
            visit_expr(callee, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index);
            for arg in arguments {
                visit_expr(arg, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index);
            }
        }
        Expression::Binary { left, right, .. } => {
            visit_expr(left, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index);
            visit_expr(right, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index);
        }
        Expression::Unary { operand, .. } => visit_expr(operand, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index),
        Expression::Member { object, property, .. } => {
            visit_expr(object, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index);
            if let PropertyKey::Computed(k) = property {
                visit_expr(k, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index);
            }
        }
        Expression::Array { elements } => {
            for e in elements.iter().flatten() {
                visit_expr(e, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index);
            }
        }
        Expression::Object { properties } => {
            for p in properties {
                visit_expr(&p.value, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index);
                if let PropertyKey::Computed(k) = &p.key {
                    visit_expr(k, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index);
                }
            }
        }
        Expression::Assignment { target, value } => {
            visit_expr(target, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index);
            visit_expr(value, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index);
        }
        Expression::New { callee, arguments } => {
             visit_expr(callee, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index);
             for arg in arguments {
                visit_expr(arg, caller_id, defs, graph, call_sites, self_param_names, param_links, metro_registry, func_name_index);
            }
        }
        _ => {}
    }
}

fn extract_function_id(expr: &Expression) -> Option<u32> {
    match expr {
        Expression::Function { id, .. } => Some(id.0),
        _ => None,
    }
}

/// Extract the required module ID from a `require` call.
/// 
/// Heuristic:
/// We look for calls that structurally resemble `require(123)`.
/// Since variable names might be minified, we rely on the specific pattern of a call
/// with exactly one integer argument.
/// We strictly check if the variable name is "require".
/// TODO: Enhance this to support aliased `require` (e.g. `r(123)`) if we can prove `r` is `require`.
fn extract_require_call(expr: &Expression) -> Option<u32> {
    if let Expression::Call { callee, arguments } = expr {
        // Match `call require(integer_constant)`
        if arguments.len() == 1 {
             if let Expression::Value(Value::Constant(crate::ir::Constant::Integer(n))) = arguments[0] {
                 match callee.as_ref() {
                     Expression::Value(Value::Variable(name)) if name == "require" => return Some(n as u32),
                     _ => {}
                 }
             }
        }
    }
    None
}

fn target_to_key(target: &AssignTarget) -> Option<String> {
    match target {
        AssignTarget::Register(r) => Some(format!("r{r}")),
        AssignTarget::Variable(name) => Some(name.clone()),
        _ => None,
    }
}

fn resolve_callee(
    callee: &Expression,
    defs: &HashMap<String, Definition>,
    metro_registry: &MetroRegistry,
    func_name_index: &FunctionNameIndex,
) -> Option<u32> {
    match callee {
        Expression::Value(Value::Variable(name)) => {
            if let Some(def) = defs.get(name) {
                match def {
                    Definition::Function(fid) => return Some(*fid),
                    Definition::Module(mod_id) => {
                         if let Some(module) = metro_registry.get_module(*mod_id) {
                              if let Some(fid) = module.exports.get("default") {
                                  return Some(*fid);
                              }
                         }
                    }
                    _ => {}
                }
            }
            // Fallback: check if variable name matches a known function name
            if let Some(&fid) = func_name_index.get(name) {
                return Some(fid);
            }
            None
        },
        Expression::Value(Value::Register(r)) => {
            let r_name = format!("r{r}");
             if let Some(def) = defs.get(&r_name) {
                 match def {
                    Definition::Function(fid) => return Some(*fid),
                    Definition::Module(mod_id) => {
                         if let Some(module) = metro_registry.get_module(*mod_id) {
                              if let Some(fid) = module.exports.get("default") {
                                  return Some(*fid);
                              }
                         }
                    }
                    _ => {}
                }
            }
            None
        },
        Expression::Function { id, .. } => Some(id.0),

        // Handle member access: obj.methodName()
        Expression::Member { object, property, .. } => {
             // First try to resolve via module registry
             if let Some(base_name) = get_base_name(object) {
                 if let Some(def) = defs.get(&base_name) {
                      if let Definition::Module(mod_id) = def {
                           if let PropertyKey::String(prop_name) = property {
                                if let Some(module) = metro_registry.get_module(*mod_id) {
                                     if let Some(fid) = module.exports.get(prop_name) {
                                         return Some(*fid);
                                     }
                                }
                           }
                      }
                 }
             }
             // Fallback: check if property name matches a known function name
             // This handles cases like `default.loginWithToken(...)` where we can't trace `default`
             // but we know `loginWithToken` is a function name in the bundle
             let prop_name = match property {
                 PropertyKey::String(s) | PropertyKey::Ident(s) => Some(s.as_str()),
                 _ => None,
             };
             if let Some(name) = prop_name {
                 if let Some(&fid) = func_name_index.get(name) {
                     return Some(fid);
                 }
             }
             None
        }
        _ => None
    }
}

fn get_base_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Value(Value::Variable(name)) => Some(name.clone()),
        Expression::Value(Value::Register(r)) => Some(format!("r{r}")),
        _ => None
    }
}

/// Extract a name hint from a call expression's callee to name the return value or parameters.
/// 
/// Heuristic:
/// - If the function name is verb-noun (e.g., `getEmail`, `fetchUser`), we extract the noun ("email", "user").
/// - We strip common verb prefixes ("get", "fetch", "load", etc.).
/// - This helps naming variables holding the result: `var email = getEmail();`
fn extract_name_from_callee(callee: &Expression) -> Option<String> {
    let name = match callee {
        Expression::Value(Value::Variable(name)) => name.clone(),
        Expression::Member { property: PropertyKey::String(prop), .. } |
        Expression::Member { property: PropertyKey::Ident(prop), .. } => prop.clone(),
        _ => return None,
    };

    // Strip common prefixes: get, fetch, load, read, find, create, make, build
    let prefixes = ["get", "fetch", "load", "read", "find", "create", "make", "build", "compute", "calculate"];
    let lower = name.to_lowercase();

    for prefix in prefixes {
        if lower.starts_with(prefix) && name.len() > prefix.len() {
            let rest = &name[prefix.len()..];
            // Make sure next char was uppercase (camelCase) or underscore
            if rest.starts_with('_') {
                return Some(rest[1..].to_string());
            } else if rest.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                // Convert first char to lowercase: Email -> email
                let mut chars = rest.chars();
                if let Some(first) = chars.next() {
                    return Some(first.to_lowercase().chain(chars).collect());
                }
            }
        }
    }

    // No prefix found, return as-is if it looks like a noun (not a verb pattern)
    None
}

/// Extract the object name from a method call like `filter.join()` -> "filter"
fn extract_object_name_from_method_call(callee: &Expression) -> Option<String> {
    if let Expression::Member { object, .. } = callee {
        match object.as_ref() {
            Expression::Value(Value::Variable(name)) => {
                // Filter out generic names
                if !super::inference::is_generic_name(name) {
                    return Some(name.clone());
                }
            }
            _ => {}
        }
    }
    None
}
