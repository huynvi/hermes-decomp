// Liveness analysis for registers.

use std::collections::{HashMap, HashSet};
use crate::ir::{CFG, BlockId, Statement, Expression, Value};

// Result of liveness analysis.
/// 
/// Liveness analysis determines which registers are "live" (hold a useful value) at each point in the program.
/// - `live_in`: Set of registers live at the entry of a block.
/// - `live_out`: Set of registers live at the exit of a block.
/// 
/// Use case: Dead Code Elimination (DCE). If a register is assigned but not live-out, the assignment is dead (unless it has side effects).
#[derive(Debug)]
pub struct LivenessInfo {
    pub live_in: HashMap<BlockId, HashSet<u32>>,
    pub live_out: HashMap<BlockId, HashSet<u32>>,
}

impl LivenessInfo {
    // Compute liveness information for a CFG.
    pub fn analyze(cfg: &CFG) -> Self {
        let mut live_in: HashMap<BlockId, HashSet<u32>> = HashMap::new();
        let mut live_out: HashMap<BlockId, HashSet<u32>> = HashMap::new();

        // Initialize empty sets
        for id in cfg.block_ids() {
            live_in.insert(id, HashSet::new());
            live_out.insert(id, HashSet::new());
        }

        // Fixed-point iteration (reverse postorder for efficiency).
        // Standard dataflow algorithm:
        // OUT[B] = U(IN[S]) for S in successors(B)
        // IN[B] = USE[B] U (OUT[B] - DEF[B])
        let rpo = cfg.reverse_postorder();
        let mut changed = true;

        while changed {
            changed = false;

            for &block_id in rpo.iter().rev() {
                let block = match cfg.get(block_id) {
                    Some(b) => b,
                    None => continue,
                };

                // live_out = union of live_in of successors
                let mut new_out: HashSet<u32> = HashSet::new();
                for succ in block.successors() {
                    if let Some(succ_in) = live_in.get(&succ) {
                        new_out.extend(succ_in);
                    }
                }

                // live_in = use(block) ∪ (live_out - def(block))
                let (uses, defs) = collect_uses_defs(block);
                let mut new_in = new_out.clone();
                for d in &defs {
                    new_in.remove(d);
                }
                new_in.extend(&uses);

                if new_in != *live_in.get(&block_id).unwrap_or(&HashSet::new()) {
                    changed = true;
                    live_in.insert(block_id, new_in);
                }
                if new_out != *live_out.get(&block_id).unwrap_or(&HashSet::new()) {
                    changed = true;
                    live_out.insert(block_id, new_out);
                }
            }
        }

        LivenessInfo { live_in, live_out }
    }

    // Check if a register is live at block exit.
    pub fn is_live_out(&self, block: BlockId, reg: u32) -> bool {
        self.live_out.get(&block).map(|s| s.contains(&reg)).unwrap_or(false)
    }

    // Check if a register is live at block entry.
    pub fn is_live_in(&self, block: BlockId, reg: u32) -> bool {
        self.live_in.get(&block).map(|s| s.contains(&reg)).unwrap_or(false)
    }
}

fn collect_uses_defs(block: &crate::ir::BasicBlock) -> (HashSet<u32>, HashSet<u32>) {
    let mut uses = HashSet::new();
    let mut defs = HashSet::new();

    for stmt in &block.statements {
        collect_stmt_uses(stmt, &mut uses, &defs);
        collect_stmt_defs(stmt, &mut defs);
    }

    // Terminator uses
    collect_terminator_uses(&block.terminator, &mut uses, &defs);

    (uses, defs)
}

fn collect_stmt_uses(stmt: &Statement, uses: &mut HashSet<u32>, defs: &HashSet<u32>) {
    match stmt {
        Statement::Expr(e) | Statement::Throw(e) => collect_expr_uses(e, uses, defs),
        Statement::Let { value, .. } => collect_expr_uses(value, uses, defs),
        Statement::Assign { value, .. } => collect_expr_uses(value, uses, defs),
        Statement::Return(Some(e)) => collect_expr_uses(e, uses, defs),
        _ => {}
    }
}

fn collect_stmt_defs(stmt: &Statement, defs: &mut HashSet<u32>) {
    if let Statement::Assign { target: crate::ir::AssignTarget::Register(r), .. } = stmt {
        defs.insert(*r);
    }
}

fn collect_terminator_uses(
    term: &crate::ir::Terminator,
    uses: &mut HashSet<u32>,
    defs: &HashSet<u32>,
) {
    match term {
        crate::ir::Terminator::Return(Some(e)) => collect_expr_uses(e, uses, defs),
        crate::ir::Terminator::Throw(e) => collect_expr_uses(e, uses, defs),
        crate::ir::Terminator::Branch { condition, .. } => collect_expr_uses(condition, uses, defs),
        crate::ir::Terminator::Switch { value, .. } => collect_expr_uses(value, uses, defs),
        _ => {}
    }
}

fn collect_expr_uses(expr: &Expression, uses: &mut HashSet<u32>, defs: &HashSet<u32>) {
    match expr {
        Expression::Value(Value::Register(r)) if !defs.contains(r) => {
            uses.insert(*r);
        }
        Expression::Binary { left, right, .. } => {
            collect_expr_uses(left, uses, defs);
            collect_expr_uses(right, uses, defs);
        }
        Expression::Unary { operand, .. } => collect_expr_uses(operand, uses, defs),
        Expression::Call { callee, arguments } | Expression::New { callee, arguments } => {
            collect_expr_uses(callee, uses, defs);
            for arg in arguments {
                collect_expr_uses(arg, uses, defs);
            }
        }
        Expression::Member { object, .. } => collect_expr_uses(object, uses, defs),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{CFGBuilder, Constant};

    #[test]
    fn test_basic_liveness() {
        let mut builder = CFGBuilder::new();
        builder.emit(Statement::assign_reg(0, Expression::constant(Constant::Integer(1))));
        builder.emit_return(Some(Expression::Value(Value::Register(0))));

        let cfg = builder.finish();
        let liveness = LivenessInfo::analyze(&cfg);

        // r0 should not be live_in at entry (it's defined first)
        assert!(!liveness.is_live_in(cfg.entry, 0));
    }
}
