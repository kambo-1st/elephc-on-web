//! Purpose:
//! Emits wasm32-web value-cell compare/swap and slot load/store helpers for array sorting.
//! Keeps sort support separate from value-cell storage and access lowering.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_value_cells` re-exports
//! - array mutator and callback sort emitters
//!
//! Key details:
//! - Operates directly on the boxed value-cell heap layout and preserves cell payload halves.

use super::*;

pub(super) fn emit_array_compare_swap(
    name: &str,
    index: usize,
    descending: bool,
    left: &str,
    right: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("i32.const {}", index * 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("i32.const {}", (index + 1) * 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(if descending { "i64.lt_s" } else { "i64.gt_s" });
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("i32.const {}", index * 8));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", right));
    module.body().line("i64.store");
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("i32.const {}", (index + 1) * 8));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", left));
    module.body().line("i64.store");
    module.body().close("end");
}

pub(super) fn emit_value_array_string_compare_swap(
    name: &str,
    index: usize,
    descending: bool,
    module: &mut WasmModule,
) {
    let left_ptr = module.next_label("value_sort_left_ptr");
    let left_len = module.next_label("value_sort_left_len");
    let right_ptr = module.next_label("value_sort_right_ptr");
    let right_len = module.next_label("value_sort_right_len");
    let byte_index = module.next_label("value_sort_byte_index");
    let left_byte = module.next_label("value_sort_left_byte");
    let right_byte = module.next_label("value_sort_right_byte");
    let cmp = module.next_label("value_sort_cmp");
    let low = module.next_label("value_sort_low");
    let high = module.next_label("value_sort_high");
    let loop_label = module.next_label("value_sort_cmp_loop");
    let done_label = module.next_label("value_sort_cmp_done");
    for local in [
        &left_ptr,
        &left_len,
        &right_ptr,
        &right_len,
        &byte_index,
        &left_byte,
        &right_byte,
        &cmp,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&low, &high] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    emit_load_value_string_part(name, index, 8, "i32.load", &left_ptr, module);
    emit_load_value_string_part(name, index, 12, "i32.load", &left_len, module);
    emit_load_value_string_part(name, index + 1, 8, "i32.load", &right_ptr, module);
    emit_load_value_string_part(name, index + 1, 12, "i32.load", &right_len, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", byte_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", cmp));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", byte_index));
    module.body().line(&format!("local.get {}", left_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("local.get {}", byte_index));
    module.body().line(&format!("local.get {}", right_len));
    module.body().line("i32.ge_u");
    module.body().line("i32.or");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", left_ptr));
    module.body().line(&format!("local.get {}", byte_index));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", left_byte));
    module.body().line(&format!("local.get {}", right_ptr));
    module.body().line(&format!("local.get {}", byte_index));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", right_byte));
    module.body().line(&format!("local.get {}", left_byte));
    module.body().line(&format!("local.get {}", right_byte));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line(&format!("local.get {}", left_byte));
    module.body().line(&format!("local.get {}", right_byte));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", cmp));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", byte_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", byte_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", cmp));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line(&format!("local.get {}", left_len));
    module.body().line(&format!("local.get {}", right_len));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", cmp));
    module.body().close("end");
    module.body().line(&format!("local.get {}", cmp));
    module.body().line("i32.const 0");
    module
        .body()
        .line(if descending { "i32.lt_s" } else { "i32.gt_s" });
    module.body().open("if");
    emit_load_value_cell_half(name, index, 0, &low, module);
    emit_load_value_cell_half(name, index, 8, &high, module);
    emit_copy_value_cell_slot(name, index, index + 1, module);
    emit_store_value_cell_half(name, index + 1, 0, &low, module);
    emit_store_value_cell_half(name, index + 1, 8, &high, module);
    module.body().close("end");
}

pub(super) fn emit_value_array_scalar_compare_swap(
    name: &str,
    index: usize,
    descending: bool,
    module: &mut WasmModule,
) {
    let left_tag = module.next_label("value_scalar_sort_left_tag");
    let right_tag = module.next_label("value_scalar_sort_right_tag");
    let should_swap = module.next_label("value_scalar_sort_should_swap");
    let left_bool = module.next_label("value_scalar_sort_left_bool");
    let right_bool = module.next_label("value_scalar_sort_right_bool");
    let left_payload = module.next_label("value_scalar_sort_left_payload");
    let right_payload = module.next_label("value_scalar_sort_right_payload");
    let low = module.next_label("value_scalar_sort_low");
    let high = module.next_label("value_scalar_sort_high");
    for local in [&left_tag, &right_tag, &should_swap, &left_bool, &right_bool] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left_payload, &right_payload, &low, &high] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    emit_load_static_value_cell_tag(name, index, &left_tag, module);
    emit_load_static_value_cell_tag(name, index + 1, &right_tag, module);
    emit_load_static_value_cell_payload(name, index, &left_payload, module);
    emit_load_static_value_cell_payload(name, index + 1, &right_payload, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", should_swap));
    module.body().line(&format!("local.get {}", left_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", right_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().line("i32.and");
    module.body().open("if");
    module.body().line(&format!("local.get {}", left_payload));
    module.body().line(&format!("local.get {}", right_payload));
    module
        .body()
        .line(if descending { "i64.lt_s" } else { "i64.gt_s" });
    module.body().line(&format!("local.set {}", should_swap));
    module.body().line("else");
    emit_value_scalar_sort_bool(&left_tag, &left_payload, &left_bool, module);
    emit_value_scalar_sort_bool(&right_tag, &right_payload, &right_bool, module);
    module.body().line(&format!("local.get {}", left_bool));
    module.body().line(&format!("local.get {}", right_bool));
    module
        .body()
        .line(if descending { "i32.lt_u" } else { "i32.gt_u" });
    module.body().line(&format!("local.set {}", should_swap));
    module.body().close("end");
    module.body().line(&format!("local.get {}", should_swap));
    module.body().open("if");
    emit_load_value_cell_half(name, index, 0, &low, module);
    emit_load_value_cell_half(name, index, 8, &high, module);
    emit_copy_value_cell_slot(name, index, index + 1, module);
    emit_store_value_cell_half(name, index + 1, 0, &low, module);
    emit_store_value_cell_half(name, index + 1, 8, &high, module);
    module.body().close("end");
}

fn emit_value_scalar_sort_bool(tag: &str, payload: &str, target: &str, module: &mut WasmModule) {
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", target));
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line(&format!("local.get {}", payload));
    module.body().line("i64.eqz");
    module.body().line("i32.eqz");
    module.body().line(&format!("local.set {}", target));
    module.body().close("end");
}

pub(super) fn emit_load_value_string_part(
    name: &str,
    index: usize,
    payload_offset: usize,
    load: &str,
    target: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE + payload_offset));
    module.body().line("i32.add");
    module.body().line(load);
    module.body().line(&format!("local.set {}", target));
}

pub(super) fn emit_load_static_value_cell_tag(name: &str, index: usize, target: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", target));
}

fn emit_load_static_value_cell_payload(
    name: &str,
    index: usize,
    target: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE + 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", target));
}

pub(super) fn emit_load_value_cell_half(
    name: &str,
    index: usize,
    offset: usize,
    target: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE + offset));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", target));
}

pub(super) fn emit_store_value_cell_half(
    name: &str,
    index: usize,
    offset: usize,
    source: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE + offset));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", source));
    module.body().line("i64.store");
}

pub(super) fn emit_copy_value_cell_slot(
    name: &str,
    target_index: usize,
    source_index: usize,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", target_index * WASM_VALUE_CELL_SIZE));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", source_index * WASM_VALUE_CELL_SIZE));
    module.body().line("i32.add");
    module.body().line("call $__rt_value_copy");
}
