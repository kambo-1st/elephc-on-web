//! Purpose:
//! Emits reusable wasm32-web associative-array entry helper snippets.
//! Keeps entry addressing, key matching, and flip-entry storage out of transform loops.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets::assoc`.
//! - `crate::codegen::wasm::expr::array_transform_sets::value_string`.
//!
//! Key details:
//! - Helpers preserve the assoc-entry memory layout and PHP int/string key normalization rules.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_assoc_entry_key_matches_value_cell(
    entry: &str,
    source_ptr: &str,
    source_index: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    let source_cell = module.next_label("value_flip_source_cell");
    let source_tag = module.next_label("value_flip_source_tag");
    let source_payload = module.next_label("value_flip_source_payload");
    module.declare_i32_local(source_cell.trim_start_matches('$').to_string());
    module.declare_i32_local(source_tag.trim_start_matches('$').to_string());
    module.declare_i64_local(source_payload.trim_start_matches('$').to_string());
    emit_value_cell_address(source_ptr, source_index, &source_cell, module);
    emit_load_value_cell_ptr_tag(&source_cell, &source_tag, module);
    module.body().line(&format!("local.get {}", source_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_load_value_cell_ptr_payload(&source_cell, &source_payload, module);
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get {}", source_payload));
    module.body().line("call $__rt_assoc_key_eq_int");
    module.body().line(&format!("local.set {}", matched));
    module.body().line("else");
    emit_assoc_entry_key_matches_value_string_cell(entry, source_ptr, source_index, matched, module);
    module.body().close("end");
}

pub(in crate::codegen::wasm::expr) fn emit_value_cell_address(source_ptr: &str, index: &str, cell: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", cell));
}

pub(in crate::codegen::wasm::expr) fn emit_assoc_entry_key_matches_value_string_cell(
    entry: &str,
    source_ptr: &str,
    source_index: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    let value_ptr = module.next_label("value_flip_string_ptr");
    let value_len = module.next_label("value_flip_string_len");
    for local in [&value_ptr, &value_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_load_value_cell_string_part(source_ptr, source_index, 8, &value_ptr, module);
    emit_load_value_cell_string_part(source_ptr, source_index, 12, &value_len, module);
    emit_assoc_entry_matches_string_parts(entry, &value_ptr, &value_len, matched, module);
}

pub(in crate::codegen::wasm::expr) fn emit_assoc_entry_address(
    source_ptr: &str,
    index: &str,
    target_entry: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
}

pub(in crate::codegen::wasm::expr) fn emit_assoc_entry_address_const_index(
    source_ptr: &str,
    index: usize,
    target_entry: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("i32.const {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
}

pub(in crate::codegen::wasm::expr) fn emit_assoc_value_cell_address(entry: &str, target_cell: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", target_cell));
}

pub(in crate::codegen::wasm::expr) fn emit_assoc_entry_values_same_string_repr(
    left_entry: &str,
    right_entry: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    let left_cell = module.next_label("assoc_unique_left_cell");
    let right_cell = module.next_label("assoc_unique_right_cell");
    for local in [&left_cell, &right_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_assoc_value_cell_address(left_entry, &left_cell, module);
    emit_assoc_value_cell_address(right_entry, &right_cell, module);
    emit_value_cell_ptrs_same_string_repr_between(&left_cell, &right_cell, matched, module);
}

pub(in crate::codegen::wasm::expr) fn emit_assoc_entry_key_matches_assoc_value(
    entry: &str,
    source_entry: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    let source_cell = module.next_label("assoc_flip_value_cell");
    let source_tag = module.next_label("assoc_flip_value_tag");
    let value_ptr = module.next_label("assoc_flip_string_ptr");
    let value_len = module.next_label("assoc_flip_string_len");
    let value_payload = module.next_label("assoc_flip_int_payload");
    for local in [&source_cell, &source_tag, &value_ptr, &value_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(value_payload.trim_start_matches('$').to_string());
    emit_assoc_value_cell_address(source_entry, &source_cell, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    emit_load_value_cell_ptr_tag(&source_cell, &source_tag, module);
    module.body().line(&format!("local.get {}", source_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_load_value_cell_ptr_payload(&source_cell, &value_payload, module);
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get {}", value_payload));
    module.body().line("call $__rt_assoc_key_eq_int");
    module.body().line(&format!("local.set {}", matched));
    module.body().line("else");
    module.body().line(&format!("local.get {}", source_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_load_value_cell_ptr_string_part(&source_cell, 8, &value_ptr, module);
    emit_load_value_cell_ptr_string_part(&source_cell, 12, &value_len, module);
    emit_assoc_entry_matches_string_parts(entry, &value_ptr, &value_len, matched, module);
    module.body().close("end");
    module.body().close("end");
}

pub(in crate::codegen::wasm::expr) fn emit_store_assoc_flip_entry_from_source(
    target_entry: &str,
    source_entry: &str,
    module: &mut WasmModule,
) {
    let value_tag = module.next_label("assoc_flip_value_tag");
    let source_value_cell = module.next_label("assoc_flip_source_value_cell");
    let target_value_cell = module.next_label("assoc_flip_target_value_cell");
    let source_value_payload = module.next_label("assoc_flip_source_value_payload");
    let source_key_payload = module.next_label("assoc_flip_source_key_payload");
    let source_key_ptr = module.next_label("assoc_flip_source_key_ptr");
    let source_key_len = module.next_label("assoc_flip_source_key_len");
    module.declare_i32_local(value_tag.trim_start_matches('$').to_string());
    for local in [&source_value_cell, &target_value_cell, &source_key_ptr, &source_key_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&source_value_payload, &source_key_payload] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    emit_assoc_value_cell_address(source_entry, &source_value_cell, module);
    emit_assoc_value_cell_address(target_entry, &target_value_cell, module);
    emit_load_value_cell_ptr_tag(&source_value_cell, &value_tag, module);
    module.body().line(&format!("local.get {}", value_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_load_value_cell_ptr_payload(&source_value_cell, &source_value_payload, module);
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", source_value_payload));
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line("else");
    module.body().line(&format!("local.get {}", target_entry));
    emit_load_value_cell_ptr_string_part(&source_value_cell, 8, &source_key_ptr, module);
    emit_load_value_cell_ptr_string_part(&source_value_cell, 12, &source_key_len, module);
    module.body().line(&format!("local.get {}", source_key_ptr));
    module.body().line(&format!("local.get {}", source_key_len));
    module.body().line("call $__rt_assoc_store_php_string_key");
    module.body().close("end");
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line(&format!("local.set {}", value_tag));
    module.body().line("else");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line(&format!("local.set {}", value_tag));
    module.body().close("end");
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("call $__rt_assoc_key_payload_i64");
    module.body().line(&format!("local.set {}", source_key_payload));
    module.body().line(&format!("local.get {}", value_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", target_value_cell));
    module.body().line(&format!("local.get {}", source_key_payload));
    module.body().line("call $__rt_value_store_int");
    module.body().line("else");
    module.body().line(&format!("local.get {}", target_value_cell));
    module.body().line(&format!("local.get {}", source_key_payload));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.get {}", source_key_payload));
    module.body().line("i64.const 32");
    module.body().line("i64.shr_u");
    module.body().line("i32.wrap_i64");
    module.body().line("call $__rt_value_store_string");
    module.body().close("end");
}
