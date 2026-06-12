//! Purpose:
//! Emits array_filter() callback-mode loops for indexed wasm32-web arrays.
//! Keeps value/key/both callback argument modes separate from scalar truthiness filters.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_filter_assign` through the wasm expression module.
//!
//! Key details:
//! - Preserves result key metadata while copying compact-int and value-cell entries.
//! - Runtime-length paths validate storage layout before emitting loops.

use super::*;

pub(super) fn emit_array_filter_compact_int_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_filter_runtime_compact_int_local_assign(
            name,
            source,
            source_span,
            callback,
            module,
        );
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Int; len]));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    let value = module.next_label("array_filter_value");
    module.declare_i64_local(value.trim_start_matches('$').to_string());
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line("i64.load");
        module.body().line(&format!("local.set {}", value));
        module.body().line(&format!("local.get {}", value));
        emit_array_filter_int_predicate(callback, module);
        module.body().open("if");
        emit_array_filter_store_int_entry(name, &out_index, index, &value, module);
        module.body().close("end");
    }
    Ok(())
}

pub(super) fn emit_array_filter_compact_int_local_instance_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_filter_runtime_compact_int_local_instance_assign(
            name,
            source,
            source_span,
            callback,
            capture_local,
            module,
        );
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Int; len]));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    let value = module.next_label("array_filter_instance_value");
    module.declare_i64_local(value.trim_start_matches('$').to_string());
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line("i64.load");
        module.body().line(&format!("local.set {}", value));
        module.body().line(&format!("local.get ${}", capture_local));
        module.body().line(&format!("local.get {}", value));
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        emit_array_filter_predicate_result_is_true(callback, module);
        module.body().open("if");
        emit_array_filter_store_int_entry(name, &out_index, index, &value, module);
        module.body().close("end");
    }
    Ok(())
}

pub(super) fn emit_array_filter_compact_int_key_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_filter_runtime_compact_int_key_local_assign(
            name,
            source,
            source_span,
            callback,
            module,
        );
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Int; len]));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    let value = module.next_label("array_filter_key_value");
    module.declare_i64_local(value.trim_start_matches('$').to_string());
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line("i64.load");
        module.body().line(&format!("local.set {}", value));
        module.body().line(&format!("i64.const {}", index));
        emit_array_filter_int_predicate(callback, module);
        module.body().open("if");
        emit_array_filter_store_int_entry(name, &out_index, index, &value, module);
        module.body().close("end");
    }
    Ok(())
}

pub(super) fn emit_array_filter_compact_int_both_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_filter_runtime_compact_int_both_local_assign(
            name,
            source,
            source_span,
            callback,
            module,
        );
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Int; len]));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    let value = module.next_label("array_filter_both_value");
    module.declare_i64_local(value.trim_start_matches('$').to_string());
    let mixed_callback = array_filter_use_both_value_param_is_mixed(callback, module);
    let mixed_arg = module.next_label("array_filter_both_compact_mixed_arg");
    if mixed_callback {
        module.declare_i32_local(mixed_arg.trim_start_matches('$').to_string());
        emit_alloc_mixed_cell(mixed_arg.trim_start_matches('$'), module);
    }
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line("i64.load");
        module.body().line(&format!("local.set {}", value));
        if mixed_callback {
            module.body().line(&format!("local.get {}", mixed_arg));
            module.body().line("call $__rt_value_release");
            module.body().line(&format!("local.get {}", mixed_arg));
            module.body().line(&format!("local.get {}", value));
            module.body().line("call $__rt_value_store_int");
            module.body().line(&format!("local.get {}", mixed_arg));
        } else {
            module.body().line(&format!("local.get {}", value));
        }
        module.body().line(&format!("i64.const {}", index));
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        emit_array_filter_predicate_result_is_true(callback, module);
        module.body().open("if");
        emit_array_filter_store_int_entry(name, &out_index, index, &value, module);
        module.body().close("end");
    }
    if mixed_callback {
        module.body().line(&format!("local.get {}", mixed_arg));
        module.body().line("call $__rt_value_release");
    }
    Ok(())
}

pub(super) fn emit_array_filter_value_key_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_filter_runtime_value_key_local_assign(
            name,
            source,
            source_span,
            callback,
            kind,
            module,
        );
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_value_cell_kinds(name, Some(vec![kind; len]));
    module.set_array_runtime_value_cell_kind(name, Some(kind));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    for index in 0..len {
        module.body().line(&format!("i64.const {}", index));
        emit_array_filter_int_predicate(callback, module);
        module.body().open("if");
        emit_array_filter_copy_value_entry(name, source, &out_index, index, module);
        module.body().close("end");
    }
    Ok(())
}

pub(super) fn emit_array_filter_value_both_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_filter_runtime_value_both_local_assign(
            name,
            source,
            source_span,
            callback,
            kind,
            module,
        );
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_value_cell_kinds(name, Some(vec![kind; len]));
    module.set_array_runtime_value_cell_kind(name, Some(kind));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    let string_to_float_callback = array_filter_use_both_value_param_is_float(callback, module)
        && kind == ValueCellKind::Str;
    let mixed_arg = if array_filter_use_both_value_param_is_mixed(callback, module) {
        let arg = module.next_label("array_filter_both_mixed_arg");
        let source_cell = module.next_label("array_filter_both_source_cell");
        for local in [&arg, &source_cell] {
            module.declare_i32_local(local.trim_start_matches('$').to_string());
        }
        emit_alloc_mixed_cell(arg.trim_start_matches('$'), module);
        Some((arg, source_cell))
    } else {
        None
    };
    for index in 0..len {
        if let Some((arg, source_cell)) = &mixed_arg {
            module.body().line(&format!("local.get {}", arg));
            module.body().line("call $__rt_value_release");
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("call $__rt_value_cell");
            module.body().line(&format!("local.set {}", source_cell));
            emit_copy_value_cell_from_addr_to_addr(arg, source_cell, module);
            module.body().line(&format!("local.get {}", arg));
        } else if string_to_float_callback {
            emit_array_filter_value_cell_arg(source, index, ValueCellKind::Str, module);
            emit_stack_string_numeric_float_arg("array_filter_both_string_float", module);
        } else {
            emit_array_filter_value_cell_arg(source, index, kind, module);
        }
        module.body().line(&format!("i64.const {}", index));
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        emit_array_filter_predicate_result_is_true(callback, module);
        module.body().open("if");
        emit_array_filter_copy_value_entry(name, source, &out_index, index, module);
        module.body().close("end");
    }
    if let Some((arg, _)) = &mixed_arg {
        module.body().line(&format!("local.get {}", arg));
        module.body().line("call $__rt_value_release");
    }
    Ok(())
}

pub(super) fn emit_array_filter_runtime_compact_int_key_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::CompactInt {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter() over runtime-length arrays currently requires compact integer storage",
        ));
    }
    let out_index = emit_array_filter_runtime_result_prelude(name, source, module);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Int));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    let value = module.next_label("array_filter_key_value");
    let index = module.next_label("array_filter_key_index");
    let done_label = module.next_label("array_filter_key_done");
    let loop_label = module.next_label("array_filter_key_loop");
    module.declare_i64_local(value.trim_start_matches('$').to_string());
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
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", value));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i64.extend_i32_u");
    emit_array_filter_int_predicate(callback, module);
    module.body().open("if");
    emit_array_filter_store_int_entry_dynamic_key(name, &out_index, &index, &value, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_filter_runtime_compact_int_both_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::CompactInt {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter() over runtime-length arrays currently requires compact integer storage",
        ));
    }
    let out_index = emit_array_filter_runtime_result_prelude(name, source, module);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Int));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    let value = module.next_label("array_filter_both_value");
    let index = module.next_label("array_filter_both_index");
    let mixed_arg = module.next_label("array_filter_both_compact_runtime_mixed_arg");
    let done_label = module.next_label("array_filter_both_done");
    let loop_label = module.next_label("array_filter_both_loop");
    module.declare_i64_local(value.trim_start_matches('$').to_string());
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    let mixed_callback = array_filter_use_both_value_param_is_mixed(callback, module);
    if mixed_callback {
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
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", value));
    if mixed_callback {
        module.body().line(&format!("local.get {}", mixed_arg));
        module.body().line("call $__rt_value_release");
        module.body().line(&format!("local.get {}", mixed_arg));
        module.body().line(&format!("local.get {}", value));
        module.body().line("call $__rt_value_store_int");
        module.body().line(&format!("local.get {}", mixed_arg));
    } else {
        module.body().line(&format!("local.get {}", value));
    }
    module.body().line(&format!("local.get {}", index));
    module.body().line("i64.extend_i32_u");
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    emit_array_filter_predicate_result_is_true(callback, module);
    module.body().open("if");
    emit_array_filter_store_int_entry_dynamic_key(name, &out_index, &index, &value, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    if mixed_callback {
        module.body().line(&format!("local.get {}", mixed_arg));
        module.body().line("call $__rt_value_release");
    }
    Ok(())
}

pub(super) fn emit_array_filter_runtime_value_key_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(kind)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web ARRAY_FILTER_USE_KEY over runtime value arrays requires homogeneous scalar value cells",
        ));
    }
    let out_index = emit_array_filter_runtime_result_prelude(name, source, module);
    module.set_array_runtime_value_cell_kind(name, Some(kind));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    let index = module.next_label("array_filter_value_key_index");
    let done_label = module.next_label("array_filter_value_key_done");
    let loop_label = module.next_label("array_filter_value_key_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i64.extend_i32_u");
    emit_array_filter_int_predicate(callback, module);
    module.body().open("if");
    emit_array_filter_copy_value_entry_dynamic_key(name, source, &out_index, &index, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_filter_runtime_value_both_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(kind)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web ARRAY_FILTER_USE_BOTH over runtime value arrays requires matching scalar value cells",
        ));
    }
    let out_index = emit_array_filter_runtime_result_prelude(name, source, module);
    module.set_array_runtime_value_cell_kind(name, Some(kind));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    let string_to_float_callback = array_filter_use_both_value_param_is_float(callback, module)
        && kind == ValueCellKind::Str;
    let index = module.next_label("array_filter_value_both_index");
    let mixed_arg = module.next_label("array_filter_value_both_mixed_arg");
    let source_cell = module.next_label("array_filter_value_both_source_cell");
    let done_label = module.next_label("array_filter_value_both_done");
    let loop_label = module.next_label("array_filter_value_both_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    let mixed_callback = array_filter_use_both_value_param_is_mixed(callback, module);
    if mixed_callback {
        module.declare_i32_local(mixed_arg.trim_start_matches('$').to_string());
        module.declare_i32_local(source_cell.trim_start_matches('$').to_string());
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
    if mixed_callback {
        module.body().line(&format!("local.get {}", mixed_arg));
        module.body().line("call $__rt_value_release");
        emit_value_cell_address_for_local(source, &index, &source_cell, module);
        emit_copy_value_cell_from_addr_to_addr(&mixed_arg, &source_cell, module);
        module.body().line(&format!("local.get {}", mixed_arg));
    } else if string_to_float_callback {
        emit_array_filter_value_cell_arg_dynamic(source, &index, ValueCellKind::Str, module);
        emit_stack_string_numeric_float_arg("array_filter_both_runtime_string_float", module);
    } else {
        emit_array_filter_value_cell_arg_dynamic(source, &index, kind, module);
    }
    module.body().line(&format!("local.get {}", index));
    module.body().line("i64.extend_i32_u");
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    emit_array_filter_predicate_result_is_true(callback, module);
    module.body().open("if");
    emit_array_filter_copy_value_entry_dynamic_key(name, source, &out_index, &index, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    if mixed_callback {
        module.body().line(&format!("local.get {}", mixed_arg));
        module.body().line("call $__rt_value_release");
    }
    Ok(())
}

fn array_filter_use_both_value_param_is_float(callback: &str, module: &WasmModule) -> bool {
    module
        .function_param_kinds(callback)
        .and_then(|kinds| kinds.first().copied())
        == Some(LocalKind::F64)
}

fn array_filter_use_both_value_param_is_mixed(callback: &str, module: &WasmModule) -> bool {
    module
        .function_param_kinds(callback)
        .and_then(|kinds| kinds.first().copied())
        == Some(LocalKind::Mixed)
}

fn emit_array_filter_runtime_compact_int_local_instance_assign(
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
            "wasm32-web array_filter() over runtime-length arrays currently requires compact integer storage",
        ));
    }
    let out_index = emit_array_filter_runtime_result_prelude(name, source, module);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Int));
    let value = module.next_label("array_filter_instance_value");
    let index = module.next_label("array_filter_instance_index");
    let done_label = module.next_label("array_filter_instance_done");
    let loop_label = module.next_label("array_filter_instance_loop");
    module.declare_i64_local(value.trim_start_matches('$').to_string());
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
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", value));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get {}", value));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    emit_array_filter_predicate_result_is_true(callback, module);
    module.body().open("if");
    emit_array_filter_store_int_entry_dynamic_key(name, &out_index, &index, &value, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_filter_runtime_compact_int_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::CompactInt {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter() over runtime-length arrays currently requires compact integer storage",
        ));
    }
    let out_index = emit_array_filter_runtime_result_prelude(name, source, module);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Int));
    let value = module.next_label("array_filter_value");
    let index = module.next_label("array_filter_index");
    let done_label = module.next_label("array_filter_done");
    let loop_label = module.next_label("array_filter_loop");
    module.declare_i64_local(value.trim_start_matches('$').to_string());
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
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", value));
    module.body().line(&format!("local.get {}", value));
    emit_array_filter_int_predicate(callback, module);
    module.body().open("if");
    emit_array_filter_store_int_entry_dynamic_key(name, &out_index, &index, &value, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_filter_value_int_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_filter_runtime_value_int_local_assign(
            name,
            source,
            source_span,
            callback,
            module,
        );
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Int; len]));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_payload_i64");
        emit_array_filter_int_predicate(callback, module);
        module.body().open("if");
        emit_array_filter_copy_value_entry(name, source, &out_index, index, module);
        module.body().close("end");
    }
    Ok(())
}

pub(super) fn emit_array_filter_runtime_value_int_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Int)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter() over runtime-length integer arrays requires integer value-cell storage",
        ));
    }
    let out_index = emit_array_filter_runtime_result_prelude(name, source, module);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Int));
    let index = module.next_label("array_filter_value_int_index");
    let done_label = module.next_label("array_filter_value_int_done");
    let loop_label = module.next_label("array_filter_value_int_loop");
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
    emit_array_filter_int_predicate(callback, module);
    module.body().open("if");
    emit_array_filter_copy_value_entry_dynamic_key(name, source, &out_index, &index, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}
