//! Purpose:
//! Emits wasm32-web runtime helper functions used by generated WAT.
//! Keeps PHP value semantics centralized instead of duplicating inline lowering.
//!
//! Called from:
//! - `crate::codegen::wasm::module::WasmModule::finish()`.
//!
//! Key details:
//! - Helpers operate on the current wasm value-cell layout in linear memory.
//! - Helper names use the native-style `__rt_*` prefix but stay WASM-only.

use super::emitter::WatEmitter;

mod assoc;
mod heap;
mod mixed;
mod value_cells;

const WASM_HEAP_HEADER_SIZE: i32 = 16;
const WASM_HEAP_SIZE_OFFSET_FROM_PAYLOAD: i32 = 16;
const WASM_HEAP_REFCOUNT_OFFSET: i32 = 4;
const WASM_HEAP_REFCOUNT_OFFSET_FROM_PAYLOAD: i32 = 12;
const WASM_HEAP_KIND_OFFSET_FROM_PAYLOAD: i32 = 8;
const WASM_HEAP_KIND_RAW: i32 = 0;
const WASM_HEAP_KIND_STRING: i32 = 1;
const WASM_HEAP_KIND_INDEXED_ARRAY: i32 = 2;
const WASM_HEAP_KIND_ASSOC_ARRAY: i32 = 3;
const WASM_HEAP_KIND_MIXED: i32 = 5;
const WASM_VALUE_CELL_SIZE: i32 = 16;

pub(super) fn emit(out: &mut WatEmitter) {
    heap::emit(out);
    value_cells::emit_array_chunk_value_cells(out);
    value_cells::emit_array_chunk_value_cells_preserve_int_keys(out);
    value_cells::emit_array_chunk_assoc_entries_preserve_keys(out);
    assoc::emit_alloc_assoc_entries(out);
    value_cells::emit(out);
    mixed::emit(out);
    assoc::emit(out);
}
