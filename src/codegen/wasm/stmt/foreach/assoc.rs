//! Purpose:
//! Lowers wasm32-web foreach loops over associative arrays.
//!
//! Called from:
//! - `crate::codegen::wasm::stmt::foreach`.
//!
//! Key details:
//! - Preserves int/string/mixed key bindings and value-cell payload metadata.
//! - Runtime-normalized PHP keys may require mixed key locals at execution time.

use crate::errors::CompileError;
use crate::parser::ast::Stmt;

use super::value::{
    set_array_metadata_for_value_foreach_array_local, value_cell_kind_for_value_foreach,
};
use super::super::emit_stmt;
use super::super::super::expr::{
    emit_copy_value_cell_from_addr_to_addr, WASM_ASSOC_KEY_INT, WASM_ASSOC_KEY_STRING,
};
use super::super::super::module::{AssocKeyKind, LocalKind, ValueCellKind, WasmModule};

pub(super) fn emit_assoc_array_foreach_by_ref(
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
                "wasm32-web associative foreach by reference currently requires scalar or mixed cells",
            ));
        }
    }
    let payload = module.next_label("assoc_foreach_ref_payload");
    module.declare_i32_local(payload.trim_start_matches('$').to_string());
    match value_kind {
        Some(LocalKind::I64) => {
            module.set_i64_ref_alias(value_var, &payload);
            module.set_i64_ref_alias_refresh(value_var, true);
        }
        Some(LocalKind::F64) => {
            module.set_f64_ref_alias(value_var, &payload);
            module.set_f64_ref_alias_refresh(value_var, true);
        }
        Some(LocalKind::I32) => {
            module.set_i32_ref_alias(value_var, &payload);
            module.set_i32_ref_alias_refresh(value_var, true);
        }
        Some(LocalKind::Str) => {
            module.set_str_ref_alias(value_var, &payload);
            module.set_str_ref_alias_refresh(value_var, true);
        }
        Some(LocalKind::Array) => {
            module.set_array_ref_alias(value_var, &payload);
            module.set_array_ref_alias_source(value_var, array);
            module.set_array_ref_alias_refresh(value_var, true);
        }
        Some(LocalKind::Mixed) => {
            if module.mixed_ref_alias(value_var).is_none() {
                module.set_mixed_ref_alias(value_var, &payload);
            }
            module.set_mixed_ref_alias_refresh(value_var, true);
        }
        _ => unreachable!(),
    }
    let result = emit_assoc_array_foreach(array, key_var, value_var, body, module);
    match value_kind {
        Some(LocalKind::I64) => module.set_i64_ref_alias_refresh(value_var, false),
        Some(LocalKind::F64) => module.set_f64_ref_alias_refresh(value_var, false),
        Some(LocalKind::I32) => module.set_i32_ref_alias_refresh(value_var, false),
        Some(LocalKind::Str) => module.set_str_ref_alias_refresh(value_var, false),
        Some(LocalKind::Array) => module.set_array_ref_alias_refresh(value_var, false),
        Some(LocalKind::Mixed) => module.set_mixed_ref_alias_refresh(value_var, false),
        _ => unreachable!(),
    }
    result
}

pub(super) fn emit_assoc_array_foreach(
    array: &str,
    key_var: Option<&str>,
    value_var: &str,
    body: &[Stmt],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let span = body.first().map_or(crate::span::Span::new(0, 0), |stmt| stmt.span);
    let value_kind = module.local_kind(value_var).ok_or_else(|| {
        CompileError::new(
            span,
            "wasm32-web associative foreach requires a value local",
        )
    })?;
    let index = module.next_label("assoc_foreach_index");
    let entry = module.next_label("assoc_foreach_entry");
    let cell = module.next_label("assoc_foreach_cell");
    let break_label = module.next_label("assoc_foreach_break");
    let loop_label = module.next_label("assoc_foreach_loop");
    let continue_label = module.next_label("assoc_foreach_continue");
    for local in [&index, &entry, &cell] {
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
    module.body().line(&format!("local.get ${}_ptr", array));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 32");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", entry));
    if let Some(key_var) = key_var {
        let key_kind = module.local_kind(key_var).ok_or_else(|| {
            CompileError::new(span, "wasm32-web associative foreach requires a key local")
        })?;
        if let Some(key_kinds) = module.array_key_kinds(array) {
            let first_key_kind = key_kinds.first().copied().unwrap_or(AssocKeyKind::Int);
            let mixed_key_kinds = key_kinds.iter().any(|kind| *kind != first_key_kind);
            if mixed_key_kinds && key_kind != LocalKind::Mixed {
                return Err(CompileError::new(
                    span,
                    "wasm32-web associative foreach mixed key binding requires a mixed key local",
                ));
            }
            match (key_kind, first_key_kind) {
                (LocalKind::Mixed, _) if mixed_key_kinds => {
                    module.body().line(&format!("local.get ${}", key_var));
                    module.body().line("call $__rt_value_release");
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.load");
                    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
                    module.body().line("i32.eq");
                    module.body().open("if");
                    module.body().line(&format!("local.get ${}", key_var));
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 8");
                    module.body().line("i32.add");
                    module.body().line("i64.load");
                    module.body().line("call $__rt_value_store_int");
                    module.body().line("else");
                    module.body().line(&format!("local.get ${}", key_var));
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 8");
                    module.body().line("i32.add");
                    module.body().line("i32.load");
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 12");
                    module.body().line("i32.add");
                    module.body().line("i32.load");
                    module.body().line("call $__rt_value_store_string");
                    module.body().close("end");
                }
                (LocalKind::Mixed, AssocKeyKind::Int) => {
                    module.body().line(&format!("local.get ${}", key_var));
                    module.body().line("call $__rt_value_release");
                    module.body().line(&format!("local.get ${}", key_var));
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 8");
                    module.body().line("i32.add");
                    module.body().line("i64.load");
                    module.body().line("call $__rt_value_store_int");
                }
                (LocalKind::Mixed, AssocKeyKind::Str) => {
                    module.body().line(&format!("local.get ${}", key_var));
                    module.body().line("call $__rt_value_release");
                    module.body().line(&format!("local.get ${}", key_var));
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 8");
                    module.body().line("i32.add");
                    module.body().line("i32.load");
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 12");
                    module.body().line("i32.add");
                    module.body().line("i32.load");
                    module.body().line("call $__rt_value_store_string");
                }
                (LocalKind::I64, AssocKeyKind::Int) => {
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 8");
                    module.body().line("i32.add");
                    module.body().line("i64.load");
                    module.body().line(&format!("local.set ${}", key_var));
                }
                (LocalKind::Str, AssocKeyKind::Str) => {
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 8");
                    module.body().line("i32.add");
                    module.body().line("i32.load");
                    module.body().line(&format!("local.set ${}_ptr", key_var));
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 12");
                    module.body().line("i32.add");
                    module.body().line("i32.load");
                    module.body().line(&format!("local.set ${}_len", key_var));
                }
                _ => {
                    return Err(CompileError::new(
                        span,
                        "wasm32-web associative foreach key local type does not match key shape",
                    ));
                }
            }
        } else if module.array_has_php_normalized_runtime_keys(array) && key_kind == LocalKind::Mixed {
            module.body().line(&format!("local.get ${}", key_var));
            module.body().line("call $__rt_value_release");
            module.body().line(&format!("local.get {}", entry));
            module.body().line("i32.load");
            module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
            module.body().line("i32.eq");
            module.body().open("if");
            module.body().line(&format!("local.get ${}", key_var));
            module.body().line(&format!("local.get {}", entry));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("call $__rt_value_store_int");
            module.body().line("else");
            module.body().line(&format!("local.get ${}", key_var));
            module.body().line(&format!("local.get {}", entry));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.get {}", entry));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line("call $__rt_value_store_string");
            module.body().close("end");
        } else if module.array_has_php_normalized_runtime_keys(array) && key_kind == LocalKind::Str {
            let key_ok = module.next_label("assoc_foreach_string_key_ok");
            module.body().open(&format!("block {}", key_ok));
            module.body().line(&format!("local.get {}", entry));
            module.body().line("i32.load");
            module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_STRING));
            module.body().line("i32.eq");
            module.body().line(&format!("br_if {}", key_ok));
            module.body().line("unreachable");
            module.body().close("end");
            module.body().line(&format!("local.get {}", entry));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.set ${}_ptr", key_var));
            module.body().line(&format!("local.get {}", entry));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.set ${}_len", key_var));
        } else if let Some(runtime_key_kind) = module.array_runtime_key_kind(array) {
            match (key_kind, runtime_key_kind) {
                (LocalKind::Mixed, AssocKeyKind::Int) => {
                    module.body().line(&format!("local.get ${}", key_var));
                    module.body().line("call $__rt_value_release");
                    module.body().line(&format!("local.get ${}", key_var));
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 8");
                    module.body().line("i32.add");
                    module.body().line("i64.load");
                    module.body().line("call $__rt_value_store_int");
                }
                (LocalKind::Mixed, AssocKeyKind::Str) => {
                    module.body().line(&format!("local.get ${}", key_var));
                    module.body().line("call $__rt_value_release");
                    module.body().line(&format!("local.get ${}", key_var));
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 8");
                    module.body().line("i32.add");
                    module.body().line("i32.load");
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 12");
                    module.body().line("i32.add");
                    module.body().line("i32.load");
                    module.body().line("call $__rt_value_store_string");
                }
                (LocalKind::I64, AssocKeyKind::Int) => {
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 8");
                    module.body().line("i32.add");
                    module.body().line("i64.load");
                    module.body().line(&format!("local.set ${}", key_var));
                }
                (LocalKind::Str, AssocKeyKind::Str) => {
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 8");
                    module.body().line("i32.add");
                    module.body().line("i32.load");
                    module.body().line(&format!("local.set ${}_ptr", key_var));
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 12");
                    module.body().line("i32.add");
                    module.body().line("i32.load");
                    module.body().line(&format!("local.set ${}_len", key_var));
                }
                _ => {
                    return Err(CompileError::new(
                        span,
                        "wasm32-web associative foreach key local type does not match runtime key shape",
                    ));
                }
            }
        } else {
            match key_kind {
                LocalKind::I64 => {
                    let key_ok = module.next_label("assoc_foreach_int_key_ok");
                    module.body().open(&format!("block {}", key_ok));
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.load");
                    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
                    module.body().line("i32.eq");
                    module.body().line(&format!("br_if {}", key_ok));
                    module.body().line("unreachable");
                    module.body().close("end");
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 8");
                    module.body().line("i32.add");
                    module.body().line("i64.load");
                    module.body().line(&format!("local.set ${}", key_var));
                }
                LocalKind::Mixed => {
                    module.body().line(&format!("local.get ${}", key_var));
                    module.body().line("call $__rt_value_release");
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.load");
                    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
                    module.body().line("i32.eq");
                    module.body().open("if");
                    module.body().line(&format!("local.get ${}", key_var));
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 8");
                    module.body().line("i32.add");
                    module.body().line("i64.load");
                    module.body().line("call $__rt_value_store_int");
                    module.body().line("else");
                    module.body().line(&format!("local.get ${}", key_var));
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 8");
                    module.body().line("i32.add");
                    module.body().line("i32.load");
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 12");
                    module.body().line("i32.add");
                    module.body().line("i32.load");
                    module.body().line("call $__rt_value_store_string");
                    module.body().close("end");
                }
                LocalKind::Str => {
                    let key_ok = module.next_label("assoc_foreach_string_key_ok");
                    module.body().open(&format!("block {}", key_ok));
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.load");
                    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_STRING));
                    module.body().line("i32.eq");
                    module.body().line(&format!("br_if {}", key_ok));
                    module.body().line("unreachable");
                    module.body().close("end");
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 8");
                    module.body().line("i32.add");
                    module.body().line("i32.load");
                    module.body().line(&format!("local.set ${}_ptr", key_var));
                    module.body().line(&format!("local.get {}", entry));
                    module.body().line("i32.const 12");
                    module.body().line("i32.add");
                    module.body().line("i32.load");
                    module.body().line(&format!("local.set ${}_len", key_var));
                }
                _ => {
                    return Err(CompileError::new(
                        span,
                        "wasm32-web associative foreach unknown key binding requires int or mixed key local",
                    ));
                }
            }
        }
    }
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 16");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
    if module.i64_ref_alias_refreshes(value_var) {
        let address_local = module.i64_ref_alias(value_var).ok_or_else(|| {
            CompileError::new(span, "wasm32-web associative foreach reference alias is missing")
        })?;
        module.body().line(&format!("local.get {}", cell));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", address_local));
    }
    if module.f64_ref_alias_refreshes(value_var) {
        let address_local = module.f64_ref_alias(value_var).ok_or_else(|| {
            CompileError::new(span, "wasm32-web associative foreach reference alias is missing")
        })?;
        module.body().line(&format!("local.get {}", cell));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", address_local));
    }
    if module.i32_ref_alias_refreshes(value_var) {
        let address_local = module.i32_ref_alias(value_var).ok_or_else(|| {
            CompileError::new(span, "wasm32-web associative foreach reference alias is missing")
        })?;
        module.body().line(&format!("local.get {}", cell));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", address_local));
    }
    if module.str_ref_alias_refreshes(value_var) {
        let address_local = module.str_ref_alias(value_var).ok_or_else(|| {
            CompileError::new(span, "wasm32-web associative foreach reference alias is missing")
        })?;
        module.body().line(&format!("local.get {}", cell));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", address_local));
    }
    if module.array_ref_alias_refreshes(value_var) {
        let address_local = module.array_ref_alias(value_var).ok_or_else(|| {
            CompileError::new(span, "wasm32-web associative foreach reference alias is missing")
        })?;
        module.body().line(&format!("local.get {}", cell));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", address_local));
    }
    if module.mixed_ref_alias_refreshes(value_var) {
        let address_local = module.mixed_ref_alias(value_var).ok_or_else(|| {
            CompileError::new(span, "wasm32-web associative foreach reference alias is missing")
        })?;
        module.body().line(&format!("local.get {}", cell));
        module.body().line(&format!("local.set {}", address_local));
    }
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
                "wasm32-web associative foreach values currently support scalar or mixed locals",
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
