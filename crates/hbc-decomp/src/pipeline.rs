use crate::analysis::{
    analyze_registers, generate_name, rename_registers, resolve_closures, ClosureContext,
    StructureAnalysis,
};
use crate::error::Result;
use crate::file::BytecodeFile;
use crate::ir::{
    BinaryOp, Expression, IRBuilder, IRBuilderOptions, Statement, Terminator,
};
use crate::opcode::BytecodeFormat;
use crate::transforms::{
    self, cleanup_statements, detect_class_patterns, detect_patterns, inline_expressions,
    optimize_statements, propagate, simplify_stmt, Codegen, CodegenOptions, PropagationConfig,
};
use crate::util::is_valid_identifier;

// High-level wrapper for cleaner workflows
pub struct Decompiler {
    pub file: BytecodeFile,
    pub format: BytecodeFormat,
    pub closure_ctx: Option<ClosureContext>,
}

impl Decompiler {
    // Initialize from bytes (auto-detect version)
    pub fn new(bytes: &[u8]) -> Result<Self> {
        let file = BytecodeFile::parse_auto(bytes)?;
        let (format, _) = BytecodeFormat::for_version_or_latest(file.header.version)?;
        Ok(Self {
            file,
            format,
            closure_ctx: None,
        })
    }

    // Initialize with existing file/format
    pub fn from_parts(file: BytecodeFile, format: BytecodeFormat) -> Self {
        Self {
            file,
            format,
            closure_ctx: None,
        }
    }

    // Build closure context for whole-program analysis
    pub fn build_closure_context(&mut self) -> Result<()> {
        let ctx = build_closure_context_from_file(&self.file, &self.format)?;
        self.closure_ctx = Some(ctx);
        Ok(())
    }

    // Decompile a specific function ID
    pub fn decompile_function(&self, function_id: u32, options: &DecompileOptionsV2) -> Result<String> {
        decompile_function_v2_with_context(
            &self.file,
            &self.format,
            function_id,
            options,
            self.closure_ctx.as_ref()
        )
    }

    // Decompile everything
    pub fn decompile_all(&self, options: &DecompileOptionsV2) -> Result<String> {
        decompile_all_v2_with_closures(
            &self.file,
            &self.format,
            options,
            self.closure_ctx.is_some() // Use existing context if available
        )
    }

    // Get the Intermediate Representation (IR) as structured data (JSON-ready)
    pub fn decompile_to_ir(&self, function_id: u32, options: &DecompileOptionsV2) -> Result<Vec<Statement>> {
        generate_ir(
            &self.file, 
            &self.format, 
            function_id, 
            options, 
            self.closure_ctx.as_ref(),
            true // Default behavior: resolve closures
        )
    }
}

// Options for the new decompilation pipeline (v2).
#[derive(Debug, Clone, Default)]
pub struct DecompileOptionsV2 {
    // Resolve string table indices to actual strings
    pub resolve_strings: bool,
    // Include bytecode offsets as comments
    pub include_offsets: bool,
    // Apply constant/copy propagation
    pub propagate: bool,
    // Apply expression simplification
    pub simplify: bool,
    // Recover control flow structures (if/while/for)
    pub recover_structures: bool,
}

impl DecompileOptionsV2 {
    // Create options with all optimizations enabled.
    pub fn optimized() -> Self {
        Self {
            resolve_strings: true,
            include_offsets: false,
            propagate: true,
            simplify: true,
            recover_structures: true,
        }
    }

    // Create options for debugging (includes offsets).
    pub fn debug() -> Self {
        Self {
            resolve_strings: true,
            include_offsets: true,
            propagate: false,
            simplify: false,
            recover_structures: true,
        }
    }
}

// Decompile a single function using the new pipeline.
pub fn decompile_function_v2(
    file: &BytecodeFile,
    format: &BytecodeFormat,
    function_id: u32,
    options: &DecompileOptionsV2,
) -> Result<String> {
    decompile_function_v2_with_context(file, format, function_id, options, None)
}

// Decompile a single function with optional closure context for cross-function resolution.
pub fn decompile_function_v2_with_context(
    file: &BytecodeFile,
    format: &BytecodeFormat,
    function_id: u32,
    options: &DecompileOptionsV2,
    closure_ctx: Option<&ClosureContext>,
) -> Result<String> {
    let statements = generate_ir(file, format, function_id, options, closure_ctx, true)?;

    // Generate code
    let function_name = get_function_name(file, function_id);
    let params = get_function_params(file, function_id);

    let codegen_options = CodegenOptions::default();
    let mut codegen = Codegen::new(codegen_options);

    let mut output = String::new();
    output.push_str(&format!("function {}({}) {{\n", function_name, params.join(", ")));

    let body = codegen.generate_statements(&statements);
    for line in body.lines() {
        output.push_str("  ");
        output.push_str(line);
        output.push('\n');
    }

    output.push_str("}\n");
    Ok(output)
}

// Generate IR for a function (Analysis + Transform phases).
pub fn generate_ir(
    file: &BytecodeFile,
    format: &BytecodeFormat,
    function_id: u32,
    options: &DecompileOptionsV2,
    closure_ctx: Option<&ClosureContext>,
    perform_resolve: bool,
) -> Result<Vec<Statement>> {
    // Build IR
    let builder_options = IRBuilderOptions {
        resolve_strings: options.resolve_strings,
        include_offsets: options.include_offsets,
    };
    let mut builder = IRBuilder::new(file, format, builder_options);
    let mut cfg = builder.build_function(function_id)?;

    // Apply SSA / Live Range Splitting
    // This disambiguates reused registers (e.g. r0 used for different variables)
    // so they can be named independently.
    transforms::transform_to_ssa(&mut cfg);

    // Apply optimizations
    if options.propagate {
        propagate(&mut cfg, &PropagationConfig::default());
    }

    // Simplify expressions in statements
    if options.simplify {
        for block in cfg.blocks_mut() {
            block.statements = block.statements.iter().map(simplify_stmt).collect();
        }
    }

    // Recover structure or generate low-level output
    let statements = if options.recover_structures {
        let analysis = StructureAnalysis::analyze(&cfg);
        analysis.root.to_statements(&cfg)
    } else {
        // Flatten blocks without structure recovery
        let mut stmts = Vec::new();
        for (id, block) in cfg.blocks_with_ids() {
            stmts.push(Statement::Comment(format!("{id}:")));
            stmts.extend(block.statements.clone());
            match &block.terminator {
                Terminator::Return(v) => stmts.push(Statement::Return(v.clone())),
                Terminator::Throw(e) => stmts.push(Statement::Throw(e.clone())),
                Terminator::Jump(t) => stmts.push(Statement::Goto(*t)),
                Terminator::Branch { condition, true_target, false_target } => {
                    stmts.push(Statement::CondGoto {
                        condition: condition.clone(),
                        target: *true_target,
                        fallthrough: *false_target,
                    });
                }
                Terminator::Switch { value, cases, default } => {
                    stmts.push(Statement::Comment("Switch dispatch".to_string()));
                    for (case_val, target) in cases {
                        let condition = Expression::binary(
                            BinaryOp::StrictEq,
                            value.clone(),
                            case_val.clone(),
                        );
                        stmts.push(Statement::CondGoto {
                            condition,
                            target: *target,
                            fallthrough: *default,
                        });
                    }
                    stmts.push(Statement::Goto(*default));
                }
                _ => {}
            }
        }
        stmts
    };

    // Check if this function contains generator patterns
    let has_generator = transforms::has_generator_patterns(&statements);
    // Determine if this is an async function from the closure context
    let is_async_function = closure_ctx
        .map(|ctx| ctx.is_async(function_id))
        .unwrap_or(false);

    // Apply high-level optimizations
    let mut statements = if options.simplify {
        let statements = optimize_statements(statements);
        let statements = inline_expressions(statements);
        
        let mut statements = statements;
        transforms::transform_logic(&mut statements);
        let statements = statements;
        
        let statements = detect_patterns(statements);
        
        // Pass context for recursive method expansion
        let statements = detect_class_patterns(statements, file, format, options, closure_ctx);

        // Map back to mutable for in-place transforms
        let mut statements = statements;
        
        // Apply object literal reconstruction
        transforms::transform_object_literals(&mut statements);
        transforms::arrays::transform_array_literals(&mut statements);
        // Transform default parameters
        transforms::transform_default_params(&mut statements);

        // Apply spread/rest pattern detection
        transforms::transform_spread_rest(&mut statements);
        let statements = statements;

        // Apply destructuring pattern detection (handles nested statements)
        let statements = transforms::detect_destructuring(statements);

        // Apply generator/async pattern detection
        let statements = if has_generator {
            let statements = transforms::detect_generator_patterns(statements, is_async_function);
            let statements = transforms::simplify_state_machine(statements);
            transforms::cleanup_generator_comments(statements)
        } else {
            statements
        };

        let statements = cleanup_statements(statements);

        // Advanced cleanup: remove redundant assignments, inline single-use temps
        let statements = transforms::cleanup_advanced(statements);

        // Optimize member access chains (r0 = obj.a; r1 = r0.b; → obj.a.b)
        let statements = transforms::optimize_chain_access(statements);

        // Optimize if-return patterns to ternary (if(c) return a; else return b; → return c ? a : b)
        let statements = transforms::optimize_ternary_returns(statements);

        // Advanced logic simplification (De Morgan, double negation, identities)
        let statements = transforms::simplify_logic_advanced(statements);

        // Map back to mutable for in-place transforms
        let mut statements = statements;
        
        // CommonJS Exports inference
        // Get param count for inference
        let param_count = file.function_headers
            .get(function_id as usize)
            .map(|h| h.param_count())
            .unwrap_or(0);

        if let Some(names) = transforms::exports::infer_commonjs_names(&mut statements, param_count) {
             transforms::exports::rename_param_registers(&mut statements, &names);
        }

        transforms::infer_names(&mut statements);
        let statements = statements; // Immutable again for analysis

        // Analyze registers and generate better names
        // Analyze registers and generate better names
        let reg_info = analyze_registers(&statements);
        
        // Extract variable names from debug info if available
        let debug_names = if let Some(debug_info) = &file.debug_info {
            let scope_offset = debug_info
                .source_locations
                .get(&function_id)
                .and_then(|locs| locs.iter().find_map(|l| l.scope_offset));
            debug_info.build_variable_map(scope_offset)
        } else {
            std::collections::HashMap::new()
        };

        let mut used_names = std::collections::HashSet::new();
        // Reserve names used in debug info
        for name in debug_names.values() {
            used_names.insert(name.clone());
        }

        let names: std::collections::HashMap<u32, String> = reg_info
            .iter()
            .map(|(&r, info)| {
                if let Some(name) = debug_names.get(&r) {
                    (r, name.clone())
                } else {
                    (r, generate_name(r, info, &mut used_names))
                }
            })
            .collect();
        let statements = rename_registers(statements, &names);

        // Apply semantic variable naming (fetch() → response, new Date() → date, etc.)
        let statements = transforms::infer_variable_names(statements);

        statements.iter().map(simplify_stmt).collect()
    } else {
        statements
    };

    // Apply cross-function closure resolution if context is provided and requested
    if perform_resolve {
        if let Some(ctx) = closure_ctx {
            let closure_info = ctx.get_closure_info_for(function_id);
            if !closure_info.slots.is_empty() {
                statements = resolve_closures(statements, &closure_info);
            }
        }
    }
    
    Ok(statements)
}

// Build a closure context by analyzing all functions.
// This enables cross-function closure resolution.
pub fn build_closure_context_from_file(
    file: &BytecodeFile,
    format: &BytecodeFormat,
) -> Result<ClosureContext> {
    let mut ctx = ClosureContext::new();

    let builder_options = IRBuilderOptions {
        resolve_strings: true,
        include_offsets: false,
    };

    // First pass: analyze all functions to build the context
    for (i, header) in file.function_headers.iter().enumerate() {
        let function_id = i as u32;

        // Record function name if available
        if let Some(entry) = file.string_at(header.function_name()) {
            if !entry.value.is_empty() && is_valid_identifier(&entry.value) {
                ctx.add_function_name(function_id, entry.value.clone());
            }
        }

        // Build IR and analyze for closure info
        let mut builder = IRBuilder::new(file, format, builder_options.clone());
        if let Ok(mut cfg) = builder.build_function(function_id) {
            propagate(&mut cfg, &PropagationConfig::default());

            // Recover structure for better analysis
            let analysis = StructureAnalysis::analyze(&cfg);
            let statements = analysis.root.to_statements(&cfg);
            
            // Extract variable names from debug info
            let debug_names = if let Some(debug_info) = &file.debug_info {
                let scope_offset = debug_info
                    .source_locations
                    .get(&function_id)
                    .and_then(|locs| locs.iter().find_map(|l| l.scope_offset));
                debug_info.build_variable_map(scope_offset)
            } else {
                std::collections::HashMap::new()
            };
            
            // Rename registers if we have debug info
            let statements = if !debug_names.is_empty() {
                rename_registers(statements, &debug_names)
            } else {
                statements
            };

            ctx.analyze_function(function_id, &statements);
        } else {
        }
    }

    Ok(ctx)
}

// Decompile all functions using the new pipeline.
pub fn decompile_all_v2(
    file: &BytecodeFile,
    format: &BytecodeFormat,
    options: &DecompileOptionsV2,
) -> Result<String> {
    decompile_all_v2_with_closures(file, format, options, true)
}

// Decompile all functions with optional cross-function closure resolution.
pub fn decompile_all_v2_with_closures(
    file: &BytecodeFile,
    format: &BytecodeFormat,
    options: &DecompileOptionsV2,
    resolve_closures: bool,
) -> Result<String> {
    
    // Build closure context if requested
    let mut closure_ctx = if resolve_closures {
        Some(build_closure_context_from_file(file, format)?)
    } else {
        None
    };

    // 1. Build Metro Registry using RAW IR (Pass 1)
    // We must use unoptimized IR because optimizations (like copy propagation) can 
    // obtain the __d call patterns that the registry analyzer expects.
    let mut registry = crate::analysis::MetroRegistry::new();
    let raw_options = DecompileOptionsV2 {
        resolve_strings: true,
        ..DecompileOptionsV2::default()
    };

    for i in 0..file.header.function_count {
        if let Ok(stmts) = generate_ir(file, format, i, &raw_options, None, false) {
             registry.analyze_statements(&stmts);
        }
    }
    // 2. Generate Optimized IR & Apply Naming (Pass 1)
    let mut all_ir = std::collections::HashMap::new();

    for i in 0..file.header.function_count {
        // Defer closure resolution! We will do it after naming.
        if let Ok(statements) = generate_ir(file, format, i, options, closure_ctx.as_ref(), false) {
            
            // Apply register naming and semantic variable naming EARLY
            // This ensures ClosureContext captures meaningful names.

            // Analyze registers and generate better names
            let reg_info = analyze_registers(&statements);
            
            // Extract variable names from debug info if available
            let debug_names = if let Some(debug_info) = &file.debug_info {
                let scope_offset = debug_info
                    .source_locations
                    .get(&i)
                    .and_then(|locs| locs.iter().find_map(|l| l.scope_offset));
                debug_info.build_variable_map(scope_offset)
            } else {
                std::collections::HashMap::new()
            };

            let mut used_names = std::collections::HashSet::new();
            // Reserve names used in debug info
            for name in debug_names.values() {
                used_names.insert(name.clone());
            }

            let names: std::collections::HashMap<u32, String> = reg_info
                .iter()
                .map(|(&r, info)| {
                    if let Some(name) = debug_names.get(&r) {
                        (r, name.clone())
                    } else {
                        (r, generate_name(r, info, &mut used_names))
                    }
                })
                .collect();
            
            let named_stmts = rename_registers(statements, &names);

            // Apply semantic variable naming (fetch() → response, new Date() → date, etc.)
            let semantic_stmts = transforms::infer_variable_names(named_stmts);
            
            // Simplify again after naming
            let final_stmts: Vec<Statement> = semantic_stmts.into_iter().map(|s| simplify_stmt(&s)).collect();
            
            // UPDATE CLOSURE CONTEXT
            // Now that statements have semantic names ("email", "token"), we re-analyze
            // so that captured slots point to "email" instead of "r1" or generic names.
            if let Some(ctx) = &mut closure_ctx {
                ctx.analyze_function(i, &final_stmts);
            }

            all_ir.insert(i, final_stmts);
        }
    }

    // 3. Propagate Module Names
    // We use the registry built from raw IR to rename variables in the optimized IR.
    crate::analysis::metro::propagate_module_names(&mut all_ir, &registry, &mut closure_ctx);

    // 4. Resolve Closures (Pass 2)
    // Now that context has semantic names, we resolve captures.
    if let Some(ctx) = &closure_ctx {
        for (i, stmts) in all_ir.iter_mut() {
            let closure_info = ctx.get_closure_info_for(*i);
            if !closure_info.slots.is_empty() {
                let mut temp_stmts = Vec::new();
                std::mem::swap(stmts, &mut temp_stmts);
                *stmts = crate::analysis::resolve_closures(temp_stmts, &closure_info);
            }
        }
    }

    // 2.4 Run Metro Export Analysis
    // We need to identify which functions are exported by each module
    // This allows IPA to resolve cross-module calls (e.g. require('foo').bar())
    for module in registry.modules.values_mut() {
        crate::analysis::metro::exports::ExportAnalyzer::analyze(module, &all_ir);
    }

    // 2.5 Build function name index for IPA
    // This allows resolving method calls like obj.loginWithToken() by function name
    let func_name_index = build_function_name_index(file);

    // 2.6 Run Inter-procedural Analysis (IPA)
    let global_analysis = crate::analysis::run_ipa(&all_ir, &registry, &func_name_index);

    // 3. Apply parameter renaming and Generate Code
    let mut output = String::new();

    for i in 0..file.header.function_count {
        if i > 0 {
            output.push('\n');
        }

        if let Some(mut statements) = all_ir.remove(&i) {
             // Apply IPA parameter names to the IR
             if let Some(param_names) = global_analysis.param_names.get(&i) {
                  transforms::exports::rename_param_registers(&mut statements, param_names);
             }

             // Generate code
             let function_name = get_function_name(file, i);
             
             // Get params with IPA names
             let params = if let Some(names) = global_analysis.param_names.get(&i) {
                 // Convert Option<String> to String, falling back to argN
                 names.iter().enumerate().map(|(idx, n)| {
                     n.clone().unwrap_or_else(|| format!("arg{idx}"))
                 }).collect()
             } else {
                 get_function_params(file, i)
             };

             let codegen_options = CodegenOptions::default();
             let mut codegen = Codegen::new(codegen_options);

             output.push_str(&format!("function {}({}) {{\n", function_name, params.join(", ")));

             let body = codegen.generate_statements(&statements);
             for line in body.lines() {
                 output.push_str("  ");
                 output.push_str(line);
                 output.push('\n');
             }

             output.push_str("}\n");
        } else {
             // Function failed to generate IR, print error stub
             output.push_str(&format!("// Error decompiling function {i}\n"));
        }
    }
    Ok(output)
}

fn get_function_name(file: &BytecodeFile, function_id: u32) -> String {
    file.function_headers
        .get(function_id as usize)
        .and_then(|h| file.string_at(h.function_name()))
        .filter(|e| !e.value.is_empty() && is_valid_identifier(&e.value))
        .map(|e| e.value.clone())
        .unwrap_or_else(|| format!("f{function_id}"))
}

fn get_function_params(file: &BytecodeFile, function_id: u32) -> Vec<String> {
    let param_count = file.function_headers
        .get(function_id as usize)
        .map(|h| h.param_count())
        .unwrap_or(0);

    (0..param_count).map(|i| format!("arg{i}")).collect()
}

/// Build an index of function names to their IDs.
/// This allows IPA to resolve method calls like `obj.loginWithToken()` by looking up the function name.
fn build_function_name_index(file: &BytecodeFile) -> crate::analysis::FunctionNameIndex {
    let mut index = std::collections::HashMap::new();

    for (id, header) in file.function_headers.iter().enumerate() {
        if let Some(entry) = file.string_at(header.function_name()) {
            let name = &entry.value;
            if !name.is_empty() && is_valid_identifier(name) {
                // Only add if not already present (first definition wins)
                index.entry(name.clone()).or_insert(id as u32);
            }
        }
    }

    index
}
