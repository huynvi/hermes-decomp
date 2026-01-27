use std::collections::HashMap;
use crate::ir::{Statement, Expression, Value, AssignTarget, PropertyKey};
use super::registry::MetroModule;

/// Analyzes the exports of a Metro module to find exported functions.
///
/// Metro modules are wrapped in a factory:
/// `function(global, require, module, exports) { ... }`
///
/// Analyzes the exports of a Metro module to find exported functions.
///
/// Metro modules are typically wrapped in a factory function:
/// `function(global, require, module, exports) { ... }`
///
/// Our goal is to find which internal functions (IDs) are exposed as exports.
/// This allows us to link calls from other modules (via `require`) to these functions.
///
/// We look for several patterns:
/// 1. `exports.foo = function_id`  (Direct export assignment)
/// 2. `module.exports = { foo: function_id }` (Module exports object literal)
/// 3. `module.exports.foo = function_id` (Member assignment)
/// 4. `Object.defineProperty(exports, "default", { get: function() { return internal_func; } })` (ESM default export pattern)
pub struct ExportAnalyzer;

impl ExportAnalyzer {
    pub fn analyze(module: &mut MetroModule, functions: &HashMap<u32, Vec<Statement>>) {
        let stmts = match functions.get(&module.function_id) {
            Some(s) => s,
            None => return,
        };
        
        let exports_aliases = vec!["p3".to_string(), "exports".to_string()];
        let module_aliases = vec!["p2".to_string(), "module".to_string()];
        
        // Track variable definitions (simple constant/object propagation)
        let mut definitions: HashMap<String, Expression> = HashMap::new();

        // Scan statements
        for stmt in stmts {
            // Update definitions
            if let Statement::Assign { target, value } = stmt {
                if let AssignTarget::Variable(name) = target {
                    definitions.insert(name.clone(), value.clone());
                }
                 // registers?
            }
            
            analyze_stmt(stmt, &mut module.exports, &exports_aliases, &module_aliases, functions, stmts, &definitions);
        }
    }
}

fn analyze_stmt(
    stmt: &Statement,
    exports: &mut HashMap<String, u32>,
    exports_aliases: &[String],
    module_aliases: &[String],
    functions: &HashMap<u32, Vec<Statement>>,
    factory_stmts: &[Statement], 
    definitions: &HashMap<String, Expression>,
) {
    match stmt {
        Statement::Assign { target, value } => {
            // Check for Object.defineProperty(exports, "default", { get: ... })
            if let Expression::Call { callee, arguments } = value {
                if is_define_property(callee) && arguments.len() >= 3 {
                    // This pattern detects:
                    // Object.defineProperty(exports, "default", { enumerable: true, get: function() { return ...; } });
                    
                    // Arg 0: Exports object
                    if let Some(arg0_name) = get_var_name(&arguments[0]) {
                        if exports_aliases.contains(&arg0_name) {
                             // Arg 1: "default"
                             if let Expression::Value(Value::Constant(crate::ir::Constant::String(prop))) = &arguments[1] {
                                 if prop == "default" {
                                     // Arg 2: Descriptor
                                     // Resolve argument 2 if it's a variable
                                     let descriptor_expr = if let Expression::Value(Value::Variable(v)) = &arguments[2] {
                                         definitions.get(v).unwrap_or(&arguments[2])
                                     } else {
                                         &arguments[2]
                                     };

                                     if let Expression::Object { properties } = descriptor_expr {
                                          analyze_descriptor(properties, exports, functions, factory_stmts);
                                     }
                                 }
                             }
                        }
                    }
                }
            }

            // Check for exports.prop = func
            if let Some((base, prop)) = get_base_and_prop(target) {
                if exports_aliases.contains(&base) {
                    if let Some(func_id) = extract_func_id(value) {
                         exports.insert(prop, func_id);
                    }
                } else if module_aliases.contains(&base) && prop == "exports" {
                    // module.exports = ...
                    analyze_module_exports_assign(value, exports);
                }
            } else if let AssignTarget::Member { object, property } = target {
                 // Handle module.exports.prop = func
                 // object must be module.exports
                 if let Expression::Member { object: inner_obj, property: inner_prop, .. } = object {
                      if let Some(base) = get_var_name(inner_obj) {
                          if module_aliases.contains(&base) {
                               if let PropertyKey::String(s) = inner_prop {
                                    if s == "exports" {
                                         // property is already &String
                                          if let Some(func_id) = extract_func_id(value) {
                                               eprintln!("[ExportAnalyzer] Found export via property assignment: {} -> {}", property, func_id);
                                               exports.insert(property.clone(), func_id);
                                          }
                                    }
                               }
                          }
                      }
                 }
            }
        }
        Statement::Block(inner) => {
            for s in inner {
                analyze_stmt(s, exports, exports_aliases, module_aliases, functions, factory_stmts, definitions);
            }
        }
        Statement::If { then_body, else_body, .. } => {
            for s in then_body { analyze_stmt(s, exports, exports_aliases, module_aliases, functions, factory_stmts, definitions); }
            for s in else_body { analyze_stmt(s, exports, exports_aliases, module_aliases, functions, factory_stmts, definitions); }
        }
        _ => {}
    }
}

fn analyze_module_exports_assign(value: &Expression, exports: &mut HashMap<String, u32>) {
     match value {
         Expression::Object { properties } => {
             for prop in properties {
                 if let PropertyKey::String(key) = &prop.key {
                      if let Some(func_id) = extract_func_id(&prop.value) {
                          eprintln!("[ExportAnalyzer] Found export in object literal: {} -> {}", key, func_id);
                          exports.insert(key.clone(), func_id);
                      }
                 }
             }
         }
         _ => {}
     }
}

fn get_base_and_prop(target: &AssignTarget) -> Option<(String, String)> {
    if let AssignTarget::Member { object, property } = target {
        if let Some(base) = get_var_name(object) {
            return Some((base, property.clone()));
        }
    }
    None
}

fn get_var_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Value(Value::Variable(n)) => Some(n.clone()),
        Expression::Value(Value::Register(r)) => Some(format!("r{r}")),
        Expression::Value(Value::Parameter(idx)) => Some(format!("p{idx}")),
        _ => None
    }
}

fn is_define_property(callee: &Expression) -> bool {
    // Matches Object.defineProperty or var.defineProperty
    if let Expression::Member { property, .. } = callee {
        if let PropertyKey::String(name) = property {
            return name == "defineProperty";
        }
    }
    false
}

fn analyze_descriptor(
    properties: &[crate::ir::ObjectProperty],
    exports: &mut HashMap<String, u32>,
    functions: &HashMap<u32, Vec<Statement>>,
    factory_stmts: &[Statement],
) {
    for prop in properties {
        if let PropertyKey::String(key) = &prop.key {
            if key == "get" {
                if let Some(getter_id) = extract_func_id(&prop.value) {
                    if let Some(getter_stmts) = functions.get(&getter_id) {
                         // The getter usually looks like: `function() { return internal_var; }`
                         if let Some(returned_var) = find_returned_variable(getter_stmts) {
                             // Now we need to find what `internal_var` points to.
                             // It is often assigned earlier in the factory: `internal_var = function_id;`
                             eprintln!("[ExportAnalyzer] Found getter returning variable: {}", returned_var);
                             scan_for_object_assignment(&returned_var, factory_stmts, exports);
                         }
                    }
                }
            }
        }
    }
}

fn find_returned_variable(stmts: &[Statement]) -> Option<String> {
    for stmt in stmts {
        match stmt {
            Statement::Return(Some(Expression::Value(Value::Variable(name)))) => return Some(name.clone()),
            Statement::Block(inner) => {
                if let Some(n) = find_returned_variable(inner) { return Some(n); }
            }
            _ => {}
        }
    }
    None
}

fn scan_for_object_assignment(var_name: &str, stmts: &[Statement], exports: &mut HashMap<String, u32>) {
    for stmt in stmts {
        match stmt {
            Statement::Assign { target, value } => {
                if let Some(name) = get_var_name_from_target(target) {
                    if name == var_name {
                        analyze_module_exports_assign(value, exports);
                    }
                }
            }
             Statement::Block(inner) => scan_for_object_assignment(var_name, inner, exports),
             Statement::If { then_body, else_body, .. } => {
                 scan_for_object_assignment(var_name, then_body, exports);
                 scan_for_object_assignment(var_name, else_body, exports);
             }
             _ => {}
        }
    }
}

fn get_var_name_from_target(target: &AssignTarget) -> Option<String> {
    match target {
        AssignTarget::Variable(n) => Some(n.clone()),
        _ => None
    }
}

fn extract_func_id(expr: &Expression) -> Option<u32> {
    match expr {
        Expression::Function { id, .. } => Some(id.0),
        _ => None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Expression, Statement, Value, AssignTarget, PropertyKey};
    use crate::ir::FunctionId;
    use std::collections::HashMap;

    fn make_func_expr(id: u32) -> Expression {
        Expression::Function { 
            id: FunctionId(id), 
            name: None, 
            is_arrow: false, 
            is_async: false, 
            is_generator: false 
        }
    }

    #[test]
    fn test_export_assignments() {
        let mut stmts = Vec::new();

        // exports.foo = func(10)
        stmts.push(Statement::Assign {
            target: AssignTarget::Member {
                object: Expression::Value(Value::Variable("exports".into())),
                property: "foo".into()
            },
            value: make_func_expr(10)
        });

        // module.exports.bar = func(20)
        stmts.push(Statement::Assign {
            target: AssignTarget::Member {
                object: Expression::Member {
                    object: Box::new(Expression::Value(Value::Variable("module".into()))),
                    property: PropertyKey::String("exports".into()),
                    optional: false
                },
                property: "bar".into()
            },
            value: make_func_expr(20)
        });
        
        // module.exports = { baz: func(30) }
        stmts.push(Statement::Assign {
            target: AssignTarget::Member {
                object: Expression::Value(Value::Variable("module".into())),
                property: "exports".into()
            },
            value: Expression::Object {
                properties: vec![
                    crate::ir::ObjectProperty {
                        key: PropertyKey::String("baz".into()),
                        value: make_func_expr(30)
                    }
                ]
            }
        });

        let mut module = MetroModule {
            module_id: 1,
            function_id: 100,
            name: None,
            dependencies: vec![],
            exports: HashMap::new()
        };

        let mut functions = HashMap::new();
        functions.insert(100, stmts);

        ExportAnalyzer::analyze(&mut module, &functions);

        assert_eq!(module.exports.get("foo"), Some(&10));
        assert_eq!(module.exports.get("bar"), Some(&20));
        assert_eq!(module.exports.get("baz"), Some(&30));
    }
}
