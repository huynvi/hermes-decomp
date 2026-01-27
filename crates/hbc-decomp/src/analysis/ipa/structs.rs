use std::collections::HashMap;

/// Global analysis result containing inferred information for all functions.
pub struct GlobalAnalysis {
    pub param_names: HashMap<u32, Vec<Option<String>>>, // FunctionID -> [Param Names]
    pub param_links: Vec<((u32, u32), (u32, u32))>,
}

impl Default for GlobalAnalysis {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalAnalysis {
    pub fn new() -> Self {
        Self {
            param_names: HashMap::new(),
            param_links: Vec::new(),
        }
    }
}
