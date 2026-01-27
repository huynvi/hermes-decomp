use std::collections::HashMap;
use crate::ir::{Statement, Expression, AssignTarget, Value};

// Encode level and slot into a single u32 key for HashMap storage.
// Uses high 8 bits for level, low 24 bits for slot.
pub fn encode_level_slot(level: u32, slot: u32) -> u32 {
    ((level & 0xFF) << 24) | (slot & 0xFFFFFF)
}

// Decode level and slot from an encoded u32 key.
#[allow(dead_code)]
pub fn decode_level_slot(key: u32) -> (u32, u32) {
    let level = (key >> 24) & 0xFF;
    let slot = key & 0xFFFFFF;
    (level, slot)
}

// Value stored in a closure slot.
#[derive(Debug, Clone)]
pub enum ClosureSlotValue {
    // A function reference
    Function { id: u32, name: Option<String> },
    // A constant value
    Constant(String),
    // A variable/register value
    Variable(String),
    // Unknown
    Unknown,
}

// Information about what's stored in each closure slot.
#[derive(Debug, Clone)]
pub struct ClosureInfo {
    // Mapping from slot index to the value stored there
    pub slots: HashMap<u32, ClosureSlotValue>,
}

impl Default for ClosureInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl ClosureInfo {
    pub fn new() -> Self {
        Self {
            slots: HashMap::new(),
        }
    }

    // Analyze statements to extract closure slot assignments.
    pub fn analyze(stmts: &[Statement]) -> Self {
        let mut info = Self::new();
        let mut register_values: HashMap<u32, ClosureSlotValue> = HashMap::new();

        for stmt in stmts {
            info.analyze_stmt(stmt, &mut register_values);
        }

        info
    }

    fn analyze_stmt(&mut self, stmt: &Statement, reg_values: &mut HashMap<u32, ClosureSlotValue>) {
        match stmt {
            Statement::Assign { target, value } => {
                // Track what gets assigned to registers
                if let AssignTarget::Register(r) = target {
                    if let Some(val) = self.extract_value(value) {
                        reg_values.insert(*r, val);
                    }
                }

                // Track closure slot assignments
                if let AssignTarget::ClosureVar { slot, .. } = target {
                    if let Some(val) = self.value_from_expr(value, reg_values) {
                        self.slots.insert(*slot, val);
                    }
                }
            }
            Statement::If { then_body, else_body, .. } => {
                for s in then_body {
                    self.analyze_stmt(s, reg_values);
                }
                for s in else_body {
                    self.analyze_stmt(s, reg_values);
                }
            }
            Statement::While { body, .. } => {
                for s in body {
                    self.analyze_stmt(s, reg_values);
                }
            }
            Statement::Block(inner) => {
                for s in inner {
                    self.analyze_stmt(s, reg_values);
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

    // Get a human-readable name for a closure slot.
    pub fn get_slot_name(&self, slot: u32) -> String {
        match self.slots.get(&slot) {
            Some(ClosureSlotValue::Function { id, name }) => {
                if let Some(n) = name {
                    n.clone()
                } else {
                    format!("f{id}")
                }
            }
            Some(ClosureSlotValue::Constant(c)) => c.clone(),
            Some(ClosureSlotValue::Variable(v)) => v.clone(),
            Some(ClosureSlotValue::Unknown) | None => format!("closure_{slot}"),
        }
    }
}
