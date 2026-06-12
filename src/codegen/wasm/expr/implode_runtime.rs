//! Purpose:
//! Emits runtime-length array materialization paths for wasm32-web `implode()`.
//! Handles arrays whose element cells are known by one runtime cell kind.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::implode::emit_implode_string_builtin_value_to_stack`
//!
//! Key details:
//! - String arrays compute exact output length before allocation.
//! - Scalar arrays allocate a conservative buffer and return the written byte count.

use super::*;
use super::implode::StringValueImplodeSeparator;

pub(super) fn emit_implode_runtime_string_array_value_to_stack(
    array_name: &str,
    separator: &StringValueImplodeSeparator,
    module: &mut WasmModule,
) {
    let out_ptr = module.next_label("implode_runtime_out_ptr");
    let out_len = module.next_label("implode_runtime_out_len");
    let out_idx = module.next_label("implode_runtime_out_idx");
    let index = module.next_label("implode_runtime_index");
    let cell = module.next_label("implode_runtime_cell");
    let part = module.next_label("implode_runtime_part").trim_start_matches('$').to_string();
    let separator_local = module
        .next_label("implode_runtime_separator")
        .trim_start_matches('$')
        .to_string();
    let len_loop = module.next_label("implode_runtime_len_loop");
    let len_done = module.next_label("implode_runtime_len_done");
    let copy_loop = module.next_label("implode_runtime_copy_loop");
    let copy_done = module.next_label("implode_runtime_copy_done");
    for local in [&out_ptr, &out_len, &out_idx, &index, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i32_local(format!("{}_ptr", part));
    module.declare_i32_local(format!("{}_len", part));
    module.declare_i32_local(format!("{}_ptr", separator_local));
    module.declare_i32_local(format!("{}_len", separator_local));
    match separator {
        StringValueImplodeSeparator::Static(separator) => {
            let (separator_ptr, separator_len) = module.intern_string(separator);
            module.body().line(&format!("i32.const {}", separator_ptr));
            module.body().line(&format!("local.set ${}_ptr", separator_local));
            module.body().line(&format!("i32.const {}", separator_len));
            module.body().line(&format!("local.set ${}_len", separator_local));
        }
        StringValueImplodeSeparator::Local(separator) => {
            module.body().line(&format!("local.get ${}_ptr", separator));
            module.body().line(&format!("local.set ${}_ptr", separator_local));
            module.body().line(&format!("local.get ${}_len", separator));
            module.body().line(&format!("local.set ${}_len", separator_local));
        }
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", len_done));
    module.body().open(&format!("loop {}", len_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", array_name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", len_done));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 0");
    module.body().line("i32.gt_u");
    module.body().open("if");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.get ${}_len", separator_local));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_len));
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.get ${}_ptr", array_name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", len_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", copy_done));
    module.body().open(&format!("loop {}", copy_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", array_name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", copy_done));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 0");
    module.body().line("i32.gt_u");
    module.body().open("if");
    emit_copy_string_to_memory(&separator_local, &out_ptr, &out_idx, module);
    module.body().close("end");
    module.body().line(&format!("local.get ${}_ptr", array_name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set ${}_ptr", part));
    module.body().line(&format!("local.get ${}_ptr", array_name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set ${}_len", part));
    emit_copy_string_to_memory(&part, &out_ptr, &out_idx, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", copy_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_len));
}

pub(super) fn emit_implode_runtime_scalar_array_value_to_stack(
    array_name: &str,
    separator: &StringValueImplodeSeparator,
    kind: ValueCellKind,
    module: &mut WasmModule,
) {
    let out_ptr = module.next_label("implode_runtime_scalar_out_ptr");
    let out_idx = module.next_label("implode_runtime_scalar_out_idx");
    let alloc_len = module.next_label("implode_runtime_scalar_alloc_len");
    let index = module.next_label("implode_runtime_scalar_index");
    let part = module
        .next_label("implode_runtime_scalar_part")
        .trim_start_matches('$')
        .to_string();
    let separator_local = module
        .next_label("implode_runtime_scalar_separator")
        .trim_start_matches('$')
        .to_string();
    let true_local = module
        .next_label("implode_runtime_scalar_true")
        .trim_start_matches('$')
        .to_string();
    let loop_label = module.next_label("implode_runtime_scalar_loop");
    let done_label = module.next_label("implode_runtime_scalar_done");
    for local in [&out_ptr, &out_idx, &alloc_len, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i32_local(format!("{}_ptr", part));
    module.declare_i32_local(format!("{}_len", part));
    module.declare_i32_local(format!("{}_ptr", separator_local));
    module.declare_i32_local(format!("{}_len", separator_local));
    module.declare_i32_local(format!("{}_ptr", true_local));
    module.declare_i32_local(format!("{}_len", true_local));
    match separator {
        StringValueImplodeSeparator::Static(separator) => {
            let (separator_ptr, separator_len) = module.intern_string(separator);
            module.body().line(&format!("i32.const {}", separator_ptr));
            module.body().line(&format!("local.set ${}_ptr", separator_local));
            module.body().line(&format!("i32.const {}", separator_len));
            module.body().line(&format!("local.set ${}_len", separator_local));
        }
        StringValueImplodeSeparator::Local(separator) => {
            module.body().line(&format!("local.get ${}_ptr", separator));
            module.body().line(&format!("local.set ${}_ptr", separator_local));
            module.body().line(&format!("local.get ${}_len", separator));
            module.body().line(&format!("local.set ${}_len", separator_local));
        }
    }
    let (true_ptr, true_len) = module.intern_string("1");
    module.body().line(&format!("i32.const {}", true_ptr));
    module.body().line(&format!("local.set ${}_ptr", true_local));
    module.body().line(&format!("i32.const {}", true_len));
    module.body().line(&format!("local.set ${}_len", true_local));
    let max_part_len = match kind {
        ValueCellKind::Int => 20,
        ValueCellKind::Float => 64,
        ValueCellKind::Bool => 1,
        ValueCellKind::Null => 0,
        ValueCellKind::Str | ValueCellKind::Array => {
            unreachable!("runtime scalar implode only handles int/float/bool/null")
        }
    };
    module.body().line(&format!("local.get ${}_len", array_name));
    module.body().line(&format!("i32.const {}", max_part_len));
    module.body().line(&format!("local.get ${}_len", separator_local));
    module.body().line("i32.add");
    module.body().line("i32.mul");
    module.body().line(&format!("local.set {}", alloc_len));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", alloc_len));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", array_name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 0");
    module.body().line("i32.gt_u");
    module.body().open("if");
    emit_copy_string_to_memory(&separator_local, &out_ptr, &out_idx, module);
    module.body().close("end");
    match kind {
        ValueCellKind::Int => {
            module.body().line(&format!("local.get ${}_ptr", array_name));
            module.body().line(&format!("local.get {}", index));
            module.body().line("call $__rt_value_payload_i64");
            emit_i64_stack_string_cast_value_to_stack("implode_runtime_scalar_int", module);
            module.body().line(&format!("local.set ${}_len", part));
            module.body().line(&format!("local.set ${}_ptr", part));
            emit_copy_string_to_memory(&part, &out_ptr, &out_idx, module);
        }
        ValueCellKind::Float => {
            module.body().line(&format!("local.get ${}_ptr", array_name));
            module.body().line(&format!("local.get {}", index));
            module.body().line("i32.const 16");
            module.body().line("i32.mul");
            module.body().line("i32.add");
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("f64.load");
            emit_f64_stack_string_cast_value_to_stack("implode_runtime_scalar_float", module);
            module.body().line(&format!("local.set ${}_len", part));
            module.body().line(&format!("local.set ${}_ptr", part));
            emit_copy_string_to_memory(&part, &out_ptr, &out_idx, module);
        }
        ValueCellKind::Bool => {
            module.body().line(&format!("local.get ${}_ptr", array_name));
            module.body().line(&format!("local.get {}", index));
            module.body().line("call $__rt_value_payload_i64");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
            module.body().open("if");
            emit_copy_string_to_memory(&true_local, &out_ptr, &out_idx, module);
            module.body().close("end");
        }
        ValueCellKind::Null => {}
        ValueCellKind::Str | ValueCellKind::Array => {
            unreachable!("runtime scalar implode only handles int/float/bool/null")
        }
    }
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
}
