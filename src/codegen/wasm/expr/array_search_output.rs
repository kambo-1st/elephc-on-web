//! Purpose:
//! Lowers wasm32-web output-specific array_search() paths.
//! Keeps string-key and printed-result handling separate from value-position search lowering.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` output lowering.
//!
//! Key details:
//! - Preserves packed key output for associative arrays and delegates index searches to array_search.

use super::*;
use super::array_search::*;
use super::array_search_assoc::{
    emit_assoc_array_search_runtime_string, emit_assoc_array_search_string,
    emit_assoc_array_search_tag_payload,
    emit_output_assoc_array_search_mixed_key, mixed_key_search_needle_is_supported,
    reject_negative_int_key_array_search,
};
use super::array_search_static::{
    static_loose_assoc_string_key_search_result,
};

pub(super) fn emit_output_array_search(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if emit_output_unknown_mixed_array_search(call, args, module)? {
        return Ok(());
    }
    if emit_output_array_search_string_key(call, args, module)? {
        return Ok(());
    }
    let result = module.next_label("array_search_output_result");
    module.declare_i64_local(result.trim_start_matches('$').to_string());
    emit_array_search_index_from_args(call, args, module)?;
    module.body().line(&format!("local.set {}", result));
    module.body().line(&format!("local.get {}", result));
    module.body().line("i64.const 0");
    module.body().line("i64.ge_s");
    module.body().open("if");
    module.body().line(&format!("local.get {}", result));
    module.body().line("call $host_write_int");
    module.body().close("end");
    Ok(())
}

fn emit_output_unknown_mixed_array_search(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let (needle, haystack, strict) = match args {
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
    let ExprKind::Variable(source) = &haystack.kind else {
        return Ok(false);
    };
    if module.local_kind(source) != Some(LocalKind::Mixed) || module.mixed_value_cell_kind(source).is_some() {
        return Ok(false);
    }
    if !mixed_key_search_needle_is_supported(needle, module) {
        return Err(CompileError::new(
            needle.span,
            "wasm32-web direct output array_search() over unknown mixed arrays requires a supported scalar needle",
        ));
    }
    let temp = module
        .next_label("unknown_mixed_array_search_output_source")
        .trim_start_matches('$')
        .to_string();
    let heap_kind = module.next_label("unknown_mixed_array_search_output_heap_kind");
    module.declare_array_local(temp.clone());
    module.declare_i32_local(heap_kind.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_ptr");
    module.body().line(&format!("local.set ${}_ptr", temp));
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_len_i32");
    module.body().line(&format!("local.set ${}_len", temp));
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_heap_kind");
    module.body().line(&format!("local.set {}", heap_kind));
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_INDEXED_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.set_array_layout(&temp, ArrayLayout::Value);
    module.set_array_value_cell_kinds(&temp, None);
    let result = module.next_label("unknown_mixed_array_search_output_index_result");
    module.declare_i64_local(result.trim_start_matches('$').to_string());
    emit_array_search_index_from_args(
        call,
        &[
            needle.clone(),
            Expr {
                kind: ExprKind::Variable(temp.clone()),
                span: haystack.span,
            },
            Expr {
                kind: ExprKind::BoolLiteral(strict),
                span: haystack.span,
            },
        ],
        module,
    )?;
    module.body().line(&format!("local.set {}", result));
    module.body().line(&format!("local.get {}", result));
    module.body().line("i64.const 0");
    module.body().line("i64.ge_s");
    module.body().open("if");
    module.body().line(&format!("local.get {}", result));
    module.body().line("call $host_write_int");
    module.body().close("end");
    module.body().close("else");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.set_array_layout(&temp, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(&temp, None);
    module.set_array_key_kinds(&temp, None);
    if !strict {
        let result = module.next_label("unknown_mixed_array_search_output_loose_assoc_result");
        let key_kind = module.next_label("unknown_mixed_array_search_output_loose_assoc_key_kind");
        let found = module.next_label("unknown_mixed_array_search_output_loose_assoc_found");
        module.declare_i64_local(result.trim_start_matches('$').to_string());
        for local in [&key_kind, &found] {
            module.declare_i32_local(local.trim_start_matches('$').to_string());
        }
        emit_assoc_array_search_loose_scalar_packed_key(
            call,
            &temp,
            needle,
            &result,
            Some(&key_kind),
            Some(&found),
            module,
        )?;
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
        module.body().close("else");
        module.body().line("unreachable");
        module.body().close("end");
        module.body().close("end");
        return Ok(true);
    }
    if !emit_output_assoc_array_search_mixed_key(call, &temp, needle, module)? {
        return Err(CompileError::new(
            needle.span,
            "wasm32-web direct output array_search() over unknown mixed associative arrays requires a supported scalar needle",
        ));
    }
    module.body().close("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    Ok(true)
}

fn emit_output_array_search_string_key(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let (needle, haystack, strict) = match args {
        [needle, haystack, strict] => {
            let Some(strict) = static_or_const_or_i32_bool_value(strict, module) else {
                return Err(CompileError::new(
                    strict.span,
                    "wasm32-web array_search() strict argument must be a static bool",
                ));
            };
            (needle, haystack, strict)
        }
        [needle, haystack] => (needle, haystack, false),
        _ => {
            return Err(CompileError::new(
                call.span,
                "wasm32-web array_search() expects two or three arguments",
            ))
        }
    };
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
                .next_label("array_search_string_key_source")
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
                .next_label("array_search_string_key_return")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, haystack, module)?;
            temp.as_str()
        }
        ExprKind::ArrayAccess { .. }
            if nested_array_metadata_for_access_expr(haystack, module).is_some() =>
        {
            temp = materialize_nested_array_search_source(haystack, module)?;
            if module.array_layout(&temp) != ArrayLayout::Assoc {
                return Ok(false);
            }
            temp.as_str()
        }
        ExprKind::ExprCall { callee, .. }
            if callable_expr_array_return_metadata(callee, module)
                .is_some_and(|metadata| metadata.layout == ArrayLayout::Assoc) =>
        {
            temp = module
                .next_label("array_search_string_key_direct")
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
                .next_label("array_search_string_key_direct")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, haystack, module)?;
            temp.as_str()
        }
        _ => return Ok(false),
    };
    let Some(key_kinds) = module.array_key_kinds(var) else {
        if module.array_runtime_key_kind(var) != Some(AssocKeyKind::Str) {
            if module.array_has_php_normalized_runtime_keys(var)
                && mixed_key_search_needle_is_supported(needle, module)
            {
                return emit_output_assoc_array_search_mixed_key(call, var, needle, module);
            }
            return Ok(false);
        }
        if !strict {
            if module.array_runtime_value_cell_kind(var).is_some() {
                let result = module.next_label("array_search_runtime_string_key_loose_result");
                module.declare_i64_local(result.trim_start_matches('$').to_string());
                emit_assoc_array_search_loose_scalar_packed_key(call, var, needle, &result, None, None, module)?;
                module.body().line(&format!("local.get {}", result));
                module.body().line("i64.const 0");
                module.body().line("i64.ge_s");
                module.body().open("if");
                module.body().line(&format!("local.get {}", result));
                module.body().line("i32.wrap_i64");
                module.body().line(&format!("local.get {}", result));
                module.body().line("i64.const 32");
                module.body().line("i64.shr_u");
                module.body().line("i32.wrap_i64");
                module.body().line("call $host_write");
                module.body().close("end");
                return Ok(true);
            }
            return Err(CompileError::new(
                call.span,
                "wasm32-web loose array_search() over runtime string-key associative arrays requires scalar value metadata",
            ));
        }
        let result = module.next_label("array_search_runtime_string_key_result");
        module.declare_i64_local(result.trim_start_matches('$').to_string());
        emit_assoc_array_search_packed_key(call, var, needle, module)?;
        module.body().line(&format!("local.set {}", result));
        module.body().line(&format!("local.get {}", result));
        module.body().line("i64.const 0");
        module.body().line("i64.ge_s");
        module.body().open("if");
        module.body().line(&format!("local.get {}", result));
        module.body().line("i32.wrap_i64");
        module.body().line(&format!("local.get {}", result));
        module.body().line("i64.const 32");
        module.body().line("i64.shr_u");
        module.body().line("i32.wrap_i64");
        module.body().line("call $host_write");
        module.body().close("end");
        return Ok(true);
    };
    if key_kinds.iter().all(|kind| *kind == AssocKeyKind::Int)
        && assoc_array_has_negative_int_key(var, module)
        && negative_key_array_search_needle_is_supported(needle, module)
    {
        let result = module.next_label("array_search_negative_int_key_result");
        let found = module.next_label("array_search_negative_int_key_found");
        module.declare_i64_local(result.trim_start_matches('$').to_string());
        module.declare_i32_local(found.trim_start_matches('$').to_string());
        if strict {
            emit_assoc_array_search_negative_key_found(call, var, needle, &result, &found, module)?;
        } else {
            emit_assoc_array_search_loose_scalar_packed_key(call, var, needle, &result, None, Some(&found), module)?;
        }
        module.body().line(&format!("local.get {}", found));
        module.body().open("if");
        module.body().line(&format!("local.get {}", result));
        module.body().line("call $host_write_int");
        module.body().close("end");
        return Ok(true);
    }
    reject_negative_int_key_array_search(call, var, module)?;
    if key_kinds
        .iter()
        .any(|kind| *kind == AssocKeyKind::Int)
        && key_kinds.iter().any(|kind| *kind == AssocKeyKind::Str)
    {
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
            runtime_string_arg_or_materialize(needle, "array_search_mixed_key_needle", module)?
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
        let result = module.next_label("array_search_mixed_key_result");
        let key_kind = module.next_label("array_search_mixed_key_kind");
        let matched = module.next_label("array_search_mixed_key_matched");
        module.declare_i64_local(result.trim_start_matches('$').to_string());
        for local in [&key_kind, &matched] {
            module.declare_i32_local(local.trim_start_matches('$').to_string());
        }
        module.body().line("i64.const -1");
        module.body().line(&format!("local.set {}", result));
        module.body().line("i32.const -1");
        module.body().line(&format!("local.set {}", key_kind));
        if let Some(needle_value) = static_string_needle {
            emit_assoc_array_search_string(var, &needle_value, &result, &matched, Some(&key_kind), None, module);
        } else if let Some(needle_value) = static_int_needle {
            emit_assoc_array_search_tag_payload(
                var,
                WASM_VALUE_TAG_INT,
                needle_value,
                &result,
                Some(&key_kind),
                None,
                module,
            );
        } else if let Some(needle_value) = static_bool_needle {
            emit_assoc_array_search_tag_payload(
                var,
                WASM_VALUE_TAG_BOOL,
                i64::from(needle_value),
                &result,
                Some(&key_kind),
                None,
                module,
            );
        } else if let Some(needle_value) = static_float_needle {
            emit_assoc_array_search_float(var, needle_value, &result, Some(&key_kind), None, module);
        } else if static_null_needle {
            emit_assoc_array_search_tag_payload(
                var,
                WASM_VALUE_TAG_NULL,
                0,
                &result,
                Some(&key_kind),
                None,
                module,
            );
        } else if let Some(needle_var) = runtime_needle {
            emit_assoc_array_search_runtime_string(var, &needle_var, &result, &matched, Some(&key_kind), None, module);
        }
        module.body().line(&format!("local.get {}", result));
        module.body().line("i64.const 0");
        module.body().line("i64.ge_s");
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
        return Ok(true);
    }
    if !key_kinds.iter().all(|kind| *kind == AssocKeyKind::Str) {
        return Ok(false);
    }
    if !strict {
        if let Some(key) = static_loose_assoc_string_key_search_result(var, needle, module) {
            if let Some(key) = key {
                let (key_ptr, key_len) = module.intern_string(&key);
                module.body().line(&format!("i32.const {}", key_ptr));
                module.body().line(&format!("i32.const {}", key_len));
                module.body().line("call $host_write");
            }
            return Ok(true);
        }
        let has_scalar_value_metadata = module
            .array_runtime_value_cell_kind(var)
            .is_some_and(|kind| kind != ValueCellKind::Array)
            || module
                .array_value_cell_kinds(var)
                .is_some_and(|kinds| kinds.iter().all(|kind| *kind != ValueCellKind::Array));
        if has_scalar_value_metadata {
            let result = module.next_label("array_search_string_key_loose_result");
            module.declare_i64_local(result.trim_start_matches('$').to_string());
            emit_assoc_array_search_loose_scalar_packed_key(call, var, needle, &result, None, None, module)?;
            module.body().line(&format!("local.get {}", result));
            module.body().line("i64.const 0");
            module.body().line("i64.ge_s");
            module.body().open("if");
            module.body().line(&format!("local.get {}", result));
            module.body().line("i32.wrap_i64");
            module.body().line(&format!("local.get {}", result));
            module.body().line("i64.const 32");
            module.body().line("i64.shr_u");
            module.body().line("i32.wrap_i64");
            module.body().line("call $host_write");
            module.body().close("end");
            return Ok(true);
        }
        return Err(CompileError::new(
            call.span,
            "wasm32-web loose array_search() over string-key associative arrays requires exact scalar value metadata",
        ));
    }
    let result = module.next_label("array_search_string_key_result");
    module.declare_i64_local(result.trim_start_matches('$').to_string());
    emit_assoc_array_search_packed_key(call, var, needle, module)?;
    module.body().line(&format!("local.set {}", result));
    module.body().line(&format!("local.get {}", result));
    module.body().line("i64.const 0");
    module.body().line("i64.ge_s");
    module.body().open("if");
    module.body().line(&format!("local.get {}", result));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.get {}", result));
    module.body().line("i64.const 32");
    module.body().line("i64.shr_u");
    module.body().line("i32.wrap_i64");
    module.body().line("call $host_write");
    module.body().close("end");
    Ok(true)
}
