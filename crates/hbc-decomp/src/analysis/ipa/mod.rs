mod structs;
mod graph;
mod traversal;
mod inference;

pub use structs::GlobalAnalysis;
pub use traversal::FunctionNameIndex;
use graph::CallGraph;
use traversal::collect_info;
use inference::{vote_on_names, is_generic_name};
use std::collections::HashMap;
use crate::ir::Statement;

use super::metro::registry::MetroRegistry;

pub fn run_ipa(
    functions: &HashMap<u32, Vec<Statement>>,
    metro_registry: &MetroRegistry,
    func_name_index: &FunctionNameIndex,
) -> GlobalAnalysis {
    let mut analysis = GlobalAnalysis::new();
    let mut graph = CallGraph::new();
    let mut call_sites: HashMap<u32, Vec<Vec<Option<String>>>> = HashMap::new();
    let mut self_param_names: HashMap<u32, Vec<Vec<Option<String>>>> = HashMap::new();

    // Pass 1: Collect initial structural names and links
    for (&func_id, stmts) in functions {
        collect_info(func_id, stmts, &mut graph, &mut call_sites, &mut self_param_names, &mut analysis.param_links, metro_registry, func_name_index);
    }
    
    // Initial vote based on bodies and call names
    for (&func_id, sites) in &call_sites {
        analysis.param_names.insert(func_id, vote_on_names(sites.clone()));
    }
    for (func_id, sites) in self_param_names {
        let structural = vote_on_names(sites);
        let existing = analysis.param_names.entry(func_id).or_insert_with(|| vec![None; structural.len()]);
        for (i, name) in structural.into_iter().enumerate() {
            if i < existing.len() && existing[i].is_none() {
                existing[i] = name;
            }
        }
    }
    
    // Pass 2: Propagate names across links (Fixed-point)
    for _ in 0..3 {
        let mut changes = false;
        let mut updates = Vec::new();
        
        for &((src_id, src_idx), (dst_id, dst_idx)) in &analysis.param_links {
            // Propagate src -> dst
            if let Some(src_names) = analysis.param_names.get(&src_id) {
                if let Some(Some(name)) = src_names.get(src_idx as usize) {
                    updates.push((dst_id, dst_idx as usize, name.clone()));
                }
            }
            // Propagate dst -> src
            if let Some(dst_names) = analysis.param_names.get(&dst_id) {
                if let Some(Some(name)) = dst_names.get(dst_idx as usize) {
                    updates.push((src_id, src_idx as usize, name.clone()));
                }
            }
        }
        
        for (id, idx, name) in updates {
            let entry = analysis.param_names.entry(id).or_insert_with(Vec::new);
            if entry.len() <= idx { entry.resize(idx + 1, None); }
            if entry[idx].is_none() {
                // Check if it's a generic name before accepting back-propagation
                if !is_generic_name(&name) {
                    entry[idx] = Some(name);
                    changes = true;
                }
            }
        }
        
        if !changes { break; }
    }
    
    analysis
}
