//! Purpose:
//! Lowers associative-key array_search() paths for wasm32-web arrays.
//! Keeps mixed key, negative key, and runtime scalar needle loops out of the main builtin emitter.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_search`
//! - mixed assignment paths through the wasm expression module
//!
//! Key details:
//! - Emits WAT loops over associative array entries and preserves explicit found-state handling.
//! - Unsupported needle/key combinations return CompileError instead of guessing PHP semantics.

mod loops;

use super::*;
pub(super) use loops::{
    emit_assoc_array_search_float, emit_assoc_array_search_runtime_bool,
    emit_assoc_array_search_runtime_float, emit_assoc_array_search_runtime_int,
    emit_assoc_array_search_runtime_string, emit_assoc_array_search_string,
    emit_assoc_array_search_tag_payload,
};
use loops::{emit_assoc_array_search_entry_loop, emit_assoc_search_store_key};

pub(super) fn assoc_key_kinds_are_mixed(key_kinds: &[AssocKeyKind]) -> bool {
    key_kinds.iter().any(|kind| *kind == AssocKeyKind::Int)
        && key_kinds.iter().any(|kind| *kind == AssocKeyKind::Str)
}

pub(super) fn mixed_key_search_needle_is_supported(needle: &Expr, module: &WasmModule) -> bool {
    static_or_const_or_i64_local_value(needle, module).is_some()
        || static_bool_value_cell_needle(needle, module).is_some()
        || static_or_const_or_f64_local_value(needle, module).is_some()
        || static_string_value(needle, module).is_some()
        || matches!(needle.kind, ExprKind::Null)
        || (expression_is_stringy(needle, module) && static_string_value(needle, module).is_none())
        || expression_is_booly(needle, module)
        || expression_is_floaty(needle, module)
        || expression_is_inty(needle, module)
}

pub(super) fn emit_assoc_array_search_mixed_key_with_found(
    call: &Expr,
    var: &str,
    needle: &Expr,
    result: &str,
    key_kind: &str,
    found: &str,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let static_string_needle = static_string_value(needle, module);
    let static_int_needle = static_or_const_or_i64_local_value(needle, module);
    let static_bool_needle = static_bool_value_cell_needle(needle, module);
    let static_float_needle = static_or_const_or_f64_local_value(needle, module);
    let static_null_needle = matches!(needle.kind, ExprKind::Null);
    let runtime_needle = if static_string_needle.is_none()
        && static_int_needle.is_none()
        && static_bool_needle.is_none()
        && static_float_needle.is_none()
        && !static_null_needle
    {
        runtime_string_arg_or_materialize(needle, "assoc_array_search_mixed_key_needle", module)?
    } else {
        None
    };
    if static_string_needle.is_none()
        && static_int_needle.is_none()
        && static_bool_needle.is_none()
        && static_float_needle.is_none()
        && !static_null_needle
        && runtime_needle.is_none()
    {
        return Ok(false);
    }
    module.body().line("i64.const -1");
    module.body().line(&format!("local.set {}", result));
    module.body().line("i32.const -1");
    module.body().line(&format!("local.set {}", key_kind));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    if let Some(needle_value) = static_string_needle {
        emit_assoc_array_search_string(var, &needle_value, result, found, Some(key_kind), Some(found), module);
    } else if let Some(needle_value) = static_int_needle {
        emit_assoc_array_search_tag_payload(
            var,
            WASM_VALUE_TAG_INT,
            needle_value,
            result,
            Some(key_kind),
            Some(found),
            module,
        );
    } else if let Some(needle_value) = static_bool_needle {
        emit_assoc_array_search_tag_payload(
            var,
            WASM_VALUE_TAG_BOOL,
            i64::from(needle_value),
            result,
            Some(key_kind),
            Some(found),
            module,
        );
    } else if let Some(needle_value) = static_float_needle {
        emit_assoc_array_search_float(var, needle_value, result, Some(key_kind), Some(found), module);
    } else if static_null_needle {
        emit_assoc_array_search_tag_payload(
            var,
            WASM_VALUE_TAG_NULL,
            0,
            result,
            Some(key_kind),
            Some(found),
            module,
        );
    } else if expression_is_booly(needle, module) {
        emit_assoc_array_search_runtime_bool(var, needle, result, Some(key_kind), Some(found), module)?;
    } else if expression_is_floaty(needle, module) {
        emit_assoc_array_search_runtime_float(var, needle, result, Some(key_kind), Some(found), module)?;
    } else if expression_is_inty(needle, module) {
        emit_assoc_array_search_runtime_int(var, needle, result, Some(key_kind), Some(found), module)?;
    } else if let Some(needle_var) = runtime_needle {
        emit_assoc_array_search_runtime_string(var, &needle_var, result, found, Some(key_kind), Some(found), module);
    } else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web array_search() over mixed-key associative arrays requires a supported scalar needle",
        ));
    }
    Ok(true)
}

pub(super) fn emit_output_assoc_array_search_mixed_key(
    call: &Expr,
    var: &str,
    needle: &Expr,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let result = module.next_label("array_search_runtime_mixed_key_result");
    let key_kind = module.next_label("array_search_runtime_mixed_key_kind");
    let found = module.next_label("array_search_runtime_mixed_key_found");
    module.declare_i64_local(result.trim_start_matches('$').to_string());
    for local in [&key_kind, &found] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    if !emit_assoc_array_search_mixed_key_with_found(call, var, needle, &result, &key_kind, &found, module)? {
        return Ok(false);
    }
    module.body().line(&format!("local.get {}", found));
    module.body().open("if");
    module.body().line(&format!("local.get {}", key_kind));
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", result));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.get {}", result));
    module.body().line("i64.const 32");
    module.body().line("i64.shr_u");
    module.body().line("i32.wrap_i64");
    module.body().line("call $host_write");
    module.body().close("else");
    module.body().line(&format!("local.get {}", result));
    module.body().line("call $host_write_int");
    module.body().close("end");
    module.body().close("end");
    Ok(true)
}

pub(super) fn emit_assoc_array_search_mixed_key_index(
    call: &Expr,
    var: &str,
    needle: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    reject_negative_int_key_array_search(call, var, module)?;
    let result = module.next_label("assoc_array_search_mixed_key_result");
    let matched = module.next_label("assoc_array_search_mixed_key_matched");
    module.declare_i64_local(result.trim_start_matches('$').to_string());
    module.declare_i32_local(matched.trim_start_matches('$').to_string());
    module.body().line("i64.const -1");
    module.body().line(&format!("local.set {}", result));
    if let Some(needle_value) = static_string_value(needle, module) {
        emit_assoc_array_search_string(var, &needle_value, &result, &matched, None, None, module);
    } else if let Some(needle_value) = static_or_const_or_i64_local_value(needle, module) {
        emit_assoc_array_search_tag_payload(
            var,
            WASM_VALUE_TAG_INT,
            needle_value,
            &result,
            None,
            None,
            module,
        );
    } else if let Some(needle_value) = static_bool_value_cell_needle(needle, module) {
        emit_assoc_array_search_tag_payload(
            var,
            WASM_VALUE_TAG_BOOL,
            i64::from(needle_value),
            &result,
            None,
            None,
            module,
        );
    } else if let Some(needle_value) = static_or_const_or_f64_local_value(needle, module) {
        emit_assoc_array_search_float(var, needle_value, &result, None, None, module);
    } else if matches!(needle.kind, ExprKind::Null) {
        emit_assoc_array_search_tag_payload(
            var,
            WASM_VALUE_TAG_NULL,
            0,
            &result,
            None,
            None,
            module,
        );
    } else if let Some(needle_var) =
        runtime_string_arg_or_materialize(needle, "assoc_array_search_mixed_key_needle", module)?
    {
        emit_assoc_array_search_runtime_string(var, &needle_var, &result, &matched, None, None, module);
    } else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web array_search() over mixed-key associative arrays requires a static int or string needle",
        ));
    }
    module.body().line(&format!("local.get {}", result));
    Ok(())
}

pub(super) fn reject_negative_int_key_array_search(
    call: &Expr,
    var: &str,
    module: &WasmModule,
) -> Result<(), CompileError> {
    if module.array_key_values(var).is_some_and(|keys| {
        keys.iter()
            .any(|key| matches!(key, AssocKeyValue::Int(value) if *value < 0))
    }) {
        return Err(CompileError::new(
            call.span,
            "wasm32-web array_search() over associative arrays with negative integer keys requires explicit found-state lowering",
        ));
    }
    Ok(())
}

pub(super) fn emit_assoc_array_search_loose_scalar_packed_key(
    _call: &Expr,
    var: &str,
    needle: &Expr,
    result: &str,
    key_kind: Option<&str>,
    found: Option<&str>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let needle_cell = module.next_label("assoc_array_search_loose_needle");
    let matched = module.next_label("assoc_array_search_loose_matched");
    for local in [&needle_cell, &matched] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i64.const -1");
    module.body().line(&format!("local.set {}", result));
    if let Some(found) = found {
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", found));
    }
    module.body().line("i32.const 1");
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", needle_cell));
    emit_store_value_cell(&needle_cell, needle, module).map_err(|_| {
        CompileError::new(
            needle.span,
            "wasm32-web loose array_search() requires a supported scalar needle",
        )
    })?;
    emit_assoc_array_search_entry_loop(var, module, |entry, cell, done_label, module| {
        emit_value_cell_ptrs_php_loose_equal_between(cell, &needle_cell, &matched, module);
        module.body().line(&format!("local.get {}", matched));
        module.body().open("if");
        emit_assoc_search_store_key(entry, result, key_kind, found, done_label, module);
        module.body().close("end");
    });
    Ok(())
}

pub(super) fn assoc_array_has_negative_int_key(var: &str, module: &WasmModule) -> bool {
    module.array_key_values(var).is_some_and(|values| {
        values
            .iter()
            .any(|value| matches!(value, AssocKeyValue::Int(key) if *key < 0))
    })
}

pub(super) fn negative_key_array_search_needle_is_supported(needle: &Expr, module: &WasmModule) -> bool {
    static_or_const_or_i64_local_value(needle, module).is_some()
        || static_bool_value_cell_needle(needle, module).is_some()
        || static_or_const_or_f64_local_value(needle, module).is_some()
        || static_string_value(needle, module).is_some()
        || matches!(needle.kind, ExprKind::Null)
        || (expression_is_stringy(needle, module) && static_string_value(needle, module).is_none())
}

pub(super) fn emit_assoc_array_search_negative_key_found(
    call: &Expr,
    var: &str,
    needle: &Expr,
    result: &str,
    found: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    if let Some(value) = static_or_const_or_i64_local_value(needle, module) {
        emit_assoc_array_search_tag_payload(var, WASM_VALUE_TAG_INT, value, result, None, Some(found), module);
        return Ok(());
    }
    if let Some(value) = static_bool_value_cell_needle(needle, module) {
        emit_assoc_array_search_tag_payload(
            var,
            WASM_VALUE_TAG_BOOL,
            i64::from(value),
            result,
            None,
            Some(found),
            module,
        );
        return Ok(());
    }
    if let Some(value) = static_or_const_or_f64_local_value(needle, module) {
        emit_assoc_array_search_float(var, value, result, None, Some(found), module);
        return Ok(());
    }
    if matches!(needle.kind, ExprKind::Null) {
        emit_assoc_array_search_tag_payload(var, WASM_VALUE_TAG_NULL, 0, result, None, Some(found), module);
        return Ok(());
    }
    let matched = module.next_label("assoc_array_search_negative_key_matched");
    module.declare_i32_local(matched.trim_start_matches('$').to_string());
    if let Some(value) = static_string_value(needle, module) {
        emit_assoc_array_search_string(var, &value, result, &matched, None, Some(found), module);
        return Ok(());
    }
    if let Some(needle_var) =
        runtime_string_arg_or_materialize(needle, "assoc_array_search_negative_key_needle", module)?
    {
        emit_assoc_array_search_runtime_string(var, &needle_var, result, &matched, None, Some(found), module);
        return Ok(());
    }
    if expression_is_booly(needle, module) {
        emit_assoc_array_search_runtime_bool(var, needle, result, None, Some(found), module)?;
        return Ok(());
    }
    if expression_is_floaty(needle, module) {
        emit_assoc_array_search_runtime_float(var, needle, result, None, Some(found), module)?;
        return Ok(());
    }
    if expression_is_inty(needle, module) {
        emit_assoc_array_search_runtime_int(var, needle, result, None, Some(found), module)?;
        return Ok(());
    }
    Err(CompileError::new(
        call.span,
        "wasm32-web array_search() over negative-key associative arrays requires a static int or string needle",
    ))
}

pub(super) fn emit_assoc_array_search_packed_key(
    call: &Expr,
    var: &str,
    needle: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let result = module.next_label("assoc_array_search_packed_result");
    let matched = module.next_label("assoc_array_search_packed_matched");
    module.declare_i64_local(result.trim_start_matches('$').to_string());
    module.declare_i32_local(matched.trim_start_matches('$').to_string());
    module.body().line("i64.const -1");
    module.body().line(&format!("local.set {}", result));
    if static_string_value(needle, module).is_none() {
        if let Some(needle_var) = runtime_string_arg_or_materialize(needle, "assoc_array_search_string_key_needle", module)? {
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
        ExprKind::Null => {
            emit_assoc_array_search_tag_payload(var, WASM_VALUE_TAG_NULL, 0, &result, None, None, module)
        }
        _ => {
            let value = static_or_const_or_i64_local_value(needle, module).ok_or_else(|| {
                CompileError::new(
                    call.span,
                    "wasm32-web array_search() over associative arrays requires a static scalar needle",
                )
            })?;
            emit_assoc_array_search_tag_payload(var, WASM_VALUE_TAG_INT, value, &result, None, None, module)
        }
    }
    module.body().line(&format!("local.get {}", result));
    Ok(())
}
