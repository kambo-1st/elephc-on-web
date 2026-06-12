//! Purpose:
//! Materializes runtime json_encode() array results onto the wasm stack.
//! Covers runtime compact, value-cell, and associative array sources.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::json_encode`
//!
//! Key details:
//! - Writes JSON strings into wasm heap memory and returns pointer/length stack values.

use super::*;
use super::json_encode_output_arrays::emit_runtime_assoc_json_list_check;
use super::json_memory_string::{
    emit_copy_literal_text_to_memory, emit_json_encode_string_locals_to_memory,
    emit_json_encode_value_string_locals_to_memory,
};

pub(super) fn emit_json_encode_runtime_value_array_value_to_stack(
    call: &Expr,
    name: &str,
    kind: ValueCellKind,
    flags: i64,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if kind == ValueCellKind::Array {
        return Err(CompileError::new(
            call.span,
            "wasm32-web json_encode() runtime value-array string values currently require scalar value cells",
        ));
    }
    let out_ptr = module.next_label("json_value_array_out_ptr");
    let out_len = module.next_label("json_value_array_out_len");
    let index = module.next_label("json_value_array_index");
    let cell = module.next_label("json_value_array_cell");
    let value_len = module.next_label("json_value_array_value_len");
    let loop_label = module.next_label("json_value_array_loop");
    let done_label = module.next_label("json_value_array_done");
    for local in [&out_ptr, &out_len, &index, &cell, &value_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_json_runtime_value_array_bound(name, kind, &index, &cell, &value_len, &out_len, module);
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_len));
    if flags & 16 != 0 {
        emit_copy_literal_text_to_memory("{", &out_ptr, &out_len, module);
    } else {
        emit_copy_literal_text_to_memory("[", &out_ptr, &out_len, module);
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_json_runtime_array_separator_and_key_to_memory(&index, &out_ptr, &out_len, flags, module);
    emit_value_cell_address_for_local(name, &index, &cell, module);
    emit_json_value_cell_to_memory(&cell, kind, &out_ptr, &out_len, flags, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    if flags & 16 != 0 {
        emit_copy_literal_text_to_memory("}", &out_ptr, &out_len, module);
    } else {
        emit_copy_literal_text_to_memory("]", &out_ptr, &out_len, module);
    }
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_len));
    Ok(())
}

pub(super) fn emit_json_encode_runtime_value_array_dynamic_value_to_stack(
    name: &str,
    flags: i64,
    module: &mut WasmModule,
) {
    let out_ptr = module.next_label("json_value_array_dynamic_out_ptr");
    let out_len = module.next_label("json_value_array_dynamic_out_len");
    let index = module.next_label("json_value_array_dynamic_index");
    let cell = module.next_label("json_value_array_dynamic_cell");
    let value_len = module.next_label("json_value_array_dynamic_value_len");
    let loop_label = module.next_label("json_value_array_dynamic_loop");
    let done_label = module.next_label("json_value_array_dynamic_done");
    for local in [&out_ptr, &out_len, &index, &cell, &value_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_json_runtime_dynamic_value_array_bound(name, &index, &cell, &value_len, &out_len, module);
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_len));
    if flags & 16 != 0 {
        emit_copy_literal_text_to_memory("{", &out_ptr, &out_len, module);
    } else {
        emit_copy_literal_text_to_memory("[", &out_ptr, &out_len, module);
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_json_runtime_array_separator_and_key_to_memory(&index, &out_ptr, &out_len, flags, module);
    emit_value_cell_address_for_local(name, &index, &cell, module);
    emit_json_value_cell_dynamic_to_memory(&cell, &out_ptr, &out_len, flags, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    if flags & 16 != 0 {
        emit_copy_literal_text_to_memory("}", &out_ptr, &out_len, module);
    } else {
        emit_copy_literal_text_to_memory("]", &out_ptr, &out_len, module);
    }
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_len));
}

fn emit_json_runtime_dynamic_value_array_bound(
    name: &str,
    index: &str,
    cell: &str,
    value_len: &str,
    out_len: &str,
    module: &mut WasmModule,
) {
    let loop_label = module.next_label("json_dynamic_value_array_bound_loop");
    let done_label = module.next_label("json_dynamic_value_array_bound_done");
    module.body().line("i32.const 2");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(name, index, cell, module);
    emit_json_dynamic_value_cell_bound(cell, value_len, out_len, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_json_runtime_value_array_bound(
    name: &str,
    kind: ValueCellKind,
    index: &str,
    cell: &str,
    value_len: &str,
    out_len: &str,
    module: &mut WasmModule,
) {
    module.body().line("i32.const 2");
    module.body().line(&format!("local.set {}", out_len));
    if kind != ValueCellKind::Str {
        module.body().line(&format!("local.get {}", out_len));
        module.body().line(&format!("local.get ${}_len", name));
        module.body().line("i32.const 64");
        module.body().line("i32.mul");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", out_len));
        return;
    }
    let loop_label = module.next_label("json_value_array_bound_loop");
    let done_label = module.next_label("json_value_array_bound_done");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(name, index, cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", value_len));
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.get {}", value_len));
    module.body().line("i32.const 6");
    module.body().line("i32.mul");
    module.body().line("i32.const 32");
    module.body().line("i32.add");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_json_encode_runtime_compact_int_array_value_to_stack(
    name: &str,
    flags: i64,
    module: &mut WasmModule,
) {
    let out_ptr = module.next_label("json_int_value_out_ptr");
    let out_len = module.next_label("json_int_value_out_len");
    let index = module.next_label("json_int_value_index");
    let part = module.next_label("json_int_value_part");
    let loop_label = module.next_label("json_int_value_loop");
    let done_label = module.next_label("json_int_value_done");
    for local in [&out_ptr, &out_len, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i32_local(format!("{}_ptr", part.trim_start_matches('$')));
    module.declare_i32_local(format!("{}_len", part.trim_start_matches('$')));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.const 64");
    module.body().line("i32.mul");
    module.body().line("i32.const 2");
    module.body().line("i32.add");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_len));
    if flags & 16 != 0 {
        emit_copy_literal_text_to_memory("{", &out_ptr, &out_len, module);
    } else {
        emit_copy_literal_text_to_memory("[", &out_ptr, &out_len, module);
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_json_runtime_array_separator_and_key_to_memory(&index, &out_ptr, &out_len, flags, module);
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    emit_i64_stack_string_cast_value_to_stack("json_mem_int_array_value", module);
    module.body().line(&format!("local.set ${}_len", part.trim_start_matches('$')));
    module.body().line(&format!("local.set ${}_ptr", part.trim_start_matches('$')));
    emit_copy_string_to_memory(part.trim_start_matches('$'), &out_ptr, &out_len, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    if flags & 16 != 0 {
        emit_copy_literal_text_to_memory("}", &out_ptr, &out_len, module);
    } else {
        emit_copy_literal_text_to_memory("]", &out_ptr, &out_len, module);
    }
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_len));
}

fn emit_json_runtime_array_separator_and_key_to_memory(
    index: &str,
    out_ptr: &str,
    out_len: &str,
    flags: i64,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 0");
    module.body().line("i32.gt_u");
    module.body().open("if");
    emit_copy_literal_text_to_memory(",", out_ptr, out_len, module);
    module.body().close("end");
    if flags & 16 != 0 {
        let key = module.next_label("json_int_value_key");
        module.declare_i32_local(format!("{}_ptr", key.trim_start_matches('$')));
        module.declare_i32_local(format!("{}_len", key.trim_start_matches('$')));
        emit_store_byte_const(34, out_ptr, out_len, module);
        module.body().line(&format!("local.get {}", index));
        module.body().line("i64.extend_i32_u");
        emit_i64_stack_string_cast_value_to_stack("json_mem_int_array_key", module);
        module.body().line(&format!("local.set ${}_len", key.trim_start_matches('$')));
        module.body().line(&format!("local.set ${}_ptr", key.trim_start_matches('$')));
        emit_copy_string_to_memory(key.trim_start_matches('$'), out_ptr, out_len, module);
        emit_copy_literal_text_to_memory("\":", out_ptr, out_len, module);
    }
}

pub(super) fn emit_json_encode_runtime_assoc_array_value_to_stack(
    call: &Expr,
    name: &str,
    kind: ValueCellKind,
    flags: i64,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if kind == ValueCellKind::Array {
        return Err(CompileError::new(
            call.span,
            "wasm32-web json_encode() runtime associative string values currently require scalar value cells",
        ));
    }
    let out_ptr = module.next_label("json_value_out_ptr");
    let out_len = module.next_label("json_value_out_len");
    let index = module.next_label("json_value_bound_index");
    let entry = module.next_label("json_value_bound_entry");
    let cell = module.next_label("json_value_bound_cell");
    let key_ptr = module.next_label("json_value_bound_key_ptr");
    let key_len = module.next_label("json_value_bound_key_len");
    let value_len = module.next_label("json_value_bound_value_len");
    let is_list = module.next_label("json_value_is_list");
    for local in [&out_ptr, &out_len, &index, &entry, &cell, &key_ptr, &key_len, &value_len, &is_list] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 2");
    module.body().line(&format!("local.set {}", out_len));
    emit_runtime_assoc_json_value_bound(
        name, kind, &index, &entry, &cell, &key_len, &value_len, &out_len, module,
    );
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_len));
    if flags & 16 == 0 {
        emit_runtime_assoc_json_list_check(name, &index, &entry, &is_list, module);
        module.body().line(&format!("local.get {}", is_list));
        module.body().open("if");
        emit_copy_literal_text_to_memory("[", &out_ptr, &out_len, module);
        emit_json_runtime_assoc_array_value_loop(
            name, kind, &index, &entry, &cell, None, None, &out_ptr, &out_len, flags, module,
        );
        emit_copy_literal_text_to_memory("]", &out_ptr, &out_len, module);
        module.body().line("else");
    }
    emit_copy_literal_text_to_memory("{", &out_ptr, &out_len, module);
    emit_json_runtime_assoc_array_value_loop(
        name,
        kind,
        &index,
        &entry,
        &cell,
        Some(&key_ptr),
        Some(&key_len),
        &out_ptr,
        &out_len,
        flags,
        module,
    );
    emit_copy_literal_text_to_memory("}", &out_ptr, &out_len, module);
    if flags & 16 == 0 {
        module.body().close("end");
    }
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_len));
    Ok(())
}

pub(super) fn emit_json_encode_runtime_assoc_array_dynamic_value_to_stack(
    name: &str,
    flags: i64,
    module: &mut WasmModule,
) {
    let out_ptr = module.next_label("json_assoc_dynamic_out_ptr");
    let out_len = module.next_label("json_assoc_dynamic_out_len");
    let index = module.next_label("json_assoc_dynamic_index");
    let entry = module.next_label("json_assoc_dynamic_entry");
    let cell = module.next_label("json_assoc_dynamic_cell");
    let key_ptr = module.next_label("json_assoc_dynamic_key_ptr");
    let key_len = module.next_label("json_assoc_dynamic_key_len");
    let value_len = module.next_label("json_assoc_dynamic_value_len");
    let is_list = module.next_label("json_assoc_dynamic_is_list");
    for local in [&out_ptr, &out_len, &index, &entry, &cell, &key_ptr, &key_len, &value_len, &is_list] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 2");
    module.body().line(&format!("local.set {}", out_len));
    emit_runtime_assoc_json_dynamic_value_bound(
        name, &index, &entry, &cell, &key_len, &value_len, &out_len, module,
    );
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_len));
    if flags & 16 == 0 {
        emit_runtime_assoc_json_list_check(name, &index, &entry, &is_list, module);
        module.body().line(&format!("local.get {}", is_list));
        module.body().open("if");
        emit_copy_literal_text_to_memory("[", &out_ptr, &out_len, module);
        emit_json_runtime_assoc_array_dynamic_value_loop(
            name, &index, &entry, &cell, None, None, &out_ptr, &out_len, flags, module,
        );
        emit_copy_literal_text_to_memory("]", &out_ptr, &out_len, module);
        module.body().line("else");
    }
    emit_copy_literal_text_to_memory("{", &out_ptr, &out_len, module);
    emit_json_runtime_assoc_array_dynamic_value_loop(
        name,
        &index,
        &entry,
        &cell,
        Some(&key_ptr),
        Some(&key_len),
        &out_ptr,
        &out_len,
        flags,
        module,
    );
    emit_copy_literal_text_to_memory("}", &out_ptr, &out_len, module);
    if flags & 16 == 0 {
        module.body().close("end");
    }
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_len));
}

#[allow(clippy::too_many_arguments)]
fn emit_runtime_assoc_json_dynamic_value_bound(
    name: &str,
    index: &str,
    entry: &str,
    cell: &str,
    key_len: &str,
    value_len: &str,
    out_len: &str,
    module: &mut WasmModule,
) {
    let loop_label = module.next_label("json_dynamic_assoc_bound_loop");
    let done_label = module.next_label("json_dynamic_assoc_bound_done");
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
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.const 32");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.load");
    module.body().open("if");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", key_len));
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.get {}", key_len));
    module.body().line("i32.const 6");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_len));
    module.body().close("end");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_json_dynamic_value_cell_bound(cell, value_len, out_len, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

#[allow(clippy::too_many_arguments)]
fn emit_json_runtime_assoc_array_dynamic_value_loop(
    name: &str,
    index: &str,
    entry: &str,
    cell: &str,
    key_ptr: Option<&str>,
    key_len: Option<&str>,
    out_ptr: &str,
    out_len: &str,
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
    emit_copy_literal_text_to_memory(",", out_ptr, out_len, module);
    module.body().close("end");
    if let (Some(key_ptr), Some(key_len)) = (key_ptr, key_len) {
        emit_json_runtime_assoc_key_to_memory(entry, key_ptr, key_len, out_ptr, out_len, flags, module);
    }
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_json_value_cell_dynamic_to_memory(cell, out_ptr, out_len, flags, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_runtime_assoc_json_value_bound(
    name: &str,
    kind: ValueCellKind,
    index: &str,
    entry: &str,
    cell: &str,
    key_len: &str,
    value_len: &str,
    out_len: &str,
    module: &mut WasmModule,
) {
    let loop_label = module.next_label("json_value_bound_loop");
    let done_label = module.next_label("json_value_bound_done");
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
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.const 32");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.load");
    module.body().open("if");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", key_len));
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.get {}", key_len));
    module.body().line("i32.const 6");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_len));
    module.body().close("end");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    if kind == ValueCellKind::Str {
        module.body().line(&format!("local.get {}", cell));
        module.body().line("i32.const 12");
        module.body().line("i32.add");
        module.body().line("i32.load");
        module.body().line(&format!("local.set {}", value_len));
        module.body().line(&format!("local.get {}", out_len));
        module.body().line(&format!("local.get {}", value_len));
        module.body().line("i32.const 6");
        module.body().line("i32.mul");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", out_len));
    } else {
        module.body().line(&format!("local.get {}", out_len));
        module.body().line("i32.const 64");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", out_len));
    }
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

#[allow(clippy::too_many_arguments)]
fn emit_json_runtime_assoc_array_value_loop(
    name: &str,
    kind: ValueCellKind,
    index: &str,
    entry: &str,
    cell: &str,
    key_ptr: Option<&str>,
    key_len: Option<&str>,
    out_ptr: &str,
    out_len: &str,
    flags: i64,
    module: &mut WasmModule,
) {
    let loop_label = module.next_label("json_value_assoc_loop");
    let done_label = module.next_label("json_value_assoc_done");
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
    emit_copy_literal_text_to_memory(",", out_ptr, out_len, module);
    module.body().close("end");
    if let (Some(key_ptr), Some(key_len)) = (key_ptr, key_len) {
        emit_json_runtime_assoc_key_to_memory(entry, key_ptr, key_len, out_ptr, out_len, flags, module);
    }
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_json_value_cell_to_memory(cell, kind, out_ptr, out_len, flags, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_json_runtime_assoc_key_to_memory(
    entry: &str,
    key_ptr: &str,
    key_len: &str,
    out_ptr: &str,
    out_len: &str,
    flags: i64,
    module: &mut WasmModule,
) {
    let int_key = module.next_label("json_key_int_string");
    module.declare_i32_local(format!("{}_ptr", int_key.trim_start_matches('$')));
    module.declare_i32_local(format!("{}_len", int_key.trim_start_matches('$')));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.load");
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_store_byte_const(34, out_ptr, out_len, module);
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_key_payload_i64");
    emit_i64_stack_string_cast_value_to_stack("json_assoc_key_int", module);
    module.body().line(&format!("local.set ${}_len", int_key.trim_start_matches('$')));
    module.body().line(&format!("local.set ${}_ptr", int_key.trim_start_matches('$')));
    emit_copy_string_to_memory(int_key.trim_start_matches('$'), out_ptr, out_len, module);
    emit_copy_literal_text_to_memory("\":", out_ptr, out_len, module);
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
            emit_json_encode_string_locals_to_memory(key_ptr, key_len, out_ptr, out_len, flags, module);
    emit_copy_literal_text_to_memory(":", out_ptr, out_len, module);
    module.body().close("end");
}

pub(super) fn emit_json_value_cell_to_memory(
    cell: &str,
    kind: ValueCellKind,
    out_ptr: &str,
    out_len: &str,
    flags: i64,
    module: &mut WasmModule,
) {
    match kind {
        ValueCellKind::Str => {
            let ptr = module.next_label("json_value_string_ptr");
            let len = module.next_label("json_value_string_len");
            module.declare_i32_local(ptr.trim_start_matches('$').to_string());
            module.declare_i32_local(len.trim_start_matches('$').to_string());
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.set {}", ptr));
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.set {}", len));
            emit_json_encode_value_string_locals_to_memory(&ptr, &len, out_ptr, out_len, flags, module);
        }
        ValueCellKind::Null => emit_copy_literal_text_to_memory("null", out_ptr, out_len, module),
        ValueCellKind::Bool => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("i64.eqz");
            module.body().open("if");
            emit_copy_literal_text_to_memory("false", out_ptr, out_len, module);
            module.body().line("else");
            emit_copy_literal_text_to_memory("true", out_ptr, out_len, module);
            module.body().close("end");
        }
        ValueCellKind::Int => {
            let part = module.next_label("json_mem_int_value");
            module.declare_i32_local(format!("{}_ptr", part.trim_start_matches('$')));
            module.declare_i32_local(format!("{}_len", part.trim_start_matches('$')));
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.load");
            emit_i64_stack_string_cast_value_to_stack("json_mem_value_int", module);
            module.body().line(&format!("local.set ${}_len", part.trim_start_matches('$')));
            module.body().line(&format!("local.set ${}_ptr", part.trim_start_matches('$')));
            emit_copy_string_to_memory(part.trim_start_matches('$'), out_ptr, out_len, module);
        }
        ValueCellKind::Float => {
            let value = module.next_label("json_mem_float_value");
            let part = module.next_label("json_mem_float_part");
            module.declare_f64_local(value.trim_start_matches('$').to_string());
            module.declare_i32_local(format!("{}_ptr", part.trim_start_matches('$')));
            module.declare_i32_local(format!("{}_len", part.trim_start_matches('$')));
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("f64.load");
            module.body().line(&format!("local.tee {}", value));
            emit_f64_stack_string_cast_value_to_stack("json_mem_value_float", module);
            module.body().line(&format!("local.set ${}_len", part.trim_start_matches('$')));
            module.body().line(&format!("local.set ${}_ptr", part.trim_start_matches('$')));
            emit_copy_string_to_memory(part.trim_start_matches('$'), out_ptr, out_len, module);
            if flags & 1024 != 0 {
                module.body().line(&format!("local.get {}", value));
                module.body().line(&format!("local.get {}", value));
                module.body().line("f64.trunc");
                module.body().line("f64.eq");
                module.body().open("if");
                emit_copy_literal_text_to_memory(".0", out_ptr, out_len, module);
                module.body().close("end");
            }
        }
        ValueCellKind::Array => module.body().line("unreachable"),
    }
}

fn emit_json_dynamic_value_cell_bound(
    cell: &str,
    value_len: &str,
    out_len: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", value_len));
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.get {}", value_len));
    module.body().line("i32.const 6");
    module.body().line("i32.mul");
    module.body().line("i32.const 32");
    module.body().line("i32.add");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line("else");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.const 64");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_len));
    module.body().close("end");
}

fn emit_json_value_cell_dynamic_to_memory(
    cell: &str,
    out_ptr: &str,
    out_len: &str,
    flags: i64,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_json_value_cell_to_memory(cell, ValueCellKind::Int, out_ptr, out_len, flags, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_json_value_cell_to_memory(cell, ValueCellKind::Float, out_ptr, out_len, flags, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_json_value_cell_to_memory(cell, ValueCellKind::Bool, out_ptr, out_len, flags, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_json_value_cell_to_memory(cell, ValueCellKind::Null, out_ptr, out_len, flags, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_json_value_cell_to_memory(cell, ValueCellKind::Str, out_ptr, out_len, flags, module);
    module.body().line("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}
