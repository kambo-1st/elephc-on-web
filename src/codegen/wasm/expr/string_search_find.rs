//! Purpose:
//! Emits low-level wasm32-web runtime string search loops for `strpos`/`strrpos` style builtins.
//! Owns offset validation, empty-needle positions, and forward/reverse byte scans.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::string_search`
//! - `crate::codegen::wasm::expr::string_strstr`
//!
//! Key details:
//! - Uses `found` locals to preserve PHP false-vs-zero distinction at higher lowering layers.

use super::*;

pub(super) enum SearchOffset {
    Static(i64),
    Dynamic(String),
}

pub(super) fn emit_runtime_empty_search_result(
    var: &str,
    reverse: bool,
    offset: &SearchOffset,
    module: &mut WasmModule,
) {
    let SearchOffset::Static(offset) = offset else {
        emit_runtime_empty_search_result_dynamic(var, reverse, offset, module);
        return;
    };
    let offset = *offset;
    if reverse && offset < 0 {
        emit_runtime_assert_strrpos_offset_in_range(var, offset, module);
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line(&format!("i32.const {}", (-offset).min(i32::MAX as i64)));
        module.body().line("i32.sub");
    } else if reverse {
        emit_runtime_assert_strrpos_offset_in_range(var, offset, module);
        module.body().line(&format!("local.get ${}_len", var));
    } else if offset < 0 {
        emit_runtime_assert_strpos_offset_in_range(var, offset, module);
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line(&format!("i32.const {}", (-offset).min(i32::MAX as i64)));
        module.body().line("i32.sub");
    } else {
        emit_runtime_assert_strpos_offset_in_range(var, offset, module);
        module.body().line(&format!("i32.const {}", offset.min(i32::MAX as i64)));
    }
}

fn emit_runtime_empty_search_result_dynamic(
    var: &str,
    reverse: bool,
    offset: &SearchOffset,
    module: &mut WasmModule,
) {
    let SearchOffset::Dynamic(offset_local) = offset else {
        unreachable!();
    };
    emit_runtime_assert_search_offset_in_range_dynamic(var, offset_local, module);
    if reverse {
        module.body().line(&format!("local.get ${}", offset_local));
        module.body().line("i64.const 0");
        module.body().line("i64.lt_s");
        module.body().open("if (result i32)");
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line(&format!("local.get ${}", offset_local));
        module.body().line("i32.wrap_i64");
        module.body().line("i32.add");
        module.body().line("else");
        module.body().line(&format!("local.get ${}_len", var));
        module.body().close("end");
    } else {
        emit_runtime_forward_search_start(var, offset, module);
    }
}

fn emit_runtime_assert_strpos_offset_in_range(var: &str, offset: i64, module: &mut WasmModule) {
    if offset < 0 {
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line(&format!("i32.const {}", (-offset).min(i32::MAX as i64)));
        module.body().line("i32.lt_u");
        module.body().open("if");
        module.body().line("unreachable");
        module.body().close("end");
    } else {
        module.body().line(&format!("i32.const {}", offset.min(i32::MAX as i64)));
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line("i32.gt_u");
        module.body().open("if");
        module.body().line("unreachable");
        module.body().close("end");
    }
}

fn emit_runtime_assert_strrpos_offset_in_range(var: &str, offset: i64, module: &mut WasmModule) {
    if offset < 0 {
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line(&format!("i32.const {}", (-offset).min(i32::MAX as i64)));
        module.body().line("i32.lt_u");
        module.body().open("if");
        module.body().line("unreachable");
        module.body().close("end");
    } else {
        module.body().line(&format!("i32.const {}", offset.min(i32::MAX as i64)));
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line("i32.gt_u");
        module.body().open("if");
        module.body().line("unreachable");
        module.body().close("end");
    }
}

fn emit_runtime_assert_search_offset_in_range_dynamic(
    var: &str,
    offset_local: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}", offset_local));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i64.extend_i32_u");
    module.body().line(&format!("local.get ${}", offset_local));
    module.body().line("i64.add");
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line("else");
    module.body().line(&format!("local.get ${}", offset_local));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i64.extend_i32_u");
    module.body().line("i64.gt_u");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
}

fn emit_runtime_forward_search_start(var: &str, offset: &SearchOffset, module: &mut WasmModule) {
    let SearchOffset::Static(offset) = offset else {
        emit_runtime_forward_search_start_dynamic(var, offset, module);
        return;
    };
    let offset = *offset;
    emit_runtime_assert_strpos_offset_in_range(var, offset, module);
    if offset < 0 {
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line(&format!("i32.const {}", (-offset).min(i32::MAX as i64)));
        module.body().line("i32.sub");
    } else {
        module.body().line(&format!("i32.const {}", offset.min(i32::MAX as i64)));
    }
}

fn emit_runtime_forward_search_start_dynamic(
    var: &str,
    offset: &SearchOffset,
    module: &mut WasmModule,
) {
    let SearchOffset::Dynamic(offset_local) = offset else {
        unreachable!();
    };
    emit_runtime_assert_search_offset_in_range_dynamic(var, offset_local, module);
    module.body().line(&format!("local.get ${}", offset_local));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get ${}", offset_local));
    module.body().line("i32.wrap_i64");
    module.body().line("i32.add");
    module.body().line("else");
    module.body().line(&format!("local.get ${}", offset_local));
    module.body().line("i32.wrap_i64");
    module.body().close("end");
}

fn emit_runtime_strrpos_max_start(
    var: &str,
    idx_local: &str,
    needle_len: usize,
    offset: &SearchOffset,
    module: &mut WasmModule,
) {
    let SearchOffset::Static(offset) = offset else {
        emit_runtime_strrpos_max_start_dynamic(var, idx_local, needle_len, offset, module);
        return;
    };
    let offset = *offset;
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("i32.const {}", needle_len));
    module.body().line("i32.sub");
    if offset < 0 {
        module.body().line(&format!("local.set {}", idx_local));
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line(&format!("i32.const {}", (-offset).min(i32::MAX as i64)));
        module.body().line("i32.sub");
        module.body().line(&format!("local.get {}", idx_local));
        module.body().line("i32.lt_u");
        module.body().open("if (result i32)");
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line(&format!("i32.const {}", (-offset).min(i32::MAX as i64)));
        module.body().line("i32.sub");
        module.body().line("else");
        module.body().line(&format!("local.get {}", idx_local));
        module.body().close("end");
    }
}

fn emit_runtime_strrpos_max_start_dynamic(
    var: &str,
    idx_local: &str,
    needle_len: usize,
    offset: &SearchOffset,
    module: &mut WasmModule,
) {
    let SearchOffset::Dynamic(offset_local) = offset else {
        unreachable!();
    };
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("i32.const {}", needle_len));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", idx_local));
    module.body().line(&format!("local.get ${}", offset_local));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get ${}", offset_local));
    module.body().line("i32.wrap_i64");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", idx_local));
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get ${}", offset_local));
    module.body().line("i32.wrap_i64");
    module.body().line("i32.add");
    module.body().line("else");
    module.body().line(&format!("local.get {}", idx_local));
    module.body().close("end");
    module.body().line("else");
    module.body().line(&format!("local.get {}", idx_local));
    module.body().close("end");
}

fn emit_runtime_strrpos_var_max_start(
    var: &str,
    needle_var: &str,
    idx_local: &str,
    offset: &SearchOffset,
    module: &mut WasmModule,
) {
    let SearchOffset::Static(offset) = offset else {
        emit_runtime_strrpos_var_max_start_dynamic(var, needle_var, idx_local, offset, module);
        return;
    };
    let offset = *offset;
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get ${}_len", needle_var));
    module.body().line("i32.sub");
    if offset < 0 {
        module.body().line(&format!("local.set {}", idx_local));
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line(&format!("i32.const {}", (-offset).min(i32::MAX as i64)));
        module.body().line("i32.sub");
        module.body().line(&format!("local.get {}", idx_local));
        module.body().line("i32.lt_u");
        module.body().open("if (result i32)");
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line(&format!("i32.const {}", (-offset).min(i32::MAX as i64)));
        module.body().line("i32.sub");
        module.body().line("else");
        module.body().line(&format!("local.get {}", idx_local));
        module.body().close("end");
    }
}

fn emit_runtime_strrpos_var_max_start_dynamic(
    var: &str,
    needle_var: &str,
    idx_local: &str,
    offset: &SearchOffset,
    module: &mut WasmModule,
) {
    let SearchOffset::Dynamic(offset_local) = offset else {
        unreachable!();
    };
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get ${}_len", needle_var));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", idx_local));
    module.body().line(&format!("local.get ${}", offset_local));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get ${}", offset_local));
    module.body().line("i32.wrap_i64");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", idx_local));
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get ${}", offset_local));
    module.body().line("i32.wrap_i64");
    module.body().line("i32.add");
    module.body().line("else");
    module.body().line(&format!("local.get {}", idx_local));
    module.body().close("end");
    module.body().line("else");
    module.body().line(&format!("local.get {}", idx_local));
    module.body().close("end");
}

pub(super) fn emit_runtime_find_literal_forward(
    var: &str,
    needle: &str,
    start_offset: &SearchOffset,
    result_local: &str,
    found_local: &str,
    module: &mut WasmModule,
) {
    let idx = module.next_label("find_idx");
    let loop_label = module.next_label("find_loop");
    let done_label = module.next_label("find_done");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    emit_runtime_forward_search_start(var, start_offset, module);
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", needle.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.gt_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_runtime_literal_match_at(var, &idx, needle, module);
    module.body().open("if");
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.set {}", result_local));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found_local));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_find_var_forward(
    var: &str,
    needle_var: &str,
    start_offset: &SearchOffset,
    result_local: &str,
    found_local: &str,
    module: &mut WasmModule,
) {
    let idx = module.next_label("find_idx");
    let loop_label = module.next_label("find_loop");
    let done_label = module.next_label("find_done");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    emit_runtime_forward_search_start(var, start_offset, module);
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", needle_var));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.gt_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_runtime_var_match_at(var, &idx, needle_var, module);
    module.body().open("if");
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.set {}", result_local));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found_local));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_find_literal_reverse(
    var: &str,
    needle: &str,
    offset: &SearchOffset,
    result_local: &str,
    found_local: &str,
    module: &mut WasmModule,
) {
    let idx = module.next_label("rfind_idx");
    let min_start = module.next_label("rfind_min_start");
    let loop_label = module.next_label("rfind_loop");
    let done_label = module.next_label("rfind_done");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(min_start.trim_start_matches('$').to_string());
    emit_runtime_set_reverse_min_start(var, offset, &min_start, module);
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("i32.const {}", needle.len()));
    module.body().line("i32.lt_u");
    module.body().open("if");
    module.body().line("else");
    emit_runtime_strrpos_max_start(var, &idx, needle.len(), offset, module);
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get {}", min_start));
    module.body().line("i32.lt_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_runtime_literal_match_at(var, &idx, needle, module);
    module.body().open("if");
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.set {}", result_local));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found_local));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}
pub(super) fn emit_runtime_find_var_reverse(
    var: &str,
    needle_var: &str,
    offset: &SearchOffset,
    result_local: &str,
    found_local: &str,
    module: &mut WasmModule,
) {
    let idx = module.next_label("rfind_idx");
    let min_start = module.next_label("rfind_min_start");
    let loop_label = module.next_label("rfind_loop");
    let done_label = module.next_label("rfind_done");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(min_start.trim_start_matches('$').to_string());
    emit_runtime_set_reverse_min_start(var, offset, &min_start, module);
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get ${}_len", needle_var));
    module.body().line("i32.lt_u");
    module.body().open("if");
    module.body().line("else");
    emit_runtime_strrpos_var_max_start(var, needle_var, &idx, offset, module);
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get {}", min_start));
    module.body().line("i32.lt_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_runtime_var_match_at(var, &idx, needle_var, module);
    module.body().open("if");
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.set {}", result_local));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found_local));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

fn emit_runtime_set_reverse_min_start(
    var: &str,
    offset: &SearchOffset,
    min_start: &str,
    module: &mut WasmModule,
) {
    match offset {
        SearchOffset::Static(offset) => {
            emit_runtime_assert_strrpos_offset_in_range(var, *offset, module);
            if *offset >= 0 {
                module
                    .body()
                    .line(&format!("i32.const {}", (*offset).min(i32::MAX as i64)));
                module.body().line(&format!("local.set {}", min_start));
            } else {
                module.body().line("i32.const 0");
                module.body().line(&format!("local.set {}", min_start));
            }
        }
        SearchOffset::Dynamic(offset_local) => {
            emit_runtime_assert_search_offset_in_range_dynamic(var, offset_local, module);
            module.body().line(&format!("local.get ${}", offset_local));
            module.body().line("i64.const 0");
            module.body().line("i64.ge_s");
            module.body().open("if (result i32)");
            module.body().line(&format!("local.get ${}", offset_local));
            module.body().line("i32.wrap_i64");
            module.body().line("else");
            module.body().line("i32.const 0");
            module.body().close("end");
            module.body().line(&format!("local.set {}", min_start));
        }
    }
}
