//! Purpose:
//! Emits wasm32-web array_map lowering for two-source array assignments.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_map_multi_emitters`.
//!
//! Key details:
//! - Handles compact integer arrays and string value-cell arrays.
//! - Runtime-length sources trap on unequal lengths until PHP null-fill semantics are implemented.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_array_map_two_compact_int_locals_assign(
    name: &str,
    left: &str,
    right: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.local_kind(left) != Some(LocalKind::Array)
        || module.local_kind(right) != Some(LocalKind::Array)
        || module.array_layout(left) != ArrayLayout::CompactInt
        || module.array_layout(right) != ArrayLayout::CompactInt
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web multi-array array_map() currently requires compact integer arrays",
        ));
    }
    let (Some(left_len), Some(right_len)) = (module.array_length(left), module.array_length(right)) else {
        return emit_array_map_two_runtime_compact_int_locals_assign(name, left, right, callback, module);
    };
    if left_len != right_len {
        return Err(CompileError::new(
            source_span,
            "wasm32-web multi-array array_map() currently rejects unequal lengths until PHP null-fill semantics are implemented",
        ));
    }
    module.set_array_length(name, left_len);
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", left_len));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", left_len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..left_len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line(&format!("local.get ${}_ptr", left));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line("i64.load");
        module.body().line(&format!("local.get ${}_ptr", right));
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

pub(in crate::codegen::wasm::expr) fn emit_array_map_two_runtime_compact_int_locals_assign(
    name: &str,
    left: &str,
    right: &str,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let index = module.next_label("array_map_pair_index");
    let done_label = module.next_label("array_map_pair_done");
    let loop_label = module.next_label("array_map_pair_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", left));
    module.body().line(&format!("local.get ${}_len", right));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get ${}_len", left));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", left));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", left));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    for source in [left, right] {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("local.get {}", index));
        module.body().line("i32.const 8");
        module.body().line("i32.mul");
        module.body().line("i32.add");
        module.body().line("i64.load");
    }
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

pub(in crate::codegen::wasm::expr) fn emit_array_map_two_value_string_locals_assign(
    name: &str,
    left: &str,
    right: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.local_kind(left) != Some(LocalKind::Array)
        || module.local_kind(right) != Some(LocalKind::Array)
        || module.array_layout(left) != ArrayLayout::Value
        || module.array_layout(right) != ArrayLayout::Value
        || !array_map_value_cells_are_strings(left, module)
        || !array_map_value_cells_are_strings(right, module)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web multi-array array_map() currently requires string value-cell arrays",
        ));
    }
    let (Some(left_len), Some(right_len)) = (module.array_length(left), module.array_length(right)) else {
        return emit_array_map_two_runtime_value_string_locals_assign(name, left, right, callback, module);
    };
    if left_len != right_len {
        return Err(CompileError::new(
            source_span,
            "wasm32-web multi-array array_map() currently rejects unequal lengths until PHP null-fill semantics are implemented",
        ));
    }
    module.set_array_length(name, left_len);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Str; left_len]));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", left_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", left_len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..left_len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.get ${}_ptr", left));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 8");
        module.body().line("call $__rt_value_payload_i32");
        module.body().line(&format!("local.get ${}_ptr", left));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 12");
        module.body().line("call $__rt_value_payload_i32");
        module.body().line(&format!("local.get ${}_ptr", right));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 8");
        module.body().line("call $__rt_value_payload_i32");
        module.body().line(&format!("local.get ${}_ptr", right));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 12");
        module.body().line("call $__rt_value_payload_i32");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("call $__rt_value_store_string");
    }
    Ok(())
}

pub(in crate::codegen::wasm::expr) fn emit_array_map_two_mixed_locals_assign(
    name: &str,
    left: &str,
    right: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    for source in [left, right] {
        if module.local_kind(source) != Some(LocalKind::Array)
            || !matches!(module.array_layout(source), ArrayLayout::CompactInt | ArrayLayout::Value | ArrayLayout::Assoc)
        {
            return Err(CompileError::new(
                source_span,
                "wasm32-web mixed multi-array array_map() currently requires compact/value/associative arrays",
            ));
        }
    }
    let result_kind = match module.function_return_kind(callback) {
        Some(ValueKind::Int) => ValueCellKind::Int,
        Some(ValueKind::Str) => ValueCellKind::Str,
        Some(ValueKind::Bool) => ValueCellKind::Bool,
        Some(ValueKind::Float) => ValueCellKind::Float,
        _ => {
            return Err(CompileError::new(
                source_span,
                "wasm32-web mixed multi-array array_map() currently requires int, string, bool, or float callback results",
            ));
        }
    };
    let result_len = module.next_label("array_map_mixed_pair_len");
    let index = module.next_label("array_map_mixed_pair_index");
    let result_cell = module.next_label("array_map_mixed_pair_result_cell");
    let left_arg = module.next_label("array_map_mixed_pair_left_arg");
    let right_arg = module.next_label("array_map_mixed_pair_right_arg");
    let source_cell = module.next_label("array_map_mixed_pair_source_cell");
    for local in [&result_len, &index, &result_cell, &left_arg, &right_arg, &source_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_nested_value_metadata(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(result_kind));
    if let (Some(left_len), Some(right_len)) = (module.array_length(left), module.array_length(right)) {
        let len = left_len.max(right_len);
        module.set_array_length(name, len);
        module.set_array_value_cell_kinds(name, Some(vec![result_kind; len]));
        module.body().line(&format!("i32.const {}", len));
    } else {
        module.clear_array_length(name);
        module.set_array_value_cell_kinds(name, None);
        module.body().line(&format!("local.get ${}_len", left));
        module.body().line(&format!("local.get ${}_len", right));
        module.body().line("i32.gt_u");
        module.body().open("if (result i32)");
        module.body().line(&format!("local.get ${}_len", left));
        module.body().close("else");
        module.body().line(&format!("local.get ${}_len", right));
        module.body().close("end");
    }
    module.body().line(&format!("local.set {}", result_len));
    module.body().line(&format!("local.get {}", result_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", result_len));
    module.body().line(&format!("local.set ${}_len", name));
    emit_alloc_mixed_cell(left_arg.trim_start_matches('$'), module);
    emit_alloc_mixed_cell(right_arg.trim_start_matches('$'), module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    let loop_label = module.next_label("array_map_mixed_pair_loop");
    let done_label = module.next_label("array_map_mixed_pair_done");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", result_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(name, &index, &result_cell, module);
    module.body().line(&format!("local.get {}", result_cell));
    emit_array_map_two_mixed_arg_cell(left, &index, &left_arg, &source_cell, module);
    emit_array_map_two_mixed_arg_cell(right, &index, &right_arg, &source_cell, module);
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    match result_kind {
        ValueCellKind::Int => module.body().line("call $__rt_value_store_int"),
        ValueCellKind::Str => module.body().line("call $__rt_value_store_string"),
        ValueCellKind::Bool => module.body().line("call $__rt_value_store_bool"),
        ValueCellKind::Float => module.body().line("call $__rt_value_store_float"),
        _ => unreachable!("mixed array_map() result kind was checked"),
    }
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", left_arg));
    module.body().line("call $__rt_value_release");
    module.body().line(&format!("local.get {}", right_arg));
    module.body().line("call $__rt_value_release");
    Ok(())
}

pub(in crate::codegen::wasm::expr) fn emit_array_map_two_mixed_arg_cell(
    source: &str,
    index: &str,
    arg_cell: &str,
    source_cell: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", arg_cell));
    module.body().line("call $__rt_value_release");
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.lt_u");
    module.body().open("if");
    match module.array_layout(source) {
        ArrayLayout::CompactInt => {
            module.body().line(&format!("local.get {}", arg_cell));
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get {}", index));
            module.body().line("i32.const 8");
            module.body().line("i32.mul");
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("call $__rt_value_store_int");
        }
        ArrayLayout::Value => {
            emit_value_cell_address_for_local(source, index, source_cell, module);
            emit_copy_value_cell_from_addr_to_addr(arg_cell, source_cell, module);
        }
        ArrayLayout::Assoc => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get {}", index));
            module.body().line("call $__rt_assoc_entry");
            module.body().line("call $__rt_assoc_value_cell");
            module.body().line(&format!("local.set {}", source_cell));
            emit_copy_value_cell_from_addr_to_addr(arg_cell, source_cell, module);
        }
    }
    module.body().close("else");
    module.body().line(&format!("local.get {}", arg_cell));
    module.body().line("call $__rt_value_store_null");
    module.body().close("end");
    module.body().line(&format!("local.get {}", arg_cell));
}

pub(in crate::codegen::wasm::expr) fn emit_array_map_two_runtime_value_string_locals_assign(
    name: &str,
    left: &str,
    right: &str,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let index = module.next_label("array_map_pair_string_index");
    let cell = module.next_label("array_map_pair_string_cell");
    let done_label = module.next_label("array_map_pair_string_done");
    let loop_label = module.next_label("array_map_pair_string_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Str));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", left));
    module.body().line(&format!("local.get ${}_len", right));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get ${}_len", left));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", left));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", left));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(name, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    for source in [left, right] {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("local.get {}", index));
        module.body().line("i32.const 8");
        module.body().line("call $__rt_value_payload_i32");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("local.get {}", index));
        module.body().line("i32.const 12");
        module.body().line("call $__rt_value_payload_i32");
    }
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("call $__rt_value_store_string");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}
