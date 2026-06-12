//! Purpose:
//! Lowers wasm32-web nested array index reads and dynamic nested associative scans.
//!
//! Called from:
//! - `super::array_indexing` and sibling expression helpers that need nested array reads.
//!
//! Key details:
//! - Dynamic nested assoc reads scan runtime hash entries while preserving value-cell kinds.
//! - Unsupported heterogeneous or missing metadata paths stay as `CompileError` cases elsewhere.

use super::*;
use super::array_indexing::{emit_assoc_scalar_result_from_cell, emit_value_cell_result_from_cell};
use super::array_indexing_chunks::emit_direct_array_chunk_first_scalar_read;
use super::array_indexing_metadata::*;
use super::array_value_cells::mixed_key_local_name;

pub(super) fn dynamic_parent_nested_assoc_static_access_kind(
    array: &Expr,
    index: &Expr,
    module: &WasmModule,
) -> Option<ValueCellKind> {
    let metadata = dynamic_nested_array_value_metadata(array, module)?;
    let key = static_assoc_access_key(index, module)?;
    nested_assoc_static_key_kind(&metadata, &key)
}

pub(super) fn emit_nested_array_index_expr(
    expr: &Expr,
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if let Some(kind) = emit_direct_array_chunk_first_scalar_read(expr, array, index, module)? {
        return Ok(kind);
    }
    if let Some(kind) = emit_partial_static_nested_assoc_index_expr(array, index, module)? {
        return Ok(kind);
    }
    if let Some(kind) = emit_dynamic_parent_nested_assoc_index_expr(array, index, module)? {
        return Ok(kind);
    }
    if let Some(kind) = emit_dynamic_outer_nested_assoc_index_expr(array, index, module)? {
        return Ok(kind);
    }
    if let Some(kind) = emit_dynamic_nested_assoc_index_expr(array, index, module)? {
        return Ok(kind);
    }
    let Some((metadata, index)) = nested_array_index_metadata(array, index, module) else {
        return Err(nested_array_access_unsupported(expr));
    };
    if matches!(index, NestedArrayIndex::Offset(index) if metadata.layout != ArrayLayout::Assoc && index >= metadata.len) {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web scalar nested-array access does not support missing indexes yet",
        ));
    }
    let ptr = module.next_label("nested_array_ptr");
    let len = module.next_label("nested_array_len");
    module.declare_i32_local(ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(len.trim_start_matches('$').to_string());
    if !emit_static_property_nested_array_source_parts(array, &ptr, &len, module)? {
        match emit_expr(array, module)? {
            ValueKind::Array => {
                module.body().line(&format!("local.set {}", len));
                module.body().line(&format!("local.set {}", ptr));
            }
            _ => return Err(nested_array_access_unsupported(expr)),
        }
    }
    match metadata.layout {
        ArrayLayout::CompactInt => {
            let NestedArrayIndex::Offset(index) = index else {
                return Err(nested_array_access_unsupported(expr));
            };
            module.body().line(&format!("i32.const {}", index));
            module.body().line(&format!("local.get {}", len));
            module.body().line("i32.ge_u");
            module.body().open("if");
            module.body().line("unreachable");
            module.body().close("end");
            module.body().line(&format!("local.get {}", ptr));
            module.body().line(&format!("i32.const {}", index * 8));
            module.body().line("i32.add");
            module.body().line("i64.load");
            Ok(ValueKind::Int)
        }
        ArrayLayout::Value => {
            let NestedArrayIndex::Offset(index) = index else {
                return Err(nested_array_access_unsupported(expr));
            };
            let cell = module.next_label("nested_value_cell");
            module.declare_i32_local(cell.trim_start_matches('$').to_string());
            module.body().line(&format!("i32.const {}", index));
            module.body().line(&format!("local.get {}", len));
            module.body().line("i32.ge_u");
            module.body().open("if");
            module.body().line("unreachable");
            module.body().close("end");
            module.body().line(&format!("local.get {}", ptr));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("call $__rt_value_cell");
            module.body().line(&format!("local.set {}", cell));
            let Some(kinds) = metadata.value_kinds else {
                module.body().line(&format!("local.get {}", cell));
                return Ok(ValueKind::Mixed);
            };
            let Some(kind) = kinds.get(index).copied() else {
                module.body().line(&format!("local.get {}", cell));
                return Ok(ValueKind::Mixed);
            };
            emit_value_cell_result_from_cell(&cell, kind, module)
        }
        ArrayLayout::Assoc => {
            let NestedArrayIndex::Key(key) = index else {
                return Err(nested_array_access_unsupported(expr));
            };
            if let (Some(keys), Some(values)) = (&metadata.key_values, &metadata.value_kinds) {
                let Some(entry_index) = keys.iter().position(|candidate| *candidate == key) else {
                    if keys.len() == metadata.len {
                        module.body().line("i32.const 0");
                        return Ok(ValueKind::Null);
                    }
                    return Err(nested_array_access_unsupported(expr));
                };
                let Some(kind) = values.get(entry_index).copied() else {
                    return Err(nested_array_access_unsupported(expr));
                };
                let entry = module.next_label("nested_assoc_entry");
                let cell = module.next_label("nested_assoc_value_cell");
                for local in [&entry, &cell] {
                    module.declare_i32_local(local.trim_start_matches('$').to_string());
                }
                module.body().line(&format!("local.get {}", ptr));
                module.body().line(&format!("i32.const {}", entry_index));
                module.body().line("call $__rt_assoc_entry");
                module.body().line(&format!("local.set {}", entry));
                module.body().line(&format!("local.get {}", entry));
                module.body().line("call $__rt_assoc_value_cell");
                module.body().line(&format!("local.set {}", cell));
                return emit_value_cell_result_from_cell(&cell, kind, module);
            }
            let Some(kind) = homogeneous_nested_assoc_value_kind(&metadata) else {
                return Err(nested_array_access_unsupported(expr));
            };
            emit_dynamic_nested_assoc_index_scan(&ptr, &len, kind, module, |entry, matched, module| {
                emit_assoc_entry_matches_assoc_key_value(entry, &key, matched, module);
            })
        }
    }
}

fn emit_static_property_nested_array_source_parts(
    array: &Expr,
    ptr: &str,
    len: &str,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let ExprKind::ArrayAccess {
        array: parent,
        index: parent_index,
    } = &array.kind
    else {
        return Ok(false);
    };
    let ExprKind::StaticPropertyAccess { receiver, property } = &parent.kind else {
        return Ok(false);
    };
    let property_info = module
        .object_static_property(receiver, property)
        .ok_or_else(|| nested_array_access_unsupported(array))?;
    if !module.object_member_is_accessible(&property_info.owner_class, &property_info.visibility) {
        return Err(nested_array_access_unsupported(array));
    }
    let key = static_assoc_access_key(parent_index, module)
        .ok_or_else(|| nested_array_access_unsupported(array))?;
    let cell = module.next_label("static_property_nested_read_cell");
    let parent_ptr = module.next_label("static_property_nested_read_ptr");
    let parent_len = module.next_label("static_property_nested_read_len");
    for local in [&cell, &parent_ptr, &parent_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    match emit_static_property_access_expr(parent, receiver, property, module)? {
        ValueKind::Mixed => {
            module.body().line(&format!("local.set {}", cell));
        }
        _ => return Err(nested_array_access_unsupported(array)),
    }
    module.body().line(&format!("local.get {}", cell));
    module.body().line("call $__rt_mixed_array_ptr");
    module.body().line(&format!("local.set {}", parent_ptr));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("call $__rt_mixed_array_len_i32");
    module.body().line(&format!("local.set {}", parent_len));
    let (child_ptr, child_len) =
        emit_static_assoc_array_child_parts(&parent_ptr, &parent_len, &key, module);
    module.body().line(&format!("local.get {}", child_ptr));
    module.body().line(&format!("local.set {}", ptr));
    module.body().line(&format!("local.get {}", child_len));
    module.body().line(&format!("local.set {}", len));
    Ok(true)
}

fn emit_partial_static_nested_assoc_index_expr(
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    let ExprKind::ArrayAccess {
        array: parent,
        index: middle_index,
    } = &array.kind
    else {
        return Ok(None);
    };
    let Some(middle_key) = static_assoc_access_key(middle_index, module) else {
        return Ok(None);
    };
    let Some(leaf_key) = static_assoc_access_key(index, module) else {
        return Ok(None);
    };
    let Some(parent_metadata) = nested_array_metadata_for_access_expr(parent, module) else {
        return Ok(None);
    };
    if parent_metadata.layout != ArrayLayout::Assoc {
        return Ok(None);
    }
    let Some(keys) = parent_metadata.key_values.as_ref() else {
        return Ok(None);
    };
    if keys.len() == parent_metadata.len || keys.iter().any(|key| *key == middle_key) {
        return Ok(None);
    }
    let Some(leaf_metadata) = parent_metadata
        .nested_values
        .as_ref()
        .and_then(|values| values.iter().skip(keys.len()).flatten().find(|metadata| {
            metadata.layout == ArrayLayout::Assoc
                && nested_assoc_static_key_kind(metadata, &leaf_key).is_some()
        }))
    else {
        return Ok(None);
    };
    let Some(kind) = nested_assoc_static_key_kind(leaf_metadata, &leaf_key) else {
        return Ok(None);
    };
    let parent_ptr = module.next_label("partial_nested_parent_ptr");
    let parent_len = module.next_label("partial_nested_parent_len");
    module.declare_i32_local(parent_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(parent_len.trim_start_matches('$').to_string());
    match emit_expr(parent, module)? {
        ValueKind::Array => {
            module.body().line(&format!("local.set {}", parent_len));
            module.body().line(&format!("local.set {}", parent_ptr));
        }
        _ => return Ok(None),
    }
    let (child_ptr, child_len) =
        emit_static_assoc_array_child_parts(&parent_ptr, &parent_len, &middle_key, module);
    emit_dynamic_nested_assoc_index_scan(&child_ptr, &child_len, kind, module, |entry, matched, module| {
        emit_assoc_entry_matches_assoc_key_value(entry, &leaf_key, matched, module);
    })
    .map(Some)
}

pub(super) fn emit_static_assoc_array_child_parts(
    ptr: &str,
    len: &str,
    key: &AssocKeyValue,
    module: &mut WasmModule,
) -> (String, String) {
    let scan_index = module.next_label("partial_nested_child_index");
    let entry = module.next_label("partial_nested_child_entry");
    let matched = module.next_label("partial_nested_child_matched");
    let cell = module.next_label("partial_nested_child_cell");
    let found = module.next_label("partial_nested_child_found");
    let child_ptr = module.next_label("partial_nested_child_ptr");
    let child_len = module.next_label("partial_nested_child_len");
    let done_label = module.next_label("partial_nested_child_done");
    let loop_label = module.next_label("partial_nested_child_loop");
    for local in [
        &scan_index,
        &entry,
        &matched,
        &cell,
        &found,
        &child_ptr,
        &child_len,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", scan_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", cell));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", scan_index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", scan_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    emit_assoc_entry_matches_assoc_key_value(&entry, key, &matched, module);
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", scan_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", child_ptr));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", child_len));
    (child_ptr, child_len)
}

fn emit_dynamic_parent_nested_assoc_index_expr(
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    let Some(metadata) = dynamic_nested_array_value_metadata(array, module) else {
        return Ok(None);
    };
    if metadata.layout != ArrayLayout::Assoc {
        return Ok(None);
    }
    let ptr = module.next_label("dynamic_parent_nested_assoc_ptr");
    let len = module.next_label("dynamic_parent_nested_assoc_len");
    module.declare_i32_local(ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(len.trim_start_matches('$').to_string());
    match emit_expr(array, module)? {
        ValueKind::Array => {
            module.body().line(&format!("local.set {}", len));
            module.body().line(&format!("local.set {}", ptr));
        }
        _ => return Ok(None),
    }
    if let Some(inner_key) = static_assoc_access_key(index, module) {
        let Some(kind) = nested_assoc_static_key_kind(&metadata, &inner_key) else {
            return Ok(None);
        };
        return emit_dynamic_nested_assoc_index_scan(&ptr, &len, kind, module, |entry, matched, module| {
            emit_assoc_entry_matches_assoc_key_value(entry, &inner_key, matched, module);
        })
        .map(Some);
    }
    let Some(kind) = homogeneous_nested_assoc_value_kind(&metadata) else {
        return Ok(None);
    };
    if let Some(key) = runtime_string_arg_or_materialize(
        index,
        "dynamic_parent_nested_assoc_key",
        module,
    )? {
        return emit_dynamic_nested_assoc_string_index_expr(
            &ptr,
            &len,
            &format!("${}_ptr", key),
            &format!("${}_len", key),
            kind,
            module,
        )
        .map(Some);
    }
    if expression_is_inty(index, module) {
        let key = module.next_label("dynamic_parent_nested_assoc_int_key");
        module.declare_i64_local(key.trim_start_matches('$').to_string());
        require_int(index, module)?;
        module.body().line(&format!("local.set {}", key));
        return emit_dynamic_nested_assoc_int_index_expr(&ptr, &len, &key, kind, module).map(Some);
    }
    Ok(None)
}

fn emit_dynamic_outer_nested_assoc_index_expr(
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    let Some((outer_name, outer_index, metadata)) = dynamic_outer_nested_assoc_metadata(array, module)
    else {
        return Ok(None);
    };
    if metadata.layout != ArrayLayout::Assoc {
        return Ok(None);
    }
    let Some((ptr, len)) =
        emit_dynamic_outer_assoc_nested_array_parts(&outer_name, outer_index, module)?
    else {
        return Ok(None);
    };
    if let Some(inner_key) = static_assoc_access_key(index, module) {
        let Some(kind) = nested_assoc_static_key_kind(&metadata, &inner_key) else {
            return Ok(None);
        };
        return emit_dynamic_nested_assoc_index_scan(&ptr, &len, kind, module, |entry, matched, module| {
            emit_assoc_entry_matches_assoc_key_value(entry, &inner_key, matched, module);
        })
        .map(Some);
    }
    let Some(kind) = homogeneous_nested_assoc_value_kind(&metadata) else {
        return Ok(None);
    };
    if let Some(key) = runtime_string_arg_or_materialize(
        index,
        "dynamic_outer_inner_nested_assoc_key",
        module,
    )? {
        return emit_dynamic_nested_assoc_string_index_expr(
            &ptr,
            &len,
            &format!("${}_ptr", key),
            &format!("${}_len", key),
            kind,
            module,
        )
        .map(Some);
    }
    if expression_is_inty(index, module) {
        let key = module.next_label("dynamic_outer_inner_nested_assoc_int_key");
        module.declare_i64_local(key.trim_start_matches('$').to_string());
        require_int(index, module)?;
        module.body().line(&format!("local.set {}", key));
        return emit_dynamic_nested_assoc_int_index_expr(&ptr, &len, &key, kind, module).map(Some);
    }
    Ok(None)
}

pub(super) fn dynamic_outer_nested_assoc_static_access_kind(
    array: &Expr,
    index: &Expr,
    module: &WasmModule,
) -> Option<ValueCellKind> {
    let (_, _, metadata) = dynamic_outer_nested_assoc_metadata(array, module)?;
    let key = static_assoc_access_key(index, module)?;
    nested_assoc_static_key_kind(&metadata, &key)
}

fn nested_assoc_static_key_kind(
    metadata: &NestedArrayMetadata,
    key: &AssocKeyValue,
) -> Option<ValueCellKind> {
    if metadata.layout != ArrayLayout::Assoc {
        return None;
    }
    let keys = metadata.key_values.as_ref()?;
    let values = metadata.value_kinds.as_ref()?;
    let entry_index = keys.iter().position(|candidate| candidate == key)?;
    values.get(entry_index).copied()
}

pub(super) fn dynamic_outer_nested_assoc_metadata<'a>(
    array: &'a Expr,
    module: &WasmModule,
) -> Option<(String, &'a Expr, NestedArrayMetadata)> {
    let ExprKind::ArrayAccess {
        array: outer_array,
        index: outer_index,
    } = &array.kind
    else {
        return None;
    };
    let ExprKind::Variable(outer_name) = &outer_array.kind else {
        return None;
    };
    if module.local_kind(outer_name) != Some(LocalKind::Array)
        || module.array_layout(outer_name) != ArrayLayout::Assoc
        || static_assoc_access_key(outer_index, module).is_some()
    {
        return None;
    }
    let mut metadata = module
        .array_nested_value_metadata_items(outer_name)?
        .iter()
        .cloned()
        .collect::<Option<Vec<_>>>()?
        .into_iter();
    let first = metadata.next()?;
    if metadata.all(|candidate| candidate == first) {
        Some((outer_name.clone(), outer_index, first))
    } else {
        None
    }
}

pub(super) fn dynamic_nested_array_value_metadata(
    array: &Expr,
    module: &WasmModule,
) -> Option<NestedArrayMetadata> {
    let ExprKind::ArrayAccess {
        array: parent,
        index,
    } = &array.kind
    else {
        return None;
    };
    if static_assoc_access_key(index, module).is_some() {
        return None;
    }
    let parent_metadata = nested_array_metadata_for_access_expr(parent, module).or_else(|| {
        dynamic_outer_nested_assoc_metadata(parent, module).map(|(_, _, metadata)| metadata)
    })?;
    if parent_metadata.layout != ArrayLayout::Assoc
        || homogeneous_nested_assoc_value_kind(&parent_metadata) != Some(ValueCellKind::Array)
    {
        return None;
    }
    common_nested_child_metadata(&parent_metadata)
}

fn common_nested_child_metadata(metadata: &NestedArrayMetadata) -> Option<NestedArrayMetadata> {
    let mut nested = metadata
        .nested_values
        .as_ref()?
        .iter()
        .cloned()
        .collect::<Option<Vec<_>>>()?
        .into_iter();
    let first = nested.next()?;
    nested.all(|candidate| candidate == first).then_some(first)
}

pub(super) fn emit_dynamic_outer_assoc_nested_array_parts(
    outer_name: &str,
    outer_index: &Expr,
    module: &mut WasmModule,
) -> Result<Option<(String, String)>, CompileError> {
    if let Some(key) = runtime_string_arg_or_materialize(
        outer_index,
        "dynamic_outer_nested_assoc_key",
        module,
    )? {
        let key_ptr = format!("${}_ptr", key);
        let key_len = format!("${}_len", key);
        return Ok(Some(emit_dynamic_outer_assoc_nested_array_scan(
            outer_name,
            module,
            |entry, matched, module| {
                emit_assoc_entry_matches_string_parts(entry, &key_ptr, &key_len, matched, module);
            },
        )));
    }
    if expression_is_inty(outer_index, module) {
        let key = module.next_label("dynamic_outer_nested_assoc_int_key");
        module.declare_i64_local(key.trim_start_matches('$').to_string());
        require_int(outer_index, module)?;
        module.body().line(&format!("local.set {}", key));
        return Ok(Some(emit_dynamic_outer_assoc_nested_array_scan(
            outer_name,
            module,
            |entry, matched, module| {
                module.body().line(&format!("local.get {}", entry));
                module.body().line(&format!("local.get {}", key));
                module.body().line("call $__rt_assoc_key_eq_int");
                module.body().line(&format!("local.set {}", matched));
            },
        )));
    }
    Ok(None)
}

fn emit_dynamic_outer_assoc_nested_array_scan(
    outer_name: &str,
    module: &mut WasmModule,
    emit_match: impl Fn(&str, &str, &mut WasmModule),
) -> (String, String) {
    let index = module.next_label("dynamic_outer_nested_assoc_index");
    let entry = module.next_label("dynamic_outer_nested_assoc_entry");
    let matched = module.next_label("dynamic_outer_nested_assoc_matched");
    let cell = module.next_label("dynamic_outer_nested_assoc_cell");
    let ptr = module.next_label("dynamic_outer_nested_assoc_ptr");
    let len = module.next_label("dynamic_outer_nested_assoc_len");
    let done_label = module.next_label("dynamic_outer_nested_assoc_done");
    let loop_label = module.next_label("dynamic_outer_nested_assoc_loop");
    for local in [&index, &entry, &matched, &cell, &ptr, &len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", len));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", outer_name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", outer_name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    emit_match(&entry, &matched, module);
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
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
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    (ptr, len)
}

pub(in crate::codegen::wasm::expr) fn emit_assoc_entry_matches_assoc_key_value(
    source_entry: &str,
    key: &AssocKeyValue,
    key_found: &str,
    module: &mut WasmModule,
) {
    match key {
        AssocKeyValue::Int(value) => {
            module.body().line(&format!("local.get {}", source_entry));
            module.body().line(&format!("i64.const {}", value));
            module.body().line("call $__rt_assoc_key_eq_int");
            module.body().line(&format!("local.set {}", key_found));
        }
        AssocKeyValue::Str(value) => {
            let (ptr, len) = module.intern_string(value);
            module.body().line(&format!("local.get {}", source_entry));
            module.body().line(&format!("i32.const {}", ptr));
            module.body().line(&format!("i32.const {}", len));
            module.body().line("call $__rt_assoc_key_eq_php_string");
            module.body().line(&format!("local.set {}", key_found));
        }
    }
}

fn emit_dynamic_nested_assoc_index_expr(
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    if static_assoc_access_key(index, module).is_some() {
        return Ok(None);
    }
    let Some(metadata) = nested_array_metadata_for_access_expr(array, module) else {
        return Ok(None);
    };
    if metadata.layout != ArrayLayout::Assoc {
        return Ok(None);
    }
    let Some(kind) = homogeneous_nested_assoc_value_kind(&metadata) else {
        return Ok(None);
    };

    let ptr = module.next_label("dynamic_nested_assoc_ptr");
    let len = module.next_label("dynamic_nested_assoc_len");
    module.declare_i32_local(ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(len.trim_start_matches('$').to_string());
    match emit_expr(array, module)? {
        ValueKind::Array => {
            module.body().line(&format!("local.set {}", len));
            module.body().line(&format!("local.set {}", ptr));
        }
        _ => return Ok(None),
    }

    if let Some(key) = runtime_string_arg_or_materialize(index, "dynamic_nested_assoc_key", module)? {
        return emit_dynamic_nested_assoc_string_index_expr(
            &ptr,
            &len,
            &format!("${}_ptr", key),
            &format!("${}_len", key),
            kind,
            module,
        )
        .map(Some);
    }
    if expression_is_inty(index, module) {
        let key = module.next_label("dynamic_nested_assoc_int_key");
        module.declare_i64_local(key.trim_start_matches('$').to_string());
        require_int(index, module)?;
        module.body().line(&format!("local.set {}", key));
        return emit_dynamic_nested_assoc_int_index_expr(&ptr, &len, &key, kind, module).map(Some);
    }
    Ok(None)
}

pub(super) fn homogeneous_nested_assoc_value_kind(metadata: &NestedArrayMetadata) -> Option<ValueCellKind> {
    let values = metadata.value_kinds.as_ref()?;
    let first = values.first().copied()?;
    values.iter().all(|kind| *kind == first).then_some(first)
}

fn emit_dynamic_nested_assoc_string_index_expr(
    ptr: &str,
    len: &str,
    key_ptr: &str,
    key_len: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    emit_dynamic_nested_assoc_index_scan(ptr, len, kind, module, |entry, matched, module| {
        emit_assoc_entry_matches_string_parts(entry, key_ptr, key_len, matched, module);
    })
}

fn emit_dynamic_nested_assoc_int_index_expr(
    ptr: &str,
    len: &str,
    key: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    emit_dynamic_nested_assoc_index_scan(ptr, len, kind, module, |entry, matched, module| {
        module.body().line(&format!("local.get {}", entry));
        module.body().line(&format!("local.get {}", key));
        module.body().line("call $__rt_assoc_key_eq_int");
        module.body().line(&format!("local.set {}", matched));
    })
}

fn emit_dynamic_nested_assoc_index_scan(
    ptr: &str,
    len: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
    emit_match: impl Fn(&str, &str, &mut WasmModule),
) -> Result<ValueKind, CompileError> {
    let index = module.next_label("dynamic_nested_assoc_index");
    let entry = module.next_label("dynamic_nested_assoc_entry");
    let cell = module.next_label("dynamic_nested_assoc_cell");
    let found = module.next_label("dynamic_nested_assoc_found");
    let done_label = module.next_label("dynamic_nested_assoc_done");
    let loop_label = module.next_label("dynamic_nested_assoc_loop");
    for local in [&index, &entry, &cell, &found] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    emit_match(&entry, &found, module);
    module.body().line(&format!("local.get {}", found));
    module.body().open("if");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    emit_assoc_scalar_result_from_cell(&cell, &found, kind, module)
}

pub(super) fn emit_output_nested_array_index(
    expr: &Expr,
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if mixed_key_local_name(index, module).is_some()
        || dynamic_nested_assoc_access_needs_mixed_cell(array, index, module)
    {
        let temp = module
            .next_label("output_dynamic_nested_assoc_mixed")
            .trim_start_matches('$')
            .to_string();
        module.declare_i32_local(temp.clone());
        emit_alloc_mixed_cell(&temp, module);
        emit_store_value_cell(&format!("${}", temp), expr, module)?;
        emit_output_value_cell(&format!("${}", temp), module);
        return Ok(());
    }
    match emit_nested_array_index_expr(expr, array, index, module)? {
        ValueKind::Int => module.body().line("call $host_write_int"),
        ValueKind::Float => module.body().line("call $host_write_float"),
        ValueKind::Bool => {
            module.body().open("if");
            module.body().line("i64.const 1");
            module.body().line("call $host_write_int");
            module.body().close("end");
        }
        ValueKind::Str => module.body().line("call $host_write"),
        ValueKind::Array => emit_output_array_marker(module),
        ValueKind::Object => {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web object array elements are not supported in output yet",
            ));
        }
        ValueKind::Null => module.body().line("drop"),
        ValueKind::Mixed => emit_output_value_cell_stack(module),
        ValueKind::Never => module.body().line("unreachable"),
    }
    Ok(())
}

pub(super) fn dynamic_nested_assoc_access_needs_mixed_cell(
    array: &Expr,
    index: &Expr,
    module: &WasmModule,
) -> bool {
    if heterogeneous_dynamic_middle_assoc_leaf_needs_mixed_cell(array, module) {
        return true;
    }
    let static_key = static_assoc_access_key(index, module);
    if let Some((_, _, metadata)) = dynamic_outer_nested_assoc_metadata(array, module) {
        return metadata.layout == ArrayLayout::Assoc
            && homogeneous_nested_assoc_value_kind(&metadata).is_none()
            && (static_key.is_none() || metadata.key_values.is_none());
    }
    let Some(metadata) = nested_array_metadata_for_access_expr(array, module)
        .or_else(|| dynamic_nested_array_value_metadata(array, module))
    else {
        return false;
    };
    metadata.layout == ArrayLayout::Assoc
        && homogeneous_nested_assoc_value_kind(&metadata).is_none()
        && (static_key.is_none() || metadata.key_values.is_none())
}

fn heterogeneous_dynamic_middle_assoc_leaf_needs_mixed_cell(
    array: &Expr,
    module: &WasmModule,
) -> bool {
    let ExprKind::ArrayAccess {
        array: parent,
        index: middle_index,
    } = &array.kind
    else {
        return false;
    };
    if static_assoc_access_key(middle_index, module).is_some() {
        return false;
    }
    let Some(parent_metadata) = nested_array_metadata_for_access_expr(parent, module) else {
        return false;
    };
    parent_metadata.layout == ArrayLayout::Assoc
        && homogeneous_nested_assoc_value_kind(&parent_metadata) == Some(ValueCellKind::Array)
        && parent_metadata
            .nested_values
            .as_ref()
            .is_some_and(|children| {
                children
                    .iter()
                    .any(|child| child.as_ref().is_some_and(|child| child.layout == ArrayLayout::Assoc))
            })
}
