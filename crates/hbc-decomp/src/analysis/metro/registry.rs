use std::collections::HashMap;

/// Information about a Metro module.
#[derive(Debug, Clone)]
pub struct MetroModule {
    // The module ID (used in require calls).
    // In Metro, this is typically an integer index (0, 1, 2...)
    // but can sometimes be mapped from complex requires.
    pub module_id: u32,
    // The function ID that implements this module
    pub function_id: u32,
    // Optional module name/path
    pub name: Option<String>,
    // Dependencies (module IDs this module requires)
    pub dependencies: Vec<u32>,
    // Exported functions (property name -> function ID)
    pub exports: HashMap<String, u32>,
}

/// Registry of all Metro modules in a bundle.
/// 
/// Helps traversing the dependency graph.
/// Essential for resolving imports/requires across files.
/// 
/// Example:
/// A Require call `require(5)` inside function `f10` needs this registry to know that
/// module 5 maps to function `f20`, so we can analyze `f20`'s exports.
#[derive(Debug, Clone, Default)]
pub struct MetroRegistry {
    // Module ID -> Module info
    pub modules: HashMap<u32, MetroModule>,
    // Function ID -> Module ID (reverse lookup)
    pub function_to_module: HashMap<u32, u32>,
}

impl MetroRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a module by its ID.
    pub fn get_module(&self, module_id: u32) -> Option<&MetroModule> {
        self.modules.get(&module_id)
    }

    /// Get the module that a function implements.
    pub fn get_module_for_function(&self, function_id: u32) -> Option<&MetroModule> {
        self.function_to_module.get(&function_id)
            .and_then(|mod_id| self.modules.get(mod_id))
    }

    // Graph related helpers that just access the struct (not traversing) can stay here, 
    // but deeper traversal (like trees) should move to graph.rs.
    // For now we expose the data directly.
    
    // Get all modules that depend on a given module.
    pub fn get_dependents(&self, module_id: u32) -> Vec<u32> {
        self.modules.values()
            .filter(|m| m.dependencies.contains(&module_id))
            .map(|m| m.module_id)
            .collect()
    }
}
