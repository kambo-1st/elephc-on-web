//! Purpose:
//! Emits associative-array reads into boxed wasm32-web value cells.
//! Keeps static, runtime string/int, and mixed-key scans separate from value-cell storage.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_value_cells`
//! - mixed-key array output paths
//!
//! Key details:
//! - Missing keys materialize PHP null cells, while found entries copy owned value-cell payloads.

use super::*;

pub(super) fn emit_copy_assoc_access_to_value_cell(
    cell: &str,
    source: &str,
    index: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(key) = static_or_const_int_value(index) {
        let key = AssocKeyValue::Int(key);
        emit_copy_assoc_access_to_value_cell_by_static_key(cell, source, &key, value, module)
    } else if let Some(key) = static_string_value(index, module) {
        let key = AssocKeyValue::Str(key);
        emit_copy_assoc_access_to_value_cell_by_static_key(cell, source, &key, value, module)
    } else {
        if let ExprKind::Variable(key_name) = &index.kind {
            if module.local_kind(key_name) == Some(LocalKind::Mixed) {
                emit_copy_assoc_access_to_value_cell_by_mixed_key(cell, source, key_name, module);
                return Ok(());
            }
        }
        if let Some(key) = runtime_string_arg_or_materialize(index, "assoc_value_read_key", module)? {
            emit_copy_assoc_access_to_value_cell_by_runtime_string(
                cell,
                source,
                &format!("${}_ptr", key),
                &format!("${}_len", key),
                module,
            );
            return Ok(());
        }
        if expression_is_inty(index, module) {
            let key = module.next_label("assoc_value_read_int_key");
            module.declare_i64_local(key.trim_start_matches('$').to_string());
            require_int(index, module)?;
            module.body().line(&format!("local.set {}", key));
            emit_copy_assoc_access_to_value_cell_by_runtime_int(cell, source, &key, module);
            return Ok(());
        }
        return Err(CompileError::new(
            index.span,
            "wasm32-web associative value-cell reads require an integer or string key",
        ));
    }
}

fn emit_copy_assoc_access_to_value_cell_by_static_key(
    cell: &str,
    source: &str,
    key: &AssocKeyValue,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(keys) = module.array_key_values(source) else {
        if module.array_value_cell_kinds(source).is_some()
            || module.array_runtime_value_cell_kind(source).is_some()
        {
            match key {
                AssocKeyValue::Int(key_value) => {
                    let key = module.next_label("assoc_value_read_static_int_key");
                    module.declare_i64_local(key.trim_start_matches('$').to_string());
                    module.body().line(&format!("i64.const {}", key_value));
                    module.body().line(&format!("local.set {}", key));
                    emit_copy_assoc_access_to_value_cell_by_runtime_int(cell, source, &key, module);
                    return Ok(());
                }
                AssocKeyValue::Str(key_value) => {
                    let (ptr, len) = module.intern_string(key_value);
                    let key_ptr = module.next_label("assoc_value_read_static_key_ptr");
                    let key_len = module.next_label("assoc_value_read_static_key_len");
                    module.declare_i32_local(key_ptr.trim_start_matches('$').to_string());
                    module.declare_i32_local(key_len.trim_start_matches('$').to_string());
                    module.body().line(&format!("i32.const {}", ptr));
                    module.body().line(&format!("local.set {}", key_ptr));
                    module.body().line(&format!("i32.const {}", len));
                    module.body().line(&format!("local.set {}", key_len));
                    emit_copy_assoc_access_to_value_cell_by_runtime_string(
                        cell,
                        source,
                        &key_ptr,
                        &key_len,
                        module,
                    );
                    return Ok(());
                }
            }
        }
        return Err(CompileError::new(
            value.span,
            "wasm32-web associative value-cell reads require known key metadata",
        ));
    };
    let Some(source_index) = keys.iter().position(|candidate| candidate == key) else {
        emit_store_null_value_cell(cell, module);
        return Ok(());
    };
    let source_cell = module.next_label("assoc_value_cell_source");
    module.declare_i32_local(source_cell.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", source_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", source_cell));
    emit_copy_value_cell_from_addr_to_addr(cell, &source_cell, module);
    Ok(())
}

fn emit_copy_assoc_access_to_value_cell_by_runtime_string(
    cell: &str,
    source: &str,
    key_ptr: &str,
    key_len: &str,
    module: &mut WasmModule,
) {
    emit_scan_assoc_access_to_value_cell(cell, source, module, |entry, matched, module| {
        emit_assoc_entry_matches_string_parts(entry, key_ptr, key_len, matched, module);
    });
}

fn emit_copy_assoc_access_to_value_cell_by_runtime_int(
    cell: &str,
    source: &str,
    key: &str,
    module: &mut WasmModule,
) {
    emit_scan_assoc_access_to_value_cell(cell, source, module, |entry, matched, module| {
        module.body().line(&format!("local.get {}", entry));
        module.body().line(&format!("local.get {}", key));
        module.body().line("call $__rt_assoc_key_eq_int");
        module.body().line(&format!("local.set {}", matched));
    });
}

pub(in crate::codegen::wasm) fn emit_copy_assoc_access_to_value_cell_by_mixed_key(
    cell: &str,
    source: &str,
    key_name: &str,
    module: &mut WasmModule,
) {
    let key_tag = module.next_label("assoc_value_read_mixed_key_tag");
    module.declare_i32_local(key_tag.trim_start_matches('$').to_string());
    emit_scan_assoc_access_to_value_cell(cell, source, module, |entry, matched, module| {
        module.body().line(&format!("local.get ${}", key_name));
        module.body().line("i32.load");
        module.body().line(&format!("local.set {}", key_tag));
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", matched));
        module.body().line(&format!("local.get {}", key_tag));
        module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
        module.body().line("i32.eq");
        module.body().open("if");
        module.body().line(&format!("local.get {}", entry));
        module.body().line(&format!("local.get ${}", key_name));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line("i32.load");
        module.body().line(&format!("local.get ${}", key_name));
        module.body().line("i32.const 12");
        module.body().line("i32.add");
        module.body().line("i32.load");
        module.body().line("call $__rt_assoc_key_eq_php_string");
        module.body().line(&format!("local.set {}", matched));
        module.body().close("else");
        module.body().line(&format!("local.get {}", key_tag));
        module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
        module.body().line("i32.eq");
        module.body().open("if");
        module.body().line(&format!("local.get {}", entry));
        module.body().line(&format!("local.get ${}", key_name));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line("i64.load");
        module.body().line("call $__rt_assoc_key_eq_int");
        module.body().line(&format!("local.set {}", matched));
        module.body().close("end");
        module.body().close("end");
    });
}

fn emit_scan_assoc_access_to_value_cell(
    cell: &str,
    source: &str,
    module: &mut WasmModule,
    emit_match: impl Fn(&str, &str, &mut WasmModule),
) {
    let index = module.next_label("assoc_value_read_index");
    let entry = module.next_label("assoc_value_read_entry");
    let matched = module.next_label("assoc_value_read_matched");
    let source_cell = module.next_label("assoc_value_read_source_cell");
    let done_label = module.next_label("assoc_value_read_done");
    let loop_label = module.next_label("assoc_value_read_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.declare_i32_local(entry.trim_start_matches('$').to_string());
    module.declare_i32_local(matched.trim_start_matches('$').to_string());
    module.declare_i32_local(source_cell.trim_start_matches('$').to_string());
    emit_store_null_value_cell(cell, module);
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
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    emit_match(&entry, &matched, module);
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", source_cell));
    emit_copy_value_cell_from_addr_to_addr(cell, &source_cell, module);
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}
