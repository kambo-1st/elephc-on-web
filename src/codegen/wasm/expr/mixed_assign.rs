//! Purpose:
//! Owns focused wasm32-web helpers used while assigning values into Mixed locals.
//! Keeps small result-wrapping paths out of the main expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::emit_assign_value`
//! - mixed local materialization paths in `crate::codegen::wasm::expr`
//!
//! Key details:
//! - Helpers produce boxed Mixed cells using the shared value-cell runtime helpers.
//! - Assignment release is skipped for current parameters because callers do not own them.

use super::*;
use super::array_search::materialize_nested_array_search_source;
use super::array_search_assoc::{
    emit_assoc_array_search_runtime_string, emit_assoc_array_search_string,
    emit_assoc_array_search_tag_payload,
};

pub(in crate::codegen::wasm) fn emit_mixed_assign(
    name: &str,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_release_mixed_local_before_assign(name, module);
    if let ExprKind::FunctionCall { name: function_name, args } = &value.kind {
        if module.has_function(function_name)
            && module.function_return_kind(function_name) == Some(ValueKind::Mixed)
        {
            emit_user_function_args(value, function_name, args, module)?;
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(function_name)));
            module.body().line(&format!("local.set ${}", name));
            module.set_mixed_value_cell_kind(name, module.function_mixed_return_kind(function_name));
            return Ok(());
        }
        if matches!(
            function_name.to_ascii_lowercase().as_str(),
            "array_pop" | "array_shift"
        ) {
            module.set_mixed_value_cell_kind(name, None);
            if function_name.eq_ignore_ascii_case("array_pop")
                && emit_unknown_mixed_array_pop_assign(name, value, args, module)?
            {
                return Ok(());
            }
            if function_name.eq_ignore_ascii_case("array_shift")
                && emit_unknown_mixed_array_shift_assign(name, value, args, module)?
            {
                return Ok(());
            }
            return emit_mixed_value_array_pop_shift_assign(name, value, function_name, args, module);
        }
        if function_name.eq_ignore_ascii_case("array_search") {
            module.set_mixed_value_cell_kind(name, None);
            return emit_mixed_array_search_assign(name, value, args, module);
        }
        if function_name.eq_ignore_ascii_case("array_rand") {
            return emit_mixed_array_rand_assign(name, value, args, module);
        }
        if matches!(function_name.to_ascii_lowercase().as_str(), "strpos" | "strrpos") {
            module.set_mixed_value_cell_kind(name, None);
            return emit_mixed_string_search_assign(name, value, function_name, args, module);
        }
    }
    emit_mixed_arg_assign(name, value, module)
}

pub(super) fn emit_mixed_array_rand_assign(
    target: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match emit_array_rand_call(call, args, module)? {
        ValueKind::Mixed => {
            module.body().line(&format!("local.set ${}", target));
            module.set_mixed_value_cell_kind(target, None);
        }
        ValueKind::Int => {
            let result = module.next_label("mixed_array_rand_result");
            module.declare_i64_local(result.trim_start_matches('$').to_string());
            module.body().line(&format!("local.set {}", result));
            emit_alloc_mixed_cell(target, module);
            module.body().line(&format!("local.get ${}", target));
            module.body().line(&format!("local.get {}", result));
            module.body().line("call $__rt_value_store_int");
            module.set_mixed_value_cell_kind(target, Some(ValueCellKind::Int));
        }
        _ => unreachable!("array_rand() emits either an integer key or a mixed associative key"),
    }
    Ok(())
}

pub(super) fn emit_release_mixed_local_before_assign(name: &str, module: &mut WasmModule) {
    if module.is_current_param(name) {
        return;
    }
    module.body().line(&format!("local.get ${}", name));
    module.body().line("call $__rt_value_release");
}

fn emit_load_unknown_mixed_array_header(
    source: &str,
    ptr: &str,
    len: &str,
    heap_kind: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_ptr");
    module.body().line(&format!("local.set {}", ptr));
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_len_i32");
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_heap_kind");
    module.body().line(&format!("local.set {}", heap_kind));
}

pub(super) fn emit_mixed_string_search_assign(
    target: &str,
    call: &Expr,
    function_name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let result = module.next_label("mixed_string_search_result");
    module.declare_i64_local(result.trim_start_matches('$').to_string());
    emit_alloc_mixed_cell(target, module);
    emit_string_search_index_from_args(call, function_name, args, module)?;
    module.body().line(&format!("local.set {}", result));
    module.body().line(&format!("local.get {}", result));
    module.body().line("i64.const 0");
    module.body().line("i64.ge_s");
    module.body().open("if");
    module.body().line(&format!("local.get ${}", target));
    module.body().line(&format!("local.get {}", result));
    module.body().line("call $__rt_value_store_int");
    module.body().close("else");
    module.body().line(&format!("local.get ${}", target));
    module.body().line("i32.const 0");
    module.body().line("call $__rt_value_store_bool");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_mixed_array_search_assign(
    target: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if emit_mixed_unknown_array_search_assign(target, call, args, module)? {
        return Ok(());
    }
    if emit_mixed_assoc_array_search_string_key_assign(target, call, args, module)? {
        return Ok(());
    }
    let result = module.next_label("mixed_array_search_result");
    module.declare_i64_local(result.trim_start_matches('$').to_string());
    emit_alloc_mixed_cell(target, module);
    emit_array_search_index_from_args(call, args, module)?;
    module.body().line(&format!("local.set {}", result));
    module.body().line(&format!("local.get {}", result));
    module.body().line("i64.const 0");
    module.body().line("i64.ge_s");
    module.body().open("if");
    module.body().line(&format!("local.get ${}", target));
    module.body().line(&format!("local.get {}", result));
    module.body().line("call $__rt_value_store_int");
    module.body().close("else");
    module.body().line(&format!("local.get ${}", target));
    module.body().line("i32.const 0");
    module.body().line("call $__rt_value_store_bool");
    module.body().close("end");
    Ok(())
}

fn emit_mixed_unknown_array_search_assign(
    target: &str,
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
    let ExprKind::Variable(source) = &haystack.kind else {
        return Ok(false);
    };
    if module.local_kind(source) != Some(LocalKind::Mixed) || module.mixed_value_cell_kind(source).is_some() {
        return Ok(false);
    }
    if !mixed_key_search_needle_is_supported(needle, module) {
        return Err(CompileError::new(
            needle.span,
            "wasm32-web assigned array_search() over unknown mixed arrays requires a supported scalar needle",
        ));
    }
    let temp = module
        .next_label("unknown_mixed_array_search_assign_source")
        .trim_start_matches('$')
        .to_string();
    let heap_kind = module.next_label("unknown_mixed_array_search_assign_heap_kind");
    module.declare_array_local(temp.clone());
    module.declare_i32_local(heap_kind.trim_start_matches('$').to_string());
    emit_load_unknown_mixed_array_header(
        source,
        &format!("${}_ptr", temp),
        &format!("${}_len", temp),
        &heap_kind,
        module,
    );
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_INDEXED_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.set_array_layout(&temp, ArrayLayout::Value);
    module.set_array_value_cell_kinds(&temp, None);
    let result = module.next_label("unknown_mixed_array_search_assign_index_result");
    module.declare_i64_local(result.trim_start_matches('$').to_string());
    emit_alloc_mixed_cell(target, module);
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
    module.body().line(&format!("local.get ${}", target));
    module.body().line(&format!("local.get {}", result));
    module.body().line("call $__rt_value_store_int");
    module.body().close("else");
    module.body().line(&format!("local.get ${}", target));
    module.body().line("i32.const 0");
    module.body().line("call $__rt_value_store_bool");
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
        let result = module.next_label("unknown_mixed_array_search_assign_loose_assoc_result");
        let key_kind = module.next_label("unknown_mixed_array_search_assign_loose_assoc_key_kind");
        let found = module.next_label("unknown_mixed_array_search_assign_loose_assoc_found");
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
        module.body().line(&format!("local.get ${}", target));
        module.body().line(&format!("local.get {}", result));
        module.body().line("i32.wrap_i64");
        module.body().line(&format!("local.get {}", result));
        module.body().line("i64.const 32");
        module.body().line("i64.shr_u");
        module.body().line("i32.wrap_i64");
        module.body().line("call $__rt_value_store_string");
        module.body().close("else");
        module.body().line(&format!("local.get ${}", target));
        module.body().line(&format!("local.get {}", result));
        module.body().line("call $__rt_value_store_int");
        module.body().close("end");
        module.body().close("else");
        module.body().line(&format!("local.get ${}", target));
        module.body().line("i32.const 0");
        module.body().line("call $__rt_value_store_bool");
        module.body().close("end");
        module.body().close("else");
        module.body().line("unreachable");
        module.body().close("end");
        module.body().close("end");
        module.set_mixed_value_cell_kind(target, None);
        return Ok(true);
    }
    if !emit_mixed_assoc_array_search_mixed_key_assign(target, call, &temp, needle, module)? {
        return Err(CompileError::new(
            needle.span,
            "wasm32-web assigned array_search() over unknown mixed associative arrays requires a supported scalar needle",
        ));
    }
    module.body().close("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    module.set_mixed_value_cell_kind(target, None);
    Ok(true)
}

fn emit_mixed_assoc_array_search_string_key_assign(
    target: &str,
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
                .next_label("array_search_mixed_string_key_source")
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
                .next_label("array_search_mixed_string_key_return")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, haystack, module)?;
            temp.as_str()
        }
        _ if expression_has_array_type(haystack, module) && !expression_is_arrayy(haystack, module) => {
            temp = module
                .next_label("array_search_mixed_string_key_direct")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, haystack, module)?;
            temp.as_str()
        }
        _ if nested_array_metadata_for_access_expr(haystack, module).is_some() => {
            temp = materialize_nested_array_search_source(haystack, module)?;
            temp.as_str()
        }
        _ => return Ok(false),
    };
    let Some(key_kinds) = module.array_key_kinds(var) else {
        if module.array_runtime_key_kind(var) != Some(AssocKeyKind::Str) {
            if module.array_has_php_normalized_runtime_keys(var)
                && mixed_key_search_needle_is_supported(needle, module)
            {
                return emit_mixed_assoc_array_search_mixed_key_assign(target, call, var, needle, module);
            }
            return Ok(false);
        }
        if !strict {
            if module.array_runtime_value_cell_kind(var).is_some() {
                let result = module.next_label("mixed_array_search_runtime_string_key_loose_result");
                let found = module.next_label("mixed_array_search_runtime_string_key_loose_found");
                module.declare_i64_local(result.trim_start_matches('$').to_string());
                module.declare_i32_local(found.trim_start_matches('$').to_string());
                emit_alloc_mixed_cell(target, module);
                emit_assoc_array_search_loose_scalar_packed_key(call, var, needle, &result, None, Some(&found), module)?;
                module.body().line(&format!("local.get {}", found));
                module.body().open("if");
                module.body().line(&format!("local.get ${}", target));
                module.body().line(&format!("local.get {}", result));
                module.body().line("i32.wrap_i64");
                module.body().line(&format!("local.get {}", result));
                module.body().line("i64.const 32");
                module.body().line("i64.shr_u");
                module.body().line("i32.wrap_i64");
                module.body().line("call $__rt_value_store_string");
                module.body().close("else");
                module.body().line(&format!("local.get ${}", target));
                module.body().line("i32.const 0");
                module.body().line("call $__rt_value_store_bool");
                module.body().close("end");
                return Ok(true);
            }
            return Err(CompileError::new(
                call.span,
                "wasm32-web loose array_search() assignment over runtime string-key associative arrays requires scalar value metadata",
            ));
        }
        let result = module.next_label("mixed_array_search_runtime_string_key_result");
        module.declare_i64_local(result.trim_start_matches('$').to_string());
        emit_alloc_mixed_cell(target, module);
        emit_assoc_array_search_packed_key(call, var, needle, module)?;
        module.body().line(&format!("local.set {}", result));
        module.body().line(&format!("local.get {}", result));
        module.body().line("i64.const 0");
        module.body().line("i64.ge_s");
        module.body().open("if");
        module.body().line(&format!("local.get ${}", target));
        module.body().line(&format!("local.get {}", result));
        module.body().line("i32.wrap_i64");
        module.body().line(&format!("local.get {}", result));
        module.body().line("i64.const 32");
        module.body().line("i64.shr_u");
        module.body().line("i32.wrap_i64");
        module.body().line("call $__rt_value_store_string");
        module.body().close("else");
        module.body().line(&format!("local.get ${}", target));
        module.body().line("i32.const 0");
        module.body().line("call $__rt_value_store_bool");
        module.body().close("end");
        return Ok(true);
    };
    if key_kinds.iter().all(|kind| *kind == AssocKeyKind::Int)
        && assoc_array_has_negative_int_key(var, module)
        && negative_key_array_search_needle_is_supported(needle, module)
    {
        let result = module.next_label("mixed_array_search_negative_int_key_result");
        let found = module.next_label("mixed_array_search_negative_int_key_found");
        module.declare_i64_local(result.trim_start_matches('$').to_string());
        module.declare_i32_local(found.trim_start_matches('$').to_string());
        emit_alloc_mixed_cell(target, module);
        if strict {
            emit_assoc_array_search_negative_key_found(call, var, needle, &result, &found, module)?;
        } else {
            emit_assoc_array_search_loose_scalar_packed_key(call, var, needle, &result, None, Some(&found), module)?;
        }
        module.body().line(&format!("local.get {}", found));
        module.body().open("if");
        module.body().line(&format!("local.get ${}", target));
        module.body().line(&format!("local.get {}", result));
        module.body().line("call $__rt_value_store_int");
        module.body().close("else");
        module.body().line(&format!("local.get ${}", target));
        module.body().line("i32.const 0");
        module.body().line("call $__rt_value_store_bool");
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
            runtime_string_arg_or_materialize(needle, "mixed_array_search_mixed_key_needle", module)?
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
        let result = module.next_label("mixed_array_search_mixed_key_result");
        let key_kind = module.next_label("mixed_array_search_mixed_key_kind");
        let matched = module.next_label("mixed_array_search_mixed_key_matched");
        module.declare_i64_local(result.trim_start_matches('$').to_string());
        for local in [&key_kind, &matched] {
            module.declare_i32_local(local.trim_start_matches('$').to_string());
        }
        emit_alloc_mixed_cell(target, module);
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
        module.body().line(&format!("local.get ${}", target));
        module.body().line(&format!("local.get {}", result));
        module.body().line("i32.wrap_i64");
        module.body().line(&format!("local.get {}", result));
        module.body().line("i64.const 32");
        module.body().line("i64.shr_u");
        module.body().line("i32.wrap_i64");
        module.body().line("call $__rt_value_store_string");
        module.body().close("else");
        module.body().line(&format!("local.get ${}", target));
        module.body().line(&format!("local.get {}", result));
        module.body().line("call $__rt_value_store_int");
        module.body().close("end");
        module.body().close("else");
        module.body().line(&format!("local.get ${}", target));
        module.body().line("i32.const 0");
        module.body().line("call $__rt_value_store_bool");
        module.body().close("end");
        return Ok(true);
    }
    if !key_kinds.iter().all(|kind| *kind == AssocKeyKind::Str) {
        return Ok(false);
    }
    if !strict {
        let Some(key) = static_loose_assoc_string_key_search_result(var, needle, module) else {
            let has_scalar_value_metadata = module
                .array_runtime_value_cell_kind(var)
                .is_some_and(|kind| kind != ValueCellKind::Array)
                || module
                    .array_value_cell_kinds(var)
                    .is_some_and(|kinds| kinds.iter().all(|kind| *kind != ValueCellKind::Array));
            if has_scalar_value_metadata {
                let result = module.next_label("mixed_array_search_string_key_loose_result");
                let found = module.next_label("mixed_array_search_string_key_loose_found");
                module.declare_i64_local(result.trim_start_matches('$').to_string());
                module.declare_i32_local(found.trim_start_matches('$').to_string());
                emit_alloc_mixed_cell(target, module);
                emit_assoc_array_search_loose_scalar_packed_key(call, var, needle, &result, None, Some(&found), module)?;
                module.body().line(&format!("local.get {}", found));
                module.body().open("if");
                module.body().line(&format!("local.get ${}", target));
                module.body().line(&format!("local.get {}", result));
                module.body().line("i32.wrap_i64");
                module.body().line(&format!("local.get {}", result));
                module.body().line("i64.const 32");
                module.body().line("i64.shr_u");
                module.body().line("i32.wrap_i64");
                module.body().line("call $__rt_value_store_string");
                module.body().close("else");
                module.body().line(&format!("local.get ${}", target));
                module.body().line("i32.const 0");
                module.body().line("call $__rt_value_store_bool");
                module.body().close("end");
                return Ok(true);
            }
            return Err(CompileError::new(
                call.span,
                "wasm32-web loose array_search() assignment over string-key associative arrays requires exact scalar value metadata",
            ));
        };
        emit_alloc_mixed_cell(target, module);
        if let Some(key) = key {
            let (key_ptr, key_len) = module.intern_string(&key);
            module.body().line(&format!("local.get ${}", target));
            module.body().line(&format!("i32.const {}", key_ptr));
            module.body().line(&format!("i32.const {}", key_len));
            module.body().line("call $__rt_value_store_string");
            module.set_mixed_value_cell_kind(target, Some(ValueCellKind::Str));
        } else {
            module.body().line(&format!("local.get ${}", target));
            module.body().line("i32.const 0");
            module.body().line("call $__rt_value_store_bool");
            module.set_mixed_value_cell_kind(target, Some(ValueCellKind::Bool));
        }
        return Ok(true);
    }
    let result = module.next_label("mixed_array_search_string_key_result");
    module.declare_i64_local(result.trim_start_matches('$').to_string());
    emit_alloc_mixed_cell(target, module);
    emit_assoc_array_search_packed_key(call, var, needle, module)?;
    module.body().line(&format!("local.set {}", result));
    module.body().line(&format!("local.get {}", result));
    module.body().line("i64.const 0");
    module.body().line("i64.ge_s");
    module.body().open("if");
    module.body().line(&format!("local.get ${}", target));
    module.body().line(&format!("local.get {}", result));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.get {}", result));
    module.body().line("i64.const 32");
    module.body().line("i64.shr_u");
    module.body().line("i32.wrap_i64");
    module.body().line("call $__rt_value_store_string");
    module.body().close("else");
    module.body().line(&format!("local.get ${}", target));
    module.body().line("i32.const 0");
    module.body().line("call $__rt_value_store_bool");
    module.body().close("end");
    Ok(true)
}

fn emit_mixed_assoc_array_search_mixed_key_assign(
    target: &str,
    call: &Expr,
    var: &str,
    needle: &Expr,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let result = module.next_label("mixed_array_search_runtime_mixed_key_result");
    let key_kind = module.next_label("mixed_array_search_runtime_mixed_key_kind");
    let found = module.next_label("mixed_array_search_runtime_mixed_key_found");
    module.declare_i64_local(result.trim_start_matches('$').to_string());
    for local in [&key_kind, &found] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_alloc_mixed_cell(target, module);
    if !emit_assoc_array_search_mixed_key_with_found(call, var, needle, &result, &key_kind, &found, module)? {
        return Ok(false);
    }
    module.body().line(&format!("local.get {}", found));
    module.body().open("if");
    module.body().line(&format!("local.get {}", key_kind));
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get ${}", target));
    module.body().line(&format!("local.get {}", result));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.get {}", result));
    module.body().line("i64.const 32");
    module.body().line("i64.shr_u");
    module.body().line("i32.wrap_i64");
    module.body().line("call $__rt_value_store_string");
    module.body().close("else");
    module.body().line(&format!("local.get ${}", target));
    module.body().line(&format!("local.get {}", result));
    module.body().line("call $__rt_value_store_int");
    module.body().close("end");
    module.body().close("else");
    module.body().line(&format!("local.get ${}", target));
    module.body().line("i32.const 0");
    module.body().line("call $__rt_value_store_bool");
    module.body().close("end");
    Ok(true)
}

pub(super) fn emit_mixed_value_array_pop_shift_assign(
    target: &str,
    expr: &Expr,
    function_name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let name = single_array_variable_arg(expr, args, function_name, module)?;
    if module.array_layout(&name) == ArrayLayout::Assoc {
        let len = module.array_length(&name).ok_or_else(|| {
            CompileError::new(
                args[0].span,
                &format!("wasm32-web {function_name}() requires a known associative array length"),
            )
        })?;
        emit_alloc_mixed_cell(target, module);
        if len == 0 {
            emit_store_null_value_cell(&format!("${}", target), module);
            module.set_array_length(&name, 0);
            return Ok(());
        }
        if function_name.eq_ignore_ascii_case("array_pop") {
            emit_mixed_assoc_array_pop_assign(target, &name, len, module);
        } else {
            emit_mixed_assoc_array_shift_assign(target, &name, len, module);
        }
        return Ok(());
    }
    if module.array_layout(&name) != ArrayLayout::Value {
        let len = module.array_length(&name).ok_or_else(|| {
            CompileError::new(
                args[0].span,
                &format!("wasm32-web {function_name}() requires a known indexed array length"),
            )
        })?;
        if len == 0 {
            emit_alloc_mixed_cell(target, module);
            emit_store_null_value_cell(&format!("${}", target), module);
            module.set_array_length(&name, 0);
            return Ok(());
        }
        return Err(CompileError::new(
            args[0].span,
            &format!("wasm32-web {function_name}() mixed assignment requires a value-cell array"),
        ));
    }
    let len = module.array_length(&name).ok_or_else(|| {
        CompileError::new(
            args[0].span,
            &format!("wasm32-web {function_name}() requires a known indexed array length"),
        )
    })?;
    emit_alloc_mixed_cell(target, module);
    if len == 0 {
        emit_store_null_value_cell(&format!("${}", target), module);
        return Ok(());
    }
    if function_name.eq_ignore_ascii_case("array_pop") {
        emit_mixed_value_array_pop_assign(target, &name, len, module);
    } else {
        emit_mixed_value_array_shift_assign(target, &name, len, module);
    }
    module.set_array_length(&name, len - 1);
    module.set_array_layout(&name, ArrayLayout::Value);
    let value_kinds = if function_name.eq_ignore_ascii_case("array_pop") {
        module.array_value_cell_kinds(&name).map(|kinds| kinds[..len - 1].to_vec())
    } else {
        module.array_value_cell_kinds(&name).map(|kinds| kinds[1..].to_vec())
    };
    let value_constants =
        array_value_constants_after_pop_shift(&name, len, function_name.eq_ignore_ascii_case("array_pop"), module);
    module.set_array_value_cell_kinds(&name, value_kinds);
    module.set_array_value_constants(&name, value_constants);
    Ok(())
}

pub(in crate::codegen::wasm::expr) fn emit_unknown_mixed_array_pop_assign(
    target: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if args.len() != 1 {
        return Ok(false);
    }
    let ExprKind::Variable(source) = &args[0].kind else {
        return Ok(false);
    };
    if module.local_kind(source) != Some(LocalKind::Mixed) || module.mixed_value_cell_kind(source).is_some() {
        return Ok(false);
    }
    let ptr = module.next_label("unknown_mixed_array_pop_ptr");
    let len = module.next_label("unknown_mixed_array_pop_len");
    let heap_kind = module.next_label("unknown_mixed_array_pop_heap_kind");
    let cell = module.next_label("unknown_mixed_array_pop_cell");
    let entry = module.next_label("unknown_mixed_array_pop_entry");
    for local in [&ptr, &len, &heap_kind, &cell, &entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_alloc_mixed_cell(target, module);
    emit_load_unknown_mixed_array_header(source, &ptr, &len, &heap_kind, module);
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_store_null_value_cell(&format!("${}", target), module);
    module.body().close("else");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line("call $__rt_ensure_unique");
    module.body().line(&format!("local.set {}", ptr));
    module.body().line(&format!("local.get ${}", source));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line("i32.store");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_INDEXED_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", cell));
    module.body().close("else");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    module.body().close("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    emit_copy_value_cell_from_addr_to_addr(&format!("${}", target), &cell, module);
    emit_release_value_cell(&cell, module);
    module.body().line(&format!("local.get ${}", source));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.store");
    module.body().close("end");
    let _ = call;
    module.set_mixed_value_cell_kind(target, None);
    Ok(true)
}

pub(in crate::codegen::wasm::expr) fn emit_unknown_mixed_array_shift_assign(
    target: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if args.len() != 1 {
        return Ok(false);
    }
    let ExprKind::Variable(source) = &args[0].kind else {
        return Ok(false);
    };
    if module.local_kind(source) != Some(LocalKind::Mixed) || module.mixed_value_cell_kind(source).is_some() {
        return Ok(false);
    }
    let ptr = module.next_label("unknown_mixed_array_shift_ptr");
    let new_ptr = module.next_label("unknown_mixed_array_shift_new_ptr");
    let len = module.next_label("unknown_mixed_array_shift_len");
    let heap_kind = module.next_label("unknown_mixed_array_shift_heap_kind");
    let cell = module.next_label("unknown_mixed_array_shift_cell");
    let source_entry = module.next_label("unknown_mixed_array_shift_source_entry");
    let target_entry = module.next_label("unknown_mixed_array_shift_target_entry");
    let index = module.next_label("unknown_mixed_array_shift_index");
    let next_int_key = module.next_label("unknown_mixed_array_shift_next_int_key");
    let copy_done = module.next_label("unknown_mixed_array_shift_copy_done");
    let copy_loop = module.next_label("unknown_mixed_array_shift_copy_loop");
    for local in [
        &ptr,
        &new_ptr,
        &len,
        &heap_kind,
        &cell,
        &source_entry,
        &target_entry,
        &index,
        &next_int_key,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_alloc_mixed_cell(target, module);
    emit_load_unknown_mixed_array_header(source, &ptr, &len, &heap_kind, module);
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_store_null_value_cell(&format!("${}", target), module);
    module.body().close("else");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line("call $__rt_ensure_unique");
    module.body().line(&format!("local.set {}", ptr));
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_INDEXED_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_unknown_mixed_indexed_array_shift_assign(target, source, &ptr, &new_ptr, &len, &cell, &index, &copy_done, &copy_loop, module);
    module.body().close("else");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_unknown_mixed_assoc_array_shift_assign(
        target,
        source,
        &ptr,
        &new_ptr,
        &len,
        &cell,
        &source_entry,
        &target_entry,
        &index,
        &next_int_key,
        &copy_done,
        &copy_loop,
        module,
    );
    module.body().close("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    let _ = call;
    module.set_mixed_value_cell_kind(target, None);
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
fn emit_unknown_mixed_indexed_array_shift_assign(
    target: &str,
    source: &str,
    ptr: &str,
    new_ptr: &str,
    len: &str,
    cell: &str,
    index: &str,
    copy_done: &str,
    copy_loop: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", ptr));
    module.body().line("i32.const 0");
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_copy_value_cell_from_addr_to_addr(&format!("${}", target), cell, module);
    emit_release_value_cell(cell, module);
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", new_ptr));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", copy_done));
    module.body().open(&format!("loop {}", copy_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", copy_done));
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_cell");
    module.body().line("call $__rt_value_copy");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", copy_loop));
    module.body().close("end");
    module.body().close("end");
    emit_update_unknown_mixed_array_payload(source, new_ptr, len, module);
}

#[allow(clippy::too_many_arguments)]
fn emit_unknown_mixed_assoc_array_shift_assign(
    target: &str,
    source: &str,
    ptr: &str,
    new_ptr: &str,
    len: &str,
    cell: &str,
    source_entry: &str,
    target_entry: &str,
    index: &str,
    next_int_key: &str,
    copy_done: &str,
    copy_loop: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", ptr));
    module.body().line("i32.const 0");
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", source_entry));
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_copy_value_cell_from_addr_to_addr(&format!("${}", target), cell, module);
    emit_release_value_cell(cell, module);
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set {}", new_ptr));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", next_int_key));
    module.body().open(&format!("block {}", copy_done));
    module.body().open(&format!("loop {}", copy_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", copy_done));
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", source_entry));
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", next_int_key));
    module.body().line("i64.extend_i32_s");
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", next_int_key));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", next_int_key));
    module.body().close("else");
    copy_assoc_entry_key(target_entry, source_entry, module);
    module.body().close("end");
    copy_assoc_entry_value(target_entry, source_entry, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", copy_loop));
    module.body().close("end");
    module.body().close("end");
    emit_update_unknown_mixed_array_payload(source, new_ptr, len, module);
}

fn emit_update_unknown_mixed_array_payload(source: &str, new_ptr: &str, len: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get ${}", source));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line("i32.store");
    module.body().line(&format!("local.get ${}", source));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.store");
}

fn emit_mixed_value_array_pop_assign(
    target: &str,
    name: &str,
    len: usize,
    module: &mut WasmModule,
) {
    let cell = module.next_label("mixed_value_array_pop_cell");
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", (len - 1) * WASM_VALUE_CELL_SIZE));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
    emit_copy_value_cell_from_addr_to_addr(&format!("${}", target), &cell, module);
    emit_release_value_cell(&cell, module);
    module.body().line(&format!("i32.const {}", len - 1));
    module.body().line(&format!("local.set ${}_len", name));
}

fn emit_mixed_value_array_shift_assign(
    target: &str,
    name: &str,
    len: usize,
    module: &mut WasmModule,
) {
    let cell = module.next_label("mixed_value_array_shift_cell");
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    let source_ptr = preserve_array_ptr(name, "mixed_value_array_shift", module);
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.set {}", cell));
    emit_copy_value_cell_from_addr_to_addr(&format!("${}", target), &cell, module);
    emit_release_value_cell(&cell, module);
    module.body().line(&format!("i32.const {}", len - 1));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len - 1));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 1..len {
        emit_copy_static_value_cell(name, index - 1, &source_ptr, index, module);
    }
}

fn emit_mixed_assoc_array_pop_assign(
    target: &str,
    name: &str,
    len: usize,
    module: &mut WasmModule,
) {
    let cell = module.next_label("mixed_assoc_array_pop_cell");
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", (len - 1) * WASM_ASSOC_ENTRY_SIZE + 16));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
    emit_copy_value_cell_from_addr_to_addr(&format!("${}", target), &cell, module);
    emit_release_value_cell(&cell, module);
    module.body().line(&format!("i32.const {}", len - 1));
    module.body().line(&format!("local.set ${}_len", name));
    module.set_array_length(name, len - 1);
    let key_kinds = module.array_key_kinds(name).map(|kinds| kinds[..len - 1].to_vec());
    let key_values = module.array_key_values(name).map(|values| values[..len - 1].to_vec());
    let value_kinds = module
        .array_value_cell_kinds(name)
        .map(|kinds| kinds[..len - 1].to_vec());
    let value_constants = array_value_constants_after_pop_shift(name, len, true, module);
    module.set_array_key_kinds(name, key_kinds);
    module.set_array_key_values(name, key_values);
    module.set_array_value_cell_kinds(name, value_kinds);
    module.set_array_value_constants(name, value_constants);
}

fn emit_mixed_assoc_array_shift_assign(
    target: &str,
    name: &str,
    len: usize,
    module: &mut WasmModule,
) {
    let cell = module.next_label("mixed_assoc_array_shift_cell");
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    let source_ptr = preserve_array_ptr(name, "mixed_assoc_array_shift", module);
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line("i32.const 16");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
    emit_copy_value_cell_from_addr_to_addr(&format!("${}", target), &cell, module);
    emit_release_value_cell(&cell, module);
    module.body().line(&format!("i32.const {}", len - 1));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len - 1));
    module.body().line(&format!("local.set ${}_len", name));
    let key_kinds = module.array_key_kinds(name).map(|kinds| kinds.to_vec());
    let key_values = module.array_key_values(name).map(|values| values.to_vec());
    let mut next_int_key = 0i64;
    for index in 1..len {
        if key_kinds
            .as_ref()
            .and_then(|kinds| kinds.get(index))
            .is_some_and(|kind| *kind == AssocKeyKind::Int)
        {
            module.body().line(&format!("local.get ${}_ptr", name));
            module
                .body()
                .line(&format!("i32.const {}", (index - 1) * WASM_ASSOC_ENTRY_SIZE));
            module.body().line("i32.add");
            module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
            module.body().line("i32.store");
            module.body().line(&format!("local.get ${}_ptr", name));
            module
                .body()
                .line(&format!("i32.const {}", (index - 1) * WASM_ASSOC_ENTRY_SIZE + 8));
            module.body().line("i32.add");
            module.body().line(&format!("i64.const {}", next_int_key));
            module.body().line("i64.store");
            next_int_key += 1;
        } else {
            for offset in [0, 8] {
                module.body().line(&format!("local.get ${}_ptr", name));
                module.body().line(&format!(
                    "i32.const {}",
                    (index - 1) * WASM_ASSOC_ENTRY_SIZE + offset
                ));
                module.body().line("i32.add");
                module.body().line(&format!("local.get {}", source_ptr));
                module
                    .body()
                    .line(&format!("i32.const {}", index * WASM_ASSOC_ENTRY_SIZE + offset));
                module.body().line("i32.add");
                module.body().line("i64.load");
                module.body().line("i64.store");
            }
        }
        module.body().line(&format!("local.get ${}_ptr", name));
        module
            .body()
            .line(&format!("i32.const {}", (index - 1) * WASM_ASSOC_ENTRY_SIZE + 16));
        module.body().line("i32.add");
        module.body().line(&format!("local.get {}", source_ptr));
        module
            .body()
            .line(&format!("i32.const {}", index * WASM_ASSOC_ENTRY_SIZE + 16));
        module.body().line("i32.add");
        module.body().line("call $__rt_value_copy");
    }
    module.set_array_length(name, len - 1);
    let key_values = key_kinds.as_ref().zip(key_values.as_ref()).map(|(kinds, values)| {
        reindexed_assoc_key_values(&kinds[1..], &values[1..])
    });
    let key_kinds = key_kinds.map(|kinds| kinds[1..].to_vec());
    let value_kinds = module.array_value_cell_kinds(name).map(|kinds| kinds[1..].to_vec());
    let value_constants = array_value_constants_after_pop_shift(name, len, false, module);
    module.set_array_key_kinds(name, key_kinds);
    module.set_array_key_values(name, key_values);
    module.set_array_value_cell_kinds(name, value_kinds);
    module.set_array_value_constants(name, value_constants);
}
