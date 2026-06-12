//! Purpose:
//! Provides shared wasm32-web helpers for array_filter result setup, predicates, and entry storage.
//! Keeps low-level filter mechanics out of the filter assignment emitters.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_map_filter` and array_filter assignment lowering.
//!
//! Key details:
//! - Preserves associative result metadata, callback truthiness conversion, and value-cell copy behavior.

use super::*;
pub(super) fn emit_array_filter_result_prelude(name: &str, capacity: usize, module: &mut WasmModule) -> String {
    let out_index = module.next_label("array_filter_out_index");
    module.declare_i32_local(out_index.trim_start_matches('$').to_string());
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.clear_array_length(name);
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", capacity));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    out_index
}

pub(super) fn emit_array_filter_runtime_result_prelude(
    name: &str,
    source: &str,
    module: &mut WasmModule,
) -> String {
    let out_index = module.next_label("array_filter_out_index");
    module.declare_i32_local(out_index.trim_start_matches('$').to_string());
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.clear_array_length(name);
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    module.set_array_value_cell_kinds(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    out_index
}

pub(super) fn emit_array_filter_predicate_result_is_true(callback: &str, module: &mut WasmModule) {
    if callback.eq_ignore_ascii_case("strlen") {
        module.body().line("i64.const 0");
        module.body().line("i64.ne");
        return;
    }
    match module.function_return_kind(callback) {
        Some(ValueKind::Bool) => {}
        Some(ValueKind::Int) => {
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
        }
        _ => unreachable!("array_filter predicate shape was validated"),
    }
}

pub(super) fn emit_array_filter_int_predicate(callback: &str, module: &mut WasmModule) {
    if callback.eq_ignore_ascii_case("is_int") || callback.eq_ignore_ascii_case("is_numeric") {
        module.body().line("drop");
        module.body().line("i32.const 1");
        return;
    }
    if callback.eq_ignore_ascii_case("strlen") {
        module.body().line("drop");
        module.body().line("i32.const 1");
        return;
    }
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    emit_array_filter_predicate_result_is_true(callback, module);
}

pub(super) fn emit_array_filter_string_predicate(callback: &str, module: &mut WasmModule) {
    if callback.eq_ignore_ascii_case("is_string") {
        module.body().line("drop");
        module.body().line("drop");
        module.body().line("i32.const 1");
        return;
    }
    if callback.eq_ignore_ascii_case("strlen") {
        let len = module.next_label("array_filter_strlen_len");
        module.declare_i32_local(len.trim_start_matches('$').to_string());
        module.body().line(&format!("local.set {}", len));
        module.body().line("drop");
        module.body().line(&format!("local.get {}", len));
        module.body().line("i64.extend_i32_u");
        emit_array_filter_predicate_result_is_true(callback, module);
        return;
    }
    if callback.eq_ignore_ascii_case("is_numeric") {
        emit_stack_string_is_numeric(module);
        return;
    }
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    emit_array_filter_predicate_result_is_true(callback, module);
}

pub(super) fn emit_array_filter_value_cell_arg(
    source: &str,
    index: usize,
    kind: ValueCellKind,
    module: &mut WasmModule,
) {
    match kind {
        ValueCellKind::Int => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("call $__rt_value_payload_i64");
        }
        ValueCellKind::Str => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("i32.const 8");
            module.body().line("call $__rt_value_payload_i32");
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("i32.const 12");
            module.body().line("call $__rt_value_payload_i32");
        }
        ValueCellKind::Bool => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("call $__rt_value_payload_i64");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
        }
        ValueCellKind::Float => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("call $__rt_value_payload_f64");
        }
        ValueCellKind::Null | ValueCellKind::Array => {
            unreachable!("array_filter USE_BOTH only accepts scalar callback value cells")
        }
    }
}

pub(super) fn emit_array_filter_value_cell_arg_dynamic(
    source: &str,
    index: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) {
    match kind {
        ValueCellKind::Int => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get {}", index));
            module.body().line("call $__rt_value_payload_i64");
        }
        ValueCellKind::Str => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get {}", index));
            module.body().line("i32.const 8");
            module.body().line("call $__rt_value_payload_i32");
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get {}", index));
            module.body().line("i32.const 12");
            module.body().line("call $__rt_value_payload_i32");
        }
        ValueCellKind::Bool => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get {}", index));
            module.body().line("call $__rt_value_payload_i64");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
        }
        ValueCellKind::Float => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get {}", index));
            module.body().line("call $__rt_value_payload_f64");
        }
        ValueCellKind::Null | ValueCellKind::Array => {
            unreachable!("array_filter USE_BOTH only accepts scalar callback value cells")
        }
    }
}

pub(super) fn emit_array_filter_scalar_predicate_arg_from_expr(
    item: &Expr,
    callback: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match kind {
        ValueCellKind::Bool => match emit_expr(item, module)? {
            ValueKind::Bool => {
                emit_array_filter_bool_predicate(callback, module);
                Ok(())
            }
            _ => Err(CompileError::new(
                item.span,
                "wasm32-web array_filter() bool predicate expected boolean values",
            )),
        },
        ValueCellKind::Float => {
            require_float(item, module)?;
            emit_array_filter_float_predicate(callback, module);
            Ok(())
        }
        ValueCellKind::Str => {
            emit_string_value_to_stack(item, module)?;
            emit_stack_string_numeric_float_arg("array_filter_literal_string_float", module);
            emit_array_filter_float_predicate(callback, module);
            Ok(())
        }
        _ => Err(CompileError::new(
            item.span,
            "wasm32-web array_filter() scalar predicates currently support bool, float, and numeric string values",
        )),
    }
}

pub(super) fn emit_array_filter_scalar_predicate_arg_from_cell(
    source: &str,
    index: usize,
    callback: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) {
    match kind {
        ValueCellKind::Bool => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("call $__rt_value_payload_i64");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
            emit_array_filter_bool_predicate(callback, module);
        }
        ValueCellKind::Float => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("call $__rt_value_payload_f64");
            emit_array_filter_float_predicate(callback, module);
        }
        ValueCellKind::Str => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("i32.const 8");
            module.body().line("call $__rt_value_payload_i32");
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("i32.const 12");
            module.body().line("call $__rt_value_payload_i32");
            emit_stack_string_numeric_float_arg("array_filter_value_string_float", module);
            emit_array_filter_float_predicate(callback, module);
        }
        _ => unreachable!("array_filter scalar predicate only handles bool, float, and numeric string cells"),
    }
}

pub(super) fn emit_array_filter_scalar_predicate_arg_from_cell_dynamic(
    source: &str,
    index: &str,
    callback: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) {
    match kind {
        ValueCellKind::Bool => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get {}", index));
            module.body().line("call $__rt_value_payload_i64");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
            emit_array_filter_bool_predicate(callback, module);
        }
        ValueCellKind::Float => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get {}", index));
            module.body().line("call $__rt_value_payload_f64");
            emit_array_filter_float_predicate(callback, module);
        }
        ValueCellKind::Str => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get {}", index));
            module.body().line("i32.const 8");
            module.body().line("call $__rt_value_payload_i32");
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get {}", index));
            module.body().line("i32.const 12");
            module.body().line("call $__rt_value_payload_i32");
            emit_stack_string_numeric_float_arg("array_filter_runtime_value_string_float", module);
            emit_array_filter_float_predicate(callback, module);
        }
        _ => unreachable!("array_filter dynamic scalar predicate only handles bool, float, and numeric string cells"),
    }
}

pub(super) fn emit_array_filter_scalar_predicate_arg_from_pointer(
    value_cell: &str,
    callback: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) {
    match kind {
        ValueCellKind::Bool => {
            module.body().line(&format!("local.get {}", value_cell));
            module.body().line("call $__rt_value_cell_payload_i64");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
            emit_array_filter_bool_predicate(callback, module);
        }
        ValueCellKind::Float => {
            module.body().line(&format!("local.get {}", value_cell));
            module.body().line("call $__rt_value_cell_payload_f64");
            emit_array_filter_float_predicate(callback, module);
        }
        ValueCellKind::Str => {
            module.body().line(&format!("local.get {}", value_cell));
            module.body().line("i32.const 8");
            module.body().line("call $__rt_value_cell_payload_i32");
            module.body().line(&format!("local.get {}", value_cell));
            module.body().line("i32.const 12");
            module.body().line("call $__rt_value_cell_payload_i32");
            emit_stack_string_numeric_float_arg("array_filter_assoc_string_float", module);
            emit_array_filter_float_predicate(callback, module);
        }
        _ => unreachable!("array_filter pointer scalar predicate only handles bool, float, and numeric string cells"),
    }
}

pub(super) fn emit_array_filter_bool_predicate(callback: &str, module: &mut WasmModule) {
    if callback.eq_ignore_ascii_case("is_bool") {
        module.body().line("drop");
        module.body().line("i32.const 1");
        return;
    }
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    emit_array_filter_predicate_result_is_true(callback, module);
}

pub(super) fn emit_array_filter_float_predicate(callback: &str, module: &mut WasmModule) {
    if callback.eq_ignore_ascii_case("is_float") || callback.eq_ignore_ascii_case("is_numeric") {
        module.body().line("drop");
        module.body().line("i32.const 1");
        return;
    }
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    emit_array_filter_predicate_result_is_true(callback, module);
}

pub(super) fn emit_array_filter_store_int_entry(
    name: &str,
    out_index: &str,
    source_index: usize,
    value: &str,
    module: &mut WasmModule,
) {
    let target_entry = module.next_label("array_filter_target_entry");
    module.declare_i32_local(target_entry.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("i64.const {}", source_index));
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.store");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("i32.const 24");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", value));
    module.body().line("i64.store");
    emit_array_filter_increment_len(name, out_index, module);
}

pub(super) fn emit_array_filter_store_int_entry_dynamic_key(
    name: &str,
    out_index: &str,
    source_index: &str,
    value: &str,
    module: &mut WasmModule,
) {
    let target_entry = module.next_label("array_filter_target_entry");
    module.declare_i32_local(target_entry.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i64.extend_i32_u");
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.store");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("i32.const 24");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", value));
    module.body().line("i64.store");
    emit_array_filter_increment_len(name, out_index, module);
}

pub(super) fn emit_array_filter_store_string_entry(
    name: &str,
    out_index: &str,
    source_index: usize,
    ptr: &str,
    len: &str,
    module: &mut WasmModule,
) {
    let target_entry = module.next_label("array_filter_target_entry");
    module.declare_i32_local(target_entry.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("i64.const {}", source_index));
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", len));
    module.body().line("call $__rt_value_store_string");
    emit_array_filter_increment_len(name, out_index, module);
}

pub(super) fn emit_array_filter_store_value_expr_entry(
    name: &str,
    out_index: &str,
    source_index: usize,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let target_entry = module.next_label("array_filter_target_entry");
    let target_cell = module.next_label("array_filter_target_cell");
    module.declare_i32_local(target_entry.trim_start_matches('$').to_string());
    module.declare_i32_local(target_cell.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("i64.const {}", source_index));
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", target_cell));
    emit_store_value_cell(&target_cell, value, module)?;
    emit_array_filter_increment_len(name, out_index, module);
    Ok(())
}

pub(super) fn emit_array_filter_copy_value_entry(
    name: &str,
    source: &str,
    out_index: &str,
    source_index: usize,
    module: &mut WasmModule,
) {
    let target_entry = module.next_label("array_filter_target_entry");
    let target_cell = module.next_label("array_filter_target_cell");
    module.declare_i32_local(target_entry.trim_start_matches('$').to_string());
    module.declare_i32_local(target_cell.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("i64.const {}", source_index));
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", target_cell));
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", source_index * WASM_VALUE_CELL_SIZE));
    module.body().line("i32.add");
    module.body().line("call $__rt_value_copy");
    emit_array_filter_increment_len(name, out_index, module);
}

pub(super) fn emit_array_filter_copy_value_entry_dynamic_key(
    name: &str,
    source: &str,
    out_index: &str,
    source_index: &str,
    module: &mut WasmModule,
) {
    let target_entry = module.next_label("array_filter_target_entry");
    let target_cell = module.next_label("array_filter_target_cell");
    let source_cell = module.next_label("array_filter_source_cell");
    module.declare_i32_local(target_entry.trim_start_matches('$').to_string());
    module.declare_i32_local(target_cell.trim_start_matches('$').to_string());
    module.declare_i32_local(source_cell.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i64.extend_i32_u");
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", target_cell));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", source_index));
    module
        .body()
        .line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_cell));
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get {}", source_cell));
    module.body().line("call $__rt_value_copy");
    emit_array_filter_increment_len(name, out_index, module);
}

pub(super) fn emit_array_filter_increment_len(name: &str, out_index: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line(&format!("local.set ${}_len", name));
}
