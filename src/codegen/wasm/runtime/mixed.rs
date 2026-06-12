//! Purpose:
//! Emits wasm32-web runtime helpers for PHP `mixed` cells.
//! Provides a semantic boundary above the lower-level value-cell primitives.
//!
//! Called from:
//! - `crate::codegen::wasm::runtime::emit()`.
//!
//! Key details:
//! - Known typed paths can keep using direct lowering; unknown boxed values should
//!   move here so tag checks, payload access, and traps stay centralized.

use super::super::emitter::WatEmitter;

const WASM_VALUE_TAG_NULL: i32 = 0;
const WASM_VALUE_TAG_INT: i32 = 1;
const WASM_VALUE_TAG_BOOL: i32 = 2;
const WASM_VALUE_TAG_STRING: i32 = 3;
const WASM_VALUE_TAG_ARRAY: i32 = 4;
const WASM_VALUE_TAG_FLOAT: i32 = 5;
const WASM_VALUE_TAG_OBJECT: i32 = 6;

pub(super) fn emit(out: &mut WatEmitter) {
    emit_mixed_tag(out);
    emit_mixed_tag_equals(out);
    emit_mixed_payload_i32(out);
    emit_mixed_payload_i64(out);
    emit_mixed_payload_f64(out);
    emit_mixed_count_array(out);
    emit_mixed_count_array_checked(out);
    emit_mixed_array_ptr(out);
    emit_mixed_array_len_i32(out);
    emit_mixed_array_heap_kind(out);
    emit_mixed_truthy(out);
}

fn emit_mixed_tag(out: &mut WatEmitter) {
    out.open("(func $__rt_mixed_tag (param $cell i32) (result i32)");
    out.line("local.get $cell");
    out.line("i32.load");
    out.close(")");
}

fn emit_mixed_tag_equals(out: &mut WatEmitter) {
    out.open("(func $__rt_mixed_tag_equals (param $cell i32) (param $tag i32) (result i32)");
    out.line("local.get $cell");
    out.line("call $__rt_mixed_tag");
    out.line("local.get $tag");
    out.line("i32.eq");
    out.close(")");
}

fn emit_mixed_payload_i32(out: &mut WatEmitter) {
    out.open("(func $__rt_mixed_payload_i32 (param $cell i32) (param $offset i32) (result i32)");
    out.line("local.get $cell");
    out.line("local.get $offset");
    out.line("i32.add");
    out.line("i32.load");
    out.close(")");
}

fn emit_mixed_payload_i64(out: &mut WatEmitter) {
    out.open("(func $__rt_mixed_payload_i64 (param $cell i32) (result i64)");
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("i64.load");
    out.close(")");
}

fn emit_mixed_payload_f64(out: &mut WatEmitter) {
    out.open("(func $__rt_mixed_payload_f64 (param $cell i32) (result f64)");
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("f64.load");
    out.close(")");
}

fn emit_mixed_count_array(out: &mut WatEmitter) {
    out.open("(func $__rt_mixed_count_array (param $cell i32) (result i64)");
    out.line("local.get $cell");
    out.line("i32.const 12");
    out.line("i32.add");
    out.line("i32.load");
    out.line("i64.extend_i32_u");
    out.close(")");
}

fn emit_mixed_count_array_checked(out: &mut WatEmitter) {
    out.open("(func $__rt_mixed_count_array_checked (param $cell i32) (result i64)");
    out.line("local.get $cell");
    out.line("i32.load");
    out.line(&format!("i32.const {WASM_VALUE_TAG_ARRAY}"));
    out.line("i32.ne");
    out.open("if");
    out.line("unreachable");
    out.close("end");
    out.line("local.get $cell");
    out.line("call $__rt_mixed_count_array");
    out.close(")");
}

fn emit_mixed_array_ptr(out: &mut WatEmitter) {
    out.open("(func $__rt_mixed_array_ptr (param $cell i32) (result i32)");
    emit_check_mixed_array_tag(out);
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("i32.load");
    out.close(")");
}

fn emit_mixed_array_len_i32(out: &mut WatEmitter) {
    out.open("(func $__rt_mixed_array_len_i32 (param $cell i32) (result i32)");
    emit_check_mixed_array_tag(out);
    out.line("local.get $cell");
    out.line("i32.const 12");
    out.line("i32.add");
    out.line("i32.load");
    out.close(")");
}

fn emit_mixed_array_heap_kind(out: &mut WatEmitter) {
    out.open("(func $__rt_mixed_array_heap_kind (param $cell i32) (result i32)");
    emit_check_mixed_array_tag(out);
    out.line("local.get $cell");
    out.line("call $__rt_mixed_array_ptr");
    out.line("call $__rt_heap_kind");
    out.close(")");
}

fn emit_check_mixed_array_tag(out: &mut WatEmitter) {
    out.line("local.get $cell");
    out.line("i32.load");
    out.line(&format!("i32.const {WASM_VALUE_TAG_ARRAY}"));
    out.line("i32.ne");
    out.open("if");
    out.line("unreachable");
    out.close("end");
}

fn emit_mixed_truthy(out: &mut WatEmitter) {
    out.open("(func $__rt_mixed_truthy (param $cell i32) (result i32)");
    out.line("local.get $cell");
    out.line("i32.load");
    out.line(&format!("i32.const {WASM_VALUE_TAG_NULL}"));
    out.line("i32.eq");
    out.open("if (result i32)");
    out.line("i32.const 0");
    out.line("else");
    out.line("local.get $cell");
    out.line("i32.load");
    out.line(&format!("i32.const {WASM_VALUE_TAG_BOOL}"));
    out.line("i32.eq");
    out.open("if (result i32)");
    emit_mixed_i64_payload_truthy(out);
    out.line("else");
    out.line("local.get $cell");
    out.line("i32.load");
    out.line(&format!("i32.const {WASM_VALUE_TAG_INT}"));
    out.line("i32.eq");
    out.open("if (result i32)");
    emit_mixed_i64_payload_truthy(out);
    out.line("else");
    out.line("local.get $cell");
    out.line("i32.load");
    out.line(&format!("i32.const {WASM_VALUE_TAG_FLOAT}"));
    out.line("i32.eq");
    out.open("if (result i32)");
    emit_mixed_f64_payload_truthy(out);
    out.line("else");
    out.line("local.get $cell");
    out.line("i32.load");
    out.line(&format!("i32.const {WASM_VALUE_TAG_STRING}"));
    out.line("i32.eq");
    out.open("if (result i32)");
    emit_mixed_string_truthy(out);
    out.line("else");
    out.line("local.get $cell");
    out.line("i32.load");
    out.line(&format!("i32.const {WASM_VALUE_TAG_ARRAY}"));
    out.line("i32.eq");
    out.open("if (result i32)");
    emit_mixed_i32_payload_truthy(out, 12);
    out.line("else");
    out.line("local.get $cell");
    out.line("i32.load");
    out.line(&format!("i32.const {WASM_VALUE_TAG_OBJECT}"));
    out.line("i32.eq");
    out.close("end");
    out.close("end");
    out.close("end");
    out.close("end");
    out.close("end");
    out.close("end");
    out.close(")");
}

fn emit_mixed_i64_payload_truthy(out: &mut WatEmitter) {
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("i64.load");
    out.line("i64.const 0");
    out.line("i64.ne");
}

fn emit_mixed_f64_payload_truthy(out: &mut WatEmitter) {
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("f64.load");
    out.line("f64.const 0");
    out.line("f64.ne");
}

fn emit_mixed_i32_payload_truthy(out: &mut WatEmitter, offset: i32) {
    out.line("local.get $cell");
    out.line(&format!("i32.const {offset}"));
    out.line("i32.add");
    out.line("i32.load");
    out.line("i32.const 0");
    out.line("i32.ne");
}

fn emit_mixed_string_truthy(out: &mut WatEmitter) {
    out.line("local.get $cell");
    out.line("i32.const 12");
    out.line("i32.add");
    out.line("i32.load");
    out.line("i32.const 1");
    out.line("i32.ne");
    out.open("if (result i32)");
    emit_mixed_i32_payload_truthy(out, 12);
    out.line("else");
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("i32.load");
    out.line("i32.load8_u");
    out.line("i32.const 48");
    out.line("i32.ne");
    out.close("end");
}
