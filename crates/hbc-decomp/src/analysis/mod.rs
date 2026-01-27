// Analysis passes for the IR.

pub mod liveness;
pub mod reaching;
pub mod structure;
pub mod loops;
pub mod naming;
pub mod closure;
pub mod metro;
pub mod xref;
pub mod ipa;

// Re-export common types
pub use liveness::LivenessInfo;
pub use reaching::ReachingDefs;
pub use structure::{StructureAnalysis, Structure};
pub use loops::LoopInfo;
pub use naming::{analyze_registers, generate_name, rename_registers, RegisterInfo, RegisterRole};
pub use closure::{resolve_closures, ClosureContext, ClosureInfo, ClosureSlotValue};
pub use metro::{MetroRegistry, MetroModule, DependencyTree};
pub use xref::{find_function_refs, find_string_xrefs, XrefResult};
pub use ipa::{run_ipa, GlobalAnalysis, FunctionNameIndex};
