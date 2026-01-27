use hbc_decomp::{BytecodeFile, BytecodeFormat, DecompileOptionsV2, IRBuilder, IRBuilderOptions, StructureAnalysis, ClosureInfo, Decompiler};
use std::collections::HashSet;
use std::error::Error;
use regex::Regex;

/// Decompile a function and expand all referenced functions up to a certain depth.
pub fn decompile_with_expansion(
    file: &BytecodeFile,
    format: &BytecodeFormat,
    root_function_id: u32,
    options: &DecompileOptionsV2,
    max_depth: usize,
) -> Result<String, Box<dyn Error>> {
    let mut output = String::new();
    let mut decompiled: HashSet<u32> = HashSet::new();
    let mut queue: Vec<(u32, usize)> = vec![(root_function_id, 0)];

    // Regex to find function references like /* F123 */
    let func_ref_re = Regex::new(r"/\* F(\d+) \*/").unwrap();

    while let Some((func_id, depth)) = queue.pop() {
        if decompiled.contains(&func_id) {
            continue;
        }
        decompiled.insert(func_id);

        // Add separator for nested functions
        if !output.is_empty() {
            output.push_str("\n// ========================================\n");
            output.push_str(&format!("// Referenced function F{func_id}\n"));
            output.push_str("// ========================================\n\n");
        }

        // Decompile the function
        let func_output = hbc_decomp::decompile_function_v2(file, format, func_id, options)?;
        output.push_str(&func_output);

        // If we haven't reached max depth, find and queue referenced functions
        if depth < max_depth {
            for cap in func_ref_re.captures_iter(&func_output) {
                if let Ok(ref_id) = cap[1].parse::<u32>() {
                    if !decompiled.contains(&ref_id) {
                        queue.push((ref_id, depth + 1));
                    }
                }
            }
        }
    }

    // Add summary
    output.push_str(&format!(
        "\n// ========================================\n\
         // Expansion summary: {} functions decompiled\n\
         // Root: F{}, Max depth: {}\n\
         // ========================================\n",
        decompiled.len(),
        root_function_id,
        max_depth
    ));

    Ok(output)
}

pub fn print_closure_info(
    file: &BytecodeFile,
    format: &BytecodeFormat,
    function_id: u32,
) -> Result<(), Box<dyn Error>> {
    let options = IRBuilderOptions {
        resolve_strings: true,
        include_offsets: false,
    };
    let mut builder = IRBuilder::new(file, format, options);
    let cfg = builder.build_function(function_id)?;

    // Get structured statements
    let analysis = StructureAnalysis::analyze(&cfg);
    let statements = analysis.root.to_statements(&cfg);

    // Analyze closures
    let closure_info = ClosureInfo::analyze(&statements);

    println!("=== Closure mappings for function {function_id} ===\n");

    if closure_info.slots.is_empty() {
        println!("No closure slots found.");
    } else {
        let mut slots: Vec<_> = closure_info.slots.iter().collect();
        slots.sort_by_key(|(k, _)| *k);

        for (slot, value) in slots {
            let desc = match value {
                hbc_decomp::ClosureSlotValue::Function { id, name } => {
                    if let Some(n) = name {
                        format!("F{id} ({n})")
                    } else {
                        format!("F{id}")
                    }
                }
                hbc_decomp::ClosureSlotValue::Constant(c) => format!("constant: {c}"),
                hbc_decomp::ClosureSlotValue::Variable(v) => format!("variable: {v}"),
                hbc_decomp::ClosureSlotValue::Unknown => "unknown".to_string(),
            };
            println!("  closure_{slot} = {desc}");
        }
    }

    Ok(())
}

pub fn expand_json(
    file: &BytecodeFile,
    format: &BytecodeFormat,
    function_id: u32,
    options: &DecompileOptionsV2,
) -> Result<String, Box<dyn Error>> {
    let decompiler = Decompiler::from_parts(file.clone(), format.clone());
    let ir = decompiler.decompile_to_ir(function_id, options)?;
    let json = serde_json::to_string_pretty(&ir)?;
    Ok(json)
}


