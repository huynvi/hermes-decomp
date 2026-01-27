pub mod registry;
pub mod detection;
pub mod exports;
mod graph;
mod propagation;

pub use registry::{MetroModule, MetroRegistry};
pub use graph::{DependencyTree, DependencyGraph};
pub use detection::MetroDetector;
pub use propagation::propagate_module_names;

// Helper to expose analyze as a static method on MetroRegistry for compatibility
impl MetroRegistry {
    // Analyze the global function statements to extract module registrations.
    pub fn analyze(statements: &[crate::ir::Statement]) -> Self {
        let mut registry = Self::new();
        MetroDetector::analyze_statements(statements, &mut registry);
        registry
    }

    pub fn analyze_statements(&mut self, statements: &[crate::ir::Statement]) {
        MetroDetector::analyze_statements(statements, self);
    }
}

// Re-expose Graph methods on Registry for compatibility if needed, 
// OR refactor callers to use DependencyGraph directly.
// For now, let's keep the extension methods or just use the new API.
// To match original API:
impl MetroRegistry {
     // Get dependency tree wrapper
    pub fn get_dependency_tree(&self, module_id: u32, max_depth: usize) -> DependencyTree {
        DependencyGraph::get_dependency_tree(self, module_id, max_depth)
    }
}
