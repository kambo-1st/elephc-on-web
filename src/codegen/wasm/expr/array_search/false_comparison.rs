//! Purpose:
//! Emits wasm32-web `array_search(...) === false` and `!== false` special handling.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_search`.
//!
//! Key details:
//! - Preserves PHP's ambiguity between key `0` and boolean false for negative/string-key cases.
//! - Returns `Ok(false)` when the comparison should fall back to normal scalar lowering.

use super::*;
use super::super::array_membership::emit_unknown_mixed_array_in_array_call;

pub(in crate::codegen::wasm::expr) fn array_search_false_comparison<'a>(
    candidate: &'a Expr,
    false_expr: &Expr,
) -> Option<&'a Expr> {
    if !matches!(false_expr.kind, ExprKind::BoolLiteral(false)) {
        return None;
    }
    match &candidate.kind {
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("array_search") => {
            Some(candidate)
        }
        _ => None,
    }
}

pub(in crate::codegen::wasm::expr) fn emit_array_search_false_comparison_bool(
    call: &Expr,
    strict_not_eq: bool,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let ExprKind::FunctionCall { name, args } = &call.kind else {
        return Ok(false);
    };
    if !name.eq_ignore_ascii_case("array_search") {
        return Ok(false);
    }
    let (needle, haystack, strict) = match args.as_slice() {
        [needle, haystack] => (needle, haystack, false),
        [needle, haystack, strict] => {
            let Some(strict) = static_or_const_or_i32_bool_value(strict, module) else {
                return Err(CompileError::new(
                    strict.span,
                    "wasm32-web array_search() strict argument must be a static bool",
                ));
            };
            (needle, haystack, strict)
        }
        _ => {
            return Err(CompileError::new(
                call.span,
                "wasm32-web array_search() expects two or three arguments",
            ))
        }
    };
    if let ExprKind::Variable(var) = &haystack.kind {
        if module.local_kind(var) == Some(LocalKind::Mixed) && module.mixed_value_cell_kind(var).is_none() {
            let found = module.next_label("array_search_unknown_mixed_false_found");
            module.declare_i32_local(found.trim_start_matches('$').to_string());
            module.body().line("i32.const 0");
            module.body().line(&format!("local.set {}", found));
            emit_unknown_mixed_array_in_array_call(needle, var, strict, &found, module)?;
            module.body().line(&format!("local.get {}", found));
            if !strict_not_eq {
                module.body().line("i32.eqz");
            }
            return Ok(true);
        }
    }
    let temp;
    let var = match &haystack.kind {
        ExprKind::Variable(var)
            if module.local_kind(var) == Some(LocalKind::Array)
                && module.array_layout(var) == ArrayLayout::Assoc =>
        {
            var.as_str()
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            temp = module
                .next_label("array_search_false_negative_key_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_assoc_array_items_assign(&temp, items, module)?;
            temp.as_str()
        }
        ExprKind::FunctionCall { name, .. }
            if module.has_function(name)
                && module.function_return_kind(name) == Some(ValueKind::Array)
                && module.function_array_return_layout(name) == ArrayLayout::Assoc =>
        {
            temp = module
                .next_label("array_search_false_negative_key_return")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, haystack, module)?;
            temp.as_str()
        }
        ExprKind::ExprCall { callee, .. }
            if callable_expr_array_return_metadata(callee, module)
                .is_some_and(|metadata| metadata.layout == ArrayLayout::Assoc) =>
        {
            temp = module
                .next_label("array_search_false_expr_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, haystack, module)?;
            temp.as_str()
        }
        _ if expression_has_array_type(haystack, module)
            && !expression_is_arrayy(haystack, module)
            && !matches!(haystack.kind, ExprKind::ExprCall { .. }) =>
        {
            temp = module
                .next_label("array_search_false_expr_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, haystack, module)?;
            if module.array_layout(&temp) != ArrayLayout::Assoc {
                return Ok(false);
            }
            temp.as_str()
        }
        _ => return Ok(false),
    };
    let Some(key_kinds) = module.array_key_kinds(var) else {
        if !strict {
            return Ok(false);
        }
        if module.array_runtime_key_kind(var) != Some(AssocKeyKind::Str) {
            return Ok(false);
        }
        let result = module.next_label("array_search_runtime_string_key_false_result");
        module.declare_i64_local(result.trim_start_matches('$').to_string());
        emit_assoc_array_search_packed_key(call, var, needle, module)?;
        module.body().line(&format!("local.set {}", result));
        module.body().line(&format!("local.get {}", result));
        module.body().line("i64.const 0");
        module.body().line("i64.ge_s");
        if !strict_not_eq {
            module.body().line("i32.eqz");
        }
        return Ok(true);
    };
    if !key_kinds.iter().all(|kind| *kind == AssocKeyKind::Int)
        || !assoc_array_has_negative_int_key(var, module)
        || !negative_key_array_search_needle_is_supported(needle, module)
    {
        return Ok(false);
    }
    let result = module.next_label("array_search_false_negative_key_result");
    let found = module.next_label("array_search_false_negative_key_found");
    module.declare_i64_local(result.trim_start_matches('$').to_string());
    module.declare_i32_local(found.trim_start_matches('$').to_string());
    if strict {
        emit_assoc_array_search_negative_key_found(call, var, needle, &result, &found, module)?;
    } else {
        emit_assoc_array_search_loose_scalar_packed_key(call, var, needle, &result, None, Some(&found), module)?;
    }
    module.body().line(&format!("local.get {}", found));
    if !strict_not_eq {
        module.body().line("i32.eqz");
    }
    Ok(true)
}
