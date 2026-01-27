mod format;
mod context;
mod ast;

// Re-export specific items to match the original API
pub use format::{format_f64, format_operand_value, render_literal_value, render_object_key};
pub use context::{
    reg_from_operand, expr_from_operand, prop_from_operand, 
    render_declare_global_var, operand_to_u32
};
pub use ast::{
    load_const, render_binary_op, render_unary_op, 
    render_get_by_id, render_put_by_id, 
    render_new_array_with_buffer, render_new_object_with_buffer,
    render_call_fixed, render_call_var,
    render_jump, normalize_jump_name, jump_target, render_fallback
};
