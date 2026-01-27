mod registers;
mod generation;
mod renaming;

pub use registers::{analyze_registers, RegisterInfo, RegisterRole};
pub use generation::generate_name;
pub use renaming::{rename_registers, rename_variables_in_stmts};
