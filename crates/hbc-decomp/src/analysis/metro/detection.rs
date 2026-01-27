use std::collections::HashMap;
use crate::ir::{Statement, Expression, Value};
use super::registry::{MetroRegistry, MetroModule};

/// Utility to analyze statements and populate the registry.
pub struct MetroDetector;

impl MetroDetector {
    // Analyze the global function statements to extract module registrations.
    //
    // Metro uses `__d(undefined, factory, moduleId, dependencyMap)` to register modules.
    // The __d function is typically stored in a variable like `r0 = __r.__d`.
    pub fn analyze_statements(statements: &[Statement], registry: &mut MetroRegistry) {
        // Track what's assigned to each register/variable
        let mut reg_functions: HashMap<String, u32> = HashMap::new(); // variable -> function ID
        let mut reg_arrays: HashMap<String, Vec<u32>> = HashMap::new(); // variable -> array values
        let mut reg_integers: HashMap<String, u32> = HashMap::new(); // variable -> integer value

        for stmt in statements {
            Self::analyze_stmt(stmt, &mut reg_functions, &mut reg_arrays, &mut reg_integers, registry);
        }
    }

    fn analyze_stmt(
        stmt: &Statement,
        reg_functions: &mut HashMap<String, u32>,
        reg_arrays: &mut HashMap<String, Vec<u32>>,
        reg_integers: &mut HashMap<String, u32>,
        registry: &mut MetroRegistry,
    ) {
        match stmt {
            Statement::Assign { target, value } => {
                let var_name = match target {
                    crate::ir::AssignTarget::Register(r) => Some(format!("r{r}")),
                    crate::ir::AssignTarget::Variable(n) => Some(n.clone()),
                    _ => None
                };

                if let Some(name) = var_name {
                    // Track function assignments
                    if let Expression::Function { id, .. } = value {
                        reg_functions.insert(name.clone(), id.0);
                    }
                    // Track array literal assignments
                    if let Some(deps) = extract_array_of_integers(value) {
                        reg_arrays.insert(name.clone(), deps);
                    }
                    // Track integer assignments
                    if let Some(n) = extract_integer(value) {
                        reg_integers.insert(name.clone(), n);
                    }
                }

                // Look for __d calls in the value
                Self::check_for_d_call(value, reg_functions, reg_arrays, reg_integers, registry);
            }
            Statement::Expr(expr) => {
                Self::check_for_d_call(expr, reg_functions, reg_arrays, reg_integers, registry);
            }
            Statement::If { then_body, else_body, .. } => {
                for s in then_body {
                    Self::analyze_stmt(s, reg_functions, reg_arrays, reg_integers, registry);
                }
                for s in else_body {
                    Self::analyze_stmt(s, reg_functions, reg_arrays, reg_integers, registry);
                }
            }
            Statement::While { body, .. } => {
                for s in body {
                    Self::analyze_stmt(s, reg_functions, reg_arrays, reg_integers, registry);
                }
            }
            Statement::For { body, .. } => {
                for s in body {
                    Self::analyze_stmt(s, reg_functions, reg_arrays, reg_integers, registry);
                }
            }
            Statement::Block(inner) => {
                for s in inner {
                    Self::analyze_stmt(s, reg_functions, reg_arrays, reg_integers, registry);
                }
            }
            _ => {}
        }
    }

    fn check_for_d_call(
        expr: &Expression,
        reg_functions: &HashMap<String, u32>,
        reg_arrays: &HashMap<String, Vec<u32>>,
        reg_integers: &HashMap<String, u32>,
        registry: &mut MetroRegistry,
    ) {
        if let Expression::Call { callee: _, arguments } = expr {
            // Metro format: __d(undefined, factory, moduleId, deps)
            // Arguments: [0] = undefined/context, [1] = function, [2] = moduleId, [3] = deps
            if arguments.len() == 4 {
                // Get function ID - either directly or via register/variable lookup
                let function_id = match &arguments[1] {
                     Expression::Function { id, .. } => Some(id.0),
                     Expression::Value(Value::Register(r)) => reg_functions.get(&format!("r{r}")).copied(),
                     Expression::Value(Value::Variable(n)) => reg_functions.get(n).copied(),
                     _ => None,
                };

                // Get module ID - either directly or via register/variable lookup
                let module_id = match &arguments[2] {
                    Expression::Value(Value::Constant(crate::ir::Constant::Integer(n))) => Some(*n as u32),
                     Expression::Value(Value::Register(r)) => reg_integers.get(&format!("r{r}")).copied(),
                     Expression::Value(Value::Variable(n)) => reg_integers.get(n).copied(),
                    _ => None,
                };

                // Get dependencies - either directly or via register/variable lookup
                let dependencies = match &arguments[3] {
                    Expression::Array { .. } => extract_array_of_integers(&arguments[3]).unwrap_or_default(),
                     Expression::Value(Value::Register(r)) => reg_arrays.get(&format!("r{r}")).cloned().unwrap_or_default(),
                     Expression::Value(Value::Variable(n)) => reg_arrays.get(n).cloned().unwrap_or_default(),
                    _ => Vec::new(),
                };
                
                // Register the module if we have function and module IDs
                if let (Some(func_id), Some(mod_id)) = (function_id, module_id) {
                    let inferred_name = match &arguments[1] {
                        Expression::Function { name: Some(n), .. } if is_meaningful_name(n) => Some(n.clone()),
                        _ => None,
                    };

                    let module = MetroModule {
                        module_id: mod_id,
                        function_id: func_id,
                        name: inferred_name,
                        dependencies,
                        exports: HashMap::new(),
                    };
                    registry.function_to_module.insert(func_id, mod_id);
                    registry.modules.insert(mod_id, module);
                }
            }
        }
    }
}

fn extract_integer(expr: &Expression) -> Option<u32> {
    match expr {
        Expression::Value(Value::Constant(crate::ir::Constant::Integer(n))) => {
            Some(*n as u32)
        }
        Expression::Value(Value::Constant(crate::ir::Constant::Number(n))) => {
            Some(*n as u32)
        }
        _ => None,
    }
}

fn extract_array_of_integers(expr: &Expression) -> Option<Vec<u32>> {
    if let Expression::Array { elements } = expr {
        let values: Vec<u32> = elements
            .iter()
            .flatten()
            .filter_map(extract_integer)
            .collect();
        if !values.is_empty() {
            return Some(values);
        }
    }
    None
}

pub(crate) fn is_meaningful_name(name: &str) -> bool {
    !name.starts_with("f") && // f1234
    name.chars().any(|c| !c.is_ascii_digit()) && // Not purely numeric
    name != "exports" && 
    name != "wrapper" && 
    name != "require" &&
    name != "anonymous" &&
    name != "global" &&
    name != "_interopDefault" &&
    name != "_interopNamespace"
}
