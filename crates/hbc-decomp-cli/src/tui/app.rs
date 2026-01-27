use std::collections::HashMap;
use ratatui::text::Text;

use hbc_decomp::{BytecodeFile, BytecodeFormat, DecompileOptions, decompile_function};

use super::formatting::{format_disasm_colored, format_info};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Disasm,
    Decompile,
    Info,
    Diff,
}

impl ViewMode {
    pub fn next(self, has_diff: bool) -> Self {
        match self {
            ViewMode::Disasm => ViewMode::Decompile,
            ViewMode::Decompile => ViewMode::Info,
            ViewMode::Info => if has_diff { ViewMode::Diff } else { ViewMode::Disasm },
            ViewMode::Diff => ViewMode::Disasm,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            ViewMode::Disasm => "Disasm",
            ViewMode::Decompile => "Decompile",
            ViewMode::Info => "Info",
            ViewMode::Diff => "BinDiff (Split View)",
        }
    }
}

pub struct App {
    pub file: BytecodeFile,
    pub format: BytecodeFormat,
    pub path: String,
    
    // Second file for diffing
    pub file2: Option<BytecodeFile>,
    pub format2: Option<BytecodeFormat>,
    pub path2: Option<String>,
    pub map2: HashMap<String, u32>,
    
    pub function_names: Vec<String>,
    pub selected: usize,
    pub scroll: u16,
    pub view: ViewMode,
    
    // Track what kind of diff to show (based on previous view)
    pub diff_kind: ViewMode, // Disasm or Decompile

    pub disasm_cache: HashMap<usize, Text<'static>>,
    pub decompile_cache: HashMap<usize, String>,
    
    // Caches for file 2
    pub disasm_cache2: HashMap<usize, Text<'static>>,
    pub decompile_cache2: HashMap<usize, String>,
}

impl App {
    pub fn new(
        file: BytecodeFile, format: BytecodeFormat, path: String,
        diff_target: Option<(BytecodeFile, BytecodeFormat, String)>
    ) -> Self {
        let mut function_names = Vec::new();
        for header in &file.function_headers {
            let name = file
                .string_at(header.function_name())
                .map(|entry| entry.value.clone())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| format!("f{}", header.function_id()));
            function_names.push(name);
        }

        let (file2, format2, path2, map2) = if let Some((f2, fmt2, p2)) = diff_target {
            let mut m = HashMap::new();
            for header in &f2.function_headers {
                let name = f2
                    .string_at(header.function_name())
                    .map(|entry| entry.value.clone())
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| format!("f{}", header.function_id()));
                m.insert(name, header.function_id());
            }
            (Some(f2), Some(fmt2), Some(p2), m)
        } else {
            (None, None, None, HashMap::new())
        };

        Self {
            file,
            format,
            path,
            file2,
            format2,
            path2,
            map2,
            function_names,
            selected: 0,
            scroll: 0,
            view: ViewMode::Disasm,
            diff_kind: ViewMode::Decompile, // Default diff mode
            disasm_cache: HashMap::new(),
            decompile_cache: HashMap::new(),
            disasm_cache2: HashMap::new(),
            decompile_cache2: HashMap::new(),
        }
    }

    pub fn selected_function_id(&self) -> u32 {
        self.selected as u32
    }
    
    pub fn selected_function_name(&self) -> &str {
        &self.function_names[self.selected]
    }

    pub fn set_selected(&mut self, index: usize) {
        if self.selected != index {
            self.selected = index;
            self.scroll = 0;
            // Clear caches to save memory? Or keep them.
        }
    }

    pub fn set_view(&mut self, view: ViewMode) {
        if self.view != view {
            // If switching TO Diff, check what we are coming from to set diff_kind
            if view == ViewMode::Diff {
                if self.view == ViewMode::Disasm {
                    self.diff_kind = ViewMode::Disasm;
                } else {
                    self.diff_kind = ViewMode::Decompile;
                }
            }
            self.view = view;
            self.scroll = 0;
        }
    }
    
    pub fn toggle_diff_kind(&mut self) {
        if self.view == ViewMode::Diff {
            self.diff_kind = match self.diff_kind {
                ViewMode::Disasm => ViewMode::Decompile,
                _ => ViewMode::Disasm,
            };
            self.scroll = 0;
        }
    }

    pub fn content(&mut self) -> (Text<'static>, Option<Text<'static>>) {
        match self.view {
            ViewMode::Info => (Text::raw(self.format_info_wrapper()), None),
            ViewMode::Disasm => (self.disasm_content(false), None),
            ViewMode::Decompile => (Text::raw(self.decompile_content(false)), None),
            ViewMode::Diff => {
                let left = match self.diff_kind {
                    ViewMode::Disasm => self.disasm_content(false),
                    _ => Text::raw(self.decompile_content(false)),
                };
                
                let right = if self.file2.is_some() {
                    let name = self.selected_function_name().to_string(); // Clone name
                    if let Some(id2) = self.map2.get(&name) {
                         match self.diff_kind {
                            ViewMode::Disasm => Some(self.disasm_content2(*id2)),
                            _ => Some(Text::raw(self.decompile_content2(*id2))),
                        }
                    } else {
                        Some(Text::raw("Function removed or renamed in file 2."))
                    }
                } else {
                    Some(Text::raw("No second file loaded."))
                };
                
                (left, right)
            },
        }
    }

    // --- Content Generators for File 1 ---

    pub fn disasm_content(&mut self, _force: bool) -> Text<'static> {
        if let Some(content) = self.disasm_cache.get(&self.selected) {
            return content.clone();
        }

        let function_id = self.selected_function_id();
        let content = match self.file.decode_function_instructions(&self.format, function_id) {
            Ok(instructions) => format_disasm_colored(&instructions, &self.format, &self.file),
            Err(e) => Text::raw(format!("Error: {e}")),
        };

        self.disasm_cache.insert(self.selected, content.clone());
        content
    }
    
    pub fn decompile_content(&mut self, _force: bool) -> String {
        if let Some(content) = self.decompile_cache.get(&self.selected) {
            return content.clone();
        }
        let options = DecompileOptions {
            show_offsets: false,
            show_labels: true,
            resolve_strings: true,
        };
        let content = decompile_function(
            &self.file,
            &self.format,
            self.selected_function_id(),
            &options,
        )
        .unwrap_or_else(|err| format!("error: {err}"));
        
        self.decompile_cache.insert(self.selected, content.clone());
        content
    }

    // --- Content Generators for File 2 ---
    
    pub fn disasm_content2(&mut self, function_id: u32) -> Text<'static> {
        if let Some(content) = self.disasm_cache2.get(&(function_id as usize)) {
            return content.clone();
        }

        let file2 = self.file2.as_ref().unwrap();
        let format2 = self.format2.as_ref().unwrap();

        let content = match file2.decode_function_instructions(format2, function_id) {
            Ok(instructions) => format_disasm_colored(&instructions, format2, file2),
            Err(e) => Text::raw(format!("Error: {e}")),
        };

        self.disasm_cache2.insert(function_id as usize, content.clone());
        content
    }

    pub fn decompile_content2(&mut self, function_id: u32) -> String {
        if let Some(content) = self.decompile_cache2.get(&(function_id as usize)) {
            return content.clone();
        }
        
        let file2 = self.file2.as_ref().unwrap();
        let format2 = self.format2.as_ref().unwrap();
        
        let options = DecompileOptions {
            show_offsets: false,
            show_labels: true,
            resolve_strings: true,
        };
        let content = decompile_function(
            file2,
            format2,
            function_id,
            &options,
        )
        .unwrap_or_else(|err| format!("error: {err}"));
        
        self.decompile_cache2.insert(function_id as usize, content.clone());
        content
    }

    pub fn format_info_wrapper(&self) -> String {
        format_info(
            &self.file,
            &self.path,
            &self.file2,
            &self.path2,
            self.selected,
            &self.function_names,
            &self.map2
        )
    }
}
