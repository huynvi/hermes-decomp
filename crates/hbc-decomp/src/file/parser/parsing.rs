use crate::error::{Error, Result};
use crate::format::{
    FunctionHeaderLayout, HeaderLayout,
};
use crate::io::ByteReader;
use crate::file::BytecodeFile;
use crate::debug::try_parse_debug_info;

use super::header::*;
use super::function::*;
use super::table::*;

pub fn parse_auto(bytes: &[u8]) -> Result<BytecodeFile> {
    let version = peek_version(bytes)?;
    let legacy = parse_with_layout(bytes, HeaderLayout::Legacy, FunctionHeaderLayout::Legacy16).ok();
    let modern = parse_with_layout(bytes, HeaderLayout::Modern, FunctionHeaderLayout::Modern12).ok();

    match (legacy, modern) {
        (Some(file), None) => Ok(file),
        (None, Some(file)) => Ok(file),
        (Some(legacy_file), Some(modern_file)) => {
            if version >= MODERN_FUNCTION_HEADER_MIN_VERSION {
                Ok(modern_file)
            } else {
                Ok(legacy_file)
            }
        }
        (None, None) => Err(Error::Parse(
            "failed to parse bytecode file using known layouts".to_string(),
        )),
    }
}

pub fn parse_with_layout(
    bytes: &[u8],
    layout: HeaderLayout,
    function_layout: FunctionHeaderLayout,
) -> Result<BytecodeFile> {
    if bytes.len() < HEADER_SIZE {
        return Err(Error::Parse("file too small for header".to_string()));
    }
    let mut reader = ByteReader::new(bytes);
    let header_start = reader.position();

    let magic = reader.read_u64()?;
    if magic != MAGIC {
        return Err(Error::Parse(format!(
            "invalid magic header: expected {MAGIC:#x} got {magic:#x}"
        )));
    }
    let version = reader.read_u32()?;
    let source_hash = {
        let bytes = reader.read_bytes(20)?;
        let mut hash = [0u8; 20];
        hash.copy_from_slice(bytes);
        hash
    };

    let mut header = match layout {
        HeaderLayout::Legacy => parse_legacy_header(&mut reader, version, magic, source_hash)?,
        HeaderLayout::Modern => parse_modern_header(&mut reader, version, magic, source_hash)?,
    };
    header.function_header_layout = function_layout;

    reader.seek(header_start + HEADER_SIZE)?;
    reader.align(4)?;

    let function_headers = parse_function_headers(&mut reader, &header)?;
    reader.align(4)?;

    let string_kinds = parse_string_kinds(&mut reader, header.string_kind_count)?;
    reader.align(4)?;

    let identifier_hashes = parse_u32_vec(&mut reader, header.identifier_count)?;
    reader.align(4)?;

    let small_string_table = parse_u32_vec(&mut reader, header.string_count)?;
    reader.align(4)?;

    let overflow_string_table = parse_overflow_string_table(&mut reader, header.overflow_string_count)?;
    reader.align(4)?;

    let string_storage = reader.read_bytes(header.string_storage_size as usize)?.to_vec();
    reader.align(4)?;

    match header.layout {
        HeaderLayout::Legacy => {
            let mut big_int_table = Vec::new();
            let mut big_int_storage = Vec::new();
            if let (Some(count), Some(size)) = (header.big_int_count, header.big_int_storage_size) {
                big_int_table = parse_table_entries(&mut reader, count)?;
                reader.align(4)?;
                big_int_storage = reader.read_bytes(size as usize)?.to_vec();
                reader.align(4)?;
            }

            let array_buffer = if let Some(size) = header.array_buffer_size {
                let buffer = reader.read_bytes(size as usize)?.to_vec();
                reader.align(4)?;
                buffer
            } else {
                Vec::new()
            };
            let literal_value_buffer = Vec::new();
            let obj_key_buffer = reader.read_bytes(header.obj_key_buffer_size as usize)?.to_vec();
            reader.align(4)?;
            let obj_value_buffer = if let Some(size) = header.obj_value_buffer_size {
                let buffer = reader.read_bytes(size as usize)?.to_vec();
                reader.align(4)?;
                buffer
            } else {
                Vec::new()
            };
            let obj_shape_table = Vec::new();

            let reg_exp_table = parse_table_entries(&mut reader, header.reg_exp_count)?;
            reader.align(4)?;

            let reg_exp_storage = reader
                .read_bytes(header.reg_exp_storage_size as usize)?
                .to_vec();
            reader.align(4)?;

            let cjs_module_table = parse_pair_table(&mut reader, header.cjs_module_count)?;
            reader.align(4)?;

            let mut function_source_table = Vec::new();
            if let Some(count) = header.function_source_count {
                function_source_table = parse_pair_table(&mut reader, count)?;
                reader.align(4)?;
            }

            let instruction_offset = reader.position() as u32;
            let instructions = bytes[instruction_offset as usize..].to_vec();

            let strings = decode_string_table(
                header.string_count,
                &string_kinds,
                &small_string_table,
                &overflow_string_table,
                &string_storage,
            )?;

            let debug_info = try_parse_debug_info(bytes, header.debug_info_offset);

            Ok(BytecodeFile {
                header,
                function_headers,
                string_kinds,
                identifier_hashes,
                strings,
                big_int_table,
                big_int_storage,
                reg_exp_table,
                reg_exp_storage,
                array_buffer,
                literal_value_buffer,
                obj_key_buffer,
                obj_value_buffer,
                obj_shape_table,
                cjs_module_table,
                function_source_table,
                instruction_offset,
                instructions,
                debug_info,
            })
        }
        HeaderLayout::Modern => {
            let array_buffer = Vec::new();
            let literal_value_buffer = if let Some(size) = header.literal_value_buffer_size {
                let buffer = reader.read_bytes(size as usize)?.to_vec();
                reader.align(4)?;
                buffer
            } else {
                Vec::new()
            };

            let obj_key_buffer = reader.read_bytes(header.obj_key_buffer_size as usize)?.to_vec();
            reader.align(4)?;

            let obj_shape_table = if let Some(count) = header.obj_shape_table_count {
                let table = parse_shape_table(&mut reader, count)?;
                reader.align(4)?;
                table
            } else {
                Vec::new()
            };

            let obj_value_buffer = Vec::new();
            let mut big_int_table = Vec::new();
            let mut big_int_storage = Vec::new();
            if let (Some(count), Some(size)) = (header.big_int_count, header.big_int_storage_size) {
                big_int_table = parse_table_entries(&mut reader, count)?;
                reader.align(4)?;
                big_int_storage = reader.read_bytes(size as usize)?.to_vec();
                reader.align(4)?;
            }

            let reg_exp_table = parse_table_entries(&mut reader, header.reg_exp_count)?;
            reader.align(4)?;

            let reg_exp_storage = reader
                .read_bytes(header.reg_exp_storage_size as usize)?
                .to_vec();
            reader.align(4)?;

            let cjs_module_table = parse_pair_table(&mut reader, header.cjs_module_count)?;
            reader.align(4)?;

            let mut function_source_table = Vec::new();
            if let Some(count) = header.function_source_count {
                function_source_table = parse_pair_table(&mut reader, count)?;
                reader.align(4)?;
            }

            let instruction_offset = reader.position() as u32;
            let instructions = bytes[instruction_offset as usize..].to_vec();

            let strings = decode_string_table(
                header.string_count,
                &string_kinds,
                &small_string_table,
                &overflow_string_table,
                &string_storage,
            )?;

            let debug_info = try_parse_debug_info(bytes, header.debug_info_offset);

            Ok(BytecodeFile {
                header,
                function_headers,
                string_kinds,
                identifier_hashes,
                strings,
                big_int_table,
                big_int_storage,
                reg_exp_table,
                reg_exp_storage,
                array_buffer,
                literal_value_buffer,
                obj_key_buffer,
                obj_value_buffer,
                obj_shape_table,
                cjs_module_table,
                function_source_table,
                instruction_offset,
                instructions,
                debug_info,
            })
        }
    }
}
