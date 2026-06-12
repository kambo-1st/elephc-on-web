//! Purpose:
//! Provides wasm32-web associative assignment key helpers and entry copy emitters.
//! Keeps dynamic key planning/comparison separate from array assignment orchestration.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_assignment` through its re-export.
//! - Sibling array modules that compare or copy associative entries.
//!
//! Key details:
//! - Dynamic string/int keys are materialized once before assignment loops.
//! - Entry copy helpers preserve the runtime hash-array and boxed value-cell layout.

use super::*;

pub(super) enum AssocAssignKey {
    Static(StaticAssocKey),
    StringVar(String),
    StringDynamic { ptr: String, len: String },
    IntVar(String),
    IntDynamic(String),
}

pub(super) fn static_assoc_assign_key_metadata(
    key: &AssocAssignKey,
) -> Option<(AssocKeyKind, AssocKeyValue)> {
    match key {
        AssocAssignKey::Static(StaticAssocKey::Int(value)) => {
            Some((AssocKeyKind::Int, AssocKeyValue::Int(*value)))
        }
        AssocAssignKey::Static(StaticAssocKey::Str(value)) => {
            Some((AssocKeyKind::Str, AssocKeyValue::Str(value.clone())))
        }
        _ => None,
    }
}

pub(super) fn assoc_assign_key(
    index: &Expr,
    module: &mut WasmModule,
) -> Result<AssocAssignKey, CompileError> {
    if let Ok(key) = static_assoc_key_value(index, module) {
        return Ok(AssocAssignKey::Static(key));
    }
    if let ExprKind::Variable(name) = &index.kind {
        if module.local_kind(name) == Some(LocalKind::Str) {
            return Ok(AssocAssignKey::StringVar(name.clone()));
        }
        if module.local_kind(name) == Some(LocalKind::I64) {
            return Ok(AssocAssignKey::IntVar(name.clone()));
        }
    }
    if expression_is_stringy(index, module) {
        let ptr = module.next_label("assoc_assign_key_ptr");
        let len = module.next_label("assoc_assign_key_len");
        module.declare_i32_local(ptr.trim_start_matches('$').to_string());
        module.declare_i32_local(len.trim_start_matches('$').to_string());
        emit_string_value_to_stack(index, module)?;
        module.body().line(&format!("local.set {}", len));
        module.body().line(&format!("local.set {}", ptr));
        return Ok(AssocAssignKey::StringDynamic { ptr, len });
    }
    let key = module.next_label("assoc_assign_int_key");
    module.declare_i64_local(key.trim_start_matches('$').to_string());
    require_int(index, module)?;
    module.body().line(&format!("local.set {}", key));
    Ok(AssocAssignKey::IntDynamic(key))
}

pub(super) fn emit_assoc_entry_matches_assign_key(
    source_entry: &str,
    key: &AssocAssignKey,
    matched: &str,
    module: &mut WasmModule,
) {
    match key {
        AssocAssignKey::Static(key) => {
            emit_assoc_entry_matches_static_key(source_entry, key, matched, module)
        }
        AssocAssignKey::StringVar(var) => {
            emit_assoc_entry_matches_string_parts(
                source_entry,
                &format!("${}_ptr", var),
                &format!("${}_len", var),
                matched,
                module,
            );
        }
        AssocAssignKey::StringDynamic { ptr, len } => {
            emit_assoc_entry_matches_string_parts(source_entry, ptr, len, matched, module);
        }
        AssocAssignKey::IntVar(var) => {
            module.body().line(&format!("local.get {}", source_entry));
            module.body().line(&format!("local.get ${}", var));
            module.body().line("call $__rt_assoc_key_eq_int");
            module.body().line(&format!("local.set {}", matched));
        }
        AssocAssignKey::IntDynamic(var) => {
            module.body().line(&format!("local.get {}", source_entry));
            module.body().line(&format!("local.get {}", var));
            module.body().line("call $__rt_assoc_key_eq_int");
            module.body().line(&format!("local.set {}", matched));
        }
    }
}

pub(super) fn emit_store_assoc_assign_key_at_entry(
    target_entry: &str,
    key: &AssocAssignKey,
    module: &mut WasmModule,
) {
    match key {
        AssocAssignKey::Static(key) => emit_store_assoc_key_at_entry(target_entry, key, module),
        AssocAssignKey::StringVar(var) => {
            module.body().line(&format!("local.get {}", target_entry));
            module.body().line(&format!("local.get ${}_ptr", var));
            module.body().line(&format!("local.get ${}_len", var));
            module.body().line("call $__rt_assoc_store_string_key");
        }
        AssocAssignKey::StringDynamic { ptr, len } => {
            module.body().line(&format!("local.get {}", target_entry));
            module.body().line(&format!("local.get {}", ptr));
            module.body().line(&format!("local.get {}", len));
            module.body().line("call $__rt_assoc_store_string_key");
        }
        AssocAssignKey::IntVar(var) => {
            module.body().line(&format!("local.get {}", target_entry));
            module.body().line(&format!("local.get ${}", var));
            module.body().line("call $__rt_assoc_store_int_key");
        }
        AssocAssignKey::IntDynamic(var) => {
            module.body().line(&format!("local.get {}", target_entry));
            module.body().line(&format!("local.get {}", var));
            module.body().line("call $__rt_assoc_store_int_key");
        }
    }
}

pub(super) fn emit_assoc_entry_matches_string_parts(
    source_entry: &str,
    key_ptr: &str,
    key_len: &str,
    key_found: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line(&format!("local.get {}", key_ptr));
    module.body().line(&format!("local.get {}", key_len));
    module.body().line("call $__rt_assoc_key_eq_php_string");
    module.body().line(&format!("local.set {}", key_found));
}

pub(super) fn copy_assoc_entry(target_entry: &str, source_entry: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("call $__rt_assoc_copy_entry");
}

pub(super) fn copy_assoc_entry_key(
    target_entry: &str,
    source_entry: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("call $__rt_assoc_copy_key");
}

pub(super) fn copy_assoc_entry_value(
    target_entry: &str,
    source_entry: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line("call $__rt_value_copy");
}
