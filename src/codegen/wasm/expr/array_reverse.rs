//! Purpose:
//! Lowers wasm32-web array_reverse() assignments that need preserve-key handling.
//! Keeps reverse-specific metadata and copy loops separate from other array transforms.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets`.
//!
//! Key details:
//! - Preserves PHP key reindexing, value-cell copies, nested metadata, and runtime COW-safe source pointers.

use super::*;
use super::array_transform_sets::emit_assoc_entry_address;

pub(super) fn emit_assoc_array_reverse_assign(
    name: &str,
    source: &str,
    _span: crate::span::Span,
    preserve_keys: bool,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let reversed_key_kinds = module
        .array_key_kinds(source)
        .map(|key_kinds| key_kinds.iter().copied().rev().collect::<Vec<_>>());
    let reversed_key_values = module.array_key_values(source).and_then(|values| {
        let key_kinds = module.array_key_kinds(source)?;
        let key_kinds = key_kinds.iter().copied().rev().collect::<Vec<_>>();
        let key_values = values.iter().cloned().rev().collect::<Vec<_>>();
        if preserve_keys {
            Some(key_values)
        } else {
            Some(reindexed_assoc_key_values(&key_kinds, &key_values))
        }
    });
    let reversed_value_kinds = module
        .array_value_cell_kinds(source)
        .map(|kinds| kinds.iter().copied().rev().collect::<Vec<_>>());
    let reversed_value_constants = module
        .array_value_constants(source)
        .map(|values| values.iter().cloned().rev().collect::<Vec<_>>());
    let reversed_nested_values = module
        .array_nested_value_metadata_items(source)
        .map(|metadata| metadata.iter().cloned().rev().collect::<Vec<_>>());
    if let Some(len) = module.array_length(source) {
        module.set_array_length(name, len);
    }
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_key_kinds(name, reversed_key_kinds);
    module.set_array_key_values(name, reversed_key_values);
    module.set_array_runtime_key_kind(name, module.array_runtime_key_kind(source));
    module.set_array_php_normalized_runtime_keys(
        name,
        module.array_has_php_normalized_runtime_keys(source),
    );
    module.set_array_value_cell_kinds(name, reversed_value_kinds);
    module.set_array_nested_value_metadata(name, reversed_nested_values);
    module.set_array_runtime_value_cell_kind(name, module.array_runtime_value_cell_kind(source));
    module.set_array_runtime_nested_value_metadata(name, module.array_runtime_nested_value_metadata(source));
    module.set_array_value_constants(name, reversed_value_constants);
    let source_ptr = module.next_label("assoc_array_reverse_source_ptr");
    let source_len = module.next_label("assoc_array_reverse_source_len");
    let index = module.next_label("assoc_array_reverse_index");
    let source_index = module.next_label("assoc_array_reverse_source_index");
    let next_int_key = module.next_label("assoc_array_reverse_next_int_key");
    let target_entry = module.next_label("assoc_array_reverse_target_entry");
    let source_entry = module.next_label("assoc_array_reverse_source_entry");
    let done_label = module.next_label("assoc_array_reverse_done");
    let loop_label = module.next_label("assoc_array_reverse_loop");
    for local in [
        &source_ptr,
        &source_len,
        &index,
        &source_index,
        &target_entry,
        &source_entry,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(next_int_key.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.set {}", source_ptr));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", source_len));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().line("i64.const 0");
    module.body().line(&format!("local.set {}", next_int_key));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", source_index));
    emit_assoc_entry_address(&format!("${}_ptr", name), &index, &target_entry, module);
    emit_assoc_entry_address(&source_ptr, &source_index, &source_entry, module);
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    if preserve_keys {
        copy_assoc_entry_key(&target_entry, &source_entry, module);
    } else {
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.store");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", next_int_key));
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", next_int_key));
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", next_int_key));
    }
    module.body().line("else");
    copy_assoc_entry_key(&target_entry, &source_entry, module);
    module.body().close("end");
    copy_assoc_entry_value(&target_entry, &source_entry, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_indexed_array_reverse_preserve_keys_assign(
    name: &str,
    source: &str,
    source_layout: ArrayLayout,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let len = module.array_length(source);
    module.set_array_layout(name, ArrayLayout::Assoc);
    if let Some(len) = len {
        module.set_array_length(name, len);
    }
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    module.set_array_php_normalized_runtime_keys(name, false);
    if let Some(len) = len {
        module.set_array_key_kinds(name, Some(vec![AssocKeyKind::Int; len]));
        module.set_array_key_values(
            name,
            Some((0..len).rev().map(|index| AssocKeyValue::Int(index as i64)).collect()),
        );
        match source_layout {
            ArrayLayout::CompactInt => {
                module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Int; len]));
            }
            ArrayLayout::Value => {
                module.set_array_value_cell_kinds(
                    name,
                    module
                        .array_value_cell_kinds(source)
                        .map(|kinds| kinds.iter().copied().rev().collect()),
                );
                module.set_array_value_constants(
                    name,
                    module
                        .array_value_constants(source)
                        .map(|values| values.iter().cloned().rev().collect()),
                );
                module.set_array_nested_value_metadata(
                    name,
                    module
                        .array_nested_value_metadata_items(source)
                        .map(|items| items.iter().cloned().rev().collect()),
                );
            }
            ArrayLayout::Assoc => {}
        }
    } else if source_layout == ArrayLayout::CompactInt {
        module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Int));
    } else {
        module.set_array_runtime_value_cell_kind(name, module.array_runtime_value_cell_kind(source));
        module.set_array_runtime_nested_value_metadata(
            name,
            module.array_runtime_nested_value_metadata(source),
        );
    }

    let source_ptr = preserve_array_ptr(source, "array_reverse_preserve_keys", module);
    let source_len = module.next_label("array_reverse_preserve_source_len");
    let index = module.next_label("array_reverse_preserve_index");
    let source_index = module.next_label("array_reverse_preserve_source_index");
    let target_entry = module.next_label("array_reverse_preserve_target_entry");
    let target_cell = module.next_label("array_reverse_preserve_target_cell");
    let source_cell = module.next_label("array_reverse_preserve_source_cell");
    let done_label = module.next_label("array_reverse_preserve_done");
    let loop_label = module.next_label("array_reverse_preserve_loop");
    for local in [
        &source_len,
        &index,
        &source_index,
        &target_entry,
        &target_cell,
        &source_cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", source_len));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.store");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i64.extend_i32_u");
    module.body().line("i64.store");
    emit_assoc_value_cell_address(&target_entry, &target_cell, module);
    match source_layout {
        ArrayLayout::CompactInt => {
            module.body().line(&format!("local.get {}", target_cell));
            module.body().line(&format!("i64.const {}", WASM_VALUE_TAG_INT));
            module.body().line("i64.store");
            module.body().line(&format!("local.get {}", target_cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line(&format!("local.get {}", source_ptr));
            module.body().line(&format!("local.get {}", source_index));
            module.body().line("i32.const 8");
            module.body().line("i32.mul");
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("i64.store");
        }
        ArrayLayout::Value => {
            module.body().line(&format!("local.get {}", source_ptr));
            module.body().line(&format!("local.get {}", source_index));
            module.body().line("call $__rt_value_cell");
            module.body().line(&format!("local.set {}", source_cell));
            module.body().line(&format!("local.get {}", target_cell));
            module.body().line(&format!("local.get {}", source_cell));
            module.body().line("call $__rt_value_copy");
        }
        ArrayLayout::Assoc => unreachable!("indexed preserve-key reverse requires indexed source"),
    }
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}
