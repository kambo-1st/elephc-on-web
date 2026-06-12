//! Purpose:
//! Emits wasm32-web sparse indexed and value-array assignment helpers.
//! Handles promotion from indexed payloads to associative arrays on sparse writes.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_assignment` through its re-export.
//!
//! Key details:
//! - Sparse integer writes preserve PHP integer-key semantics by promoting to assoc layout.
//! - Value-array paths copy boxed cells through runtime helpers to preserve ownership.

use super::*;
use super::array_assignment::emit_assoc_array_store_value;

pub(super) fn emit_compact_array_sparse_int_assign(
    name: &str,
    len: usize,
    key: i64,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if value_cell_kind_for_expr(value, module) != Some(ValueCellKind::Int) {
        return Err(CompileError::new(
            value.span,
            "wasm32-web sparse compact-array assignment currently supports integer values only",
        ));
    }
    let source_ptr = preserve_array_ptr(name, "sparse_assign_source", module);
    let out_len = len + 1;
    module.body().line(&format!("i32.const {}", out_len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", out_len));
    module.body().line(&format!("local.set ${}_len", name));
    module.set_array_length(name, out_len);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_key_kinds(name, Some(vec![AssocKeyKind::Int; out_len]));
    module.set_array_key_values(
        name,
        Some(
            (0..len)
                .map(|index| AssocKeyValue::Int(index as i64))
                .chain(std::iter::once(AssocKeyValue::Int(key)))
                .collect(),
        ),
    );
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Int; out_len]));
    for index in 0..len {
        emit_assoc_array_store_int_key(name, index, index as i64, module);
        emit_assoc_array_store_int_slot_value(name, index, &source_ptr, index, module);
    }
    emit_assoc_array_store_int_key(name, len, key, module);
    emit_assoc_array_store_value(name, len, value, module)?;
    Ok(())
}

pub(super) fn emit_dynamic_compact_array_sparse_int_assign(
    name: &str,
    len: usize,
    index: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let old_ptr = preserve_array_ptr(name, "dynamic_compact_sparse_assign", module);
    let index64 = module.next_label("dynamic_compact_sparse_key");
    let source_index = module.next_label("dynamic_compact_sparse_source_index");
    let out_index = module.next_label("dynamic_compact_sparse_out_index");
    let target_entry = module.next_label("dynamic_compact_sparse_target_entry");
    let found = module.next_label("dynamic_compact_sparse_found");
    let value_cell = module.next_label("dynamic_compact_sparse_value_cell");
    let done_label = module.next_label("dynamic_compact_sparse_done");
    let loop_label = module.next_label("dynamic_compact_sparse_loop");
    module.declare_i64_local(index64.trim_start_matches('$').to_string());
    for local in [&source_index, &out_index, &target_entry, &found, &value_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    require_int(index, module)?;
    module.body().line(&format!("local.set {}", index64));
    module.body().line(&format!("i32.const {}", len + 1));
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
    module.body().line(&format!("i32.const {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i64.extend_i32_u");
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i64.extend_i32_u");
    module.body().line(&format!("local.get {}", index64));
    module.body().line("i64.eq");
    module.body().open("if");
    emit_assoc_value_cell_address(&target_entry, &value_cell, module);
    emit_store_value_cell(&value_cell, value, module)?;
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line("else");
    emit_assoc_value_cell_address(&target_entry, &value_cell, module);
    module.body().line(&format!("local.get {}", value_cell));
    module.body().line(&format!("local.get {}", old_ptr));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("call $__rt_value_store_int");
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
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", index64));
    module.body().line("call $__rt_assoc_store_int_key");
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
    module.clear_array_length(name);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Int; len + 1]));
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    Ok(())
}

pub(super) fn emit_value_array_index_assign(
    name: &str,
    index: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(name) else {
        return Err(CompileError::new(
            index.span,
            "wasm32-web value-array assignment requires a known indexed array length",
        ));
    };
    let Some(index_value) = static_or_const_or_i64_local_value(index, module) else {
        return emit_dynamic_value_array_index_assign(name, index, value, len, module);
    };
    let Ok(index_usize) = usize::try_from(index_value) else {
        return emit_value_array_sparse_assign(name, len, index_value, value, module);
    };
    if index_usize >= len {
        return emit_value_array_sparse_assign(name, len, index_value, value, module);
    }
    emit_ensure_unique_array_payload(name, module);
    emit_value_array_store_expr(name, index_usize, value, module)?;
    module.set_array_value_cell_kind(name, index_usize, value_cell_kind_for_expr(value, module));
    module.set_array_nested_value_metadata_at(
        name,
        index_usize,
        nested_array_metadata_for_expr(value, module),
    );
    Ok(())
}

pub(super) fn emit_value_array_sparse_assign(
    name: &str,
    len: usize,
    key: i64,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let source_ptr = preserve_array_ptr(name, "value_sparse_assign_source", module);
    let out_len = len + 1;
    let mut value_kinds = module
        .array_value_cell_kinds(name)
        .map(|kinds| kinds.to_vec())
        .unwrap_or_else(|| vec![ValueCellKind::Int; len]);
    value_kinds.push(value_cell_kind_for_expr(value, module).ok_or_else(|| {
        CompileError::new(
            value.span,
            "wasm32-web sparse value-array assignment does not support this value type yet",
        )
    })?);
    module.body().line(&format!("i32.const {}", out_len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", out_len));
    module.body().line(&format!("local.set ${}_len", name));
    module.set_array_length(name, out_len);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_key_kinds(name, Some(vec![AssocKeyKind::Int; out_len]));
    module.set_array_key_values(
        name,
        Some(
            (0..len)
                .map(|index| AssocKeyValue::Int(index as i64))
                .chain(std::iter::once(AssocKeyValue::Int(key)))
                .collect(),
        ),
    );
    module.set_array_value_cell_kinds(name, Some(value_kinds));
    for index in 0..len {
        emit_assoc_array_store_int_key(name, index, index as i64, module);
        emit_assoc_array_copy_value_cell_from_indexed(name, index, &source_ptr, index, module);
    }
    emit_assoc_array_store_int_key(name, len, key, module);
    emit_assoc_array_store_value(name, len, value, module)?;
    Ok(())
}

pub(super) fn emit_dynamic_value_array_index_assign(
    name: &str,
    index: &Expr,
    value: &Expr,
    len: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if value_cell_kind_for_expr(value, module).is_some() {
        return emit_dynamic_value_array_int_key_assign(name, index, value, len, module);
    }
    let index64 = module.next_label("value_array_dynamic_assign_index64");
    let index32 = module.next_label("value_array_dynamic_assign_index32");
    let cell = module.next_label("value_array_dynamic_assign_cell");
    let old_kinds = module.array_value_cell_kinds(name).map(|kinds| kinds.to_vec());
    let value_kind = value_cell_kind_for_expr(value, module);
    module.declare_i64_local(index64.trim_start_matches('$').to_string());
    module.declare_i32_local(index32.trim_start_matches('$').to_string());
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    require_int(index, module)?;
    module.body().line(&format!("local.set {}", index64));
    module.body().line(&format!("local.get {}", index64));
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_value_index_in_bounds");
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", index64));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", index32));
    emit_ensure_unique_array_payload(name, module);
    emit_value_cell_address_for_local(name, &index32, &cell, module);
    emit_store_value_cell(&cell, value, module)?;
    let preserved_kinds = match (old_kinds, value_kind) {
        (Some(kinds), Some(kind)) if kinds.iter().all(|old_kind| *old_kind == kind) => Some(kinds),
        _ => None,
    };
    module.set_array_value_cell_kinds(name, preserved_kinds);
    Ok(())
}

pub(super) fn emit_dynamic_value_array_int_key_assign(
    name: &str,
    index: &Expr,
    value: &Expr,
    len: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let old_ptr = preserve_array_ptr(name, "dynamic_value_sparse_assign", module);
    let index64 = module.next_label("dynamic_value_sparse_key");
    let source_index = module.next_label("dynamic_value_sparse_source_index");
    let out_index = module.next_label("dynamic_value_sparse_out_index");
    let target_entry = module.next_label("dynamic_value_sparse_target_entry");
    let found = module.next_label("dynamic_value_sparse_found");
    let value_cell = module.next_label("dynamic_value_sparse_value_cell");
    let done_label = module.next_label("dynamic_value_sparse_done");
    let loop_label = module.next_label("dynamic_value_sparse_loop");
    module.declare_i64_local(index64.trim_start_matches('$').to_string());
    for local in [&source_index, &out_index, &target_entry, &found, &value_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    require_int(index, module)?;
    module.body().line(&format!("local.set {}", index64));
    module.body().line(&format!("i32.const {}", len + 1));
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
    module.body().line(&format!("i32.const {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i64.extend_i32_u");
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i64.extend_i32_u");
    module.body().line(&format!("local.get {}", index64));
    module.body().line("i64.eq");
    module.body().open("if");
    emit_assoc_value_cell_address(&target_entry, &value_cell, module);
    emit_store_value_cell(&value_cell, value, module)?;
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line("else");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.get {}", old_ptr));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("call $__rt_value_cell");
    module.body().line("call $__rt_value_copy");
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
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", index64));
    module.body().line("call $__rt_assoc_store_int_key");
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
    module.clear_array_length(name);
    let preserved_kinds = module.array_value_cell_kinds(name).and_then(|kinds| {
        let mut kinds = kinds.to_vec();
        kinds.push(value_cell_kind_for_expr(value, module)?);
        Some(kinds)
    });
    module.set_array_value_cell_kinds(name, preserved_kinds);
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    Ok(())
}
