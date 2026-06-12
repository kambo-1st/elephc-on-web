//! Purpose:
//! Lowers compact indexed-array foreach loops for wasm32-web.
//!
//! Called from:
//! - `crate::codegen::wasm::stmt::foreach`.
//!
//! Key details:
//! - Compact arrays store integer slots directly and use indexed integer keys.
//! - Mixed foreach locals receive copied value-cell integer payloads.

use crate::errors::CompileError;
use crate::parser::ast::Stmt;

use super::super::emit_stmt;
use super::super::super::module::{LocalKind, ValueCellKind, WasmModule};

pub(super) fn emit_compact_array_foreach(
    array: &str,
    key_var: Option<&str>,
    value_var: &str,
    body: &[Stmt],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let span = body.first().map_or(crate::span::Span::new(0, 0), |stmt| stmt.span);
    let ptr = module.next_label("foreach_ptr");
    let len = module.next_label("foreach_len");
    let index = module.next_label("foreach_index");
    let break_label = module.next_label("foreach_break");
    let loop_label = module.next_label("foreach_loop");
    let continue_label = module.next_label("foreach_continue");
    for local in [&ptr, &len, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_ptr", array));
    module.body().line(&format!("local.set {}", ptr));
    module.body().line(&format!("local.get ${}_len", array));
    module.body().line(&format!("local.set {}", len));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", break_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
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
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    emit_compact_foreach_int_value_assign(value_var, span, module)?;
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

pub(super) fn emit_compact_array_foreach_by_ref(
    array: &str,
    key_var: Option<&str>,
    value_var: &str,
    body: &[Stmt],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let span = body.first().map_or(crate::span::Span::new(0, 0), |stmt| stmt.span);
    if module.array_length(array) == Some(0) {
        return Ok(());
    }
    if module.local_kind(value_var) != Some(LocalKind::I64) {
        return Err(CompileError::new(
            span,
            "wasm32-web compact foreach by reference requires an int value local",
        ));
    }

    let ptr = module.next_label("foreach_ref_ptr");
    let len = module.next_label("foreach_ref_len");
    let index = module.next_label("foreach_ref_index");
    let slot = module.next_label("foreach_ref_slot");
    let break_label = module.next_label("foreach_ref_break");
    let loop_label = module.next_label("foreach_ref_loop");
    let continue_label = module.next_label("foreach_ref_continue");
    for local in [&ptr, &len, &index, &slot] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.set_i64_ref_alias(value_var, &slot);
    module.body().line(&format!("local.get ${}_ptr", array));
    module.body().line(&format!("local.set {}", ptr));
    module.body().line(&format!("local.get ${}_len", array));
    module.body().line(&format!("local.set {}", len));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", break_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", break_label));
    if let Some(key_var) = key_var {
        emit_indexed_foreach_key_assign(key_var, &index, span, module)?;
    }
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.tee {}", slot));
    module.body().line("i64.load");
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

pub(super) fn emit_compact_foreach_int_value_assign(
    value_var: &str,
    span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match module.local_kind(value_var) {
        Some(LocalKind::I64) => {
            module.body().line("i64.load");
            module.body().line(&format!("local.set ${}", value_var));
            if let Some(address_local) = module.i64_ref_alias(value_var) {
                module.body().line(&format!("local.get {}", address_local));
                module.body().line(&format!("local.get ${}", value_var));
                module.body().line("i64.store");
            }
        }
        Some(LocalKind::Mixed) => {
            let value = module.next_label("compact_foreach_value");
            module.declare_i64_local(value.trim_start_matches('$').to_string());
            module.body().line("i64.load");
            module.body().line(&format!("local.set {}", value));
            module.body().line(&format!("local.get ${}", value_var));
            module.body().line(&format!("local.get {}", value));
            module.body().line("call $__rt_value_store_int");
            module.set_mixed_value_cell_kind(value_var, Some(ValueCellKind::Int));
        }
        _ => {
            return Err(CompileError::new(
                span,
                "wasm32-web compact foreach values require int or mixed value locals",
            ));
        }
    }
    Ok(())
}

pub(super) fn emit_indexed_foreach_key_assign(
    key_var: &str,
    index: &str,
    span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match module.local_kind(key_var) {
        Some(LocalKind::I64) => {
            module.body().line(&format!("local.get {}", index));
            module.body().line("i64.extend_i32_u");
            module.body().line(&format!("local.set ${}", key_var));
        }
        Some(LocalKind::Mixed) => {
            module.body().line(&format!("local.get ${}", key_var));
            module.body().line("call $__rt_value_release");
            module.body().line(&format!("local.get ${}", key_var));
            module.body().line(&format!("local.get {}", index));
            module.body().line("i64.extend_i32_u");
            module.body().line("call $__rt_value_store_int");
            module.set_mixed_value_cell_kind(key_var, Some(ValueCellKind::Int));
        }
        _ => {
            return Err(CompileError::new(
                span,
                "wasm32-web indexed foreach key binding requires int or mixed key local",
            ));
        }
    }
    Ok(())
}
