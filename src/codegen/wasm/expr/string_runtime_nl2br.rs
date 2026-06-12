//! Purpose:
//! Emits wasm32-web nl2br runtime helpers for string builtins.
//! Keeps newline-to-HTML break output and value materialization out of the broad string runtime file.
//!
//! Called from:
//! - `super::string_runtime` re-exports used by string builtin lowering paths.
//!
//! Key details:
//! - Helpers preserve CRLF handling and the existing output/value-to-stack contracts.

use super::*;
use super::string_bytes::*;

pub(super) fn emit_runtime_nl2br(var: &str, module: &mut WasmModule) {
    let idx = module.next_label("nl_idx");
    let byte = module.next_label("nl_byte");
    let loop_label = module.next_label("nl_loop");
    let done_label = module.next_label("nl_done");
    let (br_ptr, br_len) = module.intern_string("<br />");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_string_byte(var, &idx, module);
    module.body().line(&format!("local.set {}", byte));
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 13");
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.lt_u");
    module.body().line("i32.and");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.const 10");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_write_static_string(br_ptr, br_len, module);
    module.body().line("i32.const 13");
    emit_write_stack_byte(module);
    module.body().line("i32.const 10");
    emit_write_stack_byte(module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 2");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 10");
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 13");
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().open("if");
    emit_write_static_string(br_ptr, br_len, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", byte));
    emit_write_stack_byte(module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_nl2br_value_to_stack(var: &str, module: &mut WasmModule) {
    let out_ptr = module.next_label("nl_value_ptr");
    let out_idx = module.next_label("nl_value_idx");
    let idx = module.next_label("nl_value_source_idx");
    let byte = module.next_label("nl_value_byte");
    let loop_label = module.next_label("nl_value_loop");
    let done_label = module.next_label("nl_value_done");
    let (br_ptr, br_len) = module.intern_string("<br />");
    for local in [&out_ptr, &out_idx, &idx, &byte] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.const 7");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_string_byte(var, &idx, module);
    module.body().line(&format!("local.set {}", byte));
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 13");
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.lt_u");
    module.body().line("i32.and");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.const 10");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_copy_static_range_to_memory(br_ptr, br_len, &out_ptr, &out_idx, module);
    emit_store_byte_const(13, &out_ptr, &out_idx, module);
    emit_store_byte_const(10, &out_ptr, &out_idx, module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 2");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 10");
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 13");
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().open("if");
    emit_copy_static_range_to_memory(br_ptr, br_len, &out_ptr, &out_idx, module);
    module.body().close("end");
    emit_store_byte_local(&byte, &out_ptr, &out_idx, module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
}
