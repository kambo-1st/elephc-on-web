//! Purpose:
//! Emits wasm32-web associative-array runtime helpers for key storage,
//! lookup comparison, entry copying, and assoc payload allocation.
//!
//! Called from:
//! - `crate::codegen::wasm::runtime::emit()`.
//!
//! Key details:
//! - Assoc entries are 32-byte records with key metadata followed by a value cell.
//! - Helper order is preserved by the runtime orchestrator for stable generated WAT.

use super::WASM_HEAP_KIND_ASSOC_ARRAY;
use super::super::emitter::WatEmitter;

pub(super) fn emit(out: &mut WatEmitter) {
    emit_assoc_entry(out);
    emit_assoc_value_cell(out);
    emit_assoc_array_release(out);
    emit_assoc_store_string_key(out);
    emit_assoc_store_int_key(out);
    emit_php_array_key_is_int(out);
    emit_php_array_key_to_int(out);
    emit_assoc_store_php_string_key(out);
    emit_assoc_key_payload_i64(out);
    emit_assoc_key_eq_int(out);
    emit_assoc_key_eq_string(out);
    emit_assoc_key_eq_php_string(out);
    emit_assoc_copy_entry(out);
    emit_assoc_array_copy_entries(out);
    emit_assoc_copy_key(out);
    emit_assoc_keys_equal(out);
}

pub(super) fn emit_alloc_assoc_entries(out: &mut WatEmitter) {
    out.open("(func $__rt_alloc_assoc_entries (param $count i32) (result i32)");
    out.line("(local $payload i32)");
    out.line("local.get $count");
    out.line("i32.const 32");
    out.line("i32.mul");
    out.line("call $__rt_alloc_bytes");
    out.line("local.set $payload");
    out.line("local.get $payload");
    out.line(&format!("i32.const {WASM_HEAP_KIND_ASSOC_ARRAY}"));
    out.line("call $__rt_heap_set_kind");
    out.close(")");
}

fn emit_assoc_entry(out: &mut WatEmitter) {
    out.open("(func $__rt_assoc_entry (param $base i32) (param $index i32) (result i32)");
    out.line("local.get $base");
    out.line("local.get $index");
    out.line("i32.const 32");
    out.line("i32.mul");
    out.line("i32.add");
    out.close(")");
}

fn emit_assoc_value_cell(out: &mut WatEmitter) {
    out.open("(func $__rt_assoc_value_cell (param $entry i32) (result i32)");
    out.line("local.get $entry");
    out.line("i32.const 16");
    out.line("i32.add");
    out.close(")");
}

fn emit_assoc_array_release(out: &mut WatEmitter) {
    out.open("(func $__rt_assoc_array_release (param $base i32) (param $len i32)");
    out.line("(local $index i32)");
    out.line("i32.const 0");
    out.line("local.set $index");
    out.open("block $assoc_array_release_done");
    out.open("loop $assoc_array_release_loop");
    out.line("local.get $index");
    out.line("local.get $len");
    out.line("i32.ge_u");
    out.line("br_if $assoc_array_release_done");
    out.line("local.get $base");
    out.line("local.get $index");
    out.line("call $__rt_assoc_entry");
    out.line("call $__rt_assoc_value_cell");
    out.line("call $__rt_value_release");
    out.line("local.get $index");
    out.line("i32.const 1");
    out.line("i32.add");
    out.line("local.set $index");
    out.line("br $assoc_array_release_loop");
    out.close("end");
    out.close("end");
    out.close(")");
}

fn emit_assoc_store_string_key(out: &mut WatEmitter) {
    out.open("(func $__rt_assoc_store_string_key (param $entry i32) (param $ptr i32) (param $len i32)");
    out.line("local.get $entry");
    out.line("i32.const 1");
    out.line("i32.store");
    out.line("local.get $entry");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("local.get $ptr");
    out.line("i32.store");
    out.line("local.get $entry");
    out.line("i32.const 12");
    out.line("i32.add");
    out.line("local.get $len");
    out.line("i32.store");
    out.close(")");
}

fn emit_assoc_store_int_key(out: &mut WatEmitter) {
    out.open("(func $__rt_assoc_store_int_key (param $entry i32) (param $key i64)");
    out.line("local.get $entry");
    out.line("i32.const 0");
    out.line("i32.store");
    out.line("local.get $entry");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("local.get $key");
    out.line("i64.store");
    out.close(")");
}

fn emit_php_array_key_is_int(out: &mut WatEmitter) {
    out.open("(func $__rt_php_array_key_is_int (param $ptr i32) (param $len i32) (result i32)");
    out.line("(local $start i32)");
    out.line("(local $digits i32)");
    out.line("(local $index i32)");
    out.line("(local $byte i32)");
    out.line("(local $ok i32)");
    out.line("(local $value i64)");
    out.line("i32.const 0");
    out.line("local.set $ok");
    out.line("local.get $len");
    out.line("i32.eqz");
    out.open("if");
    out.line("i32.const 0");
    out.line("return");
    out.close("end");
    out.line("local.get $ptr");
    out.line("i32.load8_u");
    out.line("i32.const 43");
    out.line("i32.eq");
    out.open("if");
    out.line("i32.const 0");
    out.line("return");
    out.close("end");
    out.line("local.get $ptr");
    out.line("i32.load8_u");
    out.line("i32.const 45");
    out.line("i32.eq");
    out.open("if");
    out.line("i32.const 1");
    out.line("local.set $start");
    out.line("local.get $len");
    out.line("i32.const 1");
    out.line("i32.le_u");
    out.open("if");
    out.line("i32.const 0");
    out.line("return");
    out.close("end");
    out.close("end");
    out.line("local.get $len");
    out.line("local.get $start");
    out.line("i32.sub");
    out.line("local.set $digits");
    out.line("local.get $digits");
    out.line("i32.const 19");
    out.line("i32.gt_u");
    out.open("if");
    out.line("i32.const 0");
    out.line("return");
    out.close("end");
    out.line("local.get $digits");
    out.line("i32.const 1");
    out.line("i32.gt_u");
    out.open("if");
    out.line("local.get $ptr");
    out.line("local.get $start");
    out.line("i32.add");
    out.line("i32.load8_u");
    out.line("i32.const 48");
    out.line("i32.eq");
    out.open("if");
    out.line("i32.const 0");
    out.line("return");
    out.close("end");
    out.close("end");
    out.line("local.get $start");
    out.line("local.set $index");
    out.line("i32.const 1");
    out.line("local.set $ok");
    out.open("block $php_array_key_is_int_done");
    out.open("loop $php_array_key_is_int_loop");
    out.line("local.get $index");
    out.line("local.get $len");
    out.line("i32.ge_u");
    out.line("br_if $php_array_key_is_int_done");
    out.line("local.get $ptr");
    out.line("local.get $index");
    out.line("i32.add");
    out.line("i32.load8_u");
    out.line("local.set $byte");
    out.line("local.get $byte");
    out.line("i32.const 48");
    out.line("i32.lt_u");
    out.line("local.get $byte");
    out.line("i32.const 57");
    out.line("i32.gt_u");
    out.line("i32.or");
    out.open("if");
    out.line("i32.const 0");
    out.line("local.set $ok");
    out.line("br $php_array_key_is_int_done");
    out.close("end");
    out.line("local.get $value");
    out.line("i64.const 10");
    out.line("i64.mul");
    out.line("local.get $byte");
    out.line("i32.const 48");
    out.line("i32.sub");
    out.line("i64.extend_i32_u");
    out.line("i64.add");
    out.line("local.set $value");
    out.line("local.get $index");
    out.line("i32.const 1");
    out.line("i32.add");
    out.line("local.set $index");
    out.line("br $php_array_key_is_int_loop");
    out.close("end");
    out.close("end");
    out.line("local.get $ok");
    out.open("if");
    out.line("local.get $digits");
    out.line("i32.const 19");
    out.line("i32.eq");
    out.open("if");
    out.line("local.get $start");
    out.open("if");
    out.line("local.get $value");
    out.line("i64.const -9223372036854775808");
    out.line("i64.gt_u");
    out.open("if");
    out.line("i32.const 0");
    out.line("local.set $ok");
    out.close("end");
    out.line("else");
    out.line("local.get $value");
    out.line("i64.const 9223372036854775807");
    out.line("i64.gt_u");
    out.open("if");
    out.line("i32.const 0");
    out.line("local.set $ok");
    out.close("end");
    out.close("end");
    out.close("end");
    out.close("end");
    out.line("local.get $ok");
    out.close(")");
}

fn emit_php_array_key_to_int(out: &mut WatEmitter) {
    out.open("(func $__rt_php_array_key_to_int (param $ptr i32) (param $len i32) (result i64)");
    out.line("(local $start i32)");
    out.line("(local $negative i32)");
    out.line("(local $index i32)");
    out.line("(local $value i64)");
    out.line("local.get $ptr");
    out.line("i32.load8_u");
    out.line("i32.const 45");
    out.line("i32.eq");
    out.open("if");
    out.line("i32.const 1");
    out.line("local.set $start");
    out.line("i32.const 1");
    out.line("local.set $negative");
    out.close("end");
    out.line("local.get $start");
    out.line("local.set $index");
    out.open("block $php_array_key_to_int_done");
    out.open("loop $php_array_key_to_int_loop");
    out.line("local.get $index");
    out.line("local.get $len");
    out.line("i32.ge_u");
    out.line("br_if $php_array_key_to_int_done");
    out.line("local.get $value");
    out.line("i64.const 10");
    out.line("i64.mul");
    out.line("local.get $ptr");
    out.line("local.get $index");
    out.line("i32.add");
    out.line("i32.load8_u");
    out.line("i32.const 48");
    out.line("i32.sub");
    out.line("i64.extend_i32_u");
    out.line("i64.add");
    out.line("local.set $value");
    out.line("local.get $index");
    out.line("i32.const 1");
    out.line("i32.add");
    out.line("local.set $index");
    out.line("br $php_array_key_to_int_loop");
    out.close("end");
    out.close("end");
    out.line("local.get $negative");
    out.open("if (result i64)");
    out.line("i64.const 0");
    out.line("local.get $value");
    out.line("i64.sub");
    out.line("else");
    out.line("local.get $value");
    out.close("end");
    out.close(")");
}

fn emit_assoc_store_php_string_key(out: &mut WatEmitter) {
    out.open("(func $__rt_assoc_store_php_string_key (param $entry i32) (param $ptr i32) (param $len i32)");
    out.line("local.get $ptr");
    out.line("local.get $len");
    out.line("call $__rt_php_array_key_is_int");
    out.open("if");
    out.line("local.get $entry");
    out.line("local.get $ptr");
    out.line("local.get $len");
    out.line("call $__rt_php_array_key_to_int");
    out.line("call $__rt_assoc_store_int_key");
    out.line("else");
    out.line("local.get $entry");
    out.line("local.get $ptr");
    out.line("local.get $len");
    out.line("call $__rt_assoc_store_string_key");
    out.close("end");
    out.close(")");
}

fn emit_assoc_key_eq_int(out: &mut WatEmitter) {
    out.open("(func $__rt_assoc_key_eq_int (param $entry i32) (param $key i64) (result i32)");
    out.line("local.get $entry");
    out.line("i32.load");
    out.line("i32.eqz");
    out.line("local.get $entry");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("i64.load");
    out.line("local.get $key");
    out.line("i64.eq");
    out.line("i32.and");
    out.close(")");
}

fn emit_assoc_key_payload_i64(out: &mut WatEmitter) {
    out.open("(func $__rt_assoc_key_payload_i64 (param $entry i32) (result i64)");
    out.line("local.get $entry");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("i64.load");
    out.close(")");
}

fn emit_assoc_key_eq_string(out: &mut WatEmitter) {
    out.open("(func $__rt_assoc_key_eq_string (param $entry i32) (param $ptr i32) (param $len i32) (result i32)");
    out.line("(local $offset i32)");
    out.line("(local $matched i32)");
    out.line("i32.const 0");
    out.line("local.set $matched");
    out.line("local.get $entry");
    out.line("i32.load");
    out.line("i32.const 1");
    out.line("i32.eq");
    out.open("if");
    out.line("local.get $entry");
    out.line("i32.const 12");
    out.line("i32.add");
    out.line("i32.load");
    out.line("local.get $len");
    out.line("i32.eq");
    out.open("if");
    out.line("i32.const 1");
    out.line("local.set $matched");
    out.line("i32.const 0");
    out.line("local.set $offset");
    out.open("block $done");
    out.open("loop $loop");
    out.line("local.get $offset");
    out.line("local.get $len");
    out.line("i32.ge_u");
    out.line("br_if $done");
    out.line("local.get $entry");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("i32.load");
    out.line("local.get $offset");
    out.line("i32.add");
    out.line("i32.load8_u");
    out.line("local.get $ptr");
    out.line("local.get $offset");
    out.line("i32.add");
    out.line("i32.load8_u");
    out.line("i32.ne");
    out.open("if");
    out.line("i32.const 0");
    out.line("local.set $matched");
    out.line("br $done");
    out.close("end");
    out.line("local.get $offset");
    out.line("i32.const 1");
    out.line("i32.add");
    out.line("local.set $offset");
    out.line("br $loop");
    out.close("end");
    out.close("end");
    out.close("end");
    out.close("end");
    out.line("local.get $matched");
    out.close(")");
}

fn emit_assoc_key_eq_php_string(out: &mut WatEmitter) {
    out.open("(func $__rt_assoc_key_eq_php_string (param $entry i32) (param $ptr i32) (param $len i32) (result i32)");
    out.line("local.get $ptr");
    out.line("local.get $len");
    out.line("call $__rt_php_array_key_is_int");
    out.open("if (result i32)");
    out.line("local.get $entry");
    out.line("local.get $ptr");
    out.line("local.get $len");
    out.line("call $__rt_php_array_key_to_int");
    out.line("call $__rt_assoc_key_eq_int");
    out.line("else");
    out.line("local.get $entry");
    out.line("local.get $ptr");
    out.line("local.get $len");
    out.line("call $__rt_assoc_key_eq_string");
    out.close("end");
    out.close(")");
}

fn emit_assoc_copy_entry(out: &mut WatEmitter) {
    out.open("(func $__rt_assoc_copy_entry (param $dst i32) (param $src i32)");
    emit_copy_i64_field(out, 0);
    emit_copy_i64_field(out, 8);
    out.line("local.get $dst");
    out.line("i32.const 16");
    out.line("i32.add");
    out.line("local.get $src");
    out.line("i32.const 16");
    out.line("i32.add");
    out.line("call $__rt_value_copy");
    out.close(")");
}

fn emit_assoc_array_copy_entries(out: &mut WatEmitter) {
    out.open("(func $__rt_assoc_array_copy_entries (param $source i32) (param $len i32) (result i32)");
    out.line("(local $copy i32)");
    out.line("(local $index i32)");
    out.line("local.get $len");
    out.line("call $__rt_alloc_assoc_entries");
    out.line("local.set $copy");
    out.line("i32.const 0");
    out.line("local.set $index");
    out.open("block $assoc_array_copy_done");
    out.open("loop $assoc_array_copy_loop");
    out.line("local.get $index");
    out.line("local.get $len");
    out.line("i32.ge_u");
    out.line("br_if $assoc_array_copy_done");
    out.line("local.get $copy");
    out.line("local.get $index");
    out.line("call $__rt_assoc_entry");
    out.line("local.get $source");
    out.line("local.get $index");
    out.line("call $__rt_assoc_entry");
    out.line("call $__rt_assoc_copy_entry");
    out.line("local.get $index");
    out.line("i32.const 1");
    out.line("i32.add");
    out.line("local.set $index");
    out.line("br $assoc_array_copy_loop");
    out.close("end");
    out.close("end");
    out.line("local.get $copy");
    out.close(")");
}

fn emit_assoc_copy_key(out: &mut WatEmitter) {
    out.open("(func $__rt_assoc_copy_key (param $dst i32) (param $src i32)");
    emit_copy_i64_field(out, 0);
    emit_copy_i64_field(out, 8);
    out.close(")");
}

fn emit_copy_i64_field(out: &mut WatEmitter, offset: i32) {
    out.line("local.get $dst");
    out.line(&format!("i32.const {}", offset));
    out.line("i32.add");
    out.line("local.get $src");
    out.line(&format!("i32.const {}", offset));
    out.line("i32.add");
    out.line("i64.load");
    out.line("i64.store");
}

fn emit_assoc_keys_equal(out: &mut WatEmitter) {
    out.open("(func $__rt_assoc_keys_equal (param $left i32) (param $right i32) (result i32)");
    out.line("(local $matched i32)");
    out.line("i32.const 0");
    out.line("local.set $matched");
    out.line("local.get $left");
    out.line("i32.load");
    out.line("local.get $right");
    out.line("i32.load");
    out.line("i32.eq");
    out.open("if");
    out.line("local.get $left");
    out.line("i32.load");
    out.line("i32.eqz");
    out.open("if");
    out.line("local.get $right");
    out.line("local.get $left");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("i64.load");
    out.line("call $__rt_assoc_key_eq_int");
    out.line("local.set $matched");
    out.line("else");
    out.line("local.get $right");
    out.line("local.get $left");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("i32.load");
    out.line("local.get $left");
    out.line("i32.const 12");
    out.line("i32.add");
    out.line("i32.load");
    out.line("call $__rt_assoc_key_eq_string");
    out.line("local.set $matched");
    out.close("end");
    out.close("end");
    out.line("local.get $matched");
    out.close(")");
}
