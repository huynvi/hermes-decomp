use crate::error::{Error, Result};
use crate::file::{BytecodeFile, Instruction};
use crate::opcode::BytecodeFormat;
use super::DecompileOptions;
use super::helpers::*;

pub fn render_instruction(
    file: &BytecodeFile,
    format: &BytecodeFormat,
    insn: &Instruction,
    options: &DecompileOptions,
) -> Result<Option<String>> {
    let def = format
        .definitions
        .get(insn.opcode as usize)
        .ok_or_else(|| Error::Parse(format!("unknown opcode {}", insn.opcode)))?;

    let name = def.name.as_str();

    match name {
        "Mov" | "MovLong" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            let src = expr_from_operand(file, &insn.operands[1], options);
            Ok(Some(format!("{dst} = {src};")))
        }
        "LoadConstUndefined" => load_const(insn, "undefined"),
        "LoadConstNull" => load_const(insn, "null"),
        "LoadConstTrue" => load_const(insn, "true"),
        "LoadConstFalse" => load_const(insn, "false"),
        "LoadConstZero" => load_const(insn, "0"),
        "LoadConstEmpty" => load_const(insn, "<empty>"),
        "LoadConstUInt8" | "LoadConstInt" | "LoadConstDouble" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            let value = expr_from_operand(file, &insn.operands[1], options);
            Ok(Some(format!("{dst} = {value};")))
        }
        "LoadConstString" | "LoadConstStringLongIndex" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            let value = expr_from_operand(file, &insn.operands[1], options);
            Ok(Some(format!("{dst} = {value};")))
        }
        "LoadConstBigInt" | "LoadConstBigIntLongIndex" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            let idx = expr_from_operand(file, &insn.operands[1], options);
            Ok(Some(format!("{dst} = bigint#{idx};")))
        }
        "LoadParam" | "LoadParamLong" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            let index = insn.operands[1].value.as_u32().unwrap_or(0);
            let value = if index == 0 { "this".to_string() } else { format!("arg{}", index - 1) };
            Ok(Some(format!("{dst} = {value};")))
        }
        "GetGlobalObject" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            Ok(Some(format!("{dst} = globalThis;")))
        }
        "DeclareGlobalVar" => {
            let rendered = render_declare_global_var(file, &insn.operands[0], options);
            Ok(Some(rendered))
        }
        "LoadThisNS" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            Ok(Some(format!("{dst} = this;")))
        }
        "CoerceThisNS" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            let src = expr_from_operand(file, &insn.operands[1], options);
            Ok(Some(format!("{dst} = Object({src});")))
        }
        "AddEmptyString" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            let src = expr_from_operand(file, &insn.operands[1], options);
            Ok(Some(format!("{dst} = \"\" + {src};")))
        }
        "ToNumber" | "ToNumeric" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            let src = expr_from_operand(file, &insn.operands[1], options);
            Ok(Some(format!("{dst} = Number({src});")))
        }
        "ToInt32" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            let src = expr_from_operand(file, &insn.operands[1], options);
            Ok(Some(format!("{dst} = ({src} | 0);")))
        }
        "ToUint32" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            let src = expr_from_operand(file, &insn.operands[1], options);
            Ok(Some(format!("{dst} = ({src} >>> 0);")))
        }
        "Add" | "AddN" | "Sub" | "SubN" | "Mul" | "MulN" | "Div" | "DivN" | "Mod" | "BitAnd"
        | "BitOr" | "BitXor" | "Shl" | "Shr" | "UShr" => {
            render_binary_op(name, insn, file, options)
        }
        "Eq" | "StrictEq" | "Neq" | "StrictNeq" | "Less" | "LessEq" | "Greater" | "GreaterEq" => {
            render_binary_op(name, insn, file, options)
        }
        "Negate" | "Not" | "BitNot" | "TypeOf" => render_unary_op(name, insn, file, options),
        "GetById" | "GetByIdLong" | "GetByIdShort" | "TryGetById" | "TryGetByIdLong"
        | "GetOwnBySlotIdx" | "GetOwnBySlotIdxLong" => {
            render_get_by_id(insn, file, options)
        }
        "PutById" | "PutByIdLong" | "PutNewOwnById" | "PutNewOwnByIdLong" | "PutNewOwnByIdShort"
        | "PutOwnById" | "PutOwnByIdLong" | "PutOwnByIdShort" | "PutNewOwnNEById" | "PutNewOwnNEByIdLong"
        | "PutByIdLoose" | "PutByIdStrict" | "PutByIdLooseLong" | "PutByIdStrictLong"
        | "TryPutByIdLoose" | "TryPutByIdStrict" | "TryPutByIdLooseLong" | "TryPutByIdStrictLong"
        | "PutOwnBySlotIdx" | "PutOwnBySlotIdxLong" => {
            render_put_by_id(insn, file, options)
        }
        "GetByVal" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            let obj = expr_from_operand(file, &insn.operands[1], options);
            let key = expr_from_operand(file, &insn.operands[2], options);
            Ok(Some(format!("{dst} = {obj}[{key}];")))
        }
        "PutByVal" => {
            let obj = expr_from_operand(file, &insn.operands[0], options);
            let key = expr_from_operand(file, &insn.operands[1], options);
            let value = expr_from_operand(file, &insn.operands[2], options);
            Ok(Some(format!("{obj}[{key}] = {value};")))
        }
        "DelByVal" => {
            let obj = expr_from_operand(file, &insn.operands[0], options);
            let key = expr_from_operand(file, &insn.operands[1], options);
            Ok(Some(format!("delete {obj}[{key}];")))
        }
        "NewObjectWithBuffer" | "NewObjectWithBufferLong" => {
            render_new_object_with_buffer(name, insn, file, options)
        }
        "NewObject" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            Ok(Some(format!("{dst} = {{}};")))
        }
        "NewObjectWithParent" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            let parent = expr_from_operand(file, &insn.operands[1], options);
            Ok(Some(format!("{dst} = Object.create({parent});")))
        }
        "NewArray" | "NewFastArray" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            let size = expr_from_operand(file, &insn.operands[1], options);
            Ok(Some(format!("{dst} = new Array({size});")))
        }
        "NewArrayWithBuffer" | "NewArrayWithBufferLong" => {
            render_new_array_with_buffer(name, insn, file, options)
        }
        "FastArrayLength" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            let array = expr_from_operand(file, &insn.operands[1], options);
            Ok(Some(format!("{dst} = {array}.length;")))
        }
        "FastArrayLoad" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            let array = expr_from_operand(file, &insn.operands[1], options);
            let index = expr_from_operand(file, &insn.operands[2], options);
            Ok(Some(format!("{dst} = {array}[{index}];")))
        }
        "FastArrayStore" => {
            let array = expr_from_operand(file, &insn.operands[0], options);
            let index = expr_from_operand(file, &insn.operands[1], options);
            let value = expr_from_operand(file, &insn.operands[2], options);
            Ok(Some(format!("{array}[{index}] = {value};")))
        }
        "FastArrayPush" => {
            let array = expr_from_operand(file, &insn.operands[0], options);
            let value = expr_from_operand(file, &insn.operands[1], options);
            Ok(Some(format!("{array}.push({value});")))
        }
        "FastArrayAppend" => {
            let array = expr_from_operand(file, &insn.operands[0], options);
            let value = expr_from_operand(file, &insn.operands[1], options);
            Ok(Some(format!("{array} = {array}.concat({value});")))
        }
        "Call1" | "Call2" | "Call3" | "Call4" => render_call_fixed(insn, file, options),
        "Call" | "CallLong" | "Construct" | "CallWithNewTarget" | "CallWithNewTargetLong" => {
            render_call_var(name, insn, file, options)
        }
        "CallRequire" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            let module_id = insn
                .operands
                .get(2)
                .map(|operand| {
                    operand
                        .value
                        .as_u32()
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| expr_from_operand(file, operand, options))
                })
                .unwrap_or_else(|| "<missing>".to_string());
            Ok(Some(format!("{dst} = require({module_id});")))
        }
        "CreateClosure" | "CreateClosureLongIndex" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            let function_id = insn
                .operands
                .get(2)
                .and_then(|operand| operand.value.as_u32())
                .unwrap_or(0);
            
            // We use a simplified label here without recursive resolution
            let label = format!("F{function_id}");
            Ok(Some(format!("{dst} = closure {label};")))
        }
        "SelectObject" => {
            let dst = reg_from_operand(&insn.operands[0])?;
            let this_obj = expr_from_operand(file, &insn.operands[1], options);
            let value = expr_from_operand(file, &insn.operands[2], options);
            Ok(Some(format!(
                "{dst} = ({value} instanceof Object) ? {value} : {this_obj};"
            )))
        }
        "Ret" => {
            if insn.operands.is_empty() {
                Ok(Some("return;".to_string()))
            } else {
                let value = expr_from_operand(file, &insn.operands[0], options);
                Ok(Some(format!("return {value};")))
            }
        }
        "Throw" => {
            let value = expr_from_operand(file, &insn.operands[0], options);
            Ok(Some(format!("throw {value};")))
        }
        _ if def.is_jump => render_jump(name, insn, file, options),
        _ => {
            let rendered = render_fallback(name, insn, file, options);
            Ok(Some(rendered))
        }
    }
}
