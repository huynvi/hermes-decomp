// Opcode handlers for call and construct operations.

use crate::{BytecodeFile, Instruction};
use crate::ir::{Expression, Value, Statement, AssignTarget};
use super::opcodes_load::{get_reg, reg_expr};

// Handle Call1, Call2, Call3, Call4 opcodes (fixed argument count).
pub fn handle_call_fixed(name: &str, inst: &Instruction) -> Option<Statement> {
    let dst = get_reg(&inst.operands, 0)?;
    let callee = reg_expr(&inst.operands, 1)?;

    let arg_count = match name {
        "Call1" => 1,
        "Call2" => 2,
        "Call3" => 3,
        "Call4" => 4,
        _ => return None,
    };

    // Arguments start at operand index 2
    let mut arguments = Vec::with_capacity(arg_count);
    for i in 0..arg_count {
        if let Some(arg) = reg_expr(&inst.operands, 2 + i) {
            arguments.push(arg);
        }
    }

    Some(Statement::Assign {
        target: AssignTarget::Register(dst),
        value: Expression::Call {
            callee: Box::new(callee),
            arguments,
        },
    })
}

// Handle Call and CallLong opcodes (variable argument count).
pub fn handle_call(inst: &Instruction) -> Option<Statement> {
    let dst = get_reg(&inst.operands, 0)?;
    let callee = reg_expr(&inst.operands, 1)?;
    let arg_count = inst.operands.get(2)?.value.as_u32()? as usize;

    // Arguments are in registers dst-arg_count to dst-1
    let mut arguments = Vec::with_capacity(arg_count);
    if arg_count > 0 && dst >= arg_count as u32 {
        let first_arg = dst - arg_count as u32;
        for i in 0..arg_count {
            arguments.push(Expression::Value(Value::Register(first_arg + i as u32)));
        }
    }

    Some(Statement::Assign {
        target: AssignTarget::Register(dst),
        value: Expression::Call {
            callee: Box::new(callee),
            arguments,
        },
    })
}

// Handle Construct opcode.
pub fn handle_construct(inst: &Instruction) -> Option<Statement> {
    let dst = get_reg(&inst.operands, 0)?;
    let callee = reg_expr(&inst.operands, 1)?;
    let arg_count = inst.operands.get(2)?.value.as_u32()? as usize;

    // Arguments are in registers dst-arg_count to dst-1
    let mut arguments = Vec::with_capacity(arg_count);
    if arg_count > 0 && dst >= arg_count as u32 {
        let first_arg = dst - arg_count as u32;
        for i in 0..arg_count {
            arguments.push(Expression::Value(Value::Register(first_arg + i as u32)));
        }
    }

    Some(Statement::Assign {
        target: AssignTarget::Register(dst),
        value: Expression::New {
            callee: Box::new(callee),
            arguments,
        },
    })
}

// Handle CreateClosure opcode.
pub fn handle_create_closure(
    inst: &Instruction,
    file: &BytecodeFile,
    resolve_strings: bool,
) -> Option<Statement> {
    let dst = get_reg(&inst.operands, 0)?;
    // Environment is operand 1
    let func_idx = inst.operands.get(2)?.value.as_u32()?;

    let func_header = file.function_headers.get(func_idx as usize);
    
    let name = if resolve_strings {
        func_header
            .and_then(|h| file.string_at(h.function_name()))
            .map(|e| e.value.clone())
            .filter(|n| !n.is_empty())
    } else {
        None
    };

    // Detect arrow functions using bytecode flags
    let is_arrow = func_header.map(|h| h.is_likely_arrow()).unwrap_or(false);

    Some(Statement::Assign {
        target: AssignTarget::Register(dst),
        value: Expression::Function {
            id: crate::ir::FunctionId(func_idx),
            name,
            is_arrow,
            is_async: false,
            is_generator: false,
        },
    })
}

// Handle CreateAsyncClosure opcode.
pub fn handle_create_async_closure(
    inst: &Instruction,
    file: &BytecodeFile,
    resolve_strings: bool,
) -> Option<Statement> {
    let dst = get_reg(&inst.operands, 0)?;
    let func_idx = inst.operands.get(2)?.value.as_u32()?;

    let func_header = file.function_headers.get(func_idx as usize);
    
    let name = if resolve_strings {
        func_header
            .and_then(|h| file.string_at(h.function_name()))
            .map(|e| e.value.clone())
            .filter(|n| !n.is_empty())
    } else {
        None
    };

    let is_arrow = func_header.map(|h| h.is_likely_arrow()).unwrap_or(false);

    Some(Statement::Assign {
        target: AssignTarget::Register(dst),
        value: Expression::Function {
            id: crate::ir::FunctionId(func_idx),
            name,
            is_arrow,
            is_async: true,
            is_generator: false,
        },
    })
}

// Handle CreateGeneratorClosure opcode.
pub fn handle_create_generator_closure(
    inst: &Instruction,
    file: &BytecodeFile,
    resolve_strings: bool,
) -> Option<Statement> {
    let dst = get_reg(&inst.operands, 0)?;
    let func_idx = inst.operands.get(2)?.value.as_u32()?;

    let func_header = file.function_headers.get(func_idx as usize);
    
    let name = if resolve_strings {
        func_header
            .and_then(|h| file.string_at(h.function_name()))
            .map(|e| e.value.clone())
            .filter(|n| !n.is_empty())
    } else {
        None
    };

    let is_arrow = func_header.map(|h| h.is_likely_arrow()).unwrap_or(false);

    Some(Statement::Assign {
        target: AssignTarget::Register(dst),
        value: Expression::Function {
            id: crate::ir::FunctionId(func_idx),
            name,
            is_arrow,
            is_async: false,
            is_generator: true,
        },
    })
}


// Handle CallBuiltin opcode.
pub fn handle_call_builtin(inst: &Instruction) -> Option<Statement> {
    let dst = get_reg(&inst.operands, 0)?;
    let builtin_idx = inst.operands.get(1)?.value.as_u32()?;
    let arg_count = inst.operands.get(2)?.value.as_u32()? as usize;

    // Arguments are in registers dst-arg_count to dst-1
    let mut arguments = Vec::with_capacity(arg_count);
    if arg_count > 0 && dst >= arg_count as u32 {
        let first_arg = dst - arg_count as u32;
        for i in 0..arg_count {
            arguments.push(Expression::Value(Value::Register(first_arg + i as u32)));
        }
    }

    // Map builtin indices to meaningful names
    let builtin_name = match builtin_idx {
        0 => "silentSetPrototypeOf",
        1 => "requireFast",
        2 => "getTemplateObject",
        3 => "ensureObject",
        4 => "getMethod",
        5 => "throwTypeError",
        6 => "generatorSetDelegated",
        7 => "copyDataProperties",
        8 => "copyRestArgs",
        9 => "arraySpread",
        10 => "apply",
        11 => "exportAll",
        12 => "exponentiationOperator",
        _ => return Some(Statement::Assign {
            target: AssignTarget::Register(dst),
            value: Expression::Call {
                callee: Box::new(Expression::Value(Value::Variable(format!("__builtin{builtin_idx}")))),
                arguments,
            },
        }),
    };

    Some(Statement::Assign {
        target: AssignTarget::Register(dst),
        value: Expression::Call {
            callee: Box::new(Expression::Value(Value::Variable(format!("__{builtin_name}")))),
            arguments,
        },
    })
}

// Handle GetBuiltinClosure opcode.
pub fn handle_get_builtin_closure(inst: &Instruction) -> Option<Statement> {
    let dst = get_reg(&inst.operands, 0)?;
    let builtin_idx = inst.operands.get(1)?.value.as_u32()?;

    Some(Statement::Assign {
        target: AssignTarget::Register(dst),
        value: Expression::Unknown {
            opcode: format!("builtin{builtin_idx}"),
            operands: vec![],
        },
    })
}

// Handle CallRequire opcode.
pub fn handle_call_require(inst: &Instruction) -> Option<Statement> {
    let dst = get_reg(&inst.operands, 0)?;
    
    // Operand 2 contains the module ID
    let module_arg = if let Some(op) = inst.operands.get(2) {
        match op.value {
            crate::opcode::OperandValue::U8(v) => Some(v as u32),
            crate::opcode::OperandValue::U16(v) => Some(v as u32),
            crate::opcode::OperandValue::U32(v) => Some(v),
            _ => None,
        }
    } else {
        None
    };

    let arg_expr = if let Some(id) = module_arg {
        Expression::Value(Value::Constant(crate::ir::Constant::Integer(id as i32)))
    } else {
        // Fallback if operand is not an immediate integer?
        // Usually CallRequire has immediate module ID.
        // If not, we might need reg_expr(inst.operands, 2)
        if let Some(expr) = reg_expr(&inst.operands, 2) {
            expr
        } else {
             Expression::Value(Value::Constant(crate::ir::Constant::Integer(-1)))
        }
    };

    Some(Statement::Assign {
        target: AssignTarget::Register(dst),
        value: Expression::Call {
            callee: Box::new(Expression::Value(Value::Variable("require".to_string()))),
            arguments: vec![arg_expr],
        },
    })
}
