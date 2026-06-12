//! Purpose:
//! Emits wasm32-web reusable value-cell indexing, copy, and output helpers.
//! Keeps low-level cell address helpers separate from nested array index traversal.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` array builders, mutators, output, and transforms.
//!
//! Key details:
//! - Helpers preserve the boxed value-cell layout and delegate ownership copies to `__rt_value_copy`.

use super::*;

pub(crate) fn emit_value_array_static_payload_addr(name: &str, index: usize, module: &mut WasmModule) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE + 8));
    module.body().line("i32.add");
}

pub(crate) fn emit_value_array_static_cell_addr(name: &str, index: usize, module: &mut WasmModule) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("i32.const {}", index));
    module.body().line("call $__rt_value_cell");
}

pub(crate) fn emit_output_array_local_index(
    expr: &Expr,
    name: &str,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let index_local = module.next_label("array_index");
    let done_label = module.next_label("array_index_done");
    module.declare_i32_local(index_local.trim_start_matches('$').to_string());
    require_int(index, module)?;
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", index_local));
    module.body().open(&format!("block {}", done_label));
    module.body().line(&format!("local.get {}", index_local));
    module.body().line("i32.const 0");
    module.body().line("i32.lt_s");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", index_local));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index_local));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("call $host_write_int");
    module.body().close("end");
    let _ = expr;
    Ok(())
}

pub(crate) fn emit_output_value_array_local_index(
    expr: &Expr,
    name: &str,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let index_local = module.next_label("value_array_index");
    let cell = module.next_label("value_array_cell");
    let done_label = module.next_label("value_array_index_done");
    for local in [&index_local, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    require_int(index, module)?;
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", index_local));
    module.body().open(&format!("block {}", done_label));
    module.body().line(&format!("local.get {}", index_local));
    module.body().line("i32.const 0");
    module.body().line("i32.lt_s");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", index_local));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(name, &index_local, &cell, module);
    emit_output_value_cell(&cell, module);
    module.body().close("end");
    let _ = expr;
    Ok(())
}

pub(crate) fn emit_value_cell_address_for_local(
    name: &str,
    index: &str,
    cell: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", cell));
}

pub(crate) fn emit_output_value_cell(cell: &str, module: &mut WasmModule) {
    let (array_marker_ptr, _) = module.intern_string("Array");
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("i32.const {}", array_marker_ptr));
    module.body().line("call $__rt_output_value_cell");
}

pub(crate) fn emit_copy_value_cell(
    target_name: &str,
    source_ptr: &str,
    target_index: &str,
    source_index: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", target_name));
    module.body().line(&format!("local.get {}", target_index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("call $__rt_value_cell");
    module.body().line("call $__rt_value_copy");
}

pub(crate) fn emit_copy_static_value_cell(
    target_name: &str,
    target_index: usize,
    source_ptr: &str,
    source_index: usize,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", target_name));
    module.body().line(&format!("i32.const {}", target_index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("i32.const {}", source_index));
    module.body().line("call $__rt_value_cell");
    module.body().line("call $__rt_value_copy");
}

pub(crate) fn emit_copy_static_assoc_value_cell(
    target_name: &str,
    target_index: usize,
    source_ptr: &str,
    source_index: usize,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", target_name));
    module.body().line(&format!("i32.const {}", target_index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.get {}", source_ptr));
    module
        .body()
        .line(&format!("i32.const {}", source_index * WASM_ASSOC_ENTRY_SIZE + 16));
    module.body().line("i32.add");
    module.body().line("call $__rt_value_copy");
}

pub(crate) fn emit_static_int_slot_as_value_cell(
    target_name: &str,
    target_index: usize,
    source_ptr: &str,
    source_index: usize,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", target_name));
    module
        .body()
        .line(&format!("i32.const {}", target_index * WASM_VALUE_CELL_SIZE));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("i32.const {}", source_index * 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("call $__rt_value_store_int");
}

pub(crate) fn emit_static_null_value_cell(target_name: &str, target_index: usize, module: &mut WasmModule) {
    module.body().line(&format!("local.get ${}_ptr", target_name));
    module.body().line(&format!("i32.const {}", target_index));
    module.body().line("call $__rt_value_cell");
    module.body().line("call $__rt_value_store_null");
}

pub(crate) fn emit_output_array_value_index(
    expr: &Expr,
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let ptr = module.next_label("array_index_ptr");
    let len = module.next_label("array_index_len");
    let index_local = module.next_label("array_index");
    let done_label = module.next_label("array_index_done");
    for local in [&ptr, &len, &index_local] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_array_value_to_stack(array, module)?;
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.set {}", ptr));
    require_int(index, module)?;
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", index_local));
    module.body().open(&format!("block {}", done_label));
    module.body().line(&format!("local.get {}", index_local));
    module.body().line("i32.const 0");
    module.body().line("i32.lt_s");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", index_local));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", index_local));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("call $host_write_int");
    module.body().close("end");
    let _ = expr;
    Ok(())
}
