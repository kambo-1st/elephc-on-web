//! Purpose:
//! Emits wasm32-web array_map lowering for associative array assignments.
//! Keeps key-preserving assoc map loops separate from indexed/value array map helpers.
//!
//! Called from:
//! - `super::array_map_assign` while lowering associative array_map assignments.
//!
//! Key details:
//! - Copies assoc keys verbatim and preserves runtime key/value metadata for mapped results.

use super::*;

pub(super) fn emit_array_map_assoc_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayMapCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_map_assoc_local_assign_with_receiver(name, source, source_span, callback, None, shape, module)
}

pub(super) fn emit_array_map_assoc_local_instance_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    receiver: &str,
    shape: ArrayMapCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_map_assoc_local_assign_with_receiver(
        name,
        source,
        source_span,
        callback,
        Some(receiver),
        shape,
        module,
    )
}

fn emit_array_map_assoc_local_assign_with_receiver(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    receiver: Option<&str>,
    shape: ArrayMapCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() over associative arrays requires known length metadata",
        ));
    };
    let key_values = module.array_key_values(source).map(|values| values.to_vec());
    let key_kinds = module.array_key_kinds(source).map(|kinds| kinds.to_vec());
    let source_value_kinds = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec());
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(
        name,
        Some(vec![
            match shape {
                ArrayMapCallbackShape::IntToInt => ValueCellKind::Int,
                ArrayMapCallbackShape::IntToBool => ValueCellKind::Bool,
                ArrayMapCallbackShape::IntIntToInt => unreachable!("multi-array map cannot preserve one assoc source"),
                ArrayMapCallbackShape::IntIntIntToInt => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::IntIntIntIntToInt => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::IntIntIntIntIntToInt => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::IntIntIntIntIntIntToInt => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::StrToInt => ValueCellKind::Int,
                ArrayMapCallbackShape::StrToBool => ValueCellKind::Bool,
                ArrayMapCallbackShape::StrToStr
                | ArrayMapCallbackShape::ObjectToStr
                | ArrayMapCallbackShape::ObjectToTypeStr => ValueCellKind::Str,
                ArrayMapCallbackShape::BoolToBool
                | ArrayMapCallbackShape::FloatToBool
                | ArrayMapCallbackShape::NullToBool
                | ArrayMapCallbackShape::NumericToBool
                | ArrayMapCallbackShape::ArrayToBool
                | ArrayMapCallbackShape::ObjectToBool => ValueCellKind::Bool,
                ArrayMapCallbackShape::StrStrToStr => unreachable!("multi-array map cannot preserve one assoc source"),
                ArrayMapCallbackShape::StrStrStrToStr => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::StrStrStrStrToStr => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::StrStrStrStrStrToStr => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::StrStrStrStrStrStrToStr => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedToInt => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedToStr => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedToBool => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedToFloat => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedToInt => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedToStr => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedToBool => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedToFloat => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedToInt => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedToStr => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedToBool => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedToFloat => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedMixedToInt => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedMixedToStr => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedMixedToBool => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedMixedToFloat => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToInt => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToStr => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToBool => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToFloat => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToInt => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToStr => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToBool => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToFloat => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToInt => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToStr => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToBool => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToFloat => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToInt => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToStr => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToBool => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
                ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToFloat => {
                    unreachable!("multi-array map cannot preserve one assoc source")
                }
            };
            len
        ]),
    );
    module.set_array_nested_value_metadata(name, None);
    module.set_array_key_kinds(name, key_kinds);
    module.set_array_key_values(name, key_values);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    let source_entry = module.next_label("array_map_assoc_source_entry");
    let target_entry = module.next_label("array_map_assoc_target_entry");
    let source_cell = module.next_label("array_map_assoc_source_cell");
    let target_cell = module.next_label("array_map_assoc_target_cell");
    for local in [&source_entry, &target_entry, &source_cell, &target_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", target_entry));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", source_entry));
        copy_assoc_entry_key(&target_entry, &source_entry, module);
        emit_assoc_value_cell_address(&source_entry, &source_cell, module);
        emit_assoc_value_cell_address(&target_entry, &target_cell, module);
        match shape {
            ArrayMapCallbackShape::IntToInt => {
                module.body().line(&format!("local.get {}", target_cell));
                emit_array_map_optional_receiver_arg(receiver, module);
                module.body().line(&format!("local.get {}", source_cell));
                module.body().line("i32.const 8");
                module.body().line("i32.add");
                let source_value_kind = source_value_kinds
                    .as_ref()
                    .and_then(|kinds| kinds.get(index))
                    .copied();
                match source_value_kind {
                    Some(ValueCellKind::Float) => {
                        module.body().line("f64.load");
                        module.body().line("i64.trunc_f64_s");
                    }
                    Some(ValueCellKind::Str) => {
                        module.body().line("i32.load");
                        module.body().line(&format!("local.get {}", source_cell));
                        module.body().line("i32.const 12");
                        module.body().line("i32.add");
                        module.body().line("i32.load");
                        emit_stack_string_numeric_int_arg("array_map_assoc_string_int", module);
                    }
                    _ => {
                        module.body().line("i64.load");
                    }
                }
                module
                    .body()
                    .line(&format!("call ${}", wasm_function_name(callback)));
                module.body().line("call $__rt_value_store_int");
            }
            ArrayMapCallbackShape::IntToBool => {
                module.body().line(&format!("local.get {}", target_cell));
                module.body().line(&format!("local.get {}", source_cell));
                module.body().line("i32.const 8");
                module.body().line("i32.add");
                module.body().line("i64.load");
                emit_array_map_int_bool_result(callback, module);
                module.body().line("call $__rt_value_store_bool");
            }
            ArrayMapCallbackShape::IntIntToInt => unreachable!("multi-array map cannot preserve one assoc source"),
            ArrayMapCallbackShape::IntIntIntToInt => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::IntIntIntIntToInt => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::IntIntIntIntIntToInt => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::IntIntIntIntIntIntToInt => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::StrToInt => {
                module.body().line(&format!("local.get {}", target_cell));
                module.body().line(&format!("local.get {}", source_cell));
                module.body().line("i32.const 8");
                module.body().line("i32.add");
                module.body().line("i32.load");
                module.body().line(&format!("local.get {}", source_cell));
                module.body().line("i32.const 12");
                module.body().line("i32.add");
                module.body().line("i32.load");
                emit_array_map_strlen_result(module);
                module.body().line("call $__rt_value_store_int");
            }
            ArrayMapCallbackShape::StrToBool => {
                module.body().line(&format!("local.get {}", target_cell));
                module.body().line(&format!("local.get {}", source_cell));
                module.body().line("i32.const 8");
                module.body().line("i32.add");
                module.body().line("i32.load");
                module.body().line(&format!("local.get {}", source_cell));
                module.body().line("i32.const 12");
                module.body().line("i32.add");
                module.body().line("i32.load");
                emit_array_map_string_bool_result(callback, module);
                module.body().line("call $__rt_value_store_bool");
            }
            ArrayMapCallbackShape::StrToStr => {
                module.body().line(&format!("local.get {}", target_cell));
                emit_array_map_optional_receiver_arg(receiver, module);
                module.body().line(&format!("local.get {}", source_cell));
                module.body().line("i32.const 8");
                module.body().line("i32.add");
                module.body().line("i32.load");
                module.body().line(&format!("local.get {}", source_cell));
                module.body().line("i32.const 12");
                module.body().line("i32.add");
                module.body().line("i32.load");
                module
                    .body()
                    .line(&format!("call ${}", wasm_function_name(callback)));
                module.body().line("call $__rt_value_store_string");
            }
            ArrayMapCallbackShape::ObjectToStr | ArrayMapCallbackShape::ObjectToTypeStr => {
                unreachable!("object-to-string map over assoc arrays is not shape-matched yet")
            }
            ArrayMapCallbackShape::BoolToBool if !array_filter_type_predicate_callback(callback) => {
                module.body().line(&format!("local.get {}", target_cell));
                if let Some(receiver) = receiver {
                    module.body().line(&format!("local.get ${}", receiver));
                    module.body().line(&format!("local.get {}", source_cell));
                    module.body().line("call $__rt_value_cell_payload_i64");
                    module.body().line("i64.const 0");
                    module.body().line("i64.ne");
                    module
                        .body()
                        .line(&format!("call ${}", wasm_function_name(callback)));
                } else {
                    emit_array_filter_scalar_predicate_arg_from_pointer(
                        &source_cell,
                        callback,
                        ValueCellKind::Bool,
                        module,
                    );
                }
                module.body().line("call $__rt_value_store_bool");
            }
            ArrayMapCallbackShape::FloatToBool if !array_filter_type_predicate_callback(callback) => {
                module.body().line(&format!("local.get {}", target_cell));
                if let Some(receiver) = receiver {
                    module.body().line(&format!("local.get ${}", receiver));
                    module.body().line(&format!("local.get {}", source_cell));
                    module.body().line("call $__rt_value_cell_payload_f64");
                    module
                        .body()
                        .line(&format!("call ${}", wasm_function_name(callback)));
                } else {
                    emit_array_filter_scalar_predicate_arg_from_pointer(
                        &source_cell,
                        callback,
                        ValueCellKind::Float,
                        module,
                    );
                }
                module.body().line("call $__rt_value_store_bool");
            }
            ArrayMapCallbackShape::BoolToBool
            | ArrayMapCallbackShape::FloatToBool
            | ArrayMapCallbackShape::NullToBool
            | ArrayMapCallbackShape::ArrayToBool
            | ArrayMapCallbackShape::ObjectToBool => {
                module.body().line(&format!("local.get {}", target_cell));
                module.body().line("i32.const 1");
                module.body().line("call $__rt_value_store_bool");
            }
            ArrayMapCallbackShape::NumericToBool => {
                module.body().line(&format!("local.get {}", target_cell));
                if source_value_kinds
                    .as_ref()
                    .and_then(|kinds| kinds.get(index))
                    == Some(&ValueCellKind::Str)
                {
                    module.body().line(&format!("local.get {}", source_cell));
                    module.body().line("i32.const 8");
                    module.body().line("i32.add");
                    module.body().line("i32.load");
                    module.body().line(&format!("local.get {}", source_cell));
                    module.body().line("i32.const 12");
                    module.body().line("i32.add");
                    module.body().line("i32.load");
                    emit_stack_string_is_numeric(module);
                } else {
                    module.body().line("i32.const 1");
                }
                module.body().line("call $__rt_value_store_bool");
            }
            ArrayMapCallbackShape::StrStrToStr => unreachable!("multi-array map cannot preserve one assoc source"),
            ArrayMapCallbackShape::StrStrStrToStr => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::StrStrStrStrToStr => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::StrStrStrStrStrToStr => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::StrStrStrStrStrStrToStr => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedToInt => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedToStr => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedToBool => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedToFloat => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedToInt => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedToStr => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedToBool => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedToFloat => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedToInt => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedToStr => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedToBool => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedToFloat => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedMixedToInt => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedMixedToStr => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedMixedToBool => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedMixedToFloat => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToInt => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToStr => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToBool => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToFloat => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToInt => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToStr => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToBool => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToFloat => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToInt => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToStr => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToBool => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToFloat => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToInt => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToStr => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToBool => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
            ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToFloat => {
                unreachable!("multi-array map cannot preserve one assoc source")
            }
        }
    }
    Ok(())
}

pub(super) fn emit_array_map_assoc_float_local_instance_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_map_runtime_assoc_float_local_instance_assign(
            name,
            source,
            source_span,
            callback,
            capture_local,
            module,
        );
    };
    if !module
        .array_value_cell_kinds(source)
        .is_some_and(|kinds| kinds.iter().all(|kind| *kind == ValueCellKind::Float))
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() over associative float arrays requires float value-cell metadata",
        ));
    }
    let key_values = module.array_key_values(source).map(|values| values.to_vec());
    let key_kinds = module.array_key_kinds(source).map(|kinds| kinds.to_vec());
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Float; len]));
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Float));
    module.set_array_nested_value_metadata(name, None);
    module.set_array_key_kinds(name, key_kinds);
    module.set_array_key_values(name, key_values);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    let source_entry = module.next_label("array_map_assoc_float_source_entry");
    let target_entry = module.next_label("array_map_assoc_float_target_entry");
    let source_cell = module.next_label("array_map_assoc_float_source_cell");
    let target_cell = module.next_label("array_map_assoc_float_target_cell");
    for local in [&source_entry, &target_entry, &source_cell, &target_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", target_entry));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", source_entry));
        copy_assoc_entry_key(&target_entry, &source_entry, module);
        emit_assoc_value_cell_address(&source_entry, &source_cell, module);
        emit_assoc_value_cell_address(&target_entry, &target_cell, module);
        module.body().line(&format!("local.get {}", target_cell));
        module.body().line(&format!("local.get ${}", capture_local));
        module.body().line(&format!("local.get {}", source_cell));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line("f64.load");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("call $__rt_value_store_float");
    }
    Ok(())
}

pub(super) fn emit_array_map_assoc_literal_assign(
    name: &str,
    items: &[(Expr, Expr)],
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayMapCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_map_assoc_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_assoc_array_items_assign(&temp, items, module)?;
    emit_array_map_assoc_local_assign(name, &temp, source_span, callback, shape, module)
}

pub(super) fn emit_array_map_assoc_strlen_literal_assign(
    name: &str,
    items: &[(Expr, Expr)],
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_map_assoc_strlen_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_assoc_array_items_assign(&temp, items, module)?;
    emit_array_map_assoc_strlen_local_assign(name, &temp, source_span, module)
}

pub(super) fn emit_array_map_assoc_strlen_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map(strlen) over associative arrays requires known length metadata",
        ));
    };
    let Some(source_value_kinds) = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec()) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map(strlen) over associative arrays requires value-cell metadata",
        ));
    };
    if !source_value_kinds
        .iter()
        .all(|kind| array_map_strlen_value_kind_is_supported(*kind))
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map(strlen) over associative arrays requires scalar string-coercible values",
        ));
    }
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Int; len]));
    module.set_array_nested_value_metadata(name, None);
    module.set_array_key_kinds(name, module.array_key_kinds(source).map(|kinds| kinds.to_vec()));
    module.set_array_key_values(name, module.array_key_values(source).map(|values| values.to_vec()));
    module.set_array_runtime_key_kind(name, module.array_runtime_key_kind(source));
    module.set_array_php_normalized_runtime_keys(
        name,
        module.array_has_php_normalized_runtime_keys(source),
    );
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    let source_entry = module.next_label("array_map_assoc_strlen_source_entry");
    let target_entry = module.next_label("array_map_assoc_strlen_target_entry");
    let source_cell = module.next_label("array_map_assoc_strlen_source_cell");
    let target_cell = module.next_label("array_map_assoc_strlen_target_cell");
    for local in [&source_entry, &target_entry, &source_cell, &target_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for (index, kind) in source_value_kinds.iter().enumerate() {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", target_entry));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", source_entry));
        copy_assoc_entry_key(&target_entry, &source_entry, module);
        emit_assoc_value_cell_address(&source_entry, &source_cell, module);
        emit_assoc_value_cell_address(&target_entry, &target_cell, module);
        module.body().line(&format!("local.get {}", target_cell));
        emit_value_cell_pointer_strlen_length(&source_cell, *kind, module);
        module.body().line("call $__rt_value_store_int");
    }
    Ok(())
}

pub(super) fn emit_array_map_runtime_assoc_strlen_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(value_kind) = module.array_runtime_value_cell_kind(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map(strlen) over runtime associative arrays requires homogeneous value metadata",
        ));
    };
    if !array_map_strlen_value_kind_is_supported(value_kind) {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map(strlen) over runtime associative arrays requires scalar string-coercible values",
        ));
    }
    let index = module.next_label("array_map_assoc_strlen_runtime_index");
    let source_entry = module.next_label("array_map_assoc_strlen_runtime_source_entry");
    let target_entry = module.next_label("array_map_assoc_strlen_runtime_target_entry");
    let source_cell = module.next_label("array_map_assoc_strlen_runtime_source_cell");
    let target_cell = module.next_label("array_map_assoc_strlen_runtime_target_cell");
    let done_label = module.next_label("array_map_assoc_strlen_runtime_done");
    let loop_label = module.next_label("array_map_assoc_strlen_runtime_loop");
    for local in [&index, &source_entry, &target_entry, &source_cell, &target_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    module.set_array_runtime_key_kind(name, module.array_runtime_key_kind(source));
    module.set_array_php_normalized_runtime_keys(
        name,
        module.array_has_php_normalized_runtime_keys(source),
    );
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Int));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_assoc_entries");
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
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", source_entry));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    copy_assoc_entry_key(&target_entry, &source_entry, module);
    emit_assoc_value_cell_address(&source_entry, &source_cell, module);
    emit_assoc_value_cell_address(&target_entry, &target_cell, module);
    module.body().line(&format!("local.get {}", target_cell));
    emit_value_cell_pointer_strlen_length(&source_cell, value_kind, module);
    module.body().line("call $__rt_value_store_int");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_map_runtime_assoc_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayMapCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_map_runtime_assoc_local_assign_with_receiver(
        name,
        source,
        source_span,
        callback,
        None,
        shape,
        module,
    )
}

pub(super) fn emit_array_map_runtime_assoc_local_instance_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    receiver: &str,
    shape: ArrayMapCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_map_runtime_assoc_local_assign_with_receiver(
        name,
        source,
        source_span,
        callback,
        Some(receiver),
        shape,
        module,
    )
}

fn emit_array_map_runtime_assoc_float_local_instance_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Float) {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() over runtime associative float arrays requires float value-cell metadata",
        ));
    }
    let index = module.next_label("array_map_assoc_float_runtime_index");
    let source_entry = module.next_label("array_map_assoc_float_runtime_source_entry");
    let target_entry = module.next_label("array_map_assoc_float_runtime_target_entry");
    let source_cell = module.next_label("array_map_assoc_float_runtime_source_cell");
    let target_cell = module.next_label("array_map_assoc_float_runtime_target_cell");
    let done_label = module.next_label("array_map_assoc_float_runtime_done");
    let loop_label = module.next_label("array_map_assoc_float_runtime_loop");
    for local in [&index, &source_entry, &target_entry, &source_cell, &target_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    module.set_array_runtime_key_kind(name, module.array_runtime_key_kind(source));
    module.set_array_php_normalized_runtime_keys(
        name,
        module.array_has_php_normalized_runtime_keys(source),
    );
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Float));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_assoc_entries");
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
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", source_entry));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    copy_assoc_entry_key(&target_entry, &source_entry, module);
    emit_assoc_value_cell_address(&source_entry, &source_cell, module);
    emit_assoc_value_cell_address(&target_entry, &target_cell, module);
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get {}", source_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("f64.load");
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

fn emit_array_map_runtime_assoc_local_assign_with_receiver(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    receiver: Option<&str>,
    shape: ArrayMapCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(value_kind) = module.array_runtime_value_cell_kind(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() over runtime associative arrays requires homogeneous value metadata",
        ));
    };
    if !array_map_runtime_assoc_local_matches_shape(source, shape, module) {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() over runtime associative arrays requires callback-compatible values",
        ));
    }
    let result_kind = array_map_result_value_cell_kind(shape);
    let index = module.next_label("array_map_assoc_runtime_index");
    let source_entry = module.next_label("array_map_assoc_runtime_source_entry");
    let target_entry = module.next_label("array_map_assoc_runtime_target_entry");
    let source_cell = module.next_label("array_map_assoc_runtime_source_cell");
    let target_cell = module.next_label("array_map_assoc_runtime_target_cell");
    let done_label = module.next_label("array_map_assoc_runtime_done");
    let loop_label = module.next_label("array_map_assoc_runtime_loop");
    for local in [&index, &source_entry, &target_entry, &source_cell, &target_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    module.set_array_runtime_key_kind(name, module.array_runtime_key_kind(source));
    module.set_array_php_normalized_runtime_keys(
        name,
        module.array_has_php_normalized_runtime_keys(source),
    );
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(result_kind));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_assoc_entries");
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
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", source_entry));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    copy_assoc_entry_key(&target_entry, &source_entry, module);
    emit_assoc_value_cell_address(&source_entry, &source_cell, module);
    emit_assoc_value_cell_address(&target_entry, &target_cell, module);
    match shape {
        ArrayMapCallbackShape::IntToInt => {
            module.body().line(&format!("local.get {}", target_cell));
            emit_array_map_optional_receiver_arg(receiver, module);
            module.body().line(&format!("local.get {}", source_cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            match value_kind {
                ValueCellKind::Float => {
                    module.body().line("f64.load");
                    module.body().line("i64.trunc_f64_s");
                }
                ValueCellKind::Str => {
                    module.body().line("i32.load");
                    module.body().line(&format!("local.get {}", source_cell));
                    module.body().line("i32.const 12");
                    module.body().line("i32.add");
                    module.body().line("i32.load");
                    emit_stack_string_numeric_int_arg("array_map_runtime_assoc_string_int", module);
                }
                _ => {
                    module.body().line("i64.load");
                }
            }
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(callback)));
            module.body().line("call $__rt_value_store_int");
        }
        ArrayMapCallbackShape::IntToBool => {
            module.body().line(&format!("local.get {}", target_cell));
            module.body().line(&format!("local.get {}", source_cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.load");
            emit_array_map_int_bool_result(callback, module);
            module.body().line("call $__rt_value_store_bool");
        }
        ArrayMapCallbackShape::StrToInt => {
            module.body().line(&format!("local.get {}", target_cell));
            module.body().line(&format!("local.get {}", source_cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.get {}", source_cell));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            emit_array_map_strlen_result(module);
            module.body().line("call $__rt_value_store_int");
        }
        ArrayMapCallbackShape::StrToBool => {
            module.body().line(&format!("local.get {}", target_cell));
            module.body().line(&format!("local.get {}", source_cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.get {}", source_cell));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            emit_array_map_string_bool_result(callback, module);
            module.body().line("call $__rt_value_store_bool");
        }
        ArrayMapCallbackShape::StrToStr => {
            module.body().line(&format!("local.get {}", target_cell));
            emit_array_map_optional_receiver_arg(receiver, module);
            module.body().line(&format!("local.get {}", source_cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.get {}", source_cell));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(callback)));
            module.body().line("call $__rt_value_store_string");
        }
        ArrayMapCallbackShape::ObjectToStr | ArrayMapCallbackShape::ObjectToTypeStr => {
            unreachable!("object-to-string map over assoc arrays is not shape-matched yet")
        }
        ArrayMapCallbackShape::BoolToBool if !array_filter_type_predicate_callback(callback) => {
            module.body().line(&format!("local.get {}", target_cell));
            if let Some(receiver) = receiver {
                module.body().line(&format!("local.get ${}", receiver));
                module.body().line(&format!("local.get {}", source_cell));
                module.body().line("call $__rt_value_cell_payload_i64");
                module.body().line("i64.const 0");
                module.body().line("i64.ne");
                module
                    .body()
                    .line(&format!("call ${}", wasm_function_name(callback)));
            } else {
                emit_array_filter_scalar_predicate_arg_from_pointer(
                    &source_cell,
                    callback,
                    ValueCellKind::Bool,
                    module,
                );
            }
            module.body().line("call $__rt_value_store_bool");
        }
        ArrayMapCallbackShape::FloatToBool if !array_filter_type_predicate_callback(callback) => {
            module.body().line(&format!("local.get {}", target_cell));
            if let Some(receiver) = receiver {
                module.body().line(&format!("local.get ${}", receiver));
                module.body().line(&format!("local.get {}", source_cell));
                module.body().line("call $__rt_value_cell_payload_f64");
                module
                    .body()
                    .line(&format!("call ${}", wasm_function_name(callback)));
            } else {
                emit_array_filter_scalar_predicate_arg_from_pointer(
                    &source_cell,
                    callback,
                    ValueCellKind::Float,
                    module,
                );
            }
            module.body().line("call $__rt_value_store_bool");
        }
        ArrayMapCallbackShape::BoolToBool
        | ArrayMapCallbackShape::FloatToBool
        | ArrayMapCallbackShape::NullToBool
        | ArrayMapCallbackShape::ArrayToBool
        | ArrayMapCallbackShape::ObjectToBool => {
            module.body().line(&format!("local.get {}", target_cell));
            module.body().line("i32.const 1");
            module.body().line("call $__rt_value_store_bool");
        }
        ArrayMapCallbackShape::NumericToBool => {
            module.body().line(&format!("local.get {}", target_cell));
            if value_kind == ValueCellKind::Str {
                module.body().line(&format!("local.get {}", source_cell));
                module.body().line("i32.const 8");
                module.body().line("i32.add");
                module.body().line("i32.load");
                module.body().line(&format!("local.get {}", source_cell));
                module.body().line("i32.const 12");
                module.body().line("i32.add");
                module.body().line("i32.load");
                emit_stack_string_is_numeric(module);
            } else {
                module.body().line("i32.const 1");
            }
            module.body().line("call $__rt_value_store_bool");
        }
        ArrayMapCallbackShape::IntIntToInt
        | ArrayMapCallbackShape::IntIntIntToInt
        | ArrayMapCallbackShape::IntIntIntIntToInt
        | ArrayMapCallbackShape::IntIntIntIntIntToInt
        | ArrayMapCallbackShape::IntIntIntIntIntIntToInt
        | ArrayMapCallbackShape::StrStrToStr
        | ArrayMapCallbackShape::StrStrStrToStr
        | ArrayMapCallbackShape::StrStrStrStrToStr
        | ArrayMapCallbackShape::StrStrStrStrStrToStr
        | ArrayMapCallbackShape::StrStrStrStrStrStrToStr
        | ArrayMapCallbackShape::MixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedToFloat
        | ArrayMapCallbackShape::MixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedToFloat
        | ArrayMapCallbackShape::MixedMixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedMixedToFloat
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedToFloat
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToFloat
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToFloat
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToFloat
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToFloat => {
            unreachable!("multi-array map cannot preserve one assoc source")
        }
    }
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_array_map_optional_receiver_arg(receiver: Option<&str>, module: &mut WasmModule) {
    if let Some(receiver) = receiver {
        module.body().line(&format!("local.get ${}", receiver));
    }
}
