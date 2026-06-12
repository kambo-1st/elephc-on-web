//! Purpose:
//! Emits wasm32-web layout-specific `array_search()` loops.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_search`.
//!
//! Key details:
//! - Handles compact-int loose search by comparing boxed temporary value cells.
//! - Handles associative integer-key search while delegating string/float/bool runtime needles.

use super::*;

pub(super) fn emit_compact_int_array_search_loose_index(
    call: &Expr,
    var: &str,
    needle: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let needle_cell = module.next_label("compact_array_search_loose_needle");
    let item_cell = module.next_label("compact_array_search_loose_item");
    let index = module.next_label("compact_array_search_loose_index");
    let result = module.next_label("compact_array_search_loose_result");
    let matched = module.next_label("compact_array_search_loose_matched");
    let done_label = module.next_label("compact_array_search_loose_done");
    let loop_label = module.next_label("compact_array_search_loose_loop");
    module.declare_i64_local(result.trim_start_matches('$').to_string());
    for local in [&needle_cell, &item_cell, &index, &matched] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 1");
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", needle_cell));
    emit_store_value_cell(&needle_cell, needle, module).map_err(|_| {
        CompileError::new(
            needle.span,
            "wasm32-web loose array_search() over compact integer arrays requires a supported scalar needle",
        )
    })?;
    module.body().line("i32.const 1");
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", item_cell));
    module.body().line(&format!("local.get {}", item_cell));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.store");
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
    module.body().line(&format!("local.get {}", item_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.store");
    emit_value_cell_ptrs_php_loose_equal_between(&item_cell, &needle_cell, &matched, module);
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
    let _ = call;
    Ok(())
}

pub(super) fn emit_assoc_array_search_index(
    call: &Expr,
    var: &str,
    needle: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let key_kinds = module.array_key_kinds(var).ok_or_else(|| {
        CompileError::new(
            call.span,
            "wasm32-web array_search() over associative arrays requires statically-known integer keys",
        )
    })?;
    if key_kinds.iter().any(|kind| *kind != AssocKeyKind::Int) {
        return Err(CompileError::new(
            call.span,
            "wasm32-web array_search() over associative arrays currently returns only integer keys",
        ));
    }
    reject_negative_int_key_array_search(call, var, module)?;
    let result = module.next_label("assoc_array_search_result");
    let matched = module.next_label("assoc_array_search_matched");
    module.declare_i64_local(result.trim_start_matches('$').to_string());
    module.declare_i32_local(matched.trim_start_matches('$').to_string());
    module.body().line("i64.const -1");
    module.body().line(&format!("local.set {}", result));
    if static_string_value(needle, module).is_none() {
        if let Some(needle_var) = runtime_string_arg_or_materialize(needle, "assoc_array_search_string_needle", module)? {
            emit_assoc_array_search_runtime_string(var, &needle_var, &result, &matched, None, None, module);
            module.body().line(&format!("local.get {}", result));
            return Ok(());
        }
    }
    if static_bool_value_cell_needle(needle, module).is_none() && expression_is_booly(needle, module) {
        emit_assoc_array_search_runtime_bool(var, needle, &result, None, None, module)?;
        module.body().line(&format!("local.get {}", result));
        return Ok(());
    }
    if static_or_const_or_f64_local_value(needle, module).is_none() && expression_is_floaty(needle, module) {
        emit_assoc_array_search_runtime_float(var, needle, &result, None, None, module)?;
        module.body().line(&format!("local.get {}", result));
        return Ok(());
    }
    if static_or_const_or_i64_local_value(needle, module).is_none() && expression_is_inty(needle, module) {
        emit_assoc_array_search_runtime_int(var, needle, &result, None, None, module)?;
        module.body().line(&format!("local.get {}", result));
        return Ok(());
    }
    match &needle.kind {
        ExprKind::StringLiteral(value) => {
            emit_assoc_array_search_string(var, value, &result, &matched, None, None, module)
        }
        _ if static_string_value(needle, module).is_some() => {
            let value = static_string_value(needle, module).expect("checked by guard");
            emit_assoc_array_search_string(var, &value, &result, &matched, None, None, module)
        }
        ExprKind::BoolLiteral(value) => {
            emit_assoc_array_search_tag_payload(
                var,
                WASM_VALUE_TAG_BOOL,
                i64::from(*value),
                &result,
                None,
                None,
                module,
            )
        }
        ExprKind::FloatLiteral(value) => {
            emit_assoc_array_search_float(var, *value, &result, None, None, module)
        }
        ExprKind::Variable(_) | ExprKind::ConstRef(_) | ExprKind::Negate(_) => {
            if let Some(value) = static_bool_value_cell_needle(needle, module) {
                emit_assoc_array_search_tag_payload(
                    var,
                    WASM_VALUE_TAG_BOOL,
                    i64::from(value),
                    &result,
                    None,
                    None,
                    module,
                )
            } else if let Some(value) = static_or_const_or_f64_local_value(needle, module) {
                emit_assoc_array_search_float(var, value, &result, None, None, module)
            } else {
                let value = static_or_const_or_i64_local_value(needle, module).ok_or_else(|| {
                    CompileError::new(
                        call.span,
                        "wasm32-web array_search() over associative arrays requires a static scalar needle",
                    )
                })?;
                emit_assoc_array_search_tag_payload(
                    var,
                    WASM_VALUE_TAG_INT,
                    value,
                    &result,
                    None,
                    None,
                    module,
                )
            }
        }
        ExprKind::Null => emit_assoc_array_search_tag_payload(
            var,
            WASM_VALUE_TAG_NULL,
            0,
            &result,
            None,
            None,
            module,
        ),
        _ => {
            let value = static_or_const_or_i64_local_value(needle, module).ok_or_else(|| {
                CompileError::new(
                    call.span,
                    "wasm32-web array_search() over associative arrays requires a static scalar needle",
                )
            })?;
            emit_assoc_array_search_tag_payload(
                var,
                WASM_VALUE_TAG_INT,
                value,
                &result,
                None,
                None,
                module,
            )
        }
    }
    module.body().line(&format!("local.get {}", result));
    Ok(())
}
