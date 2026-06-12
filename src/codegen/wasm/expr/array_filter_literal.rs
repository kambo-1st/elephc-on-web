//! Purpose:
//! Emits array_filter assignment paths for literal/static wasm32-web array sources.
//! Keeps literal-source filter wrappers separate from runtime array_filter loop emitters.
//!
//! Called from:
//! - `super::array_filter_assign` when array_filter() assigns from literal arrays.
//!
//! Key details:
//! - Preserves value-cell/key metadata and delegates storage/runtime loops to shared helpers.

use super::*;
use super::array_filter_default::emit_array_filter_default_assoc_local_assign;
use super::array_filter_truthiness::{
    emit_static_scalar_strlen_truthiness, emit_static_scalar_truthiness,
};

pub(super) fn emit_array_filter_literal_ints_assign(
    name: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let out_index = emit_array_filter_result_prelude(name, items.len(), module);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Int; items.len()]));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    let value = module.next_label("array_filter_value");
    module.declare_i64_local(value.trim_start_matches('$').to_string());
    for (index, item) in items.iter().enumerate() {
        require_int(item, module)?;
        module.body().line(&format!("local.set {}", value));
        module.body().line(&format!("local.get {}", value));
        emit_array_filter_int_predicate(callback, module);
        module.body().open("if");
        emit_array_filter_store_int_entry(name, &out_index, index, &value, module);
        module.body().close("end");
    }
    Ok(())
}

pub(super) fn emit_array_filter_default_literal_ints_assign(
    name: &str,
    items: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let out_index = emit_array_filter_result_prelude(name, items.len(), module);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Int; items.len()]));
    let value = module.next_label("array_filter_value");
    module.declare_i64_local(value.trim_start_matches('$').to_string());
    for (index, item) in items.iter().enumerate() {
        require_int(item, module)?;
        module.body().line(&format!("local.set {}", value));
        module.body().line(&format!("local.get {}", value));
        module.body().line("i64.const 0");
        module.body().line("i64.ne");
        module.body().open("if");
        emit_array_filter_store_int_entry(name, &out_index, index, &value, module);
        module.body().close("end");
    }
    Ok(())
}

pub(super) fn emit_array_filter_literal_strings_assign(
    name: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let out_index = emit_array_filter_result_prelude(name, items.len(), module);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Str; items.len()]));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    let ptr = module.next_label("array_filter_string_ptr");
    let len = module.next_label("array_filter_string_len");
    module.declare_i32_local(ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(len.trim_start_matches('$').to_string());
    for (index, item) in items.iter().enumerate() {
        emit_string_value_to_stack(item, module)?;
        module.body().line(&format!("local.set {}", len));
        module.body().line(&format!("local.set {}", ptr));
        module.body().line(&format!("local.get {}", ptr));
        module.body().line(&format!("local.get {}", len));
        emit_array_filter_string_predicate(callback, module);
        module.body().open("if");
        emit_array_filter_store_string_entry(name, &out_index, index, &ptr, &len, module);
        module.body().close("end");
    }
    Ok(())
}

pub(super) fn emit_array_filter_default_literal_strings_assign(
    name: &str,
    items: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let out_index = emit_array_filter_result_prelude(name, items.len(), module);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Str; items.len()]));
    let ptr = module.next_label("array_filter_string_ptr");
    let len = module.next_label("array_filter_string_len");
    module.declare_i32_local(ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(len.trim_start_matches('$').to_string());
    for (index, item) in items.iter().enumerate() {
        emit_string_value_to_stack(item, module)?;
        module.body().line(&format!("local.set {}", len));
        module.body().line(&format!("local.set {}", ptr));
        emit_string_parts_truthiness(&ptr, &len, module);
        module.body().open("if");
        emit_array_filter_store_string_entry(name, &out_index, index, &ptr, &len, module);
        module.body().close("end");
    }
    Ok(())
}

pub(super) fn emit_array_filter_default_literal_scalar_assign(
    name: &str,
    items: &[Expr],
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let out_index = emit_array_filter_result_prelude(name, items.len(), module);
    module.set_array_value_cell_kinds(name, Some(vec![kind; items.len()]));
    if kind == ValueCellKind::Array {
        let nested_metadata = array_filter_default_nested_metadata_for_items(items, module);
        if let Some(metadata) = nested_metadata.as_ref() {
            module.set_array_length(name, metadata.len());
        }
        module.set_array_nested_value_metadata(name, nested_metadata);
        if let Some(keys) = array_filter_default_int_key_values_for_items(items) {
            module.set_array_key_kinds(
                name,
                Some(keys.iter().map(assoc_key_kind_for_value).collect()),
            );
            module.set_array_key_values(name, Some(keys));
        }
    } else {
        module.set_array_nested_value_metadata(name, None);
    }
    for (index, item) in items.iter().enumerate() {
        emit_static_scalar_truthiness(item, kind, module)?;
        module.body().open("if");
        emit_array_filter_store_value_expr_entry(name, &out_index, index, item, module)?;
        module.body().close("end");
    }
    Ok(())
}

pub(super) fn emit_array_filter_literal_scalar_type_assign(
    name: &str,
    items: &[Expr],
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let out_index = emit_array_filter_result_prelude(name, items.len(), module);
    module.set_array_value_cell_kinds(name, Some(vec![kind; items.len()]));
    module.set_array_runtime_value_cell_kind(name, Some(kind));
    module.set_array_key_kinds(name, Some(vec![AssocKeyKind::Int; items.len()]));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    module.set_array_key_values(
        name,
        Some(
            (0..items.len())
                .map(|index| AssocKeyValue::Int(index as i64))
                .collect(),
        ),
    );
    if kind == ValueCellKind::Array {
        module.set_array_nested_value_metadata(name, nested_array_metadata_for_items(items, module));
        module.set_array_key_kinds(name, Some(vec![AssocKeyKind::Int; items.len()]));
        module.set_array_key_values(
            name,
            Some(
                (0..items.len())
                    .map(|index| AssocKeyValue::Int(index as i64))
                    .collect(),
            ),
        );
    }
    for (index, item) in items.iter().enumerate() {
        emit_array_filter_store_value_expr_entry(name, &out_index, index, item, module)?;
    }
    Ok(())
}

pub(super) fn emit_array_filter_literal_scalar_predicate_assign(
    name: &str,
    items: &[Expr],
    callback: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let out_index = emit_array_filter_result_prelude(name, items.len(), module);
    module.set_array_value_cell_kinds(name, Some(vec![kind; items.len()]));
    module.set_array_runtime_value_cell_kind(name, Some(kind));
    for (index, item) in items.iter().enumerate() {
        emit_array_filter_scalar_predicate_arg_from_expr(item, callback, kind, module)?;
        module.body().open("if");
        emit_array_filter_store_value_expr_entry(name, &out_index, index, item, module)?;
        module.body().close("end");
    }
    Ok(())
}

pub(super) fn emit_array_filter_literal_scalar_strlen_assign(
    name: &str,
    items: &[Expr],
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let out_index = emit_array_filter_result_prelude(name, items.len(), module);
    module.set_array_value_cell_kinds(name, Some(vec![kind; items.len()]));
    module.set_array_runtime_value_cell_kind(name, Some(kind));
    module.set_array_key_kinds(name, Some(vec![AssocKeyKind::Int; items.len()]));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    module.set_array_key_values(
        name,
        Some(
            (0..items.len())
                .map(|index| AssocKeyValue::Int(index as i64))
                .collect(),
        ),
    );
    for (index, item) in items.iter().enumerate() {
        emit_static_scalar_strlen_truthiness(item, kind, module)?;
        module.body().open("if");
        emit_array_filter_store_value_expr_entry(name, &out_index, index, item, module)?;
        module.body().close("end");
    }
    Ok(())
}

pub(super) fn emit_array_filter_assoc_literal_assign(
    name: &str,
    items: &[(Expr, Expr)],
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayFilterCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_filter_assoc_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_assoc_array_items_assign(&temp, items, module)?;
    emit_array_filter_assoc_local_assign(name, &temp, source_span, callback, shape, module)
}

pub(super) fn emit_array_filter_assoc_strlen_literal_assign(
    name: &str,
    items: &[(Expr, Expr)],
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_filter_assoc_strlen_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_assoc_array_items_assign(&temp, items, module)?;
    emit_array_filter_assoc_strlen_local_assign(name, &temp, source_span, module)
}

pub(super) fn emit_array_filter_default_assoc_literal_assign(
    name: &str,
    items: &[(Expr, Expr)],
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_filter_assoc_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_assoc_array_items_assign(&temp, items, module)?;
    emit_array_filter_default_assoc_local_assign(name, &temp, source_span, module)?;
    if let Some((keys, values)) = array_filter_default_assoc_literal_metadata(items, module) {
        module.set_array_key_kinds(
            name,
            Some(keys.iter().map(assoc_key_kind_for_value).collect()),
        );
        module.set_array_key_values(name, Some(keys));
        module.set_array_value_cell_kinds(name, Some(values));
        module.set_array_nested_value_metadata(
            name,
            array_filter_default_nested_metadata_for_assoc_items(items, module),
        );
    }
    Ok(())
}
