//! Purpose:
//! Emits wasm32-web helper paths for value-array and associative `array_push`.
//! Keeps push-specific metadata and append-key handling separate from other mutators.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_mutators`
//!
//! Key details:
//! - Preserves value-cell copies, associative append-key metadata, and array layout updates.

use super::*;

pub(super) fn emit_value_array_push(
    name: &str,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(name) else {
        return Err(CompileError::new(
            value.span,
            "wasm32-web value-array push requires a known indexed array length",
        ));
    };
    let old_ptr = module.next_label("value_array_push_old_ptr");
    let new_ptr = module.next_label("value_array_push_new_ptr");
    let temp_ptr = module.next_label("value_array_push_temp_ptr");
    let temp_cell = module.next_label("value_array_push_temp_cell");
    let target_cell = module.next_label("value_array_push_target_cell");
    for local in [&old_ptr, &new_ptr, &temp_ptr, &temp_cell, &target_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 1");
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", temp_ptr));
    module.body().line(&format!("local.get {}", temp_ptr));
    module.body().line(&format!("local.set {}", temp_cell));
    emit_store_value_cell(&temp_cell, value, module)?;
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.set {}", old_ptr));
    module.body().line(&format!("i32.const {}", len + 1));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", new_ptr));
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.set ${}_ptr", name));
    for index in 0..len {
        emit_copy_static_value_cell(name, index, &old_ptr, index, module);
    }
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", target_cell));
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get {}", temp_cell));
    module.body().line("call $__rt_value_copy");
    module.body().line(&format!("i32.const {}", len + 1));
    module.body().line(&format!("local.set ${}_len", name));
    module.set_array_length(name, len + 1);
    module.set_array_layout(name, ArrayLayout::Value);
    module.push_array_value_cell_kind(name, value_cell_kind_for_expr(value, module));
    Ok(())
}

pub(super) fn emit_value_array_push_call(
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let Some(len) = module.array_length(name) else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_push() requires a known indexed array length",
        ));
    };
    let inserted = args.len() - 1;
    let old_ptr = preserve_array_ptr(name, "value_array_push_call", module);
    let temp_ptr = module.next_label("value_array_push_call_temp_ptr");
    let temp_cell = module.next_label("value_array_push_call_temp_cell");
    let target_cell = module.next_label("value_array_push_call_target_cell");
    for local in [&temp_ptr, &temp_cell, &target_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("i32.const {}", inserted));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", temp_ptr));
    for (index, value) in args[1..].iter().enumerate() {
        module.body().line(&format!("local.get {}", temp_ptr));
        module
            .body()
            .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", temp_cell));
        emit_store_value_cell(&temp_cell, value, module)?;
    }
    let inserted_kinds: Vec<_> = args[1..]
        .iter()
        .map(|value| value_cell_kind_for_expr(value, module))
        .collect::<Option<_>>()
        .unwrap_or_default();
    let value_kinds = module.array_value_cell_kinds(name).and_then(|kinds| {
        if inserted_kinds.len() != inserted {
            return None;
        }
        let mut out = kinds.to_vec();
        out.extend(inserted_kinds);
        Some(out)
    });
    let out_len = len + inserted;
    module.body().line(&format!("i32.const {}", out_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", out_len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..len {
        emit_copy_static_value_cell(name, index, &old_ptr, index, module);
    }
    for index in 0..inserted {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", len + index));
        module.body().line("call $__rt_value_cell");
        module.body().line(&format!("local.set {}", target_cell));
        module.body().line(&format!("local.get {}", temp_ptr));
        module
            .body()
            .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", temp_cell));
        module.body().line(&format!("local.get {}", target_cell));
        module.body().line(&format!("local.get {}", temp_cell));
        module.body().line("call $__rt_value_copy");
    }
    module.set_array_length(name, out_len);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, value_kinds);
    module.set_array_value_constants(name, None);
    module.body().line(&format!("i64.const {}", out_len));
    Ok(ValueKind::Int)
}

pub(super) fn emit_assoc_array_push(
    name: &str,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(name) else {
        return Err(CompileError::new(
            value.span,
            "wasm32-web associative array push requires a known array length",
        ));
    };
    let old_ptr = preserve_array_ptr(name, "assoc_array_push", module);
    let source_index = module.next_label("assoc_push_source_index");
    let source_entry = module.next_label("assoc_push_source_entry");
    let target_entry = module.next_label("assoc_push_target_entry");
    let next_key = module.next_label("assoc_push_next_key");
    let int_key = module.next_label("assoc_push_int_key");
    let found_int_key = module.next_label("assoc_push_found_int_key");
    let value_cell = module.next_label("assoc_push_value_cell");
    let temp_ptr = module.next_label("assoc_push_temp_ptr");
    let temp_cell = module.next_label("assoc_push_temp_cell");
    let done_label = module.next_label("assoc_push_done");
    let loop_label = module.next_label("assoc_push_loop");
    for local in [
        &source_index,
        &source_entry,
        &target_entry,
        &found_int_key,
        &value_cell,
        &temp_ptr,
        &temp_cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&next_key, &int_key] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    let pushed_key = next_assoc_append_key(module.array_key_values(name));
    let pushed_value_kind = value_cell_kind_for_expr(value, module);
    let pushed_key_kinds = module.array_key_kinds(name).map(|kinds| {
        let mut out = kinds.to_vec();
        out.push(AssocKeyKind::Int);
        out
    });
    let pushed_key_values = module.array_key_values(name).and_then(|values| {
        let key = pushed_key?;
        let mut out = values.to_vec();
        out.push(AssocKeyValue::Int(key));
        Some(out)
    });
    let pushed_value_kinds = module.array_value_cell_kinds(name).and_then(|kinds| {
        let kind = pushed_value_kind?;
        let mut out = kinds.to_vec();
        out.push(kind);
        Some(out)
    });
    module.body().line("i32.const 1");
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", temp_ptr));
    module.body().line(&format!("local.get {}", temp_ptr));
    module.body().line(&format!("local.set {}", temp_cell));
    emit_store_value_cell(&temp_cell, value, module)?;
    module.body().line("i64.const 0");
    module.body().line(&format!("local.set {}", next_key));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found_int_key));
    module.body().line(&format!("i32.const {}", len + 1));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", source_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("i32.const {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_assoc_entry_address(&old_ptr, &source_index, &source_entry, module);
    emit_assoc_entry_address(&format!("${}_ptr", name), &source_index, &target_entry, module);
    copy_assoc_entry(&target_entry, &source_entry, module);
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("call $__rt_assoc_key_payload_i64");
    module.body().line(&format!("local.set {}", int_key));
    module.body().line(&format!("local.get {}", found_int_key));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line(&format!("local.get {}", int_key));
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", next_key));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found_int_key));
    module.body().line("else");
    module.body().line(&format!("local.get {}", int_key));
    module.body().line(&format!("local.get {}", next_key));
    module.body().line("i64.ge_s");
    module.body().open("if");
    module.body().line(&format!("local.get {}", int_key));
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", next_key));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    emit_assoc_entry_address_const_index(&format!("${}_ptr", name), len, &target_entry, module);
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.store");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", next_key));
    module.body().line("i64.store");
    emit_assoc_value_cell_address(&target_entry, &value_cell, module);
    module.body().line(&format!("local.get {}", value_cell));
    module.body().line(&format!("local.get {}", temp_cell));
    module.body().line("call $__rt_value_copy");
    module.body().line(&format!("i32.const {}", len + 1));
    module.body().line(&format!("local.set ${}_len", name));
    module.set_array_length(name, len + 1);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(name, pushed_value_kinds);
    module.set_array_key_kinds(name, pushed_key_kinds);
    module.set_array_key_values(name, pushed_key_values);
    Ok(())
}

pub(super) fn emit_assoc_array_push_call(
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let Some(len) = module.array_length(name) else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_push() requires a known associative array length",
        ));
    };
    let inserted = args.len() - 1;
    let old_ptr = preserve_array_ptr(name, "assoc_array_push_call", module);
    let source_index = module.next_label("assoc_push_call_source_index");
    let source_entry = module.next_label("assoc_push_call_source_entry");
    let target_entry = module.next_label("assoc_push_call_target_entry");
    let next_key = module.next_label("assoc_push_call_next_key");
    let int_key = module.next_label("assoc_push_call_int_key");
    let found_int_key = module.next_label("assoc_push_call_found_int_key");
    let temp_ptr = module.next_label("assoc_push_call_temp_ptr");
    let temp_cell = module.next_label("assoc_push_call_temp_cell");
    let value_cell = module.next_label("assoc_push_call_value_cell");
    let done_label = module.next_label("assoc_push_call_done");
    let loop_label = module.next_label("assoc_push_call_loop");
    for local in [
        &source_index,
        &source_entry,
        &target_entry,
        &found_int_key,
        &temp_ptr,
        &temp_cell,
        &value_cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&next_key, &int_key] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("i32.const {}", inserted));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", temp_ptr));
    for (index, value) in args[1..].iter().enumerate() {
        module.body().line(&format!("local.get {}", temp_ptr));
        module
            .body()
            .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", temp_cell));
        emit_store_value_cell(&temp_cell, value, module)?;
    }
    let inserted_kinds: Vec<_> = args[1..]
        .iter()
        .map(|value| value_cell_kind_for_expr(value, module))
        .collect::<Option<_>>()
        .unwrap_or_default();
    let pushed_key_kinds = module.array_key_kinds(name).map(|kinds| {
        let mut out = kinds.to_vec();
        out.extend(std::iter::repeat_n(AssocKeyKind::Int, inserted));
        out
    });
    let pushed_key_values = module.array_key_values(name).and_then(|values| {
        let start = next_assoc_append_key(Some(values))?;
        let mut out = values.to_vec();
        for offset in 0..inserted {
            out.push(AssocKeyValue::Int(start + offset as i64));
        }
        Some(out)
    });
    let pushed_value_kinds = module.array_value_cell_kinds(name).and_then(|kinds| {
        if inserted_kinds.len() != inserted {
            return None;
        }
        let mut out = kinds.to_vec();
        out.extend(inserted_kinds);
        Some(out)
    });
    module.body().line("i64.const 0");
    module.body().line(&format!("local.set {}", next_key));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found_int_key));
    module.body().line(&format!("i32.const {}", len + inserted));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", source_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("i32.const {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_assoc_entry_address(&old_ptr, &source_index, &source_entry, module);
    emit_assoc_entry_address(&format!("${}_ptr", name), &source_index, &target_entry, module);
    copy_assoc_entry(&target_entry, &source_entry, module);
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("call $__rt_assoc_key_payload_i64");
    module.body().line(&format!("local.set {}", int_key));
    module.body().line(&format!("local.get {}", found_int_key));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line(&format!("local.get {}", int_key));
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", next_key));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found_int_key));
    module.body().line("else");
    module.body().line(&format!("local.get {}", int_key));
    module.body().line(&format!("local.get {}", next_key));
    module.body().line("i64.ge_s");
    module.body().open("if");
    module.body().line(&format!("local.get {}", int_key));
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", next_key));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    for index in 0..inserted {
        emit_assoc_entry_address_const_index(
            &format!("${}_ptr", name),
            len + index,
            &target_entry,
            module,
        );
        module.body().line(&format!("local.get {}", target_entry));
        module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
        module.body().line("i32.store");
        module.body().line(&format!("local.get {}", target_entry));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line(&format!("local.get {}", next_key));
        if index > 0 {
            module.body().line(&format!("i64.const {}", index));
            module.body().line("i64.add");
        }
        module.body().line("i64.store");
        emit_assoc_value_cell_address(&target_entry, &value_cell, module);
        module.body().line(&format!("local.get {}", temp_ptr));
        module
            .body()
            .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", temp_cell));
        module.body().line(&format!("local.get {}", value_cell));
        module.body().line(&format!("local.get {}", temp_cell));
        module.body().line("call $__rt_value_copy");
    }
    let out_len = len + inserted;
    module.body().line(&format!("i32.const {}", out_len));
    module.body().line(&format!("local.set ${}_len", name));
    module.set_array_length(name, out_len);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(name, pushed_value_kinds);
    module.set_array_key_kinds(name, pushed_key_kinds);
    module.set_array_key_values(name, pushed_key_values);
    module.body().line(&format!("i64.const {}", out_len));
    Ok(ValueKind::Int)
}

fn next_assoc_append_key(values: Option<&[AssocKeyValue]>) -> Option<i64> {
    let mut next = 0i64;
    let mut found = false;
    for value in values? {
        if let AssocKeyValue::Int(key) = value {
            if !found || *key >= next {
                next = key + 1;
                found = true;
            }
        }
    }
    Some(next)
}
