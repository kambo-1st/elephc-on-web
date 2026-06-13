//! Purpose:
//! Emits heap-returning wordwrap() lowering for wasm32-web string values.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::wordwrap`.
//!
//! Key details:
//! - Builds the wrapped string in linear memory and leaves pointer/length on the stack.
//! - Dynamic widths are clamped like the direct-output wordwrap path.

use super::*;

#[derive(Clone, Copy)]
pub(super) enum WasmWordwrapWidth<'a> {
    Static(i64),
    Local(&'a str),
}

pub(super) fn emit_runtime_wordwrap_value_to_stack(
    var: &str,
    width: WasmWordwrapWidth<'_>,
    break_text: WasmBreakText<'_>,
    cut: WasmWordwrapCut<'_>,
    module: &mut WasmModule,
) {
    let idx = module.next_label("wrap_value_idx");
    let word_start = module.next_label("wrap_value_word_start");
    let word_end = module.next_label("wrap_value_word_end");
    let word_len = module.next_label("wrap_value_word_len");
    let line_len = module.next_label("wrap_value_line_len");
    let pending = module.next_label("wrap_value_pending");
    let chunk_start = module.next_label("wrap_value_chunk_start");
    let chunk_end = module.next_label("wrap_value_chunk_end");
    let chunk_remaining = module.next_label("wrap_value_chunk_remaining");
    let out_ptr = module.next_label("wrap_value_ptr");
    let out_len = module.next_label("wrap_value_len");
    let out_idx = module.next_label("wrap_value_out_idx");
    let result_ptr = module.next_label("wrap_value_result_ptr");
    let result_len = module.next_label("wrap_value_result_len");
    let byte = module.next_label("wrap_value_byte");
    let chunk_loop = module.next_label("wrap_value_chunk_loop");
    let chunk_done = module.next_label("wrap_value_chunk_done");
    let scan_loop = module.next_label("wrap_value_scan");
    let scan_done = module.next_label("wrap_value_scan_done");
    let loop_label = module.next_label("wrap_value_loop");
    let done_label = module.next_label("wrap_value_done");
    let literal_break = match break_text {
        WasmBreakText::Literal(value) => Some(module.intern_string(value)),
        WasmBreakText::Variable(_) => None,
    };
    for local in [
        &idx,
        &word_start,
        &word_end,
        &word_len,
        &line_len,
        &pending,
        &chunk_start,
        &chunk_end,
        &chunk_remaining,
        &out_ptr,
        &out_len,
        &out_idx,
        &result_ptr,
        &result_len,
        &byte,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_len", var));
    emit_wordwrap_width_value(width, module);
    module.body().line("i32.le_u");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.set {}", result_ptr));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.set {}", result_len));
    module.body().line("else");
    emit_runtime_wordwrap_value_capacity(var, break_text, &out_len, module);
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", line_len));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.set {}", word_start));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.set {}", word_end));
    module.body().open(&format!("block {}", scan_done));
    module.body().open(&format!("loop {}", scan_loop));
    module.body().line(&format!("local.get {}", word_end));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", scan_done));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", word_end));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.const 32");
    module.body().line("i32.eq");
    module.body().line(&format!("br_if {}", scan_done));
    module.body().line(&format!("local.get {}", word_end));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", word_end));
    module.body().line(&format!("br {}", scan_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", word_end));
    module.body().line(&format!("local.get {}", word_start));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", word_len));
    let close_cut_guard = emit_wordwrap_cut_guard_start(cut.clone(), module);
    if !matches!(cut, WasmWordwrapCut::Static(false)) {
        module.body().line(&format!("local.get {}", word_len));
        emit_wordwrap_width_value(width, module);
        module.body().line("i32.gt_u");
        module.body().open("if");
        module.body().line(&format!("local.get {}", line_len));
        module.body().line("i32.eqz");
        module.body().line("i32.eqz");
        module.body().open("if");
        emit_copy_wordwrap_break(break_text, literal_break, &out_ptr, &out_idx, module);
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", line_len));
        module.body().close("end");
        module.body().line(&format!("local.get {}", word_start));
        module.body().line(&format!("local.set {}", chunk_start));
        module.body().line(&format!("local.get {}", word_len));
        module.body().line(&format!("local.set {}", chunk_remaining));
        module.body().open(&format!("block {}", chunk_done));
        module.body().open(&format!("loop {}", chunk_loop));
        module.body().line(&format!("local.get {}", chunk_remaining));
        emit_wordwrap_width_value(width, module);
        module.body().line("i32.le_u");
        module.body().line(&format!("br_if {}", chunk_done));
        module.body().line(&format!("local.get {}", chunk_start));
        emit_wordwrap_width_value(width, module);
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", chunk_end));
        emit_copy_string_range_to_memory(var, &chunk_start, &chunk_end, &out_ptr, &out_idx, module);
        emit_copy_wordwrap_break(break_text, literal_break, &out_ptr, &out_idx, module);
        module.body().line(&format!("local.get {}", chunk_end));
        module.body().line(&format!("local.set {}", chunk_start));
        module.body().line(&format!("local.get {}", chunk_remaining));
        emit_wordwrap_width_value(width, module);
        module.body().line("i32.sub");
        module.body().line(&format!("local.set {}", chunk_remaining));
        module.body().line(&format!("br {}", chunk_loop));
        module.body().close("end");
        module.body().close("end");
        module.body().line(&format!("local.get {}", word_end));
        module.body().line(&format!("local.set {}", chunk_end));
        emit_copy_string_range_to_memory(var, &chunk_start, &chunk_end, &out_ptr, &out_idx, module);
        module.body().line(&format!("local.get {}", chunk_remaining));
        module.body().line(&format!("local.set {}", line_len));
        module.body().line(&format!("local.get {}", word_end));
        module.body().line("i32.const 1");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", idx));
        module.body().line(&format!("br {}", loop_label));
        module.body().close("end");
    }
    emit_wordwrap_cut_guard_end(close_cut_guard, module);
    module.body().line(&format!("local.get {}", line_len));
    module.body().line("i32.eqz");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", word_len));
    module.body().line("else");
    module.body().line(&format!("local.get {}", word_len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().close("end");
    module.body().line(&format!("local.set {}", pending));
    module.body().line(&format!("local.get {}", line_len));
    module.body().line("i32.eqz");
    module.body().line("i32.eqz");
    module.body().line(&format!("local.get {}", line_len));
    module.body().line(&format!("local.get {}", pending));
    module.body().line("i32.add");
    emit_wordwrap_width_value(width, module);
    module.body().line("i32.gt_u");
    module.body().line("i32.and");
    module.body().open("if");
    emit_copy_wordwrap_break(break_text, literal_break, &out_ptr, &out_idx, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", line_len));
    module.body().close("end");
    module.body().line(&format!("local.get {}", line_len));
    module.body().line("i32.eqz");
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("i32.const 32");
    module.body().line(&format!("local.set {}", byte));
    emit_store_byte_local(&byte, &out_ptr, &out_idx, module);
    module.body().line(&format!("local.get {}", line_len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", line_len));
    module.body().close("end");
    emit_copy_string_range_to_memory(var, &word_start, &word_end, &out_ptr, &out_idx, module);
    module.body().line(&format!("local.get {}", line_len));
    module.body().line(&format!("local.get {}", word_len));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", line_len));
    module.body().line(&format!("local.get {}", word_end));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.set {}", result_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line(&format!("local.set {}", result_len));
    module.body().close("end");
    module.body().line(&format!("local.get {}", result_ptr));
    module.body().line(&format!("local.get {}", result_len));
}

pub(super) fn emit_runtime_wordwrap_dynamic_value_to_stack(
    var: &str,
    width_expr: &Expr,
    break_text: WasmBreakText<'_>,
    cut: WasmWordwrapCut<'_>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let width64 = module.next_label("wrap_value_width64");
    let width = module.next_label("wrap_value_width");
    module.declare_i64_local(width64.trim_start_matches('$').to_string());
    module.declare_i32_local(width.trim_start_matches('$').to_string());
    require_int(width_expr, module)?;
    module.body().line(&format!("local.set {}", width64));
    module.body().line(&format!("local.get {}", width64));
    module.body().line("i64.const 0");
    module.body().line("i64.le_s");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", width));
    module.body().line("else");
    module.body().line(&format!("local.get {}", width64));
    module.body().line("i64.const 2147483647");
    module.body().line("i64.gt_s");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", width64));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", width));
    module.body().close("end");
    emit_runtime_wordwrap_value_to_stack(
        var,
        WasmWordwrapWidth::Local(&width),
        break_text,
        cut,
        module,
    );
    Ok(())
}

fn emit_runtime_wordwrap_value_capacity(
    var: &str,
    break_text: WasmBreakText<'_>,
    out_len: &str,
    module: &mut WasmModule,
) {
    match break_text {
        WasmBreakText::Literal(value) => {
            module.body().line(&format!("local.get ${}_len", var));
            module.body().line(&format!("i32.const {}", value.len().saturating_add(1).max(1)));
            module.body().line("i32.mul");
            module.body().line(&format!("local.set {}", out_len));
        }
        WasmBreakText::Variable(break_var) => {
            module.body().line(&format!("local.get ${}_len", break_var));
            module.body().line("i32.const 1");
            module.body().line("i32.add");
            module.body().line(&format!("local.get ${}_len", var));
            module.body().line("i32.mul");
            module.body().line(&format!("local.set {}", out_len));
        }
    }
}

fn emit_wordwrap_width_value(width: WasmWordwrapWidth<'_>, module: &mut WasmModule) {
    match width {
        WasmWordwrapWidth::Static(value) => {
            module.body().line(&format!("i32.const {}", value.max(1).min(i32::MAX as i64)));
        }
        WasmWordwrapWidth::Local(local) => {
            module.body().line(&format!("local.get {}", local));
        }
    }
}

fn emit_copy_wordwrap_break(
    break_text: WasmBreakText<'_>,
    literal_break: Option<(usize, usize)>,
    out_ptr: &str,
    out_idx: &str,
    module: &mut WasmModule,
) {
    match break_text {
        WasmBreakText::Literal(_) => {
            let (break_ptr, break_len) = literal_break.unwrap();
            emit_copy_static_range_to_memory(break_ptr, break_len, out_ptr, out_idx, module);
        }
        WasmBreakText::Variable(break_var) => emit_copy_string_to_memory(break_var, out_ptr, out_idx, module),
    }
}
