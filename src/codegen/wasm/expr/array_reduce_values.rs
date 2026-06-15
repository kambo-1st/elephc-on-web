//! Purpose:
//! Emits wasm32-web `array_reduce()` loops for value-cell array sources.
//! Keeps mixed scalar value-cell traversal separate from reduce callback dispatch.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_reduce`
//!
//! Key details:
//! - Supports known-length and runtime-length value-cell arrays with homogeneous metadata guards.

use super::*;
use super::array_filter_truthiness::emit_value_cell_pointer_truthiness;

pub(super) fn emit_array_reduce_known_value_int_loop(
    acc: &str,
    source: &str,
    len: usize,
    callback: &str,
    module: &mut WasmModule,
) {
    for index in 0..len {
        module.body().line(&format!("local.get ${}", acc));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_payload_i64");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
}

pub(super) fn emit_array_reduce_value_int_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_reduce_runtime_value_int_local(acc, source, source_span, callback, module);
    };
    emit_array_reduce_known_value_int_loop(acc, source, len, callback, module);
    Ok(())
}

pub(super) fn emit_array_reduce_runtime_value_int_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || !matches!(
            module.array_runtime_value_cell_kind(source),
            Some(ValueCellKind::Int | ValueCellKind::Bool)
        )
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over runtime-length integer arrays requires integer or bool value-cell storage",
        ));
    }
    let index = module.next_label("array_reduce_value_int_index");
    let done_label = module.next_label("array_reduce_value_int_done");
    let loop_label = module.next_label("array_reduce_value_int_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
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
    module.body().line("call $__rt_value_payload_i64");
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

pub(super) fn emit_array_reduce_value_numeric_string_as_int_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_reduce_runtime_value_numeric_string_as_int_local(
            acc,
            source,
            source_span,
            callback,
            module,
        );
    };
    for index in 0..len {
        module.body().line(&format!("local.get ${}", acc));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 8");
        module.body().line("call $__rt_value_payload_i32");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 12");
        module.body().line("call $__rt_value_payload_i32");
        emit_stack_string_numeric_int_arg("array_reduce_value_string_int", module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

pub(super) fn emit_array_reduce_runtime_value_numeric_string_as_int_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Str)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() int callbacks over runtime-length string arrays require string value-cell storage",
        ));
    }
    let index = module.next_label("array_reduce_value_string_int_index");
    let done_label = module.next_label("array_reduce_value_string_int_done");
    let loop_label = module.next_label("array_reduce_value_string_int_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
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
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    emit_stack_string_numeric_int_arg("array_reduce_runtime_value_string_int", module);
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

pub(super) fn emit_array_reduce_value_object_int_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(classes) = module.array_object_classes(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() object callbacks require object value-cell metadata",
        ));
    };
    if classes.is_empty() || classes.iter().any(Option::is_none) {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() object callbacks require exact object value-cell metadata",
        ));
    }
    let index = module.next_label("array_reduce_object_index");
    let cell = module.next_label("array_reduce_object_cell");
    let done_label = module.next_label("array_reduce_object_done");
    let loop_label = module.next_label("array_reduce_object_loop");
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
    emit_value_cell_address_for_local(source, &index, &cell, module);
    module.body().line(&format!("local.get ${}", acc));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_OBJECT));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
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

pub(super) fn emit_array_reduce_value_object_float_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(classes) = module.array_object_classes(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() object callbacks require object value-cell metadata",
        ));
    };
    if classes.is_empty() || classes.iter().any(Option::is_none) {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() object callbacks require exact object value-cell metadata",
        ));
    }
    let index = module.next_label("array_reduce_object_index");
    let cell = module.next_label("array_reduce_object_cell");
    let done_label = module.next_label("array_reduce_object_done");
    let loop_label = module.next_label("array_reduce_object_loop");
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
    emit_value_cell_address_for_local(source, &index, &cell, module);
    module.body().line(&format!("local.get ${}", acc));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_OBJECT));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
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

pub(super) fn emit_array_reduce_value_object_bool_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(classes) = module.array_object_classes(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() object callbacks require object value-cell metadata",
        ));
    };
    if classes.is_empty() || classes.iter().any(Option::is_none) {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() object callbacks require exact object value-cell metadata",
        ));
    }
    let index = module.next_label("array_reduce_object_index");
    let cell = module.next_label("array_reduce_object_cell");
    let done_label = module.next_label("array_reduce_object_done");
    let loop_label = module.next_label("array_reduce_object_loop");
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
    emit_value_cell_address_for_local(source, &index, &cell, module);
    module.body().line(&format!("local.get ${}", acc));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_OBJECT));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
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

pub(super) fn emit_array_reduce_value_bool_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_reduce_runtime_value_bool_local(acc, source, source_span, callback, module);
    };
    for index in 0..len {
        module.body().line(&format!("local.get ${}", acc));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_payload_i64");
        module.body().line("i64.const 0");
        module.body().line("i64.ne");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

pub(super) fn emit_array_reduce_runtime_value_bool_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Bool)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over runtime-length bool arrays requires bool value-cell storage",
        ));
    }
    let index = module.next_label("array_reduce_bool_index");
    let done_label = module.next_label("array_reduce_bool_done");
    let loop_label = module.next_label("array_reduce_bool_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
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
    module.body().line("call $__rt_value_payload_i64");
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

pub(super) fn emit_array_reduce_value_bool_local_instance(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_reduce_runtime_value_bool_local_instance(
            acc,
            source,
            source_span,
            callback,
            capture_local,
            module,
        );
    };
    for index in 0..len {
        module.body().line(&format!("local.get ${}", capture_local));
        module.body().line(&format!("local.get ${}", acc));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_payload_i64");
        module.body().line("i64.const 0");
        module.body().line("i64.ne");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

fn emit_array_reduce_runtime_value_bool_local_instance(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Bool)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over runtime-length bool arrays requires bool value-cell storage",
        ));
    }
    let index = module.next_label("array_reduce_bool_instance_index");
    let done_label = module.next_label("array_reduce_bool_instance_done");
    let loop_label = module.next_label("array_reduce_bool_instance_loop");
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
    module.body().line(&format!("local.get ${}", acc));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_i64");
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

pub(super) fn emit_array_reduce_value_truthy_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(kinds) = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec()) else {
        return emit_array_reduce_runtime_value_truthy_local(acc, source, source_span, callback, module);
    };
    let cell = module.next_label("array_reduce_truthy_cell");
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    for (index, kind) in kinds.iter().copied().enumerate() {
        module.body().line(&format!("local.get ${}", acc));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", cell));
        emit_value_cell_pointer_truthiness(&cell, kind, module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

pub(super) fn emit_array_reduce_runtime_value_truthy_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(kind) = module.array_runtime_value_cell_kind(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() bool callbacks over runtime arrays require scalar value-cell metadata",
        ));
    };
    if module.array_layout(source) != ArrayLayout::Value
        || !matches!(
            kind,
            ValueCellKind::Int | ValueCellKind::Bool | ValueCellKind::Float | ValueCellKind::Str
        )
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() bool callbacks over runtime arrays require scalar value-cell storage",
        ));
    }
    let index = module.next_label("array_reduce_truthy_index");
    let cell = module.next_label("array_reduce_truthy_cell");
    let done_label = module.next_label("array_reduce_truthy_done");
    let loop_label = module.next_label("array_reduce_truthy_loop");
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
    module.body().line(&format!("local.get ${}", acc));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
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

pub(super) fn emit_array_reduce_value_float_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_reduce_runtime_value_float_local(acc, source, source_span, callback, module);
    };
    for index in 0..len {
        module.body().line(&format!("local.get ${}", acc));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_payload_f64");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

pub(super) fn emit_array_reduce_runtime_value_float_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Float)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over runtime-length float arrays requires float value-cell storage",
        ));
    }
    let index = module.next_label("array_reduce_float_index");
    let done_label = module.next_label("array_reduce_float_done");
    let loop_label = module.next_label("array_reduce_float_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
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
    module.body().line("call $__rt_value_payload_f64");
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

pub(super) fn emit_array_reduce_value_float_local_instance(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_reduce_runtime_value_float_local_instance(
            acc,
            source,
            source_span,
            callback,
            capture_local,
            module,
        );
    };
    for index in 0..len {
        module.body().line(&format!("local.get ${}", capture_local));
        module.body().line(&format!("local.get ${}", acc));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_payload_f64");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

fn emit_array_reduce_runtime_value_float_local_instance(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Float)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over runtime-length float arrays requires float value-cell storage",
        ));
    }
    let index = module.next_label("array_reduce_float_instance_index");
    let done_label = module.next_label("array_reduce_float_instance_done");
    let loop_label = module.next_label("array_reduce_float_instance_loop");
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
    module.body().line(&format!("local.get ${}", acc));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_f64");
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

pub(super) fn emit_array_reduce_value_int_as_float_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_reduce_runtime_value_int_as_float_local(acc, source, source_span, callback, module);
    };
    let value_kinds = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec());
    for index in 0..len {
        module.body().line(&format!("local.get ${}", acc));
        if value_kinds
            .as_ref()
            .and_then(|kinds| kinds.get(index))
            == Some(&ValueCellKind::Str)
        {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("i32.const 8");
            module.body().line("call $__rt_value_payload_i32");
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("i32.const 12");
            module.body().line("call $__rt_value_payload_i32");
            emit_stack_string_numeric_float_arg("array_reduce_value_string_float", module);
        } else {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("call $__rt_value_payload_i64");
            module.body().line("f64.convert_i64_s");
        }
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

pub(super) fn emit_array_reduce_runtime_value_int_as_float_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || !matches!(
            module.array_runtime_value_cell_kind(source),
            Some(ValueCellKind::Int | ValueCellKind::Bool | ValueCellKind::Str)
        )
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() float callbacks over runtime arrays require integer or bool value-cell storage",
        ));
    }
    let value_kind = module.array_runtime_value_cell_kind(source).unwrap_or(ValueCellKind::Null);
    let index = module.next_label("array_reduce_int_float_index");
    let done_label = module.next_label("array_reduce_int_float_done");
    let loop_label = module.next_label("array_reduce_int_float_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}", acc));
    if value_kind == ValueCellKind::Str {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("local.get {}", index));
        module.body().line("i32.const 8");
        module.body().line("call $__rt_value_payload_i32");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("local.get {}", index));
        module.body().line("i32.const 12");
        module.body().line("call $__rt_value_payload_i32");
        emit_stack_string_numeric_float_arg("array_reduce_runtime_value_string_float", module);
    } else {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("local.get {}", index));
        module.body().line("call $__rt_value_payload_i64");
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

pub(super) fn emit_array_reduce_value_string_local(
    acc_ptr: &str,
    acc_len: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(len) = module.array_length(source) {
        for index in 0..len {
            module.body().line(&format!("local.get ${}", acc_ptr));
            module.body().line(&format!("local.get ${}", acc_len));
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("i32.const 8");
            module.body().line("call $__rt_value_payload_i32");
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("i32.const 12");
            module.body().line("call $__rt_value_payload_i32");
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(callback)));
            module.body().line(&format!("local.set ${}", acc_len));
            module.body().line(&format!("local.set ${}", acc_ptr));
        }
        return Ok(());
    }
    emit_array_reduce_runtime_value_string_local(acc_ptr, acc_len, source, source_span, callback, module)
}

pub(super) fn emit_array_reduce_value_string_local_instance(
    acc_ptr: &str,
    acc_len: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_reduce_runtime_value_string_local_instance(
            acc_ptr,
            acc_len,
            source,
            source_span,
            callback,
            capture_local,
            module,
        );
    };
    for index in 0..len {
        module.body().line(&format!("local.get ${}", capture_local));
        module.body().line(&format!("local.get ${}", acc_ptr));
        module.body().line(&format!("local.get ${}", acc_len));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 8");
        module.body().line("call $__rt_value_payload_i32");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 12");
        module.body().line("call $__rt_value_payload_i32");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc_len));
        module.body().line(&format!("local.set ${}", acc_ptr));
    }
    Ok(())
}

pub(super) fn emit_array_reduce_runtime_value_string_local(
    acc_ptr: &str,
    acc_len: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Str)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over runtime-length string arrays requires string value-cell storage",
        ));
    }
    let index = module.next_label("array_reduce_string_index");
    let done_label = module.next_label("array_reduce_string_done");
    let loop_label = module.next_label("array_reduce_string_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
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
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
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

fn emit_array_reduce_runtime_value_string_local_instance(
    acc_ptr: &str,
    acc_len: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Str)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over runtime-length string arrays requires string value-cell storage",
        ));
    }
    let index = module.next_label("array_reduce_string_instance_index");
    let done_label = module.next_label("array_reduce_string_instance_done");
    let loop_label = module.next_label("array_reduce_string_instance_loop");
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
    module.body().line(&format!("local.get ${}", acc_ptr));
    module.body().line(&format!("local.get ${}", acc_len));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
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

pub(super) fn emit_array_reduce_value_object_string_local(
    acc_ptr: &str,
    acc_len: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(classes) = module.array_object_classes(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() object callbacks require object value-cell metadata",
        ));
    };
    if classes.is_empty() || classes.iter().any(Option::is_none) {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() object callbacks require exact object value-cell metadata",
        ));
    }
    let index = module.next_label("array_reduce_object_index");
    let cell = module.next_label("array_reduce_object_cell");
    let done_label = module.next_label("array_reduce_object_done");
    let loop_label = module.next_label("array_reduce_object_loop");
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
    emit_value_cell_address_for_local(source, &index, &cell, module);
    module.body().line(&format!("local.get ${}", acc_ptr));
    module.body().line(&format!("local.get ${}", acc_len));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_OBJECT));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
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

pub(super) fn emit_array_reduce_value_dynamic_string_local(
    acc_ptr: &str,
    acc_len: &str,
    source: &str,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let index = module.next_label("array_reduce_dynamic_string_index");
    let cell = module.next_label("array_reduce_dynamic_string_cell");
    let done_label = module.next_label("array_reduce_dynamic_string_done");
    let loop_label = module.next_label("array_reduce_dynamic_string_loop");
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
    module.body().line(&format!("local.get ${}", acc_ptr));
    module.body().line(&format!("local.get ${}", acc_len));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
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
