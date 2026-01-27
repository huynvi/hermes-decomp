// High-level IR builder from Hermes bytecode.

use std::collections::HashMap;
use crate::{BytecodeFile, BytecodeFormat, Instruction, Result};
use crate::ir::{CFG, BlockId, Statement, Terminator, Expression};

use super::jump_analysis::find_block_starts;
use super::opcodes_flow::FlowResult;
use super::dispatch::dispatch_instruction;

// Options for the IR builder.
#[derive(Debug, Clone, Default)]
pub struct IRBuilderOptions {
    // Resolve string table indices to actual strings.
    pub resolve_strings: bool,
    // Include bytecode offsets as comments.
    pub include_offsets: bool,
}

// IR builder that converts bytecode to CFG.
/// 
/// The IR Builder is responsible for:
/// 1. Decoding raw bytecode instructions for a function.
/// 2. Constructing a Control Flow Graph (CFG) where nodes are basic blocks.
/// 3. Handling jumps, branches, and exception handling blocks.
/// 4. Translating stack-based bytecode operations into pseudo-register-based IR statements.
pub struct IRBuilder<'a> {
    file: &'a BytecodeFile,
    format: &'a BytecodeFormat,
    options: IRBuilderOptions,
}

impl<'a> IRBuilder<'a> {
    pub fn new(file: &'a BytecodeFile, format: &'a BytecodeFormat, options: IRBuilderOptions) -> Self {
        IRBuilder { file, format, options }
    }

    // Build a CFG for a function.
    pub fn build_function(&mut self, function_id: u32) -> Result<CFG> {
        let instructions = self.file.decode_function_instructions(self.format, function_id)?;
        self.build_from_instructions(&instructions)
    }

    fn build_from_instructions(&mut self, instructions: &[Instruction]) -> Result<CFG> {
        if instructions.is_empty() {
            let mut cfg = CFG::new();
            cfg.get_mut(cfg.entry).expect("entry block must exist").set_terminator(Terminator::Return(None));
            return Ok(cfg);
        }

        // Find all block start offsets.
        // Blocks start at:
        // - Function entry (already handled).
        // - Targets of any jump/branch instruction.
        // - Targets of exception handlers (catch blocks).
        // - Instructions following a terminator (like Return or Throw).
        let block_starts = find_block_starts(instructions, self.format, self.file);

        // Create mapping from offset to BlockId
        let mut offset_to_block: HashMap<u32, BlockId> = HashMap::new();
        let mut cfg = CFG::new();

        // Entry block is always at offset 0
        let first_offset = instructions[0].offset;
        offset_to_block.insert(first_offset, cfg.entry);

        // Create blocks for all other start points
        for &offset in &block_starts {
            if offset != first_offset {
                let id = cfg.create_block();
                offset_to_block.insert(offset, id);
            }
        }

        // Process instructions into blocks
        let mut current_block = cfg.entry;
        let mut current_stmts: Vec<Statement> = Vec::new();

        for inst in instructions {
            // Check if this instruction starts a new block
            if let Some(&block_id) = offset_to_block.get(&inst.offset) {
                if block_id != current_block {
                    // Finalize previous block with jump to this block.
                    // This handles fallthrough: if code execution just flows into the next block 
                    // without an explicit jump, we insert an implicit Jumperator.
                    self.finalize_block(&mut cfg, current_block, current_stmts, block_id);
                    current_stmts = Vec::new();
                    current_block = block_id;
                }
            }

            // Add offset comment if requested
            if self.options.include_offsets {
                current_stmts.push(Statement::Comment(format!("@{:04x}", inst.offset)));
            }

            // Dispatch instruction
            let result = dispatch_instruction(
                inst,
                self.file,
                self.format,
                self.options.resolve_strings,
            );

            match result {
                FlowResult::Statement(stmt) => {
                    current_stmts.push(stmt);
                }
                FlowResult::Jump { target } => {
                    let target_block = self.get_or_create_block(&mut cfg, &mut offset_to_block, target);
                    self.set_block_stmts(&mut cfg, current_block, current_stmts);
                    cfg.get_mut(current_block).expect("current block must exist").set_terminator(Terminator::Jump(target_block));
                    current_stmts = Vec::new();
                    // Mark that we've finished this block - next instruction starts a new block
                    current_block = target_block;
                }
                FlowResult::Branch { condition, target, fallthrough } => {
                    let true_block = self.get_or_create_block(&mut cfg, &mut offset_to_block, target);
                    let false_block = self.get_or_create_block(&mut cfg, &mut offset_to_block, fallthrough);
                    self.set_block_stmts(&mut cfg, current_block, current_stmts);
                    cfg.get_mut(current_block).expect("current block must exist").set_terminator(
                        Terminator::Branch { condition, true_target: true_block, false_target: false_block }
                    );
                    current_stmts = Vec::new();
                    // The next instruction will be the fallthrough block
                    current_block = false_block;
                }
                FlowResult::Return(value) => {
                    self.set_block_stmts(&mut cfg, current_block, current_stmts);
                    cfg.get_mut(current_block).expect("current block must exist").set_terminator(Terminator::Return(value));
                    current_stmts = Vec::new();
                    // After a return, the next instructions (if any) will start a new block
                    // This will be handled by the block start check at the top of the loop
                }
                FlowResult::Throw(value) => {
                    self.set_block_stmts(&mut cfg, current_block, current_stmts);
                    cfg.get_mut(current_block).expect("current block must exist").set_terminator(Terminator::Throw(value));
                    current_stmts = Vec::new();
                    // Same as return - next block will be handled at loop top
                }
                FlowResult::Noop => {
                    // Do nothing
                }
                FlowResult::Switch { value, default, cases } => {
                    let default_block = self.get_or_create_block(&mut cfg, &mut offset_to_block, default);
                    let mut switch_cases = Vec::new();

                    for (case_val, target_offset) in cases {
                        let target_block = self.get_or_create_block(&mut cfg, &mut offset_to_block, target_offset);
                        switch_cases.push((
                            Expression::constant(crate::ir::Constant::Integer(case_val as i32)),
                            target_block
                        ));
                    }
                    
                    self.set_block_stmts(&mut cfg, current_block, current_stmts);
                    cfg.get_mut(current_block).expect("current block must exist").set_terminator(
                        Terminator::Switch { value, cases: switch_cases, default: default_block }
                    );
                    current_stmts = Vec::new();
                    // Next instruction is dead or start of a block
                    // We don't know the next block here, so let the loop handle it
                    // But we need to update current_block to something valid for safety, though it won't be used if next instruction is a block start
                    // If fallthrough happens (not possible for Switch), we'd need it.
                    // Just set it to default block as a placeholder?
                    // Actually, find_block_starts should have marked all targets as block starts.
                    // So next iteration will update current_block.
                    // But we need to ensure we don't append to a finished block.
                    // The trick is: `current_block` is dead here.
                }
            }
        }

        // Finalize last block if needed
        if !current_stmts.is_empty() || matches!(
            cfg.get(current_block).map(|b| &b.terminator),
            Some(Terminator::None)
        ) {
            self.set_block_stmts(&mut cfg, current_block, current_stmts);
            if matches!(cfg.get(current_block).map(|b| &b.terminator), Some(Terminator::None)) {
                cfg.get_mut(current_block).expect("current block must exist").set_terminator(
                    Terminator::Return(Some(Expression::constant(crate::ir::Constant::Undefined)))
                );
            }
        }

        Ok(cfg)
    }

    fn get_or_create_block(
        &self,
        cfg: &mut CFG,
        offset_to_block: &mut HashMap<u32, BlockId>,
        offset: u32,
    ) -> BlockId {
        if let Some(&id) = offset_to_block.get(&offset) {
            id
        } else {
            let id = cfg.create_block();
            offset_to_block.insert(offset, id);
            id
        }
    }

    fn set_block_stmts(&self, cfg: &mut CFG, block: BlockId, stmts: Vec<Statement>) {
        if let Some(b) = cfg.get_mut(block) {
            b.statements = stmts;
        }
    }

    fn finalize_block(&self, cfg: &mut CFG, block: BlockId, stmts: Vec<Statement>, next: BlockId) {
        if let Some(b) = cfg.get_mut(block) {
            b.statements = stmts;
            if matches!(b.terminator, Terminator::None) {
                b.set_terminator(Terminator::Jump(next));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_options_default() {
        let options = IRBuilderOptions::default();
        assert!(!options.resolve_strings);
        assert!(!options.include_offsets);
    }
}
