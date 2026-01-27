use crate::ir::{Statement, Expression, AssignTarget, Value};


// Transform sequence of "NewArray" + "PutByIndex" into "Array [ ... ]".
pub fn transform_array_literals(statements: &mut Vec<Statement>) {
    let mut i = 0;
    while i < statements.len() {
        // Look for: let arr_reg = NewArray(size);
        if let Some((arr_reg, size_hint)) = is_new_array(&statements[i]) {
            // Collect elements
            // We use a Map or sparse vector to collect indices
            let mut elements = std::collections::BTreeMap::new();
            let mut max_index = 0u32;
            let mut j = i + 1;
            let mut consumed_indices = Vec::new();
            
            while j < statements.len() {
                let stmt = &statements[j];
                
                if let Some((idx, val)) = is_put_index(stmt, arr_reg) {
                    elements.insert(idx, val);
                    if idx > max_index { max_index = idx; }
                    consumed_indices.push(j);
                } else if is_reg_used(stmt, arr_reg) || is_reg_assigned(stmt, arr_reg) {
                    // Block boundary
                    break;
                } else {
                    // Stop on side effects (unless we are sure they don't affect ordering, but safer to stop)
                    if stmt_has_side_effects(stmt) {
                       break; 
                    }
                }
                j += 1;
            }

            // Heuristic: only transform if we have elements OR explicit size 0
            if !elements.is_empty() || size_hint == Some(0) {
                // Determine array size. 
                // If we have elements, we construct an Array literal up to `max_index`.
                let array_len = if !elements.is_empty() { max_index + 1 } else { 0 };
                
                // Avoid reconstructing massive sparse arrays if gap is huge?
                if array_len < 1000 {
                    let mut array_elements = Vec::with_capacity(array_len as usize);
                    for k in 0..array_len {
                        array_elements.push(elements.get(&k).cloned());
                    }
                    
                    // Replace NewArray
                    if let Statement::Assign { target, .. } = &mut statements[i] {
                         *target = AssignTarget::Register(arr_reg);
                         statements[i] = Statement::Assign {
                             target: AssignTarget::Register(arr_reg),
                             value: Expression::Array { elements: array_elements },
                         };
                         
                         for &idx in consumed_indices.iter().rev() {
                             statements.remove(idx);
                         }
                         i += 1;
                         continue;
                    }
                }
            }
        }
        i += 1;
    }
}

fn is_new_array(stmt: &Statement) -> Option<(u32, Option<u32>)> {
    if let Statement::Assign { target: AssignTarget::Register(r), value: Expression::Unknown { opcode, operands } } = stmt {
        if opcode == "NewArray" {
            // Operand 0 is size hint
             let size = operands.first().and_then(|s| s.parse::<u32>().ok());
             return Some((*r, size));
        }
    }
    None
}

fn is_put_index(stmt: &Statement, arr_reg: u32) -> Option<(u32, Expression)> {
    // PutByIndex(arr, index, val) -> AssignTarget::Index
    if let Statement::Assign { target: AssignTarget::Index { object: Expression::Value(Value::Register(r)), key }, value } = stmt {
        if *r == arr_reg {
            // Key must be an integer constant
            if let Expression::Value(Value::Constant(crate::ir::Constant::Integer(idx))) = key {
                if *idx >= 0 {
                    return Some((*idx as u32, value.clone()));
                }
            }
            // Or number
            if let Expression::Value(Value::Constant(crate::ir::Constant::Number(n))) = key {
                 if *n >= 0.0 && n.fract() == 0.0 {
                     return Some((*n as u32, value.clone()));
                 }
            }
        }
    }
    None
}

fn is_reg_used(stmt: &Statement, reg: u32) -> bool {
    // Basic usage check
     match stmt {
        Statement::Assign { target, value } => {
            let target_uses = match target {
                AssignTarget::Index { object, key } => expr_uses(object, reg) || expr_uses(key, reg),
                AssignTarget::Member { object, .. } => expr_uses(object, reg),
                 _ => false
            };
            target_uses || expr_uses(value, reg)
        }
        Statement::Expr(e) => expr_uses(e, reg),
        Statement::Return(Some(e)) | Statement::Throw(e) => expr_uses(e, reg),
        Statement::If { condition, .. } => expr_uses(condition, reg),
        Statement::While { condition, .. } => expr_uses(condition, reg),
        Statement::Switch { discriminant, .. } => expr_uses(discriminant, reg), // simple check
        _ => false
    }
}

fn is_reg_assigned(stmt: &Statement, reg: u32) -> bool {
     match stmt {
        Statement::Assign { target: AssignTarget::Register(r), .. } => *r == reg,
         _ => false
    }
}

fn stmt_has_side_effects(stmt: &Statement) -> bool {
    match stmt {
        Statement::Assign { value, .. } => value.has_side_effects(),
        Statement::Expr(e) => e.has_side_effects(),
        Statement::Return(_) | Statement::Throw(_) => true,
        Statement::If { .. } | Statement::While { .. } => true,
        Statement::Switch { .. } | Statement::For { .. } => true,
        Statement::TryCatch { .. } => true,
        _ => true,
    }
}

fn expr_uses(expr: &Expression, reg: u32) -> bool {
     match expr {
         Expression::Value(Value::Register(r)) => *r == reg,
         Expression::Binary { left, right, .. } => expr_uses(left, reg) || expr_uses(right, reg),
         Expression::Unary { operand, .. } => expr_uses(operand, reg),
         Expression::Member { object, property, .. } => {
             expr_uses(object, reg) || (match property {
                 crate::ir::PropertyKey::Computed(k) => expr_uses(k, reg),
                 _ => false
             })
         },
         Expression::Call { callee, arguments } | Expression::New { callee, arguments } => {
             expr_uses(callee, reg) || arguments.iter().any(|a| expr_uses(a, reg))
         },
         Expression::Array { elements } => {
              elements.iter().flatten().any(|e| expr_uses(e, reg))
         },
         Expression::Object { properties } => {
              properties.iter().any(|p| expr_uses(&p.value, reg)) // simplified
         },
         _ => false
     }
}
