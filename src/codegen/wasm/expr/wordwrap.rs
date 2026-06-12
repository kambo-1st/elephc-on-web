//! Purpose:
//! Lowers PHP wordwrap() calls for the wasm32-web backend.
//! Owns both direct-output word wrapping and heap materialization helpers.
//!
//! Called from:
//! - `crate::codegen::wasm::expr`
//! - `crate::codegen::wasm::expr::string_builtins`
//!
//! Key details:
//! - Keeps word wrapping isolated from the main expression dispatcher while reusing shared string stack/memory helpers.

mod value;

use super::*;
use value::{
    emit_runtime_wordwrap_dynamic_value_to_stack, emit_runtime_wordwrap_value_to_stack,
    WasmWordwrapWidth,
};

pub(super) fn emit_wordwrap_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !name.eq_ignore_ascii_case("wordwrap") {
        return Ok(false);
    }
    if args.is_empty() || args.len() > 4 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web wordwrap() expects one to four arguments",
        ));
    }
    if args.iter().all(|arg| static_string_value(arg, module).is_some() || static_int_value(arg).is_some() || matches!(arg.kind, ExprKind::BoolLiteral(_))) {
        let value = eval_output_string_builtin(call, name, args, module)?;
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(true);
    }
    let Some(var) = string_arg_or_materialize(&args[0], "wordwrap_value_arg", module)? else {
        return Ok(false);
    };
    let dynamic_width = args.get(1).filter(|arg| static_int_value(arg).is_none());
    let break_literal_storage = args.get(2).and_then(|arg| static_string_value(arg, module));
    let break_var_storage = if args.get(2).is_some() && break_literal_storage.is_none() {
        string_arg_or_materialize(&args[2], "wordwrap_value_break", module)?
    } else {
        None
    };
    let break_text = match args.get(2) {
        Some(arg) => {
            if let Some(value) = break_literal_storage.as_deref() {
                WasmBreakText::Literal(value)
            } else {
                let Some(break_var) = break_var_storage.as_deref() else {
                    return Ok(false);
                };
                let _ = arg;
                WasmBreakText::Variable(break_var)
            }
        }
        None => WasmBreakText::Literal("\n"),
    };
    let cut = args.get(3).map(literal_bool_arg).transpose()?.unwrap_or(false);
    if let Some(width_expr) = dynamic_width {
        emit_runtime_wordwrap_dynamic_value_to_stack(&var, width_expr, break_text, cut, module)?;
    } else {
        let width = args.get(1).and_then(static_int_value).unwrap_or(75);
        emit_runtime_wordwrap_value_to_stack(&var, WasmWordwrapWidth::Static(width), break_text, cut, module);
    }
    Ok(true)
}

#[derive(Clone, Copy)]
pub(super) enum WasmBreakText<'a> {
    Literal(&'a str),
    Variable(&'a str),
}

pub(super) fn emit_runtime_wordwrap(
    var: &str,
    width: i64,
    break_text: WasmBreakText<'_>,
    cut: bool,
    module: &mut WasmModule,
) {
    let width = width.max(1).min(i32::MAX as i64);
    let idx = module.next_label("wrap_idx");
    let word_start = module.next_label("wrap_word_start");
    let word_end = module.next_label("wrap_word_end");
    let word_len = module.next_label("wrap_word_len");
    let line_len = module.next_label("wrap_line_len");
    let pending = module.next_label("wrap_pending");
    let chunk_start = module.next_label("wrap_chunk_start");
    let chunk_end = module.next_label("wrap_chunk_end");
    let chunk_remaining = module.next_label("wrap_chunk_remaining");
    let chunk_loop = module.next_label("wrap_chunk_loop");
    let chunk_done = module.next_label("wrap_chunk_done");
    let scan_loop = module.next_label("wrap_scan");
    let scan_done = module.next_label("wrap_scan_done");
    let loop_label = module.next_label("wrap_loop");
    let done_label = module.next_label("wrap_done");
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
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("i32.const {}", width));
    module.body().line("i32.le_u");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("call $host_write");
    module.body().line("else");
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
    if cut {
        module.body().line(&format!("local.get {}", word_len));
        module.body().line(&format!("i32.const {}", width));
        module.body().line("i32.gt_u");
        module.body().open("if");
        module.body().line(&format!("local.get {}", line_len));
        module.body().line("i32.eqz");
        module.body().line("i32.eqz");
        module.body().open("if");
        emit_write_wordwrap_break(break_text, literal_break, module);
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
        module.body().line(&format!("i32.const {}", width));
        module.body().line("i32.le_u");
        module.body().line(&format!("br_if {}", chunk_done));
        module.body().line(&format!("local.get {}", chunk_start));
        module.body().line(&format!("i32.const {}", width));
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", chunk_end));
        emit_write_string_range(var, &chunk_start, &chunk_end, module);
        emit_write_wordwrap_break(break_text, literal_break, module);
        module.body().line(&format!("local.get {}", chunk_end));
        module.body().line(&format!("local.set {}", chunk_start));
        module.body().line(&format!("local.get {}", chunk_remaining));
        module.body().line(&format!("i32.const {}", width));
        module.body().line("i32.sub");
        module.body().line(&format!("local.set {}", chunk_remaining));
        module.body().line(&format!("br {}", chunk_loop));
        module.body().close("end");
        module.body().close("end");
        module.body().line(&format!("local.get {}", word_end));
        module.body().line(&format!("local.set {}", chunk_end));
        emit_write_string_range(var, &chunk_start, &chunk_end, module);
        module.body().line(&format!("local.get {}", chunk_remaining));
        module.body().line(&format!("local.set {}", line_len));
        module.body().line(&format!("local.get {}", word_end));
        module.body().line("i32.const 1");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", idx));
        module.body().line(&format!("br {}", loop_label));
        module.body().close("end");
    }
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
    module.body().line(&format!("i32.const {}", width));
    module.body().line("i32.gt_u");
    module.body().line("i32.and");
    module.body().open("if");
    emit_write_wordwrap_break(break_text, literal_break, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", line_len));
    module.body().close("end");
    module.body().line(&format!("local.get {}", line_len));
    module.body().line("i32.eqz");
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("i32.const 32");
    emit_write_stack_byte(module);
    module.body().line(&format!("local.get {}", line_len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", line_len));
    module.body().close("end");
    emit_write_string_range(var, &word_start, &word_end, module);
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
    module.body().close("end");
}

pub(super) fn emit_runtime_wordwrap_dynamic(
    var: &str,
    width_expr: &Expr,
    break_text: WasmBreakText<'_>,
    cut: bool,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let width64 = module.next_label("wrap_width64");
    let width = module.next_label("wrap_width");
    let idx = module.next_label("wrap_idx");
    let word_start = module.next_label("wrap_word_start");
    let word_end = module.next_label("wrap_word_end");
    let word_len = module.next_label("wrap_word_len");
    let line_len = module.next_label("wrap_line_len");
    let pending = module.next_label("wrap_pending");
    let chunk_start = module.next_label("wrap_chunk_start");
    let chunk_end = module.next_label("wrap_chunk_end");
    let chunk_remaining = module.next_label("wrap_chunk_remaining");
    let chunk_loop = module.next_label("wrap_chunk_loop");
    let chunk_done = module.next_label("wrap_chunk_done");
    let scan_loop = module.next_label("wrap_scan");
    let scan_done = module.next_label("wrap_scan_done");
    let loop_label = module.next_label("wrap_loop");
    let done_label = module.next_label("wrap_done");
    let literal_break = match break_text {
        WasmBreakText::Literal(value) => Some(module.intern_string(value)),
        WasmBreakText::Variable(_) => None,
    };
    module.declare_i64_local(width64.trim_start_matches('$').to_string());
    for local in [
        &width,
        &idx,
        &word_start,
        &word_end,
        &word_len,
        &line_len,
        &pending,
        &chunk_start,
        &chunk_end,
        &chunk_remaining,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
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
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get {}", width));
    module.body().line("i32.le_u");
    module.body().open("if");
    emit_write_string_var(var, module);
    module.body().line("else");
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
    if cut {
        module.body().line(&format!("local.get {}", word_len));
        module.body().line(&format!("local.get {}", width));
        module.body().line("i32.gt_u");
        module.body().open("if");
        module.body().line(&format!("local.get {}", line_len));
        module.body().line("i32.eqz");
        module.body().line("i32.eqz");
        module.body().open("if");
        emit_write_wordwrap_break(break_text, literal_break, module);
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
        module.body().line(&format!("local.get {}", width));
        module.body().line("i32.le_u");
        module.body().line(&format!("br_if {}", chunk_done));
        module.body().line(&format!("local.get {}", chunk_start));
        module.body().line(&format!("local.get {}", width));
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", chunk_end));
        emit_write_string_range(var, &chunk_start, &chunk_end, module);
        emit_write_wordwrap_break(break_text, literal_break, module);
        module.body().line(&format!("local.get {}", chunk_end));
        module.body().line(&format!("local.set {}", chunk_start));
        module.body().line(&format!("local.get {}", chunk_remaining));
        module.body().line(&format!("local.get {}", width));
        module.body().line("i32.sub");
        module.body().line(&format!("local.set {}", chunk_remaining));
        module.body().line(&format!("br {}", chunk_loop));
        module.body().close("end");
        module.body().close("end");
        module.body().line(&format!("local.get {}", word_end));
        module.body().line(&format!("local.set {}", chunk_end));
        emit_write_string_range(var, &chunk_start, &chunk_end, module);
        module.body().line(&format!("local.get {}", chunk_remaining));
        module.body().line(&format!("local.set {}", line_len));
        module.body().line(&format!("local.get {}", word_end));
        module.body().line("i32.const 1");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", idx));
        module.body().line(&format!("br {}", loop_label));
        module.body().close("end");
    }
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
    module.body().line(&format!("local.get {}", width));
    module.body().line("i32.gt_u");
    module.body().line("i32.and");
    module.body().open("if");
    emit_write_wordwrap_break(break_text, literal_break, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", line_len));
    module.body().close("end");
    module.body().line(&format!("local.get {}", line_len));
    module.body().line("i32.eqz");
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("i32.const 32");
    emit_write_stack_byte(module);
    module.body().line(&format!("local.get {}", line_len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", line_len));
    module.body().close("end");
    emit_write_string_range(var, &word_start, &word_end, module);
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
    module.body().close("end");
    Ok(())
}

fn emit_write_wordwrap_break(
    break_text: WasmBreakText<'_>,
    literal_break: Option<(usize, usize)>,
    module: &mut WasmModule,
) {
    match break_text {
        WasmBreakText::Literal(_) => {
            let (break_ptr, break_len) = literal_break.unwrap();
            emit_write_static_string(break_ptr, break_len, module);
        }
        WasmBreakText::Variable(break_var) => emit_write_string_var(break_var, module),
    }
}
