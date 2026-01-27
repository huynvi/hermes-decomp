// Transformation passes for the IR.

mod simplify;
mod propagate;
mod codegen;
mod optimize;
mod inline;
mod cleanup;
mod cleanup_advanced;
mod patterns;
mod class_patterns;
mod name_inference;
mod generator;
pub mod metro_transform;
pub mod destructuring;
pub mod spread_rest;
pub mod objects;
pub mod exports;
pub mod arrays;
pub mod default_params;
pub mod simplify_logic;
pub mod chain_access;
pub mod ternary_returns;
pub mod logic_simplify;
pub mod var_naming;
pub mod ssa;

pub use simplify::{simplify_expr, simplify_stmt};
pub use propagate::{propagate, PropagationConfig};
pub use codegen::{Codegen, CodegenOptions};
pub use optimize::optimize_statements;
pub use inline::inline_expressions;
pub use cleanup::cleanup_statements;
pub use cleanup_advanced::cleanup_advanced;
pub use patterns::detect_patterns;
pub use class_patterns::detect_class_patterns;
pub use name_inference::infer_names;
pub use generator::{detect_generator_patterns, has_generator_patterns, cleanup_generator_comments, simplify_state_machine};
pub use destructuring::{detect_destructuring, transform_destructuring, detect_parameter_destructuring, DestructuredParam, DestructuringPattern};
pub use spread_rest::transform_spread_rest;
pub use objects::transform_object_literals;
pub use default_params::transform_default_params;
pub use simplify_logic::transform_logic;
pub use chain_access::optimize_chain_access;
pub use ternary_returns::optimize_ternary_returns;
pub use logic_simplify::simplify_logic_advanced;
pub use var_naming::infer_variable_names;
pub use ssa::transform_to_ssa;

