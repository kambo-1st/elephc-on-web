//! Purpose:
//! Emits wasm32-web `in_array()` containment scans for value-cell arrays.
//! Keeps mixed runtime-value searches separate from compact and associative scans.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_contains`
//!
//! Key details:
//! - Preserves strict/loose scalar behavior and delegates loose comparisons to boxed value-cell helpers.

use super::*;

pub(super) fn emit_value_array_contains_int(
    var: &str,
    needle: &Expr,
    found: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let needle_local = module.next_label("value_in_array_needle");
    let index = module.next_label("value_in_array_index");
    let cell = module.next_label("value_in_array_cell");
    let done_label = module.next_label("value_in_array_done");
    let loop_label = module.next_label("value_in_array_loop");
    module.declare_i64_local(needle_local.trim_start_matches('$').to_string());
    for local in [&index, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    require_int(needle, module)?;
    module.body().line(&format!("local.set {}", needle_local));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(var, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_i64");
    module.body().line(&format!("local.get {}", needle_local));
    module.body().line("i64.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_value_array_contains_float(
    var: &str,
    needle: &Expr,
    found: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let needle_local = module.next_label("value_in_array_float_needle");
    let index = module.next_label("value_in_array_float_index");
    let cell = module.next_label("value_in_array_float_cell");
    let done_label = module.next_label("value_in_array_float_done");
    let loop_label = module.next_label("value_in_array_float_loop");
    module.declare_f64_local(needle_local.trim_start_matches('$').to_string());
    for local in [&index, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    require_float(needle, module)?;
    module.body().line(&format!("local.set {}", needle_local));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(var, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_f64");
    module.body().line(&format!("local.get {}", needle_local));
    module.body().line("f64.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_value_array_contains_bool(
    var: &str,
    needle: bool,
    found: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let index = module.next_label("value_in_array_bool_index");
    let cell = module.next_label("value_in_array_bool_cell");
    let done_label = module.next_label("value_in_array_bool_done");
    let loop_label = module.next_label("value_in_array_bool_loop");
    for local in [&index, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(var, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_i64");
    module.body().line(&format!("i64.const {}", i64::from(needle)));
    module.body().line("i64.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_value_array_contains_tag(
    var: &str,
    tag: i32,
    found: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let index = module.next_label("value_in_array_tag_index");
    let cell = module.next_label("value_in_array_tag_cell");
    let done_label = module.next_label("value_in_array_tag_done");
    let loop_label = module.next_label("value_in_array_tag_loop");
    for local in [&index, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(var, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", tag));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_value_array_contains_scalar(
    var: &str,
    needle: &Expr,
    found: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(value) = static_string_value(needle, module) {
        return emit_value_array_contains_string(var, &value, found, module);
    }
    if let Some(needle_var) = runtime_string_arg_or_materialize(needle, "value_in_array_string_needle", module)? {
        return emit_value_array_contains_runtime_string(var, &needle_var, found, module);
    }
    match &needle.kind {
        ExprKind::BoolLiteral(value) => emit_value_array_contains_bool(var, *value, found, module),
        _ if static_bool_value_cell_needle(needle, module).is_some() => {
            let value = static_bool_value_cell_needle(needle, module).expect("checked by guard");
            emit_value_array_contains_bool(var, value, found, module)
        }
        _ if expression_is_booly(needle, module) => {
            emit_value_array_contains_runtime_bool(var, needle, found, module)
        }
        ExprKind::FloatLiteral(_) if expression_is_floaty(needle, module) => {
            emit_value_array_contains_float(var, needle, found, module)
        }
        _ if expression_is_floaty(needle, module) => {
            emit_value_array_contains_float(var, needle, found, module)
        }
        ExprKind::Null => emit_value_array_contains_tag(var, WASM_VALUE_TAG_NULL, found, module),
        _ => emit_value_array_contains_int(var, needle, found, module),
    }
}

pub(super) fn emit_value_array_contains_object_identity(
    var: &str,
    needle: &Expr,
    found: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let needle_local = module.next_label("value_in_array_object_needle");
    let index = module.next_label("value_in_array_object_index");
    let cell = module.next_label("value_in_array_object_cell");
    let done_label = module.next_label("value_in_array_object_done");
    let loop_label = module.next_label("value_in_array_object_loop");
    for local in [&needle_local, &index, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    match emit_expr(needle, module)? {
        ValueKind::Object => {
            module.body().line(&format!("local.set {}", needle_local));
        }
        _ => {
            return Err(CompileError::new(
                needle.span,
                "wasm32-web strict in_array() object needle requires object metadata",
            ));
        }
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(var, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_OBJECT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.get {}", needle_local));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_value_array_contains_loose_scalar(
    var: &str,
    needle: &Expr,
    found: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let needle_cell = module.next_label("value_in_array_loose_needle");
    let index = module.next_label("value_in_array_loose_index");
    let cell = module.next_label("value_in_array_loose_cell");
    let matched = module.next_label("value_in_array_loose_matched");
    let done_label = module.next_label("value_in_array_loose_done");
    let loop_label = module.next_label("value_in_array_loose_loop");
    for local in [&needle_cell, &index, &cell, &matched] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 1");
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", needle_cell));
    emit_store_value_cell(&needle_cell, needle, module).map_err(|_| {
        CompileError::new(
            needle.span,
            "wasm32-web loose in_array() requires a supported scalar needle",
        )
    })?;
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(var, &index, &cell, module);
    emit_value_cell_ptrs_php_loose_equal_between(&cell, &needle_cell, &matched, module);
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_value_array_contains_string(
    var: &str,
    needle: &str,
    found: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let (needle_ptr, needle_len) = module.intern_string(needle);
    let index = module.next_label("value_in_array_string_index");
    let byte_index = module.next_label("value_in_array_string_byte");
    let cell = module.next_label("value_in_array_string_cell");
    let item_ptr = module.next_label("value_in_array_string_item_ptr");
    let item_len = module.next_label("value_in_array_string_item_len");
    let mismatch = module.next_label("value_in_array_string_mismatch");
    let next_item = module.next_label("value_in_array_string_next");
    let byte_loop = module.next_label("value_in_array_string_byte_loop");
    let done_label = module.next_label("value_in_array_string_done");
    let loop_label = module.next_label("value_in_array_string_loop");
    for local in [&index, &byte_index, &cell, &item_ptr, &item_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(var, &index, &cell, module);
    module.body().open(&format!("block {}", next_item));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.ne");
    module.body().line(&format!("br_if {}", next_item));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", item_ptr));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", item_len));
    module.body().line(&format!("local.get {}", item_len));
    module.body().line(&format!("i32.const {}", needle_len));
    module.body().line("i32.ne");
    module.body().line(&format!("br_if {}", next_item));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", byte_index));
    module.body().open(&format!("block {}", mismatch));
    module.body().open(&format!("loop {}", byte_loop));
    module.body().line(&format!("local.get {}", byte_index));
    module.body().line(&format!("i32.const {}", needle_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", mismatch));
    module.body().line(&format!("local.get {}", item_ptr));
    module.body().line(&format!("local.get {}", byte_index));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("i32.const {}", needle_ptr));
    module.body().line(&format!("local.get {}", byte_index));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line(&format!("br {}", next_item));
    module.body().close("end");
    module.body().line(&format!("local.get {}", byte_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", byte_index));
    module.body().line(&format!("br {}", byte_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_value_array_contains_runtime_bool(
    var: &str,
    needle: &Expr,
    found: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let needle_local = module.next_label("value_in_array_bool_needle");
    let index = module.next_label("value_in_array_runtime_bool_index");
    let cell = module.next_label("value_in_array_runtime_bool_cell");
    let done_label = module.next_label("value_in_array_runtime_bool_done");
    let loop_label = module.next_label("value_in_array_runtime_bool_loop");
    module.declare_i32_local(needle_local.trim_start_matches('$').to_string());
    for local in [&index, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_condition(needle, module)?;
    module.body().line(&format!("local.set {}", needle_local));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(var, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.get {}", needle_local));
    module.body().line("i64.extend_i32_u");
    module.body().line("i64.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_value_array_contains_runtime_string(
    var: &str,
    needle_var: &str,
    found: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let index = module.next_label("value_in_array_runtime_string_index");
    let byte_index = module.next_label("value_in_array_runtime_string_byte");
    let cell = module.next_label("value_in_array_runtime_string_cell");
    let item_ptr = module.next_label("value_in_array_runtime_string_item_ptr");
    let item_len = module.next_label("value_in_array_runtime_string_item_len");
    let mismatch = module.next_label("value_in_array_runtime_string_mismatch");
    let next_item = module.next_label("value_in_array_runtime_string_next");
    let byte_loop = module.next_label("value_in_array_runtime_string_byte_loop");
    let done_label = module.next_label("value_in_array_runtime_string_done");
    let loop_label = module.next_label("value_in_array_runtime_string_loop");
    for local in [&index, &byte_index, &cell, &item_ptr, &item_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(var, &index, &cell, module);
    module.body().open(&format!("block {}", next_item));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.ne");
    module.body().line(&format!("br_if {}", next_item));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", item_ptr));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", item_len));
    module.body().line(&format!("local.get {}", item_len));
    module.body().line(&format!("local.get ${}_len", needle_var));
    module.body().line("i32.ne");
    module.body().line(&format!("br_if {}", next_item));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", byte_index));
    module.body().open(&format!("block {}", mismatch));
    module.body().open(&format!("loop {}", byte_loop));
    module.body().line(&format!("local.get {}", byte_index));
    module.body().line(&format!("local.get ${}_len", needle_var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", mismatch));
    module.body().line(&format!("local.get {}", item_ptr));
    module.body().line(&format!("local.get {}", byte_index));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.get ${}_ptr", needle_var));
    module.body().line(&format!("local.get {}", byte_index));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line(&format!("br {}", next_item));
    module.body().close("end");
    module.body().line(&format!("local.get {}", byte_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", byte_index));
    module.body().line(&format!("br {}", byte_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}
