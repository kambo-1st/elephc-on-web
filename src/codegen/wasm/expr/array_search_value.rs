//! Purpose:
//! Emits runtime value-cell array_search()/in_array() loops for wasm32-web.
//! Keeps mixed value-array search lowering separate from associative/static search paths.
//!
//! Called from:
//! - `super::array_search` while lowering array_search()/in_array() over value-cell arrays.
//!
//! Key details:
//! - Preserves PHP loose/strict scalar search behavior and rejects unsupported needles before emission.

use super::*;

pub(super) fn emit_value_array_search_index(
    call: &Expr,
    var: &str,
    needle: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let result = module.next_label("value_array_search_result");
    let matched = module.next_label("value_array_search_matched");
    module.declare_i64_local(result.trim_start_matches('$').to_string());
    module.declare_i32_local(matched.trim_start_matches('$').to_string());
    module.body().line("i64.const -1");
    module.body().line(&format!("local.set {}", result));
    if expression_is_object_like(needle, module) {
        emit_value_array_search_object_identity(var, needle, &result, module)?;
        module.body().line(&format!("local.get {}", result));
        return Ok(());
    }
    if static_string_value(needle, module).is_none() {
        if let Some(needle_var) = runtime_string_arg_or_materialize(needle, "value_array_search_string_needle", module)? {
            emit_value_array_search_runtime_string(var, &needle_var, &result, &matched, module);
            module.body().line(&format!("local.get {}", result));
            return Ok(());
        }
    }
    if static_bool_value_cell_needle(needle, module).is_none() && expression_is_booly(needle, module) {
        emit_value_array_search_runtime_bool(var, needle, &result, module)?;
        module.body().line(&format!("local.get {}", result));
        return Ok(());
    }
    if static_or_const_or_f64_local_value(needle, module).is_none() && expression_is_floaty(needle, module) {
        emit_value_array_search_runtime_float(var, needle, &result, module)?;
        module.body().line(&format!("local.get {}", result));
        return Ok(());
    }
    if static_or_const_or_i64_local_value(needle, module).is_none() && expression_is_inty(needle, module) {
        emit_value_array_search_runtime_int(var, needle, &result, module)?;
        module.body().line(&format!("local.get {}", result));
        return Ok(());
    }
    match &needle.kind {
        ExprKind::StringLiteral(value) => {
            emit_value_array_search_string(var, value, &result, &matched, module)
        }
        _ if static_string_value(needle, module).is_some() => {
            let value = static_string_value(needle, module).expect("checked by guard");
            emit_value_array_search_string(var, &value, &result, &matched, module)
        }
        ExprKind::BoolLiteral(value) => {
            emit_value_array_search_tag_payload(
                var,
                WASM_VALUE_TAG_BOOL,
                i64::from(*value),
                &result,
                module,
            )
        }
        ExprKind::FloatLiteral(value) => {
            emit_value_array_search_float(var, *value, &result, module)
        }
        ExprKind::Variable(_) | ExprKind::ConstRef(_) | ExprKind::Negate(_) => {
            if let Some(value) = static_bool_value_cell_needle(needle, module) {
                emit_value_array_search_tag_payload(
                    var,
                    WASM_VALUE_TAG_BOOL,
                    i64::from(value),
                    &result,
                    module,
                )
            } else if let Some(value) = static_or_const_or_f64_local_value(needle, module) {
                emit_value_array_search_float(var, value, &result, module)
            } else {
                let value = static_or_const_or_i64_local_value(needle, module).ok_or_else(|| {
                    CompileError::new(
                        call.span,
                        "wasm32-web array_search() over mixed value-cell arrays requires a static scalar needle",
                    )
                })?;
                emit_value_array_search_tag_payload(
                    var,
                    WASM_VALUE_TAG_INT,
                    value,
                    &result,
                    module,
                )
            }
        }
        ExprKind::Null => emit_value_array_search_tag_payload(
            var,
            WASM_VALUE_TAG_NULL,
            0,
            &result,
            module,
        ),
        _ => {
            let value = static_or_const_or_i64_local_value(needle, module).ok_or_else(|| {
                CompileError::new(
                    call.span,
                    "wasm32-web array_search() over mixed value-cell arrays requires a static scalar needle",
                )
            })?;
            emit_value_array_search_tag_payload(
                var,
                WASM_VALUE_TAG_INT,
                value,
                &result,
                module,
            )
        }
    }
    module.body().line(&format!("local.get {}", result));
    Ok(())
}

fn expression_is_object_like(expr: &Expr, module: &WasmModule) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => module.local_kind(name) == Some(LocalKind::Object),
        ExprKind::ScopedConstantAccess { receiver, name } => {
            module.enum_case_class(receiver, name).is_some()
        }
        ExprKind::FunctionCall { name, .. } => module.function_return_kind(name) == Some(ValueKind::Object),
        _ => false,
    }
}

fn emit_value_array_search_object_identity(
    var: &str,
    needle: &Expr,
    result: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let needle_local = module.next_label("value_array_search_object_needle");
    let index = module.next_label("value_array_search_object_index");
    let cell = module.next_label("value_array_search_object_cell");
    let done_label = module.next_label("value_array_search_object_done");
    let loop_label = module.next_label("value_array_search_object_loop");
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
                "wasm32-web array_search() object needle requires object metadata",
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
    module.body().line(&format!("local.get {}", index));
    module.body().line("i64.extend_i32_u");
    module.body().line(&format!("local.set {}", result));
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

pub(super) fn emit_value_array_search_loose_index(
    call: &Expr,
    var: &str,
    needle: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let needle_cell = module.next_label("value_array_search_loose_needle");
    let index = module.next_label("value_array_search_loose_index");
    let cell = module.next_label("value_array_search_loose_cell");
    let result = module.next_label("value_array_search_loose_result");
    let matched = module.next_label("value_array_search_loose_matched");
    let done_label = module.next_label("value_array_search_loose_done");
    let loop_label = module.next_label("value_array_search_loose_loop");
    for local in [&needle_cell, &index, &cell, &matched] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(result.trim_start_matches('$').to_string());
    module.body().line("i32.const 1");
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", needle_cell));
    emit_store_value_cell(&needle_cell, needle, module).map_err(|_| {
        CompileError::new(
            call.span,
            "wasm32-web loose array_search() requires a supported scalar needle",
        )
    })?;
    module.body().line("i64.const -1");
    module.body().line(&format!("local.set {}", result));
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
    module.body().line(&format!("local.get {}", index));
    module.body().line("i64.extend_i32_u");
    module.body().line(&format!("local.set {}", result));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", result));
    Ok(())
}

fn emit_value_array_search_float(
    var: &str,
    payload: f64,
    result: &str,
    module: &mut WasmModule,
) {
    let index = module.next_label("value_array_search_float_index");
    let cell = module.next_label("value_array_search_float_cell");
    let tag = module.next_label("value_array_search_float_tag");
    let done_label = module.next_label("value_array_search_float_done");
    let loop_label = module.next_label("value_array_search_float_loop");
    for local in [&index, &cell, &tag] {
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
    emit_load_value_cell_ptr_tag(&cell, &tag, module);
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("call $__rt_value_cell_payload_f64");
    module.body().line(&format!("f64.const {}", payload));
    module.body().line("f64.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i64.extend_i32_u");
    module.body().line(&format!("local.set {}", result));
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
}

fn emit_value_array_search_tag_payload(
    var: &str,
    tag: i32,
    payload: i64,
    result: &str,
    module: &mut WasmModule,
) {
    let index = module.next_label("value_array_search_index");
    let cell = module.next_label("value_array_search_cell");
    let tag_local = module.next_label("value_array_search_tag");
    let done_label = module.next_label("value_array_search_done");
    let loop_label = module.next_label("value_array_search_loop");
    for local in [&index, &cell, &tag_local] {
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
    emit_load_value_cell_ptr_tag(&cell, &tag_local, module);
    module.body().line(&format!("local.get {}", tag_local));
    module.body().line(&format!("i32.const {}", tag));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("call $__rt_value_cell_payload_i64");
    module.body().line(&format!("i64.const {}", payload));
    module.body().line("i64.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i64.extend_i32_u");
    module.body().line(&format!("local.set {}", result));
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
}

fn emit_value_array_search_runtime_bool(
    var: &str,
    needle: &Expr,
    result: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let needle_local = module.next_label("value_array_search_bool_needle");
    let index = module.next_label("value_array_search_runtime_bool_index");
    let cell = module.next_label("value_array_search_runtime_bool_cell");
    let tag = module.next_label("value_array_search_runtime_bool_tag");
    let done_label = module.next_label("value_array_search_runtime_bool_done");
    let loop_label = module.next_label("value_array_search_runtime_bool_loop");
    module.declare_i32_local(needle_local.trim_start_matches('$').to_string());
    for local in [&index, &cell, &tag] {
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
    emit_load_value_cell_ptr_tag(&cell, &tag, module);
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("call $__rt_value_cell_payload_i64");
    module.body().line(&format!("local.get {}", needle_local));
    module.body().line("i64.extend_i32_u");
    module.body().line("i64.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i64.extend_i32_u");
    module.body().line(&format!("local.set {}", result));
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

fn emit_value_array_search_runtime_float(
    var: &str,
    needle: &Expr,
    result: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let needle_local = module.next_label("value_array_search_float_needle");
    let index = module.next_label("value_array_search_runtime_float_index");
    let cell = module.next_label("value_array_search_runtime_float_cell");
    let tag = module.next_label("value_array_search_runtime_float_tag");
    let done_label = module.next_label("value_array_search_runtime_float_done");
    let loop_label = module.next_label("value_array_search_runtime_float_loop");
    module.declare_f64_local(needle_local.trim_start_matches('$').to_string());
    for local in [&index, &cell, &tag] {
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
    emit_load_value_cell_ptr_tag(&cell, &tag, module);
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("call $__rt_value_cell_payload_f64");
    module.body().line(&format!("local.get {}", needle_local));
    module.body().line("f64.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i64.extend_i32_u");
    module.body().line(&format!("local.set {}", result));
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

fn emit_value_array_search_runtime_int(
    var: &str,
    needle: &Expr,
    result: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let needle_local = module.next_label("value_array_search_int_needle");
    let index = module.next_label("value_array_search_runtime_int_index");
    let cell = module.next_label("value_array_search_runtime_int_cell");
    let tag = module.next_label("value_array_search_runtime_int_tag");
    let done_label = module.next_label("value_array_search_runtime_int_done");
    let loop_label = module.next_label("value_array_search_runtime_int_loop");
    module.declare_i64_local(needle_local.trim_start_matches('$').to_string());
    for local in [&index, &cell, &tag] {
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
    emit_load_value_cell_ptr_tag(&cell, &tag, module);
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("call $__rt_value_cell_payload_i64");
    module.body().line(&format!("local.get {}", needle_local));
    module.body().line("i64.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i64.extend_i32_u");
    module.body().line(&format!("local.set {}", result));
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

fn emit_value_array_search_string(
    var: &str,
    needle: &str,
    result: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    let (needle_ptr, needle_len) = module.intern_string(needle);
    let index = module.next_label("value_array_search_string_index");
    let byte_index = module.next_label("value_array_search_string_byte");
    let cell = module.next_label("value_array_search_string_cell");
    let tag = module.next_label("value_array_search_string_tag");
    let item_ptr = module.next_label("value_array_search_string_item_ptr");
    let item_len = module.next_label("value_array_search_string_item_len");
    let mismatch = module.next_label("value_array_search_string_mismatch");
    let next_item = module.next_label("value_array_search_string_next");
    let byte_loop = module.next_label("value_array_search_string_byte_loop");
    let done_label = module.next_label("value_array_search_string_done");
    let loop_label = module.next_label("value_array_search_string_loop");
    for local in [&index, &byte_index, &cell, &tag, &item_ptr, &item_len] {
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
    emit_load_value_cell_ptr_tag(&cell, &tag, module);
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.ne");
    module.body().line(&format!("br_if {}", next_item));
    emit_load_value_cell_ptr_string_part(&cell, 8, &item_ptr, module);
    emit_load_value_cell_ptr_string_part(&cell, 12, &item_len, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("local.get {}", item_len));
    module.body().line(&format!("i32.const {}", needle_len));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", matched));
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
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("br {}", mismatch));
    module.body().close("end");
    module.body().line(&format!("local.get {}", byte_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", byte_index));
    module.body().line(&format!("br {}", byte_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i64.extend_i32_u");
    module.body().line(&format!("local.set {}", result));
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
}

fn emit_value_array_search_runtime_string(
    var: &str,
    needle_var: &str,
    result: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    let index = module.next_label("value_array_search_runtime_string_index");
    let byte_index = module.next_label("value_array_search_runtime_string_byte");
    let cell = module.next_label("value_array_search_runtime_string_cell");
    let tag = module.next_label("value_array_search_runtime_string_tag");
    let item_ptr = module.next_label("value_array_search_runtime_string_item_ptr");
    let item_len = module.next_label("value_array_search_runtime_string_item_len");
    let mismatch = module.next_label("value_array_search_runtime_string_mismatch");
    let next_item = module.next_label("value_array_search_runtime_string_next");
    let byte_loop = module.next_label("value_array_search_runtime_string_byte_loop");
    let done_label = module.next_label("value_array_search_runtime_string_done");
    let loop_label = module.next_label("value_array_search_runtime_string_loop");
    for local in [&index, &byte_index, &cell, &tag, &item_ptr, &item_len] {
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
    emit_load_value_cell_ptr_tag(&cell, &tag, module);
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.ne");
    module.body().line(&format!("br_if {}", next_item));
    emit_load_value_cell_ptr_string_part(&cell, 8, &item_ptr, module);
    emit_load_value_cell_ptr_string_part(&cell, 12, &item_len, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("local.get {}", item_len));
    module.body().line(&format!("local.get ${}_len", needle_var));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", matched));
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
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("br {}", mismatch));
    module.body().close("end");
    module.body().line(&format!("local.get {}", byte_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", byte_index));
    module.body().line(&format!("br {}", byte_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i64.extend_i32_u");
    module.body().line(&format!("local.set {}", result));
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
}
