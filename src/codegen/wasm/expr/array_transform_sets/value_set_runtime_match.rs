//! Purpose:
//! Emits runtime source-match loops for wasm32-web value-cell set operations.
//! Keeps mask scanning for array_diff/intersect/unique separate from membership orchestration.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets::value_compare`.
//!
//! Key details:
//! - Each loop preserves the comparison mode chosen by the caller: compact ints, scalar cells, strings, or mixed cells.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_value_set_runtime_compact_int_source_match(
    source_ptr: &str,
    index: &str,
    compare_ptr: &str,
    compare_len: &str,
    set_found: &str,
    candidate_found: &str,
    compare_index: &str,
    module: &mut WasmModule,
) {
    let candidate_value = module.next_label("value_array_compact_set_value");
    let done_label = module.next_label("value_array_compact_set_runtime_done");
    let loop_label = module.next_label("value_array_compact_set_runtime_loop");
    module.declare_i64_local(candidate_value.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", compare_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", compare_index));
    module.body().line(&format!("local.get {}", compare_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", compare_ptr));
    module.body().line(&format!("local.get {}", compare_index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", candidate_value));
    emit_value_cell_matches_i64_string_repr(source_ptr, index, &candidate_value, candidate_found, module);
    module.body().line(&format!("local.get {}", candidate_found));
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", set_found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", compare_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", compare_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(in crate::codegen::wasm::expr) fn emit_value_scalar_set_runtime_source_match(
    source_ptr: &str,
    index: &str,
    compare_ptr: &str,
    compare_len: &str,
    set_found: &str,
    candidate_found: &str,
    compare_index: &str,
    module: &mut WasmModule,
) {
    let done_label = module.next_label("value_array_scalar_set_runtime_done");
    let loop_label = module.next_label("value_array_scalar_set_runtime_loop");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", compare_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", compare_index));
    module.body().line(&format!("local.get {}", compare_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_scalar_cells_same_string_repr_between(
        source_ptr,
        index,
        compare_ptr,
        compare_index,
        candidate_found,
        module,
    );
    module.body().line(&format!("local.get {}", candidate_found));
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", set_found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", compare_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", compare_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(in crate::codegen::wasm::expr) fn emit_value_any_set_runtime_source_match(
    source_ptr: &str,
    index: &str,
    compare_ptr: &str,
    compare_len: &str,
    set_found: &str,
    candidate_found: &str,
    compare_index: &str,
    module: &mut WasmModule,
) {
    let done_label = module.next_label("value_array_any_set_runtime_done");
    let loop_label = module.next_label("value_array_any_set_runtime_loop");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", compare_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", compare_index));
    module.body().line(&format!("local.get {}", compare_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cells_same_string_repr_between(
        source_ptr,
        index,
        compare_ptr,
        compare_index,
        candidate_found,
        module,
    );
    module.body().line(&format!("local.get {}", candidate_found));
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", set_found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", compare_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", compare_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(in crate::codegen::wasm::expr) fn emit_value_string_set_runtime_source_match(
    source_ptr: &str,
    index: &str,
    compare_ptr: &str,
    compare_len: &str,
    set_found: &str,
    candidate_found: &str,
    compare_index: &str,
    module: &mut WasmModule,
) {
    let done_label = module.next_label("value_array_set_runtime_done");
    let loop_label = module.next_label("value_array_set_runtime_loop");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", compare_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", compare_index));
    module.body().line(&format!("local.get {}", compare_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_string_cells_equal_between(
        source_ptr,
        index,
        compare_ptr,
        compare_index,
        candidate_found,
        module,
    );
    module.body().line(&format!("local.get {}", candidate_found));
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", set_found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", compare_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", compare_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}
