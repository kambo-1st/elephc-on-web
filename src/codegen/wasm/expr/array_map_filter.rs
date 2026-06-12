//! Purpose:
//! Emits wasm32-web array_filter lowering loops and truthiness helpers.
//! Keeps filter-specific result construction separate from array_map emitters.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` and array_filter assignment lowering.
//!
//! Key details:
//! - Preserves value-cell metadata, callback argument modes, and PHP truthiness behavior.

use super::*;
use super::array_filter_runtime_scalars::*;
use super::array_filter_truthiness::emit_value_cell_strlen_truthiness;
use super::array_map_filter_value_strings::*;

pub(super) fn emit_array_filter_value_string_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_filter_runtime_value_string_local_assign(
            name,
            source,
            source_span,
            callback,
            module,
        );
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Str; len]));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 8");
        module.body().line("call $__rt_value_payload_i32");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 12");
        module.body().line("call $__rt_value_payload_i32");
        emit_array_filter_string_predicate(callback, module);
        module.body().open("if");
        emit_array_filter_copy_value_entry(name, source, &out_index, index, module);
        module.body().close("end");
    }
    Ok(())
}

pub(super) fn emit_array_filter_value_string_local_instance_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_filter_runtime_value_string_local_instance_assign(
            name,
            source,
            source_span,
            callback,
            capture_local,
            module,
        );
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Str; len]));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    for index in 0..len {
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
        emit_array_filter_predicate_result_is_true(callback, module);
        module.body().open("if");
        emit_array_filter_copy_value_entry(name, source, &out_index, index, module);
        module.body().close("end");
    }
    Ok(())
}

pub(super) fn emit_array_filter_value_float_local_instance_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_filter_runtime_value_float_local_instance_assign(
            name,
            source,
            source_span,
            callback,
            capture_local,
            module,
        );
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Float; len]));
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Float));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    for index in 0..len {
        module.body().line(&format!("local.get ${}", capture_local));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_payload_f64");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        emit_array_filter_predicate_result_is_true(callback, module);
        module.body().open("if");
        emit_array_filter_copy_value_entry(name, source, &out_index, index, module);
        module.body().close("end");
    }
    Ok(())
}

pub(super) fn emit_array_filter_value_bool_local_instance_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_filter_runtime_value_bool_local_instance_assign(
            name,
            source,
            source_span,
            callback,
            capture_local,
            module,
        );
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Bool; len]));
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Bool));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    for index in 0..len {
        module.body().line(&format!("local.get ${}", capture_local));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_payload_i64");
        module.body().line("i64.const 0");
        module.body().line("i64.ne");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        emit_array_filter_predicate_result_is_true(callback, module);
        module.body().open("if");
        emit_array_filter_copy_value_entry(name, source, &out_index, index, module);
        module.body().close("end");
    }
    Ok(())
}

fn emit_array_filter_runtime_value_float_local_instance_assign(
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
            "wasm32-web array_filter() over runtime-length float arrays requires float value-cell storage",
        ));
    }
    let out_index = emit_array_filter_runtime_result_prelude(name, source, module);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Float));
    let index = module.next_label("array_filter_float_instance_index");
    let done_label = module.next_label("array_filter_float_instance_done");
    let loop_label = module.next_label("array_filter_float_instance_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_f64");
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    emit_array_filter_predicate_result_is_true(callback, module);
    module.body().open("if");
    emit_array_filter_copy_value_entry_dynamic_key(name, source, &out_index, &index, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_array_filter_runtime_value_bool_local_instance_assign(
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
            "wasm32-web array_filter() over runtime-length bool arrays requires bool value-cell storage",
        ));
    }
    let out_index = emit_array_filter_runtime_result_prelude(name, source, module);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Bool));
    let index = module.next_label("array_filter_bool_instance_index");
    let done_label = module.next_label("array_filter_bool_instance_done");
    let loop_label = module.next_label("array_filter_bool_instance_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_i64");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    emit_array_filter_predicate_result_is_true(callback, module);
    module.body().open("if");
    emit_array_filter_copy_value_entry_dynamic_key(name, source, &out_index, &index, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_filter_value_scalar_type_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_filter_runtime_value_scalar_type_local_assign(
            name,
            source,
            source_span,
            kind,
            module,
        );
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_value_cell_kinds(name, Some(vec![kind; len]));
    module.set_array_runtime_value_cell_kind(name, Some(kind));
    if kind == ValueCellKind::Array {
        module.set_array_nested_value_metadata(
            name,
            module.array_nested_value_metadata_items(source).map(|items| items.to_vec()),
        );
        module.set_array_key_kinds(name, Some(vec![AssocKeyKind::Int; len]));
        module.set_array_key_values(
            name,
            Some(
                (0..len)
                    .map(|index| AssocKeyValue::Int(index as i64))
                    .collect(),
            ),
        );
    }
    for index in 0..len {
        emit_array_filter_copy_value_entry(name, source, &out_index, index, module);
    }
    Ok(())
}

pub(super) fn emit_array_filter_value_scalar_predicate_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_filter_runtime_value_scalar_predicate_local_assign(
            name,
            source,
            source_span,
            callback,
            kind,
            module,
        );
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_value_cell_kinds(name, Some(vec![kind; len]));
    module.set_array_runtime_value_cell_kind(name, Some(kind));
    for index in 0..len {
        emit_array_filter_scalar_predicate_arg_from_cell(source, index, callback, kind, module);
        module.body().open("if");
        emit_array_filter_copy_value_entry(name, source, &out_index, index, module);
        module.body().close("end");
    }
    Ok(())
}

pub(super) fn emit_array_filter_value_scalar_strlen_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_filter_runtime_value_scalar_strlen_local_assign(
            name,
            source,
            source_span,
            kind,
            module,
        );
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_value_cell_kinds(name, Some(vec![kind; len]));
    module.set_array_runtime_value_cell_kind(name, Some(kind));
    module.set_array_key_kinds(name, Some(vec![AssocKeyKind::Int; len]));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    module.set_array_key_values(
        name,
        Some(
            (0..len)
                .map(|index| AssocKeyValue::Int(index as i64))
                .collect(),
        ),
    );
    for index in 0..len {
        emit_value_cell_strlen_truthiness(source, index, kind, module);
        module.body().open("if");
        emit_array_filter_copy_value_entry(name, source, &out_index, index, module);
        module.body().close("end");
    }
    Ok(())
}
