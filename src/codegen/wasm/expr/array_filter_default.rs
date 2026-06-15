//! Purpose:
//! Emits no-callback/default array_filter() lowering for wasm32-web arrays.
//! Keeps PHP truthiness filtering separate from callback-driven array_map/filter emitters.
//!
//! Called from:
//! - `super::array_calls` through wasm expression dispatch.
//!
//! Key details:
//! - Preserves value-cell metadata, associative keys, nested array metadata, and runtime lengths.

use super::*;

pub(super) fn emit_array_filter_default_compact_int_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_filter_default_runtime_compact_int_local_assign(name, source, source_span, module);
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Int; len]));
    let value = module.next_label("array_filter_value");
    module.declare_i64_local(value.trim_start_matches('$').to_string());
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line("i64.load");
        module.body().line(&format!("local.set {}", value));
        module.body().line(&format!("local.get {}", value));
        module.body().line("i64.const 0");
        module.body().line("i64.ne");
        module.body().open("if");
        emit_array_filter_store_int_entry(name, &out_index, index, &value, module);
        module.body().close("end");
    }
    Ok(())
}
pub(super) fn emit_array_filter_default_runtime_compact_int_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
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
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
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
pub(super) fn emit_array_filter_default_value_int_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_filter_default_runtime_value_int_local_assign(name, source, source_span, module);
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Int; len]));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_payload_i64");
        module.body().line("i64.const 0");
        module.body().line("i64.ne");
        module.body().open("if");
        emit_array_filter_copy_value_entry(name, source, &out_index, index, module);
        module.body().close("end");
    }
    Ok(())
}
pub(super) fn emit_array_filter_default_runtime_value_int_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Int)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter() without a callback over runtime-length integer arrays requires integer value-cell storage",
        ));
    }
    let out_index = emit_array_filter_runtime_result_prelude(name, source, module);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Int));
    let index = module.next_label("array_filter_default_value_int_index");
    let done_label = module.next_label("array_filter_default_value_int_done");
    let loop_label = module.next_label("array_filter_default_value_int_loop");
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
pub(super) fn emit_array_filter_default_value_string_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_filter_default_runtime_value_string_local_assign(name, source, source_span, module);
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Str; len]));
    let ptr = module.next_label("array_filter_string_ptr");
    let str_len = module.next_label("array_filter_string_len");
    module.declare_i32_local(ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(str_len.trim_start_matches('$').to_string());
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 8");
        module.body().line("call $__rt_value_payload_i32");
        module.body().line(&format!("local.set {}", ptr));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("i32.const 12");
        module.body().line("call $__rt_value_payload_i32");
        module.body().line(&format!("local.set {}", str_len));
        emit_string_parts_truthiness(&ptr, &str_len, module);
        module.body().open("if");
        emit_array_filter_copy_value_entry(name, source, &out_index, index, module);
        module.body().close("end");
    }
    Ok(())
}
pub(super) fn emit_array_filter_default_value_scalar_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_filter_default_runtime_value_scalar_local_assign(
            name,
            source,
            source_span,
            kind,
            module,
        );
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_value_cell_kinds(name, Some(vec![kind; len]));
    module.set_array_runtime_value_cell_kind(name, Some(kind));
    module.set_array_key_kinds(name, Some(vec![AssocKeyKind::Int; len]));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    module.set_array_key_values(
        name,
        Some(
            (0..len)
                .map(|index| AssocKeyValue::Int(index as i64))
                .collect(),
        ),
    );
    if kind == ValueCellKind::Array {
        let nested_metadata = array_filter_default_nested_metadata_for_local(source, module);
        if let Some(metadata) = nested_metadata.as_ref() {
            module.set_array_length(name, metadata.len());
        }
        module.set_array_nested_value_metadata(name, nested_metadata);
        if let Some(keys) = array_filter_default_int_key_values_for_local(source, module) {
            module.set_array_key_kinds(
                name,
                Some(keys.iter().map(assoc_key_kind_for_value).collect()),
            );
            module.set_array_key_values(name, Some(keys));
        }
    } else {
        module.set_array_nested_value_metadata(name, None);
    }
    for index in 0..len {
        emit_value_cell_truthiness(source, index, kind, module);
        module.body().open("if");
        emit_array_filter_copy_value_entry(name, source, &out_index, index, module);
        module.body().close("end");
    }
    Ok(())
}

pub(super) fn emit_array_filter_default_value_object_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter() without a callback over object arrays requires known length metadata",
        ));
    };
    let Some(object_classes) = module.array_object_classes(source).map(|classes| classes.to_vec()) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter() without a callback over object arrays requires object metadata",
        ));
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_length(name, len);
    module.set_array_key_kinds(name, Some(vec![AssocKeyKind::Int; len]));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    module.set_array_key_values(
        name,
        Some(
            (0..len)
                .map(|index| AssocKeyValue::Int(index as i64))
                .collect(),
        ),
    );
    module.set_array_object_classes(name, Some(object_classes));
    for index in 0..len {
        emit_array_filter_copy_value_entry(name, source, &out_index, index, module);
    }
    Ok(())
}

pub(super) fn emit_array_filter_parent_value_object_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter(get_parent_class) over object arrays requires known length metadata",
        ));
    };
    let Some(object_classes) = module.array_object_classes(source).map(|classes| classes.to_vec()) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter(get_parent_class) over object arrays requires object metadata",
        ));
    };
    let mut kept = Vec::new();
    for (index, class_name) in object_classes.iter().enumerate() {
        let Some(class_name) = class_name else {
            return Err(CompileError::new(
                source_span,
                "wasm32-web array_filter(get_parent_class) over object arrays requires exact object metadata",
            ));
        };
        if module
            .object_class(class_name)
            .and_then(|class_info| class_info.parent.as_deref())
            .is_some()
        {
            kept.push((index, Some(class_name.clone())));
        }
    }
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_length(name, kept.len());
    module.set_array_key_kinds(name, Some(vec![AssocKeyKind::Int; kept.len()]));
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    module.set_array_key_values(
        name,
        Some(
            kept.iter()
                .map(|(index, _)| AssocKeyValue::Int(*index as i64))
                .collect(),
        ),
    );
    module.set_array_object_classes(
        name,
        Some(kept.iter().map(|(_, class_name)| class_name.clone()).collect()),
    );
    for (index, _) in kept {
        emit_array_filter_copy_value_entry(name, source, &out_index, index, module);
    }
    Ok(())
}

pub(super) fn emit_array_filter_empty_value_object_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter() false object predicate requires known length metadata",
        ));
    };
    if module.array_object_classes(source).is_none() {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter() false object predicate requires object metadata",
        ));
    }
    emit_array_filter_result_prelude(name, len, module);
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    module.set_array_object_classes(name, None);
    Ok(())
}

pub(super) fn emit_array_filter_default_runtime_value_scalar_local_assign(
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
            "wasm32-web array_filter() without a callback over runtime-length scalar arrays requires homogeneous value-cell storage",
        ));
    }
    let out_index = emit_array_filter_runtime_result_prelude(name, source, module);
    module.set_array_runtime_value_cell_kind(name, Some(kind));
    if kind == ValueCellKind::Null {
        return Ok(());
    }
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
    emit_value_cell_truthiness_dynamic(source, &index, kind, module);
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
pub(super) fn emit_array_filter_default_runtime_value_string_local_assign(
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
            "wasm32-web array_filter() without a callback over runtime-length string arrays requires string value-cell storage",
        ));
    }
    let out_index = emit_array_filter_runtime_result_prelude(name, source, module);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Str));
    let index = module.next_label("array_filter_string_index");
    let ptr = module.next_label("array_filter_string_ptr");
    let str_len = module.next_label("array_filter_string_len");
    let done_label = module.next_label("array_filter_string_done");
    let loop_label = module.next_label("array_filter_string_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.declare_i32_local(ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(str_len.trim_start_matches('$').to_string());
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
    module.body().line(&format!("local.set {}", ptr));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", str_len));
    emit_string_parts_truthiness(&ptr, &str_len, module);
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
pub(super) fn emit_array_filter_default_assoc_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter() without a callback over associative arrays requires known length metadata",
        ));
    };
    let Some(value_kinds) = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec()) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter() without a callback over associative arrays requires value-cell metadata",
        ));
    };
    let out_index = emit_array_filter_result_prelude(name, len, module);
    module.set_array_value_cell_kinds(name, Some(value_kinds.clone()));
    module.set_array_runtime_key_kind(name, module.array_runtime_key_kind(source));
    module.set_array_php_normalized_runtime_keys(
        name,
        module.array_has_php_normalized_runtime_keys(source),
    );
    if let Some(keys) = module.array_key_values(source).map(|keys| keys.to_vec()) {
        module.set_array_key_kinds(
            name,
            Some(keys.iter().map(assoc_key_kind_for_value).collect()),
        );
        module.set_array_key_values(name, Some(keys));
    }
    if let Some((keys, nested_values)) = array_filter_default_assoc_nested_metadata_for_local(source, module) {
        module.set_array_key_kinds(
            name,
            Some(keys.iter().map(assoc_key_kind_for_value).collect()),
        );
        module.set_array_key_values(name, Some(keys));
        module.set_array_nested_value_metadata(name, Some(nested_values));
    }
    let source_entry = module.next_label("array_filter_assoc_source_entry");
    let target_entry = module.next_label("array_filter_assoc_target_entry");
    let value_cell = module.next_label("array_filter_assoc_value_cell");
    for local in [&source_entry, &target_entry, &value_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for (index, kind) in value_kinds.iter().copied().enumerate() {
        module.body().line(&format!("i32.const {}", index));
        module.body().line(&format!("local.get ${}_len", source));
        module.body().line("i32.lt_u");
        module.body().open("if");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", source_entry));
        emit_assoc_value_cell_address(&source_entry, &value_cell, module);
        emit_value_cell_pointer_truthiness(&value_cell, kind, module);
        module.body().open("if");
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("local.get {}", out_index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", target_entry));
        copy_assoc_entry(&target_entry, &source_entry, module);
        emit_array_filter_increment_len(name, &out_index, module);
        module.body().close("end");
        module.body().close("end");
    }
    Ok(())
}
pub(super) fn emit_array_filter_default_runtime_assoc_local_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(value_kind) = module.array_runtime_value_cell_kind(source) else {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter() without a callback over runtime associative arrays requires homogeneous value metadata",
        ));
    };
    if !array_filter_value_cell_kind_is_supported(value_kind) {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_filter() without a callback over runtime associative arrays requires scalar or nested-array values",
        ));
    }
    let out_index = module.next_label("array_filter_assoc_runtime_out_index");
    let index = module.next_label("array_filter_assoc_runtime_index");
    let source_entry = module.next_label("array_filter_assoc_runtime_source_entry");
    let target_entry = module.next_label("array_filter_assoc_runtime_target_entry");
    let value_cell = module.next_label("array_filter_assoc_runtime_value_cell");
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
    if value_kind == ValueCellKind::Array {
        module.set_array_runtime_nested_value_metadata(
            name,
            module.array_runtime_nested_value_metadata(source),
        );
    } else {
        module.set_array_nested_value_metadata(name, None);
    }
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    let loop_label = module.next_label("array_filter_assoc_runtime_loop");
    let done_label = module.next_label("array_filter_assoc_runtime_done");
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
    emit_value_cell_pointer_truthiness(&value_cell, value_kind, module);
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
