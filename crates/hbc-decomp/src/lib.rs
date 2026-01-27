// Hermes Bytecode Decompiler Library
//
// This library provides tools for parsing, disassembling, and decompiling
// Hermes bytecode files (`.hbc`) used by React Native applications.
//
// # Architecture
//
// The decompilation pipeline consists of several phases:
//
// 1. **Parsing** (`file`, `format`, `opcode`): Parse the binary bytecode format
// 2. **IR Generation** (`ir`): Convert bytecode to an intermediate representation
// 3. **Analysis** (`analysis`): Analyze the IR (liveness, reaching defs, structure)
// 4. **Transformation** (`transforms`): Optimize and simplify the IR
// 5. **Code Generation** (`transforms::codegen`): Generate JavaScript-like output
//
// # Example
//
// ```no_run
// use hbc::{Decompiler, DecompileOptionsV2};
//
// let bytes = std::fs::read("app.hbc").unwrap();
// let decompiler = Decompiler::new(&bytes);
//
// let options = DecompileOptionsV2::default();
// let output = decompiler.decompile_function(0, &options).unwrap();
// println!("{}", output);
// ```

// Core modules
pub mod decompile;
pub mod debug;
pub mod disasm;
pub mod error;
pub mod file;
pub mod format;
pub mod io;
pub mod opcode;
pub mod pipeline;
pub mod util;

// New architecture modules
pub mod ir;
pub mod analysis;
pub mod transforms;

// Re-export legacy API
pub use decompile::{decompile_all, decompile_function, DecompileOptions};
pub use disasm::{disassemble_all, disassemble_function, DisasmOptions, collect_label_offsets};
pub use error::{Error, Result};
pub use file::{BytecodeFile, Instruction};
pub use format::{BytecodeHeader, FunctionHeader, FunctionHeaderLayout, HeaderLayout};
pub use opcode::{BytecodeFormat, Operand, OperandType, OperandValue};
pub use util::{escape_js_string, is_valid_identifier};

// Re-export new IR types
pub use ir::{
    BasicBlock, BlockId, CFG, Constant, Expression, Statement, Terminator,
    BinaryOp, UnaryOp, Value, FunctionId, AssignTarget,
    IRBuilder, IRBuilderOptions,
};

// Re-export analysis types
pub use analysis::{
    LivenessInfo, ReachingDefs, StructureAnalysis, Structure, LoopInfo,
    analyze_registers, generate_name, rename_registers, RegisterInfo, RegisterRole,
    ClosureInfo, ClosureSlotValue, ClosureContext, resolve_closures,
    MetroRegistry, MetroModule, DependencyTree,
};

// Re-export transform types
pub use transforms::{
    simplify_expr, simplify_stmt,
    propagate, PropagationConfig,
    Codegen, CodegenOptions,
    optimize_statements, inline_expressions,
    cleanup_statements, detect_patterns,
    detect_class_patterns, detect_destructuring,
};

// Re-export debug info types
pub use debug::{DebugInfo, ScopeDescriptor, SourceLocation};

// Re-export pipeline types
pub use pipeline::{
    Decompiler, DecompileOptionsV2, 
    decompile_function_v2, decompile_function_v2_with_context,
    decompile_all_v2, decompile_all_v2_with_closures,
    generate_ir, build_closure_context_from_file as build_closure_context,
};
