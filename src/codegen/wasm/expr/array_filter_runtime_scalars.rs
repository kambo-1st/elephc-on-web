//! Purpose:
//! Emits runtime-length scalar value-array `array_filter` paths for wasm32-web.
//! Keeps dynamic scalar callback/type/strlen loops separate from static value-array filtering.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_map_filter`
//!
//! Key details:
//! - Requires homogeneous runtime value-cell metadata before emitting dynamic filter loops.

use super::*;
use super::array_filter_truthiness::emit_value_cell_strlen_truthiness_dynamic;

pub(super) fn emit_array_filter_runtime_value_scalar_type_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(kind)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter() with a type predicate over runtime-length scalar arrays requires homogeneous value-cell storage",
        ));
    }
    let out_index = emit_array_filter_runtime_result_prelude(name, source, module);
    module.set_array_runtime_value_cell_kind(name, Some(kind));
    let index = module.next_label("array_filter_scalar_index");
    let done_label = module.next_label("array_filter_scalar_done");
    let loop_label = module.next_label("array_filter_scalar_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_array_filter_copy_value_entry_dynamic_key(name, source, &out_index, &index, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_array_filter_runtime_value_scalar_predicate_local_assign(
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
            "wasm32-web array_filter() with a scalar callback over runtime-length scalar arrays requires homogeneous value-cell storage",
        ));
    }
    let out_index = emit_array_filter_runtime_result_prelude(name, source, module);
    module.set_array_runtime_value_cell_kind(name, Some(kind));
    let index = module.next_label("array_filter_scalar_predicate_index");
    let done_label = module.next_label("array_filter_scalar_predicate_done");
    let loop_label = module.next_label("array_filter_scalar_predicate_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_array_filter_scalar_predicate_arg_from_cell_dynamic(source, &index, callback, kind, module);
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

pub(super) fn emit_array_filter_runtime_value_scalar_strlen_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(kind)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter(strlen(...)) over runtime-length scalar arrays requires homogeneous value-cell storage",
        ));
    }
    let out_index = emit_array_filter_runtime_result_prelude(name, source, module);
    module.set_array_runtime_value_cell_kind(name, Some(kind));
    if kind == ValueCellKind::Null {
        return Ok(());
    }
    let index = module.next_label("array_filter_strlen_scalar_index");
    let done_label = module.next_label("array_filter_strlen_scalar_done");
    let loop_label = module.next_label("array_filter_strlen_scalar_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_strlen_truthiness_dynamic(source, &index, kind, module);
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

