//! Purpose:
//! Provides wasm32-web helpers for copying string bytes into heap-allocated output buffers.
//! Shared by string builtins that materialize runtime string values.
//!
//! Called from:
//! - `crate::codegen::wasm::expr`
//! - sibling wasm expression modules through `super::*`
//!
//! Key details:
//! - Maintains caller-owned output pointer/index locals while copying bytes.
//! - Supports literal, variable, space, and zero padding sources for PHP string formatting.

use super::*;

pub(super) enum WasmPadSource<'a> {
    Literal(&'a str),
    Variable(&'a str),
}


pub(super) fn emit_copy_string_to_memory(
    var: &str,
    out_ptr: &str,
    out_idx: &str,
    module: &mut WasmModule,
) {
    let source_idx = module.next_label("copy_string_idx");
    let loop_label = module.next_label("copy_string_loop");
    let done_label = module.next_label("copy_string_done");
    module.declare_i32_local(source_idx.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", source_idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", source_idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", source_idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.store8");
    module.body().line(&format!("local.get {}", source_idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_idx));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_copy_string_range_to_memory(
    var: &str,
    start: &str,
    end: &str,
    out_ptr: &str,
    out_idx: &str,
    module: &mut WasmModule,
) {
    let source_idx = module.next_label("copy_range_idx");
    let loop_label = module.next_label("copy_range_loop");
    let done_label = module.next_label("copy_range_done");
    module.declare_i32_local(source_idx.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.set {}", source_idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", source_idx));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", source_idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.store8");
    module.body().line(&format!("local.get {}", source_idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_idx));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_copy_static_range_to_memory(
    ptr: usize,
    len: usize,
    out_ptr: &str,
    out_idx: &str,
    module: &mut WasmModule,
) {
    let source_idx = module.next_label("copy_static_idx");
    let loop_label = module.next_label("copy_static_loop");
    let done_label = module.next_label("copy_static_done");
    module.declare_i32_local(source_idx.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", source_idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", source_idx));
    module.body().line(&format!("i32.const {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.add");
    module.body().line(&format!("i32.const {}", ptr));
    module.body().line(&format!("local.get {}", source_idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.store8");
    module.body().line(&format!("local.get {}", source_idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_idx));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_copy_pad_to_memory(
    pad: &WasmPadSource<'_>,
    count: &str,
    out_ptr: &str,
    out_idx: &str,
    module: &mut WasmModule,
) {
    let pad_idx = module.next_label("copy_pad_idx");
    let written = module.next_label("copy_pad_written");
    let loop_label = module.next_label("copy_pad_loop");
    let done_label = module.next_label("copy_pad_done");
    let literal = match pad {
        WasmPadSource::Literal(value) => Some(module.intern_string(value)),
        WasmPadSource::Variable(_) => None,
    };
    module.declare_i32_local(pad_idx.trim_start_matches('$').to_string());
    module.declare_i32_local(written.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pad_idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", written));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", written));
    module.body().line(&format!("local.get {}", count));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.add");
    match pad {
        WasmPadSource::Literal(_) => {
            let (ptr, _) = literal.unwrap();
            module.body().line(&format!("i32.const {}", ptr));
        }
        WasmPadSource::Variable(pad_var) => {
            module.body().line(&format!("local.get ${}_ptr", pad_var));
        }
    }
    module.body().line(&format!("local.get {}", pad_idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.store8");
    module.body().line(&format!("local.get {}", written));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", written));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_idx));
    module.body().line(&format!("local.get {}", pad_idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    match pad {
        WasmPadSource::Literal(_) => {
            let (_, len) = literal.unwrap();
            module.body().line(&format!("i32.const {}", len));
        }
        WasmPadSource::Variable(pad_var) => {
            module.body().line(&format!("local.get ${}_len", pad_var));
        }
    }
    module.body().line("i32.rem_u");
    module.body().line(&format!("local.set {}", pad_idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}


pub(super) fn emit_copy_spaces_to_memory(count: &str, out_ptr: &str, out_idx: &str, module: &mut WasmModule) {
    let pad = WasmPadSource::Literal(" ");
    emit_copy_pad_to_memory(&pad, count, out_ptr, out_idx, module);
}

pub(super) fn emit_copy_zeros_to_memory(count: &str, out_ptr: &str, out_idx: &str, module: &mut WasmModule) {
    let pad = WasmPadSource::Literal("0");
    emit_copy_pad_to_memory(&pad, count, out_ptr, out_idx, module);
}



