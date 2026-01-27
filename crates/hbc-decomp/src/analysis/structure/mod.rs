mod conversion;
mod recovery;

use crate::ir::{Statement, Expression, BlockId};
use crate::analysis::loops::LoopInfo;

// Recovered control flow structure.
#[derive(Debug, Clone)]
pub enum Structure {
    Block(BlockId, Vec<Statement>),
    Sequence(Vec<Structure>),
    If { condition: Expression, then_: Box<Structure>, else_: Box<Structure> },
    While { condition: Expression, body: Box<Structure> },
    DoWhile { body: Box<Structure>, condition: Expression },
    For { init: Box<Structure>, condition: Expression, update: Box<Structure>, body: Box<Structure> },
    Switch {
        discriminant: Expression,
        cases: Vec<(Expression, Structure)>,
        default: Box<Structure>,
    },
    Return(Option<Expression>),
    Break(Option<String>),
    Continue(Option<String>),
    Label(String, Box<Structure>),
}

// Structure analysis result.
pub struct StructureAnalysis {
    pub root: Structure,
    pub loops: Vec<LoopInfo>,
}

impl StructureAnalysis {
    // Analyze control flow and recover high-level structures.
    pub fn analyze(cfg: &crate::ir::CFG) -> Self {
        let (root, loops) = recovery::analyze(cfg);
        StructureAnalysis { root, loops }
    }
}
