use crate::ir::{Statement, Expression, PropertyKey, ObjectProperty, AssignTarget, Value};


// Transform sequence of "NewObject" + "PutById" into "Object { ... }".
pub fn transform_object_literals(statements: &mut Vec<Statement>) {
    let mut i = 0;
    while i < statements.len() {
        // Look for: let obj_reg = NewObject(parent);
        if let Some((obj_reg, _)) = is_new_object(&statements[i]) {
            // Collect properties
            let mut properties = Vec::new();
            let mut j = i + 1;
            let mut consumed_indices = Vec::new();
            
            while j < statements.len() {
                let stmt = &statements[j];
                
                if is_put_prop(stmt, obj_reg, &mut properties) {
                    consumed_indices.push(j);
                } else if is_reg_used(stmt, obj_reg) || is_reg_assigned(stmt, obj_reg) {
                    // Block boundary
                    break;
                } else {
                    // Stop on any statement with side effects for safety
                    if stmt_has_side_effects(stmt) {
                       break; 
                    }
                }
                j += 1;
            }

            if !properties.is_empty() {
                // Replace the NewObject call
                if let Statement::Assign { target, .. } = &mut statements[i] {
                    // Create Object expression
                    *target = AssignTarget::Register(obj_reg); 
                    statements[i] = Statement::Assign {
                        target: AssignTarget::Register(obj_reg),
                        value: Expression::Object { properties },
                    };
                    
                    for &idx in consumed_indices.iter().rev() {
                        statements.remove(idx);
                    }
                    
                    i += 1; 
                    continue; 
                }
            }
        }
        i += 1;
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
        Statement::Comment(_) => false,
        _ => true,
    }
}

fn is_new_object(stmt: &Statement) -> Option<(u32, usize)> {
    if let Statement::Assign { target: AssignTarget::Register(r), value: Expression::New { .. } } = stmt {
        return Some((*r, 0));
    }
    if let Statement::Assign { target: AssignTarget::Register(r), value: Expression::Object { properties } } = stmt {
        if properties.is_empty() {
             return Some((*r, 0));
        }
    }
    if let Statement::Assign { target: AssignTarget::Register(r), value: Expression::Unknown { opcode, .. } } = stmt {
        if opcode == "NewObject" || opcode == "NewObjectWithBuffer" {
            return Some((*r, 0));
        }
    }

    None
}

fn is_put_prop(stmt: &Statement, obj_reg: u32, props: &mut Vec<ObjectProperty>) -> bool {
    // Correct struct pattern for Member variant (property is String)
    if let Statement::Assign { target: AssignTarget::Member { object: Expression::Value(Value::Register(r)), property }, value } = stmt {
        if *r == obj_reg {
            props.push(ObjectProperty {
                key: PropertyKey::Ident(property.clone()),
                value: value.clone(),
            });
            return true;
        }
    }
    // Also check Index (computed)
    if let Statement::Assign { target: AssignTarget::Index { object: Expression::Value(Value::Register(r)), key }, value } = stmt {
        if *r == obj_reg {
            props.push(ObjectProperty {
                key: PropertyKey::Computed(Box::new(key.clone())),
                value: value.clone(),
            });
            return true;
        }
    }
    
    // Check Unknown opcodes
    if let Statement::Expr(Expression::Unknown { opcode, .. }) = stmt {
        if opcode == "PutById" {
             // Ignored
        }
    }
    
    false
}

fn is_reg_assigned(stmt: &Statement, reg: u32) -> bool {
    match stmt {
        Statement::Assign { target: AssignTarget::Register(r), .. } => *r == reg,
        _ => false
    }
}

fn is_reg_used(stmt: &Statement, reg: u32) -> bool {
    match stmt {
        Statement::Assign { target, value } => {
            let target_uses = match target {
                AssignTarget::Member { object, .. } => {
                     // object is Expression
                     expr_uses(object, reg)
                },
                AssignTarget::Index { object, key } => {
                    expr_uses(object, reg) || expr_uses(key, reg)
                }
                _ => false 
            };
            target_uses || expr_uses(value, reg) 
        }
        Statement::Expr(e) => expr_uses(e, reg),
        Statement::Return(Some(e)) | Statement::Throw(e) => expr_uses(e, reg),
        Statement::If { condition, .. } => expr_uses(condition, reg),
        Statement::While { condition, .. } => expr_uses(condition, reg),
        _ => false 
    }
}

fn expr_uses(expr: &Expression, reg: u32) -> bool {
     match expr {
         Expression::Value(Value::Register(r)) => *r == reg,
         Expression::Binary { left, right, .. } => expr_uses(left, reg) || expr_uses(right, reg),
         Expression::Unary { operand, .. } => expr_uses(operand, reg),
         Expression::Member { object, property, .. } => expr_uses(object, reg) || key_uses(property, reg),
         Expression::Call { callee, arguments } | Expression::New { callee, arguments } => {
             expr_uses(callee, reg) || arguments.iter().any(|a| expr_uses(a, reg))
         },
         Expression::Object { properties } => {
             properties.iter().any(|p| expr_uses(&p.value, reg) || key_uses(&p.key, reg))
         },
         Expression::Array { elements } => {
             elements.iter().flatten().any(|e| expr_uses(e, reg))
         },
         _ => false
     }
}

fn key_uses(key: &PropertyKey, reg: u32) -> bool {
    match key {
        PropertyKey::Computed(e) => expr_uses(e, reg),
        _ => false
    }
}
