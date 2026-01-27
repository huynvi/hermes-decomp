// Instruction dispatch to appropriate handlers.

use crate::{BytecodeFile, BytecodeFormat, Instruction};
use crate::ir::Statement;
use super::opcodes_flow::{
    FlowResult, handle_jmp, handle_jmp_cond, handle_jmp_comparison, handle_jmp_undefined,
    handle_ret, handle_throw, handle_create_environment, handle_get_environment,
    handle_load_from_environment, handle_store_to_environment, handle_store_np_to_environment,
    handle_select_object, handle_debugger, handle_catch, handle_get_next_pname, handle_switch_imm,
    handle_start_generator, handle_resume_generator, handle_create_generator, handle_complete_generator,
    handle_save_generator,
};
use super::opcodes_load::*;
use super::opcodes_arith::*;
use super::opcodes_prop::*;
use super::opcodes_call::*;
use super::opcodes_obj::*;

// Dispatch an instruction to the appropriate handler.
pub fn dispatch_instruction(
    inst: &Instruction,
    file: &BytecodeFile,
    format: &BytecodeFormat,
    resolve_strings: bool,
) -> FlowResult {
    let def = match format.definitions.get(inst.opcode as usize) {
        Some(d) => d,
        None => return unknown_opcode(inst),
    };

    let name = def.name.as_str();

    // Try each handler category
    if let Some(result) = try_load_handlers(name, inst, file, resolve_strings) {
        return result;
    }
    if let Some(result) = try_arith_handlers(name, inst) {
        return result;
    }
    if let Some(result) = try_prop_handlers(name, inst, file, resolve_strings) {
        return result;
    }
    if let Some(result) = try_call_handlers(name, inst, file, resolve_strings) {
        return result;
    }
    if let Some(result) = try_obj_handlers(name, inst, file, resolve_strings) {
        return result;
    }
    if let Some(result) = try_flow_handlers(name, inst, format, file) {
        return result;
    }

    // Unknown opcode
    FlowResult::Statement(Statement::Comment(format!("{} (0x{:02x})", name, inst.opcode)))
}

fn try_load_handlers(
    name: &str,
    inst: &Instruction,
    file: &BytecodeFile,
    resolve_strings: bool,
) -> Option<FlowResult> {
    match name {
        "LoadConstUndefined" | "LoadConstNull" | "LoadConstTrue" | "LoadConstFalse"
        | "LoadConstZero" | "LoadConstEmpty" | "LoadConstUInt8" | "LoadConstInt"
        | "LoadConstDouble" | "LoadConstString" | "LoadConstStringLongIndex"
        | "LoadConstBigInt" | "LoadConstBigIntLongIndex" => {
            handle_load_const(name, inst, file, resolve_strings).map(FlowResult::Statement)
        }
        "Mov" | "MovLong" => handle_mov(inst).map(FlowResult::Statement),
        "LoadParam" | "LoadParamLong" => handle_load_param(inst).map(FlowResult::Statement),
        "GetGlobalObject" => handle_get_global(inst).map(FlowResult::Statement),
        "LoadThisNS" => handle_load_this(inst).map(FlowResult::Statement),
        "DeclareGlobalVar" => handle_declare_global(inst, file, resolve_strings).map(FlowResult::Statement),
        _ => None,
    }
}

fn try_arith_handlers(name: &str, inst: &Instruction) -> Option<FlowResult> {
    match name {
        "Add" | "AddN" | "Sub" | "SubN" | "Mul" | "MulN" | "Div" | "DivN" | "Mod"
        | "BitAnd" | "BitOr" | "BitXor" | "LShift" | "Shl" | "RShift" | "Shr"
        | "URshift" | "UShr" => {
            handle_binary_op(name, inst).map(FlowResult::Statement)
        }
        "Eq" | "StrictEq" | "Neq" | "StrictNeq" | "Less" | "LessEq" | "Greater"
        | "GreaterEq" => {
            handle_comparison(name, inst).map(FlowResult::Statement)
        }
        "Negate" | "Not" | "BitNot" | "TypeOf" => {
            handle_unary_op(name, inst).map(FlowResult::Statement)
        }
        "Inc" | "Dec" => handle_inc_dec(name, inst).map(FlowResult::Statement),
        "ToNumber" | "ToNumeric" | "ToInt32" | "ToUint32" | "AddEmptyString"
        | "CoerceThisNS" => {
            handle_coercion(name, inst).map(FlowResult::Statement)
        }
        "InstanceOf" | "IsIn" => handle_instance_in(name, inst).map(FlowResult::Statement),
        _ => None,
    }
}

fn try_prop_handlers(
    name: &str,
    inst: &Instruction,
    file: &BytecodeFile,
    resolve_strings: bool,
) -> Option<FlowResult> {
    match name {
        "GetById" | "GetByIdLong" | "GetByIdShort" => {
            handle_get_by_id(inst, file, resolve_strings).map(FlowResult::Statement)
        }
        "TryGetById" | "TryGetByIdLong" => {
            handle_try_get_by_id(inst, file, resolve_strings).map(FlowResult::Statement)
        }
        "PutById" | "PutByIdLong" | "PutNewOwnById" | "PutNewOwnByIdLong"
        | "PutNewOwnByIdShort" | "TryPutById" | "TryPutByIdLong" => {
            handle_put_by_id(inst, file, resolve_strings).map(FlowResult::Statement)
        }
        "GetByVal" => handle_get_by_val(inst).map(FlowResult::Statement),
        "PutByVal" => handle_put_by_val(inst).map(FlowResult::Statement),
        "DelByVal" => handle_del_by_val(inst).map(FlowResult::Statement),
        "DelById" => handle_del_by_id(inst, file, resolve_strings).map(FlowResult::Statement),
        _ => None,
    }
}

fn try_call_handlers(
    name: &str,
    inst: &Instruction,
    file: &BytecodeFile,
    resolve_strings: bool,
) -> Option<FlowResult> {
    match name {
        "Call1" | "Call2" | "Call3" | "Call4" => {
            handle_call_fixed(name, inst).map(FlowResult::Statement)
        }
        "Call" | "CallLong" => handle_call(inst).map(FlowResult::Statement),
        "Construct" | "ConstructLong" => handle_construct(inst).map(FlowResult::Statement),
        "CreateClosure" | "CreateClosureLongIndex" => {
            handle_create_closure(inst, file, resolve_strings).map(FlowResult::Statement)
        }
        "CreateAsyncClosure" => {
            handle_create_async_closure(inst, file, resolve_strings).map(FlowResult::Statement)
        }
        "CreateGeneratorClosure" => {
            handle_create_generator_closure(inst, file, resolve_strings).map(FlowResult::Statement)
        }
        "CallBuiltin" | "CallBuiltinLong" => handle_call_builtin(inst).map(FlowResult::Statement),
        "GetBuiltinClosure" => handle_get_builtin_closure(inst).map(FlowResult::Statement),
        "CallRequire" => handle_call_require(inst).map(FlowResult::Statement),
        _ => None,
    }
}

fn try_obj_handlers(
    name: &str,
    inst: &Instruction,
    file: &BytecodeFile,
    resolve_strings: bool,
) -> Option<FlowResult> {
    match name {
        "NewObject" => handle_new_object(inst).map(FlowResult::Statement),
        "NewObjectWithParent" => handle_new_object_with_parent(inst).map(FlowResult::Statement),
        "NewObjectWithBuffer" | "NewObjectWithBufferLong" => {
            handle_new_object_with_buffer(inst, file, resolve_strings).map(FlowResult::Statement)
        }
        "NewArray" | "NewFastArray" => handle_new_array(inst).map(FlowResult::Statement),
        "NewArrayWithBuffer" | "NewArrayWithBufferLong" => {
            handle_new_array_with_buffer(inst, file).map(FlowResult::Statement)
        }
        "PutOwnByIndex" | "PutOwnByIndexL" => {
            handle_put_own_by_index(inst).map(FlowResult::Statement)
        }
        "PutOwnByVal" => handle_put_own_by_val(inst).map(FlowResult::Statement),
        "FastArrayLoad" => handle_fast_array_load(inst).map(FlowResult::Statement),
        "FastArrayStore" | "FastArrayStoreLoose" => {
            handle_fast_array_store(inst).map(FlowResult::Statement)
        }
        "FastArrayPush" => handle_fast_array_push(inst).map(FlowResult::Statement),
        "FastArrayLength" => handle_fast_array_length(inst).map(FlowResult::Statement),
        "CreateRegExp" => {
            handle_create_regexp(inst, file, resolve_strings).map(FlowResult::Statement)
        }
        "GetArgumentsLength" => handle_get_arguments_length(inst).map(FlowResult::Statement),
        "GetArgumentsPropByVal" => {
            handle_get_arguments_prop_by_val(inst).map(FlowResult::Statement)
        }
        "ReifyArguments" => handle_reify_arguments(inst).map(FlowResult::Statement),
        "CreateThis" => handle_create_this(inst).map(FlowResult::Statement),
        "GetNewTarget" => handle_get_new_target(inst).map(FlowResult::Statement),
        "IteratorBegin" => handle_iterator_begin(inst).map(FlowResult::Statement),
        "IteratorNext" => handle_iterator_next(inst).map(FlowResult::Statement),
        "IteratorClose" => handle_iterator_close(inst).map(FlowResult::Statement),
        "GetPNameList" => handle_get_pname_list(inst).map(FlowResult::Statement),
        "PutOwnGetterSetterByVal" => {
            handle_put_own_getter_setter_by_val(inst).map(FlowResult::Statement)
        }
        _ => None,
    }
}

fn try_flow_handlers(name: &str, inst: &Instruction, format: &BytecodeFormat, file: &BytecodeFile) -> Option<FlowResult> {
    match name {
        "Jmp" | "JmpLong" => handle_jmp(inst, format),
        "JmpTrue" | "JmpTrueLong" | "JmpFalse" | "JmpFalseLong" => {
            handle_jmp_cond(name, inst, format)
        }
        "JEqual" | "JNotEqual" | "JStrictEqual" | "JStrictNotEqual"
        | "JEqualLong" | "JNotEqualLong" | "JStrictEqualLong" | "JStrictNotEqualLong"
        | "JLess" | "JLessEqual" | "JGreater" | "JGreaterEqual"
        | "JLessLong" | "JLessEqualLong" | "JGreaterLong" | "JGreaterEqualLong"
        | "JLessN" | "JLessEqualN" | "JGreaterN" | "JGreaterEqualN"
        | "JLessNLong" | "JLessEqualNLong" | "JGreaterNLong" | "JGreaterEqualNLong"
        | "JNotLess" | "JNotLessEqual" | "JNotGreater" | "JNotGreaterEqual"
        | "JNotLessLong" | "JNotLessEqualLong" | "JNotGreaterLong" | "JNotGreaterEqualLong"
        | "JNotLessN" | "JNotLessEqualN" | "JNotGreaterN" | "JNotGreaterEqualN"
        | "JNotLessNLong" | "JNotLessEqualNLong" | "JNotGreaterNLong" | "JNotGreaterEqualNLong" => {
            handle_jmp_comparison(name, inst, format)
        }
        "JmpUndefined" | "JmpUndefinedLong" => {
            handle_jmp_undefined(name, inst, format)
        }
        "Ret" => handle_ret(inst),
        "Throw" => handle_throw(inst),
        "CreateEnvironment" => handle_create_environment(inst),
        "GetEnvironment" => handle_get_environment(inst),
        "LoadFromEnvironment" | "LoadFromEnvironmentL" => handle_load_from_environment(inst),
        "StoreToEnvironment" | "StoreToEnvironmentL" => handle_store_to_environment(inst),
        "StoreNPToEnvironment" | "StoreNPToEnvironmentL" => handle_store_np_to_environment(inst),
        "SelectObject" => handle_select_object(inst),
        "Debugger" => handle_debugger(),
        "Catch" => handle_catch(inst),
        // Generator opcodes
        "StartGenerator" => handle_start_generator(),
        "ResumeGenerator" => handle_resume_generator(inst),
        "CreateGenerator" => handle_create_generator(inst),
        "CompleteGenerator" => handle_complete_generator(inst),
        "SaveGenerator" | "SaveGeneratorLong" => handle_save_generator(inst, format),
        "GetNextPName" => handle_get_next_pname(inst),
        "SwitchImm" => handle_switch_imm(inst, format, file),
        _ => None,
    }
}

fn unknown_opcode(inst: &Instruction) -> FlowResult {
    FlowResult::Statement(Statement::Comment(format!("unknown opcode 0x{:02x}", inst.opcode)))
}
