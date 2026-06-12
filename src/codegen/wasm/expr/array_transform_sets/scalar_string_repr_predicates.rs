//! Purpose:
//! Emits wasm32-web predicates for scalar cells compared by PHP string representation.
//! Keeps null/bool/int string-repr tests separate from larger comparison emitters.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets::scalar_string_repr`
//! - `crate::codegen::wasm::expr::array_transform_sets::value_compare`
//!
//! Key details:
//! - The helpers leave their boolean result on the wasm stack for caller composition.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_value_scalar_cell_is_empty_string_repr(
    tag: &str,
    payload: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", payload));
    module.body().line("i64.eqz");
    module.body().line("i32.and");
    module.body().line("i32.or");
}

pub(in crate::codegen::wasm::expr) fn emit_value_scalar_cell_is_one_string_repr(
    tag: &str,
    payload: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", payload));
    module.body().line("i64.const 1");
    module.body().line("i64.eq");
    module.body().line("i32.and");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", payload));
    module.body().line("i64.const 1");
    module.body().line("i64.eq");
    module.body().line("i32.and");
    module.body().line("i32.or");
}
