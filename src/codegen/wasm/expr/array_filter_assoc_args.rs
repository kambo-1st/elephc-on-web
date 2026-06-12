//! Purpose:
//! Emits associative key/value callback arguments for wasm32-web array_filter modes.
//! Keeps USE_KEY and USE_BOTH argument materialization separate from filter loops.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_map_filter`
//!
//! Key details:
//! - Preserves PHP int/string key payload shapes and releases temporary mixed key cells.

use super::*;

pub(super) fn emit_array_filter_assoc_both_callback_args(
    value_cell: &str,
    source_entry: &str,
    callback_shape: ArrayFilterUseBothCallback,
    source_value_kind: ValueCellKind,
    module: &mut WasmModule,
) {
    if callback_shape.value_kind.is_none() {
        module.body().line(&format!("local.get {}", value_cell));
    } else if source_value_kind == ValueCellKind::Str
        && callback_shape.value_kind == Some(ValueCellKind::Float)
    {
        emit_array_filter_value_cell_pointer_arg(value_cell, ValueCellKind::Str, module);
        emit_stack_string_numeric_float_arg("array_filter_assoc_both_string_float", module);
    } else {
        let value_kind = callback_shape
            .value_kind
            .expect("array_filter USE_BOTH typed callback kind was checked");
        emit_array_filter_value_cell_pointer_arg(value_cell, value_kind, module);
    }
    emit_array_filter_assoc_key_arg(source_entry, callback_shape.key_shape, module);
}

pub(super) fn emit_array_filter_value_cell_pointer_arg(
    value_cell: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) {
    match kind {
        ValueCellKind::Int => {
            module.body().line(&format!("local.get {}", value_cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.load");
        }
        ValueCellKind::Str => {
            module.body().line(&format!("local.get {}", value_cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.get {}", value_cell));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
        }
        ValueCellKind::Bool => {
            module.body().line(&format!("local.get {}", value_cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
        }
        ValueCellKind::Float => {
            module.body().line(&format!("local.get {}", value_cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("f64.load");
        }
        ValueCellKind::Null | ValueCellKind::Array => {
            unreachable!("array_filter USE_BOTH only accepts scalar callback value cells")
        }
    }
}

pub(super) fn emit_array_filter_assoc_key_arg(
    source_entry: &str,
    shape: ArrayFilterCallbackShape,
    module: &mut WasmModule,
) {
    match shape {
        ArrayFilterCallbackShape::Int | ArrayFilterCallbackShape::Numeric => {
            module.body().line(&format!("local.get {}", source_entry));
            module.body().line("call $__rt_assoc_key_payload_i64");
        }
        ArrayFilterCallbackShape::Str => {
            module.body().line(&format!("local.get {}", source_entry));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.get {}", source_entry));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
        }
        _ => unreachable!("array_filter USE_BOTH only accepts int/string key callbacks"),
    }
}

pub(super) fn emit_array_filter_assoc_key_predicate(
    source_entry: &str,
    callback: &str,
    shape: ArrayFilterCallbackShape,
    module: &mut WasmModule,
) {
    match shape {
        ArrayFilterCallbackShape::Int | ArrayFilterCallbackShape::Numeric => {
            module.body().line(&format!("local.get {}", source_entry));
            module.body().line("call $__rt_assoc_key_payload_i64");
            emit_array_filter_int_predicate(callback, module);
        }
        ArrayFilterCallbackShape::Str => {
            module.body().line(&format!("local.get {}", source_entry));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.get {}", source_entry));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            emit_array_filter_string_predicate(callback, module);
        }
        ArrayFilterCallbackShape::Mixed => {
            emit_array_filter_mixed_assoc_key_predicate(source_entry, callback, module);
        }
        _ => unreachable!("array_filter USE_KEY only accepts int/numeric/string/mixed key callbacks"),
    }
}

pub(super) fn emit_array_filter_mixed_assoc_key_predicate(
    source_entry: &str,
    callback: &str,
    module: &mut WasmModule,
) {
    let key_cell = module.next_label("array_filter_mixed_key_cell");
    let result = module.next_label("array_filter_mixed_key_result");
    for local in [&key_cell, &result] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 1");
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", key_cell));
    emit_store_assoc_key_as_mixed_cell(source_entry, &key_cell, module);
    module.body().line(&format!("local.get {}", key_cell));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    emit_array_filter_predicate_result_is_true(callback, module);
    module.body().line(&format!("local.set {}", result));
    module.body().line(&format!("local.get {}", key_cell));
    module.body().line("call $__rt_value_release");
    module.body().line(&format!("local.get {}", result));
}

pub(super) fn emit_store_assoc_key_as_mixed_cell(
    source_entry: &str,
    key_cell: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", key_cell));
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("call $__rt_value_store_int");
    module.body().line("else");
    module.body().line(&format!("local.get {}", key_cell));
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line("call $__rt_value_store_string");
    module.body().close("end");
}
