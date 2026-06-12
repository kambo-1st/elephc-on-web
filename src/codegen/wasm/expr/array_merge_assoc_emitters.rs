//! Purpose:
//! Emits wasm32-web associative array_merge() source and literal copy loops.
//! Keeps entry storage details separate from merge planning and metadata.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_merge`
//! - `crate::codegen::wasm::expr::array_assignment`
//!
//! Key details:
//! - Integer keys are reindexed for PHP array_merge(); string keys replace earlier entries.

use super::*;
use super::array_merge_key_set::{
    emit_assoc_entries_have_same_key, emit_assoc_entry_matches_static_string_key, StaticAssocKey,
};
use super::array_transform_sets::emit_assoc_entry_address;

pub(super) fn emit_assoc_array_merge_value_source(
    name: &str,
    source: &str,
    out_index: &str,
    source_ptr: &str,
    source_len: &str,
    source_index: &str,
    source_cell: &str,
    target_entry: &str,
    next_int_key: &str,
    target_cell: &str,
    module: &mut WasmModule,
) {
    let done_label = module.next_label("assoc_merge_value_done");
    let loop_label = module.next_label("assoc_merge_value_loop");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.set {}", source_ptr));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", source_len));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", source_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_assoc_entry_address(&format!("${}_ptr", name), out_index, target_entry, module);
    emit_store_assoc_reindexed_int_key_at_entry(target_entry, next_int_key, module);
    emit_assoc_value_cell_address(target_entry, target_cell, module);
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", source_cell));
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get {}", source_cell));
    module.body().line("call $__rt_value_copy");
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
}

pub(super) fn emit_assoc_array_merge_compact_int_source(
    name: &str,
    source: &str,
    out_index: &str,
    source_ptr: &str,
    source_len: &str,
    source_index: &str,
    target_entry: &str,
    next_int_key: &str,
    value: &str,
    module: &mut WasmModule,
) {
    let done_label = module.next_label("assoc_merge_compact_done");
    let loop_label = module.next_label("assoc_merge_compact_loop");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.set {}", source_ptr));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", source_len));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", source_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", value));
    emit_assoc_entry_address(&format!("${}_ptr", name), out_index, target_entry, module);
    emit_store_assoc_reindexed_int_key_at_entry(target_entry, next_int_key, module);
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("i32.const 16");
    module.body().line("i32.add");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.store");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("i32.const 24");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", value));
    module.body().line("i64.store");
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
}

pub(super) fn emit_assoc_array_merge_local_source(
    name: &str,
    source: &str,
    out_index: &str,
    source_ptr: &str,
    source_len: &str,
    source_index: &str,
    source_entry: &str,
    scan: &str,
    scan_entry: &str,
    matched: &str,
    found: &str,
    target_entry: &str,
    next_int_key: &str,
    module: &mut WasmModule,
) {
    let done_label = module.next_label("assoc_merge_local_done");
    let loop_label = module.next_label("assoc_merge_local_loop");
    let find_done_label = module.next_label("assoc_merge_local_find_done");
    let find_loop_label = module.next_label("assoc_merge_local_find_loop");
    let source_reindexed_int_key = module.next_label("assoc_merge_source_reindexed_int_key");
    module.declare_i32_local(source_reindexed_int_key.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.set {}", source_ptr));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", source_len));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", source_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_assoc_entry_address(source_ptr, source_index, source_entry, module);
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.eq");
    module.body().open("if (result i32)");
    module.body().line("i32.const 1");
    module.body().line("else");
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_STRING));
    module.body().line("i32.eq");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line("call $__rt_php_array_key_is_int");
    module.body().line("else");
    module.body().line("i32.const 0");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.set {}", source_reindexed_int_key));
    module.body().line(&format!("local.get {}", source_reindexed_int_key));
    module.body().open("if");
    emit_assoc_entry_address(&format!("${}_ptr", name), out_index, target_entry, module);
    emit_store_assoc_reindexed_int_key_at_entry(target_entry, next_int_key, module);
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line("else");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", scan));
    module.body().open(&format!("block {}", find_done_label));
    module.body().open(&format!("loop {}", find_loop_label));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", find_done_label));
    emit_assoc_entry_address(&format!("${}_ptr", name), scan, scan_entry, module);
    emit_assoc_entries_have_same_key(scan_entry, source_entry, matched, module);
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("local.get {}", scan_entry));
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("br {}", find_done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().line(&format!("br {}", find_loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_assoc_entry_address(&format!("${}_ptr", name), out_index, target_entry, module);
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    module.body().close("end");
    copy_assoc_entry_key(target_entry, source_entry, module);
    module.body().close("end");
    copy_assoc_entry_value(&target_entry, &source_entry, module);
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_assoc_array_merge_literal_string_item(
    name: &str,
    key: &str,
    value: &Expr,
    out_index: &str,
    scan: &str,
    target_entry: &str,
    scan_entry: &str,
    matched: &str,
    found: &str,
    value_cell: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let done_label = module.next_label("assoc_merge_find_done");
    let loop_label = module.next_label("assoc_merge_find_loop");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", scan));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_assoc_entry_address(&format!("${}_ptr", name), scan, scan_entry, module);
    emit_assoc_entry_matches_static_string_key(scan_entry, key, matched, module);
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("local.get {}", scan_entry));
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_assoc_entry_address(&format!("${}_ptr", name), out_index, target_entry, module);
    emit_store_assoc_string_key_at_entry(target_entry, key, module);
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    module.body().close("end");
    emit_assoc_value_cell_address(target_entry, value_cell, module);
    emit_store_value_cell(value_cell, value, module)
}

pub(super) fn emit_assoc_array_merge_literal_int_item(
    name: &str,
    value: &Expr,
    out_index: &str,
    target_entry: &str,
    next_int_key: &str,
    value_cell: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_assoc_entry_address(&format!("${}_ptr", name), out_index, target_entry, module);
    emit_store_assoc_reindexed_int_key_at_entry(target_entry, next_int_key, module);
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    emit_assoc_value_cell_address(target_entry, value_cell, module);
    emit_store_value_cell(value_cell, value, module)
}

pub(super) fn emit_store_assoc_string_key_at_entry(entry: &str, key: &str, module: &mut WasmModule) {
    let (ptr, len) = module.intern_string(key);
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("i32.const {}", ptr));
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_assoc_store_string_key");
}

pub(super) fn emit_store_assoc_reindexed_int_key_at_entry(entry: &str, next_int_key: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get {}", next_int_key));
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", next_int_key));
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", next_int_key));
}

pub(super) fn emit_store_assoc_key_at_entry(entry: &str, key: &StaticAssocKey, module: &mut WasmModule) {
    match key {
        StaticAssocKey::Int(value) => {
            module.body().line(&format!("local.get {}", entry));
            module.body().line(&format!("i64.const {}", value));
            module.body().line("call $__rt_assoc_store_int_key");
        }
        StaticAssocKey::Str(value) => emit_store_assoc_string_key_at_entry(entry, value, module),
    }
}
