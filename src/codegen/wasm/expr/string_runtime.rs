//! Purpose:
//! Emits wasm32-web runtime string transform helpers used by string builtins.
//! Keeps byte-wise string runtime lowering separate from expression dispatch.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` string builtin and string value lowering paths.
//!
//! Key details:
//! - Helpers emit WAT directly and preserve the existing stack/output contracts.

use super::*;

#[derive(Clone, Copy)]
pub(super) enum AsciiCase {
    Lower,
    Upper,
}

#[derive(Clone, Copy)]
pub(super) enum RuntimeStringValueTransform {
    Case(AsciiCase),
    FirstCase(AsciiCase),
    Ucwords,
    Reverse,
}

pub(super) fn emit_runtime_same_len_string_value(
    var: &str,
    transform: RuntimeStringValueTransform,
    module: &mut WasmModule,
) {
    let out_ptr = module.next_label("string_value_ptr");
    let idx = module.next_label("string_value_idx");
    let byte = module.next_label("string_value_byte");
    let word_start = module.next_label("string_value_word_start");
    let loop_label = module.next_label("string_value_loop");
    let done_label = module.next_label("string_value_done");
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    module.declare_i32_local(word_start.trim_start_matches('$').to_string());

    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", word_start));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));

    match transform {
        RuntimeStringValueTransform::Reverse => {
            module.body().line(&format!("local.get ${}_ptr", var));
            module.body().line(&format!("local.get ${}_len", var));
            module.body().line(&format!("local.get {}", idx));
            module.body().line("i32.sub");
            module.body().line("i32.const 1");
            module.body().line("i32.sub");
            module.body().line("i32.add");
            module.body().line("i32.load8_u");
            module.body().line(&format!("local.set {}", byte));
        }
        _ => {
            emit_load_string_byte(var, &idx, module);
            module.body().line(&format!("local.set {}", byte));
        }
    }

    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    emit_runtime_string_value_byte(&byte, &word_start, transform, module);
    module.body().line("i32.store8");
    if matches!(transform, RuntimeStringValueTransform::Ucwords) {
        emit_ucwords_separator_condition(&byte, module);
        module.body().line(&format!("local.set {}", word_start));
    }
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get ${}_len", var));
}

pub(super) fn emit_runtime_string_value_byte(
    byte: &str,
    word_start: &str,
    transform: RuntimeStringValueTransform,
    module: &mut WasmModule,
) {
    match transform {
        RuntimeStringValueTransform::Case(case) => emit_ascii_case_value(byte, case, module),
        RuntimeStringValueTransform::FirstCase(case) => {
            module.body().line(&format!("local.get {}", word_start));
            module.body().open("if (result i32)");
            emit_ascii_case_value(byte, case, module);
            module.body().line("else");
            module.body().line(&format!("local.get {}", byte));
            module.body().close("end");
            module.body().line("i32.const 0");
            module.body().line(&format!("local.set {}", word_start));
        }
        RuntimeStringValueTransform::Ucwords => {
            module.body().line(&format!("local.get {}", word_start));
            module.body().open("if (result i32)");
            emit_ascii_case_value(byte, AsciiCase::Upper, module);
            module.body().line("else");
            module.body().line(&format!("local.get {}", byte));
            module.body().close("end");
        }
        RuntimeStringValueTransform::Reverse => {
            module.body().line(&format!("local.get {}", byte));
        }
    }
}

pub(super) fn emit_runtime_ascii_case_transform(
    var: &str,
    case: AsciiCase,
    module: &mut WasmModule,
) {
    let idx = module.next_label("str_idx");
    let byte = module.next_label("str_byte");
    let loop_label = module.next_label("str_loop");
    let done_label = module.next_label("str_done");
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
    emit_ascii_case_value(&byte, case, module);
    emit_write_stack_byte(module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_first_case_transform(var: &str, case: AsciiCase, module: &mut WasmModule) {
    let byte = module.next_label("str_byte");
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("else");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", byte));
    emit_ascii_case_value(&byte, case, module);
    emit_write_stack_byte(module);
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.const 1");
    module.body().line("i32.gt_u");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("call $host_write");
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_strrev(var: &str, module: &mut WasmModule) {
    let idx = module.next_label("str_idx");
    let done_label = module.next_label("str_done");
    let loop_label = module.next_label("str_loop");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", idx));
    emit_load_string_byte(var, &idx, module);
    emit_write_stack_byte(module);
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_ucwords(var: &str, module: &mut WasmModule) {
    let idx = module.next_label("ucwords_idx");
    let byte = module.next_label("ucwords_byte");
    let word_start = module.next_label("ucwords_start");
    let loop_label = module.next_label("ucwords_loop");
    let done_label = module.next_label("ucwords_done");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    module.declare_i32_local(word_start.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", word_start));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_string_byte(var, &idx, module);
    module.body().line(&format!("local.set {}", byte));
    module.body().line(&format!("local.get {}", word_start));
    module.body().open("if (result i32)");
    emit_ascii_case_value(&byte, AsciiCase::Upper, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().close("end");
    emit_write_stack_byte(module);
    emit_ucwords_separator_condition(&byte, module);
    module.body().line(&format!("local.set {}", word_start));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_ucwords_separator_condition(byte_local: &str, module: &mut WasmModule) {
    for byte in [9, 10, 11, 12, 13, 32] {
        module.body().line(&format!("local.get {}", byte_local));
        module.body().line(&format!("i32.const {}", byte));
        module.body().line("i32.eq");
    }
    for _ in 1..6 {
        module.body().line("i32.or");
    }
}

pub(super) use super::string_runtime_slashes::{
    emit_runtime_addslashes, emit_runtime_addslashes_value_to_stack, emit_runtime_stripslashes,
    emit_runtime_stripslashes_value_to_stack,
};

pub(super) use super::string_runtime_hex::{
    emit_runtime_bin2hex, emit_runtime_bin2hex_value_to_stack, emit_runtime_hex2bin,
    emit_runtime_hex2bin_value_to_stack,
};

pub(super) use super::string_runtime_base64::{
    emit_runtime_base64_decode, emit_runtime_base64_decode_value_to_stack, emit_runtime_base64_encode,
    emit_runtime_base64_encode_value_to_stack,
};

pub(super) use super::string_runtime_nl2br::{
    emit_runtime_nl2br, emit_runtime_nl2br_value_to_stack,
};

pub(super) use super::string_runtime_html_escape::{
    emit_runtime_html_escape_dynamic_value_to_stack, emit_runtime_html_escape_value_to_stack,
    emit_runtime_html_escape_with_double_encode,
};

pub(super) use super::string_runtime_url::{
    emit_runtime_urldecode, emit_runtime_urldecode_value_to_stack, emit_runtime_urlencode,
    emit_runtime_urlencode_value_to_stack,
};

pub(super) use super::string_runtime_entity_decode::{
    emit_runtime_html_entity_decode, emit_runtime_html_entity_decode_dynamic,
    emit_runtime_html_entity_decode_dynamic_value_to_stack,
    emit_runtime_html_entity_decode_value_to_stack,
};
