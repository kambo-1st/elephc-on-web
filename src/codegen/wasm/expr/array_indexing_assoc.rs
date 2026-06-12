//! Purpose:
//! Emits wasm32-web associative array scalar reads for static int and string keys.
//!
//! Called from:
//! - `super::array_indexing::emit_array_index_expr()` and nested index helpers.
//!
//! Key details:
//! - Runtime scans compare PHP-normalized int/string keys against assoc entries.
//! - Missing keys materialize the current wasm scalar null/zero fallback contract.

use super::*;

pub(super) fn emit_assoc_array_index_string_expr(
    name: &str,
    key_value: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if assoc_object_class_for_static_key(name, &AssocKeyValue::Str(key_value.to_string()), module).is_some() {
        let (key_ptr, key_len) = module.intern_string(key_value);
        let key_ptr_local = module.next_label("assoc_array_expr_key_ptr");
        let key_len_local = module.next_label("assoc_array_expr_key_len");
        module.declare_i32_local(key_ptr_local.trim_start_matches('$').to_string());
        module.declare_i32_local(key_len_local.trim_start_matches('$').to_string());
        module.body().line(&format!("i32.const {}", key_ptr));
        module.body().line(&format!("local.set {}", key_ptr_local));
        module.body().line(&format!("i32.const {}", key_len));
        module.body().line(&format!("local.set {}", key_len_local));
        return emit_assoc_array_object_index_string_parts_expr(name, &key_ptr_local, &key_len_local, module);
    }
    let kind = assoc_value_kind_for_static_key(name, &AssocKeyValue::Str(key_value.to_string()), module)
        .or_else(|| assoc_runtime_scan_value_kind(name, module))
        .ok_or_else(|| {
            CompileError::new(
                crate::span::Span::dummy(),
                "wasm32-web associative scalar access requires known key metadata",
            )
        })?;
    let (key_ptr, key_len) = module.intern_string(key_value);
    let key_ptr_local = module.next_label("assoc_array_expr_key_ptr");
    let key_len_local = module.next_label("assoc_array_expr_key_len");
    module.declare_i32_local(key_ptr_local.trim_start_matches('$').to_string());
    module.declare_i32_local(key_len_local.trim_start_matches('$').to_string());
    module.body().line(&format!("i32.const {}", key_ptr));
    module.body().line(&format!("local.set {}", key_ptr_local));
    module.body().line(&format!("i32.const {}", key_len));
    module.body().line(&format!("local.set {}", key_len_local));
    emit_assoc_array_index_string_parts_expr(name, &key_ptr_local, &key_len_local, kind, module)
}

pub(super) fn emit_assoc_array_index_int_expr(
    name: &str,
    key_value: i64,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if assoc_object_class_for_static_key(name, &AssocKeyValue::Int(key_value), module).is_some() {
        let key = module.next_label("assoc_array_expr_key");
        module.declare_i64_local(key.trim_start_matches('$').to_string());
        module.body().line(&format!("i64.const {}", key_value));
        module.body().line(&format!("local.set {}", key));
        return emit_assoc_array_object_index_int_parts_expr(name, &key, module);
    }
    let kind = assoc_value_kind_for_static_key(name, &AssocKeyValue::Int(key_value), module)
        .or_else(|| assoc_runtime_scan_value_kind(name, module))
        .ok_or_else(|| {
            CompileError::new(
                crate::span::Span::dummy(),
                "wasm32-web associative scalar access requires known key metadata",
            )
        })?;
    let key = module.next_label("assoc_array_expr_key");
    module.declare_i64_local(key.trim_start_matches('$').to_string());
    module.body().line(&format!("i64.const {}", key_value));
    module.body().line(&format!("local.set {}", key));
    emit_assoc_array_index_int_parts_expr(name, &key, kind, module)
}

pub(super) fn assoc_runtime_scan_value_kind(name: &str, module: &WasmModule) -> Option<ValueCellKind> {
    if module.array_key_values(name).is_some() {
        return None;
    }
    if let Some(kind) = module.array_runtime_value_cell_kind(name) {
        return Some(kind);
    }
    let kinds = module.array_value_cell_kinds(name)?;
    let first = kinds.first().copied()?;
    kinds.iter().all(|kind| *kind == first).then_some(first)
}

pub(super) fn assoc_value_kind_for_static_key(
    name: &str,
    key: &AssocKeyValue,
    module: &WasmModule,
) -> Option<ValueCellKind> {
    let Some(keys) = module.array_key_values(name) else {
        return None;
    };
    let Some(values) = module.array_value_cell_kinds(name) else {
        return None;
    };
    Some(
        keys.iter()
            .zip(values.iter().copied())
            .rev()
            .find_map(|(candidate, kind)| (candidate == key).then_some(kind))
            .unwrap_or(ValueCellKind::Null),
    )
}

fn assoc_object_class_for_static_key(
    name: &str,
    key: &AssocKeyValue,
    module: &WasmModule,
) -> Option<String> {
    let keys = module.array_key_values(name)?;
    keys.iter()
        .enumerate()
        .rev()
        .find_map(|(index, candidate)| {
            (candidate == key)
                .then(|| module.array_object_class(name, index))
                .flatten()
        })
}

fn emit_assoc_array_object_index_int_parts_expr(
    name: &str,
    key: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let entry = module.next_label("assoc_array_object_expr_entry");
    let entry_base = module.next_label("assoc_array_object_expr_entry_base");
    let cell = module.next_label("assoc_array_object_expr_cell");
    let found = module.next_label("assoc_array_object_expr_found");
    let done_label = module.next_label("assoc_array_object_expr_done");
    let loop_label = module.next_label("assoc_array_object_expr_loop");
    for local in [&entry, &entry_base, &cell, &found] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", entry));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry_base));
    module.body().line(&format!("local.get {}", entry_base));
    module.body().line(&format!("local.get {}", key));
    module.body().line("call $__rt_assoc_key_eq_int");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("local.get {}", entry_base));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    emit_assoc_object_result_from_cell(&cell, &found, module);
    Ok(ValueKind::Object)
}

fn emit_assoc_array_object_index_string_parts_expr(
    name: &str,
    key_ptr: &str,
    key_len: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let entry = module.next_label("assoc_array_object_expr_entry");
    let entry_base = module.next_label("assoc_array_object_expr_entry_base");
    let cell = module.next_label("assoc_array_object_expr_cell");
    let found = module.next_label("assoc_array_object_expr_found");
    let done_label = module.next_label("assoc_array_object_expr_done");
    let loop_label = module.next_label("assoc_array_object_expr_loop");
    for local in [&entry, &entry_base, &cell, &found] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", entry));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", entry));
    module
        .body()
        .line(&format!("i32.const {}", WASM_ASSOC_ENTRY_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", entry_base));
    module.body().line(&format!("local.get {}", entry_base));
    module.body().line(&format!("local.get {}", key_ptr));
    module.body().line(&format!("local.get {}", key_len));
    module.body().line("call $__rt_assoc_key_eq_php_string");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("local.get {}", entry_base));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    emit_assoc_object_result_from_cell(&cell, &found, module);
    Ok(ValueKind::Object)
}

fn emit_assoc_object_result_from_cell(cell: &str, found: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", found));
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line("else");
    module.body().line("i32.const 0");
    module.body().close("end");
}

pub(super) fn emit_assoc_array_index_int_parts_expr(
    name: &str,
    key: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let entry = module.next_label("assoc_array_expr_entry");
    let entry_base = module.next_label("assoc_array_expr_entry_base");
    let cell = module.next_label("assoc_array_expr_cell");
    let found = module.next_label("assoc_array_expr_found");
    let done_label = module.next_label("assoc_array_expr_done");
    let loop_label = module.next_label("assoc_array_expr_loop");
    for local in [&entry, &entry_base, &cell, &found] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", entry));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry_base));
    module.body().line(&format!("local.get {}", entry_base));
    module.body().line(&format!("local.get {}", key));
    module.body().line("call $__rt_assoc_key_eq_int");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("local.get {}", entry_base));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    emit_assoc_scalar_result_from_cell(&cell, &found, kind, module)
}

pub(super) fn emit_assoc_array_index_string_parts_expr(
    name: &str,
    key_ptr: &str,
    key_len: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let entry = module.next_label("assoc_array_expr_entry");
    let entry_base = module.next_label("assoc_array_expr_entry_base");
    let cell = module.next_label("assoc_array_expr_cell");
    let found = module.next_label("assoc_array_expr_found");
    let done_label = module.next_label("assoc_array_expr_done");
    let loop_label = module.next_label("assoc_array_expr_loop");
    for local in [&entry, &entry_base, &cell, &found] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", entry));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", entry));
    module
        .body()
        .line(&format!("i32.const {}", WASM_ASSOC_ENTRY_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", entry_base));
    module.body().line(&format!("local.get {}", entry_base));
    module.body().line(&format!("local.get {}", key_ptr));
    module.body().line(&format!("local.get {}", key_len));
    module.body().line("call $__rt_assoc_key_eq_php_string");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("local.get {}", entry_base));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    emit_assoc_scalar_result_from_cell(&cell, &found, kind, module)
}

pub(super) fn emit_assoc_scalar_result_from_cell(
    cell: &str,
    found: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    match kind {
        ValueCellKind::Int => {
            module.body().line(&format!("local.get {}", found));
            module.body().open("if (result i64)");
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("else");
            module.body().line("i64.const 0");
            module.body().close("end");
            Ok(ValueKind::Int)
        }
        ValueCellKind::Float => {
            module.body().line(&format!("local.get {}", found));
            module.body().open("if (result f64)");
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("f64.load");
            module.body().line("else");
            module.body().line("f64.const 0");
            module.body().close("end");
            Ok(ValueKind::Float)
        }
        ValueCellKind::Bool => {
            module.body().line(&format!("local.get {}", found));
            module.body().open("if (result i32)");
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
            module.body().line("else");
            module.body().line("i32.const 0");
            module.body().close("end");
            Ok(ValueKind::Bool)
        }
        ValueCellKind::Str | ValueCellKind::Array => {
            module.body().line(&format!("local.get {}", found));
            module.body().open("if (result i32 i32)");
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line("else");
            module.body().line("i32.const 0");
            module.body().line("i32.const 0");
            module.body().close("end");
            Ok(if kind == ValueCellKind::Str {
                ValueKind::Str
            } else {
                ValueKind::Array
            })
        }
        ValueCellKind::Null => {
            module.body().line("i32.const 0");
            Ok(ValueKind::Null)
        }
    }
}
