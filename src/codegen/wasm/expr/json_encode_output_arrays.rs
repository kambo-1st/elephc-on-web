//! Purpose:
//! Emits json_encode() output paths for wasm32-web array values.
//! Keeps direct host-output JSON array lowering separate from stack materialization.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::json_encode`
//!
//! Key details:
//! - Handles compact, value-cell, and associative arrays without changing native codegen.

use super::*;
use super::json_output_metadata::{
    assoc_key_values_are_json_list, emit_output_json_assoc_value_cell_at_index,
    emit_output_json_value_cell, emit_output_json_value_cell_at_index,
    json_value_kinds_need_missing_nested_metadata,
};
use super::json_runtime_string::emit_runtime_json_encode_string_from_locals;

pub(super) fn emit_output_json_encode_runtime_assoc_array(
    call: &Expr,
    name: &str,
    kind: ValueCellKind,
    flags: i64,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if kind == ValueCellKind::Array {
        return Err(CompileError::new(
            call.span,
            "wasm32-web json_encode() runtime associative arrays currently require scalar value cells",
        ));
    }
    let index = module.next_label("json_assoc_index");
    let entry = module.next_label("json_assoc_entry");
    let cell = module.next_label("json_assoc_cell");
    let key_ptr = module.next_label("json_assoc_key_ptr");
    let key_len = module.next_label("json_assoc_key_len");
    let is_list = module.next_label("json_assoc_is_list");
    for local in [&index, &entry, &cell, &key_ptr, &key_len, &is_list] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    if flags & 16 == 0 {
        emit_runtime_assoc_json_list_check(name, &index, &entry, &is_list, module);
        module.body().line(&format!("local.get {}", is_list));
        module.body().open("if");
        emit_write_literal_text("[", module);
        emit_output_json_runtime_assoc_array_loop(
            name, kind, &index, &entry, &cell, None, None, flags, module,
        );
        emit_write_literal_text("]", module);
        module.body().line("else");
    }
    emit_write_literal_text("{", module);
    emit_output_json_runtime_assoc_array_loop(
        name,
        kind,
        &index,
        &entry,
        &cell,
        Some(&key_ptr),
        Some(&key_len),
        flags,
        module,
    );
    emit_write_literal_text("}", module);
    if flags & 16 == 0 {
        module.body().close("end");
    }
    Ok(())
}

pub(super) fn emit_output_json_encode_runtime_assoc_array_dynamic(
    name: &str,
    flags: i64,
    module: &mut WasmModule,
) {
    let index = module.next_label("json_assoc_dynamic_index");
    let entry = module.next_label("json_assoc_dynamic_entry");
    let cell = module.next_label("json_assoc_dynamic_cell");
    let key_ptr = module.next_label("json_assoc_dynamic_key_ptr");
    let key_len = module.next_label("json_assoc_dynamic_key_len");
    let is_list = module.next_label("json_assoc_dynamic_is_list");
    for local in [&index, &entry, &cell, &key_ptr, &key_len, &is_list] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    if flags & 16 == 0 {
        emit_runtime_assoc_json_list_check(name, &index, &entry, &is_list, module);
        module.body().line(&format!("local.get {}", is_list));
        module.body().open("if");
        emit_write_literal_text("[", module);
        emit_output_json_runtime_assoc_array_dynamic_loop(
            name, &index, &entry, &cell, None, None, flags, module,
        );
        emit_write_literal_text("]", module);
        module.body().line("else");
    }
    emit_write_literal_text("{", module);
    emit_output_json_runtime_assoc_array_dynamic_loop(
        name,
        &index,
        &entry,
        &cell,
        Some(&key_ptr),
        Some(&key_len),
        flags,
        module,
    );
    emit_write_literal_text("}", module);
    if flags & 16 == 0 {
        module.body().close("end");
    }
}

pub(super) fn emit_runtime_assoc_json_list_check(
    name: &str,
    index: &str,
    entry: &str,
    is_list: &str,
    module: &mut WasmModule,
) {
    let loop_label = module.next_label("json_assoc_list_check_loop");
    let done_label = module.next_label("json_assoc_list_check_done");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", is_list));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.load");
    module.body().line("i32.const 0");
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", is_list));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_key_payload_i64");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i64.extend_i32_u");
    module.body().line("i64.ne");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", is_list));
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

#[allow(clippy::too_many_arguments)]
fn emit_output_json_runtime_assoc_array_loop(
    name: &str,
    kind: ValueCellKind,
    index: &str,
    entry: &str,
    cell: &str,
    key_ptr: Option<&str>,
    key_len: Option<&str>,
    flags: i64,
    module: &mut WasmModule,
) {
    let loop_label = module.next_label("json_assoc_loop");
    let done_label = module.next_label("json_assoc_done");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 0");
    module.body().line("i32.gt_u");
    module.body().open("if");
    emit_write_literal_text(",", module);
    module.body().close("end");
    if let (Some(key_ptr), Some(key_len)) = (key_ptr, key_len) {
        emit_output_json_runtime_assoc_key(entry, key_ptr, key_len, flags, module);
    }
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_output_json_value_cell(&cell, kind, flags, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

#[allow(clippy::too_many_arguments)]
fn emit_output_json_runtime_assoc_array_dynamic_loop(
    name: &str,
    index: &str,
    entry: &str,
    cell: &str,
    key_ptr: Option<&str>,
    key_len: Option<&str>,
    flags: i64,
    module: &mut WasmModule,
) {
    let loop_label = module.next_label("json_value_assoc_dynamic_loop");
    let done_label = module.next_label("json_value_assoc_dynamic_done");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 0");
    module.body().line("i32.gt_u");
    module.body().open("if");
    emit_write_literal_text(",", module);
    module.body().close("end");
    if let (Some(key_ptr), Some(key_len)) = (key_ptr, key_len) {
        emit_output_json_runtime_assoc_key(entry, key_ptr, key_len, flags, module);
    }
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_output_json_value_cell_dynamic(cell, flags, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_output_json_runtime_assoc_key(
    entry: &str,
    key_ptr: &str,
    key_len: &str,
    flags: i64,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.load");
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_write_literal_text("\"", module);
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_key_payload_i64");
    module.body().line("call $host_write_int");
    emit_write_literal_text("\":", module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", key_ptr));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", key_len));
    emit_runtime_json_encode_string_from_locals(key_ptr, key_len, flags, module);
    emit_write_literal_text(":", module);
    module.body().close("end");
}

pub(super) fn emit_output_json_encode_compact_int_array(
    name: &str,
    len: usize,
    flags: i64,
    module: &mut WasmModule,
) {
    if flags & 16 != 0 {
        emit_write_literal_text("{", module);
    } else {
        emit_write_literal_text("[", module);
    }
    for index in 0..len {
        if index > 0 {
            emit_write_literal_text(",", module);
        }
        if flags & 16 != 0 {
            let key = json_quote(&index.to_string(), flags);
            emit_write_literal_text(&key, module);
            emit_write_literal_text(":", module);
        }
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line("i64.load");
        module.body().line("call $host_write_int");
    }
    if flags & 16 != 0 {
        emit_write_literal_text("}", module);
    } else {
        emit_write_literal_text("]", module);
    }
}

pub(super) fn emit_output_json_encode_runtime_compact_int_array(
    name: &str,
    flags: i64,
    module: &mut WasmModule,
) {
    let index = module.next_label("json_int_array_index");
    let loop_label = module.next_label("json_int_array_loop");
    let done_label = module.next_label("json_int_array_done");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    if flags & 16 != 0 {
        emit_write_literal_text("{", module);
    } else {
        emit_write_literal_text("[", module);
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_output_json_runtime_array_separator_and_key(&index, flags, module);
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("call $host_write_int");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    if flags & 16 != 0 {
        emit_write_literal_text("}", module);
    } else {
        emit_write_literal_text("]", module);
    }
}

pub(super) fn emit_output_json_encode_value_array(
    call: &Expr,
    name: &str,
    len: usize,
    flags: i64,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(kinds) = module.array_value_cell_kinds(name).map(|kinds| kinds.to_vec()) else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web json_encode() value arrays require exact value-cell metadata",
        ));
    };
    let nested_values = module.array_nested_value_metadata_items(name).map(|items| items.to_vec());
    if kinds.len() != len
        || json_value_kinds_need_missing_nested_metadata(&kinds, nested_values.as_deref())
    {
        return Err(CompileError::new(
            call.span,
            "wasm32-web json_encode() local value arrays require scalar or known nested values",
        ));
    }
    if flags & 16 != 0 {
        emit_write_literal_text("{", module);
    } else {
        emit_write_literal_text("[", module);
    }
    for (index, kind) in kinds.iter().enumerate() {
        if index > 0 {
            emit_write_literal_text(",", module);
        }
        if flags & 16 != 0 {
            let key = json_quote(&index.to_string(), flags);
            emit_write_literal_text(&key, module);
            emit_write_literal_text(":", module);
        }
        let nested = nested_values
            .as_ref()
            .and_then(|items| items.get(index))
            .and_then(Option::as_ref);
        emit_output_json_value_cell_at_index(name, index, *kind, nested, flags, call.span, module)?;
    }
    if flags & 16 != 0 {
        emit_write_literal_text("}", module);
    } else {
        emit_write_literal_text("]", module);
    }
    Ok(())
}

pub(super) fn emit_output_json_encode_runtime_value_array(
    call: &Expr,
    name: &str,
    kind: ValueCellKind,
    flags: i64,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if kind == ValueCellKind::Array {
        return Err(CompileError::new(
            call.span,
            "wasm32-web json_encode() runtime value arrays currently require scalar value cells",
        ));
    }
    let index = module.next_label("json_value_array_index");
    let cell = module.next_label("json_value_array_cell");
    let loop_label = module.next_label("json_value_array_loop");
    let done_label = module.next_label("json_value_array_done");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    if flags & 16 != 0 {
        emit_write_literal_text("{", module);
    } else {
        emit_write_literal_text("[", module);
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_output_json_runtime_array_separator_and_key(&index, flags, module);
    emit_value_cell_address_for_local(name, &index, &cell, module);
    emit_output_json_value_cell(&cell, kind, flags, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    if flags & 16 != 0 {
        emit_write_literal_text("}", module);
    } else {
        emit_write_literal_text("]", module);
    }
    Ok(())
}

pub(super) fn emit_output_json_encode_runtime_value_array_dynamic(
    name: &str,
    flags: i64,
    module: &mut WasmModule,
) {
    let index = module.next_label("json_value_array_dynamic_index");
    let cell = module.next_label("json_value_array_dynamic_cell");
    let loop_label = module.next_label("json_value_array_dynamic_loop");
    let done_label = module.next_label("json_value_array_dynamic_done");
    for local in [&index, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    if flags & 16 != 0 {
        emit_write_literal_text("{", module);
    } else {
        emit_write_literal_text("[", module);
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 0");
    module.body().line("i32.gt_u");
    module.body().open("if");
    emit_write_literal_text(",", module);
    module.body().close("end");
    if flags & 16 != 0 {
        emit_write_literal_text("\"", module);
        module.body().line(&format!("local.get {}", index));
        module.body().line("i64.extend_i32_u");
        module.body().line("call $host_write_int");
        emit_write_literal_text("\":", module);
    }
    emit_value_cell_address_for_local(name, &index, &cell, module);
    emit_output_json_value_cell_dynamic(&cell, flags, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    if flags & 16 != 0 {
        emit_write_literal_text("}", module);
    } else {
        emit_write_literal_text("]", module);
    }
}

fn emit_output_json_value_cell_dynamic(cell: &str, flags: i64, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_output_json_value_cell(cell, ValueCellKind::Int, flags, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_output_json_value_cell(cell, ValueCellKind::Float, flags, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_output_json_value_cell(cell, ValueCellKind::Bool, flags, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_output_json_value_cell(cell, ValueCellKind::Null, flags, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_output_json_value_cell(cell, ValueCellKind::Str, flags, module);
    module.body().line("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

fn emit_output_json_runtime_array_separator_and_key(
    index: &str,
    flags: i64,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 0");
    module.body().line("i32.gt_u");
    module.body().open("if");
    emit_write_literal_text(",", module);
    module.body().close("end");
    if flags & 16 != 0 {
        emit_write_literal_text("\"", module);
        module.body().line(&format!("local.get {}", index));
        module.body().line("i64.extend_i32_u");
        module.body().line("call $host_write_int");
        emit_write_literal_text("\":", module);
    }
}

pub(super) fn emit_output_json_encode_assoc_array(
    call: &Expr,
    name: &str,
    len: usize,
    flags: i64,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(keys) = module.array_key_values(name).map(|keys| keys.to_vec()) else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web json_encode() associative arrays require exact key metadata",
        ));
    };
    let Some(kinds) = module.array_value_cell_kinds(name).map(|kinds| kinds.to_vec()) else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web json_encode() associative arrays require exact value metadata",
        ));
    };
    let nested_values = module.array_nested_value_metadata_items(name).map(|items| items.to_vec());
    if keys.len() != len
        || kinds.len() != len
        || json_value_kinds_need_missing_nested_metadata(&kinds, nested_values.as_deref())
    {
        return Err(CompileError::new(
            call.span,
            "wasm32-web json_encode() associative arrays require scalar or known nested metadata",
        ));
    }
    let as_list = flags & 16 == 0 && assoc_key_values_are_json_list(&keys);
    emit_write_literal_text(if as_list { "[" } else { "{" }, module);
    for index in 0..len {
        if index > 0 {
            emit_write_literal_text(",", module);
        }
        if !as_list {
            let key = match &keys[index] {
                AssocKeyValue::Int(value) => json_quote(&value.to_string(), flags),
                AssocKeyValue::Str(value) => json_quote(value, flags),
            };
            emit_write_literal_text(&key, module);
            emit_write_literal_text(":", module);
        }
        let nested = nested_values
            .as_ref()
            .and_then(|items| items.get(index))
            .and_then(Option::as_ref);
        emit_output_json_assoc_value_cell_at_index(
            name, index, kinds[index], nested, flags, call.span, module,
        )?;
    }
    emit_write_literal_text(if as_list { "]" } else { "}" }, module);
    Ok(())
}
