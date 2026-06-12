//! Purpose:
//! Emits wasm32-web array_map strlen/ord mapping helpers.
//! Keeps scalar string-length coercions and value-cell strlen loops out of general map emitters.
//!
//! Called from:
//! - `super::array_map_assign` and sibling map/filter/scalar helper modules.
//!
//! Key details:
//! - Preserves PHP scalar-to-string length behavior for ints, floats, bools, nulls, and strings.

use super::*;
use super::array_map_filter_value_strings::emit_array_map_runtime_value_string_lengths_assign;

pub(super) fn emit_array_map_literal_string_lengths_assign(
    name: &str,
    items: &[Expr],
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
        emit_array_map_strlen_result(module);
        module.body().line("i64.store");
    }
    Ok(())
}


pub(super) fn emit_array_map_literal_scalar_lengths_assign(
    name: &str,
    items: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(kinds) = value_cell_kinds_for_items(items, module) else {
        return Err(CompileError::new(
            Span::new(0, 0),
            "wasm32-web array_map(strlen) over scalar arrays requires value-cell metadata",
        ));
    };
    module.set_array_length(name, items.len());
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line(&format!("local.set ${}_len", name));
    for (index, (item, kind)) in items.iter().zip(kinds.iter()).enumerate() {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        emit_scalar_strlen_length(item, *kind, module)?;
        module.body().line("i64.store");
    }
    Ok(())
}

pub(super) fn emit_scalar_strlen_length(
    value: &Expr,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(text) = static_scalar_cast_string(value, module) {
        module.body().line(&format!("i64.const {}", text.len()));
        return Ok(());
    }
    match kind {
        ValueCellKind::Int => {
            require_int(value, module)?;
            emit_i64_stack_strlen_length("array_map_strlen_int", module);
            Ok(())
        }
        ValueCellKind::Str => {
            emit_string_value_to_stack(value, module)?;
            emit_array_map_strlen_result(module);
            Ok(())
        }
        ValueCellKind::Bool => {
            emit_condition(value, module)?;
            module.body().line("i64.extend_i32_u");
            Ok(())
        }
        ValueCellKind::Float => {
            require_float(value, module)?;
            emit_f64_stack_strlen_length("array_map_strlen_float", module);
            Ok(())
        }
        ValueCellKind::Null => {
            module.body().line("i64.const 0");
            Ok(())
        }
        _ => Err(CompileError::new(
            value.span,
            "wasm32-web array_map(strlen) currently supports scalar string-coercible values",
        )),
    }
}

pub(super) fn emit_i64_stack_strlen_length(prefix: &str, module: &mut WasmModule) {
    let len = module.next_label(&format!("{prefix}_len"));
    module.declare_i32_local(len.trim_start_matches('$').to_string());
    emit_i64_stack_string_cast_value_to_stack(prefix, module);
    module.body().line(&format!("local.set {}", len));
    module.body().line("drop");
    module.body().line(&format!("local.get {}", len));
    module.body().line("i64.extend_i32_u");
}

pub(super) fn emit_f64_stack_strlen_length(prefix: &str, module: &mut WasmModule) {
    let len = module.next_label(&format!("{prefix}_len"));
    module.declare_i32_local(len.trim_start_matches('$').to_string());
    emit_f64_stack_string_cast_value_to_stack(prefix, module);
    module.body().line(&format!("local.set {}", len));
    module.body().line("drop");
    module.body().line(&format!("local.get {}", len));
    module.body().line("i64.extend_i32_u");
}

pub(super) fn emit_stack_string_ord_value(prefix: &str, module: &mut WasmModule) {
    let ptr = module.next_label(&format!("{prefix}_ptr"));
    let len = module.next_label(&format!("{prefix}_len"));
    module.declare_i32_local(ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(len.trim_start_matches('$').to_string());
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.set {}", ptr));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.eqz");
    module.body().open("if (result i64)");
    module.body().line("i64.const 0");
    module.body().line("else");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line("i32.load8_u");
    module.body().line("i64.extend_i32_u");
    module.body().close("end");
}


pub(super) fn emit_array_map_compact_int_strlen_lengths_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_map_runtime_compact_int_strlen_lengths_assign(name, source, source_span, module);
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
        emit_i64_stack_strlen_length("array_map_compact_int_strlen", module);
        module.body().line("i64.store");
    }
    Ok(())
}

pub(super) fn emit_array_map_runtime_compact_int_strlen_lengths_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::CompactInt {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map(strlen) over runtime-length integer arrays requires compact integer storage",
        ));
    }
    let index = module.next_label("array_map_compact_int_strlen_index");
    let done_label = module.next_label("array_map_compact_int_strlen_done");
    let loop_label = module.next_label("array_map_compact_int_strlen_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, None);
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
    emit_i64_stack_strlen_length("array_map_compact_int_strlen", module);
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

pub(super) fn emit_array_map_value_scalar_lengths_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_map_runtime_value_scalar_lengths_assign(name, source, source_span, module);
    };
    let Some(kinds) = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec()) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map(strlen) over scalar arrays requires value-cell metadata",
        ));
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
    for (index, kind) in kinds.iter().enumerate() {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        emit_value_cell_strlen_length(source, index, *kind, module);
        module.body().line("i64.store");
    }
    Ok(())
}

pub(super) fn emit_array_map_runtime_value_scalar_lengths_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(kind) = module.array_runtime_value_cell_kind(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map(strlen) over runtime-length scalar arrays requires homogeneous value-cell storage",
        ));
    };
    if !array_map_strlen_value_kind_is_supported(kind) {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map(strlen) currently supports scalar string-coercible value cells",
        ));
    }
    let index = module.next_label("array_map_scalar_strlen_index");
    let done_label = module.next_label("array_map_scalar_strlen_done");
    let loop_label = module.next_label("array_map_scalar_strlen_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, None);
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
    emit_value_cell_strlen_length_dynamic(source, &index, kind, module);
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

pub(super) fn emit_array_map_value_string_lengths_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_map_runtime_value_string_lengths_assign(name, source, source_span, module);
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
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 8");
        module.body().line("call $__rt_value_payload_i32");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 12");
        module.body().line("call $__rt_value_payload_i32");
        emit_array_map_strlen_result(module);
        module.body().line("i64.store");
    }
    Ok(())
}

pub(super) fn emit_value_cell_strlen_length(
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
            emit_i64_stack_strlen_length("array_map_value_int_strlen", module);
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
            emit_array_map_strlen_result(module);
        }
        ValueCellKind::Bool => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("call $__rt_value_payload_i64");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
            module.body().line("i64.extend_i32_u");
        }
        ValueCellKind::Float => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE + 8));
            module.body().line("i32.add");
            module.body().line("f64.load");
            emit_f64_stack_strlen_length("array_map_value_float_strlen", module);
        }
        ValueCellKind::Null => {
            module.body().line("i64.const 0");
        }
        _ => unreachable!("array_map strlen length only handles scalar cells"),
    }
}

pub(super) fn emit_value_cell_strlen_length_dynamic(
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
            emit_i64_stack_strlen_length("array_map_value_int_strlen", module);
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
            emit_array_map_strlen_result(module);
        }
        ValueCellKind::Bool => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get {}", index));
            module.body().line("call $__rt_value_payload_i64");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
            module.body().line("i64.extend_i32_u");
        }
        ValueCellKind::Float => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get {}", index));
            module.body().line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
            module.body().line("i32.mul");
            module.body().line("i32.add");
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("f64.load");
            emit_f64_stack_strlen_length("array_map_value_float_strlen", module);
        }
        ValueCellKind::Null => {
            module.body().line("i64.const 0");
        }
        _ => unreachable!("array_map dynamic strlen length only handles scalar cells"),
    }
}

pub(super) fn emit_array_map_strlen_result(module: &mut WasmModule) {
    let len = module.next_label("array_map_strlen_len");
    module.declare_i32_local(len.trim_start_matches('$').to_string());
    module.body().line(&format!("local.set {}", len));
    module.body().line("drop");
    module.body().line(&format!("local.get {}", len));
    module.body().line("i64.extend_i32_u");
}
