//! Purpose:
//! Lowers PHP path-oriented string builtins for the wasm32-web backend.
//! Owns basename(), dirname(), and pathinfo() runtime and literal helpers.
//!
//! Called from:
//! - `crate::codegen::wasm::expr`
//! - `crate::codegen::wasm::expr::string_builtins`
//!
//! Key details:
//! - Keeps path component scanning and pathinfo associative-array materialization out of the main expression dispatcher.

use super::*;

pub(super) fn emit_runtime_basename(var: &str, suffix: Option<&str>, module: &mut WasmModule) {
    let start = module.next_label("basename_start");
    let end = module.next_label("basename_end");
    let idx = module.next_label("basename_idx");
    let trim_loop = module.next_label("basename_trim_loop");
    let trim_done = module.next_label("basename_trim_done");
    let scan_loop = module.next_label("basename_scan_loop");
    let scan_done = module.next_label("basename_scan_done");
    module.declare_i32_local(start.trim_start_matches('$').to_string());
    module.declare_i32_local(end.trim_start_matches('$').to_string());
    module.declare_i32_local(idx.trim_start_matches('$').to_string());

    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.set {}", end));
    module.body().open(&format!("block {}", trim_done));
    module.body().open(&format!("loop {}", trim_loop));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", trim_done));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.const 47");
    module.body().line("i32.ne");
    module.body().line(&format!("br_if {}", trim_done));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", end));
    module.body().line(&format!("br {}", trim_loop));
    module.body().close("end");
    module.body().close("end");

    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", start));
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", scan_done));
    module.body().open(&format!("loop {}", scan_loop));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", scan_done));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.const 47");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", start));
    module.body().line(&format!("br {}", scan_done));
    module.body().close("end");
    module.body().line(&format!("br {}", scan_loop));
    module.body().close("end");
    module.body().close("end");

    if let Some(suffix) = suffix {
        if !suffix.is_empty() {
            let suffix_start = module.next_label("basename_suffix_start");
            module.declare_i32_local(suffix_start.trim_start_matches('$').to_string());
            module.body().line(&format!("local.get {}", end));
            module.body().line(&format!("local.get {}", start));
            module.body().line("i32.sub");
            module.body().line(&format!("i32.const {}", suffix.len()));
            module.body().line("i32.ge_u");
            module.body().open("if");
            module.body().line(&format!("local.get {}", end));
            module.body().line(&format!("i32.const {}", suffix.len()));
            module.body().line("i32.sub");
            module.body().line(&format!("local.set {}", suffix_start));
            emit_runtime_literal_match_at(var, &suffix_start, suffix, module);
            module.body().open("if");
            module.body().line(&format!("local.get {}", suffix_start));
            module.body().line(&format!("local.set {}", end));
            module.body().close("end");
            module.body().close("end");
        }
    }

    emit_write_string_range(var, &start, &end, module);
}

pub(super) fn emit_runtime_basename_var_suffix(var: &str, suffix_var: &str, module: &mut WasmModule) {
    let start = module.next_label("basename_start");
    let end = module.next_label("basename_end");
    let idx = module.next_label("basename_idx");
    let suffix_start = module.next_label("basename_suffix_start");
    let trim_loop = module.next_label("basename_trim_loop");
    let trim_done = module.next_label("basename_trim_done");
    let scan_loop = module.next_label("basename_scan_loop");
    let scan_done = module.next_label("basename_scan_done");
    module.declare_i32_local(start.trim_start_matches('$').to_string());
    module.declare_i32_local(end.trim_start_matches('$').to_string());
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(suffix_start.trim_start_matches('$').to_string());

    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.set {}", end));
    module.body().open(&format!("block {}", trim_done));
    module.body().open(&format!("loop {}", trim_loop));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", trim_done));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.const 47");
    module.body().line("i32.ne");
    module.body().line(&format!("br_if {}", trim_done));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", end));
    module.body().line(&format!("br {}", trim_loop));
    module.body().close("end");
    module.body().close("end");

    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", start));
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", scan_done));
    module.body().open(&format!("loop {}", scan_loop));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", scan_done));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.const 47");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", start));
    module.body().line(&format!("br {}", scan_done));
    module.body().close("end");
    module.body().line(&format!("br {}", scan_loop));
    module.body().close("end");
    module.body().close("end");

    module.body().line(&format!("local.get ${}_len", suffix_var));
    module.body().line("i32.eqz");
    module.body().line("i32.eqz");
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.sub");
    module.body().line(&format!("local.get ${}_len", suffix_var));
    module.body().line("i32.ge_u");
    module.body().line("i32.and");
    module.body().open("if");
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.get ${}_len", suffix_var));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", suffix_start));
    emit_runtime_var_match_at(var, &suffix_start, suffix_var, module);
    module.body().open("if");
    module.body().line(&format!("local.get {}", suffix_start));
    module.body().line(&format!("local.set {}", end));
    module.body().close("end");
    module.body().close("end");

    emit_write_string_range(var, &start, &end, module);
}

pub(super) fn emit_runtime_dirname(var: &str, levels: i64, module: &mut WasmModule) {
    let end = module.next_label("dirname_end");
    let idx = module.next_label("dirname_idx");
    let state = module.next_label("dirname_state");
    let zero = module.next_label("dirname_zero");
    module.declare_i32_local(end.trim_start_matches('$').to_string());
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(state.trim_start_matches('$').to_string());
    module.declare_i32_local(zero.trim_start_matches('$').to_string());
    let (dot_ptr, dot_len) = module.intern_string(".");
    let (root_ptr, root_len) = module.intern_string("/");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", zero));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.set {}", end));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", state));

    for _ in 0..levels {
        emit_runtime_dirname_step(var, &end, &idx, &state, module);
    }

    module.body().line(&format!("local.get {}", state));
    module.body().line("i32.const 1");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_write_static_string(dot_ptr, dot_len, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", state));
    module.body().line("i32.const 2");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_write_static_string(root_ptr, root_len, module);
    module.body().line("else");
    emit_write_string_range(var, &zero, &end, module);
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_dirname_dynamic(
    var: &str,
    levels_expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let end = module.next_label("dirname_end");
    let idx = module.next_label("dirname_idx");
    let state = module.next_label("dirname_state");
    let zero = module.next_label("dirname_zero");
    let levels64 = module.next_label("dirname_levels64");
    let remaining = module.next_label("dirname_remaining");
    let loop_label = module.next_label("dirname_levels_loop");
    let done_label = module.next_label("dirname_levels_done");
    module.declare_i32_local(end.trim_start_matches('$').to_string());
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(state.trim_start_matches('$').to_string());
    module.declare_i32_local(zero.trim_start_matches('$').to_string());
    module.declare_i64_local(levels64.trim_start_matches('$').to_string());
    module.declare_i32_local(remaining.trim_start_matches('$').to_string());
    let (dot_ptr, dot_len) = module.intern_string(".");
    let (root_ptr, root_len) = module.intern_string("/");
    require_int(levels_expr, module)?;
    module.body().line(&format!("local.set {}", levels64));
    module.body().line(&format!("local.get {}", levels64));
    module.body().line("i64.const 1");
    module.body().line("i64.lt_s");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", levels64));
    module.body().line("i64.const 2147483647");
    module.body().line("i64.gt_s");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", levels64));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", remaining));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", zero));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.set {}", end));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", state));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", remaining));
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", done_label));
    emit_runtime_dirname_step(var, &end, &idx, &state, module);
    module.body().line(&format!("local.get {}", remaining));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", remaining));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", state));
    module.body().line("i32.const 1");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_write_static_string(dot_ptr, dot_len, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", state));
    module.body().line("i32.const 2");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_write_static_string(root_ptr, root_len, module);
    module.body().line("else");
    emit_write_string_range(var, &zero, &end, module);
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_runtime_dirname_step(
    var: &str,
    end: &str,
    idx: &str,
    state: &str,
    module: &mut WasmModule,
) {
    let done = module.next_label("dirname_step_done");
    let trim_loop = module.next_label("dirname_trim_loop");
    let trim_done = module.next_label("dirname_trim_done");
    let scan_loop = module.next_label("dirname_scan_loop");
    let scan_done = module.next_label("dirname_scan_done");

    module.body().line(&format!("local.get {}", state));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().open(&format!("block {}", done));
    module.body().open(&format!("block {}", trim_done));
    module.body().open(&format!("loop {}", trim_loop));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", trim_done));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.const 47");
    module.body().line("i32.ne");
    module.body().line(&format!("br_if {}", trim_done));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", end));
    module.body().line(&format!("br {}", trim_loop));
    module.body().close("end");
    module.body().close("end");

    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", state));
    module.body().line("else");
    module.body().line("i32.const 2");
    module.body().line(&format!("local.set {}", state));
    module.body().close("end");
    module.body().line(&format!("br {}", done));
    module.body().close("end");

    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", scan_done));
    module.body().open(&format!("loop {}", scan_loop));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", scan_done));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.const 47");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("i32.const 2");
    module.body().line(&format!("local.set {}", state));
    module.body().line("else");
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.set {}", end));
    module.body().close("end");
    module.body().line(&format!("br {}", done));
    module.body().close("end");
    module.body().line(&format!("br {}", scan_loop));
    module.body().close("end");
    module.body().close("end");

    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", state));
    module.body().close("end");
    module.body().close("end");
}

fn emit_runtime_pathinfo_extension(var: &str, module: &mut WasmModule) {
    let start = module.next_label("pathinfo_start");
    let end = module.next_label("pathinfo_end");
    let dot = module.next_label("pathinfo_dot");
    let found_dot = module.next_label("pathinfo_found_dot");
    emit_runtime_pathinfo_basename_dot(var, &start, &end, &dot, &found_dot, module);
    module.body().line(&format!("local.get {}", found_dot));
    module.body().open("if");
    module.body().line(&format!("local.get {}", dot));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", dot));
    emit_write_string_range(var, &dot, &end, module);
    module.body().close("end");
}

fn emit_runtime_pathinfo_filename(var: &str, module: &mut WasmModule) {
    let start = module.next_label("pathinfo_start");
    let end = module.next_label("pathinfo_end");
    let dot = module.next_label("pathinfo_dot");
    let found_dot = module.next_label("pathinfo_found_dot");
    emit_runtime_pathinfo_basename_dot(var, &start, &end, &dot, &found_dot, module);
    module.body().line(&format!("local.get {}", found_dot));
    module.body().open("if");
    emit_write_string_range(var, &start, &dot, module);
    module.body().line("else");
    emit_write_string_range(var, &start, &end, module);
    module.body().close("end");
}

use super::pathinfo_flags::{pathinfo_scalar_component, PathinfoScalarComponent};

pub(super) fn emit_runtime_pathinfo_static_flag(
    call: &Expr,
    var: &str,
    flag: i64,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match pathinfo_scalar_component(flag) {
        Some(PathinfoScalarComponent::Dirname) => emit_runtime_dirname(var, 1, module),
        Some(PathinfoScalarComponent::Basename) => emit_runtime_basename(var, None, module),
        Some(PathinfoScalarComponent::Extension) => emit_runtime_pathinfo_extension(var, module),
        Some(PathinfoScalarComponent::Filename) => emit_runtime_pathinfo_filename(var, module),
        Some(PathinfoScalarComponent::Empty) => {}
        None => {
            return Err(CompileError::new(
                call.span,
                "wasm32-web runtime pathinfo() requires a scalar PATHINFO_* flag",
            ));
        }
    }
    Ok(())
}

pub(super) fn emit_runtime_pathinfo_dynamic_flag(
    var: &str,
    flag_expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let flag = module.next_label("pathinfo_flag");
    module.declare_i64_local(flag.trim_start_matches('$').to_string());
    require_int(flag_expr, module)?;
    module.body().line(&format!("local.set {}", flag));
    module.body().line(&format!("local.get {}", flag));
    module.body().line("i64.const 15");
    module.body().line("i64.eq");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", flag));
    module.body().line("i64.const 1");
    module.body().line("i64.and");
    module.body().line("i64.eqz");
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_runtime_dirname(var, 1, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", flag));
    module.body().line("i64.const 2");
    module.body().line("i64.and");
    module.body().line("i64.eqz");
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_runtime_basename(var, None, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", flag));
    module.body().line("i64.const 4");
    module.body().line("i64.and");
    module.body().line("i64.eqz");
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_runtime_pathinfo_extension(var, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", flag));
    module.body().line("i64.const 8");
    module.body().line("i64.and");
    module.body().line("i64.eqz");
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_runtime_pathinfo_filename(var, module);
    module.body().line("else");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_runtime_pathinfo_basename_dot(
    var: &str,
    start: &str,
    end: &str,
    dot: &str,
    found_dot: &str,
    module: &mut WasmModule,
) {
    let idx = module.next_label("pathinfo_idx");
    let trim_loop = module.next_label("pathinfo_trim_loop");
    let trim_done = module.next_label("pathinfo_trim_done");
    let slash_loop = module.next_label("pathinfo_slash_loop");
    let slash_done = module.next_label("pathinfo_slash_done");
    let dot_loop = module.next_label("pathinfo_dot_loop");
    let dot_done = module.next_label("pathinfo_dot_done");
    module.declare_i32_local(start.trim_start_matches('$').to_string());
    module.declare_i32_local(end.trim_start_matches('$').to_string());
    module.declare_i32_local(dot.trim_start_matches('$').to_string());
    module.declare_i32_local(found_dot.trim_start_matches('$').to_string());
    module.declare_i32_local(idx.trim_start_matches('$').to_string());

    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.set {}", end));
    module.body().open(&format!("block {}", trim_done));
    module.body().open(&format!("loop {}", trim_loop));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", trim_done));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.const 47");
    module.body().line("i32.ne");
    module.body().line(&format!("br_if {}", trim_done));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", end));
    module.body().line(&format!("br {}", trim_loop));
    module.body().close("end");
    module.body().close("end");

    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", start));
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", slash_done));
    module.body().open(&format!("loop {}", slash_loop));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", slash_done));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.const 47");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", start));
    module.body().line(&format!("br {}", slash_done));
    module.body().close("end");
    module.body().line(&format!("br {}", slash_loop));
    module.body().close("end");
    module.body().close("end");

    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found_dot));
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", dot_done));
    module.body().open(&format!("loop {}", dot_loop));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.le_u");
    module.body().line(&format!("br_if {}", dot_done));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.const 46");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.gt_u");
    module.body().open("if");
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.set {}", dot));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found_dot));
    module.body().line(&format!("br {}", dot_done));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("br {}", dot_loop));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_pathinfo_array_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if args.is_empty() || args.len() > 2 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web pathinfo() expects one or two arguments",
        ));
    }
    if let Some(flag_arg) = args.get(1) {
        let Some(flag) = static_or_const_int_value(flag_arg) else {
            return Err(CompileError::new(
                flag_arg.span,
                "wasm32-web runtime array-shaped pathinfo() currently requires PATHINFO_ALL or no flag",
            ));
        };
        if flag != 15 {
            return Err(CompileError::new(
                flag_arg.span,
                "wasm32-web runtime array-shaped pathinfo() currently requires PATHINFO_ALL or no flag",
            ));
        }
    }
    let source = string_arg_or_materialize(&args[0], "pathinfo_array_source", module)?.ok_or_else(|| {
        CompileError::new(
            args[0].span,
            "wasm32-web runtime array-shaped pathinfo() requires a string path",
        )
    })?;
    let start = module.next_label("pathinfo_array_start");
    let end = module.next_label("pathinfo_array_end");
    let dot = module.next_label("pathinfo_array_dot");
    let found_dot = module.next_label("pathinfo_array_found_dot");
    let zero = module.next_label("pathinfo_array_zero");
    let dirname_end = module.next_label("pathinfo_array_dirname_end");
    module.declare_i32_local(zero.trim_start_matches('$').to_string());
    module.declare_i32_local(dirname_end.trim_start_matches('$').to_string());

    emit_release_current_assoc_array(name, module);
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Str; 4]));
    module.set_array_nested_value_metadata(name, None);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", zero));
    module.body().line("i32.const 4");
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));

    emit_runtime_pathinfo_basename_dot(&source, &start, &end, &dot, &found_dot, module);
    module.body().line(&format!("local.get {}", found_dot));
    module.body().open("if (result i32)");
    module.body().line("i32.const 4");
    module.body().line("else");
    module.body().line("i32.const 3");
    module.body().close("end");
    module.body().line(&format!("local.set ${}_len", name));

    emit_pathinfo_runtime_array_key(name, 0, "dirname", call, module)?;
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_assoc_array_store_static_string_value(name, 0, ".", module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.const 1");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_assoc_array_store_static_string_value(name, 0, "/", module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", dirname_end));
    emit_assoc_array_store_string_range_value(name, 0, &source, &zero, &dirname_end, module);
    module.body().close("end");
    module.body().close("end");

    emit_pathinfo_runtime_array_key(name, 1, "basename", call, module)?;
    emit_assoc_array_store_string_range_value(name, 1, &source, &start, &end, module);

    module.body().line(&format!("local.get {}", found_dot));
    module.body().open("if");
    emit_pathinfo_runtime_array_key(name, 2, "extension", call, module)?;
    module.body().line(&format!("local.get {}", dot));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", dirname_end));
    emit_assoc_array_store_string_range_value(name, 2, &source, &dirname_end, &end, module);
    emit_pathinfo_runtime_array_key(name, 3, "filename", call, module)?;
    emit_assoc_array_store_string_range_value(name, 3, &source, &start, &dot, module);
    module.body().line("else");
    emit_pathinfo_runtime_array_key(name, 2, "filename", call, module)?;
    emit_assoc_array_store_string_range_value(name, 2, &source, &start, &end, module);
    module.body().close("end");
    Ok(())
}

fn emit_pathinfo_runtime_array_key(
    name: &str,
    index: usize,
    key: &str,
    call: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let key = Expr::new(ExprKind::StringLiteral(key.to_string()), call.span);
    emit_assoc_array_store_key(name, index, &key, module)
}
