//! Purpose:
//! Emits pointer-level PHP loose comparison and truthiness helpers for wasm32-web value cells.
//! Keeps dynamic mixed-value coercion paths separate from array set-membership orchestration.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets::value_compare`.
//! - Sibling array/search emitters through the array transform helper re-export.
//!
//! Key details:
//! - The helpers preserve PHP bool/null/numeric/string-representation ordering for mixed cells.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_value_cell_ptrs_php_loose_equal_between(
    left_cell: &str,
    right_cell: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    let left_tag = module.next_label("value_ptr_loose_left_tag");
    let right_tag = module.next_label("value_ptr_loose_right_tag");
    let left_bool = module.next_label("value_ptr_loose_left_bool");
    let right_bool = module.next_label("value_ptr_loose_right_bool");
    let left_number = module.next_label("value_ptr_loose_left_number");
    let right_number = module.next_label("value_ptr_loose_right_number");
    for local in [&left_tag, &right_tag, &left_bool, &right_bool] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left_number, &right_number] {
        module.declare_f64_local(local.trim_start_matches('$').to_string());
    }
    emit_load_value_cell_ptr_tag(left_cell, &left_tag, module);
    emit_load_value_cell_ptr_tag(right_cell, &right_tag, module);
    module.body().line(&format!("local.get {}", left_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", right_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().open("if");
    emit_value_cell_pointer_dynamic_truthiness(left_cell, module);
    module.body().line(&format!("local.set {}", left_bool));
    emit_value_cell_pointer_dynamic_truthiness(right_cell, module);
    module.body().line(&format!("local.set {}", right_bool));
    module.body().line(&format!("local.get {}", left_bool));
    module.body().line(&format!("local.get {}", right_bool));
    module.body().line("i32.eq");
    module.body().line(&format!("local.set {}", matched));
    module.body().line("else");
    module.body().line(&format!("local.get {}", left_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", right_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().open("if");
    module.body().line(&format!("local.get {}", left_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_value_cell_ptr_php_null_loose_equal_to(right_cell, matched, module);
    module.body().line("else");
    emit_value_cell_ptr_php_null_loose_equal_to(left_cell, matched, module);
    module.body().close("end");
    module.body().line("else");
    emit_value_cell_ptr_tag_is_numeric(&left_tag, module);
    emit_value_cell_ptr_tag_is_numeric(&right_tag, module);
    module.body().line("i32.and");
    module.body().open("if");
    emit_load_value_cell_ptr_numeric_value(left_cell, &left_tag, &left_number, module);
    emit_load_value_cell_ptr_numeric_value(right_cell, &right_tag, &right_number, module);
    module.body().line(&format!("local.get {}", left_number));
    module.body().line(&format!("local.get {}", right_number));
    module.body().line("f64.eq");
    module.body().line(&format!("local.set {}", matched));
    module.body().line("else");
    emit_value_cell_ptrs_same_string_repr_between(left_cell, right_cell, matched, module);
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

pub(in crate::codegen::wasm::expr) fn emit_value_cell_ptr_tag_is_numeric(tag: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().line("i32.or");
}

pub(in crate::codegen::wasm::expr) fn emit_load_value_cell_ptr_numeric_value(
    cell: &str,
    tag: &str,
    target: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("call $__rt_value_cell_payload_i64");
    module.body().line("f64.convert_i64_s");
    module.body().line(&format!("local.set {}", target));
    module.body().line("else");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("call $__rt_value_cell_payload_f64");
    module.body().line(&format!("local.set {}", target));
    module.body().close("end");
}

pub(in crate::codegen::wasm::expr) fn emit_value_cell_ptr_php_null_loose_equal_to(
    cell: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    let tag = module.next_label("value_ptr_null_cmp_tag");
    module.declare_i32_local(tag.trim_start_matches('$').to_string());
    emit_load_value_cell_ptr_tag(cell, &tag, module);
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.eq");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("call $__rt_value_cell_payload_i64");
    module.body().line("i64.eqz");
    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", matched));
    module.body().close("end");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("call $__rt_value_cell_payload_f64");
    module.body().line("f64.const 0");
    module.body().line("f64.eq");
    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", matched));
    module.body().close("end");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_cell_payload_i32");
    module.body().line("i32.eqz");
    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", matched));
    module.body().close("end");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_cell_payload_i32");
    module.body().line("i32.eqz");
    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", matched));
    module.body().close("end");
}

pub(in crate::codegen::wasm::expr) fn emit_value_cell_pointer_dynamic_truthiness(
    cell: &str,
    module: &mut WasmModule,
) {
    let tag = module.next_label("value_ptr_truthy_tag");
    module.declare_i32_local(tag.trim_start_matches('$').to_string());
    emit_load_value_cell_ptr_tag(cell, &tag, module);
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("call $__rt_value_cell_payload_i64");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("call $__rt_value_cell_payload_f64");
    module.body().line("f64.const 0");
    module.body().line("f64.ne");
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if (result i32)");
    emit_value_cell_pointer_truthiness(cell, ValueCellKind::Str, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if (result i32)");
    emit_value_cell_pointer_truthiness(cell, ValueCellKind::Array, module);
    module.body().line("else");
    module.body().line("i32.const 0");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}
