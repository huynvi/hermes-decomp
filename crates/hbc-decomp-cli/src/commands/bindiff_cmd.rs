use std::path::PathBuf;
use std::collections::HashMap;
use hbc_decomp::{BytecodeFile, BytecodeFormat, DecompileOptionsV2, decompile_function_v2};
use crate::cli_args::{LayoutArg, FunctionLayoutArg};

pub fn run_bindiff(
    path1: &PathBuf,
    path2: &PathBuf,
    layout: LayoutArg,
    function_layout: FunctionLayoutArg,
    format_version: Option<u32>,
    diff_code: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading {}...", path1.display());
    let file1 = crate::helpers::load_file(path1, layout, function_layout)?;
    let format1 = crate::helpers::load_format(&file1, format_version)?;

    println!("Loading {}...", path2.display());
    let file2 = crate::helpers::load_file(path2, layout, function_layout)?;
    let format2 = crate::helpers::load_format(&file2, format_version)?;

    println!("Comparing functions...");
    
    // Name -> FunctionID
    let map1 = build_function_map(&file1);
    let map2 = build_function_map(&file2);
    
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut modified = Vec::new();
    let mut identical = 0;
    
    // Comparaison
    for (name, id1) in &map1 {
        if let Some(id2) = map2.get(name) {
            if !are_functions_identical(&file1, &format1, *id1, &file2, &format2, *id2) {
                modified.push((name.clone(), *id1, *id2));
            } else {
                identical += 1;
            }
        } else {
            removed.push(name.clone());
        }
    }
    
    for name in map2.keys() {
        if !map1.contains_key(name) {
            added.push(name.clone());
        }
    }
    
    println!("\n--- BinDiff Result ---");
    println!("Identical: {identical}");
    println!("Modified:  {}", modified.len());
    println!("Removed:   {}", removed.len());
    println!("Added:     {}", added.len());
    
    if !modified.is_empty() {
        println!("\nModified Functions:");
        for (name, id1, id2) in &modified {
            println!("  - {name} (ID: {id1} -> {id2})");
            
            if diff_code {
                println!("\n    --- LEFT (v1) ---");
                let code1 = decompile_function_v2(&file1, &format1, *id1, &DecompileOptionsV2::default())
                    .unwrap_or_else(|e| format!("Error: {e}"));
                for line in code1.lines() {
                    println!("    {line}");
                }
                
                println!("\n    --- RIGHT (v2) ---");
                let code2 = decompile_function_v2(&file2, &format2, *id2, &DecompileOptionsV2::default())
                    .unwrap_or_else(|e| format!("Error: {e}"));
                for line in code2.lines() {
                    println!("    {line}");
                }
                println!("\n    ------------------");
            }
        }
    }

    Ok(())
}

fn build_function_map(file: &BytecodeFile) -> HashMap<String, u32> {
    let mut map = HashMap::new();
    for (i, header) in file.function_headers.iter().enumerate() {
        let name = file.string_at(header.function_name())
            .map(|e| e.value.clone())
            .unwrap_or_else(|| format!("f{i}"));
        map.insert(name, i as u32);
    }
    map
}

fn are_functions_identical(
    f1: &BytecodeFile, fmt1: &BytecodeFormat, id1: u32,
    f2: &BytecodeFile, fmt2: &BytecodeFormat, id2: u32
) -> bool {
    let h1 = &f1.function_headers[id1 as usize];
    let h2 = &f2.function_headers[id2 as usize];
    
    if h1.bytecode_size_in_bytes() != h2.bytecode_size_in_bytes() {
        return false;
    }
    
    // Deep comparison via disassembly
    let dis1 = hbc_decomp::disassemble_function(f1, fmt1, id1, &hbc_decomp::DisasmOptions::default());
    let dis2 = hbc_decomp::disassemble_function(f2, fmt2, id2, &hbc_decomp::DisasmOptions::default());
    
    if let (Ok(d1), Ok(d2)) = (dis1, dis2) {
        strip_offsets(&d1) == strip_offsets(&d2)
    } else {
        false
    }
}

fn strip_offsets(s: &str) -> String {
    s.lines()
     .map(|line| {
         if let Some(idx) = line.find(':') {
             if idx < 10 { // assumed offset column
                 return &line[idx+1..];
             }
         }
         line
     })
     .collect::<String>()
}
