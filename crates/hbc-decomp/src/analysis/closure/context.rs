use std::collections::{HashMap, HashSet};
use crate::ir::{Statement, Expression, AssignTarget, Value};
use super::info::{ClosureInfo, ClosureSlotValue, encode_level_slot};

// Global closure context for cross-function resolution.
// Tracks parent-child relationships and environment slot assignments across all functions.
#[derive(Debug, Clone, Default)]
pub struct ClosureContext {
    // Map from function ID to its parent function ID
    pub parent_function: HashMap<u32, u32>,
    // Map from function ID to closure info extracted from that function
    pub function_closures: HashMap<u32, ClosureInfo>,
    // Map from function ID to its name (if known)
    pub function_names: HashMap<u32, String>,
    // Set of function IDs that are async (created with CreateAsyncClosure)
    pub async_functions: HashSet<u32>,
    // Set of function IDs that are generators (created with CreateGeneratorClosure)
    pub generator_functions: HashSet<u32>,
}

impl ClosureContext {
    pub fn new() -> Self {
        Self::default()
    }

    // Record that function `child` is created inside function `parent`.
    pub fn add_child(&mut self, parent: u32, child: u32) {
        self.parent_function.insert(child, parent);
    }

    // Record the closure info for a function.
    pub fn add_closure_info(&mut self, function_id: u32, info: ClosureInfo) {
        self.function_closures.insert(function_id, info);
    }

    // Record a function name.
    pub fn add_function_name(&mut self, function_id: u32, name: String) {
        self.function_names.insert(function_id, name);
    }

    // Update the variable name associated with a slot in a function's closure info.
    pub fn update_slot_variable(&mut self, function_id: u32, slot: u32, name: String) {
        if let Some(info) = self.function_closures.get_mut(&function_id) {
            info.slots.insert(slot, ClosureSlotValue::Variable(name));
        }
    }

    // Mark a function as async.
    pub fn mark_async(&mut self, function_id: u32) {
        self.async_functions.insert(function_id);
    }

    // Mark a function as a generator.
    pub fn mark_generator(&mut self, function_id: u32) {
        self.generator_functions.insert(function_id);
    }

    // Check if a function is async.
    pub fn is_async(&self, function_id: u32) -> bool {
        self.async_functions.contains(&function_id)
    }

    // Check if a function is a generator.
    pub fn is_generator(&self, function_id: u32) -> bool {
        self.generator_functions.contains(&function_id)
    }

    // Get the closure info for resolving variables in the given function.
    // This looks up the parent chain to find closure slot assignments.
    // Supports multi-level closure resolution for deep nesting.
    pub fn get_closure_info_for(&self, function_id: u32) -> ClosureInfo {
        let mut combined = ClosureInfo::new();

        // Build a list of all ancestors (parent, grandparent, etc.)
        let mut ancestors = Vec::new();
        let mut current = function_id;
        while let Some(&parent) = self.parent_function.get(&current) {
            ancestors.push(parent);
            current = parent;
        }

        // 0. Include local slots (level 0)
        if let Some(local_info) = self.function_closures.get(&function_id) {
            for (slot, value) in &local_info.slots {
                combined.slots.insert(*slot, value.clone());
            }
        }

        // For each ancestor level, copy their closure slots
        // Level 0 = direct parent
        for (level, &ancestor) in ancestors.iter().enumerate() {
            if let Some(ancestor_info) = self.function_closures.get(&ancestor) {
                for (&slot, value) in &ancestor_info.slots {
                    // Store with the level info so we can resolve ClosureVar { level, slot }
                    let ir_level = level as u32;
                    let key = encode_level_slot(ir_level, slot);
                    combined.slots.entry(key).or_insert_with(|| value.clone());
                }
            }
        }

        combined
    }

    // Resolve a closure variable at a specific environment level.
    pub fn resolve_closure_var(&self, function_id: u32, level: u32, slot: u32) -> Option<ClosureSlotValue> {
        // Walk up the parent chain to the appropriate level
        let mut current = function_id;
        for _ in 0..=level {
            current = *self.parent_function.get(&current)?;
        }
        
        // Now get the closure info for that ancestor
        self.function_closures.get(&current)?.slots.get(&slot).cloned()
    }

    // Get function name by ID.
    pub fn get_function_name(&self, function_id: u32) -> Option<&str> {
        self.function_names.get(&function_id).map(|s| s.as_str())
    }

    // Analyze statements to extract parent-child relationships and closure info.
    pub fn analyze_function(&mut self, function_id: u32, stmts: &[Statement]) {
        let mut info = ClosureInfo::new();
        let mut register_values: HashMap<u32, ClosureSlotValue> = HashMap::new();

        for stmt in stmts {
            self.analyze_stmt_context(function_id, stmt, &mut info, &mut register_values);
        }

        self.function_closures.insert(function_id, info);
    }

    fn analyze_stmt_context(
        &mut self,
        parent_fn: u32,
        stmt: &Statement,
        info: &mut ClosureInfo,
        reg_values: &mut HashMap<u32, ClosureSlotValue>,
    ) {
        match stmt {
            Statement::Assign { target, value } => {
                // Track function creation
                if let Expression::Function { id, name, is_async, is_generator, .. } = value {
                    self.add_child(parent_fn, id.0);
                    if let Some(n) = name {
                        self.add_function_name(id.0, n.clone());
                    }

                    // Track async and generator functions
                    if *is_async {
                        self.mark_async(id.0);
                    }
                    if *is_generator {
                        self.mark_generator(id.0);
                    }

                    // Track what register holds this function
                    if let AssignTarget::Register(r) = target {
                        reg_values.insert(*r, ClosureSlotValue::Function {
                            id: id.0,
                            name: name.clone(),
                        });
                    }
                }

                // Track other register assignments
                if let AssignTarget::Register(r) = target {
                    if let Some(val) = self.extract_value(value) {
                        reg_values.insert(*r, val);
                    }
                }

                // Track closure slot assignments
                if let AssignTarget::ClosureVar { slot, level } = target {
                    if *level == 0 {
                        if let Some(val) = self.value_from_expr(value, reg_values) {
                            info.slots.insert(*slot, val);
                        }
                    }
                }
            }
            Statement::If { then_body, else_body, .. } => {
                for s in then_body {
                    self.analyze_stmt_context(parent_fn, s, info, reg_values);
                }
                for s in else_body {
                    self.analyze_stmt_context(parent_fn, s, info, reg_values);
                }
            }
            Statement::While { body, .. } | Statement::For { body, .. } => {
                for s in body {
                    self.analyze_stmt_context(parent_fn, s, info, reg_values);
                }
            }
            Statement::Block(inner) => {
                for s in inner {
                    self.analyze_stmt_context(parent_fn, s, info, reg_values);
                }
            }
            Statement::TryCatch { try_body, catch_body, finally_body, .. } => {
                for s in try_body {
                    self.analyze_stmt_context(parent_fn, s, info, reg_values);
                }
                for s in catch_body {
                    self.analyze_stmt_context(parent_fn, s, info, reg_values);
                }
                for s in finally_body {
                    self.analyze_stmt_context(parent_fn, s, info, reg_values);
                }
            }
            _ => {}
        }
    }

    fn extract_value(&self, expr: &Expression) -> Option<ClosureSlotValue> {
        match expr {
            Expression::Function { id, name, .. } => Some(ClosureSlotValue::Function {
                id: id.0,
                name: name.clone(),
            }),
            Expression::Value(Value::Constant(c)) => {
                Some(ClosureSlotValue::Constant(format!("{c}")))
            }
            Expression::Value(Value::Parameter(i)) => {
                Some(ClosureSlotValue::Variable(format!("arg{i}")))
            }
            Expression::Value(Value::Variable(name)) => {
                Some(ClosureSlotValue::Variable(name.clone()))
            }
            _ => None,
        }
    }

    fn value_from_expr(
        &self,
        expr: &Expression,
        reg_values: &HashMap<u32, ClosureSlotValue>,
    ) -> Option<ClosureSlotValue> {
        match expr {
            Expression::Function { id, name, .. } => Some(ClosureSlotValue::Function {
                id: id.0,
                name: name.clone(),
            }),
            Expression::Value(Value::Register(r)) => reg_values.get(r).cloned(),
            Expression::Value(Value::Constant(c)) => {
                Some(ClosureSlotValue::Constant(format!("{c}")))
            }
            Expression::Value(Value::Variable(name)) => {
                Some(ClosureSlotValue::Variable(name.clone()))
            }
            Expression::Value(Value::Parameter(i)) => {
                Some(ClosureSlotValue::Variable(format!("arg{i}")))
            }
            _ => Some(ClosureSlotValue::Unknown),
        }
    }
}
