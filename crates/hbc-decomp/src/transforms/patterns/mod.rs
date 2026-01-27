mod utils;
mod loops;
mod expressions;

use crate::ir::Statement;

pub use loops::{detect_for_of_loops, detect_for_in_loops, detect_for_loops};
pub use expressions::{
    detect_nullish_coalescing, detect_optional_chaining, 
    detect_logical_patterns, detect_string_concat
};

// Apply all pattern transformations.
pub fn detect_patterns(stmts: Vec<Statement>) -> Vec<Statement> {
    let stmts = detect_for_of_loops(stmts);
    let stmts = detect_for_in_loops(stmts);
    let stmts = detect_nullish_coalescing(stmts);
    let stmts = detect_optional_chaining(stmts);
    let stmts = detect_for_loops(stmts);
    let stmts = detect_logical_patterns(stmts);

    detect_string_concat(stmts)
}
