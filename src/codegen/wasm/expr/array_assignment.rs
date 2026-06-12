//! Purpose:
//! Hosts wasm32-web helpers for associative and indexed array assignment lowering.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` and sibling wasm expression modules.
//!
//! Key details:
//! - Dynamic associative keys are materialized once and reused by assignment,
//!   key-exists, nested traversal, and mutation helpers.
//! - Entry copy helpers preserve the runtime hash-array/value-cell layout.

use super::*;

pub(in crate::codegen::wasm) fn emit_assoc_array_items_assign(
    name: &str,
    items: &[(Expr, Expr)],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let normalized_items = normalize_assoc_items(items);
    let items = normalized_items.as_deref().unwrap_or(items);
    emit_release_current_assoc_array(name, module);
    module.set_array_length(name, items.len());
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(name, value_cell_kinds_for_assoc_items(items, module));
    module.set_array_value_constants(name, value_cell_constants_for_assoc_items(items, module));
    module.set_array_nested_value_metadata(
        name,
        nested_array_metadata_for_assoc_items(items, module),
    );
    module.set_array_object_classes(name, object_classes_for_assoc_items(items, module));
    module.set_array_key_kinds(name, key_kinds_for_assoc_items(items));
    module.set_array_key_values(name, key_values_for_assoc_items(items));
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line(&format!("local.set ${}_len", name));
    for (index, (key, value)) in items.iter().enumerate() {
        emit_assoc_array_store_key(name, index, key, module)?;
        emit_assoc_array_store_value(name, index, value, module)?;
    }
    Ok(())
}

fn object_classes_for_assoc_items(
    items: &[(Expr, Expr)],
    module: &WasmModule,
) -> Option<Vec<Option<String>>> {
    let classes = items
        .iter()
        .map(|(_, value)| object_class_name_for_expr(value, module))
        .collect::<Vec<_>>();
    classes.iter().any(Option::is_some).then_some(classes)
}

pub(in crate::codegen::wasm) fn normalize_assoc_items(
    items: &[(Expr, Expr)],
) -> Option<Vec<(Expr, Expr)>> {
    let mut normalized: Vec<(AssocKeyValue, Expr, Expr)> = Vec::new();
    for (key, value) in items {
        let key_value = assoc_key_value_for_expr(key)?;
        if let Some((_, _, existing_value)) = normalized
            .iter_mut()
            .find(|(existing_key, _, _)| *existing_key == key_value)
        {
            *existing_value = value.clone();
        } else {
            normalized.push((key_value, key.clone(), value.clone()));
        }
    }
    Some(
        normalized
            .into_iter()
            .map(|(_, key, value)| (key, value))
            .collect(),
    )
}

pub(in crate::codegen::wasm) fn key_values_for_assoc_items(
    items: &[(Expr, Expr)],
) -> Option<Vec<AssocKeyValue>> {
    items
        .iter()
        .map(|(key, _)| assoc_key_value_for_expr(key))
        .collect()
}

pub(in crate::codegen::wasm) fn assoc_key_value_for_expr(key: &Expr) -> Option<AssocKeyValue> {
    match &key.kind {
        ExprKind::IntLiteral(value) => Some(AssocKeyValue::Int(*value)),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => value.checked_neg().map(AssocKeyValue::Int),
            _ => None,
        },
        ExprKind::StringLiteral(value) => literal_php_array_int_key(value)
            .map(AssocKeyValue::Int)
            .or_else(|| Some(AssocKeyValue::Str(value.clone()))),
        _ => None,
    }
}

pub(in crate::codegen::wasm) fn emit_release_current_assoc_array(
    name: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("call $__rt_assoc_array_release");
}

pub(in crate::codegen::wasm) fn emit_assoc_array_store_key(
    name: &str,
    index: usize,
    key: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match &key.kind {
        ExprKind::IntLiteral(value) => {
            emit_assoc_array_store_int_key(name, index, *value, module);
            Ok(())
        }
        ExprKind::Negate(inner) if matches!(inner.kind, ExprKind::IntLiteral(_)) => {
            let ExprKind::IntLiteral(value) = &inner.kind else {
                unreachable!("guarded by matches");
            };
            let Some(value) = (*value).checked_neg() else {
                return Err(CompileError::new(
                    key.span,
                    "wasm32-web associative array literals cannot represent this negative integer key",
                ));
            };
            emit_assoc_array_store_int_key(name, index, value, module);
            Ok(())
        }
        ExprKind::StringLiteral(value) => {
            if let Some(value) = literal_php_array_int_key(value) {
                emit_assoc_array_store_int_key(name, index, value, module);
                return Ok(());
            }
            let entry = module.next_label("assoc_array_string_key_entry");
            let (ptr, len) = module.intern_string(value);
            module.declare_i32_local(entry.trim_start_matches('$').to_string());
            emit_assoc_entry_address_const_index(&format!("${}_ptr", name), index, &entry, module);
            module.body().line(&format!("local.get {}", entry));
            module.body().line(&format!("i32.const {}", ptr));
            module.body().line(&format!("i32.const {}", len));
            module.body().line("call $__rt_assoc_store_string_key");
            Ok(())
        }
        _ => Err(CompileError::new(
            key.span,
            "wasm32-web associative array literals require static integer or string keys",
        )),
    }
}

pub(in crate::codegen::wasm) fn emit_assoc_array_store_int_key(
    name: &str,
    index: usize,
    key: i64,
    module: &mut WasmModule,
) {
    let entry = module.next_label("assoc_array_int_key_entry");
    module.declare_i32_local(entry.trim_start_matches('$').to_string());
    emit_assoc_entry_address_const_index(&format!("${}_ptr", name), index, &entry, module);
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("i64.const {}", key));
    module.body().line("call $__rt_assoc_store_int_key");
}

pub(in crate::codegen::wasm) fn emit_assoc_array_store_int_slot_value(
    name: &str,
    index: usize,
    source_ptr: &str,
    source_index: usize,
    module: &mut WasmModule,
) {
    let cell = module.next_label("assoc_array_int_slot_cell");
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("i32.const {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("i32.const {}", source_index * 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("call $__rt_value_store_int");
}

pub(in crate::codegen::wasm) fn emit_assoc_array_copy_value_cell_from_indexed(
    name: &str,
    index: usize,
    source_ptr: &str,
    source_index: usize,
    module: &mut WasmModule,
) {
    let cell = module.next_label("assoc_array_indexed_value_cell");
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("i32.const {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("i32.const {}", source_index));
    module.body().line("call $__rt_value_cell");
    module.body().line("call $__rt_value_copy");
}

pub(super) fn emit_assoc_array_store_value(
    name: &str,
    index: usize,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let cell = module.next_label("assoc_array_value_cell");
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("i32.const {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_store_value_cell(&cell, value, module)
}

pub(in crate::codegen::wasm) fn emit_assoc_array_store_static_string_value(
    name: &str,
    index: usize,
    value: &str,
    module: &mut WasmModule,
) {
    let cell = module.next_label("assoc_array_string_cell");
    let (ptr, len) = module.intern_string(value);
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("i32.const {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("i32.const {}", ptr));
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_value_store_string");
}

pub(in crate::codegen::wasm) fn emit_assoc_array_store_string_range_value(
    name: &str,
    index: usize,
    source: &str,
    start: &str,
    end: &str,
    module: &mut WasmModule,
) {
    let cell = module.next_label("assoc_array_range_cell");
    let part_ptr = module.next_label("assoc_array_range_ptr");
    let part_len = module.next_label("assoc_array_range_len");
    let copy_index = module.next_label("assoc_array_range_copy_index");
    let copy_loop = module.next_label("assoc_array_range_copy_loop");
    let copy_done = module.next_label("assoc_array_range_copy_done");
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.declare_i32_local(part_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(part_len.trim_start_matches('$').to_string());
    module.declare_i32_local(copy_index.trim_start_matches('$').to_string());

    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", part_len));
    module.body().line(&format!("local.get {}", part_len));
    module.body().line("call $__rt_alloc_bytes");
    module.body().line(&format!("local.set {}", part_ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", copy_index));
    module.body().open(&format!("block {}", copy_done));
    module.body().open(&format!("loop {}", copy_loop));
    module.body().line(&format!("local.get {}", copy_index));
    module.body().line(&format!("local.get {}", part_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", copy_done));
    module.body().line(&format!("local.get {}", part_ptr));
    module.body().line(&format!("local.get {}", copy_index));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.get {}", copy_index));
    module.body().line("i32.add");
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.store8");
    module.body().line(&format!("local.get {}", copy_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", copy_index));
    module.body().line(&format!("br {}", copy_loop));
    module.body().close("end");
    module.body().close("end");

    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("i32.const {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("local.get {}", part_ptr));
    module.body().line(&format!("local.get {}", part_len));
    module.body().line("call $__rt_value_store_string");
}

pub(in crate::codegen::wasm) fn emit_array_int_assign(
    name: &str,
    index: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(name) == ArrayLayout::Value {
        return emit_value_array_index_assign(name, index, value, module);
    }
    let Some(len) = module.array_length(name) else {
        return Err(CompileError::new(
            index.span,
            "wasm32-web array assignment requires a known indexed array length",
        ));
    };
    let Some(index_value) = static_or_const_or_i64_local_value(index, module) else {
        return emit_dynamic_compact_array_sparse_int_assign(name, len, index, value, module);
    };
    let Ok(index_usize) = usize::try_from(index_value) else {
        return emit_compact_array_sparse_int_assign(name, len, index_value, value, module);
    };
    if index_usize >= len {
        return emit_compact_array_sparse_int_assign(name, len, index_value, value, module);
    }
    emit_ensure_unique_array_payload(name, module);
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("i32.const {}", index_usize * 8));
    module.body().line("i32.add");
    require_int(value, module)?;
    module.body().line("i64.store");
    Ok(())
}

pub(super) use super::array_assignment_sparse::*;

pub(in crate::codegen::wasm) fn emit_assoc_array_assign(
    name: &str,
    index: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let key = assoc_assign_key(index, module)?;
    let len = module.array_length(name).ok_or_else(|| {
        CompileError::new(
            index.span,
            "wasm32-web associative array assignment requires a known array length",
        )
    })?;
    let old_key_kinds = module.array_key_kinds(name).map(|kinds| kinds.to_vec());
    let old_key_values = module.array_key_values(name).map(|values| values.to_vec());
    let old_value_kinds = module.array_value_cell_kinds(name).map(|kinds| kinds.to_vec());
    let old_ptr = preserve_array_ptr(name, "assoc_array_assign", module);
    let source_len = module.next_label("assoc_assign_source_len");
    let source_index = module.next_label("assoc_assign_source_index");
    let out_index = module.next_label("assoc_assign_out_index");
    let source_entry = module.next_label("assoc_assign_source_entry");
    let target_entry = module.next_label("assoc_assign_target_entry");
    let matched = module.next_label("assoc_assign_matched");
    let found = module.next_label("assoc_assign_found");
    let value_cell = module.next_label("assoc_assign_value_cell");
    let done_label = module.next_label("assoc_assign_done");
    let loop_label = module.next_label("assoc_assign_loop");
    for local in [
        &source_index,
        &source_len,
        &out_index,
        &source_entry,
        &target_entry,
        &matched,
        &found,
        &value_cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line(&format!("local.set {}", source_len));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_assoc_entry_address(&old_ptr, &source_index, &source_entry, module);
    emit_assoc_entry_address(&format!("${}_ptr", name), &out_index, &target_entry, module);
    emit_assoc_entry_matches_assign_key(&source_entry, &key, &matched, module);
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    copy_assoc_entry_key(&target_entry, &source_entry, module);
    emit_assoc_value_cell_address(&target_entry, &value_cell, module);
    emit_store_value_cell(&value_cell, value, module)?;
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line("else");
    copy_assoc_entry(&target_entry, &source_entry, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_assoc_entry_address(&format!("${}_ptr", name), &out_index, &target_entry, module);
    emit_store_assoc_assign_key_at_entry(&target_entry, &key, module);
    emit_assoc_value_cell_address(&target_entry, &value_cell, module);
    emit_store_value_cell(&value_cell, value, module)?;
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_index));
    module.body().line(&format!("local.set ${}_len", name));
    module.set_array_layout(name, ArrayLayout::Assoc);
    if let (
        Some((assigned_key_kind, assigned_key_value)),
        Some(mut key_kinds),
        Some(mut key_values),
        Some(mut value_kinds),
        Some(value_kind),
    ) = (
        static_assoc_assign_key_metadata(&key),
        old_key_kinds,
        old_key_values,
        old_value_kinds,
        value_cell_kind_for_expr(value, module),
    ) {
        if let Some(index) = key_values.iter().position(|candidate| *candidate == assigned_key_value) {
            value_kinds[index] = value_kind;
            module.set_array_length(name, len);
        } else {
            key_kinds.push(assigned_key_kind);
            key_values.push(assigned_key_value);
            value_kinds.push(value_kind);
            module.set_array_length(name, len + 1);
        }
        module.set_array_key_kinds(name, Some(key_kinds));
        module.set_array_key_values(name, Some(key_values));
        module.set_array_value_cell_kinds(name, Some(value_kinds));
    } else {
        module.set_array_length(name, len + 1);
        module.set_array_value_cell_kinds(name, None);
        module.set_array_key_kinds(name, None);
    }
    Ok(())
}

pub(super) use super::array_assignment_assoc_key::*;
