use std::collections::HashMap;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};

use hbc_decomp::{
    collect_label_offsets, escape_js_string, BytecodeFile, BytecodeFormat,
    Instruction, Operand, OperandType, OperandValue,
};

pub fn format_disasm_colored(
    instructions: &[Instruction],
    format: &BytecodeFormat,
    file: &BytecodeFile,
) -> Text<'static> {
    let mut lines = Vec::new();
    let label_offsets = collect_label_offsets(instructions, format);

    for insn in instructions {
        if label_offsets.contains(&insn.offset) {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("L{}:", insn.offset),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
            ]));
        }

        let mut spans = Vec::new();

        // Offset
        spans.push(Span::styled(
            format!("{:04x}  ", insn.offset),
            Style::default().fg(Color::DarkGray),
        ));

        // Opcode
        let def = match format.definitions.get(insn.opcode as usize) {
            Some(d) => d,
            None => {
                lines.push(Line::from(vec![Span::raw(format!(
                    "<unknown opcode {}>",
                    insn.opcode
                ))]));
                continue;
            }
        };
        spans.push(Span::styled(def.name.clone(), Style::default().fg(Color::Blue)));
        spans.push(Span::raw(" "));

        // Operands
        for (i, operand) in insn.operands.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(", "));
            }
            spans.push(format_operand(insn, operand, file));
        }

        lines.push(Line::from(spans));
    }

    Text::from(lines)
}

fn format_operand(insn: &Instruction, operand: &Operand, file: &BytecodeFile) -> Span<'static> {
    match operand.ty {
        OperandType::Reg8 | OperandType::Reg32 => {
            let s = match operand.value {
                OperandValue::U8(v) => format!("r{v}"),
                OperandValue::U16(v) => format!("r{v}"),
                OperandValue::U32(v) => format!("r{v}"),
                _ => "r?".to_string(),
            };
            Span::styled(s, Style::default().fg(Color::Red))
        }
        OperandType::Addr8 | OperandType::Addr32 => {
            let s = match operand.value.as_i32() {
                Some(rel) => {
                    let target = insn.offset as i32 + rel;
                    format!("L{target}")
                }
                None => "L?".to_string(),
            };
            Span::styled(s, Style::default().fg(Color::Yellow))
        }
        OperandType::UInt8S | OperandType::UInt16S | OperandType::UInt32S => {
            if let Some(id) = operand.value.as_u32() {
                if let Some(entry) = file.string_at(id) {
                    let s = escape_js_string(&entry.value);
                    return Span::styled(format!("\"{s}\""), Style::default().fg(Color::Green));
                }
            }
            let s = format_operand_value(&operand.value);
            Span::styled(s, Style::default().fg(Color::Magenta))
        }
        _ => {
            let s = format_operand_value(&operand.value);
            Span::styled(s, Style::default().fg(Color::Magenta))
        }
    }
}

fn format_operand_value(value: &OperandValue) -> String {
    match value {
        OperandValue::U8(v) => v.to_string(),
        OperandValue::U16(v) => v.to_string(),
        OperandValue::U32(v) => v.to_string(),
        OperandValue::I8(v) => v.to_string(),
        OperandValue::I32(v) => v.to_string(),
        OperandValue::F64(v) => format!("{v}"),
    }
}

pub fn format_info(
    file: &BytecodeFile,
    path: &String,
    file2: &Option<BytecodeFile>,
    path2: &Option<String>,
    selected: usize,
    function_names: &[String],
    map2: &HashMap<String, u32>,
) -> String {
    let mut lines = Vec::new();
    lines.push(format!("File: {path}"));
    lines.push(format!("Version: {}", file.header.version));
    lines.push(format!("Header layout: {:?}", file.header.layout));
    lines.push(format!("Functions: {}", file.header.function_count));
    lines.push(format!("Strings: {}", file.header.string_count));

    if let Some(p2) = path2 {
        lines.push(format!("\nFile 2: {p2}"));
        if let Some(f2) = file2 {
            lines.push(format!("Version: {}", f2.header.version));
            lines.push(format!("Functions: {}", f2.header.function_count));
        }
    }
    lines.push("".to_string());

    if let Some(header) = file.function_headers.get(selected) {
        lines.push(format!("Current Function ({selected})"));
        lines.push(format!("ID: {}", header.function_id()));
        lines.push(format!("Name: {}", function_names[selected]));
        lines.push(format!(
            "Bytecode size: {}",
            header.bytecode_size_in_bytes()
        ));
        lines.push(format!("Frame size: {}", header.frame_size()));
        lines.push(format!("Flags: 0x{:02x}", header.flags()));

        if file2.is_some() {
            let name = &function_names[selected];
            if let Some(id2) = map2.get(name) {
                lines.push(format!("\nMatches in File 2: ID {id2}"));
                if let Some(f2) = file2 {
                    let h2 = &f2.function_headers[*id2 as usize];
                    lines.push(format!(
                        "Bytecode size: {}",
                        h2.bytecode_size_in_bytes()
                    ));
                    if h2.bytecode_size_in_bytes() != header.bytecode_size_in_bytes() {
                        lines.push("Status: MODIFIED (Size mismatch)".to_string());
                    } else {
                        lines.push(
                            "Status: POTENTIALLY IDENTICAL (Size match)".to_string(),
                        );
                    }
                }
            } else {
                lines.push("\nStatus: REMOVED in File 2".to_string());
            }
        }
    }

    lines.join("\n")
}
