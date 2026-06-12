//! Purpose:
//! Emits wasm32-web helpers for reading and storing string-oriented value cells.
//! Keeps low-level cell address math separate from value-string transform loops.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets::value_string`
//! - Sibling array set helpers through the parent re-export.
//!
//! Key details:
//! - Helpers preserve the boxed value-cell payload shape and associative entry layout.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_load_value_cell_string_part(
    source_ptr: &str,
    index: &str,
    offset: usize,
    target: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("i32.const {}", offset));
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", target));
}

pub(in crate::codegen::wasm::expr) fn emit_store_value_cell_assoc_entry(
    name: &str,
    target_entry: &str,
    out_index: &str,
    source_ptr: &str,
    source_index: &str,
    module: &mut WasmModule,
) {
    let source_cell = module.next_label("value_set_source_cell");
    let target_cell = module.next_label("value_set_target_cell");
    module.declare_i32_local(source_cell.trim_start_matches('$').to_string());
    module.declare_i32_local(target_cell.trim_start_matches('$').to_string());
    emit_assoc_entry_address(&format!("${}_ptr", name), out_index, target_entry, module);
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i64.extend_i32_u");
    module.body().line("call $__rt_assoc_store_int_key");
    emit_assoc_value_cell_address(target_entry, &target_cell, module);
    emit_value_cell_address(source_ptr, source_index, &source_cell, module);
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get {}", source_cell));
    module.body().line("call $__rt_value_copy");
}
