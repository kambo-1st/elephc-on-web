//! Purpose:
//! Emits wasm32-web dynamic `array_fill()` builders.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_builders`.
//!
//! Key details:
//! - Chooses assoc, value-cell, or compact-int runtime layout based on start/count/value metadata.
//! - Preserves runtime nested-value metadata and releases temporary fill cells after cloning.

use super::*;

pub(super) fn emit_dynamic_assoc_array_fill_assign(
    name: &str,
    start: &Expr,
    count: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let len = module.next_label("array_fill_assoc_len");
    let index = module.next_label("array_fill_assoc_index");
    let key = module.next_label("array_fill_assoc_key");
    let entry = module.next_label("array_fill_assoc_entry");
    let cell = module.next_label("array_fill_assoc_cell");
    let fill_cell = module.next_label("array_fill_assoc_fill_cell");
    if let Some(static_count) = static_or_const_or_i64_local_value(count, module) {
        if static_count < 0 {
            return Err(CompileError::new(
                count.span,
                "wasm32-web array_fill() count must not be negative",
            ));
        }
    }
    for local in [&len, &index, &entry, &cell, &fill_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(key.trim_start_matches('$').to_string());
    emit_release_current_assoc_array(name, module);
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    if let Some(static_count) = static_or_const_or_i64_local_value(count, module)
        .and_then(|count| usize::try_from(count).ok())
    {
        module.set_array_value_cell_kinds(
            name,
            value_cell_kind_for_expr(value, module).map(|kind| vec![kind; static_count]),
        );
    } else {
        module.set_array_value_cell_kinds(name, None);
        module.set_array_runtime_value_cell_kind(name, value_cell_kind_for_expr(value, module));
    }
    module.set_array_runtime_nested_value_metadata(name, nested_array_metadata_for_expr(value, module));
    require_int(start, module)?;
    module.body().line(&format!("local.set {}", key));
    require_int(count, module)?;
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.get {}", len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 1");
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", fill_cell));
    emit_store_value_cell(&fill_cell, value, module)?;
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    let loop_label = module.next_label("array_fill_assoc_loop");
    let done_label = module.next_label("array_fill_assoc_done");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get {}", key));
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_copy_value_cell_from_addr_to_addr(&cell, &fill_cell, module);
    module.body().line(&format!("local.get {}", key));
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", key));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", fill_cell));
    module.body().line("call $__rt_value_release");
    Ok(())
}

pub(super) fn emit_dynamic_value_array_fill_assign(
    name: &str,
    count: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let len = module.next_label("array_fill_value_len");
    let index = module.next_label("array_fill_value_index");
    let fill_cell = module.next_label("array_fill_value_cell");
    for local in [&len, &index, &fill_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_release_current_value_array(name, module);
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, value_cell_kind_for_expr(value, module));
    module.set_array_runtime_nested_value_metadata(name, nested_array_metadata_for_expr(value, module));
    require_int(count, module)?;
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.get {}", len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 1");
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", fill_cell));
    emit_store_value_cell(&fill_cell, value, module)?;
    emit_dynamic_value_array_pad_values(name, &index, None, &len, &fill_cell, module);
    module.body().line(&format!("local.get {}", fill_cell));
    module.body().line("call $__rt_value_release");
    Ok(())
}

pub(super) fn emit_dynamic_indexed_array_fill_assign(
    name: &str,
    count: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let len = module.next_label("array_fill_len");
    let index = module.next_label("array_fill_index");
    let fill_value = module.next_label("array_fill_value");
    for local in [&len, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(fill_value.trim_start_matches('$').to_string());
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::CompactInt);
    require_int(count, module)?;
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", len));
    require_int(value, module)?;
    module.body().line(&format!("local.set {}", fill_value));
    module.body().line(&format!("local.get {}", len));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    let loop_label = module.next_label("array_fill_loop");
    let done_label = module.next_label("array_fill_done");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", fill_value));
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}
