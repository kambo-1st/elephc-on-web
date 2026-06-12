//! Purpose:
//! Emits nested metadata-backed json_encode() output paths for wasm32-web arrays.
//! Keeps recursive output traversal and value-cell JSON rendering out of the main json_encode emitter.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::json_encode` for known nested array output.
//!
//! Key details:
//! - Validates nested metadata before recursive output so unsupported layouts do not silently miscompile.
//! - Uses the same runtime string JSON escaping helpers as the main json_encode emitter.

use super::*;
use super::json_runtime_string::emit_runtime_json_encode_value_string_from_locals;

pub(super) fn json_value_kinds_need_missing_nested_metadata(
    kinds: &[ValueCellKind],
    nested_values: Option<&[Option<NestedArrayMetadata>]>,
) -> bool {
    kinds.iter().enumerate().any(|(index, kind)| {
        *kind == ValueCellKind::Array
            && nested_values
                .and_then(|items| items.get(index))
                .and_then(Option::as_ref)
                .is_none()
    })
}

pub(super) fn assoc_key_values_are_json_list(keys: &[AssocKeyValue]) -> bool {
    keys.iter()
        .enumerate()
        .all(|(index, key)| *key == AssocKeyValue::Int(index as i64))
}

pub(super) fn emit_output_json_value_cell_at_index(
    name: &str,
    index: usize,
    kind: ValueCellKind,
    nested: Option<&NestedArrayMetadata>,
    flags: i64,
    span: Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let cell = module.next_label("json_value_cell");
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
    emit_output_json_value_cell_with_nested(&cell, kind, nested, flags, span, module)
}

pub(super) fn emit_output_json_assoc_value_cell_at_index(
    name: &str,
    index: usize,
    kind: ValueCellKind,
    nested: Option<&NestedArrayMetadata>,
    flags: i64,
    span: Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let cell = module.next_label("json_assoc_value_cell");
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_ASSOC_ENTRY_SIZE + 16));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
    emit_output_json_value_cell_with_nested(&cell, kind, nested, flags, span, module)
}

fn emit_output_json_value_cell_with_nested(
    cell: &str,
    kind: ValueCellKind,
    nested: Option<&NestedArrayMetadata>,
    flags: i64,
    span: Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if kind != ValueCellKind::Array {
        emit_output_json_value_cell(cell, kind, flags, module);
        return Ok(());
    }
    let Some(nested) = nested else {
        return Err(CompileError::new(
            span,
            "wasm32-web json_encode() array value cells require nested metadata",
        ));
    };
    emit_output_json_nested_array_cell(cell, nested, flags, span, module)
}

fn emit_output_json_nested_array_cell(
    cell: &str,
    metadata: &NestedArrayMetadata,
    flags: i64,
    span: Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let ptr = module.next_label("json_nested_array_ptr");
    module.declare_i32_local(ptr.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", ptr));
    emit_output_json_array_from_metadata(&ptr, metadata, flags, span, module)
}

fn emit_output_json_array_from_metadata(
    ptr: &str,
    metadata: &NestedArrayMetadata,
    flags: i64,
    span: Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match metadata.layout {
        ArrayLayout::CompactInt => {
            emit_output_json_compact_int_from_metadata(ptr, metadata.len, flags, module);
            Ok(())
        }
        ArrayLayout::Value => {
            emit_output_json_value_array_from_metadata(ptr, metadata, flags, span, module)
        }
        ArrayLayout::Assoc => {
            emit_output_json_assoc_array_from_metadata(ptr, metadata, flags, span, module)
        }
    }
}

fn emit_output_json_compact_int_from_metadata(
    ptr: &str,
    len: usize,
    flags: i64,
    module: &mut WasmModule,
) {
    if flags & 16 != 0 {
        emit_write_literal_text("{", module);
    } else {
        emit_write_literal_text("[", module);
    }
    for index in 0..len {
        if index > 0 {
            emit_write_literal_text(",", module);
        }
        if flags & 16 != 0 {
            let key = json_quote(&index.to_string(), flags);
            emit_write_literal_text(&key, module);
            emit_write_literal_text(":", module);
        }
        module.body().line(&format!("local.get {}", ptr));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line("i64.load");
        module.body().line("call $host_write_int");
    }
    if flags & 16 != 0 {
        emit_write_literal_text("}", module);
    } else {
        emit_write_literal_text("]", module);
    }
}

fn emit_output_json_value_array_from_metadata(
    ptr: &str,
    metadata: &NestedArrayMetadata,
    flags: i64,
    span: Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(kinds) = metadata.value_kinds.as_ref() else {
        return Err(CompileError::new(
            span,
            "wasm32-web json_encode() nested value arrays require exact value metadata",
        ));
    };
    if kinds.len() != metadata.len
        || json_value_kinds_need_missing_nested_metadata(kinds, metadata.nested_values.as_deref())
    {
        return Err(CompileError::new(
            span,
            "wasm32-web json_encode() nested value arrays require known nested metadata",
        ));
    }
    if flags & 16 != 0 {
        emit_write_literal_text("{", module);
    } else {
        emit_write_literal_text("[", module);
    }
    for (index, kind) in kinds.iter().enumerate() {
        if index > 0 {
            emit_write_literal_text(",", module);
        }
        if flags & 16 != 0 {
            let key = json_quote(&index.to_string(), flags);
            emit_write_literal_text(&key, module);
            emit_write_literal_text(":", module);
        }
        let cell = module.next_label("json_nested_value_cell");
        module.declare_i32_local(cell.trim_start_matches('$').to_string());
        module.body().line(&format!("local.get {}", ptr));
        module
            .body()
            .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", cell));
        let nested = metadata
            .nested_values
            .as_ref()
            .and_then(|items| items.get(index))
            .and_then(Option::as_ref);
        emit_output_json_value_cell_with_nested(&cell, *kind, nested, flags, span, module)?;
    }
    if flags & 16 != 0 {
        emit_write_literal_text("}", module);
    } else {
        emit_write_literal_text("]", module);
    }
    Ok(())
}

fn emit_output_json_assoc_array_from_metadata(
    ptr: &str,
    metadata: &NestedArrayMetadata,
    flags: i64,
    span: Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(keys) = metadata.key_values.as_ref() else {
        return Err(CompileError::new(
            span,
            "wasm32-web json_encode() nested associative arrays require exact key metadata",
        ));
    };
    let Some(kinds) = metadata.value_kinds.as_ref() else {
        return Err(CompileError::new(
            span,
            "wasm32-web json_encode() nested associative arrays require exact value metadata",
        ));
    };
    if keys.len() != metadata.len
        || kinds.len() != metadata.len
        || json_value_kinds_need_missing_nested_metadata(kinds, metadata.nested_values.as_deref())
    {
        return Err(CompileError::new(
            span,
            "wasm32-web json_encode() nested associative arrays require known metadata",
        ));
    }
    let as_list = flags & 16 == 0 && assoc_key_values_are_json_list(keys);
    emit_write_literal_text(if as_list { "[" } else { "{" }, module);
    for index in 0..metadata.len {
        if index > 0 {
            emit_write_literal_text(",", module);
        }
        if !as_list {
            let key = match &keys[index] {
                AssocKeyValue::Int(value) => json_quote(&value.to_string(), flags),
                AssocKeyValue::Str(value) => json_quote(value, flags),
            };
            emit_write_literal_text(&key, module);
            emit_write_literal_text(":", module);
        }
        let cell = module.next_label("json_nested_assoc_value_cell");
        module.declare_i32_local(cell.trim_start_matches('$').to_string());
        module.body().line(&format!("local.get {}", ptr));
        module
            .body()
            .line(&format!("i32.const {}", index * WASM_ASSOC_ENTRY_SIZE + 16));
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", cell));
        let nested = metadata
            .nested_values
            .as_ref()
            .and_then(|items| items.get(index))
            .and_then(Option::as_ref);
        emit_output_json_value_cell_with_nested(&cell, kinds[index], nested, flags, span, module)?;
    }
    emit_write_literal_text(if as_list { "]" } else { "}" }, module);
    Ok(())
}

pub(super) fn emit_output_json_value_cell(
    cell: &str,
    kind: ValueCellKind,
    flags: i64,
    module: &mut WasmModule,
) {
    match kind {
        ValueCellKind::Int => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("call $host_write_int");
        }
        ValueCellKind::Float => {
            let value = module.next_label("json_float_value");
            module.declare_f64_local(value.trim_start_matches('$').to_string());
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("f64.load");
            module.body().line(&format!("local.tee {}", value));
            module.body().line("call $host_write_float");
            if flags & 1024 != 0 {
                module.body().line(&format!("local.get {}", value));
                module.body().line(&format!("local.get {}", value));
                module.body().line("f64.trunc");
                module.body().line("f64.eq");
                module.body().open("if");
                emit_write_literal_text(".0", module);
                module.body().close("end");
            }
        }
        ValueCellKind::Bool => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("i64.eqz");
            module.body().open("if");
            emit_write_literal_text("false", module);
            module.body().line("else");
            emit_write_literal_text("true", module);
            module.body().close("end");
        }
        ValueCellKind::Null => emit_write_literal_text("null", module),
        ValueCellKind::Str => {
            let ptr = module.next_label("json_string_ptr");
            let len = module.next_label("json_string_len");
            module.declare_i32_local(ptr.trim_start_matches('$').to_string());
            module.declare_i32_local(len.trim_start_matches('$').to_string());
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.set {}", ptr));
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.set {}", len));
            emit_runtime_json_encode_value_string_from_locals(&ptr, &len, flags, module);
        }
        ValueCellKind::Array => emit_write_literal_text("null", module),
    }
}
