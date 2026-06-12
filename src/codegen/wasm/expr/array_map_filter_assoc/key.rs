//! Purpose:
//! Emits wasm32-web ARRAY_FILTER_USE_KEY lowering for associative arrays.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_map_filter_assoc`.
//!
//! Key details:
//! - Preserves keys and nested metadata for known-length associative arrays.
//! - Runtime-length associative arrays keep dynamic key/value metadata and filter by key predicate.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_array_filter_assoc_key_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayFilterCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_filter_runtime_assoc_key_local_assign(
            name,
            source,
            source_span,
            callback,
            shape,
            module,
        );
    };
    let Some(value_kinds) = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec()) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web ARRAY_FILTER_USE_KEY over associative arrays requires value-cell metadata",
        ));
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_layout(name, ArrayLayout::Assoc);
    if shape == ArrayFilterCallbackShape::Mixed {
        module.set_array_value_cell_kinds(name, None);
    } else {
        module.set_array_value_cell_kinds(name, Some(value_kinds));
    }
    module.set_array_key_kinds(
        name,
        (shape != ArrayFilterCallbackShape::Mixed)
            .then(|| module.array_key_kinds(source).map(|kinds| kinds.to_vec()))
            .flatten(),
    );
    module.set_array_runtime_key_kind(name, module.array_runtime_key_kind(source));
    module.set_array_php_normalized_runtime_keys(
        name,
        module.array_has_php_normalized_runtime_keys(source),
    );
    module.set_array_key_values(
        name,
        (shape != ArrayFilterCallbackShape::Mixed)
            .then(|| module.array_key_values(source).map(|keys| keys.to_vec()))
            .flatten(),
    );
    module.set_array_nested_value_metadata(
        name,
        module
            .array_nested_value_metadata_items(source)
            .map(|metadata| metadata.to_vec()),
    );
    let source_entry = module.next_label("array_filter_assoc_key_source_entry");
    let target_entry = module.next_label("array_filter_assoc_key_target_entry");
    for local in [&source_entry, &target_entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", source_entry));
        emit_array_filter_assoc_key_predicate(&source_entry, callback, shape, module);
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

fn emit_array_filter_runtime_assoc_key_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayFilterCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(value_kind) = module.array_runtime_value_cell_kind(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web ARRAY_FILTER_USE_KEY over runtime associative arrays requires homogeneous value metadata",
        ));
    };
    let out_index = module.next_label("array_filter_assoc_key_runtime_out_index");
    let index = module.next_label("array_filter_assoc_key_runtime_index");
    let source_entry = module.next_label("array_filter_assoc_key_runtime_source_entry");
    let target_entry = module.next_label("array_filter_assoc_key_runtime_target_entry");
    for local in [&out_index, &index, &source_entry, &target_entry] {
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
    let loop_label = module.next_label("array_filter_assoc_key_runtime_loop");
    let done_label = module.next_label("array_filter_assoc_key_runtime_done");
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
    emit_array_filter_assoc_key_predicate(&source_entry, callback, shape, module);
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
