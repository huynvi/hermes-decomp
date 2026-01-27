use std::collections::{HashMap, HashSet};
use crate::ir::{Expression, PropertyKey};
use super::suggestions::{
    get_function_name, name_for_call, name_for_property, 
    name_for_instance, sanitize_name
};

pub struct VariableNamer {
    // Map from original name (or "r{reg}") to inferred name
    pub inferred_names: HashMap<String, String>,
    // Track which names are already used to avoid duplicates
    used_names: HashSet<String>,
    // Counter for disambiguation
    name_counters: HashMap<String, u32>,
}

impl VariableNamer {
    pub fn new() -> Self {
        Self {
            inferred_names: HashMap::new(),
            used_names: HashSet::new(),
            name_counters: HashMap::new(),
        }
    }

    pub fn suggest_name(&mut self, key: &str, base_name: &str) {
        if self.inferred_names.contains_key(key) {
            return;
        }

        let name = self.get_unique_name(base_name);
        self.inferred_names.insert(key.to_string(), name);
    }

    pub fn suggest_name_from_property(&mut self, var_name: &str, prop: &str) {
        self.suggest_name(var_name, prop);
    }

    fn get_unique_name(&mut self, base: &str) -> String {
        // Clean the base name
        let base = sanitize_name(base);

        if !self.used_names.contains(&base) {
            self.used_names.insert(base.clone());
            return base;
        }

        // Add a number suffix
        let counter = self.name_counters.entry(base.clone()).or_insert(1);
        loop {
            let name = format!("{}{}", base, counter);
            *counter += 1;
            if !self.used_names.contains(&name) {
                self.used_names.insert(name.clone());
                return name;
            }
        }
    }

    pub fn infer_name_from_expr(&self, expr: &Expression) -> Option<String> {
        match expr {
            // fetch(url) → response
            Expression::Call { callee, .. } => {
                if let Some(func_name) = get_function_name(callee) {
                    return Some(name_for_call(&func_name));
                }
            }

            // obj.property → property-based name
            Expression::Member { property: PropertyKey::Ident(prop), .. } => {
                return Some(name_for_property(prop));
            }

            // new Constructor() → instance name
            Expression::New { callee, .. } => {
                if let Some(class_name) = get_function_name(callee) {
                    return Some(name_for_instance(&class_name));
                }
            }

            // Array literals → items, arr, list
            Expression::Array { .. } => {
                return Some("items".to_string());
            }

            // Object literals → obj, config, options
            Expression::Object { .. } => {
                return Some("obj".to_string());
            }

            // Await expression → result of the awaited call
            Expression::Await(inner) => {
                return self.infer_name_from_expr(inner);
            }

            _ => {}
        }

        None
    }
}
