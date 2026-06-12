//! Purpose:
//! Owns small wasm32-web mixed/value-cell helper emitters shared by expression
//! lowering, array helpers, and statement lowering.
//!
//! Called from:
//! - `crate::codegen::wasm::expr`
//! - sibling modules under `crate::codegen::wasm::expr`
//!
//! Key details:
//! - Helpers preserve the boxed Mixed/value-cell runtime contract.
//! - Callers remain responsible for ownership decisions around when to copy or release.

use super::WasmModule;

pub(crate) fn emit_alloc_mixed_cell(target: &str, module: &mut WasmModule) {
    module.body().line("call $__rt_alloc_mixed_cell");
    module.body().line(&format!("local.set ${}", target));
}

pub(crate) fn emit_null_mixed_value_to_stack(module: &mut WasmModule) {
    module.body().line("call $__rt_alloc_null_mixed_cell");
}

pub(crate) fn emit_copy_value_cell_from_addr_to_addr(
    target_cell: &str,
    source_cell: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get {}", source_cell));
    module.body().line("call $__rt_value_copy");
}

pub(crate) fn emit_release_value_cell(cell: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line("call $__rt_value_release");
}

pub(crate) fn emit_store_null_value_cell(cell: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line("call $__rt_value_store_null");
}
