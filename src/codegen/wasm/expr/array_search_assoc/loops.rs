//! Purpose:
//! Emits reusable WAT loops for associative array_search() over wasm32-web arrays.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_search_assoc`.
//!
//! Key details:
//! - Each helper walks associative entries and stores the matching key payload.
//! - The parent module owns policy; this module owns repeated loop shapes.

use super::*;

pub(super) fn emit_assoc_search_store_key(
    entry: &str,
    result: &str,
    key_kind: Option<&str>,
    found: Option<&str>,
    done_label: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_key_payload_i64");
    module.body().line(&format!("local.set {}", result));
    if let Some(key_kind) = key_kind {
        module.body().line(&format!("local.get {}", entry));
        module.body().line("i32.load");
        module.body().line(&format!("local.set {}", key_kind));
    }
    if let Some(found) = found {
        module.body().line("i32.const 1");
        module.body().line(&format!("local.set {}", found));
    }
    module.body().line(&format!("br {}", done_label));
}

pub(super) fn emit_assoc_array_search_entry_loop<F>(
    var: &str,
    module: &mut WasmModule,
    mut emit_match: F,
) where
    F: FnMut(&str, &str, &str, &mut WasmModule),
{
    let index = module.next_label("assoc_array_search_index");
    let entry = module.next_label("assoc_array_search_entry");
    let cell = module.next_label("assoc_array_search_cell");
    let done_label = module.next_label("assoc_array_search_done");
    let loop_label = module.next_label("assoc_array_search_loop");
    for local in [&index, &entry, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("i32.const {}", WASM_ASSOC_ENTRY_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 16");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
    emit_match(&entry, &cell, &done_label, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(in crate::codegen::wasm::expr) fn emit_assoc_array_search_float(
    var: &str,
    payload: f64,
    result: &str,
    key_kind: Option<&str>,
    found: Option<&str>,
    module: &mut WasmModule,
) {
    emit_assoc_array_search_entry_loop(var, module, |entry, cell, done_label, module| {
        let match_done_label = module
            .next_label("assoc_array_search_float_match_done")
            .to_string();
        let tag = module.next_label("assoc_array_search_float_tag");
        module.declare_i32_local(tag.trim_start_matches('$').to_string());
        module.body().open(&format!("block {}", match_done_label));
        emit_load_value_cell_ptr_tag(cell, &tag, module);
        module.body().line(&format!("local.get {}", tag));
        module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
        module.body().line("i32.ne");
        module.body().line(&format!("br_if {}", match_done_label));
        module.body().line(&format!("local.get {}", cell));
        module.body().line("call $__rt_value_cell_payload_f64");
        module.body().line(&format!("f64.const {}", payload));
        module.body().line("f64.ne");
        module.body().line(&format!("br_if {}", match_done_label));
        emit_assoc_search_store_key(entry, result, key_kind, found, done_label, module);
        module.body().close("end");
    });
}

pub(in crate::codegen::wasm::expr) fn emit_assoc_array_search_tag_payload(
    var: &str,
    tag: i32,
    payload: i64,
    result: &str,
    key_kind: Option<&str>,
    found: Option<&str>,
    module: &mut WasmModule,
) {
    emit_assoc_array_search_entry_loop(var, module, |entry, cell, done_label, module| {
        let match_done_label = module
            .next_label("assoc_array_search_tag_match_done")
            .to_string();
        let tag_local = module.next_label("assoc_array_search_tag");
        module.declare_i32_local(tag_local.trim_start_matches('$').to_string());
        module.body().open(&format!("block {}", match_done_label));
        emit_load_value_cell_ptr_tag(cell, &tag_local, module);
        module.body().line(&format!("local.get {}", tag_local));
        module.body().line(&format!("i32.const {}", tag));
        module.body().line("i32.ne");
        module.body().line(&format!("br_if {}", match_done_label));
        if tag != WASM_VALUE_TAG_NULL {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("call $__rt_value_cell_payload_i64");
            module.body().line(&format!("i64.const {}", payload));
            module.body().line("i64.ne");
            module.body().line(&format!("br_if {}", match_done_label));
        }
        emit_assoc_search_store_key(entry, result, key_kind, found, done_label, module);
        module.body().close("end");
    });
}

pub(in crate::codegen::wasm::expr) fn emit_assoc_array_search_runtime_bool(
    var: &str,
    needle: &Expr,
    result: &str,
    key_kind: Option<&str>,
    found: Option<&str>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let needle_local = module.next_label("assoc_array_search_bool_needle");
    module.declare_i32_local(needle_local.trim_start_matches('$').to_string());
    emit_condition(needle, module)?;
    module.body().line(&format!("local.set {}", needle_local));
    emit_assoc_array_search_entry_loop(var, module, |entry, cell, done_label, module| {
        let next_item = module.next_label("assoc_array_search_runtime_bool_next");
        let tag = module.next_label("assoc_array_search_runtime_bool_tag");
        module.declare_i32_local(tag.trim_start_matches('$').to_string());
        module.body().open(&format!("block {}", next_item));
        emit_load_value_cell_ptr_tag(cell, &tag, module);
        module.body().line(&format!("local.get {}", tag));
        module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
        module.body().line("i32.ne");
        module.body().line(&format!("br_if {}", next_item));
        module.body().line(&format!("local.get {}", cell));
        module.body().line("call $__rt_value_cell_payload_i64");
        module.body().line(&format!("local.get {}", needle_local));
        module.body().line("i64.extend_i32_u");
        module.body().line("i64.ne");
        module.body().line(&format!("br_if {}", next_item));
        emit_assoc_search_store_key(entry, result, key_kind, found, done_label, module);
        module.body().close("end");
    });
    Ok(())
}

pub(in crate::codegen::wasm::expr) fn emit_assoc_array_search_runtime_float(
    var: &str,
    needle: &Expr,
    result: &str,
    key_kind: Option<&str>,
    found: Option<&str>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let needle_local = module.next_label("assoc_array_search_float_needle");
    module.declare_f64_local(needle_local.trim_start_matches('$').to_string());
    require_float(needle, module)?;
    module.body().line(&format!("local.set {}", needle_local));
    emit_assoc_array_search_entry_loop(var, module, |entry, cell, done_label, module| {
        let next_item = module.next_label("assoc_array_search_runtime_float_next");
        let tag = module.next_label("assoc_array_search_runtime_float_tag");
        module.declare_i32_local(tag.trim_start_matches('$').to_string());
        module.body().open(&format!("block {}", next_item));
        emit_load_value_cell_ptr_tag(cell, &tag, module);
        module.body().line(&format!("local.get {}", tag));
        module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
        module.body().line("i32.ne");
        module.body().line(&format!("br_if {}", next_item));
        module.body().line(&format!("local.get {}", cell));
        module.body().line("call $__rt_value_cell_payload_f64");
        module.body().line(&format!("local.get {}", needle_local));
        module.body().line("f64.ne");
        module.body().line(&format!("br_if {}", next_item));
        emit_assoc_search_store_key(entry, result, key_kind, found, done_label, module);
        module.body().close("end");
    });
    Ok(())
}

pub(in crate::codegen::wasm::expr) fn emit_assoc_array_search_runtime_int(
    var: &str,
    needle: &Expr,
    result: &str,
    key_kind: Option<&str>,
    found: Option<&str>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let needle_local = module.next_label("assoc_array_search_int_needle");
    module.declare_i64_local(needle_local.trim_start_matches('$').to_string());
    require_int(needle, module)?;
    module.body().line(&format!("local.set {}", needle_local));
    emit_assoc_array_search_entry_loop(var, module, |entry, cell, done_label, module| {
        let next_item = module.next_label("assoc_array_search_runtime_int_next");
        let tag = module.next_label("assoc_array_search_runtime_int_tag");
        module.declare_i32_local(tag.trim_start_matches('$').to_string());
        module.body().open(&format!("block {}", next_item));
        emit_load_value_cell_ptr_tag(cell, &tag, module);
        module.body().line(&format!("local.get {}", tag));
        module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
        module.body().line("i32.ne");
        module.body().line(&format!("br_if {}", next_item));
        module.body().line(&format!("local.get {}", cell));
        module.body().line("call $__rt_value_cell_payload_i64");
        module.body().line(&format!("local.get {}", needle_local));
        module.body().line("i64.ne");
        module.body().line(&format!("br_if {}", next_item));
        emit_assoc_search_store_key(entry, result, key_kind, found, done_label, module);
        module.body().close("end");
    });
    Ok(())
}

pub(in crate::codegen::wasm::expr) fn emit_assoc_array_search_string(
    var: &str,
    needle: &str,
    result: &str,
    matched: &str,
    key_kind: Option<&str>,
    found: Option<&str>,
    module: &mut WasmModule,
) {
    let (needle_ptr, needle_len) = module.intern_string(needle);
    let byte_index = module.next_label("assoc_array_search_string_byte");
    module.declare_i32_local(byte_index.trim_start_matches('$').to_string());
    emit_assoc_array_search_entry_loop(var, module, |entry, cell, done_label, module| {
        let item_ptr = module.next_label("assoc_array_search_string_item_ptr");
        let item_len = module.next_label("assoc_array_search_string_item_len");
        let tag = module.next_label("assoc_array_search_string_tag");
        let mismatch = module.next_label("assoc_array_search_string_mismatch");
        let next_item = module.next_label("assoc_array_search_string_next");
        let byte_loop = module.next_label("assoc_array_search_string_byte_loop");
        for local in [&item_ptr, &item_len, &tag] {
            module.declare_i32_local(local.trim_start_matches('$').to_string());
        }
        module.body().open(&format!("block {}", next_item));
        emit_load_value_cell_ptr_tag(cell, &tag, module);
        module.body().line(&format!("local.get {}", tag));
        module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
        module.body().line("i32.ne");
        module.body().line(&format!("br_if {}", next_item));
        emit_load_value_cell_ptr_string_part(cell, 8, &item_ptr, module);
        emit_load_value_cell_ptr_string_part(cell, 12, &item_len, module);
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", matched));
        module.body().line(&format!("local.get {}", item_len));
        module.body().line(&format!("i32.const {}", needle_len));
        module.body().line("i32.eq");
        module.body().open("if");
        module.body().line("i32.const 1");
        module.body().line(&format!("local.set {}", matched));
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", byte_index));
        module.body().open(&format!("block {}", mismatch));
        module.body().open(&format!("loop {}", byte_loop));
        module.body().line(&format!("local.get {}", byte_index));
        module.body().line(&format!("i32.const {}", needle_len));
        module.body().line("i32.ge_u");
        module.body().line(&format!("br_if {}", mismatch));
        module.body().line(&format!("local.get {}", item_ptr));
        module.body().line(&format!("local.get {}", byte_index));
        module.body().line("i32.add");
        module.body().line("i32.load8_u");
        module.body().line(&format!("i32.const {}", needle_ptr));
        module.body().line(&format!("local.get {}", byte_index));
        module.body().line("i32.add");
        module.body().line("i32.load8_u");
        module.body().line("i32.ne");
        module.body().open("if");
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", matched));
        module.body().line(&format!("br {}", mismatch));
        module.body().close("end");
        module.body().line(&format!("local.get {}", byte_index));
        module.body().line("i32.const 1");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", byte_index));
        module.body().line(&format!("br {}", byte_loop));
        module.body().close("end");
        module.body().close("end");
        module.body().close("end");
        module.body().line(&format!("local.get {}", matched));
        module.body().open("if");
        emit_assoc_search_store_key(entry, result, key_kind, found, done_label, module);
        module.body().close("end");
        module.body().close("end");
    });
}

pub(in crate::codegen::wasm::expr) fn emit_assoc_array_search_runtime_string(
    var: &str,
    needle_var: &str,
    result: &str,
    matched: &str,
    key_kind: Option<&str>,
    found: Option<&str>,
    module: &mut WasmModule,
) {
    let byte_index = module.next_label("assoc_array_search_runtime_string_byte");
    module.declare_i32_local(byte_index.trim_start_matches('$').to_string());
    emit_assoc_array_search_entry_loop(var, module, |entry, cell, done_label, module| {
        let item_ptr = module.next_label("assoc_array_search_runtime_string_item_ptr");
        let item_len = module.next_label("assoc_array_search_runtime_string_item_len");
        let tag = module.next_label("assoc_array_search_runtime_string_tag");
        let mismatch = module.next_label("assoc_array_search_runtime_string_mismatch");
        let next_item = module.next_label("assoc_array_search_runtime_string_next");
        let byte_loop = module.next_label("assoc_array_search_runtime_string_byte_loop");
        for local in [&item_ptr, &item_len, &tag] {
            module.declare_i32_local(local.trim_start_matches('$').to_string());
        }
        module.body().open(&format!("block {}", next_item));
        emit_load_value_cell_ptr_tag(cell, &tag, module);
        module.body().line(&format!("local.get {}", tag));
        module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
        module.body().line("i32.ne");
        module.body().line(&format!("br_if {}", next_item));
        emit_load_value_cell_ptr_string_part(cell, 8, &item_ptr, module);
        emit_load_value_cell_ptr_string_part(cell, 12, &item_len, module);
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", matched));
        module.body().line(&format!("local.get {}", item_len));
        module.body().line(&format!("local.get ${}_len", needle_var));
        module.body().line("i32.eq");
        module.body().open("if");
        module.body().line("i32.const 1");
        module.body().line(&format!("local.set {}", matched));
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", byte_index));
        module.body().open(&format!("block {}", mismatch));
        module.body().open(&format!("loop {}", byte_loop));
        module.body().line(&format!("local.get {}", byte_index));
        module.body().line(&format!("local.get ${}_len", needle_var));
        module.body().line("i32.ge_u");
        module.body().line(&format!("br_if {}", mismatch));
        module.body().line(&format!("local.get {}", item_ptr));
        module.body().line(&format!("local.get {}", byte_index));
        module.body().line("i32.add");
        module.body().line("i32.load8_u");
        module.body().line(&format!("local.get ${}_ptr", needle_var));
        module.body().line(&format!("local.get {}", byte_index));
        module.body().line("i32.add");
        module.body().line("i32.load8_u");
        module.body().line("i32.ne");
        module.body().open("if");
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", matched));
        module.body().line(&format!("br {}", mismatch));
        module.body().close("end");
        module.body().line(&format!("local.get {}", byte_index));
        module.body().line("i32.const 1");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", byte_index));
        module.body().line(&format!("br {}", byte_loop));
        module.body().close("end");
        module.body().close("end");
        module.body().close("end");
        module.body().line(&format!("local.get {}", matched));
        module.body().open("if");
        emit_assoc_search_store_key(entry, result, key_kind, found, done_label, module);
        module.body().close("end");
        module.body().close("end");
    });
}
