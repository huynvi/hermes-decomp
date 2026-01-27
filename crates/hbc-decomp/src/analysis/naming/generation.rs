use std::collections::HashSet;
use super::registers::{RegisterInfo, RegisterRole};

// Generate a variable name for a register based on its usage.
pub fn generate_name(_reg: u32, info: &RegisterInfo, used_names: &mut HashSet<String>) -> String {
    let base = match &info.role {
        RegisterRole::Array => "arr",
        RegisterRole::Object => "obj",
        RegisterRole::Function => "fn",
        RegisterRole::String => "str",
        RegisterRole::Number => "num",
        RegisterRole::Boolean => "flag",
        RegisterRole::BigInt => "bigint",
        RegisterRole::Iterator => "iter",
        RegisterRole::Promise => "promise",
        RegisterRole::This => "self",
        RegisterRole::Null | RegisterRole::Undefined => "tmp",
        RegisterRole::Unknown => {
            // Try to infer from property access
            if let Some(prop) = &info.from_property {
                let base = if prop.chars().all(|c| c.is_ascii_digit()) {
                    format!("v{}", prop)
                } else {
                    prop.clone()
                };
                return make_unique(base, used_names);
            }
            // Try to infer from accessed properties
            if info.accessed_props.contains("length") && info.called_methods.contains("push") {
                "arr"
            } else if !info.called_methods.is_empty() {
                "obj"
            } else {
                "tmp"
            }
        }
    };

    make_unique(base.to_string(), used_names)
}

fn make_unique(base: String, used: &mut HashSet<String>) -> String {
    if !used.contains(&base) {
        used.insert(base.clone());
        return base;
    }

    for i in 2..100 {
        let name = format!("{base}{i}");
        if !used.contains(&name) {
            used.insert(name.clone());
            return name;
        }
    }

    base
}
