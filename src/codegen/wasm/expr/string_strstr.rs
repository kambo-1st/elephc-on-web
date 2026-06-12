//! Purpose:
//! Emits wasm32-web `strstr()` string-result lowering helpers.
//! Keeps substring-returning search separate from integer `strpos`/`strrpos` lowering.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` string builtin and string-value dispatch.
//!
//! Key details:
//! - Preserves PHP false-vs-string result handling and `before_needle` range behavior.

use super::*;
use super::string_search_find::{emit_runtime_find_literal_forward, emit_runtime_find_var_forward};

pub(super) fn emit_runtime_strstr(
    var: &str,
    needle: &str,
    before_needle: bool,
    module: &mut WasmModule,
) {
    let start = module.next_label("strstr_start");
    let found = module.next_label("strstr_found");
    module.declare_i32_local(start.trim_start_matches('$').to_string());
    module.declare_i32_local(found.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", start));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    if needle.is_empty() {
        module.body().line("i32.const 1");
        module.body().line(&format!("local.set {}", found));
    } else {
        emit_runtime_find_literal_forward(var, needle, 0, &start, &found, module);
    }
    module.body().line(&format!("local.get {}", found));
    module.body().open("if");
    if before_needle {
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", found));
        emit_write_string_range(var, &found, &start, module);
    } else {
        module.body().line(&format!("local.get ${}_ptr", var));
        module.body().line(&format!("local.get {}", start));
        module.body().line("i32.add");
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line(&format!("local.get {}", start));
        module.body().line("i32.sub");
        module.body().line("call $host_write");
    }
    module.body().close("end");
}

pub(super) fn emit_runtime_strstr_var(
    var: &str,
    needle_var: &str,
    before_needle: bool,
    module: &mut WasmModule,
) {
    let start = module.next_label("strstr_start");
    let found = module.next_label("strstr_found");
    module.declare_i32_local(start.trim_start_matches('$').to_string());
    module.declare_i32_local(found.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", start));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("local.get ${}_len", needle_var));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line("else");
    emit_runtime_find_var_forward(var, needle_var, 0, &start, &found, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
    module.body().open("if");
    if before_needle {
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", found));
        emit_write_string_range(var, &found, &start, module);
    } else {
        module.body().line(&format!("local.get ${}_ptr", var));
        module.body().line(&format!("local.get {}", start));
        module.body().line("i32.add");
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line(&format!("local.get {}", start));
        module.body().line("i32.sub");
        module.body().line("call $host_write");
    }
    module.body().close("end");
}

pub(super) fn emit_runtime_strstr_value_to_stack(
    var: &str,
    needle: &str,
    before_needle: bool,
    module: &mut WasmModule,
) {
    let start = module.next_label("strstr_value_start");
    let found = module.next_label("strstr_value_found");
    module.declare_i32_local(start.trim_start_matches('$').to_string());
    module.declare_i32_local(found.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", start));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    if needle.is_empty() {
        module.body().line("i32.const 1");
        module.body().line(&format!("local.set {}", found));
    } else {
        emit_runtime_find_literal_forward(var, needle, 0, &start, &found, module);
    }
    emit_runtime_strstr_found_value(var, &start, &found, before_needle, module);
}

pub(super) fn emit_runtime_strstr_var_value_to_stack(
    var: &str,
    needle_var: &str,
    before_needle: bool,
    module: &mut WasmModule,
) {
    let start = module.next_label("strstr_value_start");
    let found = module.next_label("strstr_value_found");
    module.declare_i32_local(start.trim_start_matches('$').to_string());
    module.declare_i32_local(found.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", start));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("local.get ${}_len", needle_var));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line("else");
    emit_runtime_find_var_forward(var, needle_var, 0, &start, &found, module);
    module.body().close("end");
    emit_runtime_strstr_found_value(var, &start, &found, before_needle, module);
}

fn emit_runtime_strstr_found_value(
    var: &str,
    start: &str,
    found: &str,
    before_needle: bool,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", found));
    module.body().open("if (result i32 i32)");
    if before_needle {
        module.body().line(&format!("local.get ${}_ptr", var));
        module.body().line(&format!("local.get {}", start));
    } else {
        module.body().line(&format!("local.get ${}_ptr", var));
        module.body().line(&format!("local.get {}", start));
        module.body().line("i32.add");
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line(&format!("local.get {}", start));
        module.body().line("i32.sub");
    }
    module.body().line("else");
    module.body().line("i32.const 0");
    module.body().line("i32.const 0");
    module.body().close("end");
}
