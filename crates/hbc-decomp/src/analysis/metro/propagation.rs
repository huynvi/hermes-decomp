use std::collections::HashMap;
use crate::ir::{Statement, Expression, Value, PropertyKey};
use crate::analysis::ClosureContext;
use super::registry::MetroRegistry;
use super::detection::is_meaningful_name;

/// Propagate module names to variables that hold module exports.
pub fn propagate_module_names(
    functions: &mut HashMap<u32, Vec<Statement>>,
    registry: &MetroRegistry,
    closure_ctx: &mut Option<ClosureContext>,
) {
    // 0. Infer names for anonymous modules based on exports
    let mut inferred_names = HashMap::new();
    for (mod_id, module) in &registry.modules {
        if module.name.is_none() {
            if let Some(factory_stmts) = functions.get(&module.function_id) {
                 if let Some(name) = infer_module_name_from_stmts(factory_stmts, functions) {
                     inferred_names.insert(*mod_id, name);
                 }
            }
        }
    }

    // Capture effective names for lookup
    let effective_names: HashMap<u32, String> = registry.modules.iter()
        .map(|(id, m)| {
            let name = m.name.clone()
                .or_else(|| inferred_names.get(id).cloned())
                .unwrap_or_else(|| format!("v{}", id));
            
            // Ensure valid identifier
            let sanitized = if name.chars().all(|c| c.is_ascii_digit()) {
                format!("v{}", name)
            } else {
                name
            };
            (*id, sanitized)
        })
        .collect();

    // Iterate all functions to find `require` calls
    for (func_id, stmts) in functions.iter_mut() {
        let mut renames = HashMap::new();
        
        // Track register/variable sources for simple constant propagation
        // Map Name -> Parameter Index
        let mut reg_params: HashMap<String, u32> = HashMap::new();
        // Map Name -> (Base Name, Index) for array/property loads
        let mut reg_props: HashMap<String, (String, u32)> = HashMap::new();

        for stmt in stmts.iter() {
            // 1. Analyze assignments to track data flow
            if let Statement::Assign { target, value } = stmt {
                let var_name = target_to_key(target);

                if let Some(name) = var_name {
                    // Try to infer parameter index from value
                    let param_idx = match value {
                        Expression::Value(Value::Parameter(idx)) => Some(*idx),
                        Expression::Value(Value::Variable(v)) => {
                            v.strip_prefix("arg").and_then(|s| s.parse::<u32>().ok())
                        }
                        _ => None
                    };

                    if let Some(idx) = param_idx {
                        reg_params.insert(name.clone(), idx);
                    }
                    
                    match value {
                        Expression::Member { object, property: PropertyKey::Index(idx), .. } => {
                             let base_name = match &**object {
                                 Expression::Value(Value::Register(r)) => Some(format!("r{r}")),
                                 Expression::Value(Value::Variable(n)) => Some(n.clone()),
                                 Expression::Value(Value::Parameter(i)) => Some(format!("arg{i}")),
                                 _ => None
                             };
                             if let Some(base) = base_name {
                                 reg_props.insert(name.clone(), (base, *idx as u32));
                             }
                        }
                        Expression::Value(Value::Register(r)) => {
                            let r_name = format!("r{r}");
                            if let Some(prop) = reg_props.get(&r_name) {
                                let val = prop.clone();
                                reg_props.insert(name.clone(), val);
                            }
                            if let Some(param) = reg_params.get(&r_name) {
                                reg_params.insert(name.clone(), *param);
                            }
                        }
                         Expression::Value(Value::Variable(v)) => {
                            if let Some(prop) = reg_props.get(v) {
                                let val = prop.clone();
                                reg_props.insert(name.clone(), val);
                            }
                            if let Some(param) = reg_params.get(v) {
                                reg_params.insert(name.clone(), *param);
                            }
                        }
                        _ => {}
                    }
                }
            }

            // 2. Check for require calls or propagation
            if let Statement::Assign { target, value } = stmt {
                // Case 1: Direct require() call
                if let Some(mod_id) = resolve_require_module(value, *func_id, registry, &reg_params, &reg_props) {
                      if let Some(name) = effective_names.get(&mod_id) {
                         // Found a target to rename!
                         if let Some(var_name) = target_to_key(target) {
                             renames.insert(var_name, name.clone());
                         } 
                         
                         // Handle closure variables specifically
                         if let crate::ir::AssignTarget::ClosureVar { slot, level, .. } = target {
                             if let Some(ctx) = closure_ctx {
                                 // Walk up levels to find the defining function
                                 let mut defining_func = *func_id;
                                 for _ in 0..*level {
                                     if let Some(&p) = ctx.parent_function.get(&defining_func) {
                                         defining_func = p;
                                     }
                                 }
                                 // Update the slot name
                                 ctx.update_slot_variable(defining_func, *slot, name.clone());
                             }
                         }
                     }
                } else if matches!(value, Expression::Value(Value::Variable(_)) | Expression::Value(Value::Register(_))) {
                     // Case 2: Simple assignment (x = y)
                     let source_name = match value {
                         Expression::Value(Value::Variable(v)) => Some(v.clone()),
                         Expression::Value(Value::Register(r)) => Some(format!("r{r}")),
                         _ => None
                     };

                     if let Some(src) = source_name {
                         if let Some(name) = renames.get(&src).cloned() {
                            if let Some(var_name) = target_to_key(target) {
                                renames.insert(var_name, name.clone());
                            } 
                            if let crate::ir::AssignTarget::ClosureVar { slot, level, .. } = target {
                                if let Some(ctx) = closure_ctx {
                                    let mut defining_func = *func_id;
                                    for _ in 0..*level {
                                        if let Some(&p) = ctx.parent_function.get(&defining_func) {
                                            defining_func = p;
                                        }
                                    }
                                    ctx.update_slot_variable(defining_func, *slot, name.clone());
                                }
                            }
                         }
                     }
                } else if let Expression::Call { callee: _, arguments } = value {
                    // Case 3: Wrapper call (x = _interopDefault(y))
                    let arg = if arguments.len() == 1 {
                        Some(&arguments[0])
                    } else if arguments.len() == 2 {
                        Some(&arguments[1]) 
                    } else {
                        None
                    };

                    if let Some(arg_expr) = arg {
                         // Sub-case 3.1: Argument is a nested require() call?
                         let mut propagated_name = None;
                         
                         if let Some(mod_id) = resolve_require_module(arg_expr, *func_id, registry, &reg_params, &reg_props) {
                             if let Some(name) = effective_names.get(&mod_id) {
                                 propagated_name = Some(name.clone());
                             }
                         }
                         
                         // Sub-case 3.2: Argument is a variable holding a module?
                         if propagated_name.is_none() {
                             if let Expression::Value(val) = arg_expr {
                                 let arg_name = match val {
                                     Value::Variable(v) => Some(v.clone()),
                                     Value::Register(r) => Some(format!("r{r}")),
                                     _ => None
                                 };
                                 if let Some(arg_v) = arg_name {
                                     if let Some(name) = renames.get(&arg_v).cloned() {
                                         propagated_name = Some(name);
                                     }
                                 }
                             }
                         }

                        if let Some(name) = propagated_name {
                            if let Some(var_name) = target_to_key(target) {
                                renames.insert(var_name, name.clone());
                            }
                            // 4. Update Closure Context if it's a closure variable
                            if let crate::ir::AssignTarget::ClosureVar { slot, level, .. } = target {
                                if let Some(ctx) = closure_ctx {
                                    let mut defining_func = *func_id;
                                    for _ in 0..*level {
                                        if let Some(&p) = ctx.parent_function.get(&defining_func) {
                                            defining_func = p;
                                        }
                                    }
                                    ctx.update_slot_variable(defining_func, *slot, name.clone());
                                }
                            } else if let crate::ir::AssignTarget::Variable(v) = target {
                                // Fallback: handle "closure_N" as a slot update
                                if let Some(slot_id_str) = v.strip_prefix("closure_") {
                                    if let Ok(slot) = slot_id_str.parse::<u32>() {
                                        if let Some(ctx) = closure_ctx {
                                            ctx.update_slot_variable(*func_id, slot, name.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Apply renames to current function statements (rename usages)
        if !renames.is_empty() {
             crate::analysis::naming::rename_variables_in_stmts(stmts, &renames);
        }
    }
}

fn resolve_require_module(
    expr: &Expression, 
    func_id: u32, 
    registry: &MetroRegistry,
    reg_params: &HashMap<String, u32>,
    reg_props: &HashMap<String, (String, u32)>,
) -> Option<u32> {
    if let Expression::Call { callee, arguments } = expr {
        let is_require = match &**callee {
            Expression::Value(Value::Variable(name)) => name == "require" || name == "arg1",
            Expression::Value(Value::Parameter(1)) => true, 
            _ => false
        };

        if is_require {
                 // Argument is usually at index 0 (if pure) or 1 (if call with this)
                 let arg_expr = if arguments.len() == 1 {
                     Some(&arguments[0])
                 } else if arguments.len() == 2 {
                     Some(&arguments[1]) 
                 } else {
                     None
                 };

                 if let Some(arg) = arg_expr {
                      // Sub-case 1.1: Constant Integer
                      if let Expression::Value(Value::Constant(crate::ir::Constant::Integer(id))) = arg {
                          return Some(*id as u32);
                      }
                      
                      // Sub-case 1.2: Register (Dynamic Require via Dependency Array)
                      let reg_name = match arg {
                          Expression::Value(Value::Register(r)) => Some(format!("r{r}")),
                          Expression::Value(Value::Variable(n)) => Some(n.clone()),
                          _ => None
                      };

                      if let Some(r) = reg_name {
                          // Trace r
                          if let Some((base, idx)) = reg_props.get(&r) {
                               // Check if base is a parameter (argN)
                               let _param_idx = reg_params.get(base).copied().or_else(|| {
                                   base.strip_prefix("arg").and_then(|s| s.parse::<u32>().ok())
                               });
                                    if let Some(module) = registry.get_module_for_function(func_id) {
                                        if (*idx as usize) < module.dependencies.len() {
                                            let mod_id = module.dependencies[*idx as usize];
                                            return Some(mod_id);
                                        }
                                    }
                               }
                          }
                      }

                      // Sub-case 1.3: Member Expression (Direct dependency map lookup)
                      if let Some(Expression::Member { object, property: PropertyKey::Index(idx), .. }) = arg_expr {
                           let base_name = match &**object {
                               Expression::Value(Value::Register(r)) => Some(format!("r{r}")),
                               Expression::Value(Value::Variable(n)) => Some(n.clone()),
                               Expression::Value(Value::Parameter(i)) => Some(format!("arg{i}")),
                               _ => None
                           };
                           if let Some(base) = base_name {
                               // Check if base is the dependency array (usually a parameter index > 3)
                               let param_idx = base.strip_prefix("arg").and_then(|s| s.parse::<u32>().ok());
                               if let Some(p_idx) = param_idx {
                                   if p_idx >= 4 { // Standard Metro factory: (g, r, m, e, d)
                                       if let Some(module) = registry.get_module_for_function(func_id) {
                                           if (*idx as usize) < module.dependencies.len() {
                                               return Some(module.dependencies[*idx as usize]);
                                           }
                                       }
                                   }
                               }
                           }
                      }
                 }
             }
    None
}

fn target_to_key(target: &crate::ir::AssignTarget) -> Option<String> {
    match target {
        crate::ir::AssignTarget::Variable(n) => Some(n.clone()),
        crate::ir::AssignTarget::Register(r) => Some(format!("r{r}")),
        _ => None
    }
}

fn infer_module_name_from_stmts(
    stmts: &[Statement], 
    functions: &HashMap<u32, Vec<Statement>>
) -> Option<String> {
    for stmt in stmts {
        match stmt {
            Statement::Assign { target, value } => {
                // Check if target is 'exports' or 'module.exports'
                let is_export = match target {
                    crate::ir::AssignTarget::Variable(n) => n == "exports" || n == "arg3",
                    crate::ir::AssignTarget::Member { object, .. } => {
                        match object {
                            Expression::Value(Value::Variable(n)) => n == "module" || n == "arg2" || n == "exports" || n == "arg3",
                            Expression::Value(Value::Parameter(2)) | Expression::Value(Value::Parameter(3)) => true,
                            _ => false
                        }
                    }
                    _ => false,
                };

                if is_export {
                    if let Some(name) = infer_from_expr(value, functions) {
                        return Some(name);
                    }
                }

                if let Some(name) = infer_from_expr(value, functions) {
                    return Some(name);
                }
            }
            Statement::Expr(value) | Statement::Return(Some(value)) => {
                if let Some(name) = infer_from_expr(value, functions) {
                    return Some(name);
                }
            }
            _ => {}
        }
    }
    None
}

fn infer_from_expr(expr: &Expression, functions: &HashMap<u32, Vec<Statement>>) -> Option<String> {
    match expr {
        Expression::Function { id, name, .. } => {
            if let Some(n) = name {
                if is_meaningful_name(n) {
                    return Some(n.clone());
                }
            }
            // Recurse into body?
            if let Some(body) = functions.get(&id.0) {
                 if let Some(inner) = infer_module_name_from_stmts(body, functions) {
                     return Some(inner);
                 }
            }
            None
        }
        Expression::Call { callee, .. } => {
            // Check if IIFE
             infer_from_expr(callee, functions)
        }
        _ => None
    }
}
