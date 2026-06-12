//! Purpose:
//! Emits wasm32-web heap allocation, heap-kind, refcount, and COW helpers.
//!
//! Called from:
//! - `crate::codegen::wasm::runtime::emit()`.
//!
//! Key details:
//! - Helper order is preserved because generated WAT references these names directly.
//! - These helpers operate on wasm heap payload pointers, not PHP source-level values.

use super::{
    WASM_HEAP_HEADER_SIZE, WASM_HEAP_KIND_INDEXED_ARRAY, WASM_HEAP_KIND_OFFSET_FROM_PAYLOAD,
    WASM_HEAP_KIND_RAW, WASM_HEAP_KIND_STRING, WASM_HEAP_REFCOUNT_OFFSET,
    WASM_HEAP_REFCOUNT_OFFSET_FROM_PAYLOAD, WASM_HEAP_SIZE_OFFSET_FROM_PAYLOAD,
};
use super::super::emitter::WatEmitter;

pub(super) fn emit(out: &mut WatEmitter) {
    emit_alloc_bytes(out);
    emit_alloc_string(out);
    emit_heap_kind(out);
    emit_heap_set_kind(out);
    emit_incref(out);
    emit_decref(out);
    emit_ensure_unique(out);
    emit_alloc_indexed_slots(out);
    emit_alloc_value_cells(out);
}

fn emit_alloc_bytes(out: &mut WatEmitter) {
    out.open("(func $__rt_alloc_bytes (param $size i32) (result i32)");
    out.line("(local $header i32)");
    out.line("(local $payload i32)");
    out.line("global.get $heap");
    out.line("local.set $header");
    out.line("local.get $header");
    out.line(&format!("i32.const {WASM_HEAP_HEADER_SIZE}"));
    out.line("i32.add");
    out.line("local.set $payload");
    out.line("local.get $header");
    out.line("local.get $size");
    out.line("i32.store");
    out.line("local.get $header");
    out.line(&format!("i32.const {WASM_HEAP_REFCOUNT_OFFSET}"));
    out.line("i32.add");
    out.line("i32.const 1");
    out.line("i32.store");
    out.line("local.get $payload");
    out.line(&format!("i32.const {WASM_HEAP_KIND_OFFSET_FROM_PAYLOAD}"));
    out.line("i32.sub");
    out.line(&format!("i64.const {WASM_HEAP_KIND_RAW}"));
    out.line("i64.store");
    out.line("local.get $header");
    out.line("local.get $size");
    out.line(&format!("i32.const {WASM_HEAP_HEADER_SIZE}"));
    out.line("i32.add");
    out.line("i32.add");
    out.line("global.set $heap");
    out.line("local.get $payload");
    out.close(")");
}

fn emit_alloc_string(out: &mut WatEmitter) {
    out.open("(func $__rt_alloc_string (param $ptr i32) (param $len i32) (result i32)");
    out.line("(local $payload i32)");
    out.line("(local $index i32)");
    out.line("local.get $len");
    out.line("call $__rt_alloc_bytes");
    out.line("local.set $payload");
    out.line("local.get $payload");
    out.line(&format!("i32.const {WASM_HEAP_KIND_STRING}"));
    out.line("call $__rt_heap_set_kind");
    out.line("local.set $payload");
    out.line("i32.const 0");
    out.line("local.set $index");
    out.open("block $alloc_string_done");
    out.open("loop $alloc_string_loop");
    out.line("local.get $index");
    out.line("local.get $len");
    out.line("i32.ge_u");
    out.line("br_if $alloc_string_done");
    out.line("local.get $payload");
    out.line("local.get $index");
    out.line("i32.add");
    out.line("local.get $ptr");
    out.line("local.get $index");
    out.line("i32.add");
    out.line("i32.load8_u");
    out.line("i32.store8");
    out.line("local.get $index");
    out.line("i32.const 1");
    out.line("i32.add");
    out.line("local.set $index");
    out.line("br $alloc_string_loop");
    out.close("end");
    out.close("end");
    out.line("local.get $payload");
    out.close(")");
}

fn emit_heap_kind(out: &mut WatEmitter) {
    out.open("(func $__rt_heap_kind (param $payload i32) (result i32)");
    out.line("local.get $payload");
    out.line(&format!("i32.const {WASM_HEAP_KIND_OFFSET_FROM_PAYLOAD}"));
    out.line("i32.sub");
    out.line("i64.load");
    out.line("i32.wrap_i64");
    out.close(")");
}

fn emit_heap_set_kind(out: &mut WatEmitter) {
    out.open("(func $__rt_heap_set_kind (param $payload i32) (param $kind i32) (result i32)");
    out.line("local.get $payload");
    out.line(&format!("i32.const {WASM_HEAP_KIND_OFFSET_FROM_PAYLOAD}"));
    out.line("i32.sub");
    out.line("local.get $kind");
    out.line("i64.extend_i32_u");
    out.line("i64.store");
    out.line("local.get $payload");
    out.close(")");
}

fn emit_incref(out: &mut WatEmitter) {
    out.open("(func $__rt_incref (param $payload i32) (result i32)");
    out.line("local.get $payload");
    out.line("i32.eqz");
    out.open("if");
    out.line("i32.const 0");
    out.line("return");
    out.close("end");
    out.line("local.get $payload");
    out.line(&format!("i32.const {WASM_HEAP_REFCOUNT_OFFSET_FROM_PAYLOAD}"));
    out.line("i32.sub");
    out.line("local.get $payload");
    out.line(&format!("i32.const {WASM_HEAP_REFCOUNT_OFFSET_FROM_PAYLOAD}"));
    out.line("i32.sub");
    out.line("i32.load");
    out.line("i32.const 1");
    out.line("i32.add");
    out.line("i32.store");
    out.line("local.get $payload");
    out.close(")");
}

fn emit_decref(out: &mut WatEmitter) {
    out.open("(func $__rt_decref (param $payload i32)");
    out.line("local.get $payload");
    out.line("i32.eqz");
    out.open("if");
    out.line("return");
    out.close("end");
    out.line("local.get $payload");
    out.line(&format!("i32.const {WASM_HEAP_REFCOUNT_OFFSET_FROM_PAYLOAD}"));
    out.line("i32.sub");
    out.line("local.get $payload");
    out.line(&format!("i32.const {WASM_HEAP_REFCOUNT_OFFSET_FROM_PAYLOAD}"));
    out.line("i32.sub");
    out.line("i32.load");
    out.line("i32.const 1");
    out.line("i32.sub");
    out.line("i32.store");
    out.close(")");
}

fn emit_ensure_unique(out: &mut WatEmitter) {
    out.open("(func $__rt_ensure_unique (param $payload i32) (result i32)");
    out.line("(local $size i32)");
    out.line("(local $kind i32)");
    out.line("(local $new_payload i32)");
    out.line("(local $index i32)");
    out.line("local.get $payload");
    out.line("i32.eqz");
    out.open("if");
    out.line("i32.const 0");
    out.line("return");
    out.close("end");
    out.line("local.get $payload");
    out.line(&format!("i32.const {WASM_HEAP_REFCOUNT_OFFSET_FROM_PAYLOAD}"));
    out.line("i32.sub");
    out.line("i32.load");
    out.line("i32.const 1");
    out.line("i32.le_u");
    out.open("if");
    out.line("local.get $payload");
    out.line("return");
    out.close("end");
    out.line("local.get $payload");
    out.line(&format!("i32.const {WASM_HEAP_SIZE_OFFSET_FROM_PAYLOAD}"));
    out.line("i32.sub");
    out.line("i32.load");
    out.line("local.set $size");
    out.line("local.get $payload");
    out.line("call $__rt_heap_kind");
    out.line("local.set $kind");
    out.line("local.get $payload");
    out.line("call $__rt_decref");
    out.line("local.get $size");
    out.line("call $__rt_alloc_bytes");
    out.line("local.set $new_payload");
    out.line("local.get $new_payload");
    out.line("local.get $kind");
    out.line("call $__rt_heap_set_kind");
    out.line("local.set $new_payload");
    out.line("i32.const 0");
    out.line("local.set $index");
    out.open("block $ensure_unique_done");
    out.open("loop $ensure_unique_loop");
    out.line("local.get $index");
    out.line("local.get $size");
    out.line("i32.ge_u");
    out.line("br_if $ensure_unique_done");
    out.line("local.get $new_payload");
    out.line("local.get $index");
    out.line("i32.add");
    out.line("local.get $payload");
    out.line("local.get $index");
    out.line("i32.add");
    out.line("i32.load8_u");
    out.line("i32.store8");
    out.line("local.get $index");
    out.line("i32.const 1");
    out.line("i32.add");
    out.line("local.set $index");
    out.line("br $ensure_unique_loop");
    out.close("end");
    out.close("end");
    out.line("local.get $new_payload");
    out.close(")");
}

fn emit_alloc_indexed_slots(out: &mut WatEmitter) {
    out.open("(func $__rt_alloc_indexed_slots (param $count i32) (result i32)");
    out.line("(local $payload i32)");
    out.line("local.get $count");
    out.line("i32.const 8");
    out.line("i32.mul");
    out.line("call $__rt_alloc_bytes");
    out.line("local.set $payload");
    out.line("local.get $payload");
    out.line(&format!("i32.const {WASM_HEAP_KIND_INDEXED_ARRAY}"));
    out.line("call $__rt_heap_set_kind");
    out.close(")");
}

fn emit_alloc_value_cells(out: &mut WatEmitter) {
    out.open("(func $__rt_alloc_value_cells (param $count i32) (result i32)");
    out.line("(local $payload i32)");
    out.line("local.get $count");
    out.line("i32.const 16");
    out.line("i32.mul");
    out.line("call $__rt_alloc_bytes");
    out.line("local.set $payload");
    out.line("local.get $payload");
    out.line(&format!("i32.const {WASM_HEAP_KIND_INDEXED_ARRAY}"));
    out.line("call $__rt_heap_set_kind");
    out.close(")");
}
