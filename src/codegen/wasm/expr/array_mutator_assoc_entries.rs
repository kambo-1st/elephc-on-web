//! Purpose:
//! Provides WASM helper emitters for associative-array entry payload loads and swaps.
//! Keeps low-level entry movement separate from array sort policy.
//!
//! Called from:
//! - `super::array_mutator_sorts` and callback-based array sort emitters.
//!
//! Key details:
//! - Helpers preserve the associative entry layout contract used by `__rt_assoc_entry`.

use super::*;

pub(super) fn emit_load_assoc_value_payload_i64(
    name: &str,
    index: usize,
    target: &str,
    module: &mut WasmModule,
) {
    emit_assoc_payload_address_const_index(name, index, 24, target, module);
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", target));
}

pub(super) fn emit_load_assoc_value_payload_f64(
    name: &str,
    index: usize,
    target: &str,
    module: &mut WasmModule,
) {
    emit_assoc_payload_address_const_index(name, index, 24, target, module);
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", target));
}

pub(super) fn emit_load_assoc_key_payload_i64(
    name: &str,
    index: usize,
    target: &str,
    module: &mut WasmModule,
) {
    emit_assoc_payload_address_const_index(name, index, 8, target, module);
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", target));
}

pub(super) fn emit_load_assoc_key_payload_i32(
    name: &str,
    index: usize,
    payload_offset: usize,
    target: &str,
    module: &mut WasmModule,
) {
    emit_assoc_payload_address_const_index(name, index, payload_offset, target, module);
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", target));
}

pub(super) fn emit_load_assoc_value_tag(name: &str, index: usize, target: &str, module: &mut WasmModule) {
    emit_assoc_payload_address_const_index(name, index, 16, target, module);
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", target));
}

pub(super) fn emit_load_assoc_value_numeric(
    name: &str,
    index: usize,
    tag: &str,
    target: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_assoc_payload_address_const_index(name, index, 24, target, module);
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", target));
    module.body().line("else");
    emit_assoc_payload_address_const_index(name, index, 24, target, module);
    module.body().line("i64.load");
    module.body().line("f64.convert_i64_s");
    module.body().line(&format!("local.set {}", target));
    module.body().close("end");
}

pub(super) fn emit_load_assoc_value_truthy_scalar(
    name: &str,
    index: usize,
    tag: &str,
    target: &str,
    module: &mut WasmModule,
) {
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", target));
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_assoc_payload_address_const_index(name, index, 24, target, module);
    module.body().line("f64.load");
    module.body().line("f64.const 0");
    module.body().line("f64.ne");
    module.body().line(&format!("local.set {}", target));
    module.body().line("else");
    emit_assoc_payload_address_const_index(name, index, 24, target, module);
    module.body().line("i64.load");
    module.body().line("i64.eqz");
    module.body().line("i32.eqz");
    module.body().line(&format!("local.set {}", target));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_load_assoc_value_payload_i32(
    name: &str,
    index: usize,
    payload_offset: usize,
    target: &str,
    module: &mut WasmModule,
) {
    emit_assoc_payload_address_const_index(name, index, 16 + payload_offset, target, module);
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", target));
}

fn emit_assoc_payload_address_const_index(
    name: &str,
    index: usize,
    offset: usize,
    label: &str,
    module: &mut WasmModule,
) {
    let address = module.next_label(label.trim_start_matches('$'));
    module.declare_i32_local(address.trim_start_matches('$').to_string());
    emit_assoc_entry_address_const_index(&format!("${}_ptr", name), index, &address, module);
    module.body().line(&format!("local.get {}", address));
    module.body().line(&format!("i32.const {}", offset));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", address));
    module.body().line(&format!("local.get {}", address));
}

pub(super) fn emit_runtime_assoc_neighbor_value_cells(
    name: &str,
    index: &str,
    left_entry: &str,
    right_entry: &str,
    left_cell: &str,
    right_cell: &str,
    module: &mut WasmModule,
) {
    emit_assoc_entry_address(&format!("${}_ptr", name), index, left_entry, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", right_entry));
    emit_assoc_entry_address(&format!("${}_ptr", name), right_entry, right_entry, module);
    emit_assoc_value_cell_address(left_entry, left_cell, module);
    emit_assoc_value_cell_address(right_entry, right_cell, module);
}

pub(super) fn emit_runtime_assoc_neighbor_entries(
    name: &str,
    index: &str,
    left_entry: &str,
    right_entry: &str,
    module: &mut WasmModule,
) {
    emit_assoc_entry_address(&format!("${}_ptr", name), index, left_entry, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", right_entry));
    emit_assoc_entry_address(&format!("${}_ptr", name), right_entry, right_entry, module);
}

pub(super) fn emit_swap_dynamic_assoc_entries(left_entry: &str, right_entry: &str, module: &mut WasmModule) {
    let temp_a = module.next_label("assoc_entry_dynamic_swap_a");
    let temp_b = module.next_label("assoc_entry_dynamic_swap_b");
    let temp_c = module.next_label("assoc_entry_dynamic_swap_c");
    let temp_d = module.next_label("assoc_entry_dynamic_swap_d");
    for local in [&temp_a, &temp_b, &temp_c, &temp_d] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    emit_load_dynamic_assoc_entry_word(left_entry, 0, &temp_a, module);
    emit_load_dynamic_assoc_entry_word(left_entry, 8, &temp_b, module);
    emit_load_dynamic_assoc_entry_word(left_entry, 16, &temp_c, module);
    emit_load_dynamic_assoc_entry_word(left_entry, 24, &temp_d, module);
    emit_copy_dynamic_assoc_entry_word(left_entry, right_entry, 0, module);
    emit_copy_dynamic_assoc_entry_word(left_entry, right_entry, 8, module);
    emit_copy_dynamic_assoc_entry_word(left_entry, right_entry, 16, module);
    emit_copy_dynamic_assoc_entry_word(left_entry, right_entry, 24, module);
    emit_store_dynamic_assoc_entry_word(right_entry, 0, &temp_a, module);
    emit_store_dynamic_assoc_entry_word(right_entry, 8, &temp_b, module);
    emit_store_dynamic_assoc_entry_word(right_entry, 16, &temp_c, module);
    emit_store_dynamic_assoc_entry_word(right_entry, 24, &temp_d, module);
}

fn emit_load_dynamic_assoc_entry_word(entry: &str, offset: usize, target: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("i32.const {}", offset));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", target));
}

fn emit_copy_dynamic_assoc_entry_word(target: &str, source: &str, offset: usize, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", target));
    module.body().line(&format!("i32.const {}", offset));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", source));
    module.body().line(&format!("i32.const {}", offset));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.store");
}

fn emit_store_dynamic_assoc_entry_word(entry: &str, offset: usize, source: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("i32.const {}", offset));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", source));
    module.body().line("i64.store");
}

pub(super) fn emit_swap_assoc_entries(name: &str, index: usize, module: &mut WasmModule) {
    let first_entry = module.next_label("assoc_entry_swap_first");
    let second_entry = module.next_label("assoc_entry_swap_second");
    let temp_a = module.next_label("assoc_entry_swap_a");
    let temp_b = module.next_label("assoc_entry_swap_b");
    let temp_c = module.next_label("assoc_entry_swap_c");
    let temp_d = module.next_label("assoc_entry_swap_d");
    for local in [&temp_a, &temp_b, &temp_c, &temp_d] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    for local in [&first_entry, &second_entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_assoc_entry_address_const_index(&format!("${}_ptr", name), index, &first_entry, module);
    emit_assoc_entry_address_const_index(&format!("${}_ptr", name), index + 1, &second_entry, module);
    emit_load_dynamic_assoc_entry_word(&first_entry, 0, &temp_a, module);
    emit_load_dynamic_assoc_entry_word(&first_entry, 8, &temp_b, module);
    emit_load_dynamic_assoc_entry_word(&first_entry, 16, &temp_c, module);
    emit_load_dynamic_assoc_entry_word(&first_entry, 24, &temp_d, module);
    emit_copy_dynamic_assoc_entry_word(&first_entry, &second_entry, 0, module);
    emit_copy_dynamic_assoc_entry_word(&first_entry, &second_entry, 8, module);
    emit_copy_dynamic_assoc_entry_word(&first_entry, &second_entry, 16, module);
    emit_copy_dynamic_assoc_entry_word(&first_entry, &second_entry, 24, module);
    emit_store_dynamic_assoc_entry_word(&second_entry, 0, &temp_a, module);
    emit_store_dynamic_assoc_entry_word(&second_entry, 8, &temp_b, module);
    emit_store_dynamic_assoc_entry_word(&second_entry, 16, &temp_c, module);
    emit_store_dynamic_assoc_entry_word(&second_entry, 24, &temp_d, module);
}
