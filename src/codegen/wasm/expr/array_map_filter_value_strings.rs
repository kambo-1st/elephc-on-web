//! Purpose:
//! Emits wasm32-web array_filter()/array_map() loops for runtime string value-cell arrays.
//! Keeps runtime string payload copying and strlen mapping out of the main filter emitter.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_map_filter`.
//! - array_map/array_filter assignment lowering helpers.
//!
//! Key details:
//! - Preserves value-cell string metadata, runtime lengths, and PHP callback truthiness.

use super::*;

pub(super) fn emit_array_filter_runtime_value_string_local_assign(
    name: &str,
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
            "wasm32-web array_filter() over runtime-length string arrays requires string value-cell storage",
        ));
    }
    let out_index = emit_array_filter_runtime_result_prelude(name, source, module);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Str));
    let index = module.next_label("array_filter_string_index");
    let done_label = module.next_label("array_filter_string_done");
    let loop_label = module.next_label("array_filter_string_loop");
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
    emit_array_filter_string_predicate(callback, module);
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

pub(super) fn emit_array_filter_runtime_value_string_local_instance_assign(
    name: &str,
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
            "wasm32-web array_filter() over runtime-length string arrays requires string value-cell storage",
        ));
    }
    let out_index = emit_array_filter_runtime_result_prelude(name, source, module);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Str));
    let index = module.next_label("array_filter_string_instance_index");
    let done_label = module.next_label("array_filter_string_instance_done");
    let loop_label = module.next_label("array_filter_string_instance_loop");
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
    Ok(())
}

pub(super) fn emit_array_map_runtime_value_string_lengths_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Str)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_map(strlen) over runtime-length string arrays requires string value-cell storage",
        ));
    }
    let index = module.next_label("array_map_strlen_index");
    let done_label = module.next_label("array_map_strlen_done");
    let loop_label = module.next_label("array_map_strlen_loop");
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
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    emit_array_map_strlen_result(module);
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
