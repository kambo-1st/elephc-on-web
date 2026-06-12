//! Purpose:
//! Lowers wasm32-web str_repeat() output and heap-materialized value paths.
//! Keeps repetition loops separate from str_replace() search/replacement lowering.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::string_builtins`.
//! - `crate::codegen::wasm::expr::string_values`.
//!
//! Key details:
//! - Count expressions are checked for non-negative i32 bounds before runtime loops emit bytes.

use super::*;

pub(super) fn emit_runtime_str_repeat(var: &str, times: i64, module: &mut WasmModule) {
    let idx = module.next_label("repeat_idx");
    let loop_label = module.next_label("repeat_loop");
    let done_label = module.next_label("repeat_done");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", times.min(i32::MAX as i64)));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("call $host_write");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_str_repeat_dynamic(
    var: &str,
    times: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let idx = module.next_label("repeat_idx");
    let count64 = module.next_label("repeat_count64");
    let count = module.next_label("repeat_count");
    let loop_label = module.next_label("repeat_loop");
    let done_label = module.next_label("repeat_done");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i64_local(count64.trim_start_matches('$').to_string());
    module.declare_i32_local(count.trim_start_matches('$').to_string());
    require_int(times, module)?;
    module.body().line(&format!("local.set {}", count64));
    module.body().line(&format!("local.get {}", count64));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", count64));
    module.body().line("i64.const 2147483647");
    module.body().line("i64.gt_s");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", count64));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", count));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get {}", count));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("call $host_write");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_non_negative_i32_count(
    times: &Expr,
    target: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let count64 = module.next_label("count64");
    module.declare_i64_local(count64.trim_start_matches('$').to_string());
    require_int(times, module)?;
    module.body().line(&format!("local.set {}", count64));
    module.body().line(&format!("local.get {}", count64));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", count64));
    module.body().line("i64.const 2147483647");
    module.body().line("i64.gt_s");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", count64));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", target));
    Ok(())
}

pub(super) fn emit_runtime_str_repeat_value_to_stack(var: &str, count: &str, module: &mut WasmModule) {
    let out_ptr = module.next_label("repeat_value_ptr");
    let out_len = module.next_label("repeat_value_len");
    let out_idx = module.next_label("repeat_value_idx");
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(out_len.trim_start_matches('$').to_string());
    module.declare_i32_local(out_idx.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get {}", count));
    module.body().line("i32.mul");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.get {}", count));
    module.body().line("i32.eqz");
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.get {}", count));
    module.body().line("i32.div_u");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    emit_runtime_str_repeat_copy(var, &out_ptr, &out_len, &out_idx, module);
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_len));
}

fn emit_runtime_str_repeat_copy(
    var: &str,
    out_ptr: &str,
    out_len: &str,
    out_idx: &str,
    module: &mut WasmModule,
) {
    let loop_label = module.next_label("repeat_value_loop");
    let done_label = module.next_label("repeat_value_done");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.rem_u");
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.store8");
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

