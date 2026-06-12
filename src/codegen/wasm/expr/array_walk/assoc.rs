//! Purpose:
//! Emits wasm32-web array_walk() loops for associative arrays.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_walk`.
//!
//! Key details:
//! - Matches callback value/key shapes against static and runtime associative metadata.
//! - Materializes scalar value cells plus int or string keys before calling the user callback.

use super::*;

pub(super) fn array_walk_assoc_local_matches_shape(
    source: &str,
    shape: ArrayWalkCallbackShape,
    module: &WasmModule,
) -> bool {
    let Some(value_kinds) = module.array_value_cell_kinds(source) else {
        return false;
    };
    let values_match = value_kinds.iter().all(|kind| {
        matches!(
            (shape, *kind),
            (ArrayWalkCallbackShape::Int, ValueCellKind::Int)
                | (ArrayWalkCallbackShape::Int, ValueCellKind::Str)
                | (ArrayWalkCallbackShape::Float, ValueCellKind::Float)
                | (ArrayWalkCallbackShape::Float, ValueCellKind::Str)
                | (ArrayWalkCallbackShape::Bool, ValueCellKind::Bool)
                | (ArrayWalkCallbackShape::IntWithIntKey, ValueCellKind::Int)
                | (ArrayWalkCallbackShape::IntWithIntKey, ValueCellKind::Str)
                | (ArrayWalkCallbackShape::IntWithStrKey, ValueCellKind::Int)
                | (ArrayWalkCallbackShape::IntWithStrKey, ValueCellKind::Str)
                | (ArrayWalkCallbackShape::FloatWithIntKey, ValueCellKind::Float)
                | (ArrayWalkCallbackShape::FloatWithIntKey, ValueCellKind::Str)
                | (ArrayWalkCallbackShape::FloatWithStrKey, ValueCellKind::Float)
                | (ArrayWalkCallbackShape::FloatWithStrKey, ValueCellKind::Str)
                | (ArrayWalkCallbackShape::BoolWithIntKey, ValueCellKind::Bool)
                | (ArrayWalkCallbackShape::BoolWithStrKey, ValueCellKind::Bool)
                | (ArrayWalkCallbackShape::Str, ValueCellKind::Str)
                | (ArrayWalkCallbackShape::StrWithIntKey, ValueCellKind::Str)
                | (ArrayWalkCallbackShape::StrWithStrKey, ValueCellKind::Str)
                | (ArrayWalkCallbackShape::Mixed, _)
                | (ArrayWalkCallbackShape::MixedWithIntKey, _)
                | (ArrayWalkCallbackShape::MixedWithStrKey, _)
        )
    });
    if !values_match {
        return false;
    }
    if shape.needs_int_key() {
        return module
            .array_key_kinds(source)
            .is_some_and(|kinds| kinds.iter().all(|kind| *kind == AssocKeyKind::Int));
    }
    if shape.needs_string_key() {
        return module
            .array_key_kinds(source)
            .is_some_and(|kinds| kinds.iter().all(|kind| *kind == AssocKeyKind::Str));
    }
    true
}

pub(super) fn array_walk_runtime_assoc_local_matches_shape(
    source: &str,
    shape: ArrayWalkCallbackShape,
    module: &WasmModule,
) -> bool {
    let Some(value_kind) = module.array_runtime_value_cell_kind(source) else {
        return false;
    };
    let values_match = matches!(
        (shape, value_kind),
        (ArrayWalkCallbackShape::Int, ValueCellKind::Int)
            | (ArrayWalkCallbackShape::Int, ValueCellKind::Str)
            | (ArrayWalkCallbackShape::Float, ValueCellKind::Float)
            | (ArrayWalkCallbackShape::Float, ValueCellKind::Str)
            | (ArrayWalkCallbackShape::Bool, ValueCellKind::Bool)
            | (ArrayWalkCallbackShape::IntWithIntKey, ValueCellKind::Int)
            | (ArrayWalkCallbackShape::IntWithIntKey, ValueCellKind::Str)
            | (ArrayWalkCallbackShape::IntWithStrKey, ValueCellKind::Int)
            | (ArrayWalkCallbackShape::IntWithStrKey, ValueCellKind::Str)
            | (ArrayWalkCallbackShape::FloatWithIntKey, ValueCellKind::Float)
            | (ArrayWalkCallbackShape::FloatWithIntKey, ValueCellKind::Str)
            | (ArrayWalkCallbackShape::FloatWithStrKey, ValueCellKind::Float)
            | (ArrayWalkCallbackShape::FloatWithStrKey, ValueCellKind::Str)
            | (ArrayWalkCallbackShape::BoolWithIntKey, ValueCellKind::Bool)
            | (ArrayWalkCallbackShape::BoolWithStrKey, ValueCellKind::Bool)
            | (ArrayWalkCallbackShape::Str, ValueCellKind::Str)
            | (ArrayWalkCallbackShape::StrWithIntKey, ValueCellKind::Str)
            | (ArrayWalkCallbackShape::StrWithStrKey, ValueCellKind::Str)
            | (ArrayWalkCallbackShape::Mixed, _)
            | (ArrayWalkCallbackShape::MixedWithIntKey, _)
            | (ArrayWalkCallbackShape::MixedWithStrKey, _)
    );
    if !values_match {
        return false;
    }
    if shape.needs_int_key() {
        return module.array_runtime_key_kind(source) == Some(AssocKeyKind::Int);
    }
    if shape.needs_string_key() {
        return module.array_runtime_key_kind(source) == Some(AssocKeyKind::Str);
    }
    true
}

pub(super) fn emit_array_walk_assoc_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_walk_assoc_local_with_receiver(source, source_span, callback, shape, None, module)
}

pub(super) fn emit_array_walk_assoc_local_instance(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_walk_assoc_local_with_receiver(
        source,
        source_span,
        callback,
        shape,
        Some(capture_local),
        module,
    )
}

fn emit_array_walk_assoc_local_with_receiver(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    receiver: Option<&str>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_walk() over associative arrays requires known length metadata",
        ));
    };
    let entry = module.next_label("array_walk_assoc_entry");
    let cell = module.next_label("array_walk_assoc_cell");
    let mixed_arg = module.next_label("array_walk_assoc_mixed_arg");
    for local in [&entry, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    if shape.accepts_mixed_value() {
        module.declare_i32_local(mixed_arg.trim_start_matches('$').to_string());
        emit_alloc_mixed_cell(mixed_arg.trim_start_matches('$'), module);
    }
    let value_kinds = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec());
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", entry));
        emit_assoc_value_cell_address(&entry, &cell, module);
        let value_kind = value_kinds
            .as_ref()
            .and_then(|kinds| kinds.get(index))
            .copied()
            .unwrap_or(ValueCellKind::Null);
        if let Some(receiver) = receiver {
            module.body().line(&format!("local.get ${}", receiver));
        }
        emit_array_walk_assoc_callback_args(&entry, &cell, &mixed_arg, shape, value_kind, module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("drop");
    }
    if shape.accepts_mixed_value() {
        module.body().line(&format!("local.get {}", mixed_arg));
        module.body().line("call $__rt_value_release");
    }
    Ok(())
}

pub(super) fn emit_array_walk_runtime_assoc_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_walk_runtime_assoc_local_with_receiver(source, source_span, callback, shape, None, module)
}

pub(super) fn emit_array_walk_runtime_assoc_local_instance(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_walk_runtime_assoc_local_with_receiver(
        source,
        source_span,
        callback,
        shape,
        Some(capture_local),
        module,
    )
}

fn emit_array_walk_runtime_assoc_local_with_receiver(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    receiver: Option<&str>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Assoc
        || !array_walk_runtime_assoc_local_matches_shape(source, shape, module)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_walk() over runtime associative arrays requires homogeneous callback-compatible metadata",
        ));
    }
    let value_kind = module.array_runtime_value_cell_kind(source).unwrap_or(ValueCellKind::Null);
    let index = module.next_label("array_walk_assoc_index");
    let entry = module.next_label("array_walk_assoc_entry");
    let cell = module.next_label("array_walk_assoc_cell");
    let mixed_arg = module.next_label("array_walk_assoc_mixed_arg");
    let done_label = module.next_label("array_walk_assoc_done");
    let loop_label = module.next_label("array_walk_assoc_loop");
    for local in [&index, &entry, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    if shape.accepts_mixed_value() {
        module.declare_i32_local(mixed_arg.trim_start_matches('$').to_string());
        emit_alloc_mixed_cell(mixed_arg.trim_start_matches('$'), module);
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    emit_assoc_value_cell_address(&entry, &cell, module);
    if let Some(receiver) = receiver {
        module.body().line(&format!("local.get ${}", receiver));
    }
    emit_array_walk_assoc_callback_args(&entry, &cell, &mixed_arg, shape, value_kind, module);
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("drop");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    if shape.accepts_mixed_value() {
        module.body().line(&format!("local.get {}", mixed_arg));
        module.body().line("call $__rt_value_release");
    }
    Ok(())
}

fn emit_array_walk_assoc_callback_args(
    entry: &str,
    cell: &str,
    mixed_arg: &str,
    shape: ArrayWalkCallbackShape,
    value_kind: ValueCellKind,
    module: &mut WasmModule,
) {
    match shape {
        ArrayWalkCallbackShape::Int => emit_array_walk_assoc_int_value(cell, value_kind, module),
        ArrayWalkCallbackShape::Float => emit_array_walk_assoc_float_value(cell, value_kind, module),
        ArrayWalkCallbackShape::Bool => emit_array_walk_assoc_bool_value(cell, module),
        ArrayWalkCallbackShape::Str => emit_array_walk_assoc_string_value(cell, module),
        ArrayWalkCallbackShape::IntWithIntKey => {
            emit_array_walk_assoc_int_value(cell, value_kind, module);
            emit_array_walk_assoc_int_key_arg(entry, module);
        }
        ArrayWalkCallbackShape::FloatWithIntKey => {
            emit_array_walk_assoc_float_value(cell, value_kind, module);
            emit_array_walk_assoc_int_key_arg(entry, module);
        }
        ArrayWalkCallbackShape::BoolWithIntKey => {
            emit_array_walk_assoc_bool_value(cell, module);
            emit_array_walk_assoc_int_key_arg(entry, module);
        }
        ArrayWalkCallbackShape::StrWithIntKey => {
            emit_array_walk_assoc_string_value(cell, module);
            emit_array_walk_assoc_int_key_arg(entry, module);
        }
        ArrayWalkCallbackShape::IntWithStrKey => {
            emit_array_walk_assoc_int_value(cell, value_kind, module);
            emit_array_walk_assoc_string_key_arg(entry, module);
        }
        ArrayWalkCallbackShape::FloatWithStrKey => {
            emit_array_walk_assoc_float_value(cell, value_kind, module);
            emit_array_walk_assoc_string_key_arg(entry, module);
        }
        ArrayWalkCallbackShape::BoolWithStrKey => {
            emit_array_walk_assoc_bool_value(cell, module);
            emit_array_walk_assoc_string_key_arg(entry, module);
        }
        ArrayWalkCallbackShape::StrWithStrKey => {
            emit_array_walk_assoc_string_value(cell, module);
            emit_array_walk_assoc_string_key_arg(entry, module);
        }
        ArrayWalkCallbackShape::Mixed => {
            emit_array_walk_assoc_mixed_value(cell, mixed_arg, module);
        }
        ArrayWalkCallbackShape::MixedWithIntKey => {
            emit_array_walk_assoc_mixed_value(cell, mixed_arg, module);
            emit_array_walk_assoc_int_key_arg(entry, module);
        }
        ArrayWalkCallbackShape::MixedWithStrKey => {
            emit_array_walk_assoc_mixed_value(cell, mixed_arg, module);
            emit_array_walk_assoc_string_key_arg(entry, module);
        }
    }
}

fn emit_array_walk_assoc_mixed_value(cell: &str, mixed_arg: &str, module: &mut WasmModule) {
    emit_copy_value_cell_from_addr_to_addr(mixed_arg, cell, module);
    module.body().line(&format!("local.get {}", mixed_arg));
}

fn emit_array_walk_assoc_int_value(cell: &str, value_kind: ValueCellKind, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    if value_kind == ValueCellKind::Str {
        module.body().line("i32.load");
        module.body().line(&format!("local.get {}", cell));
        module.body().line("i32.const 12");
        module.body().line("i32.add");
        module.body().line("i32.load");
        emit_stack_string_numeric_int_arg("array_walk_assoc_string_int", module);
    } else {
        module.body().line("i64.load");
    }
}

fn emit_array_walk_assoc_float_value(cell: &str, value_kind: ValueCellKind, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    if value_kind == ValueCellKind::Str {
        module.body().line("i32.load");
        module.body().line(&format!("local.get {}", cell));
        module.body().line("i32.const 12");
        module.body().line("i32.add");
        module.body().line("i32.load");
        emit_stack_string_numeric_float_arg("array_walk_assoc_string_float", module);
    } else {
        module.body().line("f64.load");
    }
}

fn emit_array_walk_assoc_bool_value(cell: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
}

fn emit_array_walk_assoc_string_value(cell: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
}

fn emit_array_walk_assoc_int_key_arg(entry: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
}

fn emit_array_walk_assoc_string_key_arg(entry: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
}
