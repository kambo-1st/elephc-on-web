//! Purpose:
//! Emits wasm32-web `uasort()` helper loops and compare-swap paths.
//! Keeps value-preserving associative sorting separate from other callback sort modes.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_sort`
//!
//! Key details:
//! - Swaps complete associative entries so keys remain attached to their values.

use super::*;
use super::array_mutators::{
    emit_load_assoc_value_payload_f64, emit_load_assoc_value_payload_i32,
    emit_load_assoc_value_payload_i64, emit_runtime_assoc_neighbor_value_cells,
    emit_swap_assoc_entries, emit_swap_dynamic_assoc_entries,
};

pub(super) fn emit_uasort_int_compare_swap(
    name: &str,
    index: usize,
    callback: &str,
    module: &mut WasmModule,
) {
    let left = module.next_label("uasort_left");
    let right = module.next_label("uasort_right");
    let cmp = module.next_label("uasort_cmp");
    for local in [&left, &right, &cmp] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    emit_load_assoc_value_payload_i64(name, index, &left, module);
    emit_load_assoc_value_payload_i64(name, index + 1, &right, module);
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set {}", cmp));
    module.body().line(&format!("local.get {}", cmp));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_swap_assoc_entries(name, index, module);
    module.body().close("end");
}

pub(super) fn emit_uasort_int_compare_swap_instance(
    name: &str,
    index: usize,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) {
    let left = module.next_label("uasort_instance_left");
    let right = module.next_label("uasort_instance_right");
    let cmp = module.next_label("uasort_instance_cmp");
    for local in [&left, &right, &cmp] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    emit_load_assoc_value_payload_i64(name, index, &left, module);
    emit_load_assoc_value_payload_i64(name, index + 1, &right, module);
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set {}", cmp));
    module.body().line(&format!("local.get {}", cmp));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_swap_assoc_entries(name, index, module);
    module.body().close("end");
}

pub(super) fn emit_uasort_float_compare_swap(
    name: &str,
    index: usize,
    callback: &str,
    module: &mut WasmModule,
) {
    let left = module.next_label("uasort_float_left");
    let right = module.next_label("uasort_float_right");
    let cmp = module.next_label("uasort_float_cmp");
    for local in [&left, &right] {
        module.declare_f64_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(cmp.trim_start_matches('$').to_string());
    emit_load_assoc_value_payload_f64(name, index, &left, module);
    emit_load_assoc_value_payload_f64(name, index + 1, &right, module);
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set {}", cmp));
    module.body().line(&format!("local.get {}", cmp));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_swap_assoc_entries(name, index, module);
    module.body().close("end");
}

pub(super) fn emit_uasort_float_compare_swap_instance(
    name: &str,
    index: usize,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) {
    let left = module.next_label("uasort_float_instance_left");
    let right = module.next_label("uasort_float_instance_right");
    let cmp = module.next_label("uasort_float_instance_cmp");
    for local in [&left, &right] {
        module.declare_f64_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(cmp.trim_start_matches('$').to_string());
    emit_load_assoc_value_payload_f64(name, index, &left, module);
    emit_load_assoc_value_payload_f64(name, index + 1, &right, module);
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set {}", cmp));
    module.body().line(&format!("local.get {}", cmp));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_swap_assoc_entries(name, index, module);
    module.body().close("end");
}

pub(super) fn emit_uasort_bool_compare_swap_instance(
    name: &str,
    index: usize,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) {
    let left = module.next_label("uasort_bool_instance_left");
    let right = module.next_label("uasort_bool_instance_right");
    let left_raw = module.next_label("uasort_bool_instance_left_raw");
    let right_raw = module.next_label("uasort_bool_instance_right_raw");
    let cmp = module.next_label("uasort_bool_instance_cmp");
    for local in [&left, &right] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left_raw, &right_raw, &cmp] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    emit_load_assoc_value_payload_i64(name, index, &left_raw, module);
    module.body().line(&format!("local.get {}", left_raw));
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().line(&format!("local.set {}", left));
    emit_load_assoc_value_payload_i64(name, index + 1, &right_raw, module);
    module.body().line(&format!("local.get {}", right_raw));
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set {}", cmp));
    module.body().line(&format!("local.get {}", cmp));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_swap_assoc_entries(name, index, module);
    module.body().close("end");
}

pub(super) fn emit_uasort_bool_compare_swap(
    name: &str,
    index: usize,
    callback: &str,
    module: &mut WasmModule,
) {
    let left = module.next_label("uasort_bool_left");
    let right = module.next_label("uasort_bool_right");
    let left_raw = module.next_label("uasort_bool_left_raw");
    let right_raw = module.next_label("uasort_bool_right_raw");
    let cmp = module.next_label("uasort_bool_cmp");
    for local in [&left, &right] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left_raw, &right_raw, &cmp] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    emit_load_assoc_value_payload_i64(name, index, &left_raw, module);
    module.body().line(&format!("local.get {}", left_raw));
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().line(&format!("local.set {}", left));
    emit_load_assoc_value_payload_i64(name, index + 1, &right_raw, module);
    module.body().line(&format!("local.get {}", right_raw));
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set {}", cmp));
    module.body().line(&format!("local.get {}", cmp));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_swap_assoc_entries(name, index, module);
    module.body().close("end");
}

pub(super) fn emit_uasort_runtime_int_compare_sort(
    name: &str,
    callback: &str,
    module: &mut WasmModule,
) {
    let pass = module.next_label("uasort_runtime_pass");
    let index = module.next_label("uasort_runtime_index");
    let outer_done = module.next_label("uasort_runtime_outer_done");
    let outer_loop = module.next_label("uasort_runtime_outer_loop");
    let inner_done = module.next_label("uasort_runtime_inner_done");
    let inner_loop = module.next_label("uasort_runtime_inner_loop");
    for local in [&pass, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pass));
    module.body().open(&format!("block {}", outer_done));
    module.body().open(&format!("loop {}", outer_loop));
    module.body().line(&format!("local.get {}", pass));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", outer_done));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", inner_done));
    module.body().open(&format!("loop {}", inner_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.sub");
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", inner_done));
    emit_uasort_runtime_int_compare_swap(name, &index, callback, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", inner_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", pass));
    module.body().line(&format!("br {}", outer_loop));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_uasort_runtime_float_compare_sort(
    name: &str,
    callback: &str,
    module: &mut WasmModule,
) {
    let pass = module.next_label("uasort_float_runtime_pass");
    let index = module.next_label("uasort_float_runtime_index");
    let outer_done = module.next_label("uasort_float_runtime_outer_done");
    let outer_loop = module.next_label("uasort_float_runtime_outer_loop");
    let inner_done = module.next_label("uasort_float_runtime_inner_done");
    let inner_loop = module.next_label("uasort_float_runtime_inner_loop");
    for local in [&pass, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pass));
    module.body().open(&format!("block {}", outer_done));
    module.body().open(&format!("loop {}", outer_loop));
    module.body().line(&format!("local.get {}", pass));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", outer_done));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", inner_done));
    module.body().open(&format!("loop {}", inner_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.sub");
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", inner_done));
    emit_uasort_runtime_float_compare_swap(name, &index, callback, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", inner_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", pass));
    module.body().line(&format!("br {}", outer_loop));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_uasort_runtime_int_compare_sort_instance(
    name: &str,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) {
    let pass = module.next_label("uasort_instance_runtime_pass");
    let index = module.next_label("uasort_instance_runtime_index");
    let outer_done = module.next_label("uasort_instance_runtime_outer_done");
    let outer_loop = module.next_label("uasort_instance_runtime_outer_loop");
    let inner_done = module.next_label("uasort_instance_runtime_inner_done");
    let inner_loop = module.next_label("uasort_instance_runtime_inner_loop");
    for local in [&pass, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pass));
    module.body().open(&format!("block {}", outer_done));
    module.body().open(&format!("loop {}", outer_loop));
    module.body().line(&format!("local.get {}", pass));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", outer_done));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", inner_done));
    module.body().open(&format!("loop {}", inner_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.sub");
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", inner_done));
    emit_uasort_runtime_int_compare_swap_instance(name, &index, callback, capture_local, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", inner_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", pass));
    module.body().line(&format!("br {}", outer_loop));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_uasort_runtime_float_compare_sort_instance(
    name: &str,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) {
    let pass = module.next_label("uasort_float_instance_runtime_pass");
    let index = module.next_label("uasort_float_instance_runtime_index");
    let outer_done = module.next_label("uasort_float_instance_runtime_outer_done");
    let outer_loop = module.next_label("uasort_float_instance_runtime_outer_loop");
    let inner_done = module.next_label("uasort_float_instance_runtime_inner_done");
    let inner_loop = module.next_label("uasort_float_instance_runtime_inner_loop");
    for local in [&pass, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pass));
    module.body().open(&format!("block {}", outer_done));
    module.body().open(&format!("loop {}", outer_loop));
    module.body().line(&format!("local.get {}", pass));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", outer_done));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", inner_done));
    module.body().open(&format!("loop {}", inner_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.sub");
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", inner_done));
    emit_uasort_runtime_float_compare_swap_instance(name, &index, callback, capture_local, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", inner_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", pass));
    module.body().line(&format!("br {}", outer_loop));
    module.body().close("end");
    module.body().close("end");
}

fn emit_uasort_runtime_float_compare_swap(
    name: &str,
    index: &str,
    callback: &str,
    module: &mut WasmModule,
) {
    let left_entry = module.next_label("uasort_float_runtime_left_entry");
    let right_entry = module.next_label("uasort_float_runtime_right_entry");
    let left_cell = module.next_label("uasort_float_runtime_left_cell");
    let right_cell = module.next_label("uasort_float_runtime_right_cell");
    let left = module.next_label("uasort_float_runtime_left");
    let right = module.next_label("uasort_float_runtime_right");
    let cmp = module.next_label("uasort_float_runtime_cmp");
    for local in [&left_entry, &right_entry, &left_cell, &right_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left, &right] {
        module.declare_f64_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(cmp.trim_start_matches('$').to_string());
    emit_runtime_assoc_neighbor_value_cells(name, index, &left_entry, &right_entry, &left_cell, &right_cell, module);
    module.body().line(&format!("local.get {}", left_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get {}", right_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set {}", cmp));
    module.body().line(&format!("local.get {}", cmp));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_swap_dynamic_assoc_entries(&left_entry, &right_entry, module);
    module.body().close("end");
}

fn emit_uasort_runtime_int_compare_swap(
    name: &str,
    index: &str,
    callback: &str,
    module: &mut WasmModule,
) {
    let left_entry = module.next_label("uasort_runtime_left_entry");
    let right_entry = module.next_label("uasort_runtime_right_entry");
    let left_cell = module.next_label("uasort_runtime_left_cell");
    let right_cell = module.next_label("uasort_runtime_right_cell");
    let left = module.next_label("uasort_runtime_left");
    let right = module.next_label("uasort_runtime_right");
    let cmp = module.next_label("uasort_runtime_cmp");
    for local in [&left_entry, &right_entry, &left_cell, &right_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left, &right, &cmp] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    emit_runtime_assoc_neighbor_value_cells(name, index, &left_entry, &right_entry, &left_cell, &right_cell, module);
    module.body().line(&format!("local.get {}", left_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get {}", right_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set {}", cmp));
    module.body().line(&format!("local.get {}", cmp));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_swap_dynamic_assoc_entries(&left_entry, &right_entry, module);
    module.body().close("end");
}

pub(super) fn emit_uasort_runtime_bool_compare_sort_instance(
    name: &str,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) {
    let pass = module.next_label("uasort_bool_instance_runtime_pass");
    let index = module.next_label("uasort_bool_instance_runtime_index");
    let outer_done = module.next_label("uasort_bool_instance_runtime_outer_done");
    let outer_loop = module.next_label("uasort_bool_instance_runtime_outer_loop");
    let inner_done = module.next_label("uasort_bool_instance_runtime_inner_done");
    let inner_loop = module.next_label("uasort_bool_instance_runtime_inner_loop");
    for local in [&pass, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pass));
    module.body().open(&format!("block {}", outer_done));
    module.body().open(&format!("loop {}", outer_loop));
    module.body().line(&format!("local.get {}", pass));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", outer_done));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", inner_done));
    module.body().open(&format!("loop {}", inner_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.sub");
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", inner_done));
    emit_uasort_runtime_bool_compare_swap_instance(name, &index, callback, capture_local, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", inner_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", pass));
    module.body().line(&format!("br {}", outer_loop));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_uasort_runtime_bool_compare_sort(
    name: &str,
    callback: &str,
    module: &mut WasmModule,
) {
    let pass = module.next_label("uasort_bool_runtime_pass");
    let index = module.next_label("uasort_bool_runtime_index");
    let outer_done = module.next_label("uasort_bool_runtime_outer_done");
    let outer_loop = module.next_label("uasort_bool_runtime_outer_loop");
    let inner_done = module.next_label("uasort_bool_runtime_inner_done");
    let inner_loop = module.next_label("uasort_bool_runtime_inner_loop");
    for local in [&pass, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pass));
    module.body().open(&format!("block {}", outer_done));
    module.body().open(&format!("loop {}", outer_loop));
    module.body().line(&format!("local.get {}", pass));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", outer_done));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", inner_done));
    module.body().open(&format!("loop {}", inner_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.sub");
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", inner_done));
    emit_uasort_runtime_bool_compare_swap(name, &index, callback, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", inner_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", pass));
    module.body().line(&format!("br {}", outer_loop));
    module.body().close("end");
    module.body().close("end");
}

fn emit_uasort_runtime_int_compare_swap_instance(
    name: &str,
    index: &str,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) {
    let left_entry = module.next_label("uasort_instance_runtime_left_entry");
    let right_entry = module.next_label("uasort_instance_runtime_right_entry");
    let left_cell = module.next_label("uasort_instance_runtime_left_cell");
    let right_cell = module.next_label("uasort_instance_runtime_right_cell");
    let left = module.next_label("uasort_instance_runtime_left");
    let right = module.next_label("uasort_instance_runtime_right");
    let cmp = module.next_label("uasort_instance_runtime_cmp");
    for local in [&left_entry, &right_entry, &left_cell, &right_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left, &right, &cmp] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    emit_runtime_assoc_neighbor_value_cells(
        name,
        index,
        &left_entry,
        &right_entry,
        &left_cell,
        &right_cell,
        module,
    );
    module.body().line(&format!("local.get {}", left_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get {}", right_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set {}", cmp));
    module.body().line(&format!("local.get {}", cmp));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_swap_dynamic_assoc_entries(&left_entry, &right_entry, module);
    module.body().close("end");
}

fn emit_uasort_runtime_bool_compare_swap_instance(
    name: &str,
    index: &str,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) {
    let left_entry = module.next_label("uasort_bool_instance_runtime_left_entry");
    let right_entry = module.next_label("uasort_bool_instance_runtime_right_entry");
    let left_cell = module.next_label("uasort_bool_instance_runtime_left_cell");
    let right_cell = module.next_label("uasort_bool_instance_runtime_right_cell");
    let left = module.next_label("uasort_bool_instance_runtime_left");
    let right = module.next_label("uasort_bool_instance_runtime_right");
    let cmp = module.next_label("uasort_bool_instance_runtime_cmp");
    for local in [&left_entry, &right_entry, &left_cell, &right_cell, &left, &right] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(cmp.trim_start_matches('$').to_string());
    emit_runtime_assoc_neighbor_value_cells(
        name,
        index,
        &left_entry,
        &right_entry,
        &left_cell,
        &right_cell,
        module,
    );
    module.body().line(&format!("local.get {}", left_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get {}", right_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set {}", cmp));
    module.body().line(&format!("local.get {}", cmp));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_swap_dynamic_assoc_entries(&left_entry, &right_entry, module);
    module.body().close("end");
}

fn emit_uasort_runtime_bool_compare_swap(
    name: &str,
    index: &str,
    callback: &str,
    module: &mut WasmModule,
) {
    let left_entry = module.next_label("uasort_bool_runtime_left_entry");
    let right_entry = module.next_label("uasort_bool_runtime_right_entry");
    let left_cell = module.next_label("uasort_bool_runtime_left_cell");
    let right_cell = module.next_label("uasort_bool_runtime_right_cell");
    let left = module.next_label("uasort_bool_runtime_left");
    let right = module.next_label("uasort_bool_runtime_right");
    let cmp = module.next_label("uasort_bool_runtime_cmp");
    for local in [&left_entry, &right_entry, &left_cell, &right_cell, &left, &right] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(cmp.trim_start_matches('$').to_string());
    emit_runtime_assoc_neighbor_value_cells(
        name,
        index,
        &left_entry,
        &right_entry,
        &left_cell,
        &right_cell,
        module,
    );
    module.body().line(&format!("local.get {}", left_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get {}", right_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set {}", cmp));
    module.body().line(&format!("local.get {}", cmp));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_swap_dynamic_assoc_entries(&left_entry, &right_entry, module);
    module.body().close("end");
}

fn emit_uasort_runtime_float_compare_swap_instance(
    name: &str,
    index: &str,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) {
    let left_entry = module.next_label("uasort_float_instance_runtime_left_entry");
    let right_entry = module.next_label("uasort_float_instance_runtime_right_entry");
    let left_cell = module.next_label("uasort_float_instance_runtime_left_cell");
    let right_cell = module.next_label("uasort_float_instance_runtime_right_cell");
    let left = module.next_label("uasort_float_instance_runtime_left");
    let right = module.next_label("uasort_float_instance_runtime_right");
    let cmp = module.next_label("uasort_float_instance_runtime_cmp");
    for local in [&left_entry, &right_entry, &left_cell, &right_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left, &right] {
        module.declare_f64_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(cmp.trim_start_matches('$').to_string());
    emit_runtime_assoc_neighbor_value_cells(
        name,
        index,
        &left_entry,
        &right_entry,
        &left_cell,
        &right_cell,
        module,
    );
    module.body().line(&format!("local.get {}", left_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get {}", right_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set {}", cmp));
    module.body().line(&format!("local.get {}", cmp));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_swap_dynamic_assoc_entries(&left_entry, &right_entry, module);
    module.body().close("end");
}

pub(super) fn emit_uasort_string_compare_swap(
    name: &str,
    index: usize,
    callback: &str,
    module: &mut WasmModule,
) {
    let left_ptr = module.next_label("uasort_string_left_ptr");
    let left_len = module.next_label("uasort_string_left_len");
    let right_ptr = module.next_label("uasort_string_right_ptr");
    let right_len = module.next_label("uasort_string_right_len");
    let cmp = module.next_label("uasort_string_cmp");
    for local in [&left_ptr, &left_len, &right_ptr, &right_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(cmp.trim_start_matches('$').to_string());
    emit_load_assoc_value_payload_i32(name, index, 8, &left_ptr, module);
    emit_load_assoc_value_payload_i32(name, index, 12, &left_len, module);
    emit_load_assoc_value_payload_i32(name, index + 1, 8, &right_ptr, module);
    emit_load_assoc_value_payload_i32(name, index + 1, 12, &right_len, module);
    module.body().line(&format!("local.get {}", left_ptr));
    module.body().line(&format!("local.get {}", left_len));
    module.body().line(&format!("local.get {}", right_ptr));
    module.body().line(&format!("local.get {}", right_len));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set {}", cmp));
    module.body().line(&format!("local.get {}", cmp));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_swap_assoc_entries(name, index, module);
    module.body().close("end");
}

pub(super) fn emit_uasort_string_compare_swap_instance(
    name: &str,
    index: usize,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) {
    let left_ptr = module.next_label("uasort_string_instance_left_ptr");
    let left_len = module.next_label("uasort_string_instance_left_len");
    let right_ptr = module.next_label("uasort_string_instance_right_ptr");
    let right_len = module.next_label("uasort_string_instance_right_len");
    let cmp = module.next_label("uasort_string_instance_cmp");
    for local in [&left_ptr, &left_len, &right_ptr, &right_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(cmp.trim_start_matches('$').to_string());
    emit_load_assoc_value_payload_i32(name, index, 8, &left_ptr, module);
    emit_load_assoc_value_payload_i32(name, index, 12, &left_len, module);
    emit_load_assoc_value_payload_i32(name, index + 1, 8, &right_ptr, module);
    emit_load_assoc_value_payload_i32(name, index + 1, 12, &right_len, module);
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get {}", left_ptr));
    module.body().line(&format!("local.get {}", left_len));
    module.body().line(&format!("local.get {}", right_ptr));
    module.body().line(&format!("local.get {}", right_len));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set {}", cmp));
    module.body().line(&format!("local.get {}", cmp));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_swap_assoc_entries(name, index, module);
    module.body().close("end");
}

pub(super) fn emit_uasort_runtime_string_compare_sort(
    name: &str,
    callback: &str,
    module: &mut WasmModule,
) {
    let pass = module.next_label("uasort_string_runtime_pass");
    let index = module.next_label("uasort_string_runtime_index");
    let outer_done = module.next_label("uasort_string_runtime_outer_done");
    let outer_loop = module.next_label("uasort_string_runtime_outer_loop");
    let inner_done = module.next_label("uasort_string_runtime_inner_done");
    let inner_loop = module.next_label("uasort_string_runtime_inner_loop");
    for local in [&pass, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pass));
    module.body().open(&format!("block {}", outer_done));
    module.body().open(&format!("loop {}", outer_loop));
    module.body().line(&format!("local.get {}", pass));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", outer_done));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", inner_done));
    module.body().open(&format!("loop {}", inner_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.sub");
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", inner_done));
    emit_uasort_runtime_string_compare_swap(name, &index, callback, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", inner_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", pass));
    module.body().line(&format!("br {}", outer_loop));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_uasort_runtime_string_compare_sort_instance(
    name: &str,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) {
    let pass = module.next_label("uasort_string_instance_runtime_pass");
    let index = module.next_label("uasort_string_instance_runtime_index");
    let outer_done = module.next_label("uasort_string_instance_runtime_outer_done");
    let outer_loop = module.next_label("uasort_string_instance_runtime_outer_loop");
    let inner_done = module.next_label("uasort_string_instance_runtime_inner_done");
    let inner_loop = module.next_label("uasort_string_instance_runtime_inner_loop");
    for local in [&pass, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pass));
    module.body().open(&format!("block {}", outer_done));
    module.body().open(&format!("loop {}", outer_loop));
    module.body().line(&format!("local.get {}", pass));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", outer_done));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", inner_done));
    module.body().open(&format!("loop {}", inner_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.sub");
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", inner_done));
    emit_uasort_runtime_string_compare_swap_instance(name, &index, callback, capture_local, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", inner_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", pass));
    module.body().line(&format!("br {}", outer_loop));
    module.body().close("end");
    module.body().close("end");
}

fn emit_uasort_runtime_string_compare_swap(
    name: &str,
    index: &str,
    callback: &str,
    module: &mut WasmModule,
) {
    let left_entry = module.next_label("uasort_string_runtime_left_entry");
    let right_entry = module.next_label("uasort_string_runtime_right_entry");
    let left_cell = module.next_label("uasort_string_runtime_left_cell");
    let right_cell = module.next_label("uasort_string_runtime_right_cell");
    let left_ptr = module.next_label("uasort_string_runtime_left_ptr");
    let left_len = module.next_label("uasort_string_runtime_left_len");
    let right_ptr = module.next_label("uasort_string_runtime_right_ptr");
    let right_len = module.next_label("uasort_string_runtime_right_len");
    let cmp = module.next_label("uasort_string_runtime_cmp");
    for local in [
        &left_entry,
        &right_entry,
        &left_cell,
        &right_cell,
        &left_ptr,
        &left_len,
        &right_ptr,
        &right_len,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(cmp.trim_start_matches('$').to_string());
    emit_runtime_assoc_neighbor_value_cells(name, index, &left_entry, &right_entry, &left_cell, &right_cell, module);
    module.body().line(&format!("local.get {}", left_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", left_ptr));
    module.body().line(&format!("local.get {}", left_cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", left_len));
    module.body().line(&format!("local.get {}", right_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", right_ptr));
    module.body().line(&format!("local.get {}", right_cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", right_len));
    module.body().line(&format!("local.get {}", left_ptr));
    module.body().line(&format!("local.get {}", left_len));
    module.body().line(&format!("local.get {}", right_ptr));
    module.body().line(&format!("local.get {}", right_len));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set {}", cmp));
    module.body().line(&format!("local.get {}", cmp));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_swap_dynamic_assoc_entries(&left_entry, &right_entry, module);
    module.body().close("end");
}

fn emit_uasort_runtime_string_compare_swap_instance(
    name: &str,
    index: &str,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) {
    let left_entry = module.next_label("uasort_string_instance_runtime_left_entry");
    let right_entry = module.next_label("uasort_string_instance_runtime_right_entry");
    let left_cell = module.next_label("uasort_string_instance_runtime_left_cell");
    let right_cell = module.next_label("uasort_string_instance_runtime_right_cell");
    let left_ptr = module.next_label("uasort_string_instance_runtime_left_ptr");
    let left_len = module.next_label("uasort_string_instance_runtime_left_len");
    let right_ptr = module.next_label("uasort_string_instance_runtime_right_ptr");
    let right_len = module.next_label("uasort_string_instance_runtime_right_len");
    let cmp = module.next_label("uasort_string_instance_runtime_cmp");
    for local in [
        &left_entry,
        &right_entry,
        &left_cell,
        &right_cell,
        &left_ptr,
        &left_len,
        &right_ptr,
        &right_len,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(cmp.trim_start_matches('$').to_string());
    emit_runtime_assoc_neighbor_value_cells(
        name,
        index,
        &left_entry,
        &right_entry,
        &left_cell,
        &right_cell,
        module,
    );
    module.body().line(&format!("local.get {}", left_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", left_ptr));
    module.body().line(&format!("local.get {}", left_cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", left_len));
    module.body().line(&format!("local.get {}", right_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", right_ptr));
    module.body().line(&format!("local.get {}", right_cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", right_len));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get {}", left_ptr));
    module.body().line(&format!("local.get {}", left_len));
    module.body().line(&format!("local.get {}", right_ptr));
    module.body().line(&format!("local.get {}", right_len));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set {}", cmp));
    module.body().line(&format!("local.get {}", cmp));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_swap_dynamic_assoc_entries(&left_entry, &right_entry, module);
    module.body().close("end");
}
