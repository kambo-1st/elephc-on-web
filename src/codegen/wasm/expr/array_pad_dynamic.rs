//! Purpose:
//! Emits wasm32-web dynamic `array_pad()` paths for runtime-length array values.
//! Covers value-cell and compact indexed arrays plus their copy/fill loops.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_pad` through its re-export.
//! - Other array builders that reuse dynamic value-cell pad loops.
//!
//! Key details:
//! - Dynamic paths allocate based on runtime source length and requested pad length.
//! - Value-cell loops preserve ownership by copying cells through `__rt_value_copy`.

use super::*;

pub(super) fn emit_dynamic_value_array_pad_assign(
    name: &str,
    source: &Expr,
    source_layout: ArrayLayout,
    target_len: i64,
    target_abs: i32,
    pad_value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if source_layout == ArrayLayout::Assoc {
        return Err(array_unsupported(source));
    }
    let source_ptr = module.next_label("value_array_pad_source_ptr");
    let source_len = module.next_label("value_array_pad_source_len");
    let out_len = module.next_label("value_array_pad_out_len");
    let pad_count = module.next_label("value_array_pad_count");
    let index = module.next_label("value_array_pad_index");
    let pad_cell = module.next_label("value_array_pad_cell");
    for local in [&source_ptr, &source_len, &out_len, &pad_count, &index, &pad_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_array_value_to_stack(source, module)?;
    module.body().line(&format!("local.set {}", source_len));
    module.body().line(&format!("local.set {}", source_ptr));
    module.body().line(&format!("i32.const {}", target_abs));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("else");
    module.body().line(&format!("i32.const {}", target_abs));
    module.body().close("end");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", pad_count));
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, None);
    let runtime_source_kind = match &source.kind {
        ExprKind::Variable(source_name) => module.array_runtime_value_cell_kind(source_name),
        _ => None,
    };
    let pad_kind = value_cell_kind_for_expr(pad_value, module);
    module.set_array_runtime_value_cell_kind(
        name,
        runtime_source_kind.filter(|source_kind| Some(*source_kind) == pad_kind),
    );
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 1");
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", pad_cell));
    emit_store_value_cell(&pad_cell, pad_value, module)?;
    if target_len < 0 {
        emit_dynamic_value_array_pad_values(name, &index, None, &pad_count, &pad_cell, module);
        emit_dynamic_value_array_pad_source(
            name,
            &source_ptr,
            &source_len,
            &index,
            Some(&pad_count),
            source_layout,
            module,
        );
    } else {
        emit_dynamic_value_array_pad_source(name, &source_ptr, &source_len, &index, None, source_layout, module);
        emit_dynamic_value_array_pad_values(
            name,
            &index,
            Some(&source_len),
            &out_len,
            &pad_cell,
            module,
        );
    }
    module.body().line(&format!("local.get {}", pad_cell));
    module.body().line("call $__rt_value_release");
    Ok(())
}

pub(super) fn assoc_array_has_only_int_keys(source: &str, module: &WasmModule) -> bool {
    module
        .array_key_kinds(source)
        .is_some_and(|kinds| kinds.iter().all(|kind| *kind == AssocKeyKind::Int))
}

pub(super) fn function_array_return_has_only_int_keys(function_name: &str, module: &WasmModule) -> bool {
    module
        .function_array_return_key_kinds(function_name)
        .is_some_and(|kinds| kinds.iter().all(|kind| *kind == AssocKeyKind::Int))
}

pub(super) fn emit_dynamic_value_array_pad_values(
    name: &str,
    index: &str,
    start: Option<&str>,
    end: &str,
    pad_cell: &str,
    module: &mut WasmModule,
) {
    let done_label = module.next_label("value_array_pad_values_done");
    let loop_label = module.next_label("value_array_pad_values_loop");
    if let Some(start) = start {
        module.body().line(&format!("local.get {}", start));
    } else {
        module.body().line("i32.const 0");
    }
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.get {}", pad_cell));
    module.body().line("call $__rt_value_copy");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_dynamic_value_array_pad_source(
    name: &str,
    source_ptr: &str,
    source_len: &str,
    index: &str,
    dest_offset: Option<&str>,
    source_layout: ArrayLayout,
    module: &mut WasmModule,
) {
    let done_label = module.next_label("value_array_pad_source_done");
    let loop_label = module.next_label("value_array_pad_source_loop");
    let dest_cell = module.next_label("value_array_pad_dest_cell");
    module.declare_i32_local(dest_cell.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    if let Some(dest_offset) = dest_offset {
        module.body().line(&format!("local.get {}", dest_offset));
        module.body().line(&format!("local.get {}", index));
        module.body().line("i32.add");
    } else {
        module.body().line(&format!("local.get {}", index));
    }
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", dest_cell));
    match source_layout {
        ArrayLayout::Value => {
            module.body().line(&format!("local.get {}", dest_cell));
            module.body().line(&format!("local.get {}", source_ptr));
            module.body().line(&format!("local.get {}", index));
            module.body().line("call $__rt_value_cell");
            module.body().line("call $__rt_value_copy");
        }
        ArrayLayout::CompactInt => {
            module.body().line(&format!("local.get {}", dest_cell));
            module.body().line(&format!("local.get {}", source_ptr));
            module.body().line(&format!("local.get {}", index));
            module.body().line("i32.const 8");
            module.body().line("i32.mul");
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("call $__rt_value_store_int");
        }
        ArrayLayout::Assoc => {}
    }
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_value_pad_source_cell(
    target_name: &str,
    target_index: usize,
    source_layout: ArrayLayout,
    source_ptr: &str,
    source_index: usize,
    module: &mut WasmModule,
) {
    match source_layout {
        ArrayLayout::Value => {
            emit_copy_static_value_cell(target_name, target_index, source_ptr, source_index, module);
        }
        ArrayLayout::Assoc => {
            emit_copy_static_assoc_value_cell(target_name, target_index, source_ptr, source_index, module);
        }
        ArrayLayout::CompactInt => {
            emit_static_int_slot_as_value_cell(target_name, target_index, source_ptr, source_index, module);
        }
    }
}


pub(super) fn emit_dynamic_indexed_array_pad_assign(
    name: &str,
    source: &Expr,
    target_len: i64,
    target_abs: i32,
    pad_value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let source_ptr = module.next_label("array_pad_source_ptr");
    let source_len = module.next_label("array_pad_source_len");
    let out_len = module.next_label("array_pad_out_len");
    let pad_count = module.next_label("array_pad_count");
    let index = module.next_label("array_pad_index");
    let pad_local = module.next_label("array_pad_value");
    for local in [&source_ptr, &source_len, &out_len, &pad_count, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(pad_local.trim_start_matches('$').to_string());
    emit_array_value_to_stack(source, module)?;
    module.body().line(&format!("local.set {}", source_len));
    module.body().line(&format!("local.set {}", source_ptr));
    require_int(pad_value, module)?;
    module.body().line(&format!("local.set {}", pad_local));
    module.body().line(&format!("i32.const {}", target_abs));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("else");
    module.body().line(&format!("i32.const {}", target_abs));
    module.body().close("end");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", pad_count));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.set ${}_len", name));
    if target_len < 0 {
        emit_dynamic_array_pad_values(name, &index, None, &pad_count, &pad_local, module);
        emit_dynamic_array_pad_source(
            name,
            &source_ptr,
            &source_len,
            &index,
            Some(&pad_count),
            module,
        );
    } else {
        emit_dynamic_array_pad_source(name, &source_ptr, &source_len, &index, None, module);
        emit_dynamic_array_pad_values(
            name,
            &index,
            Some(&source_len),
            &out_len,
            &pad_local,
            module,
        );
    }
    Ok(())
}

pub(super) fn emit_dynamic_array_pad_values(
    name: &str,
    index: &str,
    start: Option<&str>,
    end: &str,
    pad_value: &str,
    module: &mut WasmModule,
) {
    let done_label = module.next_label("array_pad_values_done");
    let loop_label = module.next_label("array_pad_values_loop");
    if let Some(start) = start {
        module.body().line(&format!("local.get {}", start));
    } else {
        module.body().line("i32.const 0");
    }
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", pad_value));
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_dynamic_array_pad_source(
    name: &str,
    source_ptr: &str,
    source_len: &str,
    index: &str,
    dest_offset: Option<&str>,
    module: &mut WasmModule,
) {
    let done_label = module.next_label("array_pad_source_done");
    let loop_label = module.next_label("array_pad_source_loop");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    if let Some(dest_offset) = dest_offset {
        module.body().line(&format!("local.get {}", dest_offset));
        module.body().line(&format!("local.get {}", index));
        module.body().line("i32.add");
    } else {
        module.body().line(&format!("local.get {}", index));
    }
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}
