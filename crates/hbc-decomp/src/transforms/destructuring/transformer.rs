use crate::ir::{Statement, AssignTarget, PropertyKey};
use super::utils::{extract_property_access, exprs_equal, get_index};
use super::arrays::try_array_destructuring;

/// Main in-place destructuring transform.
pub fn transform_destructuring(stmts: &mut Vec<Statement>) {
    let mut i = 0;
    while i < stmts.len() {
        // Look for start of a sequence
        if let Some((obj_expr, prop, target)) = extract_property_access(&stmts[i]) {
            // Check if obj_expr is simple (register/variable) to avoid side effects
            if !obj_expr.is_simple() {
                i += 1;
                continue;
            }

            let mut properties = Vec::new();
            properties.push((prop, target));

            let mut j = i + 1;
            while j < stmts.len() {
                // Skip comments
                if let Statement::Comment(_) = &stmts[j] {
                    j += 1;
                    continue;
                }

                if let Some((next_obj, next_prop, next_target)) = extract_property_access(&stmts[j]) {
                    if exprs_equal(&obj_expr, &next_obj) {
                        properties.push((next_prop, next_target));
                        j += 1;
                        continue;
                    }
                }
                break;
            }

            if properties.len() > 1 {
                // Detect Array vs Object based on property types
                let all_indices = properties.iter().all(|(k, _)| matches!(k, PropertyKey::Index(_) | PropertyKey::Computed(_)));
                let all_members = properties.iter().all(|(k, _)| matches!(k, PropertyKey::String(_) | PropertyKey::Ident(_)));

                if all_indices && try_array_destructuring(&properties) {
                    // Array Destructuring
                    let mut indexed_props: Vec<(i64, AssignTarget)> = properties.iter().filter_map(|(k, t)| {
                        get_index(k).map(|idx| (idx, t.clone()))
                    }).collect();

                    if indexed_props.is_empty() {
                        i += 1;
                        continue;
                    }

                    indexed_props.sort_by_key(|(idx, _)| *idx);
                    let max_idx = indexed_props.last().map(|(idx, _)| *idx).unwrap_or(0);

                    // Only create array destructuring if indices are consecutive from 0
                    let expected_count = (max_idx + 1) as usize;
                    if indexed_props.len() == expected_count && indexed_props[0].0 == 0 {
                        let mut targets: Vec<Option<AssignTarget>> = vec![None; expected_count];
                        for (idx, t) in indexed_props {
                            if idx >= 0 && (idx as usize) < targets.len() {
                                targets[idx as usize] = Some(t);
                            }
                        }

                        stmts[i] = Statement::Assign {
                            target: AssignTarget::DestructuringArray(targets),
                            value: obj_expr,
                        };

                        // Remove merged statements, keeping comments
                        let mut to_remove = Vec::new();
                        for (offset, stmt) in stmts[(i + 1)..j].iter().enumerate() {
                            if !matches!(stmt, Statement::Comment(_)) {
                                to_remove.push(i + 1 + offset);
                            }
                        }
                        for idx in to_remove.into_iter().rev() {
                            stmts.remove(idx);
                        }
                        i += 1;
                        continue;
                    }

                } else if all_members {
                    // Object Destructuring
                    let props: Vec<(String, AssignTarget)> = properties.into_iter().map(|(k, t)| {
                        let key = match k {
                            PropertyKey::String(s) => s,
                            PropertyKey::Ident(s) => s,
                            _ => String::new(),
                        };
                        (key, t)
                    }).filter(|(k, _)| !k.is_empty()).collect();

                    if props.len() > 1 {
                        stmts[i] = Statement::Assign {
                            target: AssignTarget::DestructuringObject(props),
                            value: obj_expr,
                        };

                        // Remove merged statements, keeping comments
                        let mut to_remove = Vec::new();
                        for (offset, stmt) in stmts[(i + 1)..j].iter().enumerate() {
                            if !matches!(stmt, Statement::Comment(_)) {
                                to_remove.push(i + 1 + offset);
                            }
                        }
                        for idx in to_remove.into_iter().rev() {
                            stmts.remove(idx);
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
