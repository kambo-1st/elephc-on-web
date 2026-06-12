//! Purpose:
//! Emits wasm32-web associative-array flip transform helpers.
//! Keeps array_flip-specific key/value conversion loops out of generic assoc set operations.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets::assoc`.
//! - `crate::codegen::wasm::expr::array_transform_sets::value_string`.
//!
//! Key details:
//! - Helpers preserve PHP int/string key normalization and value-cell flip restrictions.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_store_value_string_flip_entry(
    target_entry: &str,
    source_ptr: &str,
    source_index: &str,
    module: &mut WasmModule,
) {
    let key_ptr = module.next_label("value_flip_key_ptr");
    let key_len = module.next_label("value_flip_key_len");
    module.declare_i32_local(key_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(key_len.trim_start_matches('$').to_string());
    emit_load_value_cell_string_part(source_ptr, source_index, 8, &key_ptr, module);
    emit_load_value_cell_string_part(source_ptr, source_index, 12, &key_len, module);
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", key_ptr));
    module.body().line(&format!("local.get {}", key_len));
    module.body().line("call $__rt_assoc_store_php_string_key");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i64.extend_i32_u");
    module.body().line("call $__rt_value_store_int");
}

pub(in crate::codegen::wasm::expr) fn emit_value_cell_is_int_or_string(
    source_ptr: &str,
    source_index: &str,
    target: &str,
    module: &mut WasmModule,
) {
    let tag = module.next_label("value_flip_tag");
    module.declare_i32_local(tag.trim_start_matches('$').to_string());
    emit_load_value_cell_tag(source_ptr, source_index, &tag, module);
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", target));
}

pub(in crate::codegen::wasm::expr) fn emit_store_value_flip_entry(
    target_entry: &str,
    source_ptr: &str,
    source_index: &str,
    module: &mut WasmModule,
) {
    let source_cell = module.next_label("value_flip_source_cell");
    let source_tag = module.next_label("value_flip_source_tag");
    module.declare_i32_local(source_cell.trim_start_matches('$').to_string());
    module.declare_i32_local(source_tag.trim_start_matches('$').to_string());
    emit_value_cell_address(source_ptr, source_index, &source_cell, module);
    emit_load_value_cell_ptr_tag(&source_cell, &source_tag, module);
    module.body().line(&format!("local.get {}", source_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", source_cell));
    module.body().line("call $__rt_value_cell_payload_i64");
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i64.extend_i32_u");
    module.body().line("call $__rt_value_store_int");
    module.body().line("else");
    emit_store_value_string_flip_entry(target_entry, source_ptr, source_index, module);
    module.body().close("end");
}

pub(in crate::codegen::wasm::expr) fn runtime_assoc_key_kind_from_value_kinds(kinds: &[ValueCellKind]) -> Option<AssocKeyKind> {
    let first = *kinds.first()?;
    match first {
        ValueCellKind::Str if kinds.iter().all(|kind| *kind == ValueCellKind::Str) => {
            Some(AssocKeyKind::Str)
        }
        ValueCellKind::Int if kinds.iter().all(|kind| *kind == ValueCellKind::Int) => {
            Some(AssocKeyKind::Int)
        }
        _ => None,
    }
}

pub(in crate::codegen::wasm::expr) fn runtime_value_kind_from_assoc_key_kinds(kinds: &[AssocKeyKind]) -> Option<ValueCellKind> {
    let first = *kinds.first()?;
    match first {
        AssocKeyKind::Str if kinds.iter().all(|kind| *kind == AssocKeyKind::Str) => {
            Some(ValueCellKind::Str)
        }
        AssocKeyKind::Int if kinds.iter().all(|kind| *kind == AssocKeyKind::Int) => {
            Some(ValueCellKind::Int)
        }
        _ => None,
    }
}

pub(in crate::codegen::wasm::expr) fn emit_known_assoc_array_flip_assign(
    name: &str,
    source: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(value_kinds) = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec()) else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_flip() on associative arrays requires known value types",
        ));
    };
    if value_kinds
        .iter()
        .any(|kind| !matches!(kind, ValueCellKind::Int | ValueCellKind::Str))
    {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_flip() on associative arrays currently requires integer or string values",
        ));
    }
    let Some(key_kinds) = module.array_key_kinds(source).map(|kinds| kinds.to_vec()) else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_flip() on associative arrays requires known key types",
        ));
    };
    if key_kinds
        .iter()
        .any(|kind| !matches!(kind, AssocKeyKind::Int | AssocKeyKind::Str))
    {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_flip() on associative arrays currently requires integer or string keys",
        ));
    }
    let source_len = module.array_length(source).ok_or_else(|| {
        CompileError::new(
            args[0].span,
            "wasm32-web array_flip() requires a known associative array length",
        )
    })?;
    let exact_metadata = exact_assoc_value_flip_metadata(source, module);
    let source_ptr = preserve_array_ptr(source, "array_flip", module);
    let index = module.next_label("assoc_array_flip_index");
    let scan = module.next_label("assoc_array_flip_scan");
    let out_index = module.next_label("assoc_array_flip_out_index");
    let matched = module.next_label("assoc_array_flip_matched");
    let found = module.next_label("assoc_array_flip_found");
    let source_entry = module.next_label("assoc_array_flip_source_entry");
    let scan_entry = module.next_label("assoc_array_flip_scan_entry");
    let target_entry = module.next_label("assoc_array_flip_target_entry");
    let done_label = module.next_label("assoc_array_flip_done");
    let loop_label = module.next_label("assoc_array_flip_loop");
    let scan_done_label = module.next_label("assoc_array_flip_scan_done");
    let scan_loop_label = module.next_label("assoc_array_flip_scan_loop");
    for local in [
        &index,
        &scan,
        &out_index,
        &matched,
        &found,
        &source_entry,
        &scan_entry,
        &target_entry,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_key_kinds(name, None);
    module.set_array_runtime_key_kind(name, runtime_assoc_key_kind_from_value_kinds(&value_kinds));
    module.set_array_runtime_value_cell_kind(name, runtime_value_kind_from_assoc_key_kinds(&key_kinds));
    module.body().line(&format!("i32.const {}", source_len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("i32.const {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_assoc_entry_address(&source_ptr, &index, &source_entry, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", scan));
    module.body().open(&format!("block {}", scan_done_label));
    module.body().open(&format!("loop {}", scan_loop_label));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", scan_done_label));
    emit_assoc_entry_address(&format!("${}_ptr", name), &scan, &scan_entry, module);
    emit_assoc_entry_key_matches_assoc_value(&scan_entry, &source_entry, &matched, module);
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("local.get {}", scan_entry));
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("br {}", scan_done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().line(&format!("br {}", scan_loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_assoc_entry_address(&format!("${}_ptr", name), &out_index, &target_entry, module);
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    module.body().close("end");
    emit_store_assoc_flip_entry_from_source(&target_entry, &source_entry, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_index));
    module.body().line(&format!("local.set ${}_len", name));
    if let Some(metadata) = exact_metadata {
        apply_exact_unique_metadata(name, metadata, module);
    }
    Ok(())
}

pub(in crate::codegen::wasm::expr) fn emit_unknown_assoc_array_flip_assign(
    name: &str,
    source: &str,
    module: &mut WasmModule,
) {
    let source_ptr = preserve_array_ptr(source, "array_flip", module);
    let index = module.next_label("unknown_assoc_array_flip_index");
    let scan = module.next_label("unknown_assoc_array_flip_scan");
    let out_index = module.next_label("unknown_assoc_array_flip_out_index");
    let matched = module.next_label("unknown_assoc_array_flip_matched");
    let found = module.next_label("unknown_assoc_array_flip_found");
    let supported = module.next_label("unknown_assoc_array_flip_supported");
    let source_entry = module.next_label("unknown_assoc_array_flip_source_entry");
    let scan_entry = module.next_label("unknown_assoc_array_flip_scan_entry");
    let target_entry = module.next_label("unknown_assoc_array_flip_target_entry");
    let done_label = module.next_label("unknown_assoc_array_flip_done");
    let loop_label = module.next_label("unknown_assoc_array_flip_loop");
    let scan_done_label = module.next_label("unknown_assoc_array_flip_scan_done");
    let scan_loop_label = module.next_label("unknown_assoc_array_flip_scan_loop");
    for local in [
        &index,
        &scan,
        &out_index,
        &matched,
        &found,
        &supported,
        &source_entry,
        &scan_entry,
        &target_entry,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_key_kinds(name, None);
    module.set_array_runtime_key_kind(name, None);
    module.set_array_runtime_value_cell_kind(name, None);
    module.set_array_php_normalized_runtime_keys(name, true);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_assoc_entry_address(&source_ptr, &index, &source_entry, module);
    emit_assoc_entry_value_is_int_or_string(&source_entry, &supported, module);
    module.body().line(&format!("local.get {}", supported));
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", scan));
    module.body().open(&format!("block {}", scan_done_label));
    module.body().open(&format!("loop {}", scan_loop_label));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", scan_done_label));
    emit_assoc_entry_address(&format!("${}_ptr", name), &scan, &scan_entry, module);
    emit_assoc_entry_key_matches_assoc_value(&scan_entry, &source_entry, &matched, module);
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("local.get {}", scan_entry));
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("br {}", scan_done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().line(&format!("br {}", scan_loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_assoc_entry_address(&format!("${}_ptr", name), &out_index, &target_entry, module);
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    module.body().close("end");
    emit_store_assoc_flip_entry_from_source(&target_entry, &source_entry, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_index));
    module.body().line(&format!("local.set ${}_len", name));
}

fn emit_assoc_entry_value_is_int_or_string(entry: &str, target: &str, module: &mut WasmModule) {
    let cell = module.next_label("assoc_flip_runtime_value_cell");
    let tag = module.next_label("assoc_flip_runtime_value_tag");
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.declare_i32_local(tag.trim_start_matches('$').to_string());
    emit_assoc_value_cell_address(entry, &cell, module);
    emit_load_value_cell_ptr_tag(&cell, &tag, module);
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", target));
}
