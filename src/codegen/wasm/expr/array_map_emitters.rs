//! Purpose:
//! Emits wasm32-web array_map lowering loops and result materialization helpers.
//! Keeps map-specific callback invocation paths separate from array_filter emitters.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` and array_map assignment lowering.
//!
//! Key details:
//! - Preserves value-cell metadata, callback return shape, and string-length coercion behavior.

use super::*;
pub(super) fn emit_array_map_literal_ints_assign(
    name: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.set_array_length(name, items.len());
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line(&format!("local.set ${}_len", name));
    for (index, item) in items.iter().enumerate() {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        require_int(item, module)?;
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("i64.store");
    }
    Ok(())
}

pub(super) fn emit_array_map_literal_bools_as_ints_assign(
    name: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.set_array_length(name, items.len());
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line(&format!("local.set ${}_len", name));
    for (index, item) in items.iter().enumerate() {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        emit_condition(item, module)?;
        module.body().line("i64.extend_i32_u");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("i64.store");
    }
    Ok(())
}

pub(super) fn emit_array_map_literal_floats_as_ints_assign(
    name: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.set_array_length(name, items.len());
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line(&format!("local.set ${}_len", name));
    for (index, item) in items.iter().enumerate() {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        require_float(item, module)?;
        module.body().line("i64.trunc_f64_s");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("i64.store");
    }
    Ok(())
}

pub(super) fn emit_array_map_literal_numeric_strings_as_ints_assign(
    name: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.set_array_length(name, items.len());
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line(&format!("local.set ${}_len", name));
    for (index, item) in items.iter().enumerate() {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        emit_string_value_to_stack(item, module)?;
        emit_stack_string_numeric_int_arg("array_map_string_int", module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("i64.store");
    }
    Ok(())
}

pub(super) fn emit_array_map_literal_strings_assign(
    name: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.set_array_length(name, items.len());
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Str; items.len()]));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line(&format!("local.set ${}_len", name));
    for (index, item) in items.iter().enumerate() {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        emit_string_value_to_stack(item, module)?;
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("call $__rt_value_store_string");
    }
    Ok(())
}

pub(super) fn emit_array_map_int_bool_result(callback: &str, module: &mut WasmModule) {
    if callback.eq_ignore_ascii_case("is_int") || callback.eq_ignore_ascii_case("is_numeric") {
        module.body().line("drop");
        module.body().line("i32.const 1");
        return;
    }
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
}

pub(super) fn emit_array_map_string_bool_result(callback: &str, module: &mut WasmModule) {
    if callback.eq_ignore_ascii_case("is_string") {
        module.body().line("drop");
        module.body().line("drop");
        module.body().line("i32.const 1");
        return;
    }
    if callback.eq_ignore_ascii_case("is_numeric") {
        emit_stack_string_is_numeric(module);
        return;
    }
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
}

pub(super) fn emit_stack_string_is_numeric(module: &mut WasmModule) {
    let string = module
        .next_label("callback_numeric_string")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(format!("{}_ptr", string));
    module.declare_i32_local(format!("{}_len", string));
    module.body().line(&format!("local.set ${}_len", string));
    module.body().line(&format!("local.set ${}_ptr", string));
    emit_runtime_is_numeric(&string, module);
}

pub(super) fn emit_stack_string_numeric_int_arg(prefix: &str, module: &mut WasmModule) {
    emit_stack_string_numeric_float_arg(prefix, module);
    module.body().line("i64.trunc_f64_s");
}

pub(super) fn emit_stack_string_numeric_float_arg(prefix: &str, module: &mut WasmModule) {
    let string = module.next_label(prefix).trim_start_matches('$').to_string();
    let is_numeric = module.next_label(prefix);
    let number = module.next_label(prefix);
    module.declare_i32_local(format!("{}_ptr", string));
    module.declare_i32_local(format!("{}_len", string));
    module.declare_i32_local(is_numeric.trim_start_matches('$').to_string());
    module.declare_f64_local(number.trim_start_matches('$').to_string());
    module.body().line(&format!("local.set ${}_len", string));
    module.body().line(&format!("local.set ${}_ptr", string));
    emit_runtime_numeric_string_value(&string, &is_numeric, &number, module);
    module.body().line(&format!("local.get {}", is_numeric));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", number));
}

pub(super) fn emit_array_map_literal_int_bools_assign(
    name: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.set_array_length(name, items.len());
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Bool; items.len()]));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line(&format!("local.set ${}_len", name));
    for (index, item) in items.iter().enumerate() {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        require_int(item, module)?;
        emit_array_map_int_bool_result(callback, module);
        module.body().line("call $__rt_value_store_bool");
    }
    Ok(())
}

pub(super) fn emit_array_map_literal_string_bools_assign(
    name: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.set_array_length(name, items.len());
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Bool; items.len()]));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line(&format!("local.set ${}_len", name));
    for (index, item) in items.iter().enumerate() {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        emit_string_value_to_stack(item, module)?;
        emit_array_map_string_bool_result(callback, module);
        module.body().line("call $__rt_value_store_bool");
    }
    Ok(())
}

pub(super) fn emit_array_map_literal_scalar_type_bools_assign(
    name: &str,
    len: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_map_literal_const_bools_assign(name, len, true, module)
}

pub(super) fn emit_array_map_literal_const_bools_assign(
    name: &str,
    len: usize,
    value: bool,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Bool; len]));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module
            .body()
            .line(if value { "i32.const 1" } else { "i32.const 0" });
        module.body().line("call $__rt_value_store_bool");
    }
    Ok(())
}

pub(super) fn emit_array_map_literal_scalar_predicate_bools_assign(
    name: &str,
    items: &[Expr],
    callback: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.set_array_length(name, items.len());
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Bool; items.len()]));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line(&format!("local.set ${}_len", name));
    for (index, item) in items.iter().enumerate() {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        emit_array_filter_scalar_predicate_arg_from_expr(item, callback, kind, module)?;
        module.body().line("call $__rt_value_store_bool");
    }
    Ok(())
}

pub(super) fn emit_array_map_compact_int_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_map_runtime_compact_int_local_assign(
            name,
            source,
            source_span,
            callback,
            module,
        );
    };
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line("i64.load");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("i64.store");
    }
    Ok(())
}

pub(super) fn emit_array_map_compact_int_local_instance_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_map_runtime_compact_int_local_instance_assign(
            name,
            source,
            source_span,
            callback,
            capture_local,
            module,
        );
    };
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line(&format!("local.get ${}", capture_local));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line("i64.load");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("i64.store");
    }
    Ok(())
}

pub(super) fn emit_array_map_compact_int_bools_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_map_runtime_compact_int_bools_local_assign(
            name,
            source,
            source_span,
            callback,
            module,
        );
    };
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Bool; len]));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line("i64.load");
        emit_array_map_int_bool_result(callback, module);
        module.body().line("call $__rt_value_store_bool");
    }
    Ok(())
}

fn emit_array_map_runtime_compact_int_local_instance_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::CompactInt {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() over runtime-length arrays currently requires compact integer storage",
        ));
    }
    let index = module.next_label("array_map_instance_index");
    let done_label = module.next_label("array_map_instance_done");
    let loop_label = module.next_label("array_map_instance_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_map_runtime_compact_int_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::CompactInt {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() over runtime-length arrays currently requires compact integer storage",
        ));
    }
    let index = module.next_label("array_map_index");
    let done_label = module.next_label("array_map_done");
    let loop_label = module.next_label("array_map_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_map_runtime_compact_int_bools_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::CompactInt {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map() over runtime-length arrays currently requires compact integer storage",
        ));
    }
    let index = module.next_label("array_map_bool_index");
    let cell = module.next_label("array_map_bool_cell");
    let done_label = module.next_label("array_map_bool_done");
    let loop_label = module.next_label("array_map_bool_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Bool));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(name, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    emit_array_map_int_bool_result(callback, module);
    module.body().line("call $__rt_value_store_bool");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}
