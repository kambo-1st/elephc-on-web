//! Purpose:
//! Emits wasm32-web array transform helpers for indexed integer arrays.
//! Keeps integer array_unique/diff/intersect/flip lowering out of the transform dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets`.
//!
//! Key details:
//! - Integer transforms produce associative arrays with PHP-compatible source indexes and flipped keys.

use super::*;

pub(super) fn emit_known_indexed_int_array_value_set_assign(
    name: &str,
    source: &str,
    function_name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let compare_sets = static_int_compare_sets(function_name, args)?;
    let source_len = module.array_length(source).ok_or_else(|| {
        CompileError::new(
            args[0].span,
            &format!("wasm32-web {function_name}() requires a known indexed array length"),
        )
    })?;
    let keep_matching = function_name.eq_ignore_ascii_case("array_intersect");
    let source_ptr = preserve_array_ptr(source, function_name, module);
    let index = module.next_label("array_value_set_index");
    let out_index = module.next_label("array_value_set_out_index");
    let value = module.next_label("array_value_set_value");
    let found = module.next_label("array_value_set_found");
    let target_entry = module.next_label("array_value_set_target_entry");
    let done_label = module.next_label("array_value_set_done");
    let loop_label = module.next_label("array_value_set_loop");
    for local in [&index, &out_index, &found, &target_entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(value.trim_start_matches('$').to_string());
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_key_kinds(name, None);
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
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
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", value));
    emit_value_set_membership(&value, &found, &compare_sets, keep_matching, module);
    module.body().line(&format!("local.get {}", found));
    if !keep_matching {
        module.body().line("i32.eqz");
    }
    module.body().open("if");
    emit_store_indexed_int_assoc_entry(name, &target_entry, &out_index, &index, &value, module);
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
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
    Ok(())
}

pub(super) fn emit_known_indexed_int_array_unique_assign(
    name: &str,
    source: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let sort_mode = array_unique_sort_mode(args, args[0].span, module)?;
    let source_ptr = preserve_array_ptr(source, "array_unique", module);
    let source_len = module.next_label("array_unique_source_len");
    let index = module.next_label("array_unique_index");
    let scan = module.next_label("array_unique_scan");
    let out_index = module.next_label("array_unique_out_index");
    let value = module.next_label("array_unique_value");
    let scan_value = module.next_label("array_unique_scan_value");
    let seen = module.next_label("array_unique_seen");
    let target_entry = module.next_label("array_unique_target_entry");
    let done_label = module.next_label("array_unique_done");
    let loop_label = module.next_label("array_unique_loop");
    let scan_done_label = module.next_label("array_unique_scan_done");
    let scan_loop_label = module.next_label("array_unique_scan_loop");
    for local in [&source_len, &index, &scan, &out_index, &seen, &target_entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&value, &scan_value] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_key_kinds(name, None);
    if let Some(len) = module.array_length(source) {
        module.body().line(&format!("i32.const {}", len));
    } else {
        module.body().line(&format!("local.get ${}_len", source));
    }
    module.body().line(&format!("local.set {}", source_len));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", value));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", seen));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", scan));
    module.body().open(&format!("block {}", scan_done_label));
    module.body().open(&format!("loop {}", scan_loop_label));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", scan_done_label));
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", scan_value));
    match sort_mode {
        ArrayUniqueSortMode::Regular | ArrayUniqueSortMode::Numeric => {
            module.body().line(&format!("local.get {}", value));
            module.body().line(&format!("local.get {}", scan_value));
            module.body().line("i64.eq");
        }
        ArrayUniqueSortMode::String => {
            module.body().line(&format!("local.get {}", value));
            module.body().line(&format!("local.get {}", scan_value));
            module.body().line("i64.eq");
        }
    }
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", seen));
    module.body().line(&format!("br {}", scan_done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().line(&format!("br {}", scan_loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", seen));
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_store_indexed_int_assoc_entry(name, &target_entry, &out_index, &index, &value, module);
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
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
    Ok(())
}
pub(super) fn emit_known_indexed_int_array_flip_assign(
    name: &str,
    source: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let source_len = module.array_length(source).ok_or_else(|| {
        CompileError::new(
            args[0].span,
            "wasm32-web array_flip() requires a known indexed array length",
        )
    })?;
    let source_ptr = preserve_array_ptr(source, "array_flip", module);
    let index = module.next_label("array_flip_index");
    let scan = module.next_label("array_flip_scan");
    let out_index = module.next_label("array_flip_out_index");
    let value = module.next_label("array_flip_value");
    let scan_value = module.next_label("array_flip_scan_value");
    let seen = module.next_label("array_flip_seen");
    let last_index = module.next_label("array_flip_last_index");
    let target_entry = module.next_label("array_flip_target_entry");
    let done_label = module.next_label("array_flip_done");
    let loop_label = module.next_label("array_flip_loop");
    let scan_done_label = module.next_label("array_flip_scan_done");
    let scan_loop_label = module.next_label("array_flip_scan_loop");
    for local in [&index, &scan, &out_index, &seen, &last_index, &target_entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&value, &scan_value] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_key_kinds(name, None);
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
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", value));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", seen));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.set {}", last_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", scan));
    module.body().open(&format!("block {}", scan_done_label));
    module.body().open(&format!("loop {}", scan_loop_label));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("i32.const {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", scan_done_label));
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", scan_value));
    module.body().line(&format!("local.get {}", value));
    module.body().line(&format!("local.get {}", scan_value));
    module.body().line("i64.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.lt_u");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", seen));
    module.body().line("else");
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.set {}", last_index));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().line(&format!("br {}", scan_loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", seen));
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_store_flipped_int_assoc_entry(name, &target_entry, &out_index, &value, &last_index, module);
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
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
    Ok(())
}

pub(super) fn emit_store_flipped_int_assoc_entry(
    name: &str,
    target_entry: &str,
    out_index: &str,
    key_value: &str,
    value_index: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", key_value));
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.get {}", value_index));
    module.body().line("i64.extend_i32_u");
    module.body().line("call $__rt_value_store_int");
}

pub(super) fn emit_store_indexed_int_assoc_entry(
    name: &str,
    target_entry: &str,
    out_index: &str,
    source_index: &str,
    value: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i64.extend_i32_u");
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.get {}", value));
    module.body().line("call $__rt_value_store_int");
}
