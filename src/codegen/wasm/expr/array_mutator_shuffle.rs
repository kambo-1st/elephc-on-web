//! Purpose:
//! Lowers WASM array shuffle mutation for indexed, value-cell, and associative arrays.
//! Keeps host-random shuffling separate from deterministic sort emitters.
//!
//! Called from:
//! - `super::array_mutators` re-exports used by expression builtin lowering.
//!
//! Key details:
//! - Associative shuffle reindexes values into value-cell arrays and preserves COW before mutation.

use super::*;
use super::array_value_cells::emit_ensure_unique_array_payload;

pub(super) fn emit_array_shuffle_call(
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let array = single_array_variable_arg(expr, args, "shuffle", module)?;
    let Some(len) = module.array_length(&array) else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web shuffle() requires a known indexed array length",
        ));
    };
    match module.array_layout(&array) {
        ArrayLayout::CompactInt => {
            emit_ensure_unique_array_payload(&array, module);
            emit_array_shuffle_slots(&array, len, 8, module);
        }
        ArrayLayout::Value => {
            emit_ensure_unique_array_payload(&array, module);
            emit_array_shuffle_slots(&array, len, WASM_VALUE_CELL_SIZE, module);
            if module.array_value_cell_kinds(&array).is_some_and(|kinds| {
                kinds
                    .first()
                    .is_some_and(|first| kinds.iter().any(|kind| kind != first))
            }) {
                module.set_array_value_cell_kinds(&array, None);
            }
        }
        ArrayLayout::Assoc => {
            emit_assoc_array_shuffle_reindex(&array, len, module);
        }
    }
    module.body().line("i32.const 1");
    Ok(ValueKind::Bool)
}

fn emit_assoc_array_shuffle_reindex(name: &str, len: usize, module: &mut WasmModule) {
    let value_kinds = module.array_value_cell_kinds(name).map(|kinds| kinds.to_vec());
    if len == 0 {
        module.set_array_layout(name, ArrayLayout::Value);
        module.set_array_key_kinds(name, None);
        module.set_array_key_values(name, None);
        module.set_array_value_cell_kinds(name, value_kinds);
        return;
    }
    let source_ptr = preserve_array_ptr(name, "assoc_array_shuffle", module);
    let source_entry = module.next_label("assoc_array_shuffle_source_entry");
    let source_cell = module.next_label("assoc_array_shuffle_source_cell");
    let target_cell = module.next_label("assoc_array_shuffle_target_cell");
    for local in [&source_entry, &source_cell, &target_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_cell");
        module.body().line(&format!("local.set {}", target_cell));
        module.body().line(&format!("local.get {}", source_ptr));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", source_entry));
        module.body().line(&format!("local.get {}", source_entry));
        module.body().line("i32.const 16");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", source_cell));
        emit_copy_value_cell_from_addr_to_addr(&target_cell, &source_cell, module);
    }
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_assoc_array_release");
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    module.set_array_value_cell_kinds(name, value_kinds);
    emit_array_shuffle_slots(name, len, WASM_VALUE_CELL_SIZE, module);
    if module.array_value_cell_kinds(name).is_some_and(|kinds| {
        kinds
            .first()
            .is_some_and(|first| kinds.iter().any(|kind| kind != first))
    }) {
        module.set_array_value_cell_kinds(name, None);
    }
}

fn emit_array_shuffle_slots(name: &str, len: usize, slot_size: usize, module: &mut WasmModule) {
    if len <= 1 {
        return;
    }
    let cursor = module.next_label("array_shuffle_cursor");
    let swap_index = module.next_label("array_shuffle_swap_index");
    let left_ptr = module.next_label("array_shuffle_left_ptr");
    let right_ptr = module.next_label("array_shuffle_right_ptr");
    let low = module.next_label("array_shuffle_low");
    let high = module.next_label("array_shuffle_high");
    let loop_label = module.next_label("array_shuffle_loop");
    let done_label = module.next_label("array_shuffle_done");
    for local in [&cursor, &swap_index, &left_ptr, &right_ptr] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&low, &high] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("i32.const {}", len - 1));
    module.body().line(&format!("local.set {}", cursor));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", cursor));
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line("call $host_random_u32");
    module.body().line(&format!("local.get {}", cursor));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("i32.rem_u");
    module.body().line(&format!("local.set {}", swap_index));
    emit_array_shuffle_slot_ptr(name, &cursor, slot_size, &left_ptr, module);
    emit_array_shuffle_slot_ptr(name, &swap_index, slot_size, &right_ptr, module);
    module.body().line(&format!("local.get {}", left_ptr));
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", low));
    if slot_size == WASM_VALUE_CELL_SIZE {
        module.body().line(&format!("local.get {}", left_ptr));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line("i64.load");
        module.body().line(&format!("local.set {}", high));
    }
    module.body().line(&format!("local.get {}", left_ptr));
    module.body().line(&format!("local.get {}", right_ptr));
    module.body().line("i64.load");
    module.body().line("i64.store");
    if slot_size == WASM_VALUE_CELL_SIZE {
        module.body().line(&format!("local.get {}", left_ptr));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line(&format!("local.get {}", right_ptr));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line("i64.load");
        module.body().line("i64.store");
    }
    module.body().line(&format!("local.get {}", right_ptr));
    module.body().line(&format!("local.get {}", low));
    module.body().line("i64.store");
    if slot_size == WASM_VALUE_CELL_SIZE {
        module.body().line(&format!("local.get {}", right_ptr));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line(&format!("local.get {}", high));
        module.body().line("i64.store");
    }
    module.body().line(&format!("local.get {}", cursor));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", cursor));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_array_shuffle_slot_ptr(
    name: &str,
    index: &str,
    slot_size: usize,
    target: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("i32.const {}", slot_size));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", target));
}
