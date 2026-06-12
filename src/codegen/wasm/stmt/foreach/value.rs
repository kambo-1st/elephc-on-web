//! Purpose:
//! Lowers wasm32-web foreach loops over value-cell arrays.
//!
//! Called from:
//! - `crate::codegen::wasm::stmt::foreach`.
//!
//! Key details:
//! - Value-cell foreach preserves scalar, string, array, and mixed payload kinds.
//! - Nested array metadata is propagated when foreach binds an array element.

use crate::errors::CompileError;
use crate::parser::ast::Stmt;

use super::compact::emit_indexed_foreach_key_assign;
use super::super::emit_stmt;
use super::super::super::expr::{
    emit_copy_value_cell_from_addr_to_addr, emit_value_cell_address_for_local,
};
use super::super::super::module::{
    AssocKeyKind, AssocKeyValue, ArrayLayout, LocalKind, ValueCellKind, WasmModule,
};

pub(super) fn emit_value_array_foreach_by_ref(
    array: &str,
    key_var: Option<&str>,
    value_var: &str,
    body: &[Stmt],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let span = body.first().map_or(crate::span::Span::new(0, 0), |stmt| stmt.span);
    let array_len = module.array_length(array);
    if array_len == Some(0) {
        return Ok(());
    }
    let value_kind = module.local_kind(value_var);
    let cell_kind = value_cell_kind_for_value_foreach(array, module);
    match (value_kind, cell_kind) {
        (Some(LocalKind::I64), Some(ValueCellKind::Int))
        | (Some(LocalKind::F64), Some(ValueCellKind::Float))
        | (Some(LocalKind::I32), Some(ValueCellKind::Bool))
        | (Some(LocalKind::Str), Some(ValueCellKind::Str))
        | (Some(LocalKind::Array), Some(ValueCellKind::Array))
        | (Some(LocalKind::Mixed), _) => {}
        _ => {
            return Err(CompileError::new(
                span,
                "wasm32-web value-cell foreach by reference currently requires scalar or mixed cells",
            ));
        }
    }
    let index = module.next_label("value_foreach_ref_index");
    let cell = module.next_label("value_foreach_ref_cell");
    let payload = module.next_label("value_foreach_ref_payload");
    let break_label = module.next_label("value_foreach_ref_break");
    let loop_label = module.next_label("value_foreach_ref_loop");
    let continue_label = module.next_label("value_foreach_ref_continue");
    for local in [&index, &cell, &payload] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    match value_kind {
        Some(LocalKind::I64) => module.set_i64_ref_alias(value_var, &payload),
        Some(LocalKind::F64) => module.set_f64_ref_alias(value_var, &payload),
        Some(LocalKind::I32) => module.set_i32_ref_alias(value_var, &payload),
        Some(LocalKind::Str) => module.set_str_ref_alias(value_var, &payload),
        Some(LocalKind::Array) => {
            module.set_array_ref_alias(value_var, &payload);
            module.set_array_ref_alias_source(value_var, array);
        }
        Some(LocalKind::Mixed) if module.mixed_ref_alias_refreshes(value_var) => {}
        Some(LocalKind::Mixed) => module.set_mixed_ref_alias(value_var, &cell),
        _ => unreachable!(),
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", break_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", array));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", break_label));
    if let Some(key_var) = key_var {
        emit_indexed_foreach_key_assign(key_var, &index, span, module)?;
    }
    emit_value_cell_address_for_local(array, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("local.tee {}", payload));
    match value_kind {
        Some(LocalKind::I64) => module.body().line("i64.load"),
        Some(LocalKind::F64) => module.body().line("f64.load"),
        Some(LocalKind::I32) => {
            module.body().line("i64.load");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
        }
        Some(LocalKind::Str) => {
            module.body().line("i32.load");
            module.body().line(&format!("local.set ${}_ptr", value_var));
            module.body().line(&format!("local.get {}", payload));
            module.body().line("i32.const 4");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.set ${}_len", value_var));
            module.body().open(&format!("block {}", continue_label));
            module.push_loop(break_label.clone(), continue_label.clone());
            for stmt in body {
                emit_stmt(stmt, module)?;
            }
            module.pop_loop();
            module.body().close("end");
            module.body().line(&format!("local.get {}", index));
            module.body().line("i32.const 1");
            module.body().line("i32.add");
            module.body().line(&format!("local.set {}", index));
            module.body().line(&format!("br {}", loop_label));
            module.body().close("end");
            module.body().close("end");
            return Ok(());
        }
        Some(LocalKind::Mixed) => {
            module.body().line("drop");
            if module.mixed_ref_alias_refreshes(value_var) {
                let address_local = module.mixed_ref_alias(value_var).ok_or_else(|| {
                    CompileError::new(span, "wasm32-web value-cell foreach reference alias is missing")
                })?;
                module.body().line(&format!("local.get {}", cell));
                module.body().line(&format!("local.set {}", address_local));
            }
            emit_copy_value_cell_from_addr_to_addr(&format!("${}", value_var), &cell, module);
            module.set_mixed_value_cell_kind(value_var, cell_kind);
            module.body().open(&format!("block {}", continue_label));
            module.push_loop(break_label.clone(), continue_label.clone());
            for stmt in body {
                emit_stmt(stmt, module)?;
            }
            module.pop_loop();
            module.body().close("end");
            module.body().line(&format!("local.get {}", index));
            module.body().line("i32.const 1");
            module.body().line("i32.add");
            module.body().line(&format!("local.set {}", index));
            module.body().line(&format!("br {}", loop_label));
            module.body().close("end");
            module.body().close("end");
            return Ok(());
        }
        Some(LocalKind::Array) => {
            module.body().line("i32.load");
            module.body().line(&format!("local.set ${}_ptr", value_var));
            module.body().line(&format!("local.get {}", payload));
            module.body().line("i32.const 4");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.set ${}_len", value_var));
            set_array_metadata_for_value_foreach_array_local(value_var, array, module);
            module.body().open(&format!("block {}", continue_label));
            module.push_loop(break_label.clone(), continue_label.clone());
            for stmt in body {
                emit_stmt(stmt, module)?;
            }
            module.pop_loop();
            module.body().close("end");
            module.body().line(&format!("local.get {}", index));
            module.body().line("i32.const 1");
            module.body().line("i32.add");
            module.body().line(&format!("local.set {}", index));
            module.body().line(&format!("br {}", loop_label));
            module.body().close("end");
            module.body().close("end");
            return Ok(());
        }
        _ => unreachable!(),
    }
    module.body().line(&format!("local.set ${}", value_var));
    module.body().open(&format!("block {}", continue_label));
    module.push_loop(break_label.clone(), continue_label.clone());
    for stmt in body {
        emit_stmt(stmt, module)?;
    }
    module.pop_loop();
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

pub(super) fn emit_value_array_foreach(
    array: &str,
    key_var: Option<&str>,
    value_var: &str,
    body: &[Stmt],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let mut value_kind = module.local_kind(value_var).ok_or_else(|| {
        CompileError::new(
            body.first().map_or(crate::span::Span::new(0, 0), |stmt| stmt.span),
            "wasm32-web value-cell foreach requires a value local",
        )
    })?;
    if value_kind == LocalKind::I64 && homogeneous_object_array_class(array, module).is_some() {
        module.declare_object_local(value_var.to_string());
        value_kind = LocalKind::Object;
    }
    let index = module.next_label("value_foreach_index");
    let cell = module.next_label("value_foreach_cell");
    let break_label = module.next_label("value_foreach_break");
    let loop_label = module.next_label("value_foreach_loop");
    let continue_label = module.next_label("value_foreach_continue");
    for local in [&index, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", break_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", array));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", break_label));
    if let Some(key_var) = key_var {
        emit_indexed_foreach_key_assign(
            key_var,
            &index,
            body.first().map_or(crate::span::Span::new(0, 0), |stmt| stmt.span),
            module,
        )?;
    }
    emit_value_cell_address_for_local(array, &index, &cell, module);
    match value_kind {
        LocalKind::I64 => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line(&format!("local.set ${}", value_var));
            if let Some(address_local) = module.i64_ref_alias(value_var) {
                module.body().line(&format!("local.get {}", address_local));
                module.body().line(&format!("local.get ${}", value_var));
                module.body().line("i64.store");
            }
        }
        LocalKind::F64 => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("f64.load");
            module.body().line(&format!("local.set ${}", value_var));
            if let Some(address_local) = module.f64_ref_alias(value_var) {
                module.body().line(&format!("local.get {}", address_local));
                module.body().line(&format!("local.get ${}", value_var));
                module.body().line("f64.store");
            }
        }
        LocalKind::I32 => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
            module.body().line(&format!("local.set ${}", value_var));
            if let Some(address_local) = module.i32_ref_alias(value_var) {
                module.body().line(&format!("local.get {}", address_local));
                module.body().line(&format!("local.get ${}", value_var));
                module.body().line("i64.extend_i32_u");
                module.body().line("i64.store");
            }
        }
        LocalKind::Str => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.set ${}_ptr", value_var));
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.set ${}_len", value_var));
            if let Some(address_local) = module.str_ref_alias(value_var) {
                module.body().line(&format!("local.get {}", address_local));
                module.body().line(&format!("local.get ${}_ptr", value_var));
                module.body().line("i32.store");
                module.body().line(&format!("local.get {}", address_local));
                module.body().line("i32.const 4");
                module.body().line("i32.add");
                module.body().line(&format!("local.get ${}_len", value_var));
                module.body().line("i32.store");
            }
        }
        LocalKind::Array => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.set ${}_ptr", value_var));
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.set ${}_len", value_var));
            set_array_metadata_for_value_foreach_array_local(value_var, array, module);
            if let Some(address_local) = module.array_ref_alias(value_var) {
                module.body().line(&format!("local.get {}", address_local));
                module.body().line(&format!("local.get ${}_ptr", value_var));
                module.body().line("i32.store");
                module.body().line(&format!("local.get {}", address_local));
                module.body().line("i32.const 4");
                module.body().line("i32.add");
                module.body().line(&format!("local.get ${}_len", value_var));
                module.body().line("i32.store");
            }
        }
        LocalKind::Object => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.set ${}", value_var));
            set_object_metadata_for_value_foreach_object_local(value_var, array, module);
        }
        LocalKind::Mixed => {
            emit_copy_value_cell_from_addr_to_addr(&format!("${}", value_var), &cell, module);
            module.set_mixed_value_cell_kind(value_var, value_cell_kind_for_value_foreach(array, module));
            if let Some(address_local) = module.mixed_ref_alias(value_var) {
                emit_copy_value_cell_from_addr_to_addr(&address_local, &format!("${}", value_var), module);
            }
        }
        _ => {
            return Err(CompileError::new(
                body.first().map_or(crate::span::Span::new(0, 0), |stmt| stmt.span),
                "wasm32-web value-cell foreach values currently support scalar or mixed locals",
            ));
        }
    }
    module.body().open(&format!("block {}", continue_label));
    module.push_loop(break_label.clone(), continue_label.clone());
    for stmt in body {
        emit_stmt(stmt, module)?;
    }
    module.pop_loop();
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

fn set_object_metadata_for_value_foreach_object_local(
    value_var: &str,
    array: &str,
    module: &mut WasmModule,
) {
    let class_name = homogeneous_object_array_class(array, module);
    module.set_object_class_for_local(value_var, class_name);
}

fn homogeneous_object_array_class(array: &str, module: &WasmModule) -> Option<String> {
    module.array_object_classes(array).and_then(|classes| {
        let first = classes.first()?.as_ref()?.clone();
        classes
            .iter()
            .all(|class_name| class_name.as_ref() == Some(&first))
            .then_some(first)
    })
}

pub(super) fn value_cell_kind_for_value_foreach(
    array: &str,
    module: &WasmModule,
) -> Option<ValueCellKind> {
    if let Some(kind) = module.array_runtime_value_cell_kind(array) {
        return Some(kind);
    }
    if module.array_nested_value_metadata_items(array).is_some()
        || module.array_runtime_nested_value_metadata(array).is_some()
    {
        return Some(ValueCellKind::Array);
    }
    let kinds = module.array_value_cell_kinds(array)?;
    let first = kinds.first().copied()?;
    kinds.iter().all(|kind| *kind == first).then_some(first)
}

pub(super) fn set_array_metadata_for_value_foreach_array_local(
    value_var: &str,
    array: &str,
    module: &mut WasmModule,
) {
    let metadata = module
        .array_runtime_nested_value_metadata(array)
        .or_else(|| {
            module
                .array_nested_value_metadata_items(array)
                .and_then(|items| items.iter().flatten().next().cloned())
        });
    let Some(metadata) = metadata else {
        module.clear_array_length(value_var);
        module.set_array_layout(value_var, ArrayLayout::Value);
        module.set_array_value_cell_kinds(value_var, None);
        module.set_array_nested_value_metadata(value_var, None);
        return;
    };
    module.set_array_layout(value_var, metadata.layout);
    module.set_array_length(value_var, metadata.len);
    module.set_array_value_cell_kinds(value_var, metadata.value_kinds);
    module.set_array_key_values(value_var, metadata.key_values.clone());
    module.set_array_key_kinds(
        value_var,
        metadata.key_values.map(|keys| {
            keys.into_iter()
                .map(|key| match key {
                    AssocKeyValue::Int(_) => AssocKeyKind::Int,
                    AssocKeyValue::Str(_) => AssocKeyKind::Str,
                })
                .collect()
        }),
    );
    module.set_array_php_normalized_runtime_keys(
        value_var,
        metadata.layout == ArrayLayout::Assoc && module.array_key_kinds(value_var).is_none(),
    );
    module.set_array_nested_value_metadata(value_var, metadata.nested_values);
}
