//! Purpose:
//! Emits wasm32-web `array_reduce()` loops for associative array sources.
//! Keeps assoc value-cell traversal separate from callback dispatch and scalar cases.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_reduce`
//!
//! Key details:
//! - Uses existing associative value-cell runtime helpers and preserves CompileError guards.

use super::*;
use super::array_filter_truthiness::emit_value_cell_pointer_truthiness;

pub(super) fn emit_array_reduce_assoc_int_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_reduce_assoc_int_local_with_receiver(
        acc,
        source,
        source_span,
        callback,
        None,
        module,
    )
}

pub(super) fn emit_array_reduce_assoc_int_local_instance(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_reduce_assoc_int_local_with_receiver(
        acc,
        source,
        source_span,
        callback,
        Some(capture_local),
        module,
    )
}

fn emit_array_reduce_assoc_int_local_with_receiver(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    receiver: Option<&str>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over associative arrays requires known length metadata",
        ));
    };
    let entry = module.next_label("array_reduce_assoc_entry");
    let cell = module.next_label("array_reduce_assoc_cell");
    for local in [&entry, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    let value_kinds = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec());
    for index in 0..len {
        emit_array_reduce_optional_receiver_arg(receiver, module);
        module.body().line(&format!("local.get ${}", acc));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", entry));
        emit_assoc_value_cell_address(&entry, &cell, module);
        module.body().line(&format!("local.get {}", cell));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        if value_kinds
            .as_ref()
            .and_then(|kinds| kinds.get(index))
            == Some(&ValueCellKind::Str)
        {
            module.body().line("i32.load");
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            emit_stack_string_numeric_int_arg("array_reduce_assoc_string_int", module);
        } else {
            module.body().line("i64.load");
        }
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

pub(super) fn emit_array_reduce_runtime_assoc_int_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_reduce_runtime_assoc_int_local_with_receiver(
        acc,
        source,
        source_span,
        callback,
        None,
        module,
    )
}

pub(super) fn emit_array_reduce_runtime_assoc_int_local_instance(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_reduce_runtime_assoc_int_local_with_receiver(
        acc,
        source,
        source_span,
        callback,
        Some(capture_local),
        module,
    )
}

fn emit_array_reduce_runtime_assoc_int_local_with_receiver(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    receiver: Option<&str>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Assoc
        || !matches!(
            module.array_runtime_value_cell_kind(source),
            Some(ValueCellKind::Int | ValueCellKind::Bool | ValueCellKind::Str)
        )
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over runtime associative integer arrays requires homogeneous integer or bool metadata",
        ));
    }
    let value_kind = module.array_runtime_value_cell_kind(source).unwrap_or(ValueCellKind::Null);
    let index = module.next_label("array_reduce_assoc_index");
    let entry = module.next_label("array_reduce_assoc_entry");
    let cell = module.next_label("array_reduce_assoc_cell");
    let done_label = module.next_label("array_reduce_assoc_done");
    let loop_label = module.next_label("array_reduce_assoc_loop");
    for local in [&index, &entry, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_array_reduce_optional_receiver_arg(receiver, module);
    module.body().line(&format!("local.get ${}", acc));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    emit_assoc_value_cell_address(&entry, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    if value_kind == ValueCellKind::Str {
        module.body().line("i32.load");
        module.body().line(&format!("local.get {}", cell));
        module.body().line("i32.const 12");
        module.body().line("i32.add");
        module.body().line("i32.load");
        emit_stack_string_numeric_int_arg("array_reduce_runtime_assoc_string_int", module);
    } else {
        module.body().line("i64.load");
    }
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set ${}", acc));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_reduce_assoc_bool_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_reduce_assoc_bool_local_with_receiver(acc, source, source_span, callback, None, module)
}

pub(super) fn emit_array_reduce_assoc_bool_local_instance(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    receiver: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_reduce_assoc_bool_local_with_receiver(
        acc,
        source,
        source_span,
        callback,
        Some(receiver),
        module,
    )
}

fn emit_array_reduce_assoc_bool_local_with_receiver(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    receiver: Option<&str>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over associative arrays requires known length metadata",
        ));
    };
    let entry = module.next_label("array_reduce_assoc_entry");
    let cell = module.next_label("array_reduce_assoc_cell");
    for local in [&entry, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for index in 0..len {
        if let Some(receiver) = receiver {
            module.body().line(&format!("local.get ${}", receiver));
        }
        module.body().line(&format!("local.get ${}", acc));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", entry));
        emit_assoc_value_cell_address(&entry, &cell, module);
        module.body().line(&format!("local.get {}", cell));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line("i64.load");
        module.body().line("i64.const 0");
        module.body().line("i64.ne");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

pub(super) fn emit_array_reduce_runtime_assoc_bool_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_reduce_runtime_assoc_bool_local_with_receiver(
        acc,
        source,
        source_span,
        callback,
        None,
        module,
    )
}

pub(super) fn emit_array_reduce_runtime_assoc_bool_local_instance(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    receiver: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_reduce_runtime_assoc_bool_local_with_receiver(
        acc,
        source,
        source_span,
        callback,
        Some(receiver),
        module,
    )
}

fn emit_array_reduce_runtime_assoc_bool_local_with_receiver(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    receiver: Option<&str>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Assoc
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Bool)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over runtime associative bool arrays requires homogeneous bool metadata",
        ));
    }
    let index = module.next_label("array_reduce_assoc_index");
    let entry = module.next_label("array_reduce_assoc_entry");
    let cell = module.next_label("array_reduce_assoc_cell");
    let done_label = module.next_label("array_reduce_assoc_done");
    let loop_label = module.next_label("array_reduce_assoc_loop");
    for local in [&index, &entry, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    if let Some(receiver) = receiver {
        module.body().line(&format!("local.get ${}", receiver));
    }
    module.body().line(&format!("local.get ${}", acc));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    emit_assoc_value_cell_address(&entry, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set ${}", acc));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_reduce_assoc_truthy_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(kinds) = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec()) else {
        return emit_array_reduce_runtime_assoc_truthy_local(acc, source, source_span, callback, module);
    };
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over associative arrays requires known length metadata",
        ));
    };
    let entry = module.next_label("array_reduce_assoc_entry");
    let cell = module.next_label("array_reduce_assoc_cell");
    for local in [&entry, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for index in 0..len {
        let kind = kinds.get(index).copied().unwrap_or(ValueCellKind::Null);
        module.body().line(&format!("local.get ${}", acc));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", entry));
        emit_assoc_value_cell_address(&entry, &cell, module);
        emit_value_cell_pointer_truthiness(&cell, kind, module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

pub(super) fn emit_array_reduce_runtime_assoc_truthy_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(kind) = module.array_runtime_value_cell_kind(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() bool callbacks over runtime associative arrays require scalar metadata",
        ));
    };
    if module.array_layout(source) != ArrayLayout::Assoc
        || !matches!(
            kind,
            ValueCellKind::Int | ValueCellKind::Bool | ValueCellKind::Float | ValueCellKind::Str
        )
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() bool callbacks over runtime associative arrays require scalar storage",
        ));
    }
    let index = module.next_label("array_reduce_assoc_index");
    let entry = module.next_label("array_reduce_assoc_entry");
    let cell = module.next_label("array_reduce_assoc_cell");
    let done_label = module.next_label("array_reduce_assoc_done");
    let loop_label = module.next_label("array_reduce_assoc_loop");
    for local in [&index, &entry, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}", acc));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    emit_assoc_value_cell_address(&entry, &cell, module);
    emit_value_cell_pointer_truthiness(&cell, kind, module);
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set ${}", acc));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_reduce_assoc_float_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_reduce_assoc_float_local_with_receiver(acc, source, source_span, callback, None, module)
}

pub(super) fn emit_array_reduce_assoc_float_local_instance(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    receiver: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_reduce_assoc_float_local_with_receiver(
        acc,
        source,
        source_span,
        callback,
        Some(receiver),
        module,
    )
}

fn emit_array_reduce_assoc_float_local_with_receiver(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    receiver: Option<&str>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over associative arrays requires known length metadata",
        ));
    };
    let entry = module.next_label("array_reduce_assoc_entry");
    let cell = module.next_label("array_reduce_assoc_cell");
    for local in [&entry, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for index in 0..len {
        if let Some(receiver) = receiver {
            module.body().line(&format!("local.get ${}", receiver));
        }
        module.body().line(&format!("local.get ${}", acc));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", entry));
        emit_assoc_value_cell_address(&entry, &cell, module);
        module.body().line(&format!("local.get {}", cell));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line("f64.load");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

pub(super) fn emit_array_reduce_runtime_assoc_float_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_reduce_runtime_assoc_float_local_with_receiver(
        acc,
        source,
        source_span,
        callback,
        None,
        module,
    )
}

pub(super) fn emit_array_reduce_runtime_assoc_float_local_instance(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    receiver: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_reduce_runtime_assoc_float_local_with_receiver(
        acc,
        source,
        source_span,
        callback,
        Some(receiver),
        module,
    )
}

fn emit_array_reduce_runtime_assoc_float_local_with_receiver(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    receiver: Option<&str>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Assoc
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Float)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over runtime associative float arrays requires homogeneous float metadata",
        ));
    }
    let index = module.next_label("array_reduce_assoc_index");
    let entry = module.next_label("array_reduce_assoc_entry");
    let cell = module.next_label("array_reduce_assoc_cell");
    let done_label = module.next_label("array_reduce_assoc_done");
    let loop_label = module.next_label("array_reduce_assoc_loop");
    for local in [&index, &entry, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    if let Some(receiver) = receiver {
        module.body().line(&format!("local.get ${}", receiver));
    }
    module.body().line(&format!("local.get ${}", acc));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    emit_assoc_value_cell_address(&entry, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("f64.load");
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set ${}", acc));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_reduce_assoc_int_as_float_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over associative arrays requires known length metadata",
        ));
    };
    let entry = module.next_label("array_reduce_assoc_entry");
    let cell = module.next_label("array_reduce_assoc_cell");
    for local in [&entry, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    let value_kinds = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec());
    for index in 0..len {
        module.body().line(&format!("local.get ${}", acc));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", entry));
        emit_assoc_value_cell_address(&entry, &cell, module);
        module.body().line(&format!("local.get {}", cell));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        if value_kinds
            .as_ref()
            .and_then(|kinds| kinds.get(index))
            == Some(&ValueCellKind::Str)
        {
            module.body().line("i32.load");
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            emit_stack_string_numeric_float_arg("array_reduce_assoc_string_float", module);
        } else {
            module.body().line("i64.load");
            module.body().line("f64.convert_i64_s");
        }
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

pub(super) fn emit_array_reduce_runtime_assoc_int_as_float_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Assoc
        || !matches!(
            module.array_runtime_value_cell_kind(source),
            Some(ValueCellKind::Int | ValueCellKind::Bool | ValueCellKind::Str)
        )
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() float callbacks over runtime associative arrays require integer, bool, or string metadata",
        ));
    }
    let value_kind = module.array_runtime_value_cell_kind(source).unwrap_or(ValueCellKind::Null);
    let index = module.next_label("array_reduce_assoc_index");
    let entry = module.next_label("array_reduce_assoc_entry");
    let cell = module.next_label("array_reduce_assoc_cell");
    let done_label = module.next_label("array_reduce_assoc_done");
    let loop_label = module.next_label("array_reduce_assoc_loop");
    for local in [&index, &entry, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}", acc));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    emit_assoc_value_cell_address(&entry, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    if value_kind == ValueCellKind::Str {
        module.body().line("i32.load");
        module.body().line(&format!("local.get {}", cell));
        module.body().line("i32.const 12");
        module.body().line("i32.add");
        module.body().line("i32.load");
        emit_stack_string_numeric_float_arg("array_reduce_runtime_assoc_string_float", module);
    } else {
        module.body().line("i64.load");
        module.body().line("f64.convert_i64_s");
    }
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set ${}", acc));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_reduce_assoc_string_local(
    acc_ptr: &str,
    acc_len: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_reduce_assoc_string_local_with_receiver(
        acc_ptr,
        acc_len,
        source,
        source_span,
        callback,
        None,
        module,
    )
}

pub(super) fn emit_array_reduce_assoc_string_local_instance(
    acc_ptr: &str,
    acc_len: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_reduce_assoc_string_local_with_receiver(
        acc_ptr,
        acc_len,
        source,
        source_span,
        callback,
        Some(capture_local),
        module,
    )
}

fn emit_array_reduce_assoc_string_local_with_receiver(
    acc_ptr: &str,
    acc_len: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    receiver: Option<&str>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over associative arrays requires known length metadata",
        ));
    };
    let entry = module.next_label("array_reduce_assoc_entry");
    let cell = module.next_label("array_reduce_assoc_cell");
    for local in [&entry, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for index in 0..len {
        emit_array_reduce_optional_receiver_arg(receiver, module);
        module.body().line(&format!("local.get ${}", acc_ptr));
        module.body().line(&format!("local.get ${}", acc_len));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", entry));
        emit_assoc_value_cell_address(&entry, &cell, module);
        module.body().line(&format!("local.get {}", cell));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line("i32.load");
        module.body().line(&format!("local.get {}", cell));
        module.body().line("i32.const 12");
        module.body().line("i32.add");
        module.body().line("i32.load");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc_len));
        module.body().line(&format!("local.set ${}", acc_ptr));
    }
    Ok(())
}

pub(super) fn emit_array_reduce_runtime_assoc_string_local(
    acc_ptr: &str,
    acc_len: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_reduce_runtime_assoc_string_local_with_receiver(
        acc_ptr,
        acc_len,
        source,
        source_span,
        callback,
        None,
        module,
    )
}

pub(super) fn emit_array_reduce_runtime_assoc_string_local_instance(
    acc_ptr: &str,
    acc_len: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_reduce_runtime_assoc_string_local_with_receiver(
        acc_ptr,
        acc_len,
        source,
        source_span,
        callback,
        Some(capture_local),
        module,
    )
}

fn emit_array_reduce_runtime_assoc_string_local_with_receiver(
    acc_ptr: &str,
    acc_len: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    receiver: Option<&str>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Assoc
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Str)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over runtime associative string arrays requires homogeneous string metadata",
        ));
    }
    let index = module.next_label("array_reduce_assoc_index");
    let entry = module.next_label("array_reduce_assoc_entry");
    let cell = module.next_label("array_reduce_assoc_cell");
    let done_label = module.next_label("array_reduce_assoc_done");
    let loop_label = module.next_label("array_reduce_assoc_loop");
    for local in [&index, &entry, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_array_reduce_optional_receiver_arg(receiver, module);
    module.body().line(&format!("local.get ${}", acc_ptr));
    module.body().line(&format!("local.get ${}", acc_len));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    emit_assoc_value_cell_address(&entry, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set ${}", acc_len));
    module.body().line(&format!("local.set ${}", acc_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_array_reduce_optional_receiver_arg(receiver: Option<&str>, module: &mut WasmModule) {
    if let Some(receiver) = receiver {
        module.body().line(&format!("local.get ${}", receiver));
    }
}

pub(super) fn emit_array_reduce_assoc_dynamic_string_local(
    acc_ptr: &str,
    acc_len: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over associative scalar arrays requires known length metadata",
        ));
    };
    let entry = module.next_label("array_reduce_assoc_entry");
    let cell = module.next_label("array_reduce_assoc_cell");
    for local in [&entry, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for index in 0..len {
        module.body().line(&format!("local.get ${}", acc_ptr));
        module.body().line(&format!("local.get ${}", acc_len));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", entry));
        emit_assoc_value_cell_address(&entry, &cell, module);
        emit_dynamic_mixed_string_cast_value_to_stack(cell.trim_start_matches('$'), module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc_len));
        module.body().line(&format!("local.set ${}", acc_ptr));
    }
    Ok(())
}

pub(super) fn emit_array_reduce_runtime_assoc_dynamic_string_local(
    acc_ptr: &str,
    acc_len: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Assoc
        || !module
            .array_runtime_value_cell_kind(source)
            .is_some_and(array_reduce_string_callback_value_kind_is_supported)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over runtime associative scalar arrays requires scalar value-cell metadata",
        ));
    }
    let index = module.next_label("array_reduce_assoc_index");
    let entry = module.next_label("array_reduce_assoc_entry");
    let cell = module.next_label("array_reduce_assoc_cell");
    let done_label = module.next_label("array_reduce_assoc_done");
    let loop_label = module.next_label("array_reduce_assoc_loop");
    for local in [&index, &entry, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}", acc_ptr));
    module.body().line(&format!("local.get ${}", acc_len));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    emit_assoc_value_cell_address(&entry, &cell, module);
    emit_dynamic_mixed_string_cast_value_to_stack(cell.trim_start_matches('$'), module);
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set ${}", acc_len));
    module.body().line(&format!("local.set ${}", acc_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_reduce_assoc_literal_ints(
    acc: &str,
    items: &[(Expr, Expr)],
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_reduce_assoc_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_assoc_array_items_assign(&temp, items, module)?;
    emit_array_reduce_assoc_int_local(acc, &temp, source_span, callback, module)
}

pub(super) fn emit_array_reduce_assoc_literal_bools(
    acc: &str,
    items: &[(Expr, Expr)],
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_reduce_assoc_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_assoc_array_items_assign(&temp, items, module)?;
    emit_array_reduce_assoc_bool_local(acc, &temp, source_span, callback, module)
}

pub(super) fn emit_array_reduce_assoc_literal_truthy_cells(
    acc: &str,
    items: &[(Expr, Expr)],
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_reduce_assoc_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_assoc_array_items_assign(&temp, items, module)?;
    emit_array_reduce_assoc_truthy_local(acc, &temp, source_span, callback, module)
}

pub(super) fn emit_array_reduce_assoc_literal_floats(
    acc: &str,
    items: &[(Expr, Expr)],
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_reduce_assoc_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_assoc_array_items_assign(&temp, items, module)?;
    emit_array_reduce_assoc_float_local(acc, &temp, source_span, callback, module)
}

pub(super) fn emit_array_reduce_assoc_literal_ints_as_floats(
    acc: &str,
    items: &[(Expr, Expr)],
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_reduce_assoc_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_assoc_array_items_assign(&temp, items, module)?;
    emit_array_reduce_assoc_int_as_float_local(acc, &temp, source_span, callback, module)
}

pub(super) fn emit_array_reduce_assoc_literal_strings(
    acc_ptr: &str,
    acc_len: &str,
    items: &[(Expr, Expr)],
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_reduce_assoc_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_assoc_array_items_assign(&temp, items, module)?;
    emit_array_reduce_assoc_string_local(acc_ptr, acc_len, &temp, source_span, callback, module)
}

pub(super) fn emit_array_reduce_assoc_literal_dynamic_strings(
    acc_ptr: &str,
    acc_len: &str,
    items: &[(Expr, Expr)],
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_reduce_assoc_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_assoc_array_items_assign(&temp, items, module)?;
    emit_array_reduce_assoc_dynamic_string_local(acc_ptr, acc_len, &temp, source_span, callback, module)
}
