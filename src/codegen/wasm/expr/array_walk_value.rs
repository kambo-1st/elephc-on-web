//! Purpose:
//! Lowers wasm32-web array_walk() loops over homogeneous value-cell arrays.
//! Keeps scalar value payload handling separate from callback dispatch and assoc walkers.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_walk`.
//!
//! Key details:
//! - Handles known-length and runtime-length int, float, bool, and string value-cell arrays.

use super::*;
use super::array_walk::{
    emit_array_walk_runtime_int_key_arg, emit_array_walk_static_int_key_arg,
    ArrayWalkCallbackShape,
};

pub(super) fn emit_array_walk_mixed_value_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_walk() mixed callbacks over value arrays require value-cell storage",
        ));
    }
    let mixed_arg = module.next_label("array_walk_mixed_value_arg");
    let source_cell = module.next_label("array_walk_mixed_value_source_cell");
    for local in [&mixed_arg, &source_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_alloc_mixed_cell(mixed_arg.trim_start_matches('$'), module);
    if let Some(len) = module.array_length(source) {
        for index in 0..len {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("call $__rt_value_cell");
            module.body().line(&format!("local.set {}", source_cell));
            emit_copy_value_cell_from_addr_to_addr(&mixed_arg, &source_cell, module);
            module.body().line(&format!("local.get {}", mixed_arg));
            emit_array_walk_static_int_key_arg(index, shape, module);
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(callback)));
            module.body().line("drop");
        }
    } else {
        emit_array_walk_runtime_mixed_value_local(source, callback, shape, &mixed_arg, &source_cell, module);
    }
    module.body().line(&format!("local.get {}", mixed_arg));
    module.body().line("call $__rt_value_release");
    Ok(())
}

fn emit_array_walk_runtime_mixed_value_local(
    source: &str,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    mixed_arg: &str,
    source_cell: &str,
    module: &mut WasmModule,
) {
    let index = module.next_label("array_walk_mixed_value_index");
    let done_label = module.next_label("array_walk_mixed_value_done");
    let loop_label = module.next_label("array_walk_mixed_value_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(source, &index, source_cell, module);
    emit_copy_value_cell_from_addr_to_addr(mixed_arg, source_cell, module);
    module.body().line(&format!("local.get {}", mixed_arg));
    emit_array_walk_runtime_int_key_arg(&index, shape, module);
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
}

pub(super) fn emit_array_walk_value_int_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_walk_runtime_value_int_local(source, source_span, callback, shape, module);
    };
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_payload_i64");
        emit_array_walk_static_int_key_arg(index, shape, module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("drop");
    }
    Ok(())
}

pub(super) fn emit_array_walk_runtime_value_int_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Int)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_walk() over runtime-length integer arrays requires integer value-cell storage",
        ));
    }
    let index = module.next_label("array_walk_value_int_index");
    let done_label = module.next_label("array_walk_value_int_done");
    let loop_label = module.next_label("array_walk_value_int_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
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
    module.body().line("call $__rt_value_payload_i64");
    emit_array_walk_runtime_int_key_arg(&index, shape, module);
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
    Ok(())
}

pub(super) fn emit_array_walk_value_numeric_string_as_int_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_walk_runtime_value_numeric_string_as_int_local(
            source,
            source_span,
            callback,
            shape,
            module,
        );
    };
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 8");
        module.body().line("call $__rt_value_payload_i32");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 12");
        module.body().line("call $__rt_value_payload_i32");
        emit_stack_string_numeric_int_arg("array_walk_value_string_int", module);
        emit_array_walk_static_int_key_arg(index, shape, module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("drop");
    }
    Ok(())
}

pub(super) fn emit_array_walk_runtime_value_numeric_string_as_int_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Str)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_walk() int callbacks over runtime-length string arrays require string value-cell storage",
        ));
    }
    let index = module.next_label("array_walk_value_string_int_index");
    let done_label = module.next_label("array_walk_value_string_int_done");
    let loop_label = module.next_label("array_walk_value_string_int_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
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
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    emit_stack_string_numeric_int_arg("array_walk_runtime_value_string_int", module);
    emit_array_walk_runtime_int_key_arg(&index, shape, module);
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
    Ok(())
}

pub(super) fn emit_array_walk_value_numeric_string_as_float_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_walk_runtime_value_numeric_string_as_float_local(
            source,
            source_span,
            callback,
            shape,
            module,
        );
    };
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 8");
        module.body().line("call $__rt_value_payload_i32");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 12");
        module.body().line("call $__rt_value_payload_i32");
        emit_stack_string_numeric_float_arg("array_walk_value_string_float", module);
        emit_array_walk_static_int_key_arg(index, shape, module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("drop");
    }
    Ok(())
}

pub(super) fn emit_array_walk_runtime_value_numeric_string_as_float_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Str)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_walk() float callbacks over runtime-length string arrays require string value-cell storage",
        ));
    }
    let index = module.next_label("array_walk_value_string_float_index");
    let done_label = module.next_label("array_walk_value_string_float_done");
    let loop_label = module.next_label("array_walk_value_string_float_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
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
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    emit_stack_string_numeric_float_arg("array_walk_runtime_value_string_float", module);
    emit_array_walk_runtime_int_key_arg(&index, shape, module);
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
    Ok(())
}

pub(super) fn emit_array_walk_value_float_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_walk_runtime_value_float_local(source, source_span, callback, shape, module);
    };
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_payload_f64");
        emit_array_walk_static_int_key_arg(index, shape, module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("drop");
    }
    Ok(())
}

pub(super) fn emit_array_walk_runtime_value_float_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Float)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_walk() over runtime-length float arrays requires float value-cell storage",
        ));
    }
    let index = module.next_label("array_walk_float_index");
    let done_label = module.next_label("array_walk_float_done");
    let loop_label = module.next_label("array_walk_float_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
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
    module.body().line("call $__rt_value_payload_f64");
    emit_array_walk_runtime_int_key_arg(&index, shape, module);
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
    Ok(())
}

pub(super) fn emit_array_walk_value_float_local_instance(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_walk_runtime_value_float_local_instance(
            source,
            source_span,
            callback,
            capture_local,
            shape,
            module,
        );
    };
    for index in 0..len {
        module.body().line(&format!("local.get ${}", capture_local));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_payload_f64");
        emit_array_walk_static_int_key_arg(index, shape, module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("drop");
    }
    Ok(())
}

fn emit_array_walk_runtime_value_float_local_instance(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Float)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_walk() over runtime-length float arrays requires float value-cell storage",
        ));
    }
    let index = module.next_label("array_walk_float_instance_index");
    let done_label = module.next_label("array_walk_float_instance_done");
    let loop_label = module.next_label("array_walk_float_instance_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_f64");
    emit_array_walk_runtime_int_key_arg(&index, shape, module);
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
    Ok(())
}

pub(super) fn emit_array_walk_value_bool_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_walk_runtime_value_bool_local(source, source_span, callback, shape, module);
    };
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_payload_i64");
        module.body().line("i64.const 0");
        module.body().line("i64.ne");
        emit_array_walk_static_int_key_arg(index, shape, module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("drop");
    }
    Ok(())
}

pub(super) fn emit_array_walk_runtime_value_bool_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Bool)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_walk() over runtime-length bool arrays requires bool value-cell storage",
        ));
    }
    let index = module.next_label("array_walk_bool_index");
    let done_label = module.next_label("array_walk_bool_done");
    let loop_label = module.next_label("array_walk_bool_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
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
    module.body().line("call $__rt_value_payload_i64");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    emit_array_walk_runtime_int_key_arg(&index, shape, module);
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
    Ok(())
}

pub(super) fn emit_array_walk_value_bool_local_instance(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_walk_runtime_value_bool_local_instance(
            source,
            source_span,
            callback,
            capture_local,
            shape,
            module,
        );
    };
    for index in 0..len {
        module.body().line(&format!("local.get ${}", capture_local));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_payload_i64");
        module.body().line("i64.const 0");
        module.body().line("i64.ne");
        emit_array_walk_static_int_key_arg(index, shape, module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("drop");
    }
    Ok(())
}

fn emit_array_walk_runtime_value_bool_local_instance(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Bool)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_walk() over runtime-length bool arrays requires bool value-cell storage",
        ));
    }
    let index = module.next_label("array_walk_bool_instance_index");
    let done_label = module.next_label("array_walk_bool_instance_done");
    let loop_label = module.next_label("array_walk_bool_instance_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_i64");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    emit_array_walk_runtime_int_key_arg(&index, shape, module);
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
    Ok(())
}

pub(super) fn emit_array_walk_value_string_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_walk_runtime_value_string_local(source, source_span, callback, shape, module);
    };
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 8");
        module.body().line("call $__rt_value_payload_i32");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 12");
        module.body().line("call $__rt_value_payload_i32");
        emit_array_walk_static_int_key_arg(index, shape, module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("drop");
    }
    Ok(())
}

pub(super) fn emit_array_walk_value_string_local_instance(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_walk_runtime_value_string_local_instance(
            source,
            source_span,
            callback,
            capture_local,
            shape,
            module,
        );
    };
    for index in 0..len {
        module.body().line(&format!("local.get ${}", capture_local));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 8");
        module.body().line("call $__rt_value_payload_i32");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 12");
        module.body().line("call $__rt_value_payload_i32");
        emit_array_walk_static_int_key_arg(index, shape, module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("drop");
    }
    Ok(())
}

pub(super) fn emit_array_walk_runtime_value_string_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Str)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_walk() over runtime-length string arrays requires string value-cell storage",
        ));
    }
    let index = module.next_label("array_walk_string_index");
    let done_label = module.next_label("array_walk_string_done");
    let loop_label = module.next_label("array_walk_string_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
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
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    emit_array_walk_runtime_int_key_arg(&index, shape, module);
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
    Ok(())
}

fn emit_array_walk_runtime_value_string_local_instance(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Str)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_walk() over runtime-length string arrays requires string value-cell storage",
        ));
    }
    let index = module.next_label("array_walk_string_instance_index");
    let done_label = module.next_label("array_walk_string_instance_done");
    let loop_label = module.next_label("array_walk_string_instance_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    emit_array_walk_runtime_int_key_arg(&index, shape, module);
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
    Ok(())
}

pub(super) fn emit_array_walk_value_dynamic_string_local(
    source: &str,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let index = module.next_label("array_walk_dynamic_string_index");
    let cell = module.next_label("array_walk_dynamic_string_cell");
    let done_label = module.next_label("array_walk_dynamic_string_done");
    let loop_label = module.next_label("array_walk_dynamic_string_loop");
    for local in [&index, &cell] {
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
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
    emit_dynamic_mixed_string_cast_value_to_stack(cell.trim_start_matches('$'), module);
    emit_array_walk_runtime_int_key_arg(&index, shape, module);
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
    Ok(())
}
