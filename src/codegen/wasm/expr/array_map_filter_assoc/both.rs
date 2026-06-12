//! Purpose:
//! Emits wasm32-web ARRAY_FILTER_USE_BOTH lowering for associative arrays.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_map_filter_assoc`.
//!
//! Key details:
//! - Passes each callback both the value cell and associative key payload.
//! - Preserves key metadata for known-length arrays and runtime associative results.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_array_filter_assoc_both_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    callback_shape: ArrayFilterUseBothCallback,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_filter_runtime_assoc_both_local_assign(
            name,
            source,
            source_span,
            callback,
            callback_shape,
            module,
        );
    };
    let Some(value_kinds) = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec()) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web ARRAY_FILTER_USE_BOTH over associative arrays requires value-cell metadata",
        ));
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(name, Some(value_kinds.clone()));
    module.set_array_key_kinds(name, module.array_key_kinds(source).map(|kinds| kinds.to_vec()));
    module.set_array_runtime_key_kind(name, module.array_runtime_key_kind(source));
    module.set_array_php_normalized_runtime_keys(
        name,
        module.array_has_php_normalized_runtime_keys(source),
    );
    module.set_array_key_values(name, module.array_key_values(source).map(|keys| keys.to_vec()));
    module.set_array_nested_value_metadata(
        name,
        module
            .array_nested_value_metadata_items(source)
            .map(|metadata| metadata.to_vec()),
    );
    let source_entry = module.next_label("array_filter_assoc_both_source_entry");
    let target_entry = module.next_label("array_filter_assoc_both_target_entry");
    let value_cell = module.next_label("array_filter_assoc_both_value_cell");
    for local in [&source_entry, &target_entry, &value_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", source_entry));
        emit_assoc_value_cell_address(&source_entry, &value_cell, module);
        emit_array_filter_assoc_both_callback_args(
            &value_cell,
            &source_entry,
            callback_shape,
            value_kinds[index],
            module,
        );
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        emit_array_filter_predicate_result_is_true(callback, module);
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

fn emit_array_filter_runtime_assoc_both_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    callback_shape: ArrayFilterUseBothCallback,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(value_kind) = module.array_runtime_value_cell_kind(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web ARRAY_FILTER_USE_BOTH over runtime associative arrays requires homogeneous value metadata",
        ));
    };
    if !callback_shape.value_kind.map_or_else(
        || array_filter_value_cell_kind_is_supported(value_kind),
        |callback_kind| value_kind == callback_kind
            || (value_kind == ValueCellKind::Str && callback_kind == ValueCellKind::Float),
    ) {
        return Err(CompileError::new(
            source_span,
            "wasm32-web ARRAY_FILTER_USE_BOTH over runtime associative arrays requires callback-compatible values",
        ));
    }
    let out_index = module.next_label("array_filter_assoc_both_runtime_out_index");
    let index = module.next_label("array_filter_assoc_both_runtime_index");
    let source_entry = module.next_label("array_filter_assoc_both_runtime_source_entry");
    let target_entry = module.next_label("array_filter_assoc_both_runtime_target_entry");
    let value_cell = module.next_label("array_filter_assoc_both_runtime_value_cell");
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
    let loop_label = module.next_label("array_filter_assoc_both_runtime_loop");
    let done_label = module.next_label("array_filter_assoc_both_runtime_done");
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
    emit_array_filter_assoc_both_callback_args(
        &value_cell,
        &source_entry,
        callback_shape,
        value_kind,
        module,
    );
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    emit_array_filter_predicate_result_is_true(callback, module);
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
