//! Purpose:
//! Emits wasm32-web addslashes/stripslashes runtime helpers for string builtins.
//! Keeps slash escaping output and value materialization out of the broad string runtime file.
//!
//! Called from:
//! - `super::string_runtime` re-exports used by string builtin lowering paths.
//!
//! Key details:
//! - Helpers preserve existing output and heap materialization contracts for escaped string values.

use super::*;
use super::string_bytes::*;

pub(super) fn emit_runtime_addslashes(var: &str, module: &mut WasmModule) {
    let idx = module.next_label("slashes_idx");
    let byte = module.next_label("slashes_byte");
    let loop_label = module.next_label("slashes_loop");
    let done_label = module.next_label("slashes_done");
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
    emit_addslashes_escape_condition(&byte, module);
    module.body().open("if");
    module.body().line("i32.const 92");
    emit_write_stack_byte(module);
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

pub(super) fn emit_runtime_stripslashes(var: &str, module: &mut WasmModule) {
    let idx = module.next_label("strip_idx");
    let byte = module.next_label("strip_byte");
    let escaping = module.next_label("strip_escaping");
    let loop_label = module.next_label("strip_loop");
    let done_label = module.next_label("strip_done");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    module.declare_i32_local(escaping.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", escaping));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_string_byte(var, &idx, module);
    module.body().line(&format!("local.set {}", byte));
    module.body().line(&format!("local.get {}", escaping));
    module.body().open("if");
    module.body().line(&format!("local.get {}", byte));
    emit_write_stack_byte(module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", escaping));
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 92");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", escaping));
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    emit_write_stack_byte(module);
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", escaping));
    module.body().open("if");
    module.body().line("i32.const 92");
    emit_write_stack_byte(module);
    module.body().close("end");
}

pub(super) fn emit_runtime_addslashes_value_to_stack(var: &str, module: &mut WasmModule) {
    let out_ptr = module.next_label("slashes_value_ptr");
    let out_idx = module.next_label("slashes_value_idx");
    let idx = module.next_label("slashes_value_source_idx");
    let byte = module.next_label("slashes_value_byte");
    let loop_label = module.next_label("slashes_value_loop");
    let done_label = module.next_label("slashes_value_done");
    for local in [&out_ptr, &out_idx, &idx, &byte] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.const 2");
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
    emit_addslashes_escape_condition(&byte, module);
    module.body().open("if");
    emit_store_byte_const(92, &out_ptr, &out_idx, module);
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

pub(super) fn emit_runtime_stripslashes_value_to_stack(var: &str, module: &mut WasmModule) {
    let out_ptr = module.next_label("strip_value_ptr");
    let out_idx = module.next_label("strip_value_idx");
    let idx = module.next_label("strip_value_source_idx");
    let byte = module.next_label("strip_value_byte");
    let escaping = module.next_label("strip_value_escaping");
    let loop_label = module.next_label("strip_value_loop");
    let done_label = module.next_label("strip_value_done");
    for local in [&out_ptr, &out_idx, &idx, &byte, &escaping] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", escaping));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_string_byte(var, &idx, module);
    module.body().line(&format!("local.set {}", byte));
    module.body().line(&format!("local.get {}", escaping));
    module.body().open("if");
    emit_store_byte_local(&byte, &out_ptr, &out_idx, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", escaping));
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 92");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", escaping));
    module.body().line("else");
    emit_store_byte_local(&byte, &out_ptr, &out_idx, module);
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", escaping));
    module.body().open("if");
    emit_store_byte_const(92, &out_ptr, &out_idx, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
}
