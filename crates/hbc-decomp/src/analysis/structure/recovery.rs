use std::collections::HashSet;
use crate::ir::{CFG, BlockId, Statement, Terminator, Expression};
use crate::analysis::loops::{detect_loops, LoopInfo};
use super::Structure;

// Recovers high-level structures from CFG.
/// 
/// This is the "Structurization" phase.
/// The CFG (Control Flow Graph) is a graph of basic blocks with jumps.
/// We need to convert this back into nested structures like `if`, `while`, `for`.
///
/// Strategy:
/// 1. Detect loops (headers, bodies, exits) using graph analysis.
/// 2. Traverse the graph starting from entry.
/// 3. Recursively match patterns:
///    - If a node dominates 2 children that merge back, it's an `if`.
///    - If a node is a loop header, recurse into the loop body.
///    - Handle `break` and `continue` by checking loop stacks.
pub fn analyze(cfg: &CFG) -> (Structure, Vec<LoopInfo>) {
    let loops = detect_loops(cfg);
    let loop_headers: HashSet<_> = loops.iter().map(|l| l.header).collect();
    let mut visited = HashSet::new();
    let loop_stack = Vec::new();
    let root = recover_structure(cfg, cfg.entry, &loops, &loop_headers, &mut visited, &loop_stack);
    (root, loops)
}

fn recover_structure(
    cfg: &CFG,
    block_id: BlockId,
    loops: &[LoopInfo],
    _loop_headers: &HashSet<BlockId>,
    visited: &mut HashSet<BlockId>,
    loop_stack: &[&LoopInfo],
) -> Structure {
    if visited.contains(&block_id) {
        // If we're jumping to a loop header we're in, it's a continue
        for (i, loop_info) in loop_stack.iter().enumerate().rev() {
            if block_id == loop_info.header {
                 let label = if i < loop_stack.len() - 1 { Some(format!("label{i}")) } else { None };
                 return Structure::Continue(label);
            }
            if loop_info.exit == Some(block_id) {
                 let label = if i < loop_stack.len() - 1 { Some(format!("label{i}")) } else { None };
                 return Structure::Break(label);
            }
        }
        return Structure::Block(block_id, vec![]);
    }

    // Check if this is a loop header
    if let Some(loop_info) = loops.iter().find(|l| l.header == block_id) {
        if !visited.contains(&block_id) {
            return recover_loop(cfg, loop_info, loops, _loop_headers, visited, loop_stack);
        }
    }

    visited.insert(block_id);

    let block = match cfg.get(block_id) {
        Some(b) => b,
        None => return Structure::Block(block_id, vec![]),
    };

    let stmts = block.statements.clone();

    match &block.terminator {
        Terminator::Return(e) => {
            let mut all = stmts;
            all.push(Statement::Return(e.clone()));
            Structure::Block(block_id, all)
        }
        Terminator::Throw(e) => {
            let mut all = stmts;
            all.push(Statement::Throw(e.clone()));
            Structure::Block(block_id, all)
        }
        Terminator::Jump(target) => {
            // Check for loop continue/break
            for (i, loop_info) in loop_stack.iter().enumerate().rev() {
                if *target == loop_info.header && visited.contains(target) {
                    let label = if i < loop_stack.len() - 1 { Some(format!("label{i}")) } else { None };
                    let mut stmts = stmts;
                    stmts.push(Statement::Comment(if let Some(l) = label { format!("continue {l}") } else { "continue".to_string() }));
                    return Structure::Block(block_id, stmts);
                }
                if loop_info.exit == Some(*target) {
                    let label = if i < loop_stack.len() - 1 { Some(format!("label{i}")) } else { None };
                    let mut stmts = stmts;
                    stmts.push(Statement::Comment(if let Some(l) = label { format!("break {l}") } else { "break".to_string() }));
                    return Structure::Block(block_id, stmts);
                }
            }

            let block_struct = Structure::Block(block_id, stmts);
            let next = recover_structure(cfg, *target, loops, _loop_headers, visited, loop_stack);
            Structure::Sequence(vec![block_struct, next])
        }
        Terminator::Branch { condition, true_target, false_target } => {
            // Check for loop patterns
            if let Some(loop_info) = loop_stack.last() {
                // Break pattern: branch out of loop
                let true_exits = loop_info.exit == Some(*true_target);
                let false_exits = loop_info.exit == Some(*false_target);

                if true_exits && !false_exits {
                    // if (cond) break; else continue_body
                    let mut parts = vec![Structure::Block(block_id, stmts.clone())];
                    let else_ = recover_structure(cfg, *false_target, loops, _loop_headers, visited, loop_stack);
                    parts.push(Structure::If {
                        condition: condition.clone(),
                        then_: Box::new(Structure::Break(None)),
                        else_: Box::new(else_),
                    });
                    return Structure::Sequence(parts);
                }

                if false_exits && !true_exits {
                    // if (!cond) break; else continue_body
                    let mut parts = vec![Structure::Block(block_id, stmts.clone())];
                    let then_ = recover_structure(cfg, *true_target, loops, _loop_headers, visited, loop_stack);
                    parts.push(Structure::If {
                        condition: Expression::unary(crate::ir::UnaryOp::Not, condition.clone()),
                        then_: Box::new(Structure::Break(None)),
                        else_: Box::new(then_),
                    });
                    return Structure::Sequence(parts);
                }
            }

            let then_ = recover_structure(cfg, *true_target, loops, _loop_headers, visited, loop_stack);
            let else_ = recover_structure(cfg, *false_target, loops, _loop_headers, visited, loop_stack);
            let mut parts = vec![Structure::Block(block_id, stmts)];
            parts.push(Structure::If {
                condition: condition.clone(),
                then_: Box::new(then_),
                else_: Box::new(else_),
            });
            Structure::Sequence(parts)
        }
        Terminator::Switch { value, cases, default } => {
            let mut parts = vec![Structure::Block(block_id, stmts)];
            
            let mut switch_cases = Vec::new();
            for (case_val, target) in cases {
                 let case_body = recover_structure(cfg, *target, loops, _loop_headers, visited, loop_stack);
                 switch_cases.push((case_val.clone(), case_body));
            }
            
            let default_body = recover_structure(cfg, *default, loops, _loop_headers, visited, loop_stack);
            
            parts.push(Structure::Switch {
                discriminant: value.clone(),
                cases: switch_cases,
                default: Box::new(default_body),
            });
            
            Structure::Sequence(parts)
        }
        Terminator::None => Structure::Block(block_id, stmts),
    }
}

fn recover_loop(
    cfg: &CFG,
    loop_info: &LoopInfo,
    loops: &[LoopInfo],
    loop_headers: &HashSet<BlockId>,
    visited: &mut HashSet<BlockId>,
    loop_stack: &[&LoopInfo],
) -> Structure {
    visited.insert(loop_info.header);

    let header = match cfg.get(loop_info.header) {
        Some(b) => b,
        None => return Structure::Block(loop_info.header, vec![]),
    };

    let header_stmts = header.statements.clone();

    // Check if we can identify an update block
    // An update block is the single back-edge source to the header.
    // In a `for(init; cond; update)`, the update block is executed after the body.
    let update_block = if loop_info.back_edges.len() == 1 {
        Some(loop_info.back_edges[0].0)
    } else {
        None
    };

    // Helper to wrap body in Loop/While structure
    let make_loop = |body: Structure, condition: Expression| -> Structure {
        let loop_struct = if let Some(update_id) = update_block {
             // Try to peel the update block from the end of the body
             // This extracts the `i++` from the end of the loop body to place it in the `for` update slot.
             if let Some((new_body, update_struct)) = split_update_from_body(body.clone(), update_id) {
                 Structure::For {
                     init: Box::new(Structure::Block(BlockId(0), vec![])), // Init is handled outside (before loop)
                     condition,
                     update: Box::new(update_struct),
                     body: Box::new(new_body),
                 }
             } else {
                 Structure::While { condition, body: Box::new(body) }
             }
        } else {
           Structure::While { condition, body: Box::new(body) }
        };
        
        // Label the loop if nested
        let label_name = format!("label{}", loop_stack.len());
        Structure::Label(label_name, Box::new(loop_struct))
    };
    
    // Prepare new stack with this loop
    let mut new_stack = loop_stack.to_vec();
    new_stack.push(loop_info);

    match &header.terminator {
        Terminator::Branch { condition, true_target, false_target } => {
            // Determine which branch is the loop body vs exit
            let true_in_loop = loop_info.body.contains(true_target);
            let false_in_loop = loop_info.body.contains(false_target);

            if true_in_loop && !false_in_loop {
                // while (cond) { body }
                let body = recover_structure(cfg, *true_target, loops, loop_headers, visited, &new_stack);
                let mut parts = vec![Structure::Block(loop_info.header, header_stmts)];
                parts.push(make_loop(body, condition.clone()));
                
                // Continue after loop
                if let Some(exit) = loop_info.exit {
                    if !visited.contains(&exit) {
                        let after = recover_structure(cfg, exit, loops, loop_headers, visited, loop_stack);
                        parts.push(after);
                    }
                }
                Structure::Sequence(parts)
            } else if false_in_loop && !true_in_loop {
                // while (!cond) { body }
                let body = recover_structure(cfg, *false_target, loops, loop_headers, visited, &new_stack);
                let mut parts = vec![Structure::Block(loop_info.header, header_stmts)];
                parts.push(make_loop(body, Expression::unary(crate::ir::UnaryOp::Not, condition.clone())));
               
                if let Some(exit) = loop_info.exit {
                    if !visited.contains(&exit) {
                        let after = recover_structure(cfg, exit, loops, loop_headers, visited, loop_stack);
                        parts.push(after);
                    }
                }
                Structure::Sequence(parts)
            } else {
                // Both in loop - complex loop structure, fall back to if
                let then_ = recover_structure(cfg, *true_target, loops, loop_headers, visited, &new_stack);
                let else_ = recover_structure(cfg, *false_target, loops, loop_headers, visited, &new_stack);
                let mut parts = vec![Structure::Block(loop_info.header, header_stmts)];
                parts.push(Structure::If {
                    condition: condition.clone(),
                    then_: Box::new(then_),
                    else_: Box::new(else_),
                });
                Structure::Sequence(parts)
            }
        }
        Terminator::Jump(target) => {
            // Unconditional loop - while(true) or infinite loop
            if loop_info.body.contains(target) {
                let body = recover_structure(cfg, *target, loops, loop_headers, visited, &new_stack);
                let mut parts = vec![Structure::Block(loop_info.header, header_stmts)];
                parts.push(make_loop(body, Expression::constant(crate::ir::Constant::Bool(true))));
                Structure::Sequence(parts)
            } else {
                let next = recover_structure(cfg, *target, loops, loop_headers, visited, loop_stack);
                Structure::Sequence(vec![Structure::Block(loop_info.header, header_stmts), next])
            }
        }
        _ => Structure::Block(loop_info.header, header_stmts),
    }
}

fn split_update_from_body(body: Structure, update_id: BlockId) -> Option<(Structure, Structure)> {
    match body {
        Structure::Sequence(mut parts) => {
            if parts.is_empty() {
                return None;
            }
            // Check the last part
            let last = parts.pop().unwrap();
            
            // If the last part IS the update block
            if let Structure::Block(id, ref _stmts) = last {
                if id == update_id {
                    let new_body = if parts.len() == 1 {
                        parts.into_iter().next().unwrap()
                    } else {
                        Structure::Sequence(parts)
                    };
                    return Some((new_body, last));
                }
            }
            
            // If the last part is a sequence, try to split inside it
            if let Some((new_last, update)) = split_update_from_body(last.clone(), update_id) {
                 parts.push(new_last);
                 let new_body = Structure::Sequence(parts);
                 return Some((new_body, update));
            }
            
            // If not found, put it back
            parts.push(last);
            None
        }
        Structure::Block(id, _) => {
             if id == update_id {
                 return Some((Structure::Block(BlockId(0), vec![]), body));
             }
             None
        }
        _ => None,
    }
}
