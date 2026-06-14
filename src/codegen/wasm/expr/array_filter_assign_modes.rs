//! Purpose:
//! Lowers ARRAY_FILTER_USE_KEY and ARRAY_FILTER_USE_BOTH assignment paths for wasm32-web.
//! Keeps callback mode validation and source-shape matching separate from default filtering.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_filter_assign`
//!
//! Key details:
//! - Preserves PHP key/value callback mode rules while retaining array layout metadata.

use super::*;

pub(super) fn emit_array_filter_mode_assign(
    name: &str,
    _call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(mode) = static_or_const_or_i64_local_value(&args[2], module) else {
        return Err(CompileError::new(
            args[2].span,
            "wasm32-web array_filter() mode must be a static ARRAY_FILTER_* constant",
        ));
    };
    if mode != 1 && mode != 2 {
        return Err(CompileError::new(
            args[2].span,
            "wasm32-web array_filter() currently supports ARRAY_FILTER_USE_KEY and ARRAY_FILTER_USE_BOTH modes only",
        ));
    }
    let Some(callback) = evaluated_static_callback_function_name(&args[1], module)? else {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web array_filter() currently requires a static string, direct first-class function callback, or simple callable variable alias",
        ));
    };
    if !module.has_function(&callback)
        && !(mode == 2 && array_filter_builtin_callback_is_supported(&callback))
    {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web ARRAY_FILTER_USE_KEY currently requires a user-defined key callback or supported builtin key callback",
        ));
    }
    let (both_callback, key_callback_shape) = if mode == 1 {
        (
            Some(validate_array_filter_use_both_callback(&callback, args[1].span, module)?),
            None,
        )
    } else {
        let shape = array_filter_callback_shape(&callback, args[1].span, module)?;
        if !matches!(
            shape,
            ArrayFilterCallbackShape::Int
                | ArrayFilterCallbackShape::Str
                | ArrayFilterCallbackShape::Numeric
                | ArrayFilterCallbackShape::Mixed
        ) {
            return Err(CompileError::new(
                args[1].span,
                "wasm32-web ARRAY_FILTER_USE_KEY currently requires an integer, string, or mixed key callback",
            ));
        }
        (None, Some(shape))
    };
    match &args[0].kind {
        ExprKind::ArrayLiteral(items) if !array_literal_needs_value_cells(items) => {
            if key_callback_shape.is_some_and(|shape| !array_filter_int_key_callback_shape_is_supported(shape)) {
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web ARRAY_FILTER_USE_KEY callback key type must match integer array keys",
                ));
            }
            if both_callback.is_some_and(|callback| {
                callback.value_kind.is_some_and(|kind| kind != ValueCellKind::Int)
                    || !array_filter_int_key_callback_shape_is_supported(callback.key_shape)
            }) {
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web ARRAY_FILTER_USE_BOTH callback must match integer values and integer keys",
                ));
            }
            let temp = module
                .next_label("array_filter_key_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_static_array_items_assign(&temp, items, module)?;
            if mode == 1 {
                emit_array_filter_compact_int_both_local_assign(name, &temp, args[0].span, &callback, module)
            } else {
                emit_array_filter_compact_int_key_local_assign(name, &temp, args[0].span, &callback, module)
            }
        }
        ExprKind::ArrayLiteral(items)
            if mode == 2 && array_filter_key_mode_items_value_kind(items, module).is_some() =>
        {
            let temp = module
                .next_label("array_filter_key_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_value_array_items_assign(&temp, items, module)?;
            let kind = array_filter_key_mode_source_value_kind(&temp, module)
                .expect("array_filter key-mode literal source kind was prevalidated");
            emit_array_filter_value_key_local_assign(name, &temp, args[0].span, &callback, kind, module)
        }
        ExprKind::ArrayLiteral(items)
            if mode == 1
                && array_filter_both_mode_items_source_kind(items, both_callback, module).is_some() =>
        {
            let temp = module
                .next_label("array_filter_key_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_value_array_items_assign(&temp, items, module)?;
            let kind = array_filter_both_mode_items_source_kind(items, both_callback, module)
                .expect("array_filter both-mode literal source kind was prevalidated");
            emit_array_filter_value_both_local_assign(name, &temp, args[0].span, &callback, kind, module)
        }
        ExprKind::MethodCall { object, method, .. }
        | ExprKind::NullsafeMethodCall { object, method, .. }
            if method_call_array_return_metadata(object, method, module).is_some() =>
        {
            let temp = module
                .next_label("array_filter_key_method_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, &args[0], module)?;
            emit_array_filter_mode_staged_source(
                name,
                &temp,
                args[0].span,
                mode,
                &callback,
                both_callback,
                key_callback_shape,
                module,
            )
        }
        ExprKind::StaticMethodCall { receiver, method, .. }
            if static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            let temp = module
                .next_label("array_filter_key_static_method_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, &args[0], module)?;
            emit_array_filter_mode_staged_source(
                name,
                &temp,
                args[0].span,
                mode,
                &callback,
                both_callback,
                key_callback_shape,
                module,
            )
        }
        ExprKind::DynamicStaticMethodCall { receiver, method, .. }
            if dynamic_static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            let temp = module
                .next_label("array_filter_key_dynamic_static_method_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, &args[0], module)?;
            emit_array_filter_mode_staged_source(
                name,
                &temp,
                args[0].span,
                mode,
                &callback,
                both_callback,
                key_callback_shape,
                module,
            )
        }
        ExprKind::FunctionCall { .. } | ExprKind::ExprCall { .. }
            if expression_has_array_type(&args[0], module) =>
        {
            let temp = module
                .next_label("array_filter_key_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, &args[0], module)?;
            emit_array_filter_mode_staged_source(
                name,
                &temp,
                args[0].span,
                mode,
                &callback,
                both_callback,
                key_callback_shape,
                module,
            )
        }
        ExprKind::ArrayLiteralAssoc(items)
            if mode == 2
                && array_filter_key_mode_assoc_literal_matches_callback(
                    items,
                    key_callback_shape,
                    module,
                ) =>
        {
            let temp = module
                .next_label("array_filter_key_assoc_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_assoc_array_items_assign(&temp, items, module)?;
            let shape = key_callback_shape.expect("array_filter key-mode callback shape was validated");
            emit_array_filter_assoc_key_local_assign(name, &temp, args[0].span, &callback, shape, module)
        }
        ExprKind::ArrayLiteralAssoc(items)
            if mode == 1
                && array_filter_both_mode_assoc_literal_matches_callback(
                    items,
                    both_callback,
                    module,
                ) =>
        {
            let temp = module
                .next_label("array_filter_both_assoc_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_assoc_array_items_assign(&temp, items, module)?;
            let callback_shape = both_callback.expect("array_filter both-mode callback was validated");
            emit_array_filter_assoc_both_local_assign(
                name,
                &temp,
                args[0].span,
                &callback,
                callback_shape,
                module,
            )
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::CompactInt =>
        {
            if key_callback_shape.is_some_and(|shape| !array_filter_int_key_callback_shape_is_supported(shape)) {
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web ARRAY_FILTER_USE_KEY callback key type must match integer array keys",
                ));
            }
            if both_callback.is_some_and(|callback| {
                callback.value_kind.is_some_and(|kind| kind != ValueCellKind::Int)
                    || !array_filter_int_key_callback_shape_is_supported(callback.key_shape)
            }) {
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web ARRAY_FILTER_USE_BOTH callback must match integer values and integer keys",
                ));
            }
            if mode == 1 {
                emit_array_filter_compact_int_both_local_assign(name, source, args[0].span, &callback, module)
            } else {
                emit_array_filter_compact_int_key_local_assign(name, source, args[0].span, &callback, module)
            }
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && mode == 2
                && array_filter_key_mode_source_value_kind(source, module).is_some() =>
        {
            let kind = array_filter_key_mode_source_value_kind(source, module)
                .expect("array_filter key-mode source kind was prevalidated");
            emit_array_filter_value_key_local_assign(name, source, args[0].span, &callback, kind, module)
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && mode == 1
                && array_filter_both_mode_source_kind(source, both_callback, module).is_some() =>
        {
            let kind = array_filter_both_mode_source_kind(source, both_callback, module)
                .expect("array_filter both-mode source kind was prevalidated");
            emit_array_filter_value_both_local_assign(name, source, args[0].span, &callback, kind, module)
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && mode == 2
                && array_filter_key_mode_assoc_source_matches_callback(
                    source,
                    key_callback_shape,
                    module,
                ) =>
        {
            let shape = key_callback_shape.expect("array_filter key-mode callback shape was validated");
            emit_array_filter_assoc_key_local_assign(name, source, args[0].span, &callback, shape, module)
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && mode == 1
                && array_filter_both_mode_assoc_source_matches_callback(
                    source,
                    both_callback,
                    module,
                ) =>
        {
            let callback_shape = both_callback.expect("array_filter both-mode callback was validated");
            emit_array_filter_assoc_both_local_assign(
                name,
                source,
                args[0].span,
                &callback,
                callback_shape,
                module,
            )
        }
        _ => Err(CompileError::new(
            args[0].span,
            "wasm32-web ARRAY_FILTER_USE_KEY currently supports homogeneous scalar value arrays and homogeneous associative key shapes; ARRAY_FILTER_USE_BOTH supports matching homogeneous scalar arrays",
        )),
    }
}

fn emit_array_filter_mode_staged_source(
    name: &str,
    source: &str,
    span: crate::span::Span,
    mode: i64,
    callback: &str,
    both_callback: Option<ArrayFilterUseBothCallback>,
    key_callback_shape: Option<ArrayFilterCallbackShape>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match module.array_layout(source) {
        ArrayLayout::CompactInt => {
            if mode == 1 {
                emit_array_filter_compact_int_both_local_assign(name, source, span, callback, module)
            } else {
                emit_array_filter_compact_int_key_local_assign(name, source, span, callback, module)
            }
        }
        ArrayLayout::Value => {
            if mode == 2 {
                if let Some(kind) = array_filter_key_mode_source_value_kind(source, module) {
                    emit_array_filter_value_key_local_assign(name, source, span, callback, kind, module)
                } else {
                    Err(CompileError::new(
                        span,
                        "wasm32-web ARRAY_FILTER_USE_KEY currently supports homogeneous scalar value arrays",
                    ))
                }
            } else if array_filter_both_mode_source_kind(source, both_callback, module).is_some() {
                let kind = array_filter_both_mode_source_kind(source, both_callback, module)
                    .expect("array_filter both-mode source kind was prevalidated");
                emit_array_filter_value_both_local_assign(name, source, span, callback, kind, module)
            } else {
                Err(CompileError::new(
                    span,
                    "wasm32-web ARRAY_FILTER_USE_BOTH currently supports homogeneous scalar value arrays matching the callback",
                ))
            }
        }
        ArrayLayout::Assoc if mode == 2
            && array_filter_key_mode_assoc_source_matches_callback(
                source,
                key_callback_shape,
                module,
            ) =>
        {
            let shape = key_callback_shape.expect("array_filter key-mode callback shape was validated");
            emit_array_filter_assoc_key_local_assign(name, source, span, callback, shape, module)
        }
        ArrayLayout::Assoc if mode == 1
            && array_filter_both_mode_assoc_source_matches_callback(
                source,
                both_callback,
                module,
            ) =>
        {
            let callback_shape = both_callback.expect("array_filter both-mode callback was validated");
            emit_array_filter_assoc_both_local_assign(
                name,
                source,
                span,
                callback,
                callback_shape,
                module,
            )
        }
        _ => Err(CompileError::new(
            span,
            "wasm32-web ARRAY_FILTER_* modes currently support indexed arrays or homogeneous associative key shapes",
        )),
    }
}

fn array_filter_int_key_callback_shape_is_supported(shape: ArrayFilterCallbackShape) -> bool {
    matches!(shape, ArrayFilterCallbackShape::Int | ArrayFilterCallbackShape::Numeric)
}

fn array_filter_key_mode_assoc_literal_matches_callback(
    items: &[(Expr, Expr)],
    callback_shape: Option<ArrayFilterCallbackShape>,
    module: &WasmModule,
) -> bool {
    let Some(callback_shape) = callback_shape else {
        return false;
    };
    let Some(key_kinds) = key_kinds_for_assoc_items(items) else {
        return false;
    };
    array_filter_key_mode_assoc_keys_match_callback(&key_kinds, None, callback_shape)
        && value_cell_kinds_for_assoc_items(items, module).is_some_and(|kinds| {
            kinds.iter()
                .copied()
                .all(array_filter_value_cell_kind_is_supported)
        })
        && normalize_assoc_items(items).is_some()
}

fn array_filter_key_mode_assoc_source_matches_callback(
    source: &str,
    callback_shape: Option<ArrayFilterCallbackShape>,
    module: &WasmModule,
) -> bool {
    let Some(callback_shape) = callback_shape else {
        return false;
    };
    let key_kinds = module.array_key_kinds(source);
    let runtime_key_kind = module.array_runtime_key_kind(source);
    array_filter_key_mode_assoc_keys_match_callback(key_kinds.unwrap_or(&[]), runtime_key_kind, callback_shape)
        && (module.array_value_cell_kinds(source).is_some()
            || module.array_runtime_value_cell_kind(source).is_some())
}

fn array_filter_both_mode_assoc_literal_matches_callback(
    items: &[(Expr, Expr)],
    callback: Option<ArrayFilterUseBothCallback>,
    module: &WasmModule,
) -> bool {
    let Some(callback) = callback else {
        return false;
    };
    let Some(key_kinds) = key_kinds_for_assoc_items(items) else {
        return false;
    };
    array_filter_key_mode_assoc_keys_match_callback(&key_kinds, None, callback.key_shape)
        && value_cell_kinds_for_assoc_items(items, module)
            .and_then(|kinds| array_filter_both_source_kind_for_callback(&kinds, callback))
            .is_some()
        && normalize_assoc_items(items).is_some()
}

fn array_filter_both_mode_assoc_source_matches_callback(
    source: &str,
    callback: Option<ArrayFilterUseBothCallback>,
    module: &WasmModule,
) -> bool {
    let Some(callback) = callback else {
        return false;
    };
    let key_kinds = module.array_key_kinds(source);
    let runtime_key_kind = module.array_runtime_key_kind(source);
    array_filter_key_mode_assoc_keys_match_callback(key_kinds.unwrap_or(&[]), runtime_key_kind, callback.key_shape)
        && (module
            .array_value_cell_kinds(source)
            .and_then(|kinds| array_filter_both_source_kind_for_callback(kinds, callback))
            .is_some()
            || module
                .array_runtime_value_cell_kind(source)
                .filter(|kind| array_filter_both_kind_matches_callback(*kind, callback))
                .is_some())
}

fn array_filter_key_mode_assoc_keys_match_callback(
    key_kinds: &[AssocKeyKind],
    runtime_key_kind: Option<AssocKeyKind>,
    callback_shape: ArrayFilterCallbackShape,
) -> bool {
    match callback_shape {
        ArrayFilterCallbackShape::Int | ArrayFilterCallbackShape::Numeric => {
            key_kinds.iter().all(|kind| *kind == AssocKeyKind::Int)
                && (!key_kinds.is_empty() || runtime_key_kind == Some(AssocKeyKind::Int))
        }
        ArrayFilterCallbackShape::Str => {
            key_kinds.iter().all(|kind| *kind == AssocKeyKind::Str)
                && (!key_kinds.is_empty() || runtime_key_kind == Some(AssocKeyKind::Str))
        }
        ArrayFilterCallbackShape::Mixed => !key_kinds.is_empty() || runtime_key_kind.is_some(),
        _ => false,
    }
}

fn array_filter_key_mode_items_value_kind(
    items: &[Expr],
    module: &WasmModule,
) -> Option<ValueCellKind> {
    let kinds = value_cell_kinds_for_items(items, module)?;
    homogeneous_array_filter_key_value_kind(&kinds)
}

fn array_filter_key_mode_source_value_kind(
    source: &str,
    module: &WasmModule,
) -> Option<ValueCellKind> {
    if let Some(kind) = module
        .array_value_cell_kinds(source)
        .and_then(homogeneous_array_filter_key_value_kind)
    {
        return Some(kind);
    }
    module
        .array_runtime_value_cell_kind(source)
        .filter(|kind| array_filter_key_value_kind_is_supported(*kind))
}

fn homogeneous_array_filter_key_value_kind(kinds: &[ValueCellKind]) -> Option<ValueCellKind> {
    let first = *kinds.first()?;
    if !array_filter_key_value_kind_is_supported(first) {
        return None;
    }
    kinds.iter().all(|kind| *kind == first).then_some(first)
}

fn array_filter_key_value_kind_is_supported(kind: ValueCellKind) -> bool {
    matches!(
        kind,
        ValueCellKind::Int | ValueCellKind::Str | ValueCellKind::Bool | ValueCellKind::Float | ValueCellKind::Null
    )
}

fn array_filter_both_mode_items_source_kind(
    items: &[Expr],
    callback: Option<ArrayFilterUseBothCallback>,
    module: &WasmModule,
) -> Option<ValueCellKind> {
    let Some(callback) = callback else {
        return None;
    };
    if !array_filter_int_key_callback_shape_is_supported(callback.key_shape) {
        return None;
    }
    value_cell_kinds_for_items(items, module)
        .and_then(|kinds| array_filter_both_source_kind_for_callback(&kinds, callback))
}

fn array_filter_both_mode_source_kind(
    source: &str,
    callback: Option<ArrayFilterUseBothCallback>,
    module: &WasmModule,
) -> Option<ValueCellKind> {
    let Some(callback) = callback else {
        return None;
    };
    if !array_filter_int_key_callback_shape_is_supported(callback.key_shape) {
        return None;
    }
    if let Some(kind) = module
        .array_value_cell_kinds(source)
        .and_then(|kinds| array_filter_both_source_kind_for_callback(kinds, callback))
    {
        return Some(kind);
    }
    module
        .array_runtime_value_cell_kind(source)
        .filter(|kind| array_filter_both_kind_matches_callback(*kind, callback))
}

fn array_filter_both_source_kind_for_callback(
    kinds: &[ValueCellKind],
    callback: ArrayFilterUseBothCallback,
) -> Option<ValueCellKind> {
    let first = *kinds.first()?;
    kinds
        .iter()
        .all(|kind| *kind == first)
        .then_some(first)
        .filter(|kind| array_filter_both_kind_matches_callback(*kind, callback))
}

fn array_filter_both_kind_matches_callback(
    kind: ValueCellKind,
    callback: ArrayFilterUseBothCallback,
) -> bool {
    callback.value_kind.map_or_else(
        || array_filter_value_cell_kind_is_supported(kind),
        |callback_kind| kind == callback_kind
            || (kind == ValueCellKind::Str && callback_kind == ValueCellKind::Float),
    )
}

fn validate_array_filter_use_both_callback(
    callback: &str,
    span: crate::span::Span,
    module: &WasmModule,
) -> Result<ArrayFilterUseBothCallback, CompileError> {
    let Some(param_kinds) = module.function_param_kinds(callback) else {
        return Err(CompileError::new(
            span,
            "wasm32-web array_filter() callback metadata is missing",
        ));
    };
    match (param_kinds.as_slice(), module.function_return_kind(callback)) {
        ([LocalKind::I64, LocalKind::I64], Some(ValueKind::Bool | ValueKind::Int)) => {
            Ok(ArrayFilterUseBothCallback {
                value_kind: Some(ValueCellKind::Int),
                key_shape: ArrayFilterCallbackShape::Int,
            })
        }
        ([LocalKind::Str, LocalKind::I64], Some(ValueKind::Bool | ValueKind::Int)) => {
            Ok(ArrayFilterUseBothCallback {
                value_kind: Some(ValueCellKind::Str),
                key_shape: ArrayFilterCallbackShape::Int,
            })
        }
        ([LocalKind::I32, LocalKind::I64], Some(ValueKind::Bool | ValueKind::Int)) => {
            Ok(ArrayFilterUseBothCallback {
                value_kind: Some(ValueCellKind::Bool),
                key_shape: ArrayFilterCallbackShape::Int,
            })
        }
        ([LocalKind::F64, LocalKind::I64], Some(ValueKind::Bool | ValueKind::Int)) => {
            Ok(ArrayFilterUseBothCallback {
                value_kind: Some(ValueCellKind::Float),
                key_shape: ArrayFilterCallbackShape::Int,
            })
        }
        ([LocalKind::Mixed, LocalKind::I64], Some(ValueKind::Bool | ValueKind::Int)) => {
            Ok(ArrayFilterUseBothCallback {
                value_kind: None,
                key_shape: ArrayFilterCallbackShape::Int,
            })
        }
        ([LocalKind::I64, LocalKind::Str], Some(ValueKind::Bool | ValueKind::Int)) => {
            Ok(ArrayFilterUseBothCallback {
                value_kind: Some(ValueCellKind::Int),
                key_shape: ArrayFilterCallbackShape::Str,
            })
        }
        ([LocalKind::Str, LocalKind::Str], Some(ValueKind::Bool | ValueKind::Int)) => {
            Ok(ArrayFilterUseBothCallback {
                value_kind: Some(ValueCellKind::Str),
                key_shape: ArrayFilterCallbackShape::Str,
            })
        }
        ([LocalKind::I32, LocalKind::Str], Some(ValueKind::Bool | ValueKind::Int)) => {
            Ok(ArrayFilterUseBothCallback {
                value_kind: Some(ValueCellKind::Bool),
                key_shape: ArrayFilterCallbackShape::Str,
            })
        }
        ([LocalKind::F64, LocalKind::Str], Some(ValueKind::Bool | ValueKind::Int)) => {
            Ok(ArrayFilterUseBothCallback {
                value_kind: Some(ValueCellKind::Float),
                key_shape: ArrayFilterCallbackShape::Str,
            })
        }
        ([LocalKind::Mixed, LocalKind::Str], Some(ValueKind::Bool | ValueKind::Int)) => {
            Ok(ArrayFilterUseBothCallback {
                value_kind: None,
                key_shape: ArrayFilterCallbackShape::Str,
            })
        }
        _ => Err(CompileError::new(
            span,
            "wasm32-web ARRAY_FILTER_USE_BOTH currently requires a scalar value and int/string key callback",
        )),
    }
}
