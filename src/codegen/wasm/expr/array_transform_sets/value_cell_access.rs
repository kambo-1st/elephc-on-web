//! Purpose:
//! Emits shared wasm32-web value-cell load and tag-test snippets for array transform helpers.
//! Keeps raw value-cell layout access separate from higher-level comparison and assoc logic.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets` sibling transform modules.
//!
//! Key details:
//! - Helpers preserve the boxed value-cell layout and route indexed loads through `__rt_*` helpers.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_load_value_cell_tag(
    source_ptr: &str,
    index: &str,
    target: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_tag");
    module.body().line(&format!("local.set {}", target));
}

pub(in crate::codegen::wasm::expr) fn emit_load_value_cell_ptr_tag(
    cell: &str,
    target: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", target));
}

pub(in crate::codegen::wasm::expr) fn emit_load_value_cell_ptr_payload(
    cell: &str,
    target: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line("call $__rt_value_cell_payload_i64");
    module.body().line(&format!("local.set {}", target));
}

pub(in crate::codegen::wasm::expr) fn emit_load_value_cell_ptr_string_part(
    cell: &str,
    offset: i32,
    target: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("i32.const {}", offset));
    module.body().line("call $__rt_value_cell_payload_i32");
    module.body().line(&format!("local.set {}", target));
}

pub(in crate::codegen::wasm::expr) fn emit_load_value_cell_payload(
    source_ptr: &str,
    index: &str,
    target: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_i64");
    module.body().line(&format!("local.set {}", target));
}

pub(in crate::codegen::wasm::expr) fn emit_value_cell_tag_equals(
    source_ptr: &str,
    index: &str,
    tag: i32,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("i32.const {}", tag));
    module.body().line("call $__rt_value_tag_equals");
}

pub(in crate::codegen::wasm::expr) fn emit_load_value_cell_i64_payload(
    source_ptr: &str,
    index: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_i64");
}
