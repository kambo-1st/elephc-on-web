//! Purpose:
//! Emits wasm32-web array_filter(strlen(...)) lowering for associative arrays.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_map_filter_assoc`.
//!
//! Key details:
//! - Preserves associative key metadata while filtering by PHP strlen-style truthiness.
//! - Handles both known-length metadata and runtime-length homogeneous associative arrays.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_array_filter_assoc_strlen_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter(strlen(...)) over associative arrays requires known length metadata",
        ));
    };
    let Some(value_kinds) = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec()) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter(strlen(...)) over associative arrays requires value-cell metadata",
        ));
    };
    if !value_kinds
        .iter()
        .all(|kind| array_filter_strlen_value_kind_is_supported(*kind))
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter(strlen(...)) over associative arrays requires scalar string-coercible values",
        ));
    }
    let key_kinds = module.array_key_kinds(source).map(|kinds| kinds.to_vec());
    let key_values = module.array_key_values(source).map(|keys| keys.to_vec());
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_value_cell_kinds(name, Some(value_kinds.clone()));
    module.set_array_key_kinds(name, key_kinds);
    module.set_array_runtime_key_kind(name, module.array_runtime_key_kind(source));
    module.set_array_php_normalized_runtime_keys(
        name,
        module.array_has_php_normalized_runtime_keys(source),
    );
    module.set_array_key_values(name, key_values);
    let source_entry = module.next_label("array_filter_assoc_strlen_source_entry");
    let target_entry = module.next_label("array_filter_assoc_strlen_target_entry");
    let value_cell = module.next_label("array_filter_assoc_strlen_value_cell");
    for local in [&source_entry, &target_entry, &value_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for (index, kind) in value_kinds.iter().copied().enumerate() {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", source_entry));
        emit_assoc_value_cell_address(&source_entry, &value_cell, module);
        emit_value_cell_pointer_strlen_truthiness(&value_cell, kind, module);
        module.body().open("if");
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("local.get {}", out_index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", target_entry));
        copy_assoc_entry(&target_entry, &source_entry, module);
        emit_array_filter_increment_len(name, &out_index, module);
        module.body().close("end");
    }
    Ok(())
}

pub(in crate::codegen::wasm::expr) fn emit_array_filter_runtime_assoc_strlen_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(value_kind) = module.array_runtime_value_cell_kind(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter(strlen(...)) over runtime associative arrays requires homogeneous value metadata",
        ));
    };
    if !array_filter_strlen_value_kind_is_supported(value_kind) {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter(strlen(...)) over runtime associative arrays requires scalar string-coercible values",
        ));
    }
    let out_index = module.next_label("array_filter_assoc_strlen_runtime_out_index");
    let index = module.next_label("array_filter_assoc_strlen_runtime_index");
    let source_entry = module.next_label("array_filter_assoc_strlen_runtime_source_entry");
    let target_entry = module.next_label("array_filter_assoc_strlen_runtime_target_entry");
    let value_cell = module.next_label("array_filter_assoc_strlen_runtime_value_cell");
    for local in [&out_index, &index, &source_entry, &target_entry, &value_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.clear_array_length(name);
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    module.set_array_runtime_key_kind(name, module.array_runtime_key_kind(source));
    module.set_array_php_normalized_runtime_keys(
        name,
        module.array_has_php_normalized_runtime_keys(source),
    );
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(value_kind));
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    let loop_label = module.next_label("array_filter_assoc_strlen_runtime_loop");
    let done_label = module.next_label("array_filter_assoc_strlen_runtime_done");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", source_entry));
    emit_assoc_value_cell_address(&source_entry, &value_cell, module);
    emit_value_cell_pointer_strlen_truthiness(&value_cell, value_kind, module);
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    copy_assoc_entry(&target_entry, &source_entry, module);
    emit_array_filter_increment_len(name, &out_index, module);
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
