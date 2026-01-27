use crate::ir::{Statement, Expression, Value, Constant, BinaryOp, PropertyKey, AssignTarget};

// Detect for-of loop patterns.
pub fn detect_for_of_loops(stmts: Vec<Statement>) -> Vec<Statement> {
    let mut result = Vec::new();
    let mut iter = stmts.into_iter().peekable();

    while let Some(stmt) = iter.next() {
        // Look for: iter = source[Symbol.iterator]()
        if let Statement::Assign { target: AssignTarget::Register(iter_reg), value } = &stmt {
            if let Some((source_expr, _)) = is_iterator_call(value) {
                // Check if next statement is a while(true) loop
                if let Some(Statement::While { condition, body }) = iter.peek() {
                    if is_true_condition(condition) {
                        if let Some((var_name, loop_body)) = extract_for_of_body(body, *iter_reg) {
                            // Found for-of pattern!
                            iter.next(); // consume the while
                            result.push(Statement::ForOf {
                                variable: var_name,
                                iterable: source_expr.clone(),
                                body: detect_for_of_loops(loop_body),
                            });
                            continue;
                        }
                    }
                }
            }
        }

        // Recursively transform nested statements
        let transformed = match stmt {
            Statement::While { condition, body } => Statement::While {
                condition,
                body: detect_for_of_loops(body),
            },
            Statement::If { condition, then_body, else_body } => Statement::If {
                condition,
                then_body: detect_for_of_loops(then_body),
                else_body: detect_for_of_loops(else_body),
            },
            Statement::Block(inner) => Statement::Block(detect_for_of_loops(inner)),
            Statement::For { init, condition, update, body } => Statement::For {
                init,
                condition,
                update,
                body: detect_for_of_loops(body),
            },
            other => other,
        };
        result.push(transformed);
    }

    result
}

// Check if expression is a call to [Symbol.iterator]()
fn is_iterator_call(expr: &Expression) -> Option<(Expression, ())> {
    if let Expression::Call { callee, arguments } = expr {
        if arguments.is_empty() {
            if let Expression::Member { object, property, .. } = callee.as_ref() {
                // Check for [Symbol.iterator] pattern
                if let PropertyKey::Computed(computed) = property {
                    if let Expression::Member { object: symbol_obj, property: PropertyKey::Ident(iter_prop), .. } = computed.as_ref() {
                        if let Expression::Value(Value::Variable(name)) = symbol_obj.as_ref() {
                            if name == "Symbol" && iter_prop == "iterator" {
                                return Some((*object.clone(), ()));
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

// Check if condition is `true`
fn is_true_condition(expr: &Expression) -> bool {
    matches!(expr, Expression::Value(Value::Constant(Constant::Bool(true))))
}

// Extract for-of loop body from while body.
fn extract_for_of_body(body: &[Statement], iter_reg: u32) -> Option<(String, Vec<Statement>)> {
    if body.len() < 3 {
        return None;
    }

    // First statement: result = iter.next()
    let result_reg = if let Statement::Assign { target: AssignTarget::Register(r), value } = &body[0] {
        if let Expression::Call { callee, arguments } = value {
            if arguments.is_empty() {
                if let Expression::Member { object, property: PropertyKey::Ident(prop), .. } = callee.as_ref() {
                    if prop == "next" {
                        if let Expression::Value(Value::Register(iter_r)) = object.as_ref() {
                            if *iter_r == iter_reg {
                                Some(*r)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    }?;

    // Second statement: if (result.done) break  (or similar pattern)
    // This might be: if (result.done) { break } or if (!result.done) { ... } else { break }
    let body_start = if let Statement::If { condition, then_body, else_body: _ } = &body[1] {
        // Check if condition is result.done
        if is_done_check(condition, result_reg) {
            // if (result.done) break pattern
            if then_body.iter().any(|s| matches!(s, Statement::Comment(c) if c == "break")) ||
               then_body.is_empty() {
                2 // body starts at index 2
            } else {
                return None;
            }
        } else {
            return None;
        }
    } else {
        return None;
    };

    // Third statement: item = result.value
    let (item_name, value_stmt_idx) = if let Statement::Assign { target, value } = &body[body_start] {
        if let Expression::Member { object, property: PropertyKey::Ident(prop), .. } = value {
            if prop == "value" {
                if let Expression::Value(Value::Register(r)) = object.as_ref() {
                    if *r == result_reg {
                        let name = match target {
                            AssignTarget::Register(r) => format!("item{r}"),
                            AssignTarget::Variable(v) => v.clone(),
                            _ => return None,
                        };
                        Some((name, body_start + 1))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    }?;

    // Remaining statements are the loop body
    let loop_body = body[value_stmt_idx..].to_vec();

    Some((item_name, loop_body))
}

// Check if expression is result.done
fn is_done_check(expr: &Expression, result_reg: u32) -> bool {
    if let Expression::Member { object, property: PropertyKey::Ident(prop), .. } = expr {
        if prop == "done" {
            if let Expression::Value(Value::Register(r)) = object.as_ref() {
                return *r == result_reg;
            }
        }
    }
    false
}

// Detect for-in loop patterns.
pub fn detect_for_in_loops(stmts: Vec<Statement>) -> Vec<Statement> {
    let mut result = Vec::new();
    let mut iter = stmts.into_iter().peekable();

    while let Some(stmt) = iter.next() {
        // Look for: keys = Object.keys(obj)
        if let Statement::Assign { target: AssignTarget::Register(keys_reg), value } = &stmt {
            if let Some(obj_expr) = is_object_keys_call(value) {
                // Look for i = 0 followed by while loop
                if let Some(Statement::Assign { target: AssignTarget::Register(idx_reg), value: idx_value }) = iter.peek() {
                    if is_zero(idx_value) {
                        let idx_reg = *idx_reg;
                        iter.next(); // consume i = 0

                        if let Some(Statement::While { condition, body }) = iter.peek() {
                            if is_length_check(condition, idx_reg, *keys_reg) {
                                if let Some((var_name, loop_body)) = extract_for_in_body(body, *keys_reg, idx_reg) {
                                    iter.next(); // consume while
                                    result.push(Statement::ForIn {
                                        variable: var_name,
                                        object: obj_expr.clone(),
                                        body: detect_for_in_loops(loop_body),
                                    });
                                    continue;
                                }
                            }
                        }

                        // Didn't match for-in, push the i = 0 we consumed
                        result.push(stmt);
                        // Note: We already consumed iter.next() for i=0, need to handle this
                        // For simplicity, we'll just fall through and let the while be processed normally
                        continue;
                    }
                }
            }
        }

        // Recursively transform nested statements
        let transformed = match stmt {
            Statement::While { condition, body } => Statement::While {
                condition,
                body: detect_for_in_loops(body),
            },
            Statement::If { condition, then_body, else_body } => Statement::If {
                condition,
                then_body: detect_for_in_loops(then_body),
                else_body: detect_for_in_loops(else_body),
            },
            Statement::Block(inner) => Statement::Block(detect_for_in_loops(inner)),
            Statement::For { init, condition, update, body } => Statement::For {
                init,
                condition,
                update,
                body: detect_for_in_loops(body),
            },
            other => other,
        };
        result.push(transformed);
    }

    result
}

// Check if expression is Object.keys(obj)
fn is_object_keys_call(expr: &Expression) -> Option<Expression> {
    if let Expression::Call { callee, arguments } = expr {
        if arguments.len() == 1 {
            if let Expression::Member { object, property: PropertyKey::Ident(prop), .. } = callee.as_ref() {
                if prop == "keys" {
                    if let Expression::Value(Value::Variable(name)) = object.as_ref() {
                        if name == "Object" {
                            return Some(arguments[0].clone());
                        }
                    }
                }
            }
        }
    }
    None
}

// Check if expression is 0
fn is_zero(expr: &Expression) -> bool {
    match expr {
        Expression::Value(Value::Constant(Constant::Integer(0))) => true,
        Expression::Value(Value::Constant(Constant::Number(n))) => *n == 0.0,
        _ => false,
    }
}

// Check if expression is i < keys.length
fn is_length_check(expr: &Expression, idx_reg: u32, keys_reg: u32) -> bool {
    if let Expression::Binary { op: BinaryOp::Lt, left, right } = expr {
        // Check left is idx_reg
        if let Expression::Value(Value::Register(r)) = left.as_ref() {
            if *r == idx_reg {
                // Check right is keys.length
                if let Expression::Member { object, property: PropertyKey::Ident(prop), .. } = right.as_ref() {
                    if prop == "length" {
                        if let Expression::Value(Value::Register(r)) = object.as_ref() {
                            return *r == keys_reg;
                        }
                    }
                }
            }
        }
    }
    false
}

// Extract for-in body from while body
fn extract_for_in_body(body: &[Statement], keys_reg: u32, idx_reg: u32) -> Option<(String, Vec<Statement>)> {
    if body.is_empty() {
        return None;
    }

    // First statement: key = keys[i]
    let (var_name, body_start) = if let Statement::Assign { target, value } = &body[0] {
        if let Expression::Member { object, property: PropertyKey::Computed(idx_expr), .. } = value {
            if let Expression::Value(Value::Register(keys_r)) = object.as_ref() {
                if *keys_r == keys_reg {
                    if let Expression::Value(Value::Register(idx_r)) = idx_expr.as_ref() {
                        if *idx_r == idx_reg {
                            let name = match target {
                                AssignTarget::Register(r) => format!("key{r}"),
                                AssignTarget::Variable(v) => v.clone(),
                                _ => return None,
                            };
                            Some((name, 1))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    }?;

    // Remove the increment statement (i++) from the end
    let loop_body = if body.len() > body_start {
        let last_idx = body.len() - 1;
        if is_increment(&body[last_idx], idx_reg) {
            body[body_start..last_idx].to_vec()
        } else {
            body[body_start..].to_vec()
        }
    } else {
        vec![]
    };

    Some((var_name, loop_body))
}

// Check if statement is i++ or i = i + 1
fn is_increment(stmt: &Statement, reg: u32) -> bool {
    if let Statement::Assign { target: AssignTarget::Register(r), value } = stmt {
        if *r == reg {
            if let Expression::Binary { op: BinaryOp::Add, left, right } = value {
                // i = i + 1
                if let Expression::Value(Value::Register(lr)) = left.as_ref() {
                    if *lr == reg {
                        if let Expression::Value(Value::Constant(Constant::Integer(1))) = right.as_ref() {
                            return true;
                        }
                    }
                }
                // i = 1 + i
                if let Expression::Value(Value::Register(rr)) = right.as_ref() {
                    if *rr == reg {
                        if let Expression::Value(Value::Constant(Constant::Integer(1))) = left.as_ref() {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

// Detect for loop patterns from while loops.
pub fn detect_for_loops(stmts: Vec<Statement>) -> Vec<Statement> {
    let mut result = Vec::new();
    let mut iter = stmts.into_iter().peekable();

    while let Some(stmt) = iter.next() {
        match stmt {
            // Check if this assignment might be a for-loop init followed by while
            Statement::Assign { target, value } => {
                if let Some(Statement::While { condition, body }) = iter.peek() {
                    // Check if this is a for-loop pattern
                    if let Some((update, new_body)) = extract_for_loop_update(body) {
                        // Check if the while condition uses the same variable
                        if uses_variable(condition, &target) {
                            let Some(while_stmt) = iter.next() else { continue };
                            if let Statement::While { condition, body: _ } = while_stmt {
                                result.push(Statement::For {
                                    init: Some(Box::new(Statement::Assign {
                                        target,
                                        value,
                                    })),
                                    condition: Some(condition),
                                    update: Some(Box::new(update)),
                                    body: detect_for_loops(new_body),
                                });
                                continue;
                            }
                        }
                    }
                }
                result.push(Statement::Assign { target, value });
            }
            Statement::While { condition, body } => {
                // Check for for-loop pattern without preceding init
                if let Some((update, new_body)) = extract_for_loop_update(&body) {
                    result.push(Statement::For {
                        init: None,
                        condition: Some(condition),
                        update: Some(Box::new(update)),
                        body: detect_for_loops(new_body),
                    });
                } else {
                    result.push(Statement::While {
                        condition,
                        body: detect_for_loops(body),
                    });
                }
            }
            Statement::If { condition, then_body, else_body } => {
                result.push(Statement::If {
                    condition,
                    then_body: detect_for_loops(then_body),
                    else_body: detect_for_loops(else_body),
                });
            }
            Statement::Block(inner) => {
                result.push(Statement::Block(detect_for_loops(inner)));
            }
            other => result.push(other),
        }
    }
    result
}

// Extract a for-loop update statement from the end of a while body.
fn extract_for_loop_update(body: &[Statement]) -> Option<(Statement, Vec<Statement>)> {
    if body.is_empty() {
        return None;
    }

    let last = body.last()?;

    // Look for increment patterns: i = i + 1, i++, ++i
    match last {
        Statement::Assign { target, value: Expression::Binary { op, left, right } }
            if matches!(op, BinaryOp::Add | BinaryOp::Sub)
                && (is_same_target(target, left) || is_same_target(target, right)) =>
        {
            let new_body = body[..body.len() - 1].to_vec();
            Some((last.clone(), new_body))
        }
        Statement::Expr(Expression::Assignment { .. }) => {
            let new_body = body[..body.len() - 1].to_vec();
            Some((last.clone(), new_body))
        }
        _ => None,
    }
}

// Check if an expression uses a given assignment target.
fn uses_variable(expr: &Expression, target: &crate::ir::AssignTarget) -> bool {
    match (expr, target) {
        (Expression::Value(Value::Register(r1)), crate::ir::AssignTarget::Register(r2)) => r1 == r2,
        (Expression::Value(Value::Variable(v1)), crate::ir::AssignTarget::Variable(v2)) => v1 == v2,
        (Expression::Binary { left, right, .. }, _) => {
            uses_variable(left, target) || uses_variable(right, target)
        }
        _ => false,
    }
}

// Check if an expression is the same as an assignment target.
fn is_same_target(target: &crate::ir::AssignTarget, expr: &Expression) -> bool {
    match (target, expr) {
        (crate::ir::AssignTarget::Register(r1), Expression::Value(Value::Register(r2))) => r1 == r2,
        (crate::ir::AssignTarget::Variable(v1), Expression::Value(Value::Variable(v2))) => v1 == v2,
        _ => false,
    }
}
