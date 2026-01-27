use std::collections::HashMap;

/// Represents the Call Graph of the module.
pub struct CallGraph {
    // Caller -> Callees
    pub calls: HashMap<u32, Vec<u32>>,
    // Callee -> Callers
    pub callers: HashMap<u32, Vec<u32>>,
}

impl Default for CallGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl CallGraph {
    pub fn new() -> Self {
        Self {
            calls: HashMap::new(),
            callers: HashMap::new(),
        }
    }

    pub fn add_call(&mut self, caller: u32, callee: u32) {
        self.calls.entry(caller).or_default().push(callee);
        self.callers.entry(callee).or_default().push(caller);
    }
}
