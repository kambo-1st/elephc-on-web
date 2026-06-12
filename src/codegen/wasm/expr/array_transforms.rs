//! Purpose:
//! Lowers wasm32-web array reindexing and transform helpers.
//! Keeps array_reverse(), array_values(), array_keys(), and static/dynamic transform loops out of expr.rs.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets`
//!
//! Key details:
//! - Helpers preserve array metadata, PHP key normalization, value-cell copies, and runtime-length transform loops.

use super::*;
use super::array_transform_sets::{
    emit_assoc_entry_address, emit_assoc_value_cell_address, emit_value_cell_address,
};

pub(super) fn emit_assoc_array_values_assign(
    name: &str,
    source: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(len) = module.array_length(source) {
        module.set_array_length(name, len);
    }
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(
        name,
        module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec()),
    );
    module.set_array_nested_value_metadata(
        name,
        module
            .array_nested_value_metadata_items(source)
            .map(|metadata| metadata.to_vec()),
    );
    module.set_array_runtime_value_cell_kind(name, module.array_runtime_value_cell_kind(source));
    module.set_array_runtime_nested_value_metadata(name, module.array_runtime_nested_value_metadata(source));
    module.set_array_value_constants(
        name,
        module.array_value_constants(source).map(|values| values.to_vec()),
    );
    let source_ptr = preserve_array_ptr(source, "assoc_array_values", module);
    let source_len = module.next_label("assoc_array_values_source_len");
    let index = module.next_label("assoc_array_values_index");
    let target_cell = module.next_label("assoc_array_values_target_cell");
    let source_cell = module.next_label("assoc_array_values_source_cell");
    let done_label = module.next_label("assoc_array_values_done");
    let loop_label = module.next_label("assoc_array_values_loop");
    for local in [&source_len, &index, &target_cell, &source_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", source_len));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", source_len));
    module
        .body()
        .line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
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
    emit_value_cell_address(&format!("${}_ptr", name), &index, &target_cell, module);
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line("i32.const 0");
    module.body().line("i32.store");
    emit_assoc_entry_address(&source_ptr, &index, &source_cell, module);
    emit_assoc_value_cell_address(&source_cell, &source_cell, module);
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get {}", source_cell));
    module.body().line("call $__rt_value_copy");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_assoc_array_keys_assign(
    name: &str,
    source: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(len) = module.array_length(source) {
        module.set_array_length(name, len);
    }
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(
        name,
        module.array_key_kinds(source).map(|kinds| {
            kinds
                .iter()
                .map(|kind| match kind {
                    AssocKeyKind::Int => ValueCellKind::Int,
                    AssocKeyKind::Str => ValueCellKind::Str,
                })
                .collect()
        }),
    );
    if module.array_key_kinds(source).is_none() {
        if module.array_has_php_normalized_runtime_keys(source) {
            module.set_array_runtime_value_cell_kind(name, None);
        } else {
            module.set_array_runtime_value_cell_kind(
                name,
                module.array_runtime_key_kind(source).map(|kind| match kind {
                    AssocKeyKind::Int => ValueCellKind::Int,
                    AssocKeyKind::Str => ValueCellKind::Str,
                }),
            );
        }
    }
    module.set_array_value_constants(name, assoc_key_constants_for_source(source, module));
    let source_ptr = preserve_array_ptr(source, "assoc_array_keys", module);
    let source_len = module.next_label("assoc_array_keys_source_len");
    let index = module.next_label("assoc_array_keys_index");
    let target_cell = module.next_label("assoc_array_keys_target_cell");
    let source_entry = module.next_label("assoc_array_keys_source_entry");
    let done_label = module.next_label("assoc_array_keys_done");
    let loop_label = module.next_label("assoc_array_keys_loop");
    for local in [&source_len, &index, &target_cell, &source_entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", source_len));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", source_len));
    module
        .body()
        .line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
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
    emit_value_cell_address(&format!("${}_ptr", name), &index, &target_cell, module);
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line("i32.const 0");
    module.body().line("i32.store");
    emit_assoc_entry_address(&source_ptr, &index, &source_entry, module);
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line("call $__rt_value_store_string");
    module.body().line("else");
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("call $__rt_value_store_int");
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

pub(super) fn emit_runtime_indexed_array_transform_assign(
    name: &str,
    source_ptr: &str,
    len: usize,
    function_name: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("global.get $heap");
    module.body().line(&format!("i32.const {}", len * 8));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..len {
        let source_index = if function_name.eq_ignore_ascii_case("array_reverse") {
            len - 1 - index
        } else {
            index
        };
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        if function_name.eq_ignore_ascii_case("array_keys") {
            module.body().line(&format!("i64.const {}", index));
        } else {
            module.body().line(&format!("local.get {}", source_ptr));
            module.body().line(&format!("i32.const {}", source_index * 8));
            module.body().line("i32.add");
            module.body().line("i64.load");
        }
        module.body().line("i64.store");
    }
    Ok(())
}

pub(super) fn emit_dynamic_indexed_array_transform_assign(
    name: &str,
    source: &Expr,
    function_name: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.set_array_layout(name, ArrayLayout::CompactInt);
    if let ExprKind::Variable(source_name) = &source.kind {
        if let Some(len) = module.array_length(source_name) {
            module.set_array_length(name, len);
        }
    } else if let ExprKind::FunctionCall { name: function_name, .. } = &source.kind {
        if let Some(len) = module.function_array_return_length(function_name) {
            module.set_array_length(name, len);
        }
    } else if let ExprKind::MethodCall { object, method, .. }
        | ExprKind::NullsafeMethodCall { object, method, .. } = &source.kind
    {
        if let Some(len) = method_call_compact_int_array_return_len(object, method, module) {
            module.set_array_length(name, len);
        }
    }
    let source_ptr = module.next_label("array_transform_source_ptr");
    let source_len = module.next_label("array_transform_source_len");
    let index = module.next_label("array_transform_index");
    let source_index = module.next_label("array_transform_source_index");
    let done_label = module.next_label("array_transform_done");
    let loop_label = module.next_label("array_transform_loop");
    for local in [&source_ptr, &source_len, &index, &source_index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_array_value_to_stack(source, module)?;
    module.body().line(&format!("local.set {}", source_len));
    module.body().line(&format!("local.set {}", source_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
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
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    if function_name.eq_ignore_ascii_case("array_keys") {
        module.body().line(&format!("local.get {}", index));
        module.body().line("i64.extend_i32_u");
    } else {
        if function_name.eq_ignore_ascii_case("array_reverse") {
            module.body().line(&format!("local.get {}", source_len));
            module.body().line("i32.const 1");
            module.body().line("i32.sub");
            module.body().line(&format!("local.get {}", index));
            module.body().line("i32.sub");
            module.body().line(&format!("local.set {}", source_index));
        } else {
            module.body().line(&format!("local.get {}", index));
            module.body().line(&format!("local.set {}", source_index));
        }
        module.body().line(&format!("local.get {}", source_ptr));
        module.body().line(&format!("local.get {}", source_index));
        module.body().line("i32.const 8");
        module.body().line("i32.mul");
        module.body().line("i32.add");
        module.body().line("i64.load");
    }
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

pub(super) fn emit_dynamic_value_array_transform_assign(
    name: &str,
    source: &Expr,
    transform_name: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let source_ptr = module.next_label("value_array_transform_source_ptr");
    let source_len = module.next_label("value_array_transform_source_len");
    let index = module.next_label("value_array_transform_index");
    let source_index = module.next_label("value_array_transform_source_index");
    let done_label = module.next_label("value_array_transform_done");
    let loop_label = module.next_label("value_array_transform_loop");
    for local in [&source_ptr, &source_len, &index, &source_index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.set_array_layout(name, ArrayLayout::Value);
    if let ExprKind::Variable(source_name) = &source.kind {
        module.set_array_runtime_value_cell_kind(name, module.array_runtime_value_cell_kind(source_name));
    }
    if let ExprKind::Variable(source_name) = &source.kind {
        if let Some(len) = module.array_length(source_name) {
            module.set_array_length(name, len);
            let mut value_kinds = module
                .array_value_cell_kinds(source_name)
                .map(|kinds| kinds.to_vec());
            let mut value_constants = module
                .array_value_constants(source_name)
                .map(|values| values.to_vec());
            let mut nested_values = module
                .array_nested_value_metadata_items(source_name)
                .map(|items| items.to_vec());
            let mut object_classes = module
                .array_object_classes(source_name)
                .map(|classes| classes.to_vec());
            if transform_name.eq_ignore_ascii_case("array_reverse") {
                if let Some(kinds) = value_kinds.as_mut() {
                    kinds.reverse();
                }
                if let Some(values) = value_constants.as_mut() {
                    values.reverse();
                }
                if let Some(items) = nested_values.as_mut() {
                    items.reverse();
                }
                if let Some(classes) = object_classes.as_mut() {
                    classes.reverse();
                }
            }
            module.set_array_value_cell_kinds(name, value_kinds);
            module.set_array_value_constants(name, value_constants);
            module.set_array_nested_value_metadata(name, nested_values);
            module.set_array_object_classes(name, object_classes);
        }
    } else if let ExprKind::FunctionCall { name: function_name, .. } = &source.kind {
        if let Some(len) = module.function_array_return_length(function_name) {
            module.set_array_length(name, len);
            let mut value_kinds = module
                .function_array_return_value_kinds(function_name)
                .map(|kinds| kinds.to_vec());
            let mut nested_values = module
                .function_array_return_nested_values(function_name)
                .map(|items| items.to_vec());
            if transform_name.eq_ignore_ascii_case("array_reverse") {
                if let Some(kinds) = value_kinds.as_mut() {
                    kinds.reverse();
                }
                if let Some(items) = nested_values.as_mut() {
                    items.reverse();
                }
            }
            module.set_array_value_cell_kinds(name, value_kinds);
            module.set_array_nested_value_metadata(name, nested_values);
        }
    }
    emit_array_value_to_stack(source, module)?;
    module.body().line(&format!("local.set {}", source_len));
    module.body().line(&format!("local.set {}", source_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", source_len));
    module
        .body()
        .line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
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
    if transform_name.eq_ignore_ascii_case("array_reverse") {
        module.body().line(&format!("local.get {}", source_len));
        module.body().line("i32.const 1");
        module.body().line("i32.sub");
        module.body().line(&format!("local.get {}", index));
        module.body().line("i32.sub");
        module.body().line(&format!("local.set {}", source_index));
    } else {
        module.body().line(&format!("local.get {}", index));
        module.body().line(&format!("local.set {}", source_index));
    }
    emit_copy_value_cell(name, &source_ptr, &index, &source_index, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}
