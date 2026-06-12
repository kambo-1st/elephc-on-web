//! Purpose:
//! Emits wasm32-web array_map lowering for six-source array_map assignments.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_map_multi_emitters`.
//!
//! Key details:
//! - Keeps this arity-specific lowering isolated from native codegen.
//! - Supports compact integer, string value-cell, and mixed value-cell arrays.
//! - Mixed lowering preserves PHP null-fill semantics by mapping to the max source length.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_array_map_six_compact_int_locals_assign(
    name: &str,
    first: &str,
    second: &str,
    third: &str,
    fourth: &str,
    fifth: &str,
    sixth: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if [first, second, third, fourth, fifth, sixth].iter().any(|source| {
        module.local_kind(source) != Some(LocalKind::Array) || module.array_layout(source) != ArrayLayout::CompactInt
    }) {
        return Err(CompileError::new(
            source_span,
            "wasm32-web six-array array_map() currently requires compact integer arrays",
        ));
    }
    let (Some(first_len), Some(second_len), Some(third_len), Some(fourth_len), Some(fifth_len), Some(sixth_len)) = (
        module.array_length(first),
        module.array_length(second),
        module.array_length(third),
        module.array_length(fourth),
        module.array_length(fifth),
        module.array_length(sixth),
    ) else {
        return emit_array_map_six_runtime_compact_int_locals_assign(
            name, first, second, third, fourth, fifth, sixth, callback, module,
        );
    };
    if first_len != second_len
        || first_len != third_len
        || first_len != fourth_len
        || first_len != fifth_len
        || first_len != sixth_len
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web six-array array_map() currently rejects unequal lengths until PHP null-fill semantics are implemented",
        ));
    }
    module.set_array_length(name, first_len);
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", first_len));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", first_len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..first_len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        for source in [first, second, third, fourth, fifth, sixth] {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index * 8));
            module.body().line("i32.add");
            module.body().line("i64.load");
        }
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("i64.store");
    }
    Ok(())
}

fn emit_array_map_six_runtime_compact_int_locals_assign(
    name: &str,
    first: &str,
    second: &str,
    third: &str,
    fourth: &str,
    fifth: &str,
    sixth: &str,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let index = module.next_label("array_map_six_index");
    let done_label = module.next_label("array_map_six_done");
    let loop_label = module.next_label("array_map_six_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_nested_value_metadata(name, None);
    for source in [second, third, fourth, fifth, sixth] {
        module.body().line(&format!("local.get ${}_len", first));
        module.body().line(&format!("local.get ${}_len", source));
        module.body().line("i32.ne");
        module.body().open("if");
        module.body().line("unreachable");
        module.body().close("end");
    }
    module.body().line(&format!("local.get ${}_len", first));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", first));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", first));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    for source in [first, second, third, fourth, fifth, sixth] {
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

pub(in crate::codegen::wasm::expr) fn emit_array_map_six_value_string_locals_assign(
    name: &str,
    first: &str,
    second: &str,
    third: &str,
    fourth: &str,
    fifth: &str,
    sixth: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if [first, second, third, fourth, fifth, sixth].iter().any(|source| {
        module.local_kind(source) != Some(LocalKind::Array)
            || module.array_layout(source) != ArrayLayout::Value
            || !array_map_value_cells_are_strings(source, module)
    }) {
        return Err(CompileError::new(
            source_span,
            "wasm32-web six-array array_map() currently requires string value-cell arrays",
        ));
    }
    let (Some(first_len), Some(second_len), Some(third_len), Some(fourth_len), Some(fifth_len), Some(sixth_len)) = (
        module.array_length(first),
        module.array_length(second),
        module.array_length(third),
        module.array_length(fourth),
        module.array_length(fifth),
        module.array_length(sixth),
    ) else {
        return emit_array_map_six_runtime_value_string_locals_assign(
            name, first, second, third, fourth, fifth, sixth, callback, module,
        );
    };
    if first_len != second_len
        || first_len != third_len
        || first_len != fourth_len
        || first_len != fifth_len
        || first_len != sixth_len
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web six-array array_map() currently rejects unequal lengths until PHP null-fill semantics are implemented",
        ));
    }
    module.set_array_length(name, first_len);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Str; first_len]));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("i32.const {}", first_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", first_len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..first_len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        for source in [first, second, third, fourth, fifth, sixth] {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("i32.const 8");
            module.body().line("call $__rt_value_payload_i32");
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("i32.const 12");
            module.body().line("call $__rt_value_payload_i32");
        }
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("call $__rt_value_store_string");
    }
    Ok(())
}

fn emit_array_map_six_runtime_value_string_locals_assign(
    name: &str,
    first: &str,
    second: &str,
    third: &str,
    fourth: &str,
    fifth: &str,
    sixth: &str,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let index = module.next_label("array_map_six_string_index");
    let result_cell = module.next_label("array_map_six_string_result_cell");
    let done_label = module.next_label("array_map_six_string_done");
    let loop_label = module.next_label("array_map_six_string_loop");
    for local in [&index, &result_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Str));
    module.set_array_nested_value_metadata(name, None);
    for source in [second, third, fourth, fifth, sixth] {
        module.body().line(&format!("local.get ${}_len", first));
        module.body().line(&format!("local.get ${}_len", source));
        module.body().line("i32.ne");
        module.body().open("if");
        module.body().line("unreachable");
        module.body().close("end");
    }
    module.body().line(&format!("local.get ${}_len", first));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", first));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", first));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(name, &index, &result_cell, module);
    module.body().line(&format!("local.get {}", result_cell));
    for source in [first, second, third, fourth, fifth, sixth] {
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

pub(in crate::codegen::wasm::expr) fn emit_array_map_six_mixed_locals_assign(
    name: &str,
    first: &str,
    second: &str,
    third: &str,
    fourth: &str,
    fifth: &str,
    sixth: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    for source in [first, second, third, fourth, fifth, sixth] {
        if module.local_kind(source) != Some(LocalKind::Array)
            || !matches!(module.array_layout(source), ArrayLayout::CompactInt | ArrayLayout::Value | ArrayLayout::Assoc)
        {
            return Err(CompileError::new(
                source_span,
                "wasm32-web mixed six-array array_map() currently requires compact/value/associative arrays",
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
                "wasm32-web mixed six-array array_map() currently requires int, string, bool, or float callback results",
            ));
        }
    };
    let result_len = module.next_label("array_map_mixed_six_len");
    let index = module.next_label("array_map_mixed_six_index");
    let result_cell = module.next_label("array_map_mixed_six_result_cell");
    let first_arg = module.next_label("array_map_mixed_six_first_arg");
    let second_arg = module.next_label("array_map_mixed_six_second_arg");
    let third_arg = module.next_label("array_map_mixed_six_third_arg");
    let fourth_arg = module.next_label("array_map_mixed_six_fourth_arg");
    let fifth_arg = module.next_label("array_map_mixed_six_fifth_arg");
    let sixth_arg = module.next_label("array_map_mixed_six_sixth_arg");
    let source_cell = module.next_label("array_map_mixed_six_source_cell");
    for local in [
        &result_len,
        &index,
        &result_cell,
        &first_arg,
        &second_arg,
        &third_arg,
        &fourth_arg,
        &fifth_arg,
        &sixth_arg,
        &source_cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_nested_value_metadata(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(result_kind));
    if let (
        Some(first_len),
        Some(second_len),
        Some(third_len),
        Some(fourth_len),
        Some(fifth_len),
        Some(sixth_len),
    ) = (
        module.array_length(first),
        module.array_length(second),
        module.array_length(third),
        module.array_length(fourth),
        module.array_length(fifth),
        module.array_length(sixth),
    ) {
        let len = first_len
            .max(second_len)
            .max(third_len)
            .max(fourth_len)
            .max(fifth_len)
            .max(sixth_len);
        module.set_array_length(name, len);
        module.set_array_value_cell_kinds(name, Some(vec![result_kind; len]));
        module.body().line(&format!("i32.const {}", len));
    } else {
        module.clear_array_length(name);
        module.set_array_value_cell_kinds(name, None);
        module.body().line(&format!("local.get ${}_len", first));
        module.body().line(&format!("local.get ${}_len", second));
        module.body().line("i32.gt_u");
        module.body().open("if (result i32)");
        module.body().line(&format!("local.get ${}_len", first));
        module.body().close("else");
        module.body().line(&format!("local.get ${}_len", second));
        module.body().close("end");
        module.body().line(&format!("local.set {}", result_len));
        for source in [third, fourth, fifth, sixth] {
            module.body().line(&format!("local.get {}", result_len));
            module.body().line(&format!("local.get ${}_len", source));
            module.body().line("i32.gt_u");
            module.body().open("if (result i32)");
            module.body().line(&format!("local.get {}", result_len));
            module.body().close("else");
            module.body().line(&format!("local.get ${}_len", source));
            module.body().close("end");
            module.body().line(&format!("local.set {}", result_len));
        }
        module.body().line(&format!("local.get {}", result_len));
    }
    module.body().line(&format!("local.set {}", result_len));
    module.body().line(&format!("local.get {}", result_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", result_len));
    module.body().line(&format!("local.set ${}_len", name));
    emit_alloc_mixed_cell(first_arg.trim_start_matches('$'), module);
    emit_alloc_mixed_cell(second_arg.trim_start_matches('$'), module);
    emit_alloc_mixed_cell(third_arg.trim_start_matches('$'), module);
    emit_alloc_mixed_cell(fourth_arg.trim_start_matches('$'), module);
    emit_alloc_mixed_cell(fifth_arg.trim_start_matches('$'), module);
    emit_alloc_mixed_cell(sixth_arg.trim_start_matches('$'), module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    let loop_label = module.next_label("array_map_mixed_six_loop");
    let done_label = module.next_label("array_map_mixed_six_done");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", result_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(name, &index, &result_cell, module);
    module.body().line(&format!("local.get {}", result_cell));
    emit_array_map_two_mixed_arg_cell(first, &index, &first_arg, &source_cell, module);
    emit_array_map_two_mixed_arg_cell(second, &index, &second_arg, &source_cell, module);
    emit_array_map_two_mixed_arg_cell(third, &index, &third_arg, &source_cell, module);
    emit_array_map_two_mixed_arg_cell(fourth, &index, &fourth_arg, &source_cell, module);
    emit_array_map_two_mixed_arg_cell(fifth, &index, &fifth_arg, &source_cell, module);
    emit_array_map_two_mixed_arg_cell(sixth, &index, &sixth_arg, &source_cell, module);
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    match result_kind {
        ValueCellKind::Int => module.body().line("call $__rt_value_store_int"),
        ValueCellKind::Str => module.body().line("call $__rt_value_store_string"),
        ValueCellKind::Bool => module.body().line("call $__rt_value_store_bool"),
        ValueCellKind::Float => module.body().line("call $__rt_value_store_float"),
        _ => unreachable!("mixed six-array array_map() result kind was checked"),
    }
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    for arg in [
        &first_arg,
        &second_arg,
        &third_arg,
        &fourth_arg,
        &fifth_arg,
        &sixth_arg,
    ] {
        module.body().line(&format!("local.get {}", arg));
        module.body().line("call $__rt_value_release");
    }
    Ok(())
}
