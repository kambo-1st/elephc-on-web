//! Purpose:
//! Emits wasm32-web array_map lowering for value-cell array locals.
//! Keeps int, string, bool, and scalar predicate value-cell loops separate from literal map setup.
//!
//! Called from:
//! - `super::array_map_assign` while lowering array_map assignments over value-cell locals.
//!
//! Key details:
//! - Preserves runtime-length value-cell metadata and homogeneous runtime value-kind checks.

use super::*;

pub(super) fn emit_array_map_value_int_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_map_runtime_value_int_local_assign(
            name,
            source,
            source_span,
            callback,
            module,
        );
    };
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_payload_i64");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("i64.store");
    }
    Ok(())
}

pub(super) fn emit_array_map_runtime_value_int_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || !matches!(
            module.array_runtime_value_cell_kind(source),
            Some(ValueCellKind::Int | ValueCellKind::Bool)
        )
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() int callbacks over runtime-length arrays require integer or bool value-cell storage",
        ));
    }
    let index = module.next_label("array_map_value_int_index");
    let done_label = module.next_label("array_map_value_int_done");
    let loop_label = module.next_label("array_map_value_int_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_i64");
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_map_value_floats_as_ints_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_map_runtime_value_floats_as_ints_local_assign(
            name,
            source,
            source_span,
            callback,
            module,
        );
    };
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_payload_f64");
        module.body().line("i64.trunc_f64_s");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("i64.store");
    }
    Ok(())
}

pub(super) fn emit_array_map_runtime_value_floats_as_ints_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Float)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() int callbacks over runtime-length float arrays require float value-cell storage",
        ));
    }
    let index = module.next_label("array_map_value_float_int_index");
    let done_label = module.next_label("array_map_value_float_int_done");
    let loop_label = module.next_label("array_map_value_float_int_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_f64");
    module.body().line("i64.trunc_f64_s");
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_map_value_numeric_strings_as_ints_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_map_runtime_value_numeric_strings_as_ints_local_assign(
            name,
            source,
            source_span,
            callback,
            module,
        );
    };
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 8");
        module.body().line("call $__rt_value_payload_i32");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 12");
        module.body().line("call $__rt_value_payload_i32");
        emit_stack_string_numeric_int_arg("array_map_value_string_int", module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("i64.store");
    }
    Ok(())
}

pub(super) fn emit_array_map_runtime_value_numeric_strings_as_ints_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Str)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() int callbacks over runtime-length string arrays require string value-cell storage",
        ));
    }
    let index = module.next_label("array_map_value_string_int_index");
    let done_label = module.next_label("array_map_value_string_int_done");
    let loop_label = module.next_label("array_map_value_string_int_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    emit_stack_string_numeric_int_arg("array_map_runtime_value_string_int", module);
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_map_value_int_bools_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_map_runtime_value_int_bools_local_assign(
            name,
            source,
            source_span,
            callback,
            module,
        );
    };
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Bool; len]));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_payload_i64");
        emit_array_map_int_bool_result(callback, module);
        module.body().line("call $__rt_value_store_bool");
    }
    Ok(())
}

pub(super) fn emit_array_map_runtime_value_int_bools_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Int)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() over runtime-length integer arrays requires integer value-cell storage",
        ));
    }
    let index = module.next_label("array_map_value_int_bool_index");
    let cell = module.next_label("array_map_value_int_bool_cell");
    let done_label = module.next_label("array_map_value_int_bool_done");
    let loop_label = module.next_label("array_map_value_int_bool_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Bool));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(name, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_i64");
    emit_array_map_int_bool_result(callback, module);
    module.body().line("call $__rt_value_store_bool");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_map_value_string_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_map_runtime_value_string_local_assign(
            name,
            source,
            source_span,
            callback,
            module,
        );
    };
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Str; len]));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 8");
        module.body().line("call $__rt_value_payload_i32");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 12");
        module.body().line("call $__rt_value_payload_i32");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("call $__rt_value_store_string");
    }
    Ok(())
}

pub(super) fn emit_array_map_value_string_local_instance_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_map_runtime_value_string_local_instance_assign(
            name,
            source,
            source_span,
            callback,
            capture_local,
            module,
        );
    };
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Str; len]));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.get ${}", capture_local));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 8");
        module.body().line("call $__rt_value_payload_i32");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 12");
        module.body().line("call $__rt_value_payload_i32");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("call $__rt_value_store_string");
    }
    Ok(())
}

pub(super) fn emit_array_map_value_float_local_instance_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_map_runtime_value_float_local_instance_assign(
            name,
            source,
            source_span,
            callback,
            capture_local,
            module,
        );
    };
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Float; len]));
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Float));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.get ${}", capture_local));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_payload_f64");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("call $__rt_value_store_float");
    }
    Ok(())
}

pub(super) fn emit_array_map_value_float_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_map_runtime_value_float_local_assign(
            name,
            source,
            source_span,
            callback,
            module,
        );
    };
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Float; len]));
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Float));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_payload_f64");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("call $__rt_value_store_float");
    }
    Ok(())
}

pub(super) fn emit_array_map_value_string_bools_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_map_runtime_value_string_bools_local_assign(
            name,
            source,
            source_span,
            callback,
            module,
        );
    };
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Bool; len]));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 8");
        module.body().line("call $__rt_value_payload_i32");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 12");
        module.body().line("call $__rt_value_payload_i32");
        emit_array_map_string_bool_result(callback, module);
        module.body().line("call $__rt_value_store_bool");
    }
    Ok(())
}

pub(super) fn emit_array_map_value_scalar_type_bools_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_map_runtime_value_scalar_type_bools_local_assign(
            name,
            source,
            source_span,
            kind,
            module,
        );
    };
    emit_array_map_literal_scalar_type_bools_assign(name, len, module)
}

pub(super) fn emit_array_map_value_object_type_bools_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map(is_object) over object arrays requires known length metadata",
        ));
    };
    if module.array_object_classes(source).is_none() {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map(is_object) over object arrays requires object metadata",
        ));
    }
    emit_array_map_literal_scalar_type_bools_assign(name, len, module)
}

pub(super) fn emit_array_map_value_object_false_bools_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() false object predicates require known length metadata",
        ));
    };
    if module.array_object_classes(source).is_none() {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() false object predicates require object metadata",
        ));
    }
    emit_array_map_literal_const_bools_assign(name, len, false, module)
}

pub(super) fn emit_array_map_value_object_class_names_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map(get_class) over object arrays requires known length metadata",
        ));
    };
    let Some(classes) = module.array_object_classes(source).map(|classes| classes.to_vec()) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map(get_class) over object arrays requires object metadata",
        ));
    };
    if classes.len() != len || classes.iter().any(Option::is_none) {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map(get_class) over object arrays requires exact class metadata",
        ));
    }
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Str; len]));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    for (index, class_name) in classes.iter().enumerate() {
        let class_name = class_name.as_ref().expect("checked exact object metadata");
        let (ptr, string_len) = module.intern_string(class_name);
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", string_len));
        module.body().line("call $__rt_value_store_string");
    }
    Ok(())
}

pub(super) fn emit_array_map_value_object_type_names_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map(gettype) over object arrays requires known length metadata",
        ));
    };
    if module.array_object_classes(source).is_none() {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map(gettype) over object arrays requires object metadata",
        ));
    }
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Str; len]));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    let (ptr, string_len) = module.intern_string("object");
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", string_len));
        module.body().line("call $__rt_value_store_string");
    }
    Ok(())
}

pub(super) fn emit_array_map_value_scalar_predicate_bools_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_map_runtime_value_scalar_predicate_bools_local_assign(
            name,
            source,
            source_span,
            callback,
            kind,
            module,
        );
    };
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Bool; len]));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        emit_array_filter_scalar_predicate_arg_from_cell(source, index, callback, kind, module);
        module.body().line("call $__rt_value_store_bool");
    }
    Ok(())
}

pub(super) fn emit_array_map_runtime_value_scalar_type_bools_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(kind)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() with a type predicate over runtime-length scalar arrays requires homogeneous value-cell storage",
        ));
    }
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Bool));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set ${}_len", name));
    let index = module.next_label("array_map_scalar_type_index");
    let done_label = module.next_label("array_map_scalar_type_done");
    let loop_label = module.next_label("array_map_scalar_type_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module
        .body()
        .line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i32.const 1");
    module.body().line("call $__rt_value_store_bool");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_map_value_bool_local_instance_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_map_runtime_value_bool_local_instance_assign(
            name,
            source,
            source_span,
            callback,
            capture_local,
            module,
        );
    };
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Bool; len]));
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Bool));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.get ${}", capture_local));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_payload_i64");
        module.body().line("i64.const 0");
        module.body().line("i64.ne");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("call $__rt_value_store_bool");
    }
    Ok(())
}

fn emit_array_map_runtime_value_bool_local_instance_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Bool)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() over runtime-length bool arrays requires bool value-cell storage",
        ));
    }
    let index = module.next_label("array_map_bool_instance_index");
    let cell = module.next_label("array_map_bool_instance_cell");
    let done_label = module.next_label("array_map_bool_instance_done");
    let loop_label = module.next_label("array_map_bool_instance_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Bool));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(name, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_i64");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("call $__rt_value_store_bool");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_map_runtime_value_scalar_predicate_bools_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(kind)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() with a scalar callback over runtime-length scalar arrays requires homogeneous value-cell storage",
        ));
    }
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Bool));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set ${}_len", name));
    let index = module.next_label("array_map_scalar_predicate_index");
    let cell = module.next_label("array_map_scalar_predicate_cell");
    let done_label = module.next_label("array_map_scalar_predicate_done");
    let loop_label = module.next_label("array_map_scalar_predicate_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(name, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    emit_array_filter_scalar_predicate_arg_from_cell_dynamic(source, &index, callback, kind, module);
    module.body().line("call $__rt_value_store_bool");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_map_runtime_value_string_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Str)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() over runtime-length string arrays requires string value-cell storage",
        ));
    }
    let index = module.next_label("array_map_string_index");
    let cell = module.next_label("array_map_string_cell");
    let done_label = module.next_label("array_map_string_done");
    let loop_label = module.next_label("array_map_string_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Str));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(name, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("call $__rt_value_store_string");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_array_map_runtime_value_string_local_instance_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Str)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() over runtime-length string arrays requires string value-cell storage",
        ));
    }
    let index = module.next_label("array_map_string_instance_index");
    let cell = module.next_label("array_map_string_instance_cell");
    let done_label = module.next_label("array_map_string_instance_done");
    let loop_label = module.next_label("array_map_string_instance_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Str));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(name, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("call $__rt_value_store_string");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_array_map_runtime_value_float_local_instance_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Float)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() over runtime-length float arrays requires float value-cell storage",
        ));
    }
    let index = module.next_label("array_map_float_instance_index");
    let cell = module.next_label("array_map_float_instance_cell");
    let done_label = module.next_label("array_map_float_instance_done");
    let loop_label = module.next_label("array_map_float_instance_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Float));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(name, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_f64");
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("call $__rt_value_store_float");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_array_map_runtime_value_float_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Float)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() over runtime-length float arrays requires float value-cell storage",
        ));
    }
    let index = module.next_label("array_map_float_index");
    let cell = module.next_label("array_map_float_cell");
    let done_label = module.next_label("array_map_float_done");
    let loop_label = module.next_label("array_map_float_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Float));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(name, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_f64");
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("call $__rt_value_store_float");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_map_runtime_value_string_bools_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Str)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() over runtime-length string arrays requires string value-cell storage",
        ));
    }
    let index = module.next_label("array_map_string_bool_index");
    let cell = module.next_label("array_map_string_bool_cell");
    let done_label = module.next_label("array_map_string_bool_done");
    let loop_label = module.next_label("array_map_string_bool_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Bool));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(name, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    emit_array_map_string_bool_result(callback, module);
    module.body().line("call $__rt_value_store_bool");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}
