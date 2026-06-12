//! Purpose:
//! Emits wasm32-web associative array_filter value-callback lowering.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_map_filter_assoc`.
//!
//! Key details:
//! - Handles default value callback mode for known-length and runtime associative arrays.
//! - Preserves keys and nested metadata while filtering by value predicate.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_array_filter_runtime_assoc_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayFilterCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_filter_runtime_assoc_local_assign_with_receiver(
        name,
        source,
        source_span,
        callback,
        None,
        shape,
        module,
    )
}

pub(in crate::codegen::wasm::expr) fn emit_array_filter_runtime_assoc_local_instance_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    receiver: &str,
    shape: ArrayFilterCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_filter_runtime_assoc_local_assign_with_receiver(
        name,
        source,
        source_span,
        callback,
        Some(receiver),
        shape,
        module,
    )
}

fn emit_array_filter_runtime_assoc_local_assign_with_receiver(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    receiver: Option<&str>,
    shape: ArrayFilterCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(value_kind) = module.array_runtime_value_cell_kind(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter() over runtime associative arrays requires homogeneous value metadata",
        ));
    };
    let string_to_float_predicate = shape == ArrayFilterCallbackShape::Float
        && !array_filter_type_predicate_callback(callback)
        && value_kind == ValueCellKind::Str;
    if !array_filter_value_kind_matches_shape(value_kind, shape) && !string_to_float_predicate {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter() over runtime associative arrays requires callback-compatible values",
        ));
    }
    let out_index = module.next_label("array_filter_assoc_runtime_out_index");
    let index = module.next_label("array_filter_assoc_runtime_index");
    let source_entry = module.next_label("array_filter_assoc_runtime_source_entry");
    let target_entry = module.next_label("array_filter_assoc_runtime_target_entry");
    let value_cell = module.next_label("array_filter_assoc_runtime_value_cell");
    for local in [&out_index, &index, &source_entry, &target_entry, &value_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.clear_array_length(name);
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    module.set_array_runtime_key_kind(name, module.array_runtime_key_kind(source));
    module.set_array_php_normalized_runtime_keys(
        name,
        module.array_has_php_normalized_runtime_keys(source),
    );
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(value_kind));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    let loop_label = module.next_label("array_filter_assoc_runtime_loop");
    let done_label = module.next_label("array_filter_assoc_runtime_done");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", source_entry));
    emit_assoc_value_cell_address(&source_entry, &value_cell, module);
    emit_array_filter_optional_receiver_arg(receiver, module);
    match shape {
        ArrayFilterCallbackShape::Int => {
            module.body().line(&format!("local.get {}", value_cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.load");
            emit_array_filter_int_predicate(callback, module);
        }
        ArrayFilterCallbackShape::Str => {
            module.body().line(&format!("local.get {}", value_cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.get {}", value_cell));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            emit_array_filter_string_predicate(callback, module);
        }
        ArrayFilterCallbackShape::Bool if !array_filter_type_predicate_callback(callback) => {
            emit_array_filter_scalar_predicate_arg_from_pointer(
                &value_cell,
                callback,
                ValueCellKind::Bool,
                module,
            );
        }
        ArrayFilterCallbackShape::Float if !array_filter_type_predicate_callback(callback) => {
            emit_array_filter_scalar_predicate_arg_from_pointer(
                &value_cell,
                callback,
                if value_kind == ValueCellKind::Str {
                    ValueCellKind::Str
                } else {
                    ValueCellKind::Float
                },
                module,
            );
        }
        ArrayFilterCallbackShape::Bool
        | ArrayFilterCallbackShape::Float
        | ArrayFilterCallbackShape::Null
        | ArrayFilterCallbackShape::Array => {
            module.body().line("i32.const 1");
        }
        ArrayFilterCallbackShape::Numeric => {
            if value_kind == ValueCellKind::Str {
                module.body().line(&format!("local.get {}", value_cell));
                module.body().line("i32.const 8");
                module.body().line("i32.add");
                module.body().line("i32.load");
                module.body().line(&format!("local.get {}", value_cell));
                module.body().line("i32.const 12");
                module.body().line("i32.add");
                module.body().line("i32.load");
                emit_stack_string_is_numeric(module);
            } else {
                module.body().line("i32.const 1");
            }
        }
        ArrayFilterCallbackShape::Object | ArrayFilterCallbackShape::Mixed => {
            unreachable!("array_filter value predicates do not accept mixed callbacks")
        }
    }
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    copy_assoc_entry(&target_entry, &source_entry, module);
    emit_array_filter_increment_len(name, &out_index, module);
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

pub(in crate::codegen::wasm::expr) fn emit_array_filter_assoc_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayFilterCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_filter_assoc_local_assign_with_receiver(
        name,
        source,
        source_span,
        callback,
        None,
        shape,
        module,
    )
}

pub(in crate::codegen::wasm::expr) fn emit_array_filter_assoc_local_instance_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    receiver: &str,
    shape: ArrayFilterCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_filter_assoc_local_assign_with_receiver(
        name,
        source,
        source_span,
        callback,
        Some(receiver),
        shape,
        module,
    )
}

fn emit_array_filter_assoc_local_assign_with_receiver(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    receiver: Option<&str>,
    shape: ArrayFilterCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter() over associative arrays requires known length metadata",
        ));
    };
    let Some(value_kinds) = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec()) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter() over associative arrays requires value-cell metadata",
        ));
    };
    let string_to_float_predicate = shape == ArrayFilterCallbackShape::Float
        && !array_filter_type_predicate_callback(callback)
        && value_kinds.iter().all(|kind| *kind == ValueCellKind::Str);
    if !array_filter_assoc_local_matches_shape(source, shape, module) && !string_to_float_predicate {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter() over associative arrays requires homogeneous callback-compatible values",
        ));
    }
    let preserves_exact_entries = array_filter_type_predicate_callback(callback);
    let key_kinds = module.array_key_kinds(source).map(|kinds| kinds.to_vec());
    let key_values = module.array_key_values(source).map(|keys| keys.to_vec());
    let runtime_key_kind = key_kinds.as_ref().and_then(|kinds| {
        let first = kinds.first().copied()?;
        kinds.iter().all(|kind| *kind == first).then_some(first)
    });
    let runtime_value_kind = value_kinds.first().copied().filter(|first| {
        value_kinds.iter().all(|kind| kind == first)
    });
    let out_index = emit_array_filter_result_prelude(name, len, module);
    if preserves_exact_entries {
        module.set_array_value_cell_kinds(name, Some(value_kinds.clone()));
        module.set_array_key_kinds(name, key_kinds);
        module.set_array_key_values(name, key_values);
    } else {
        module.set_array_value_cell_kinds(name, None);
        module.set_array_key_kinds(name, None);
        module.set_array_key_values(name, None);
    }
    module.set_array_runtime_value_cell_kind(name, runtime_value_kind);
    module.set_array_runtime_key_kind(name, runtime_key_kind.or_else(|| module.array_runtime_key_kind(source)));
    module.set_array_php_normalized_runtime_keys(
        name,
        module.array_has_php_normalized_runtime_keys(source),
    );
    if preserves_exact_entries {
        if let Some((keys, nested_values)) =
            array_filter_callback_assoc_nested_metadata_for_local(source, shape, module)
        {
            module.set_array_key_kinds(
                name,
                Some(keys.iter().map(assoc_key_kind_for_value).collect()),
            );
            module.set_array_key_values(name, Some(keys));
            module.set_array_nested_value_metadata(name, Some(nested_values));
        }
    }
    let source_entry = module.next_label("array_filter_assoc_source_entry");
    let target_entry = module.next_label("array_filter_assoc_target_entry");
    let value_cell = module.next_label("array_filter_assoc_value_cell");
    for local in [&source_entry, &target_entry, &value_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", source_entry));
        emit_assoc_value_cell_address(&source_entry, &value_cell, module);
        emit_array_filter_optional_receiver_arg(receiver, module);
        match shape {
            ArrayFilterCallbackShape::Int => {
                module.body().line(&format!("local.get {}", value_cell));
                module.body().line("i32.const 8");
                module.body().line("i32.add");
                module.body().line("i64.load");
            }
            ArrayFilterCallbackShape::Str => {
                module.body().line(&format!("local.get {}", value_cell));
                module.body().line("i32.const 8");
                module.body().line("i32.add");
                module.body().line("i32.load");
                module.body().line(&format!("local.get {}", value_cell));
                module.body().line("i32.const 12");
                module.body().line("i32.add");
                module.body().line("i32.load");
            }
            ArrayFilterCallbackShape::Bool if !array_filter_type_predicate_callback(callback) => {
                emit_array_filter_scalar_predicate_arg_from_pointer(
                    &value_cell,
                    callback,
                    ValueCellKind::Bool,
                    module,
                );
            }
            ArrayFilterCallbackShape::Float if !array_filter_type_predicate_callback(callback) => {
                emit_array_filter_scalar_predicate_arg_from_pointer(
                    &value_cell,
                    callback,
                    if value_kinds[index] == ValueCellKind::Str {
                        ValueCellKind::Str
                    } else {
                        ValueCellKind::Float
                    },
                    module,
                );
            }
            ArrayFilterCallbackShape::Bool
            | ArrayFilterCallbackShape::Float
            | ArrayFilterCallbackShape::Null
            | ArrayFilterCallbackShape::Array => {
                module.body().line("i32.const 1");
            }
            ArrayFilterCallbackShape::Numeric => {
                if value_kinds[index] == ValueCellKind::Str {
                    module.body().line(&format!("local.get {}", value_cell));
                    module.body().line("i32.const 8");
                    module.body().line("i32.add");
                    module.body().line("i32.load");
                    module.body().line(&format!("local.get {}", value_cell));
                    module.body().line("i32.const 12");
                    module.body().line("i32.add");
                    module.body().line("i32.load");
                } else {
                    module.body().line("i32.const 1");
                }
            }
            ArrayFilterCallbackShape::Object | ArrayFilterCallbackShape::Mixed => {
                unreachable!("array_filter value predicates do not accept mixed callbacks")
            }
        }
        match shape {
            ArrayFilterCallbackShape::Int => emit_array_filter_int_predicate(callback, module),
            ArrayFilterCallbackShape::Str => emit_array_filter_string_predicate(callback, module),
            ArrayFilterCallbackShape::Bool | ArrayFilterCallbackShape::Float
                if !array_filter_type_predicate_callback(callback) => {}
            ArrayFilterCallbackShape::Bool
            | ArrayFilterCallbackShape::Float
            | ArrayFilterCallbackShape::Null
            | ArrayFilterCallbackShape::Array => {}
            ArrayFilterCallbackShape::Numeric => {
                if value_kinds[index] == ValueCellKind::Str {
                    emit_stack_string_is_numeric(module);
                }
            }
            ArrayFilterCallbackShape::Object | ArrayFilterCallbackShape::Mixed => {
                unreachable!("array_filter value predicates do not accept mixed callbacks")
            }
        }
        module.body().open("if");
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("local.get {}", out_index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", target_entry));
        copy_assoc_entry(&target_entry, &source_entry, module);
        emit_array_filter_increment_len(name, &out_index, module);
        module.body().close("end");
    }
    Ok(())
}

fn emit_array_filter_optional_receiver_arg(receiver: Option<&str>, module: &mut WasmModule) {
    if let Some(receiver) = receiver {
        module.body().line(&format!("local.get ${}", receiver));
    }
}
