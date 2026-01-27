use std::sync::Mutex;
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo, CallToolResult, Content},
    schemars, tool, tool_router, tool_handler,
};
use rmcp::ErrorData as McpError;
use serde::Deserialize;

use hbc_decomp::{BytecodeFile, DecompileOptionsV2, Decompiler, IRBuilder, IRBuilderOptions, StructureAnalysis, MetroRegistry};
use hbc_decomp::opcode::BytecodeFormat;

#[derive(Debug)]
struct LoadedFile {
    file: BytecodeFile,
    format: BytecodeFormat,
    #[allow(dead_code)]
    path: String,
}

#[derive(Debug)]
pub struct HermesService {
    loaded: Mutex<Option<LoadedFile>>,
    tool_router: ToolRouter<Self>,
}

impl HermesService {
    pub fn new() -> Self {
        Self {
            loaded: Mutex::new(None),
            tool_router: Self::tool_router(),
        }
    }

    fn with_file<F, T>(&self, f: F) -> Result<T, McpError>
    where
        F: FnOnce(&BytecodeFile, &BytecodeFormat) -> Result<T, McpError>,
    {
        let guard = self.loaded.lock().map_err(|e| McpError::internal_error(format!("lock: {e}"), None))?;
        let loaded = guard.as_ref().ok_or_else(|| McpError::invalid_params("No file loaded. Use load_file first.", None))?;
        f(&loaded.file, &loaded.format)
    }

    fn default_options() -> DecompileOptionsV2 {
        DecompileOptionsV2 {
            resolve_strings: true,
            include_offsets: false,
            propagate: true,
            simplify: true,
            recover_structures: true,
        }
    }

    fn build_registry(file: &BytecodeFile, format: &BytecodeFormat) -> Result<MetroRegistry, McpError> {
        let options = IRBuilderOptions { resolve_strings: true, include_offsets: false };
        let mut builder = IRBuilder::new(file, format, options);
        let cfg = builder.build_function(0)
            .map_err(|e| McpError::internal_error(format!("IR build error: {e}"), None))?;
        let analysis = StructureAnalysis::analyze(&cfg);
        let statements = analysis.root.to_statements(&cfg);
        Ok(MetroRegistry::analyze(&statements))
    }
}

// --- Parameter types ---

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LoadFileParams {
    #[schemars(description = "Absolute path to the .hbc file")]
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FunctionIdParams {
    #[schemars(description = "Function ID (0-based index)")]
    pub function_id: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DecompileFunctionParams {
    #[schemars(description = "Function ID (0-based index)")]
    pub function_id: u32,
    #[schemars(description = "Include bytecode offsets as comments")]
    #[serde(default)]
    pub show_offsets: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct XrefParams {
    #[schemars(description = "String to search for in the bytecode")]
    pub query: String,
    #[schemars(description = "Type of query: 'string' or 'function' (default: 'string')")]
    #[serde(default = "default_string")]
    pub kind: String,
}

fn default_string() -> String { "string".to_string() }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ModuleDepsParams {
    #[schemars(description = "Metro module ID")]
    pub module_id: u32,
    #[schemars(description = "Dependency tree depth (default: 2)")]
    #[serde(default = "default_depth")]
    pub depth: usize,
}

fn default_depth() -> usize { 2 }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListModulesParams {
    #[schemars(description = "Maximum number of modules to return")]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DumpParams {
    #[schemars(description = "What to dump: 'strings' or 'functions'")]
    #[serde(default = "default_strings")]
    pub kind: String,
}

fn default_strings() -> String { "strings".to_string() }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DisasmParams {
    #[schemars(description = "Function ID (0-based index)")]
    pub function_id: u32,
    #[schemars(description = "Show bytecode offsets")]
    #[serde(default)]
    pub show_offsets: bool,
}

// --- Tool implementations ---

#[tool_router(router = tool_router)]
impl HermesService {
    #[tool(description = "Load a Hermes bytecode (.hbc) file for analysis. Must be called before any other tool.")]
    fn load_file(&self, Parameters(params): Parameters<LoadFileParams>) -> Result<CallToolResult, McpError> {
        let bytes = std::fs::read(&params.path)
            .map_err(|e| McpError::internal_error(format!("Failed to read file: {e}"), None))?;
        let file = BytecodeFile::parse_auto(&bytes)
            .map_err(|e| McpError::internal_error(format!("Failed to parse HBC: {e}"), None))?;
        let (format, _) = BytecodeFormat::for_version_or_latest(file.header.version)
            .map_err(|e| McpError::internal_error(format!("Unsupported version: {e}"), None))?;

        let info = format!(
            "Loaded: {}\nVersion: {}\nFunctions: {}\nStrings: {}",
            params.path, file.header.version, file.header.function_count, file.header.string_count,
        );

        let mut guard = self.loaded.lock().map_err(|e| McpError::internal_error(format!("{e}"), None))?;
        *guard = Some(LoadedFile { file, format, path: params.path });
        Ok(CallToolResult::success(vec![Content::text(info)]))
    }

    #[tool(description = "Get file header info: version, function count, string count.")]
    fn file_info(&self) -> Result<CallToolResult, McpError> {
        self.with_file(|file, _| {
            let info = format!(
                "Version: {}\nFunctions: {}\nStrings: {}\nGlobal code: function 0",
                file.header.version, file.header.function_count, file.header.string_count,
            );
            Ok(CallToolResult::success(vec![Content::text(info)]))
        })
    }

    #[tool(description = "Decompile a function to readable JavaScript with full structure recovery (if/while/for).")]
    fn decompile_function(&self, Parameters(params): Parameters<DecompileFunctionParams>) -> Result<CallToolResult, McpError> {
        self.with_file(|file, format| {
            let mut opts = Self::default_options();
            opts.include_offsets = params.show_offsets;
            let decomp = Decompiler::from_parts(file.clone(), format.clone());
            let code = decomp.decompile_function(params.function_id, &opts)
                .map_err(|e| McpError::internal_error(format!("{e}"), None))?;
            Ok(CallToolResult::success(vec![Content::text(code)]))
        })
    }

    #[tool(description = "Decompile all functions in the file to JavaScript.")]
    fn decompile_all(&self) -> Result<CallToolResult, McpError> {
        self.with_file(|file, format| {
            let opts = Self::default_options();
            let decomp = Decompiler::from_parts(file.clone(), format.clone());
            let code = decomp.decompile_all(&opts)
                .map_err(|e| McpError::internal_error(format!("{e}"), None))?;
            Ok(CallToolResult::success(vec![Content::text(code)]))
        })
    }

    #[tool(description = "Get structured JSON IR of a function. Useful for programmatic analysis.")]
    fn get_ir_json(&self, Parameters(params): Parameters<FunctionIdParams>) -> Result<CallToolResult, McpError> {
        self.with_file(|file, format| {
            let opts = Self::default_options();
            let decomp = Decompiler::from_parts(file.clone(), format.clone());
            let ir = decomp.decompile_to_ir(params.function_id, &opts)
                .map_err(|e| McpError::internal_error(format!("{e}"), None))?;
            let json = serde_json::to_string_pretty(&ir)
                .map_err(|e| McpError::internal_error(format!("{e}"), None))?;
            Ok(CallToolResult::success(vec![Content::text(json)]))
        })
    }

    #[tool(description = "Disassemble a function to raw Hermes bytecode instructions.")]
    fn disassemble(&self, Parameters(params): Parameters<DisasmParams>) -> Result<CallToolResult, McpError> {
        self.with_file(|file, format| {
            let options = hbc_decomp::DisasmOptions {
                show_offsets: params.show_offsets,
                show_labels: true,
                resolve_strings: true,
                enable_color: false,
            };
            let asm = hbc_decomp::disassemble_function(file, format, params.function_id, &options)
                .map_err(|e| McpError::internal_error(format!("{e}"), None))?;
            Ok(CallToolResult::success(vec![Content::text(asm)]))
        })
    }

    #[tool(description = "Search for cross-references to a string or function ID in the bytecode.")]
    fn xref_search(&self, Parameters(params): Parameters<XrefParams>) -> Result<CallToolResult, McpError> {
        self.with_file(|file, format| {
            let results = if params.kind == "function" {
                let fid = params.query.parse::<u32>()
                    .map_err(|_| McpError::invalid_params("Invalid function ID", None))?;
                hbc_decomp::analysis::find_function_refs(file, format, fid)
            } else {
                hbc_decomp::analysis::find_string_xrefs(file, format, &params.query)
            };

            let mut output = format!("Found {} cross-references for '{}':\n", results.len(), params.query);
            for xref in &results {
                let name = file.string_at(file.function_headers[xref.function_id as usize].function_name())
                    .map(|e| e.value.as_str()).unwrap_or("<anonymous>");
                output.push_str(&format!("  Function {} ({}) at offset {:04x}: {}\n",
                    xref.function_id, name, xref.offset, xref.opcode));
            }
            Ok(CallToolResult::success(vec![Content::text(output)]))
        })
    }

    #[tool(description = "List all Metro modules in the React Native bundle.")]
    fn list_modules(&self, Parameters(params): Parameters<ListModulesParams>) -> Result<CallToolResult, McpError> {
        self.with_file(|file, format| {
            let registry = Self::build_registry(file, format)?;
            let mut modules: Vec<_> = registry.modules.values().collect();
            modules.sort_by_key(|m| m.module_id);
            let limit = params.limit.unwrap_or(modules.len()).min(modules.len());

            let mut output = format!("Found {} Metro modules:\n", modules.len());
            for m in modules.iter().take(limit) {
                let name_str = m.name.as_deref().map(|n| format!(" - {n}")).unwrap_or_default();
                output.push_str(&format!("  Module {} (F{}){} deps: {:?}\n",
                    m.module_id, m.function_id, name_str, m.dependencies));
            }
            Ok(CallToolResult::success(vec![Content::text(output)]))
        })
    }

    #[tool(description = "Show dependency tree for a Metro module.")]
    fn module_deps(&self, Parameters(params): Parameters<ModuleDepsParams>) -> Result<CallToolResult, McpError> {
        self.with_file(|file, format| {
            let registry = Self::build_registry(file, format)?;
            let tree = registry.get_dependency_tree(params.module_id, params.depth);
            Ok(CallToolResult::success(vec![Content::text(tree.format(0))]))
        })
    }

    #[tool(description = "Dump strings or function headers from the HBC file. Useful for finding API keys, endpoints, secrets.")]
    fn dump(&self, Parameters(params): Parameters<DumpParams>) -> Result<CallToolResult, McpError> {
        self.with_file(|file, _| {
            let mut output = String::new();
            if params.kind == "functions" {
                for (i, fh) in file.function_headers.iter().enumerate() {
                    let name = file.string_at(fh.function_name())
                        .map(|e| e.value.clone()).unwrap_or_default();
                    output.push_str(&format!("Function {}: name=\"{}\" params={} regs={} size={}\n",
                        i, name, fh.param_count(), fh.frame_size(), fh.bytecode_size_in_bytes()));
                }
            } else {
                for i in 0..file.header.string_count {
                    if let Some(s) = file.string_at(i) {
                        output.push_str(&format!("{}: {}\n", i, s.value));
                    }
                }
            }
            Ok(CallToolResult::success(vec![Content::text(output)]))
        })
    }

    #[tool(description = "List supported Hermes bytecode versions.")]
    fn list_versions(&self) -> Result<CallToolResult, McpError> {
        let versions = hbc_decomp::opcode::available_versions();
        let list = versions.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ");
        Ok(CallToolResult::success(vec![Content::text(format!("Supported versions: {list}"))]))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for HermesService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Hermes bytecode decompiler. Load a .hbc file with load_file, then use decompile/disassemble/xref tools to analyze React Native apps.".into()
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
