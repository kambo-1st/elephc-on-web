//! Purpose:
//! Lowers PHP array_search() behavior for wasm32-web arrays and associative arrays.
//! Keeps search loops, key packing, and strict/loose value-cell comparisons together.
//!
//! Called from:
//! - `crate::codegen::wasm::expr`
//! - `crate::codegen::wasm::expr::string_search`
//!
//! Key details:
//! - Emits WAT directly for indexed, associative, mixed-key, and value-cell array searches.
//! - Unsupported metadata combinations return CompileError instead of guessing PHP semantics.

mod false_comparison;
mod layouts;

use super::*;
use super::array_search_assoc::{
    assoc_key_kinds_are_mixed, emit_assoc_array_search_mixed_key_index,
    emit_assoc_array_search_runtime_bool, emit_assoc_array_search_runtime_float,
    emit_assoc_array_search_runtime_int, emit_assoc_array_search_runtime_string,
    emit_assoc_array_search_string, emit_assoc_array_search_tag_payload,
    mixed_key_search_needle_is_supported, reject_negative_int_key_array_search,
};
use super::array_search_static::{
    static_loose_array_constant_search_index, static_loose_assoc_int_key_search_result,
    static_loose_value_array_search_index,
};
use super::array_search_value::{
    emit_value_array_search_index, emit_value_array_search_loose_index,
};
pub(super) use false_comparison::{
    array_search_false_comparison, emit_array_search_false_comparison_bool,
};
use layouts::{emit_assoc_array_search_index, emit_compact_int_array_search_loose_index};

pub(super) fn emit_array_search_index(
    call: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let ExprKind::FunctionCall { name, args } = &call.kind else {
        return Err(array_unsupported(call));
    };
    if !name.eq_ignore_ascii_case("array_search") {
        return Err(array_unsupported(call));
    }
    emit_array_search_index_from_args(call, args, module)
}

pub(super) fn emit_array_search_index_from_args(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
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
    if matches!(haystack.kind, ExprKind::ArrayAccess { .. })
        && nested_array_metadata_for_access_expr(haystack, module).is_some()
    {
        let temp = materialize_nested_array_search_source(haystack, module)?;
        return emit_array_search_index_from_args(
            call,
            &[
                needle.clone(),
                Expr::new(ExprKind::Variable(temp), haystack.span),
                Expr::new(ExprKind::BoolLiteral(strict), haystack.span),
            ],
            module,
        );
    }
    if strict {
        if let ExprKind::ArrayLiteralAssoc(items) = &haystack.kind {
            let temp = module
                .next_label("assoc_array_search_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_assoc_array_items_assign(&temp, items, module)?;
            if module
                .array_key_kinds(&temp)
                .is_some_and(|kinds| kinds.iter().all(|kind| *kind == AssocKeyKind::Str))
            {
                return emit_assoc_array_search_packed_key(call, &temp, needle, module);
            }
            if module.array_key_kinds(&temp).is_some_and(assoc_key_kinds_are_mixed)
                && mixed_key_search_needle_is_supported(needle, module)
            {
                return emit_assoc_array_search_mixed_key_index(call, &temp, needle, module);
            }
            return emit_assoc_array_search_index(call, &temp, needle, module);
        }
        if let ExprKind::ArrayLiteral(items) = &haystack.kind {
            let temp = module
                .next_label("value_array_search_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_value_array_items_assign(&temp, items, module)?;
            return emit_value_array_search_index(call, &temp, needle, module);
        }
        if let ExprKind::FunctionCall { name: function_name, .. } = &haystack.kind {
            if module.has_function(function_name)
                && module.function_return_kind(function_name) == Some(ValueKind::Array)
                && matches!(
                    module.function_array_return_layout(function_name),
                    ArrayLayout::Assoc | ArrayLayout::Value
                )
            {
                let temp = module
                    .next_label("array_search_return_source")
                    .trim_start_matches('$')
                    .to_string();
                module.declare_array_local(temp.clone());
                emit_array_assign(&temp, haystack, module)?;
                if module.array_layout(&temp) == ArrayLayout::Assoc {
                    if module
                        .array_key_kinds(&temp)
                        .is_some_and(|kinds| kinds.iter().all(|kind| *kind == AssocKeyKind::Str))
                    {
                        return emit_assoc_array_search_packed_key(call, &temp, needle, module);
                    }
                    if module.array_key_kinds(&temp).is_some_and(assoc_key_kinds_are_mixed)
                        && mixed_key_search_needle_is_supported(needle, module)
                    {
                        return emit_assoc_array_search_mixed_key_index(call, &temp, needle, module);
                    }
                    return emit_assoc_array_search_index(call, &temp, needle, module);
                }
                return emit_value_array_search_index(call, &temp, needle, module);
            }
        }
        if expression_has_array_type(haystack, module) && !expression_is_arrayy(haystack, module) {
            let temp = module
                .next_label("array_search_direct_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, haystack, module)?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                if let Some(kinds) = module.array_key_kinds(&temp) {
                    if kinds.iter().all(|kind| *kind == AssocKeyKind::Str) {
                        return emit_assoc_array_search_packed_key(call, &temp, needle, module);
                    }
                    if assoc_key_kinds_are_mixed(kinds) && mixed_key_search_needle_is_supported(needle, module) {
                        return emit_assoc_array_search_mixed_key_index(call, &temp, needle, module);
                    }
                    if !kinds.iter().all(|kind| *kind == AssocKeyKind::Int) {
                        return Err(CompileError::new(
                            haystack.span,
                            "wasm32-web direct associative array_search() currently requires known int/string keys in value position",
                        ));
                    }
                } else if module.array_runtime_key_kind(&temp) == Some(AssocKeyKind::Str) {
                    return emit_assoc_array_search_packed_key(call, &temp, needle, module);
                } else if module.array_has_php_normalized_runtime_keys(&temp)
                    && mixed_key_search_needle_is_supported(needle, module)
                {
                    return emit_assoc_array_search_mixed_key_index(call, &temp, needle, module);
                } else {
                    return Err(CompileError::new(
                        haystack.span,
                        "wasm32-web direct associative array_search() requires known key metadata in value position",
                    ));
                }
                return emit_assoc_array_search_index(call, &temp, needle, module);
            }
            if module.array_layout(&temp) == ArrayLayout::Value {
                return emit_value_array_search_index(call, &temp, needle, module);
            }
            return emit_array_search_index_from_args(
                call,
                &[
                    needle.clone(),
                    Expr {
                        kind: ExprKind::Variable(temp),
                        span: haystack.span,
                    },
                    Expr {
                        kind: ExprKind::BoolLiteral(true),
                        span: haystack.span,
                    },
                ],
                module,
            );
        }
    }
    if !strict {
        if expression_has_array_type(haystack, module) && !expression_is_arrayy(haystack, module) {
            let temp = module
                .next_label("array_search_loose_direct_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, haystack, module)?;
            return match module.array_layout(&temp) {
                ArrayLayout::CompactInt => {
                    emit_compact_int_array_search_loose_index(call, &temp, needle, module)
                }
                ArrayLayout::Value => emit_value_array_search_loose_index(call, &temp, needle, module),
                ArrayLayout::Assoc => {
                    if let Some(key) = static_loose_assoc_int_key_search_result(&temp, needle, module) {
                        match key {
                            Some(key) => module.body().line(&format!("i64.const {}", key)),
                            None => module.body().line("i64.const -1"),
                        }
                        Ok(())
                    } else {
                        Err(CompileError::new(
                            call.span,
                            "wasm32-web loose array_search() over direct associative builders requires static scalar value metadata",
                        ))
                    }
                }
            };
        }
        if let ExprKind::ArrayLiteralAssoc(items) = &haystack.kind {
            let temp = module
                .next_label("assoc_array_search_loose_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_assoc_array_items_assign(&temp, items, module)?;
            if let Some(key) = static_loose_assoc_int_key_search_result(&temp, needle, module) {
                match key {
                    Some(key) => module.body().line(&format!("i64.const {}", key)),
                    None => module.body().line("i64.const -1"),
                }
                return Ok(());
            }
            let has_string_keys = module
                .array_key_kinds(&temp)
                .is_some_and(|kinds| kinds.iter().all(|kind| *kind == AssocKeyKind::Str));
            let has_scalar_value_metadata = module
                .array_runtime_value_cell_kind(&temp)
                .is_some_and(|kind| kind != ValueCellKind::Array)
                || module
                    .array_value_cell_kinds(&temp)
                    .is_some_and(|kinds| kinds.iter().all(|kind| *kind != ValueCellKind::Array));
            if has_string_keys && has_scalar_value_metadata {
                let result = module.next_label("array_search_direct_string_key_loose_result");
                module.declare_i64_local(result.trim_start_matches('$').to_string());
                emit_assoc_array_search_loose_scalar_packed_key(call, &temp, needle, &result, None, None, module)?;
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
                return Ok(());
            }
            return Err(CompileError::new(
                call.span,
                "wasm32-web loose array_search() over associative arrays requires non-negative integer keys and static scalar value metadata",
            ));
        }
        if let ExprKind::ArrayLiteral(items) = &haystack.kind {
            if let Some(index) = static_loose_value_array_search_index(items, needle, module) {
                match index {
                    Some(index) => module.body().line(&format!("i64.const {}", index)),
                    None => module.body().line("i64.const -1"),
                }
                return Ok(());
            }
            let temp = module
                .next_label("value_array_search_loose_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_value_array_items_assign(&temp, items, module)?;
            return emit_value_array_search_loose_index(call, &temp, needle, module);
        }
    }
    if !expression_is_arrayy(haystack, module) {
        return Err(CompileError::new(
            haystack.span,
            "wasm32-web array_search() currently supports indexed array values only",
        ));
    }
    if let ExprKind::Variable(var) = &haystack.kind {
        if module.local_kind(var) == Some(LocalKind::Array)
            && module.array_layout(var) == ArrayLayout::Assoc
        {
            if !strict {
                if let Some(key) = static_loose_assoc_int_key_search_result(var, needle, module) {
                    match key {
                        Some(key) => module.body().line(&format!("i64.const {}", key)),
                        None => module.body().line("i64.const -1"),
                    }
                    return Ok(());
                }
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web loose array_search() over associative arrays requires non-negative integer keys and static scalar value metadata",
                ));
            }
            if let Some(key_kinds) = module.array_key_kinds(var) {
                if key_kinds.iter().all(|kind| *kind == AssocKeyKind::Str) {
                    return emit_assoc_array_search_packed_key(call, var, needle, module);
                }
                if assoc_key_kinds_are_mixed(key_kinds)
                    && mixed_key_search_needle_is_supported(needle, module)
                {
                    return emit_assoc_array_search_mixed_key_index(call, var, needle, module);
                }
            }
            if module.array_has_php_normalized_runtime_keys(var)
                && mixed_key_search_needle_is_supported(needle, module)
            {
                return emit_assoc_array_search_mixed_key_index(call, var, needle, module);
            }
            return emit_assoc_array_search_index(call, var, needle, module);
        }
        if module.local_kind(var) == Some(LocalKind::Array)
            && module.array_layout(var) == ArrayLayout::Value
        {
            if !strict {
                if let Some(index) = static_loose_array_constant_search_index(var, needle, module) {
                    match index {
                        Some(index) => module.body().line(&format!("i64.const {}", index)),
                        None => module.body().line("i64.const -1"),
                    }
                    return Ok(());
                }
                return emit_value_array_search_loose_index(call, var, needle, module);
            }
            return emit_value_array_search_index(call, var, needle, module);
        }
        if module.local_kind(var) == Some(LocalKind::Array)
            && module.array_layout(var) == ArrayLayout::CompactInt
            && !strict
        {
            return emit_compact_int_array_search_loose_index(call, var, needle, module);
        }
    }
    let needle_local = module.next_label("array_search_needle");
    let ptr = module.next_label("array_search_ptr");
    let len = module.next_label("array_search_len");
    let index = module.next_label("array_search_index");
    let result = module.next_label("array_search_result");
    let done_label = module.next_label("array_search_done");
    let loop_label = module.next_label("array_search_loop");
    module.declare_i64_local(needle_local.trim_start_matches('$').to_string());
    module.declare_i64_local(result.trim_start_matches('$').to_string());
    for local in [&ptr, &len, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    require_int(needle, module)?;
    module.body().line(&format!("local.set {}", needle_local));
    emit_array_value_to_stack(haystack, module)?;
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.set {}", ptr));
    module.body().line("i64.const -1");
    module.body().line(&format!("local.set {}", result));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.get {}", needle_local));
    module.body().line("i64.eq");
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

pub(super) fn materialize_nested_array_search_source(
    source: &Expr,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let Some(metadata) = nested_array_metadata_for_access_expr(source, module) else {
        return Err(CompileError::new(
            source.span,
            "wasm32-web array_search() requires nested array metadata",
        ));
    };
    let temp = module
        .next_label("array_search_nested_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    match emit_expr(source, module)? {
        ValueKind::Array => {
            module.body().line(&format!("local.set ${}_len", temp));
            module.body().line(&format!("local.set ${}_ptr", temp));
        }
        _ => {
            return Err(CompileError::new(
                source.span,
                "wasm32-web array_search() expected a nested array value",
            ));
        }
    }
    let key_values = metadata.key_values.clone();
    module.set_array_layout(&temp, metadata.layout);
    module.set_array_length(&temp, metadata.len);
    module.set_array_value_cell_kinds(&temp, metadata.value_kinds);
    module.set_array_key_values(&temp, key_values.clone());
    module.set_array_key_kinds(
        &temp,
        key_values.map(|keys| {
            keys.into_iter()
                .map(|key| match key {
                    AssocKeyValue::Int(_) => AssocKeyKind::Int,
                    AssocKeyValue::Str(_) => AssocKeyKind::Str,
                })
                .collect()
        }),
    );
    module.set_array_php_normalized_runtime_keys(
        &temp,
        metadata.layout == ArrayLayout::Assoc && module.array_key_kinds(&temp).is_none(),
    );
    module.set_array_nested_value_metadata(&temp, metadata.nested_values);
    Ok(temp)
}
