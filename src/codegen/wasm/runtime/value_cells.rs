//! Purpose:
//! Emits wasm32-web mixed/value-cell runtime helpers for copy, ownership,
//! storage, indexing, output, and value-array chunk construction.
//!
//! Called from:
//! - `crate::codegen::wasm::runtime::emit()`.
//!
//! Key details:
//! - Helper order is intentionally preserved to keep generated WAT stable.
//! - Value cells are 16-byte tagged runtime cells in wasm linear memory.

use super::{WASM_HEAP_KIND_MIXED, WASM_VALUE_CELL_SIZE};
use super::super::emitter::WatEmitter;

pub(super) fn emit(out: &mut WatEmitter) {
    emit_alloc_mixed_cell(out);
    emit_alloc_null_mixed_cell(out);
    emit_value_retain(out);
    emit_value_release(out);
    emit_value_copy(out);
    emit_value_copy_index_or_null(out);
    emit_value_index_in_bounds(out);
    emit_value_array_release(out);
    emit_value_store_int(out);
    emit_value_store_float(out);
    emit_value_store_bool(out);
    emit_value_store_string(out);
    emit_value_store_array(out);
    emit_value_store_object(out);
    emit_value_store_null(out);
    emit_value_cell(out);
    emit_output_value_cell(out);
    emit_value_tag(out);
    emit_value_tag_equals(out);
    emit_value_payload_i32(out);
    emit_value_payload_i64(out);
    emit_value_payload_f64(out);
    emit_value_cell_payload_i32(out);
    emit_value_cell_payload_i64(out);
    emit_value_cell_payload_f64(out);
    emit_string_range_empty_cast_int(out);
}

pub(super) fn emit_array_chunk_value_cells(out: &mut WatEmitter) {
    out.open("(func $__rt_array_chunk_value_cells (param $source i32) (param $source_len i32) (param $chunk_size i32) (param $outer_len i32) (result i32)");
    out.line("(local $outer i32)");
    out.line("(local $chunk_index i32)");
    out.line("(local $source_index i32)");
    out.line("(local $inner_index i32)");
    out.line("(local $take i32)");
    out.line("(local $inner i32)");
    out.line("(local $dst i32)");
    out.line("(local $src i32)");
    out.line("local.get $chunk_size");
    out.line("i32.const 0");
    out.line("i32.le_s");
    out.open("if");
    out.line("unreachable");
    out.close("end");
    out.line("local.get $outer_len");
    out.line("call $__rt_alloc_value_cells");
    out.line("local.set $outer");
    out.line("i32.const 0");
    out.line("local.set $chunk_index");
    out.line("i32.const 0");
    out.line("local.set $source_index");
    out.open("block $array_chunk_done");
    out.open("loop $array_chunk_loop");
    out.line("local.get $chunk_index");
    out.line("local.get $outer_len");
    out.line("i32.ge_u");
    out.line("br_if $array_chunk_done");
    out.line("local.get $source_len");
    out.line("local.get $source_index");
    out.line("i32.sub");
    out.line("local.set $take");
    out.line("local.get $take");
    out.line("local.get $chunk_size");
    out.line("i32.gt_u");
    out.open("if");
    out.line("local.get $chunk_size");
    out.line("local.set $take");
    out.close("end");
    out.line("local.get $take");
    out.line("call $__rt_alloc_value_cells");
    out.line("local.set $inner");
    out.line("i32.const 0");
    out.line("local.set $inner_index");
    out.open("block $array_chunk_copy_done");
    out.open("loop $array_chunk_copy_loop");
    out.line("local.get $inner_index");
    out.line("local.get $take");
    out.line("i32.ge_u");
    out.line("br_if $array_chunk_copy_done");
    out.line("local.get $inner");
    out.line("local.get $inner_index");
    out.line(&format!("i32.const {WASM_VALUE_CELL_SIZE}"));
    out.line("i32.mul");
    out.line("i32.add");
    out.line("local.set $dst");
    out.line("local.get $source");
    out.line("local.get $source_index");
    out.line("local.get $inner_index");
    out.line("i32.add");
    out.line(&format!("i32.const {WASM_VALUE_CELL_SIZE}"));
    out.line("i32.mul");
    out.line("i32.add");
    out.line("local.set $src");
    out.line("local.get $dst");
    out.line("local.get $src");
    out.line("call $__rt_value_copy");
    out.line("local.get $inner_index");
    out.line("i32.const 1");
    out.line("i32.add");
    out.line("local.set $inner_index");
    out.line("br $array_chunk_copy_loop");
    out.close("end");
    out.close("end");
    out.line("local.get $outer");
    out.line("local.get $chunk_index");
    out.line(&format!("i32.const {WASM_VALUE_CELL_SIZE}"));
    out.line("i32.mul");
    out.line("i32.add");
    out.line("local.get $inner");
    out.line("local.get $take");
    out.line("call $__rt_value_store_array");
    out.line("local.get $source_index");
    out.line("local.get $take");
    out.line("i32.add");
    out.line("local.set $source_index");
    out.line("local.get $chunk_index");
    out.line("i32.const 1");
    out.line("i32.add");
    out.line("local.set $chunk_index");
    out.line("br $array_chunk_loop");
    out.close("end");
    out.close("end");
    out.line("local.get $outer");
    out.close(")");
}

pub(super) fn emit_array_chunk_value_cells_preserve_int_keys(out: &mut WatEmitter) {
    out.open("(func $__rt_array_chunk_value_cells_preserve_int_keys (param $source i32) (param $source_len i32) (param $chunk_size i32) (param $outer_len i32) (result i32)");
    out.line("(local $outer i32)");
    out.line("(local $chunk_index i32)");
    out.line("(local $source_index i32)");
    out.line("(local $inner_index i32)");
    out.line("(local $take i32)");
    out.line("(local $inner i32)");
    out.line("(local $entry i32)");
    out.line("(local $dst i32)");
    out.line("(local $src i32)");
    out.line("local.get $chunk_size");
    out.line("i32.const 0");
    out.line("i32.le_s");
    out.open("if");
    out.line("unreachable");
    out.close("end");
    out.line("local.get $outer_len");
    out.line("call $__rt_alloc_value_cells");
    out.line("local.set $outer");
    out.line("i32.const 0");
    out.line("local.set $chunk_index");
    out.line("i32.const 0");
    out.line("local.set $source_index");
    out.open("block $array_chunk_done");
    out.open("loop $array_chunk_loop");
    out.line("local.get $chunk_index");
    out.line("local.get $outer_len");
    out.line("i32.ge_u");
    out.line("br_if $array_chunk_done");
    out.line("local.get $source_len");
    out.line("local.get $source_index");
    out.line("i32.sub");
    out.line("local.set $take");
    out.line("local.get $take");
    out.line("local.get $chunk_size");
    out.line("i32.gt_u");
    out.open("if");
    out.line("local.get $chunk_size");
    out.line("local.set $take");
    out.close("end");
    out.line("local.get $take");
    out.line("call $__rt_alloc_assoc_entries");
    out.line("local.set $inner");
    out.line("i32.const 0");
    out.line("local.set $inner_index");
    out.open("block $array_chunk_copy_done");
    out.open("loop $array_chunk_copy_loop");
    out.line("local.get $inner_index");
    out.line("local.get $take");
    out.line("i32.ge_u");
    out.line("br_if $array_chunk_copy_done");
    out.line("local.get $inner");
    out.line("local.get $inner_index");
    out.line("call $__rt_assoc_entry");
    out.line("local.set $entry");
    out.line("local.get $entry");
    out.line("i32.const 0");
    out.line("i32.store");
    out.line("local.get $entry");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("local.get $source_index");
    out.line("local.get $inner_index");
    out.line("i32.add");
    out.line("i64.extend_i32_s");
    out.line("i64.store");
    out.line("local.get $entry");
    out.line("call $__rt_assoc_value_cell");
    out.line("local.set $dst");
    out.line("local.get $source");
    out.line("local.get $source_index");
    out.line("local.get $inner_index");
    out.line("i32.add");
    out.line(&format!("i32.const {WASM_VALUE_CELL_SIZE}"));
    out.line("i32.mul");
    out.line("i32.add");
    out.line("local.set $src");
    out.line("local.get $dst");
    out.line("local.get $src");
    out.line("call $__rt_value_copy");
    out.line("local.get $inner_index");
    out.line("i32.const 1");
    out.line("i32.add");
    out.line("local.set $inner_index");
    out.line("br $array_chunk_copy_loop");
    out.close("end");
    out.close("end");
    out.line("local.get $outer");
    out.line("local.get $chunk_index");
    out.line(&format!("i32.const {WASM_VALUE_CELL_SIZE}"));
    out.line("i32.mul");
    out.line("i32.add");
    out.line("local.get $inner");
    out.line("local.get $take");
    out.line("call $__rt_value_store_array");
    out.line("local.get $source_index");
    out.line("local.get $take");
    out.line("i32.add");
    out.line("local.set $source_index");
    out.line("local.get $chunk_index");
    out.line("i32.const 1");
    out.line("i32.add");
    out.line("local.set $chunk_index");
    out.line("br $array_chunk_loop");
    out.close("end");
    out.close("end");
    out.line("local.get $outer");
    out.close(")");
}

pub(super) fn emit_array_chunk_assoc_entries_preserve_keys(out: &mut WatEmitter) {
    out.open("(func $__rt_array_chunk_assoc_entries_preserve_keys (param $source i32) (param $source_len i32) (param $chunk_size i32) (param $outer_len i32) (result i32)");
    out.line("(local $outer i32)");
    out.line("(local $chunk_index i32)");
    out.line("(local $source_index i32)");
    out.line("(local $inner_index i32)");
    out.line("(local $take i32)");
    out.line("(local $inner i32)");
    out.line("(local $dst i32)");
    out.line("(local $src i32)");
    out.line("local.get $chunk_size");
    out.line("i32.const 0");
    out.line("i32.le_s");
    out.open("if");
    out.line("unreachable");
    out.close("end");
    out.line("local.get $outer_len");
    out.line("call $__rt_alloc_value_cells");
    out.line("local.set $outer");
    out.line("i32.const 0");
    out.line("local.set $chunk_index");
    out.line("i32.const 0");
    out.line("local.set $source_index");
    out.open("block $array_chunk_done");
    out.open("loop $array_chunk_loop");
    out.line("local.get $chunk_index");
    out.line("local.get $outer_len");
    out.line("i32.ge_u");
    out.line("br_if $array_chunk_done");
    out.line("local.get $source_len");
    out.line("local.get $source_index");
    out.line("i32.sub");
    out.line("local.set $take");
    out.line("local.get $take");
    out.line("local.get $chunk_size");
    out.line("i32.gt_u");
    out.open("if");
    out.line("local.get $chunk_size");
    out.line("local.set $take");
    out.close("end");
    out.line("local.get $take");
    out.line("call $__rt_alloc_assoc_entries");
    out.line("local.set $inner");
    out.line("i32.const 0");
    out.line("local.set $inner_index");
    out.open("block $array_chunk_copy_done");
    out.open("loop $array_chunk_copy_loop");
    out.line("local.get $inner_index");
    out.line("local.get $take");
    out.line("i32.ge_u");
    out.line("br_if $array_chunk_copy_done");
    out.line("local.get $inner");
    out.line("local.get $inner_index");
    out.line("call $__rt_assoc_entry");
    out.line("local.set $dst");
    out.line("local.get $source");
    out.line("local.get $source_index");
    out.line("local.get $inner_index");
    out.line("i32.add");
    out.line("call $__rt_assoc_entry");
    out.line("local.set $src");
    out.line("local.get $dst");
    out.line("local.get $src");
    out.line("call $__rt_assoc_copy_entry");
    out.line("local.get $inner_index");
    out.line("i32.const 1");
    out.line("i32.add");
    out.line("local.set $inner_index");
    out.line("br $array_chunk_copy_loop");
    out.close("end");
    out.close("end");
    out.line("local.get $outer");
    out.line("local.get $chunk_index");
    out.line(&format!("i32.const {WASM_VALUE_CELL_SIZE}"));
    out.line("i32.mul");
    out.line("i32.add");
    out.line("local.get $inner");
    out.line("local.get $take");
    out.line("call $__rt_value_store_array");
    out.line("local.get $source_index");
    out.line("local.get $take");
    out.line("i32.add");
    out.line("local.set $source_index");
    out.line("local.get $chunk_index");
    out.line("i32.const 1");
    out.line("i32.add");
    out.line("local.set $chunk_index");
    out.line("br $array_chunk_loop");
    out.close("end");
    out.close("end");
    out.line("local.get $outer");
    out.close(")");
}

fn emit_alloc_mixed_cell(out: &mut WatEmitter) {
    out.open("(func $__rt_alloc_mixed_cell (result i32)");
    out.line("(local $payload i32)");
    out.line(&format!("i32.const {WASM_VALUE_CELL_SIZE}"));
    out.line("call $__rt_alloc_bytes");
    out.line("local.set $payload");
    out.line("local.get $payload");
    out.line(&format!("i32.const {WASM_HEAP_KIND_MIXED}"));
    out.line("call $__rt_heap_set_kind");
    out.close(")");
}

fn emit_alloc_null_mixed_cell(out: &mut WatEmitter) {
    out.open("(func $__rt_alloc_null_mixed_cell (result i32)");
    out.line("(local $cell i32)");
    out.line("call $__rt_alloc_mixed_cell");
    out.line("local.set $cell");
    out.line("local.get $cell");
    out.line("call $__rt_value_store_null");
    out.line("local.get $cell");
    out.close(")");
}

fn emit_value_copy(out: &mut WatEmitter) {
    out.open("(func $__rt_value_copy (param $dst i32) (param $src i32)");
    out.line("local.get $dst");
    out.line("call $__rt_value_release");
    out.line("local.get $dst");
    out.line("local.get $src");
    out.line("i64.load");
    out.line("i64.store");
    out.line("local.get $dst");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("local.get $src");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("i64.load");
    out.line("i64.store");
    out.line("local.get $dst");
    out.line("call $__rt_value_retain");
    out.close(")");
}

fn emit_value_retain(out: &mut WatEmitter) {
    out.open("(func $__rt_value_retain (param $cell i32)");
    out.line("local.get $cell");
    out.line("i32.load");
    out.line("i32.const 3");
    out.line("i32.eq");
    out.line("local.get $cell");
    out.line("i32.load");
    out.line("i32.const 4");
    out.line("i32.eq");
    out.line("i32.or");
    out.line("local.get $cell");
    out.line("i32.load");
    out.line("i32.const 6");
    out.line("i32.eq");
    out.line("i32.or");
    out.open("if");
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("i32.load");
    out.line("call $__rt_incref");
    out.line("drop");
    out.close("end");
    out.close(")");
}

fn emit_value_release(out: &mut WatEmitter) {
    out.open("(func $__rt_value_release (param $cell i32)");
    out.line("local.get $cell");
    out.line("i32.load");
    out.line("i32.const 3");
    out.line("i32.eq");
    out.line("local.get $cell");
    out.line("i32.load");
    out.line("i32.const 4");
    out.line("i32.eq");
    out.line("i32.or");
    out.line("local.get $cell");
    out.line("i32.load");
    out.line("i32.const 6");
    out.line("i32.eq");
    out.line("i32.or");
    out.open("if");
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("i32.load");
    out.line("call $__rt_decref");
    out.close("end");
    out.close(")");
}

fn emit_value_copy_index_or_null(out: &mut WatEmitter) {
    out.open("(func $__rt_value_copy_index_or_null (param $dst i32) (param $base i32) (param $len i32) (param $index i32)");
    out.line("local.get $index");
    out.line("i32.const 0");
    out.line("i32.lt_s");
    out.open("if");
    out.line("local.get $dst");
    out.line("call $__rt_value_store_null");
    out.line("return");
    out.close("end");
    out.line("local.get $index");
    out.line("local.get $len");
    out.line("i32.ge_u");
    out.open("if");
    out.line("local.get $dst");
    out.line("call $__rt_value_store_null");
    out.line("return");
    out.close("end");
    out.line("local.get $dst");
    out.line("local.get $base");
    out.line("local.get $index");
    out.line("call $__rt_value_cell");
    out.line("call $__rt_value_copy");
    out.close(")");
}

fn emit_value_index_in_bounds(out: &mut WatEmitter) {
    out.open("(func $__rt_value_index_in_bounds (param $index i64) (param $len i32) (result i32)");
    out.line("local.get $index");
    out.line("i64.const 0");
    out.line("i64.lt_s");
    out.open("if (result i32)");
    out.line("i32.const 0");
    out.close("else");
    out.line("local.get $index");
    out.line("local.get $len");
    out.line("i64.extend_i32_u");
    out.line("i64.lt_u");
    out.close("end");
    out.close(")");
}

fn emit_value_array_release(out: &mut WatEmitter) {
    out.open("(func $__rt_value_array_release (param $base i32) (param $len i32)");
    out.line("(local $index i32)");
    out.line("i32.const 0");
    out.line("local.set $index");
    out.open("block $value_array_release_done");
    out.open("loop $value_array_release_loop");
    out.line("local.get $index");
    out.line("local.get $len");
    out.line("i32.ge_u");
    out.line("br_if $value_array_release_done");
    out.line("local.get $base");
    out.line("local.get $index");
    out.line(&format!("i32.const {WASM_VALUE_CELL_SIZE}"));
    out.line("i32.mul");
    out.line("i32.add");
    out.line("call $__rt_value_release");
    out.line("local.get $index");
    out.line("i32.const 1");
    out.line("i32.add");
    out.line("local.set $index");
    out.line("br $value_array_release_loop");
    out.close("end");
    out.close("end");
    out.close(")");
}

fn emit_value_store_string(out: &mut WatEmitter) {
    out.open("(func $__rt_value_store_string (param $cell i32) (param $ptr i32) (param $len i32)");
    out.line("(local $owned i32)");
    out.line("local.get $ptr");
    out.line("local.get $len");
    out.line("call $__rt_alloc_string");
    out.line("local.set $owned");
    out.line("local.get $cell");
    out.line("call $__rt_value_release");
    out.line("local.get $cell");
    out.line("i32.const 3");
    out.line("i32.store");
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("local.get $owned");
    out.line("i32.store");
    out.line("local.get $cell");
    out.line("i32.const 12");
    out.line("i32.add");
    out.line("local.get $len");
    out.line("i32.store");
    out.close(")");
}

fn emit_value_store_array(out: &mut WatEmitter) {
    out.open("(func $__rt_value_store_array (param $cell i32) (param $ptr i32) (param $len i32)");
    emit_value_store_pair_body(out, 4);
    out.line("local.get $ptr");
    out.line("call $__rt_incref");
    out.line("drop");
    out.close(")");
}

fn emit_value_store_object(out: &mut WatEmitter) {
    out.open("(func $__rt_value_store_object (param $cell i32) (param $ptr i32)");
    out.line("local.get $cell");
    out.line("call $__rt_value_release");
    out.line("local.get $cell");
    out.line("i32.const 6");
    out.line("i32.store");
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("local.get $ptr");
    out.line("i32.store");
    out.line("local.get $cell");
    out.line("i32.const 12");
    out.line("i32.add");
    out.line("i32.const 0");
    out.line("i32.store");
    out.line("local.get $ptr");
    out.line("call $__rt_incref");
    out.line("drop");
    out.close(")");
}

fn emit_value_store_pair_body(out: &mut WatEmitter, tag: i32) {
    out.line("local.get $cell");
    out.line("call $__rt_value_release");
    out.line("local.get $cell");
    out.line(&format!("i32.const {}", tag));
    out.line("i32.store");
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("local.get $ptr");
    out.line("i32.store");
    out.line("local.get $cell");
    out.line("i32.const 12");
    out.line("i32.add");
    out.line("local.get $len");
    out.line("i32.store");
}

fn emit_value_store_bool(out: &mut WatEmitter) {
    out.open("(func $__rt_value_store_bool (param $cell i32) (param $payload i32)");
    out.line("local.get $cell");
    out.line("call $__rt_value_release");
    out.line("local.get $cell");
    out.line("i32.const 2");
    out.line("i32.store");
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("local.get $payload");
    out.line("i64.extend_i32_u");
    out.line("i64.store");
    out.close(")");
}

fn emit_value_store_float(out: &mut WatEmitter) {
    out.open("(func $__rt_value_store_float (param $cell i32) (param $payload f64)");
    out.line("local.get $cell");
    out.line("call $__rt_value_release");
    out.line("local.get $cell");
    out.line("i32.const 5");
    out.line("i32.store");
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("local.get $payload");
    out.line("f64.store");
    out.close(")");
}

fn emit_value_store_int(out: &mut WatEmitter) {
    out.open("(func $__rt_value_store_int (param $cell i32) (param $payload i64)");
    out.line("local.get $cell");
    out.line("call $__rt_value_release");
    out.line("local.get $cell");
    out.line("i32.const 1");
    out.line("i32.store");
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("local.get $payload");
    out.line("i64.store");
    out.close(")");
}

fn emit_value_tag_equals(out: &mut WatEmitter) {
    out.open("(func $__rt_value_tag_equals (param $source i32) (param $index i32) (param $tag i32) (result i32)");
    out.line("local.get $source");
    out.line("local.get $index");
    out.line("i32.const 16");
    out.line("i32.mul");
    out.line("i32.add");
    out.line("i32.load");
    out.line("local.get $tag");
    out.line("i32.eq");
    out.close(")");
}

fn emit_value_tag(out: &mut WatEmitter) {
    out.open("(func $__rt_value_tag (param $source i32) (param $index i32) (result i32)");
    out.line("local.get $source");
    out.line("local.get $index");
    out.line("i32.const 16");
    out.line("i32.mul");
    out.line("i32.add");
    out.line("i32.load");
    out.close(")");
}

fn emit_value_payload_i64(out: &mut WatEmitter) {
    out.open("(func $__rt_value_payload_i64 (param $source i32) (param $index i32) (result i64)");
    out.line("local.get $source");
    out.line("local.get $index");
    out.line("i32.const 16");
    out.line("i32.mul");
    out.line("i32.add");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("i64.load");
    out.close(")");
}

fn emit_value_payload_f64(out: &mut WatEmitter) {
    out.open("(func $__rt_value_payload_f64 (param $source i32) (param $index i32) (result f64)");
    out.line("local.get $source");
    out.line("local.get $index");
    out.line("i32.const 16");
    out.line("i32.mul");
    out.line("i32.add");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("f64.load");
    out.close(")");
}

fn emit_value_payload_i32(out: &mut WatEmitter) {
    out.open("(func $__rt_value_payload_i32 (param $source i32) (param $index i32) (param $offset i32) (result i32)");
    out.line("local.get $source");
    out.line("local.get $index");
    out.line("i32.const 16");
    out.line("i32.mul");
    out.line("i32.add");
    out.line("local.get $offset");
    out.line("i32.add");
    out.line("i32.load");
    out.close(")");
}

fn emit_value_cell_payload_i64(out: &mut WatEmitter) {
    out.open("(func $__rt_value_cell_payload_i64 (param $cell i32) (result i64)");
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("i64.load");
    out.close(")");
}

fn emit_value_cell_payload_f64(out: &mut WatEmitter) {
    out.open("(func $__rt_value_cell_payload_f64 (param $cell i32) (result f64)");
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("f64.load");
    out.close(")");
}

fn emit_value_cell_payload_i32(out: &mut WatEmitter) {
    out.open("(func $__rt_value_cell_payload_i32 (param $cell i32) (param $offset i32) (result i32)");
    out.line("local.get $cell");
    out.line("local.get $offset");
    out.line("i32.add");
    out.line("i32.load");
    out.close(")");
}

fn emit_string_range_empty_cast_int(out: &mut WatEmitter) {
    out.open("(func $__rt_string_range_empty_cast_int (param $ptr i32) (param $len i32) (result i64)");
    out.line("(local $index i32)");
    out.line("(local $sign i64)");
    out.line("(local $value i64)");
    out.line("(local $byte i32)");
    out.line("(local $valid i32)");
    out.line("i64.const 1");
    out.line("local.set $sign");
    out.line("i32.const 1");
    out.line("local.set $valid");
    out.line("local.get $len");
    out.line("i32.eqz");
    out.open("if");
    out.line("i64.const 0");
    out.line("return");
    out.close("end");
    out.line("local.get $ptr");
    out.line("i32.load8_u");
    out.line("local.set $byte");
    out.line("local.get $byte");
    out.line("i32.const 45");
    out.line("i32.eq");
    out.open("if");
    out.line("i64.const -1");
    out.line("local.set $sign");
    out.line("i32.const 1");
    out.line("local.set $index");
    out.line("else");
    out.line("local.get $byte");
    out.line("i32.const 43");
    out.line("i32.eq");
    out.open("if");
    out.line("i32.const 1");
    out.line("local.set $index");
    out.close("end");
    out.close("end");
    out.line("local.get $index");
    out.line("local.get $len");
    out.line("i32.ge_u");
    out.open("if");
    out.line("i64.const 0");
    out.line("return");
    out.close("end");
    out.open("block $string_range_cast_done");
    out.open("loop $string_range_cast_loop");
    out.line("local.get $index");
    out.line("local.get $len");
    out.line("i32.ge_u");
    out.line("br_if $string_range_cast_done");
    out.line("local.get $ptr");
    out.line("local.get $index");
    out.line("i32.add");
    out.line("i32.load8_u");
    out.line("local.set $byte");
    out.line("local.get $byte");
    out.line("i32.const 48");
    out.line("i32.lt_u");
    out.open("if");
    out.line("i32.const 0");
    out.line("local.set $valid");
    out.line("br $string_range_cast_done");
    out.close("end");
    out.line("local.get $byte");
    out.line("i32.const 57");
    out.line("i32.gt_u");
    out.open("if");
    out.line("i32.const 0");
    out.line("local.set $valid");
    out.line("br $string_range_cast_done");
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
    out.line("br $string_range_cast_loop");
    out.close("end");
    out.close("end");
    out.line("local.get $valid");
    out.open("if (result i64)");
    out.line("local.get $value");
    out.line("local.get $sign");
    out.line("i64.mul");
    out.line("else");
    out.line("i64.const 0");
    out.close("end");
    out.close(")");
}

fn emit_value_store_null(out: &mut WatEmitter) {
    out.open("(func $__rt_value_store_null (param $cell i32)");
    out.line("local.get $cell");
    out.line("call $__rt_value_release");
    out.line("local.get $cell");
    out.line("i32.const 0");
    out.line("i32.store");
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("i64.const 0");
    out.line("i64.store");
    out.close(")");
}

fn emit_value_cell(out: &mut WatEmitter) {
    out.open("(func $__rt_value_cell (param $base i32) (param $index i32) (result i32)");
    out.line("local.get $base");
    out.line("local.get $index");
    out.line("i32.const 16");
    out.line("i32.mul");
    out.line("i32.add");
    out.close(")");
}

fn emit_output_value_cell(out: &mut WatEmitter) {
    out.open("(func $__rt_output_value_cell (param $cell i32) (param $array_marker_ptr i32)");
    out.line("local.get $cell");
    out.line("i32.load");
    out.line("i32.const 1");
    out.line("i32.eq");
    out.open("if");
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("i64.load");
    out.line("call $host_write_int");
    out.line("else");
    out.line("local.get $cell");
    out.line("i32.load");
    out.line("i32.const 5");
    out.line("i32.eq");
    out.open("if");
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("f64.load");
    out.line("call $host_write_float");
    out.line("else");
    out.line("local.get $cell");
    out.line("i32.load");
    out.line("i32.const 2");
    out.line("i32.eq");
    out.open("if");
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("i64.load");
    out.line("i64.eqz");
    out.line("i32.eqz");
    out.open("if");
    out.line("i64.const 1");
    out.line("call $host_write_int");
    out.close("end");
    out.line("else");
    out.line("local.get $cell");
    out.line("i32.load");
    out.line("i32.const 3");
    out.line("i32.eq");
    out.open("if");
    out.line("local.get $cell");
    out.line("i32.const 8");
    out.line("i32.add");
    out.line("i32.load");
    out.line("local.get $cell");
    out.line("i32.const 12");
    out.line("i32.add");
    out.line("i32.load");
    out.line("call $host_write");
    out.line("else");
    out.line("local.get $cell");
    out.line("i32.load");
    out.line("i32.const 4");
    out.line("i32.eq");
    out.open("if");
    out.line("local.get $array_marker_ptr");
    out.line("i32.const 5");
    out.line("call $host_write");
    out.close("end");
    out.close("end");
    out.close("end");
    out.close("end");
    out.close("end");
    out.close(")");
}
