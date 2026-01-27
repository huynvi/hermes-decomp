use crate::ir::{Statement, Expression, AssignTarget, Value, PropertyKey};

pub fn infer_commonjs_names(statements: &mut [Statement], num_params: u32) -> Option<Vec<Option<String>>> {
    // Metro modules typically have 4 params: (global, require, module, exports).
    // Or 3 params: (require, module, exports) ?
    // Let's assume the common signature: function(global, require, module, exports).
    // If num_params >= 3, we can try to guess.
    
    // Heuristic 1: If param N is used as `paramN.exports = ...`, then N is likely `module`.
    // Heuristic 2: If param M is used as `paramM.prop = ...` and M != N, then M is likely `exports`.
    // Heuristic 3: If param K is called `paramK("string")`, it might be `require`.
    
    // We need to scan for `LoadParam` assignments to map registers to params.
    let mut param_map = std::collections::HashMap::new();
    
    for stmt in statements.iter() {
        if let Statement::Assign { target: AssignTarget::Register(r), value: Expression::Unknown { opcode, operands } } = stmt {
            if opcode == "LoadParam" {
                // Operand 0 is param index
                if let Ok(idx) = operands[0].parse::<u32>() {
                    param_map.insert(*r, idx);
                }
            }
        }
    }
    
    if param_map.is_empty() {
        return None;
    }

    // Scan usages
    let mut module_reg = None;
    // exports_reg unused for now, directly naming param

    
    // Simple scan for `r.exports = ...` pattern
    for stmt in statements.iter() {
        if let Statement::Assign { target: AssignTarget::Member { object, property }, .. } = stmt {
             if property == "exports" {
                 if let Expression::Value(Value::Register(r)) = object {
                     if let Some(&p_idx) = param_map.get(r) {
                         module_reg = Some(p_idx);
                     }
                 }
             }
        }
    }
    
    // If we found module, we can guess exports is likely another param.
    // If param structure matches Metro (p0, p1, p2=module, p3=exports), we can assign names.
    
    let mut names = vec![None; num_params as usize];
    
    if let Some(mod_idx) = module_reg {
        if (mod_idx as usize) < names.len() {
            names[mod_idx as usize] = Some("module".to_string());
        }
    }
    
    // If we have 4 params and p2 is module, p3 is likely exports, p1 require, p0 global.
    if num_params == 4
         && module_reg == Some(2) {
             names[0] = Some("global".to_string());
             names[1] = Some("require".to_string());
             names[3] = Some("exports".to_string());
         }
    
    // If we have 3 params and p1 is module
    if num_params == 3
        && module_reg == Some(1) {
             names[0] = Some("require".to_string());
             names[2] = Some("exports".to_string());
        }

    // Verify if we found anything interesting
    if names.iter().all(|n| n.is_none()) {
        return None;
    }

    Some(names)
}

pub fn rename_param_registers(statements: &mut [Statement], names: &[Option<String>]) {
    // 1. Map of Reg -> Name
    let mut reg_rename_map = std::collections::HashMap::new();
    
    // 2. Map of VarName -> Name
    let mut var_rename_map = std::collections::HashMap::new();
    
    // Scan for LoadParam to get registers
    for stmt in statements.iter() {
        if let Statement::Assign { target: AssignTarget::Register(r), value: Expression::Unknown { opcode, operands } } = stmt {
            if opcode == "LoadParam" {
                if let Ok(idx) = operands[0].parse::<usize>() {
                    if idx < names.len() {
                        if let Some(name) = &names[idx] {
                            reg_rename_map.insert(*r, name.clone());
                        }
                    }
                }
            }
        }
    }

    // Always include argN -> Name mappings
    for (idx, name_opt) in names.iter().enumerate() {
        if let Some(name) = name_opt {
            var_rename_map.insert(format!("arg{}", idx), name.clone());
        }
    }
    
    if reg_rename_map.is_empty() && var_rename_map.is_empty() { return; }
    
    // 3. Rename usages
    for stmt in statements.iter_mut() {
        rename_stmt(stmt, &reg_rename_map, &var_rename_map);
    }
}

fn rename_stmt(
    stmt: &mut Statement, 
    reg_map: &std::collections::HashMap<u32, String>,
    var_map: &std::collections::HashMap<String, String>,
) {
    match stmt {
        Statement::Let { name, value, .. } => {
            if let Some(new_name) = var_map.get(name) {
                *name = new_name.clone();
            }
            rename_expr(value, reg_map, var_map);
        }
        Statement::Assign { target, value } => {
            rename_target(target, reg_map, var_map);
            rename_expr(value, reg_map, var_map);
        }
        Statement::Expr(e) => rename_expr(e, reg_map, var_map),
        Statement::Return(Some(e)) | Statement::Throw(e) => rename_expr(e, reg_map, var_map),
        Statement::If { condition, then_body, else_body } => {
            rename_expr(condition, reg_map, var_map);
            for s in then_body { rename_stmt(s, reg_map, var_map); }
            for s in else_body { rename_stmt(s, reg_map, var_map); }
        }
        Statement::While { condition, body } => {
             rename_expr(condition, reg_map, var_map);
             for s in body { rename_stmt(s, reg_map, var_map); }
        }
        Statement::Block(body) => {
             for s in body { rename_stmt(s, reg_map, var_map); }
        }
        Statement::For { init, condition, update, body } => {
             if let Some(s) = init { rename_stmt(s, reg_map, var_map); }
             if let Some(e) = condition { rename_expr(e, reg_map, var_map); }
             if let Some(s) = update { rename_stmt(s, reg_map, var_map); }
             for s in body { rename_stmt(s, reg_map, var_map); }
        }
        Statement::Switch { discriminant, cases, default } => {
             rename_expr(discriminant, reg_map, var_map);
             for (val, body) in cases {
                 rename_expr(val, reg_map, var_map);
                 for s in body { rename_stmt(s, reg_map, var_map); }
             }
             if let Some(body) = default {
                 for s in body { rename_stmt(s, reg_map, var_map); }
             }
        }
        Statement::TryCatch { try_body, catch_param, catch_body, finally_body } => {
             for s in try_body { rename_stmt(s, reg_map, var_map); }
             if let Some(p) = catch_param {
                  if let Some(new_p) = var_map.get(p) {
                      *catch_param = Some(new_p.clone());
                  }
             }
             for s in catch_body { rename_stmt(s, reg_map, var_map); }
             for s in finally_body { rename_stmt(s, reg_map, var_map); }
        }
        Statement::Class { name, super_class, methods, .. } => {
             if let Some(new_name) = var_map.get(name) {
                 *name = new_name.clone();
             }
             if let Some(sc) = super_class { rename_expr(sc, reg_map, var_map); }
             for m in methods {
                  if let Some(b) = &mut m.body {
                      for s in b { rename_stmt(s, reg_map, var_map); }
                  }
             }
        }
        _ => {}
    }
}

fn rename_target(
    target: &mut AssignTarget, 
    reg_map: &std::collections::HashMap<u32, String>,
    var_map: &std::collections::HashMap<String, String>,
) {
    match target {
        AssignTarget::Register(r) => {
            if let Some(name) = reg_map.get(r) {
                *target = AssignTarget::Variable(name.clone());
            }
        }
        AssignTarget::Variable(v) => {
            if let Some(name) = var_map.get(v) {
                *v = name.clone();
            }
        }
        AssignTarget::Member { object, .. } => rename_expr(object, reg_map, var_map),
        AssignTarget::Index { object, key } => {
            rename_expr(object, reg_map, var_map);
            rename_expr(key, reg_map, var_map);
        }
        AssignTarget::DestructuringArray(targets) => {
             for t in targets.iter_mut().flatten() {
                 rename_target(t, reg_map, var_map);
             }
        }
        AssignTarget::DestructuringObject(targets) => {
             for (_, t) in targets {
                 rename_target(t, reg_map, var_map);
             }
        }
        _ => {}
    }
}

fn rename_expr(
    expr: &mut Expression, 
    reg_map: &std::collections::HashMap<u32, String>,
    var_map: &std::collections::HashMap<String, String>,
) {
    match expr {
        Expression::Value(Value::Register(r)) => {
            if let Some(name) = reg_map.get(r) {
                *expr = Expression::Value(Value::Variable(name.clone()));
            }
        }
        Expression::Value(Value::Variable(v)) => {
            if let Some(name) = var_map.get(v) {
                *v = name.clone();
            }
        }
        Expression::Value(Value::Parameter(idx)) => {
            // This is direct Parameter access (argN)
            // We can rename it to Variable(name) if we have it
            if let Some(Some(name)) = var_map.get(&format!("arg{}", idx)).map(|s| Some(s)) {
                 // Wait, var_map.get returns &String. 
                 // If we have a name, convert to Variable.
                 *expr = Expression::Value(Value::Variable(name.clone()));
            }
        }
        Expression::Binary { left, right, .. } => {
            rename_expr(left, reg_map, var_map);
            rename_expr(right, reg_map, var_map);
        }
        Expression::Unary { operand, .. } => rename_expr(operand, reg_map, var_map),
        Expression::Member { object, property, .. } => {
            rename_expr(object, reg_map, var_map);
            if let PropertyKey::Computed(k) = property {
                rename_expr(k, reg_map, var_map);
            }
        }
        Expression::Call { callee, arguments } | Expression::New { callee, arguments } => {
             rename_expr(callee, reg_map, var_map);
             for a in arguments { rename_expr(a, reg_map, var_map); }
        }
        Expression::Object { properties } => {
             for p in properties {
                 rename_expr(&mut p.value, reg_map, var_map);
                 if let PropertyKey::Computed(k) = &mut p.key {
                     rename_expr(k, reg_map, var_map);
                 }
             }
        }
        Expression::Array { elements } => {
             for e in elements.iter_mut().flatten() {
                 rename_expr(e, reg_map, var_map);
             }
        }
        Expression::Assignment { target, value } => {
             rename_expr(target, reg_map, var_map);
             rename_expr(value, reg_map, var_map);
        }
        Expression::Spread(e) => rename_expr(e, reg_map, var_map),
        Expression::TemplateLiteral { expressions, .. } => {
             for e in expressions { rename_expr(e, reg_map, var_map); }
        }
        Expression::Yield { value, .. } | Expression::Await(value) => rename_expr(value, reg_map, var_map),
        Expression::Conditional { condition, then_expr, else_expr } => {
             rename_expr(condition, reg_map, var_map);
             rename_expr(then_expr, reg_map, var_map);
             rename_expr(else_expr, reg_map, var_map);
        }
        _ => {}
    }
}
