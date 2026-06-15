//! Purpose:
//! Lowers WASM `array_reduce()` calls over supported static and runtime array
//! shapes.
//!
//! Called from:
//! - `super::emit_expr()` and string/output kind probes.
//!
//! Key details:
//! - Callback signatures are resolved from user-function metadata before
//!   lowering, and unsupported dynamic callback shapes remain compile errors.

use super::*;
use super::array_reduce_assoc::*;
use super::array_reduce_values::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArrayReduceCallbackShape {
    IntToInt,
    IntObjectToInt,
    FloatToFloat,
    FloatObjectToFloat,
    BoolToBool,
    BoolObjectToBool,
    StrToStr,
    StrObjectToStr,
    MixedMixedToInt,
    MixedMixedToBool,
    MixedMixedToFloat,
    MixedMixedToStr,
}

enum ArrayReduceDynamicSource<'a> {
    Literal(&'a [Expr]),
    Local(&'a str, crate::span::Span),
}

fn instance_callback_for_array_reduce(
    callback: &Expr,
    module: &mut WasmModule,
) -> Result<Option<(String, String)>, CompileError> {
    if let ExprKind::Variable(callback_var) = &callback.kind {
        if let Some((target, capture_local)) = module.callable_instance_target(callback_var) {
            return Ok(Some((target, capture_local)));
        }
        if let Some((target, _)) = instance_callback_target(callback, "__invoke", module) {
            return Ok(Some((target, callback_var.clone())));
        }
    }
    if let Some((object, method)) = fixed_instance_callable_array_parts(callback, module) {
        return capture_instance_callback_for_array_reduce(callback, object, &method, module);
    }
    let ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) = &callback.kind else {
        if let Some((target, class_name)) = instance_callback_target(callback, "__invoke", module) {
            let capture_local = module
                .next_label("array_reduce_invokable_object")
                .trim_start_matches('$')
                .to_string();
            module.declare_object_local(capture_local.clone());
            let kind = emit_expr(callback, module)?;
            if kind != ValueKind::Object {
                return Err(CompileError::new(
                    callback.span,
                    "wasm32-web array_reduce() invokable callback requires an object receiver",
                ));
            }
            module.body().line(&format!("local.set ${}", capture_local));
            module.set_object_class_for_local(&capture_local, Some(class_name));
            return Ok(Some((target, capture_local)));
        }
        return Ok(None);
    };
    capture_instance_callback_for_array_reduce(callback, object, method, module)
}

fn fixed_instance_callable_array_parts<'a>(
    callback: &'a Expr,
    module: &WasmModule,
) -> Option<(&'a Expr, String)> {
    let (receiver, method) = match &callback.kind {
        ExprKind::ArrayLiteral(items) => {
            let [receiver, method] = items.as_slice() else {
                return None;
            };
            (receiver, method)
        }
        ExprKind::ArrayLiteralAssoc(items) => (
            fixed_callable_assoc_value(items, 0, module)?,
            fixed_callable_assoc_value(items, 1, module)?,
        ),
        _ => return None,
    };
    let method = fixed_callable_static_string_value(method, module)?;
    instance_callback_target(receiver, &method, module)?;
    Some((receiver, method))
}

fn fixed_callable_assoc_value<'a>(
    items: &'a [(Expr, Expr)],
    needle: i64,
    module: &WasmModule,
) -> Option<&'a Expr> {
    items
        .iter()
        .rev()
        .find_map(|(key, value)| {
            matches!(
                static_assoc_access_key(key, module),
                Some(AssocKeyValue::Int(value)) if value == needle
            )
            .then_some(value)
        })
}

fn fixed_callable_static_string_value(expr: &Expr, module: &WasmModule) -> Option<String> {
    match &expr.kind {
        ExprKind::Variable(name) => module.string_static_value(name),
        _ => static_string_value(expr, module),
    }
}

fn capture_instance_callback_for_array_reduce(
    callback: &Expr,
    object: &Expr,
    method: &str,
    module: &mut WasmModule,
) -> Result<Option<(String, String)>, CompileError> {
    let capture_local = module
        .next_label("array_reduce_callable_object")
        .trim_start_matches('$')
        .to_string();
    let Some((target, class_name)) = instance_callback_target(object, method, module) else {
        return Ok(None);
    };
    module.declare_object_local(capture_local.clone());
    let kind = emit_expr(object, module)?;
    if kind != ValueKind::Object {
        return Err(CompileError::new(
            callback.span,
            "wasm32-web array_reduce() instance callback requires an object receiver",
        ));
    }
    module.body().line(&format!("local.set ${}", capture_local));
    module.set_object_class_for_local(&capture_local, Some(class_name));
    Ok(Some((target, capture_local)))
}

fn instance_callback_target(
    object: &Expr,
    method: &str,
    module: &WasmModule,
) -> Option<(String, String)> {
    let Some(class_name) = object_class_name_for_expr(object, module) else {
        return None;
    };
    let Some((declaring_class, method_info)) = module.object_method_in_hierarchy(&class_name, method) else {
        return None;
    };
    if !module.object_member_is_accessible(&method_info.owner_class, &method_info.visibility) {
        return None;
    }
    if !declaring_class.eq_ignore_ascii_case(&class_name)
        && method_body_uses_this_property(&method_info.body)
        && !inherited_property_layout_available(&class_name, &declaring_class, module)
    {
        return None;
    }
    Some((method_info.symbol, class_name))
}

pub(super) fn is_callback_array_builtin(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "array_map" | "array_filter" | "array_reduce" | "array_walk" | "usort" | "uksort" | "uasort"
    )
}

pub(super) fn emit_array_reduce_call(
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if args.len() != 3 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_reduce() currently requires array, callback, and initial value",
        ));
    }
    if let Some((callback, capture_local)) = instance_callback_for_array_reduce(&args[1], module)? {
            let param_kinds = module.function_param_kinds(&callback).ok_or_else(|| {
                CompileError::new(
                    args[1].span,
                    "wasm32-web array_reduce() instance callback metadata is missing",
                )
            })?;
            let callback_takes_ints =
                param_kinds.as_slice() == [LocalKind::Object, LocalKind::I64, LocalKind::I64]
                    && module.function_return_kind(&callback) == Some(ValueKind::Int);
            let callback_takes_object_int =
                param_kinds.as_slice() == [LocalKind::Object, LocalKind::I64, LocalKind::Object]
                    && module.function_return_kind(&callback) == Some(ValueKind::Int);
            let callback_takes_strings =
                param_kinds.as_slice() == [LocalKind::Object, LocalKind::Str, LocalKind::Str]
                    && module.function_return_kind(&callback) == Some(ValueKind::Str);
            let callback_takes_object_string =
                param_kinds.as_slice() == [LocalKind::Object, LocalKind::Str, LocalKind::Object]
                    && module.function_return_kind(&callback) == Some(ValueKind::Str);
            let callback_takes_floats =
                param_kinds.as_slice() == [LocalKind::Object, LocalKind::F64, LocalKind::F64]
                    && module.function_return_kind(&callback) == Some(ValueKind::Float);
            let callback_takes_object_float =
                param_kinds.as_slice() == [LocalKind::Object, LocalKind::F64, LocalKind::Object]
                    && module.function_return_kind(&callback) == Some(ValueKind::Float);
            let callback_takes_bools =
                param_kinds.as_slice() == [LocalKind::Object, LocalKind::I32, LocalKind::I32]
                    && module.function_return_kind(&callback) == Some(ValueKind::Bool);
            let callback_takes_object_bool =
                param_kinds.as_slice() == [LocalKind::Object, LocalKind::I32, LocalKind::Object]
                    && module.function_return_kind(&callback) == Some(ValueKind::Bool);
            if !callback_takes_ints
                && !callback_takes_object_int
                && !callback_takes_strings
                && !callback_takes_object_string
                && !callback_takes_floats
                && !callback_takes_object_float
                && !callback_takes_bools
                && !callback_takes_object_bool
            {
                return Err(CompileError::new(
                    args[1].span,
                    "wasm32-web array_reduce() instance callbacks currently require scalar-to-scalar or scalar/object-to-scalar reducers",
                ));
            }
            if callback_takes_ints || callback_takes_object_int {
                let acc = module.next_label("array_reduce_acc").trim_start_matches('$').to_string();
                module.declare_i64_local(acc.clone());
                require_int(&args[2], module)?;
                module.body().line(&format!("local.set ${}", acc));
                if let ExprKind::Variable(source) = &args[0].kind {
                    if callback_takes_object_int
                        && module.local_kind(source) == Some(LocalKind::Array)
                        && module.array_layout(source) == ArrayLayout::Value
                        && array_reduce_object_value_cells_are_supported(source, module)
                    {
                        emit_array_reduce_value_object_int_local_instance(
                            &acc,
                            source,
                            args[0].span,
                            &callback,
                            &capture_local,
                            module,
                        )?;
                        module.body().line(&format!("local.get ${}", acc));
                        return Ok(ValueKind::Int);
                    }
                    if callback_takes_ints
                    && module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::CompactInt
                    {
                        emit_array_reduce_compact_int_local_instance(
                            &acc,
                            source,
                            args[0].span,
                            &callback,
                            &capture_local,
                            module,
                        )?;
                        module.body().line(&format!("local.get ${}", acc));
                        return Ok(ValueKind::Int);
                    }
                    if callback_takes_ints
                        && module.local_kind(source) == Some(LocalKind::Array)
                        && module.array_layout(source) == ArrayLayout::Assoc
                        && array_filter_assoc_local_matches_shape(
                            source,
                            ArrayFilterCallbackShape::Int,
                            module,
                        )
                    {
                        emit_array_reduce_assoc_int_local_instance(
                            &acc,
                            source,
                            args[0].span,
                            &callback,
                            &capture_local,
                            module,
                        )?;
                        module.body().line(&format!("local.get ${}", acc));
                        return Ok(ValueKind::Int);
                    }
                    if callback_takes_ints
                        && module.local_kind(source) == Some(LocalKind::Array)
                        && module.array_layout(source) == ArrayLayout::Assoc
                        && matches!(
                            module.array_runtime_value_cell_kind(source),
                            Some(ValueCellKind::Int | ValueCellKind::Bool | ValueCellKind::Str)
                        )
                    {
                        emit_array_reduce_runtime_assoc_int_local_instance(
                            &acc,
                            source,
                            args[0].span,
                            &callback,
                            &capture_local,
                            module,
                        )?;
                        module.body().line(&format!("local.get ${}", acc));
                        return Ok(ValueKind::Int);
                    }
                }
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web array_reduce() instance int callbacks currently require an assigned compact integer or associative integer array",
                ));
            }
            if callback_takes_floats || callback_takes_object_float {
                let acc = module.next_label("array_reduce_acc").trim_start_matches('$').to_string();
                module.declare_f64_local(acc.clone());
                require_float(&args[2], module)?;
                module.body().line(&format!("local.set ${}", acc));
                if let ExprKind::Variable(source) = &args[0].kind {
                    if callback_takes_object_float
                        && module.local_kind(source) == Some(LocalKind::Array)
                        && module.array_layout(source) == ArrayLayout::Value
                        && array_reduce_object_value_cells_are_supported(source, module)
                    {
                        emit_array_reduce_value_object_float_local_instance(
                            &acc,
                            source,
                            args[0].span,
                            &callback,
                            &capture_local,
                            module,
                        )?;
                        module.body().line(&format!("local.get ${}", acc));
                        return Ok(ValueKind::Float);
                    }
                    if callback_takes_floats
                        && module.local_kind(source) == Some(LocalKind::Array)
                        && module.array_layout(source) == ArrayLayout::Value
                        && (module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Float)
                            || module.array_value_cell_kinds(source).is_some_and(|kinds| {
                                kinds.iter().all(|kind| *kind == ValueCellKind::Float)
                            }))
                    {
                        emit_array_reduce_value_float_local_instance(
                            &acc,
                            source,
                            args[0].span,
                            &callback,
                            &capture_local,
                            module,
                        )?;
                        module.body().line(&format!("local.get ${}", acc));
                        return Ok(ValueKind::Float);
                    }
                    if callback_takes_floats
                        && module.local_kind(source) == Some(LocalKind::Array)
                        && module.array_layout(source) == ArrayLayout::Assoc
                        && array_filter_assoc_local_matches_shape(
                            source,
                            ArrayFilterCallbackShape::Float,
                            module,
                        )
                    {
                        emit_array_reduce_assoc_float_local_instance(
                            &acc,
                            source,
                            args[0].span,
                            &callback,
                            &capture_local,
                            module,
                        )?;
                        module.body().line(&format!("local.get ${}", acc));
                        return Ok(ValueKind::Float);
                    }
                    if callback_takes_floats
                        && module.local_kind(source) == Some(LocalKind::Array)
                        && module.array_layout(source) == ArrayLayout::Assoc
                        && module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Float)
                    {
                        emit_array_reduce_runtime_assoc_float_local_instance(
                            &acc,
                            source,
                            args[0].span,
                            &callback,
                            &capture_local,
                            module,
                        )?;
                        module.body().line(&format!("local.get ${}", acc));
                        return Ok(ValueKind::Float);
                    }
                }
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web array_reduce() instance float callbacks currently require an assigned float value-cell or associative array",
                ));
            }
            if callback_takes_bools || callback_takes_object_bool {
                let acc = module.next_label("array_reduce_acc").trim_start_matches('$').to_string();
                module.declare_i32_local(acc.clone());
                if emit_expr(&args[2], module)? != ValueKind::Bool {
                    return Err(CompileError::new(
                        args[2].span,
                        "wasm32-web array_reduce() instance bool callbacks require a bool initial value",
                    ));
                }
                module.body().line(&format!("local.set ${}", acc));
                if let ExprKind::Variable(source) = &args[0].kind {
                    if callback_takes_object_bool
                        && module.local_kind(source) == Some(LocalKind::Array)
                        && module.array_layout(source) == ArrayLayout::Value
                        && array_reduce_object_value_cells_are_supported(source, module)
                    {
                        emit_array_reduce_value_object_bool_local_instance(
                            &acc,
                            source,
                            args[0].span,
                            &callback,
                            &capture_local,
                            module,
                        )?;
                        module.body().line(&format!("local.get ${}", acc));
                        return Ok(ValueKind::Bool);
                    }
                    if callback_takes_bools
                        && module.local_kind(source) == Some(LocalKind::Array)
                        && module.array_layout(source) == ArrayLayout::Value
                        && (module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Bool)
                            || module.array_value_cell_kinds(source).is_some_and(|kinds| {
                                kinds.iter().all(|kind| *kind == ValueCellKind::Bool)
                            }))
                    {
                        emit_array_reduce_value_bool_local_instance(
                            &acc,
                            source,
                            args[0].span,
                            &callback,
                            &capture_local,
                            module,
                        )?;
                        module.body().line(&format!("local.get ${}", acc));
                        return Ok(ValueKind::Bool);
                    }
                    if callback_takes_bools
                        && module.local_kind(source) == Some(LocalKind::Array)
                        && module.array_layout(source) == ArrayLayout::Assoc
                        && array_filter_assoc_local_matches_shape(
                            source,
                            ArrayFilterCallbackShape::Bool,
                            module,
                        )
                    {
                        emit_array_reduce_assoc_bool_local_instance(
                            &acc,
                            source,
                            args[0].span,
                            &callback,
                            &capture_local,
                            module,
                        )?;
                        module.body().line(&format!("local.get ${}", acc));
                        return Ok(ValueKind::Bool);
                    }
                    if callback_takes_bools
                        && module.local_kind(source) == Some(LocalKind::Array)
                        && module.array_layout(source) == ArrayLayout::Assoc
                        && module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Bool)
                    {
                        emit_array_reduce_runtime_assoc_bool_local_instance(
                            &acc,
                            source,
                            args[0].span,
                            &callback,
                            &capture_local,
                            module,
                        )?;
                        module.body().line(&format!("local.get ${}", acc));
                        return Ok(ValueKind::Bool);
                    }
                }
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web array_reduce() instance bool callbacks currently require an assigned bool value-cell or associative array",
                ));
            }
            let acc_ptr = module
                .next_label("array_reduce_acc_ptr")
                .trim_start_matches('$')
                .to_string();
            let acc_len = module
                .next_label("array_reduce_acc_len")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(acc_ptr.clone());
            module.declare_i32_local(acc_len.clone());
            emit_string_value_to_stack(&args[2], module)?;
            module.body().line(&format!("local.set ${}", acc_len));
            module.body().line(&format!("local.set ${}", acc_ptr));
            if let ExprKind::Variable(source) = &args[0].kind {
                if callback_takes_object_string
                    && module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::Value
                    && array_reduce_object_value_cells_are_supported(source, module)
                {
                    emit_array_reduce_value_object_string_local_instance(
                        &acc_ptr,
                        &acc_len,
                        source,
                        args[0].span,
                        &callback,
                        &capture_local,
                        module,
                    )?;
                    module.body().line(&format!("local.get ${}", acc_ptr));
                    module.body().line(&format!("local.get ${}", acc_len));
                    return Ok(ValueKind::Str);
                }
                if callback_takes_strings
                    && module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::Value
                    && array_map_value_cells_are_strings(source, module)
                {
                    emit_array_reduce_value_string_local_instance(
                        &acc_ptr,
                        &acc_len,
                        source,
                        args[0].span,
                        &callback,
                        &capture_local,
                        module,
                    )?;
                    module.body().line(&format!("local.get ${}", acc_ptr));
                    module.body().line(&format!("local.get ${}", acc_len));
                    return Ok(ValueKind::Str);
                }
                if callback_takes_strings
                    && module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::Assoc
                    && array_filter_assoc_local_matches_shape(
                        source,
                        ArrayFilterCallbackShape::Str,
                        module,
                    )
                {
                    emit_array_reduce_assoc_string_local_instance(
                        &acc_ptr,
                        &acc_len,
                        source,
                        args[0].span,
                        &callback,
                        &capture_local,
                        module,
                    )?;
                    module.body().line(&format!("local.get ${}", acc_ptr));
                    module.body().line(&format!("local.get ${}", acc_len));
                    return Ok(ValueKind::Str);
                }
                if callback_takes_strings
                    && module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::Assoc
                    && module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Str)
                {
                    emit_array_reduce_runtime_assoc_string_local_instance(
                        &acc_ptr,
                        &acc_len,
                        source,
                        args[0].span,
                        &callback,
                        &capture_local,
                        module,
                    )?;
                    module.body().line(&format!("local.get ${}", acc_ptr));
                    module.body().line(&format!("local.get ${}", acc_len));
                    return Ok(ValueKind::Str);
                }
            }
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_reduce() instance string callbacks currently require an assigned string value-cell or associative string array",
            ));
    }
    let Some(callback) = evaluated_static_callback_function_name(&args[1], module)? else {
        if let Some(kind) = emit_array_reduce_static_callback_ternary_call(args, module)? {
            return Ok(kind);
        }
        if let Some(kind) = emit_array_reduce_dynamic_callable_descriptor_call(args, module)? {
            return Ok(kind);
        }
        if let Some(kind) = emit_array_reduce_dynamic_static_return_callback_call(args, module)? {
            return Ok(kind);
        }
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web array_reduce() currently requires a static string, direct first-class function callback, or simple callable variable alias",
        ));
    };
    if !module.has_function(&callback) {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web array_reduce() currently requires a user-defined callback",
        ));
    }
    let shape = array_reduce_callback_shape(&callback, args[1].span, module)?;
    if shape == ArrayReduceCallbackShape::MixedMixedToInt {
        return emit_array_reduce_mixed_to_int_call(expr, args, &callback, module);
    }
    if shape == ArrayReduceCallbackShape::MixedMixedToBool {
        return emit_array_reduce_mixed_to_bool_call(expr, args, &callback, module);
    }
    if shape == ArrayReduceCallbackShape::MixedMixedToFloat {
        return emit_array_reduce_mixed_to_float_call(expr, args, &callback, module);
    }
    if shape == ArrayReduceCallbackShape::MixedMixedToStr {
        return emit_array_reduce_mixed_to_string_call(expr, args, &callback, module);
    }
    if shape == ArrayReduceCallbackShape::StrToStr {
        return emit_array_reduce_string_call(expr, args, &callback, module);
    }
    if shape == ArrayReduceCallbackShape::StrObjectToStr {
        return emit_array_reduce_string_object_call(expr, args, &callback, module);
    }
    if shape == ArrayReduceCallbackShape::IntObjectToInt {
        return emit_array_reduce_int_object_call(expr, args, &callback, module);
    }
    if shape == ArrayReduceCallbackShape::FloatObjectToFloat {
        return emit_array_reduce_float_object_call(expr, args, &callback, module);
    }
    if shape == ArrayReduceCallbackShape::BoolObjectToBool {
        return emit_array_reduce_bool_object_call(expr, args, &callback, module);
    }
    if shape == ArrayReduceCallbackShape::FloatToFloat {
        return emit_array_reduce_float_call(expr, args, &callback, module);
    }
    if shape == ArrayReduceCallbackShape::BoolToBool {
        return emit_array_reduce_bool_call(expr, args, &callback, module);
    }

    let acc = module.next_label("array_reduce_acc").trim_start_matches('$').to_string();
    module.declare_i64_local(acc.clone());
    require_int(&args[2], module)?;
    module.body().line(&format!("local.set ${}", acc));

    match &args[0].kind {
        ExprKind::ArrayLiteral(items) if !array_literal_needs_value_cells(items) => {
            emit_array_reduce_literal_ints(&acc, items, &callback, module)?;
        }
        ExprKind::ArrayLiteral(items) if array_map_items_are_ints(items, module) => {
            emit_array_reduce_literal_ints(&acc, items, &callback, module)?;
        }
        ExprKind::ArrayLiteral(items) if array_filter_items_are_bools(items, module) => {
            emit_array_reduce_literal_bools_as_ints(&acc, items, &callback, module)?;
        }
        ExprKind::ArrayLiteral(items) if array_map_items_are_strings(items, module) => {
            emit_array_reduce_literal_numeric_strings_as_ints(&acc, items, &callback, module)?;
        }
        ExprKind::ArrayLiteralAssoc(items) if array_filter_assoc_items_match_shape(
            items,
            ArrayFilterCallbackShape::Int,
            module,
        ) =>
        {
            emit_array_reduce_assoc_literal_ints(&acc, items, args[0].span, &callback, module)?;
        }
        ExprKind::ArrayLiteralAssoc(items) if array_filter_assoc_items_match_shape(
            items,
            ArrayFilterCallbackShape::Bool,
            module,
        ) =>
        {
            emit_array_reduce_assoc_literal_ints(&acc, items, args[0].span, &callback, module)?;
        }
        ExprKind::ArrayLiteralAssoc(items)
            if value_cell_kinds_for_assoc_items(items, module)
                .is_some_and(|kinds| kinds.iter().all(|kind| *kind == ValueCellKind::Str)) =>
        {
            emit_array_reduce_assoc_literal_ints(&acc, items, args[0].span, &callback, module)?;
        }
        ExprKind::FunctionCall { .. } | ExprKind::ExprCall { .. }
            if expression_has_array_type(&args[0], module) =>
        {
            emit_array_reduce_int_array_expr(&acc, &args[0], &callback, module)?;
        }
        ExprKind::DynamicStaticMethodCall { receiver, method, .. }
            if dynamic_static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            let temp = materialize_dynamic_static_method_array_reduce_source(
                &args[0],
                receiver,
                method,
                "array_reduce_dynamic_static_method_source",
                module,
            )?;
            emit_array_reduce_int_staged_local(&acc, &temp, args[0].span, &callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::CompactInt =>
        {
            emit_array_reduce_compact_int_local(&acc, source, args[0].span, &callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_ints(source, module) =>
        {
            emit_array_reduce_value_int_local(&acc, source, args[0].span, &callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_bools(source, module) =>
        {
            emit_array_reduce_value_int_local(&acc, source, args[0].span, &callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_reduce_value_numeric_string_as_int_local(&acc, source, args[0].span, &callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_filter_assoc_local_matches_shape(
                    source,
                    ArrayFilterCallbackShape::Int,
                    module,
                ) =>
        {
            emit_array_reduce_assoc_int_local(&acc, source, args[0].span, &callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_filter_assoc_local_matches_shape(
                    source,
                    ArrayFilterCallbackShape::Bool,
                    module,
                ) =>
        {
            emit_array_reduce_assoc_int_local(&acc, source, args[0].span, &callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && module.array_value_cell_kinds(source).is_some_and(|kinds| {
                    kinds.iter().all(|kind| *kind == ValueCellKind::Str)
                }) =>
        {
            emit_array_reduce_assoc_int_local(&acc, source, args[0].span, &callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Int) =>
        {
            emit_array_reduce_runtime_assoc_int_local(&acc, source, args[0].span, &callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Bool) =>
        {
            emit_array_reduce_runtime_assoc_int_local(&acc, source, args[0].span, &callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Str) =>
        {
            emit_array_reduce_runtime_assoc_int_local(&acc, source, args[0].span, &callback, module)?;
        }
        _ => {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_reduce() currently supports integer arrays for int callbacks",
            ));
        }
    }

    module.body().line(&format!("local.get ${}", acc));
    Ok(ValueKind::Int)
}

fn emit_array_reduce_static_callback_ternary_call(
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    let [source, callback_expr, initial] = args else {
        return Ok(None);
    };
    let ExprKind::Ternary {
        condition,
        then_expr,
        else_expr,
    } = &callback_expr.kind
    else {
        return Ok(None);
    };
    let Some(then_callback) = static_callback_function_name(then_expr, module) else {
        return Ok(None);
    };
    let Some(else_callback) = static_callback_function_name(else_expr, module) else {
        return Ok(None);
    };
    if then_callback.eq_ignore_ascii_case(&else_callback) {
        return Ok(None);
    }
    for callback in [&then_callback, &else_callback] {
        if !module.has_function(callback) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web array_reduce() callable ternary arms currently require user-defined callbacks",
            ));
        }
    }
    let then_shape = array_reduce_callback_shape(&then_callback, then_expr.span, module)?;
    let else_shape = array_reduce_callback_shape(&else_callback, else_expr.span, module)?;
    if then_shape != else_shape || !array_reduce_branch_callback_shape_is_supported(then_shape) {
        return Err(CompileError::new(
            callback_expr.span,
            "wasm32-web array_reduce() callable ternary arms currently require the same supported scalar callback shape",
        ));
    }

    let source_storage;
    let dynamic_source = match &source.kind {
        ExprKind::ArrayLiteral(items) => ArrayReduceDynamicSource::Literal(items),
        ExprKind::Variable(name)
            if module.local_kind(name) == Some(LocalKind::Array) =>
        {
            ArrayReduceDynamicSource::Local(name, source.span)
        }
        _ if expression_has_array_type(source, module) => {
            source_storage = module
                .next_label("array_reduce_ternary_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(source_storage.clone());
            emit_array_assign(&source_storage, source, module)?;
            ArrayReduceDynamicSource::Local(&source_storage, source.span)
        }
        _ => return Ok(None),
    };
    if !array_reduce_dynamic_source_supports_shape(&dynamic_source, then_shape, module) {
        return Ok(None);
    }

    emit_condition(condition, module)?;
    module.body().open(array_reduce_branch_result_if(then_shape));
    emit_array_reduce_branch_source_call(&dynamic_source, initial, &then_callback, then_shape, module)?;
    module.body().line("else");
    emit_array_reduce_branch_source_call(&dynamic_source, initial, &else_callback, then_shape, module)?;
    module.body().close("end");
    Ok(Some(array_reduce_branch_value_kind(then_shape)))
}

fn array_reduce_branch_callback_shape_is_supported(shape: ArrayReduceCallbackShape) -> bool {
    matches!(
        shape,
        ArrayReduceCallbackShape::IntToInt
            | ArrayReduceCallbackShape::FloatToFloat
            | ArrayReduceCallbackShape::BoolToBool
            | ArrayReduceCallbackShape::StrToStr
    )
}

fn array_reduce_branch_result_if(shape: ArrayReduceCallbackShape) -> &'static str {
    match shape {
        ArrayReduceCallbackShape::IntToInt => "if (result i64)",
        ArrayReduceCallbackShape::FloatToFloat => "if (result f64)",
        ArrayReduceCallbackShape::BoolToBool => "if (result i32)",
        ArrayReduceCallbackShape::StrToStr => "if (result i32 i32)",
        _ => unreachable!("unsupported branch reduce shape"),
    }
}

fn array_reduce_branch_value_kind(shape: ArrayReduceCallbackShape) -> ValueKind {
    match shape {
        ArrayReduceCallbackShape::IntToInt => ValueKind::Int,
        ArrayReduceCallbackShape::FloatToFloat => ValueKind::Float,
        ArrayReduceCallbackShape::BoolToBool => ValueKind::Bool,
        ArrayReduceCallbackShape::StrToStr => ValueKind::Str,
        _ => unreachable!("unsupported branch reduce shape"),
    }
}

fn emit_array_reduce_branch_source_call(
    source: &ArrayReduceDynamicSource<'_>,
    initial: &Expr,
    callback: &str,
    shape: ArrayReduceCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match shape {
        ArrayReduceCallbackShape::IntToInt => {
            let acc = module
                .next_label("array_reduce_ternary_acc")
                .trim_start_matches('$')
                .to_string();
            module.declare_i64_local(acc.clone());
            require_int(initial, module)?;
            module.body().line(&format!("local.set ${}", acc));
            emit_array_reduce_dynamic_int_source(&acc, source, callback, module)?;
            module.body().line(&format!("local.get ${}", acc));
        }
        ArrayReduceCallbackShape::FloatToFloat => {
            let acc = module
                .next_label("array_reduce_ternary_acc")
                .trim_start_matches('$')
                .to_string();
            module.declare_f64_local(acc.clone());
            emit_array_reduce_float_initial(initial, module)?;
            module.body().line(&format!("local.set ${}", acc));
            emit_array_reduce_dynamic_float_source(&acc, source, callback, module)?;
            module.body().line(&format!("local.get ${}", acc));
        }
        ArrayReduceCallbackShape::BoolToBool => {
            let acc = module
                .next_label("array_reduce_ternary_acc")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(acc.clone());
            emit_condition(initial, module)?;
            module.body().line(&format!("local.set ${}", acc));
            emit_array_reduce_dynamic_bool_source(&acc, source, callback, module)?;
            module.body().line(&format!("local.get ${}", acc));
        }
        ArrayReduceCallbackShape::StrToStr => {
            let acc_ptr = module
                .next_label("array_reduce_ternary_acc_ptr")
                .trim_start_matches('$')
                .to_string();
            let acc_len = module
                .next_label("array_reduce_ternary_acc_len")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(acc_ptr.clone());
            module.declare_i32_local(acc_len.clone());
            emit_string_value_to_stack(initial, module)?;
            module.body().line(&format!("local.set ${}", acc_len));
            module.body().line(&format!("local.set ${}", acc_ptr));
            emit_array_reduce_dynamic_string_source(&acc_ptr, &acc_len, source, callback, module)?;
            module.body().line(&format!("local.get ${}", acc_ptr));
            module.body().line(&format!("local.get ${}", acc_len));
        }
        _ => unreachable!("unsupported branch reduce shape"),
    }
    Ok(())
}

fn emit_array_reduce_dynamic_static_return_callback_call(
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    let [source, callback_expr, initial] = args else {
        return Ok(None);
    };
    let Some(callbacks) = dynamic_array_reduce_callback_names(callback_expr, module) else {
        return Ok(None);
    };
    let source_storage;
    let dynamic_source = match &source.kind {
        ExprKind::ArrayLiteral(items) => ArrayReduceDynamicSource::Literal(items),
        ExprKind::Variable(name)
            if module.local_kind(name) == Some(LocalKind::Array) =>
        {
            ArrayReduceDynamicSource::Local(name, source.span)
        }
        _ if expression_has_array_type(source, module) => {
            source_storage = module
                .next_label("array_reduce_dynamic_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(source_storage.clone());
            emit_array_assign(&source_storage, source, module)?;
            ArrayReduceDynamicSource::Local(&source_storage, source.span)
        }
        _ => return Ok(None),
    };
    let mut callback_shape = None;
    for callback in &callbacks {
        if !module.has_function(callback) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic array_reduce() callback helper can only return declared user functions",
            ));
        }
        let shape = array_reduce_callback_shape(callback, callback_expr.span, module)?;
        if callback_shape.is_some_and(|existing| existing != shape) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic array_reduce() callback helper currently requires callbacks with matching reduce signatures",
            ));
        }
        callback_shape = Some(shape);
    }
    let Some(callback_shape) = callback_shape else {
        return Ok(None);
    };
    if !array_reduce_dynamic_source_supports_shape(&dynamic_source, callback_shape, module) {
        return Ok(None);
    }
    emit_array_reduce_dynamic_source_call(
        dynamic_source,
        callback_expr,
        initial,
        callbacks,
        callback_shape,
        module,
    )
}

fn emit_array_reduce_dynamic_callable_descriptor_call(
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    let [source, callback_expr, initial] = args else {
        return Ok(None);
    };
    let Some(callbacks) = dynamic_array_reduce_callable_descriptor_targets(callback_expr, module) else {
        return Ok(None);
    };
    let source_storage;
    let dynamic_source = match &source.kind {
        ExprKind::ArrayLiteral(items) => ArrayReduceDynamicSource::Literal(items),
        ExprKind::Variable(name)
            if module.local_kind(name) == Some(LocalKind::Array) =>
        {
            ArrayReduceDynamicSource::Local(name, source.span)
        }
        _ if expression_has_array_type(source, module) => {
            source_storage = module
                .next_label("array_reduce_callable_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(source_storage.clone());
            emit_array_assign(&source_storage, source, module)?;
            ArrayReduceDynamicSource::Local(&source_storage, source.span)
        }
        _ => return Ok(None),
    };
    let mut callback_shape = None;
    for callback in &callbacks {
        if !module.has_function(callback) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic array_reduce() callable descriptor can only target declared user functions",
            ));
        }
        let shape = array_reduce_callback_shape(callback, callback_expr.span, module)?;
        if callback_shape.is_some_and(|existing| existing != shape) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic array_reduce() callable descriptor currently requires callbacks with matching reduce signatures",
            ));
        }
        callback_shape = Some(shape);
    }
    let Some(callback_shape) = callback_shape else {
        return Ok(None);
    };
    if !array_reduce_dynamic_source_supports_shape(&dynamic_source, callback_shape, module) {
        return Ok(None);
    }
    emit_array_reduce_dynamic_descriptor_source_call(
        dynamic_source,
        callback_expr,
        initial,
        callbacks,
        callback_shape,
        module,
    )
}

fn dynamic_array_reduce_callback_names(expr: &Expr, module: &WasmModule) -> Option<Vec<String>> {
    match &expr.kind {
        ExprKind::FunctionCall { name, .. } => module
            .function_possible_static_string_returns(name.as_str())
            .map(<[_]>::to_vec),
        ExprKind::Variable(name) => module.possible_static_string_values(name).map(<[_]>::to_vec),
        _ => None,
    }
}

fn dynamic_array_reduce_callable_descriptor_targets(
    expr: &Expr,
    module: &WasmModule,
) -> Option<Vec<String>> {
    if let Some(targets) = callable_return_expr_targets(expr, module) {
        return Some(targets);
    }
    match &expr.kind {
        ExprKind::Variable(name) => module
            .possible_callable_targets(name)
            .map(<[_]>::to_vec)
            .or_else(|| module.callable_target(name).map(|target| vec![target])),
        _ => None,
    }
}

fn emit_array_reduce_dynamic_source_call(
    source: ArrayReduceDynamicSource<'_>,
    callback_expr: &Expr,
    initial: &Expr,
    callbacks: Vec<String>,
    shape: ArrayReduceCallbackShape,
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    let callback_ptr = module.next_label("array_reduce_callback_ptr");
    let callback_len = module.next_label("array_reduce_callback_len");
    let matched = module.next_label("array_reduce_callback_matched");
    for local in [&callback_ptr, &callback_len, &matched] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_string_value_to_stack(callback_expr, module)?;
    module.body().line(&format!("local.set {}", callback_len));
    module.body().line(&format!("local.set {}", callback_ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));

    match shape {
        ArrayReduceCallbackShape::IntToInt => {
            let acc = module
                .next_label("array_reduce_dynamic_acc")
                .trim_start_matches('$')
                .to_string();
            module.declare_i64_local(acc.clone());
            require_int(initial, module)?;
            module.body().line(&format!("local.set ${}", acc));
            emit_array_reduce_dynamic_literal_branches(
                &callbacks,
                &callback_ptr,
                &callback_len,
                &matched,
                module,
                |callback, module| emit_array_reduce_dynamic_int_source(&acc, &source, callback, module),
            )?;
            emit_array_reduce_dynamic_missing_branch(&matched, module);
            module.body().line(&format!("local.get ${}", acc));
            Ok(Some(ValueKind::Int))
        }
        ArrayReduceCallbackShape::FloatToFloat => {
            let acc = module
                .next_label("array_reduce_dynamic_acc")
                .trim_start_matches('$')
                .to_string();
            module.declare_f64_local(acc.clone());
            emit_array_reduce_float_initial(initial, module)?;
            module.body().line(&format!("local.set ${}", acc));
            emit_array_reduce_dynamic_literal_branches(
                &callbacks,
                &callback_ptr,
                &callback_len,
                &matched,
                module,
                |callback, module| emit_array_reduce_dynamic_float_source(&acc, &source, callback, module),
            )?;
            emit_array_reduce_dynamic_missing_branch(&matched, module);
            module.body().line(&format!("local.get ${}", acc));
            Ok(Some(ValueKind::Float))
        }
        ArrayReduceCallbackShape::BoolToBool => {
            let acc = module
                .next_label("array_reduce_dynamic_acc")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(acc.clone());
            emit_condition(initial, module)?;
            module.body().line(&format!("local.set ${}", acc));
            emit_array_reduce_dynamic_literal_branches(
                &callbacks,
                &callback_ptr,
                &callback_len,
                &matched,
                module,
                |callback, module| emit_array_reduce_dynamic_bool_source(&acc, &source, callback, module),
            )?;
            emit_array_reduce_dynamic_missing_branch(&matched, module);
            module.body().line(&format!("local.get ${}", acc));
            Ok(Some(ValueKind::Bool))
        }
        ArrayReduceCallbackShape::StrToStr => {
            let acc_ptr = module
                .next_label("array_reduce_dynamic_acc_ptr")
                .trim_start_matches('$')
                .to_string();
            let acc_len = module
                .next_label("array_reduce_dynamic_acc_len")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(acc_ptr.clone());
            module.declare_i32_local(acc_len.clone());
            emit_string_value_to_stack(initial, module)?;
            module.body().line(&format!("local.set ${}", acc_len));
            module.body().line(&format!("local.set ${}", acc_ptr));
            emit_array_reduce_dynamic_literal_branches(
                &callbacks,
                &callback_ptr,
                &callback_len,
                &matched,
                module,
                |callback, module| {
                    emit_array_reduce_dynamic_string_source(
                        &acc_ptr,
                        &acc_len,
                        &source,
                        callback,
                        module,
                    )
                },
            )?;
            emit_array_reduce_dynamic_missing_branch(&matched, module);
            module.body().line(&format!("local.get ${}", acc_ptr));
            module.body().line(&format!("local.get ${}", acc_len));
            Ok(Some(ValueKind::Str))
        }
        _ => Ok(None),
    }
}

fn emit_array_reduce_dynamic_descriptor_source_call(
    source: ArrayReduceDynamicSource<'_>,
    callback_expr: &Expr,
    initial: &Expr,
    callbacks: Vec<String>,
    shape: ArrayReduceCallbackShape,
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    let callback_id = module.next_label("array_reduce_callable_id");
    let matched = module.next_label("array_reduce_callable_matched");
    for local in [&callback_id, &matched] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_array_reduce_callable_descriptor(callback_expr, module)?;
    module.body().line(&format!("local.set {}", callback_id));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));

    match shape {
        ArrayReduceCallbackShape::IntToInt => {
            let acc = module
                .next_label("array_reduce_callable_acc")
                .trim_start_matches('$')
                .to_string();
            module.declare_i64_local(acc.clone());
            require_int(initial, module)?;
            module.body().line(&format!("local.set ${}", acc));
            emit_array_reduce_dynamic_descriptor_branches(
                &callbacks,
                &callback_id,
                &matched,
                module,
                |callback, module| emit_array_reduce_dynamic_int_source(&acc, &source, callback, module),
            )?;
            emit_array_reduce_dynamic_missing_branch(&matched, module);
            module.body().line(&format!("local.get ${}", acc));
            Ok(Some(ValueKind::Int))
        }
        ArrayReduceCallbackShape::FloatToFloat => {
            let acc = module
                .next_label("array_reduce_callable_acc")
                .trim_start_matches('$')
                .to_string();
            module.declare_f64_local(acc.clone());
            emit_array_reduce_float_initial(initial, module)?;
            module.body().line(&format!("local.set ${}", acc));
            emit_array_reduce_dynamic_descriptor_branches(
                &callbacks,
                &callback_id,
                &matched,
                module,
                |callback, module| emit_array_reduce_dynamic_float_source(&acc, &source, callback, module),
            )?;
            emit_array_reduce_dynamic_missing_branch(&matched, module);
            module.body().line(&format!("local.get ${}", acc));
            Ok(Some(ValueKind::Float))
        }
        ArrayReduceCallbackShape::BoolToBool => {
            let acc = module
                .next_label("array_reduce_callable_acc")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(acc.clone());
            emit_condition(initial, module)?;
            module.body().line(&format!("local.set ${}", acc));
            emit_array_reduce_dynamic_descriptor_branches(
                &callbacks,
                &callback_id,
                &matched,
                module,
                |callback, module| emit_array_reduce_dynamic_bool_source(&acc, &source, callback, module),
            )?;
            emit_array_reduce_dynamic_missing_branch(&matched, module);
            module.body().line(&format!("local.get ${}", acc));
            Ok(Some(ValueKind::Bool))
        }
        ArrayReduceCallbackShape::StrToStr => {
            let acc_ptr = module
                .next_label("array_reduce_callable_acc_ptr")
                .trim_start_matches('$')
                .to_string();
            let acc_len = module
                .next_label("array_reduce_callable_acc_len")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(acc_ptr.clone());
            module.declare_i32_local(acc_len.clone());
            emit_string_value_to_stack(initial, module)?;
            module.body().line(&format!("local.set ${}", acc_len));
            module.body().line(&format!("local.set ${}", acc_ptr));
            emit_array_reduce_dynamic_descriptor_branches(
                &callbacks,
                &callback_id,
                &matched,
                module,
                |callback, module| {
                    emit_array_reduce_dynamic_string_source(
                        &acc_ptr,
                        &acc_len,
                        &source,
                        callback,
                        module,
                    )
                },
            )?;
            emit_array_reduce_dynamic_missing_branch(&matched, module);
            module.body().line(&format!("local.get ${}", acc_ptr));
            module.body().line(&format!("local.get ${}", acc_len));
            Ok(Some(ValueKind::Str))
        }
        _ => Ok(None),
    }
}

fn emit_array_reduce_dynamic_literal_branches<F>(
    callbacks: &[String],
    callback_ptr: &str,
    callback_len: &str,
    matched: &str,
    module: &mut WasmModule,
    mut emit_for_callback: F,
) -> Result<(), CompileError>
where
    F: FnMut(&str, &mut WasmModule) -> Result<(), CompileError>,
{
    for callback in callbacks {
        let candidate_ptr = module.next_label("array_reduce_callback_candidate_ptr");
        let candidate_len = module.next_label("array_reduce_callback_candidate_len");
        let candidate_match = module.next_label("array_reduce_callback_candidate_match");
        for local in [&candidate_ptr, &candidate_len, &candidate_match] {
            module.declare_i32_local(local.trim_start_matches('$').to_string());
        }
        let (ptr, len) = module.intern_string(&callback);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("local.set {}", candidate_ptr));
        module.body().line(&format!("i32.const {}", len));
        module.body().line(&format!("local.set {}", candidate_len));
        emit_string_parts_equal(
            &callback_ptr,
            &callback_len,
            &candidate_ptr,
            &candidate_len,
            &candidate_match,
            module,
        );
        module.body().line(&format!("local.get {}", candidate_match));
        module.body().open("if");
        emit_for_callback(callback, module)?;
        module.body().line("i32.const 1");
        module.body().line(&format!("local.set {}", matched));
        module.body().close("end");
    }
    Ok(())
}

fn emit_array_reduce_dynamic_descriptor_branches<F>(
    callbacks: &[String],
    callback_id: &str,
    matched: &str,
    module: &mut WasmModule,
    mut emit_for_callback: F,
) -> Result<(), CompileError>
where
    F: FnMut(&str, &mut WasmModule) -> Result<(), CompileError>,
{
    for callback in callbacks {
        let target_id = module.callable_target_id(callback);
        module.body().line(&format!("local.get {}", callback_id));
        module.body().line(&format!("i32.const {}", target_id));
        module.body().line("i32.eq");
        module.body().open("if");
        emit_for_callback(callback, module)?;
        module.body().line("i32.const 1");
        module.body().line(&format!("local.set {}", matched));
        module.body().close("end");
    }
    Ok(())
}

fn emit_array_reduce_callable_descriptor(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if callable_return_expr_targets(expr, module).is_some() {
        let kind = emit_expr(expr, module)?;
        if kind != ValueKind::Callable {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web array_reduce() callable descriptor expected a callable value",
            ));
        }
        return Ok(());
    }
    match &expr.kind {
        ExprKind::Variable(name) if module.possible_callable_targets(name).is_some() => {
            module.body().line(&format!("local.get ${}", name));
            Ok(())
        }
        ExprKind::Variable(name) if module.callable_target(name).is_some() => {
            let target = module
                .callable_target(name)
                .expect("callable target was checked above");
            let id = module.callable_target_id(&target);
            module.body().line(&format!("i32.const {}", id));
            Ok(())
        }
        _ => Err(CompileError::new(
            expr.span,
            "wasm32-web array_reduce() callable descriptor requires tracked callable metadata",
        )),
    }
}

fn emit_array_reduce_dynamic_missing_branch(matched: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
}

fn array_reduce_dynamic_source_supports_shape(
    source: &ArrayReduceDynamicSource<'_>,
    shape: ArrayReduceCallbackShape,
    module: &WasmModule,
) -> bool {
    let ArrayReduceDynamicSource::Literal(items) = source else {
        return array_reduce_dynamic_local_supports_shape(source, shape, module);
    };
    match shape {
        ArrayReduceCallbackShape::IntToInt => {
            !array_literal_needs_value_cells(items)
                || array_map_items_are_ints(items, module)
                || array_filter_items_are_bools(items, module)
                || array_map_items_are_strings(items, module)
        }
        ArrayReduceCallbackShape::FloatToFloat => {
            array_filter_items_are_floats(items, module)
                || array_map_items_are_ints(items, module)
                || array_filter_items_are_bools(items, module)
                || array_reduce_float_callback_items_are_supported(items, module)
        }
        ArrayReduceCallbackShape::BoolToBool => {
            array_filter_items_are_bools(items, module)
                || array_map_items_are_ints(items, module)
                || array_filter_items_are_floats(items, module)
                || array_map_items_are_strings(items, module)
                || array_reduce_bool_callback_items_are_supported(items, module)
        }
        ArrayReduceCallbackShape::StrToStr => {
            array_map_items_are_strings(items, module)
                || array_reduce_string_callback_items_are_supported(items, module)
        }
        _ => false,
    }
}

fn array_reduce_dynamic_local_supports_shape(
    source: &ArrayReduceDynamicSource<'_>,
    shape: ArrayReduceCallbackShape,
    module: &WasmModule,
) -> bool {
    let ArrayReduceDynamicSource::Local(source, _) = source else {
        return false;
    };
    if module.local_kind(source) != Some(LocalKind::Array) {
        return false;
    }
    match shape {
        ArrayReduceCallbackShape::IntToInt => {
            module.array_layout(source) == ArrayLayout::CompactInt
                || (module.array_layout(source) == ArrayLayout::Value
                    && (array_map_value_cells_are_ints(source, module)
                        || array_filter_value_cells_are_bools(source, module)
                        || array_map_value_cells_are_strings(source, module)))
                || (module.array_layout(source) == ArrayLayout::Assoc
                    && (array_filter_assoc_local_matches_shape(
                        source,
                        ArrayFilterCallbackShape::Int,
                        module,
                    ) || array_filter_assoc_local_matches_shape(
                        source,
                        ArrayFilterCallbackShape::Bool,
                        module,
                    ) || module.array_value_cell_kinds(source).is_some_and(|kinds| {
                        kinds.iter().all(|kind| *kind == ValueCellKind::Str)
                    }) || matches!(
                        module.array_runtime_value_cell_kind(source),
                        Some(ValueCellKind::Int | ValueCellKind::Bool | ValueCellKind::Str)
                    )))
        }
        ArrayReduceCallbackShape::FloatToFloat => {
            module.array_layout(source) == ArrayLayout::CompactInt
                || (module.array_layout(source) == ArrayLayout::Value
                    && (array_filter_value_cells_are_floats(source, module)
                        || array_reduce_float_callback_value_cells_are_supported(source, module)))
                || (module.array_layout(source) == ArrayLayout::Assoc
                    && (array_filter_assoc_local_matches_shape(
                        source,
                        ArrayFilterCallbackShape::Float,
                        module,
                    ) || array_reduce_float_callback_assoc_local_is_supported(source, module)
                        || matches!(
                            module.array_runtime_value_cell_kind(source),
                            Some(ValueCellKind::Float | ValueCellKind::Int | ValueCellKind::Bool | ValueCellKind::Str)
                        )))
        }
        ArrayReduceCallbackShape::BoolToBool => {
            module.array_layout(source) == ArrayLayout::CompactInt
                || (module.array_layout(source) == ArrayLayout::Value
                    && (array_filter_value_cells_are_bools(source, module)
                        || array_reduce_bool_callback_value_cells_are_supported(source, module)))
                || (module.array_layout(source) == ArrayLayout::Assoc
                    && (array_filter_assoc_local_matches_shape(
                        source,
                        ArrayFilterCallbackShape::Bool,
                        module,
                    ) || array_reduce_bool_callback_assoc_local_is_supported(source, module)
                        || module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Bool)))
        }
        ArrayReduceCallbackShape::StrToStr => {
            (module.array_layout(source) == ArrayLayout::Value
                && (array_map_value_cells_are_strings(source, module)
                    || array_reduce_string_callback_value_cells_are_supported(source, module)))
                || (module.array_layout(source) == ArrayLayout::Assoc
                    && (array_filter_assoc_local_matches_shape(
                        source,
                        ArrayFilterCallbackShape::Str,
                        module,
                    ) || array_reduce_string_callback_assoc_local_is_supported(source, module)
                        || module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Str)
                        || array_reduce_string_callback_runtime_assoc_local_is_supported(source, module)))
        }
        _ => false,
    }
}

fn emit_array_reduce_dynamic_int_source(
    acc: &str,
    source: &ArrayReduceDynamicSource<'_>,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match source {
        ArrayReduceDynamicSource::Literal(items) => {
            emit_array_reduce_literal_int_shape(acc, items, callback, module)
        }
        ArrayReduceDynamicSource::Local(source, span) => {
            emit_array_reduce_int_local_shape(acc, source, *span, callback, module)
        }
    }
}

fn emit_array_reduce_dynamic_float_source(
    acc: &str,
    source: &ArrayReduceDynamicSource<'_>,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match source {
        ArrayReduceDynamicSource::Literal(items) => {
            emit_array_reduce_literal_float_shape(acc, items, callback, module)
        }
        ArrayReduceDynamicSource::Local(source, span) => {
            emit_array_reduce_float_local_shape(acc, source, *span, callback, module)
        }
    }
}

fn emit_array_reduce_dynamic_bool_source(
    acc: &str,
    source: &ArrayReduceDynamicSource<'_>,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match source {
        ArrayReduceDynamicSource::Literal(items) => {
            emit_array_reduce_literal_bool_shape(acc, items, callback, module)
        }
        ArrayReduceDynamicSource::Local(source, span) => {
            emit_array_reduce_bool_local_shape(acc, source, *span, callback, module)
        }
    }
}

fn emit_array_reduce_dynamic_string_source(
    acc_ptr: &str,
    acc_len: &str,
    source: &ArrayReduceDynamicSource<'_>,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match source {
        ArrayReduceDynamicSource::Literal(items) => {
            emit_array_reduce_literal_string_shape(acc_ptr, acc_len, items, callback, module)
        }
        ArrayReduceDynamicSource::Local(source, span) => {
            emit_array_reduce_string_local_shape(acc_ptr, acc_len, source, *span, callback, module)
        }
    }
}

fn emit_array_reduce_literal_float_shape(
    acc: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if array_filter_items_are_floats(items, module) {
        return emit_array_reduce_literal_floats(acc, items, callback, module);
    }
    if array_map_items_are_ints(items, module) {
        return emit_array_reduce_literal_ints_as_floats(acc, items, callback, module);
    }
    if array_filter_items_are_bools(items, module) {
        return emit_array_reduce_literal_bools_as_floats(acc, items, callback, module);
    }
    emit_array_reduce_literal_int_bool_cells_as_floats(acc, items, callback, module)
}

fn emit_array_reduce_int_local_shape(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) == ArrayLayout::CompactInt {
        return emit_array_reduce_compact_int_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Value && array_map_value_cells_are_ints(source, module) {
        return emit_array_reduce_value_int_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Value && array_filter_value_cells_are_bools(source, module) {
        return emit_array_reduce_value_int_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Value && array_map_value_cells_are_strings(source, module) {
        return emit_array_reduce_value_numeric_string_as_int_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_filter_assoc_local_matches_shape(source, ArrayFilterCallbackShape::Int, module)
    {
        return emit_array_reduce_assoc_int_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_filter_assoc_local_matches_shape(source, ArrayFilterCallbackShape::Bool, module)
    {
        return emit_array_reduce_assoc_int_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && module.array_value_cell_kinds(source).is_some_and(|kinds| {
            kinds.iter().all(|kind| *kind == ValueCellKind::Str)
        })
    {
        return emit_array_reduce_assoc_int_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && matches!(
            module.array_runtime_value_cell_kind(source),
            Some(ValueCellKind::Int | ValueCellKind::Bool | ValueCellKind::Str)
        )
    {
        return emit_array_reduce_runtime_assoc_int_local(acc, source, source_span, callback, module);
    }
    Err(CompileError::new(
        source_span,
        "wasm32-web dynamic array_reduce() callback helper currently requires callbacks matching the source array values",
    ))
}

fn emit_array_reduce_float_local_shape(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) == ArrayLayout::Value && array_filter_value_cells_are_floats(source, module) {
        return emit_array_reduce_value_float_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::CompactInt {
        return emit_array_reduce_compact_int_as_float_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Value
        && array_reduce_float_callback_value_cells_are_supported(source, module)
    {
        return emit_array_reduce_value_int_as_float_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_filter_assoc_local_matches_shape(source, ArrayFilterCallbackShape::Float, module)
    {
        return emit_array_reduce_assoc_float_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_reduce_float_callback_assoc_local_is_supported(source, module)
    {
        return emit_array_reduce_assoc_int_as_float_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Float)
    {
        return emit_array_reduce_runtime_assoc_float_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && matches!(
            module.array_runtime_value_cell_kind(source),
            Some(ValueCellKind::Int | ValueCellKind::Bool | ValueCellKind::Str)
        )
    {
        return emit_array_reduce_runtime_assoc_int_as_float_local(acc, source, source_span, callback, module);
    }
    Err(CompileError::new(
        source_span,
        "wasm32-web dynamic array_reduce() callback helper currently requires callbacks matching the source array values",
    ))
}

fn emit_array_reduce_literal_int_shape(
    acc: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if !array_literal_needs_value_cells(items) || array_map_items_are_ints(items, module) {
        return emit_array_reduce_literal_ints(acc, items, callback, module);
    }
    if array_filter_items_are_bools(items, module) {
        return emit_array_reduce_literal_bools_as_ints(acc, items, callback, module);
    }
    emit_array_reduce_literal_numeric_strings_as_ints(acc, items, callback, module)
}

fn emit_array_reduce_bool_local_shape(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) == ArrayLayout::CompactInt {
        return emit_array_reduce_compact_int_as_bool_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Value && array_filter_value_cells_are_bools(source, module) {
        return emit_array_reduce_value_bool_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Value
        && array_reduce_bool_callback_value_cells_are_supported(source, module)
    {
        return emit_array_reduce_value_truthy_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_filter_assoc_local_matches_shape(source, ArrayFilterCallbackShape::Bool, module)
    {
        return emit_array_reduce_assoc_bool_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_reduce_bool_callback_assoc_local_is_supported(source, module)
    {
        return emit_array_reduce_assoc_truthy_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Bool)
    {
        return emit_array_reduce_runtime_assoc_bool_local(acc, source, source_span, callback, module);
    }
    Err(CompileError::new(
        source_span,
        "wasm32-web dynamic array_reduce() callback helper currently requires callbacks matching the source array values",
    ))
}

fn emit_array_reduce_literal_bool_shape(
    acc: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if array_filter_items_are_bools(items, module) {
        return emit_array_reduce_literal_bools(acc, items, callback, module);
    }
    if array_map_items_are_ints(items, module) {
        return emit_array_reduce_literal_ints_as_bools(acc, items, callback, module);
    }
    if array_filter_items_are_floats(items, module) {
        return emit_array_reduce_literal_floats_as_bools(acc, items, callback, module);
    }
    if array_map_items_are_strings(items, module) {
        return emit_array_reduce_literal_strings_as_bools(acc, items, callback, module);
    }
    emit_array_reduce_literal_truthy_cells(acc, items, callback, module)
}

fn emit_array_reduce_literal_string_shape(
    acc_ptr: &str,
    acc_len: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if array_map_items_are_strings(items, module) {
        return emit_array_reduce_literal_strings(acc_ptr, acc_len, items, callback, module);
    }
    let span = items.first().map_or(crate::span::Span::dummy(), |item| item.span);
    emit_array_reduce_literal_dynamic_strings(acc_ptr, acc_len, items, span, callback, module)
}

fn emit_array_reduce_string_local_shape(
    acc_ptr: &str,
    acc_len: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) == ArrayLayout::Value && array_map_value_cells_are_strings(source, module) {
        return emit_array_reduce_value_string_local(acc_ptr, acc_len, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Value
        && array_reduce_string_callback_value_cells_are_supported(source, module)
    {
        return emit_array_reduce_value_dynamic_string_local(acc_ptr, acc_len, source, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_filter_assoc_local_matches_shape(source, ArrayFilterCallbackShape::Str, module)
    {
        return emit_array_reduce_assoc_string_local(acc_ptr, acc_len, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_reduce_string_callback_assoc_local_is_supported(source, module)
    {
        return emit_array_reduce_assoc_dynamic_string_local(
            acc_ptr,
            acc_len,
            source,
            source_span,
            callback,
            module,
        );
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Str)
    {
        return emit_array_reduce_runtime_assoc_string_local(
            acc_ptr,
            acc_len,
            source,
            source_span,
            callback,
            module,
        );
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_reduce_string_callback_runtime_assoc_local_is_supported(source, module)
    {
        return emit_array_reduce_runtime_assoc_dynamic_string_local(
            acc_ptr,
            acc_len,
            source,
            source_span,
            callback,
            module,
        );
    }
    Err(CompileError::new(
        source_span,
        "wasm32-web dynamic array_reduce() callback helper currently requires callbacks matching the source array values",
    ))
}

fn array_reduce_callback_shape(
    callback: &str,
    span: crate::span::Span,
    module: &WasmModule,
) -> Result<ArrayReduceCallbackShape, CompileError> {
    let Some(param_kinds) = module.function_param_kinds(callback) else {
        return Err(CompileError::new(
            span,
            "wasm32-web array_reduce() callback metadata is missing",
        ));
    };
    match (param_kinds.as_slice(), module.function_return_kind(callback)) {
        ([LocalKind::I64, LocalKind::I64], Some(ValueKind::Int)) => Ok(ArrayReduceCallbackShape::IntToInt),
        ([LocalKind::I64, LocalKind::Object], Some(ValueKind::Int)) => {
            Ok(ArrayReduceCallbackShape::IntObjectToInt)
        }
        ([LocalKind::F64, LocalKind::F64], Some(ValueKind::Float)) => {
            Ok(ArrayReduceCallbackShape::FloatToFloat)
        }
        ([LocalKind::F64, LocalKind::Object], Some(ValueKind::Float)) => {
            Ok(ArrayReduceCallbackShape::FloatObjectToFloat)
        }
        ([LocalKind::I32, LocalKind::I32], Some(ValueKind::Bool)) => {
            Ok(ArrayReduceCallbackShape::BoolToBool)
        }
        ([LocalKind::I32, LocalKind::Object], Some(ValueKind::Bool)) => {
            Ok(ArrayReduceCallbackShape::BoolObjectToBool)
        }
        ([LocalKind::Str, LocalKind::Str], Some(ValueKind::Str)) => Ok(ArrayReduceCallbackShape::StrToStr),
        ([LocalKind::Str, LocalKind::Object], Some(ValueKind::Str)) => {
            Ok(ArrayReduceCallbackShape::StrObjectToStr)
        }
        ([LocalKind::Mixed, LocalKind::Mixed], Some(ValueKind::Int)) => {
            Ok(ArrayReduceCallbackShape::MixedMixedToInt)
        }
        ([LocalKind::Mixed, LocalKind::Mixed], Some(ValueKind::Bool)) => {
            Ok(ArrayReduceCallbackShape::MixedMixedToBool)
        }
        ([LocalKind::Mixed, LocalKind::Mixed], Some(ValueKind::Float)) => {
            Ok(ArrayReduceCallbackShape::MixedMixedToFloat)
        }
        ([LocalKind::Mixed, LocalKind::Mixed], Some(ValueKind::Str)) => {
            Ok(ArrayReduceCallbackShape::MixedMixedToStr)
        }
        _ => Err(CompileError::new(
            span,
            "wasm32-web array_reduce() currently requires homogeneous int, int/object-to-int, float, float/object-to-float, bool, bool/object-to-bool, string, string/object-to-string, mixed,mixed-to-int, mixed,mixed-to-bool, mixed,mixed-to-float, or mixed,mixed-to-string callbacks",
        )),
    }
}

fn emit_array_reduce_mixed_to_int_call(
    _expr: &Expr,
    args: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let acc = module
        .next_label("array_reduce_mixed_acc")
        .trim_start_matches('$')
        .to_string();
    module.declare_i64_local(acc.clone());
    require_int(&args[2], module)?;
    module.body().line(&format!("local.set ${}", acc));

    match &args[0].kind {
        ExprKind::ArrayLiteral(_)
        | ExprKind::ArrayLiteralAssoc(_)
        | ExprKind::FunctionCall { .. }
        | ExprKind::ExprCall { .. }
            if expression_has_array_type(&args[0], module)
                || matches!(args[0].kind, ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)) =>
        {
            let temp = module
                .next_label("array_reduce_mixed_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, &args[0], module)?;
            emit_array_reduce_mixed_to_int_local(&acc, &temp, args[0].span, callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && matches!(
                    module.array_layout(source),
                    ArrayLayout::CompactInt | ArrayLayout::Value | ArrayLayout::Assoc
                ) =>
        {
            emit_array_reduce_mixed_to_int_local(&acc, source, args[0].span, callback, module)?;
        }
        _ => {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_reduce() mixed callbacks currently require compact/value/associative array sources",
            ));
        }
    }

    module.body().line(&format!("local.get ${}", acc));
    Ok(ValueKind::Int)
}

fn emit_array_reduce_mixed_to_int_local(
    acc: &str,
    source: &str,
    _source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let acc_arg = module.next_label("array_reduce_mixed_acc_arg");
    let item_arg = module.next_label("array_reduce_mixed_item_arg");
    let index = module.next_label("array_reduce_mixed_index");
    let source_cell = module.next_label("array_reduce_mixed_source_cell");
    for local in [&acc_arg, &item_arg, &index, &source_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_alloc_mixed_cell(acc_arg.trim_start_matches('$'), module);
    emit_alloc_mixed_cell(item_arg.trim_start_matches('$'), module);
    if let Some(len) = module.array_length(source) {
        for item_index in 0..len {
            emit_array_reduce_mixed_acc_arg(acc, &acc_arg, module);
            module.body().line(&format!("i32.const {}", item_index));
            module.body().line(&format!("local.set {}", index));
            emit_array_map_two_mixed_arg_cell(source, &index, &item_arg, &source_cell, module);
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(callback)));
            module.body().line(&format!("local.set ${}", acc));
        }
    } else {
        let done_label = module.next_label("array_reduce_mixed_done");
        let loop_label = module.next_label("array_reduce_mixed_loop");
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", index));
        module.body().open(&format!("block {}", done_label));
        module.body().open(&format!("loop {}", loop_label));
        module.body().line(&format!("local.get {}", index));
        module.body().line(&format!("local.get ${}_len", source));
        module.body().line("i32.ge_u");
        module.body().line(&format!("br_if {}", done_label));
        emit_array_reduce_mixed_acc_arg(acc, &acc_arg, module);
        emit_array_map_two_mixed_arg_cell(source, &index, &item_arg, &source_cell, module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
        module.body().line(&format!("local.get {}", index));
        module.body().line("i32.const 1");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", index));
        module.body().line(&format!("br {}", loop_label));
        module.body().close("end");
        module.body().close("end");
    }
    module.body().line(&format!("local.get {}", acc_arg));
    module.body().line("call $__rt_value_release");
    module.body().line(&format!("local.get {}", item_arg));
    module.body().line("call $__rt_value_release");
    Ok(())
}

fn emit_array_reduce_mixed_acc_arg(acc: &str, acc_arg: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", acc_arg));
    module.body().line(&format!("local.get ${}", acc));
    module.body().line("call $__rt_value_store_int");
    module.body().line(&format!("local.get {}", acc_arg));
}

fn emit_array_reduce_mixed_to_bool_call(
    _expr: &Expr,
    args: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let acc = module
        .next_label("array_reduce_mixed_bool_acc")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(acc.clone());
    emit_condition(&args[2], module)?;
    module.body().line(&format!("local.set ${}", acc));

    match &args[0].kind {
        ExprKind::ArrayLiteral(_)
        | ExprKind::ArrayLiteralAssoc(_)
        | ExprKind::FunctionCall { .. }
        | ExprKind::ExprCall { .. }
            if expression_has_array_type(&args[0], module)
                || matches!(args[0].kind, ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)) =>
        {
            let temp = module
                .next_label("array_reduce_mixed_bool_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, &args[0], module)?;
            emit_array_reduce_mixed_to_bool_local(&acc, &temp, callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && matches!(
                    module.array_layout(source),
                    ArrayLayout::CompactInt | ArrayLayout::Value | ArrayLayout::Assoc
                ) =>
        {
            emit_array_reduce_mixed_to_bool_local(&acc, source, callback, module)?;
        }
        _ => {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_reduce() mixed bool callbacks currently require compact/value/associative array sources",
            ));
        }
    }

    module.body().line(&format!("local.get ${}", acc));
    Ok(ValueKind::Bool)
}

fn emit_array_reduce_mixed_to_bool_local(
    acc: &str,
    source: &str,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let acc_arg = module.next_label("array_reduce_mixed_bool_acc_arg");
    let item_arg = module.next_label("array_reduce_mixed_bool_item_arg");
    let index = module.next_label("array_reduce_mixed_bool_index");
    let source_cell = module.next_label("array_reduce_mixed_bool_source_cell");
    for local in [&acc_arg, &item_arg, &index, &source_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_alloc_mixed_cell(acc_arg.trim_start_matches('$'), module);
    emit_alloc_mixed_cell(item_arg.trim_start_matches('$'), module);
    if let Some(len) = module.array_length(source) {
        for item_index in 0..len {
            emit_array_reduce_mixed_bool_acc_arg(acc, &acc_arg, module);
            module.body().line(&format!("i32.const {}", item_index));
            module.body().line(&format!("local.set {}", index));
            emit_array_map_two_mixed_arg_cell(source, &index, &item_arg, &source_cell, module);
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(callback)));
            module.body().line(&format!("local.set ${}", acc));
        }
    } else {
        let done_label = module.next_label("array_reduce_mixed_bool_done");
        let loop_label = module.next_label("array_reduce_mixed_bool_loop");
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", index));
        module.body().open(&format!("block {}", done_label));
        module.body().open(&format!("loop {}", loop_label));
        module.body().line(&format!("local.get {}", index));
        module.body().line(&format!("local.get ${}_len", source));
        module.body().line("i32.ge_u");
        module.body().line(&format!("br_if {}", done_label));
        emit_array_reduce_mixed_bool_acc_arg(acc, &acc_arg, module);
        emit_array_map_two_mixed_arg_cell(source, &index, &item_arg, &source_cell, module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
        module.body().line(&format!("local.get {}", index));
        module.body().line("i32.const 1");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", index));
        module.body().line(&format!("br {}", loop_label));
        module.body().close("end");
        module.body().close("end");
    }
    module.body().line(&format!("local.get {}", acc_arg));
    module.body().line("call $__rt_value_release");
    module.body().line(&format!("local.get {}", item_arg));
    module.body().line("call $__rt_value_release");
    Ok(())
}

fn emit_array_reduce_mixed_bool_acc_arg(acc: &str, acc_arg: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", acc_arg));
    module.body().line(&format!("local.get ${}", acc));
    module.body().line("call $__rt_value_store_bool");
    module.body().line(&format!("local.get {}", acc_arg));
}

fn emit_array_reduce_mixed_to_float_call(
    _expr: &Expr,
    args: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let acc = module
        .next_label("array_reduce_mixed_float_acc")
        .trim_start_matches('$')
        .to_string();
    module.declare_f64_local(acc.clone());
    emit_array_reduce_float_initial(&args[2], module)?;
    module.body().line(&format!("local.set ${}", acc));

    match &args[0].kind {
        ExprKind::ArrayLiteral(_)
        | ExprKind::ArrayLiteralAssoc(_)
        | ExprKind::FunctionCall { .. }
        | ExprKind::ExprCall { .. }
            if expression_has_array_type(&args[0], module)
                || matches!(args[0].kind, ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)) =>
        {
            let temp = module
                .next_label("array_reduce_mixed_float_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, &args[0], module)?;
            emit_array_reduce_mixed_to_float_local(&acc, &temp, callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && matches!(
                    module.array_layout(source),
                    ArrayLayout::CompactInt | ArrayLayout::Value | ArrayLayout::Assoc
                ) =>
        {
            emit_array_reduce_mixed_to_float_local(&acc, source, callback, module)?;
        }
        _ => {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_reduce() mixed float callbacks currently require compact/value/associative array sources",
            ));
        }
    }

    module.body().line(&format!("local.get ${}", acc));
    Ok(ValueKind::Float)
}

fn emit_array_reduce_mixed_to_float_local(
    acc: &str,
    source: &str,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let acc_arg = module.next_label("array_reduce_mixed_float_acc_arg");
    let item_arg = module.next_label("array_reduce_mixed_float_item_arg");
    let index = module.next_label("array_reduce_mixed_float_index");
    let source_cell = module.next_label("array_reduce_mixed_float_source_cell");
    for local in [&acc_arg, &item_arg, &index, &source_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_alloc_mixed_cell(acc_arg.trim_start_matches('$'), module);
    emit_alloc_mixed_cell(item_arg.trim_start_matches('$'), module);
    if let Some(len) = module.array_length(source) {
        for item_index in 0..len {
            emit_array_reduce_mixed_float_acc_arg(acc, &acc_arg, module);
            module.body().line(&format!("i32.const {}", item_index));
            module.body().line(&format!("local.set {}", index));
            emit_array_map_two_mixed_arg_cell(source, &index, &item_arg, &source_cell, module);
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(callback)));
            module.body().line(&format!("local.set ${}", acc));
        }
    } else {
        let done_label = module.next_label("array_reduce_mixed_float_done");
        let loop_label = module.next_label("array_reduce_mixed_float_loop");
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", index));
        module.body().open(&format!("block {}", done_label));
        module.body().open(&format!("loop {}", loop_label));
        module.body().line(&format!("local.get {}", index));
        module.body().line(&format!("local.get ${}_len", source));
        module.body().line("i32.ge_u");
        module.body().line(&format!("br_if {}", done_label));
        emit_array_reduce_mixed_float_acc_arg(acc, &acc_arg, module);
        emit_array_map_two_mixed_arg_cell(source, &index, &item_arg, &source_cell, module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
        module.body().line(&format!("local.get {}", index));
        module.body().line("i32.const 1");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", index));
        module.body().line(&format!("br {}", loop_label));
        module.body().close("end");
        module.body().close("end");
    }
    module.body().line(&format!("local.get {}", acc_arg));
    module.body().line("call $__rt_value_release");
    module.body().line(&format!("local.get {}", item_arg));
    module.body().line("call $__rt_value_release");
    Ok(())
}

fn emit_array_reduce_mixed_float_acc_arg(acc: &str, acc_arg: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", acc_arg));
    module.body().line(&format!("local.get ${}", acc));
    module.body().line("call $__rt_value_store_float");
    module.body().line(&format!("local.get {}", acc_arg));
}

fn emit_array_reduce_mixed_to_string_call(
    _expr: &Expr,
    args: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let acc_ptr = module
        .next_label("array_reduce_mixed_acc_ptr")
        .trim_start_matches('$')
        .to_string();
    let acc_len = module
        .next_label("array_reduce_mixed_acc_len")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(acc_ptr.clone());
    module.declare_i32_local(acc_len.clone());
    emit_string_value_to_stack(&args[2], module)?;
    module.body().line(&format!("local.set ${}", acc_len));
    module.body().line(&format!("local.set ${}", acc_ptr));

    match &args[0].kind {
        ExprKind::ArrayLiteral(_)
        | ExprKind::ArrayLiteralAssoc(_)
        | ExprKind::FunctionCall { .. }
        | ExprKind::ExprCall { .. }
            if expression_has_array_type(&args[0], module)
                || matches!(args[0].kind, ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)) =>
        {
            let temp = module
                .next_label("array_reduce_mixed_string_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, &args[0], module)?;
            emit_array_reduce_mixed_to_string_local(
                &acc_ptr,
                &acc_len,
                &temp,
                callback,
                module,
            )?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && matches!(
                    module.array_layout(source),
                    ArrayLayout::CompactInt | ArrayLayout::Value | ArrayLayout::Assoc
                ) =>
        {
            emit_array_reduce_mixed_to_string_local(
                &acc_ptr,
                &acc_len,
                source,
                callback,
                module,
            )?;
        }
        _ => {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_reduce() mixed string callbacks currently require compact/value/associative array sources",
            ));
        }
    }

    module.body().line(&format!("local.get ${}", acc_ptr));
    module.body().line(&format!("local.get ${}", acc_len));
    Ok(ValueKind::Str)
}

fn emit_array_reduce_mixed_to_string_local(
    acc_ptr: &str,
    acc_len: &str,
    source: &str,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let acc_arg = module.next_label("array_reduce_mixed_string_acc_arg");
    let item_arg = module.next_label("array_reduce_mixed_string_item_arg");
    let index = module.next_label("array_reduce_mixed_string_index");
    let source_cell = module.next_label("array_reduce_mixed_string_source_cell");
    for local in [&acc_arg, &item_arg, &index, &source_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_alloc_mixed_cell(acc_arg.trim_start_matches('$'), module);
    emit_alloc_mixed_cell(item_arg.trim_start_matches('$'), module);
    if let Some(len) = module.array_length(source) {
        for item_index in 0..len {
            emit_array_reduce_mixed_string_acc_arg(acc_ptr, acc_len, &acc_arg, module);
            module.body().line(&format!("i32.const {}", item_index));
            module.body().line(&format!("local.set {}", index));
            emit_array_map_two_mixed_arg_cell(source, &index, &item_arg, &source_cell, module);
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(callback)));
            module.body().line(&format!("local.set ${}", acc_len));
            module.body().line(&format!("local.set ${}", acc_ptr));
        }
    } else {
        let done_label = module.next_label("array_reduce_mixed_string_done");
        let loop_label = module.next_label("array_reduce_mixed_string_loop");
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", index));
        module.body().open(&format!("block {}", done_label));
        module.body().open(&format!("loop {}", loop_label));
        module.body().line(&format!("local.get {}", index));
        module.body().line(&format!("local.get ${}_len", source));
        module.body().line("i32.ge_u");
        module.body().line(&format!("br_if {}", done_label));
        emit_array_reduce_mixed_string_acc_arg(acc_ptr, acc_len, &acc_arg, module);
        emit_array_map_two_mixed_arg_cell(source, &index, &item_arg, &source_cell, module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc_len));
        module.body().line(&format!("local.set ${}", acc_ptr));
        module.body().line(&format!("local.get {}", index));
        module.body().line("i32.const 1");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", index));
        module.body().line(&format!("br {}", loop_label));
        module.body().close("end");
        module.body().close("end");
    }
    module.body().line(&format!("local.get {}", acc_arg));
    module.body().line("call $__rt_value_release");
    module.body().line(&format!("local.get {}", item_arg));
    module.body().line("call $__rt_value_release");
    Ok(())
}

fn emit_array_reduce_mixed_string_acc_arg(
    acc_ptr: &str,
    acc_len: &str,
    acc_arg: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", acc_arg));
    module.body().line(&format!("local.get ${}", acc_ptr));
    module.body().line(&format!("local.get ${}", acc_len));
    module.body().line("call $__rt_value_store_string");
    module.body().line(&format!("local.get {}", acc_arg));
}

pub(super) fn array_reduce_call_is_string(args: &[Expr], module: &WasmModule) -> bool {
    if args.len() != 3 {
        return false;
    }
    if let Some(shape) = array_reduce_static_callback_shape_for_expr(&args[1], module) {
        return matches!(
            shape,
            ArrayReduceCallbackShape::StrToStr
                | ArrayReduceCallbackShape::StrObjectToStr
                | ArrayReduceCallbackShape::MixedMixedToStr
        );
    }
    if let ExprKind::Variable(callback_var) = &args[1].kind {
        if let Some((callback, _capture_local)) = module.callable_instance_target(callback_var) {
            return module
                .function_param_kinds(&callback)
                .is_some_and(|kinds| {
                    kinds.as_slice() == [LocalKind::Object, LocalKind::Str, LocalKind::Str]
                        || kinds.as_slice() == [LocalKind::Object, LocalKind::Str, LocalKind::Object]
                })
                && module.function_return_kind(&callback) == Some(ValueKind::Str);
        }
        if let Some((callback, _class_name)) = instance_callback_target(&args[1], "__invoke", module) {
            return module
                .function_param_kinds(&callback)
                .is_some_and(|kinds| {
                    kinds.as_slice() == [LocalKind::Object, LocalKind::Str, LocalKind::Str]
                        || kinds.as_slice() == [LocalKind::Object, LocalKind::Str, LocalKind::Object]
                })
                && module.function_return_kind(&callback) == Some(ValueKind::Str);
        }
    }
    if let Some((callback, _class_name)) = instance_callback_target(&args[1], "__invoke", module) {
        return module
            .function_param_kinds(&callback)
            .is_some_and(|kinds| {
                kinds.as_slice() == [LocalKind::Object, LocalKind::Str, LocalKind::Str]
                    || kinds.as_slice() == [LocalKind::Object, LocalKind::Str, LocalKind::Object]
            })
            && module.function_return_kind(&callback) == Some(ValueKind::Str);
    }
    if let ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) = &args[1].kind {
        let Some(class_name) = object_class_name_for_expr(object, module) else {
            return false;
        };
        let Some((declaring_class, method_info)) = module.object_method_in_hierarchy(&class_name, method) else {
            return false;
        };
        if !module.object_member_is_accessible(&method_info.owner_class, &method_info.visibility) {
            return false;
        }
        if !declaring_class.eq_ignore_ascii_case(&class_name)
            && method_body_uses_this_property(&method_info.body)
            && !inherited_property_layout_available(&class_name, &declaring_class, module)
        {
            return false;
        }
        return module
            .function_param_kinds(&method_info.symbol)
            .is_some_and(|kinds| {
                kinds.as_slice() == [LocalKind::Object, LocalKind::Str, LocalKind::Str]
                    || kinds.as_slice() == [LocalKind::Object, LocalKind::Str, LocalKind::Object]
            })
            && module.function_return_kind(&method_info.symbol) == Some(ValueKind::Str);
    }
    let ExprKind::FunctionCall { name, .. } = &args[1].kind else {
        return false;
    };
    let dynamic_source = match &args[0].kind {
        ExprKind::ArrayLiteral(items) => Some(ArrayReduceDynamicSource::Literal(items)),
        ExprKind::Variable(name)
            if module.local_kind(name) == Some(LocalKind::Array) =>
        {
            Some(ArrayReduceDynamicSource::Local(name, args[0].span))
        }
        _ => None,
    };
    module
        .function_possible_static_string_returns(name.as_str())
        .is_some_and(|callbacks| {
            callbacks.iter().all(|callback| {
                array_reduce_callback_shape(callback, args[1].span, module)
                    .is_ok_and(|shape| {
                        matches!(
                            shape,
                            ArrayReduceCallbackShape::StrToStr
                                | ArrayReduceCallbackShape::StrObjectToStr
                        )
                    })
            }) && (dynamic_source
                .as_ref()
                .is_some_and(|source| {
                    array_reduce_dynamic_source_supports_shape(
                        source,
                        ArrayReduceCallbackShape::StrToStr,
                        module,
                    )
                })
                || expression_has_array_type(&args[0], module))
        })
}

pub(super) fn array_reduce_static_callback_return_kind(
    args: &[Expr],
    module: &WasmModule,
) -> Option<ValueKind> {
    array_reduce_static_callback_shape_for_expr(args.get(1)?, module).map(array_reduce_shape_value_kind)
}

fn array_reduce_static_callback_shape_for_expr(
    callback_expr: &Expr,
    module: &WasmModule,
) -> Option<ArrayReduceCallbackShape> {
    if let ExprKind::Ternary {
        then_expr,
        else_expr,
        ..
    } = &callback_expr.kind
    {
        let then_callback = static_callback_function_name(then_expr, module)?;
        let else_callback = static_callback_function_name(else_expr, module)?;
        let then_shape = array_reduce_callback_shape(&then_callback, then_expr.span, module).ok()?;
        let else_shape = array_reduce_callback_shape(&else_callback, else_expr.span, module).ok()?;
        return (then_shape == else_shape).then_some(then_shape);
    }
    let callback = static_callback_function_name(callback_expr, module)?;
    array_reduce_callback_shape(&callback, callback_expr.span, module).ok()
}

fn array_reduce_shape_value_kind(shape: ArrayReduceCallbackShape) -> ValueKind {
        match shape {
        ArrayReduceCallbackShape::IntToInt
        | ArrayReduceCallbackShape::IntObjectToInt
        | ArrayReduceCallbackShape::MixedMixedToInt => ValueKind::Int,
        ArrayReduceCallbackShape::FloatToFloat
        | ArrayReduceCallbackShape::FloatObjectToFloat
        | ArrayReduceCallbackShape::MixedMixedToFloat => ValueKind::Float,
        ArrayReduceCallbackShape::BoolToBool
        | ArrayReduceCallbackShape::BoolObjectToBool
        | ArrayReduceCallbackShape::MixedMixedToBool => ValueKind::Bool,
        ArrayReduceCallbackShape::StrToStr
        | ArrayReduceCallbackShape::StrObjectToStr
        | ArrayReduceCallbackShape::MixedMixedToStr => ValueKind::Str,
    }
}

fn emit_array_reduce_float_object_call(
    _expr: &Expr,
    args: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let acc = module
        .next_label("array_reduce_object_acc")
        .trim_start_matches('$')
        .to_string();
    module.declare_f64_local(acc.clone());
    emit_array_reduce_float_initial(&args[2], module)?;
    module.body().line(&format!("local.set ${}", acc));

    match &args[0].kind {
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_reduce_object_value_cells_are_supported(source, module) =>
        {
            emit_array_reduce_value_object_float_local(&acc, source, args[0].span, callback, module)?;
        }
        _ if expression_has_array_type(&args[0], module) => {
            let source = module
                .next_label("array_reduce_object_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(source.clone());
            emit_array_assign(&source, &args[0], module)?;
            if !array_reduce_object_value_cells_are_supported(&source, module) {
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web array_reduce() object callbacks require exact object value-cell metadata",
                ));
            }
            emit_array_reduce_value_object_float_local(&acc, &source, args[0].span, callback, module)?;
        }
        _ => {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_reduce() object callbacks require object value-cell arrays",
            ));
        }
    }

    module.body().line(&format!("local.get ${}", acc));
    Ok(ValueKind::Float)
}

fn emit_array_reduce_bool_object_call(
    _expr: &Expr,
    args: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let acc = module
        .next_label("array_reduce_object_acc")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(acc.clone());
    emit_condition(&args[2], module)?;
    module.body().line(&format!("local.set ${}", acc));

    match &args[0].kind {
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_reduce_object_value_cells_are_supported(source, module) =>
        {
            emit_array_reduce_value_object_bool_local(&acc, source, args[0].span, callback, module)?;
        }
        _ if expression_has_array_type(&args[0], module) => {
            let source = module
                .next_label("array_reduce_object_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(source.clone());
            emit_array_assign(&source, &args[0], module)?;
            if !array_reduce_object_value_cells_are_supported(&source, module) {
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web array_reduce() object callbacks require exact object value-cell metadata",
                ));
            }
            emit_array_reduce_value_object_bool_local(&acc, &source, args[0].span, callback, module)?;
        }
        _ => {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_reduce() object callbacks require object value-cell arrays",
            ));
        }
    }

    module.body().line(&format!("local.get ${}", acc));
    Ok(ValueKind::Bool)
}

fn emit_array_reduce_int_object_call(
    _expr: &Expr,
    args: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let acc = module
        .next_label("array_reduce_object_acc")
        .trim_start_matches('$')
        .to_string();
    module.declare_i64_local(acc.clone());
    require_int(&args[2], module)?;
    module.body().line(&format!("local.set ${}", acc));

    match &args[0].kind {
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_reduce_object_value_cells_are_supported(source, module) =>
        {
            emit_array_reduce_value_object_int_local(&acc, source, args[0].span, callback, module)?;
        }
        _ if expression_has_array_type(&args[0], module) => {
            let source = module
                .next_label("array_reduce_object_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(source.clone());
            emit_array_assign(&source, &args[0], module)?;
            if !array_reduce_object_value_cells_are_supported(&source, module) {
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web array_reduce() object callbacks require exact object value-cell metadata",
                ));
            }
            emit_array_reduce_value_object_int_local(&acc, &source, args[0].span, callback, module)?;
        }
        _ => {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_reduce() object callbacks require object value-cell arrays",
            ));
        }
    }

    module.body().line(&format!("local.get ${}", acc));
    Ok(ValueKind::Int)
}

fn emit_array_reduce_string_object_call(
    _expr: &Expr,
    args: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let acc_ptr = module
        .next_label("array_reduce_object_acc_ptr")
        .trim_start_matches('$')
        .to_string();
    let acc_len = module
        .next_label("array_reduce_object_acc_len")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(acc_ptr.clone());
    module.declare_i32_local(acc_len.clone());
    emit_string_value_to_stack(&args[2], module)?;
    module.body().line(&format!("local.set ${}", acc_len));
    module.body().line(&format!("local.set ${}", acc_ptr));

    match &args[0].kind {
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_reduce_object_value_cells_are_supported(source, module) =>
        {
            emit_array_reduce_value_object_string_local(
                &acc_ptr,
                &acc_len,
                source,
                args[0].span,
                callback,
                module,
            )?;
        }
        _ if expression_has_array_type(&args[0], module) => {
            let source = module
                .next_label("array_reduce_object_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(source.clone());
            emit_array_assign(&source, &args[0], module)?;
            if !array_reduce_object_value_cells_are_supported(&source, module) {
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web array_reduce() object callbacks require exact object value-cell metadata",
                ));
            }
            emit_array_reduce_value_object_string_local(
                &acc_ptr,
                &acc_len,
                &source,
                args[0].span,
                callback,
                module,
            )?;
        }
        _ => {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_reduce() object callbacks require object value-cell arrays",
            ));
        }
    }

    module.body().line(&format!("local.get ${}", acc_ptr));
    module.body().line(&format!("local.get ${}", acc_len));
    Ok(ValueKind::Str)
}

fn array_reduce_object_value_cells_are_supported(source: &str, module: &WasmModule) -> bool {
    module.array_layout(source) == ArrayLayout::Value
        && module
            .array_object_classes(source)
            .is_some_and(|classes| !classes.is_empty() && classes.iter().all(Option::is_some))
}

fn emit_array_reduce_string_call(
    _expr: &Expr,
    args: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let acc_ptr = module
        .next_label("array_reduce_acc_ptr")
        .trim_start_matches('$')
        .to_string();
    let acc_len = module
        .next_label("array_reduce_acc_len")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(acc_ptr.clone());
    module.declare_i32_local(acc_len.clone());
    emit_string_value_to_stack(&args[2], module)?;
    module.body().line(&format!("local.set ${}", acc_len));
    module.body().line(&format!("local.set ${}", acc_ptr));

    match &args[0].kind {
        ExprKind::ArrayLiteral(items) if array_map_items_are_strings(items, module) => {
            emit_array_reduce_literal_strings(&acc_ptr, &acc_len, items, callback, module)?;
        }
        ExprKind::ArrayLiteral(items) if array_reduce_string_callback_items_are_supported(items, module) => {
            emit_array_reduce_literal_dynamic_strings(
                &acc_ptr,
                &acc_len,
                items,
                args[0].span,
                callback,
                module,
            )?;
        }
        ExprKind::ArrayLiteralAssoc(items) if array_filter_assoc_items_match_shape(
            items,
            ArrayFilterCallbackShape::Str,
            module,
        ) =>
        {
            emit_array_reduce_assoc_literal_strings(&acc_ptr, &acc_len, items, args[0].span, callback, module)?;
        }
        ExprKind::ArrayLiteralAssoc(items)
            if array_reduce_string_callback_assoc_items_are_supported(items, module) =>
        {
            emit_array_reduce_assoc_literal_dynamic_strings(
                &acc_ptr,
                &acc_len,
                items,
                args[0].span,
                callback,
                module,
            )?;
        }
        ExprKind::FunctionCall { .. } | ExprKind::ExprCall { .. }
            if expression_has_array_type(&args[0], module) =>
        {
            emit_array_reduce_string_array_expr(&acc_ptr, &acc_len, &args[0], callback, module)?;
        }
        ExprKind::MethodCall { object, method, .. }
        | ExprKind::NullsafeMethodCall { object, method, .. }
            if method_call_array_return_metadata(object, method, module).is_some() =>
        {
            let temp = materialize_method_array_reduce_source(
                &args[0],
                object,
                method,
                "array_reduce_method_source",
                module,
            )?;
            emit_array_reduce_string_staged_local(
                &acc_ptr,
                &acc_len,
                &temp,
                args[0].span,
                callback,
                module,
            )?;
        }
        ExprKind::StaticMethodCall { receiver, method, .. }
            if static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            let temp = materialize_static_method_array_reduce_source(
                &args[0],
                receiver,
                method,
                "array_reduce_static_method_source",
                module,
            )?;
            emit_array_reduce_string_staged_local(
                &acc_ptr,
                &acc_len,
                &temp,
                args[0].span,
                callback,
                module,
            )?;
        }
        ExprKind::DynamicStaticMethodCall { receiver, method, .. }
            if dynamic_static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            let temp = materialize_dynamic_static_method_array_reduce_source(
                &args[0],
                receiver,
                method,
                "array_reduce_dynamic_static_method_source",
                module,
            )?;
            emit_array_reduce_string_staged_local(
                &acc_ptr,
                &acc_len,
                &temp,
                args[0].span,
                callback,
                module,
            )?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_reduce_value_string_local(&acc_ptr, &acc_len, source, args[0].span, callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_reduce_string_callback_value_cells_are_supported(source, module) =>
        {
            emit_array_reduce_value_dynamic_string_local(&acc_ptr, &acc_len, source, callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_filter_assoc_local_matches_shape(
                    source,
                    ArrayFilterCallbackShape::Str,
                    module,
                ) =>
        {
            emit_array_reduce_assoc_string_local(&acc_ptr, &acc_len, source, args[0].span, callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_reduce_string_callback_assoc_local_is_supported(source, module) =>
        {
            emit_array_reduce_assoc_dynamic_string_local(
                &acc_ptr,
                &acc_len,
                source,
                args[0].span,
                callback,
                module,
            )?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Str) =>
        {
            emit_array_reduce_runtime_assoc_string_local(
                &acc_ptr,
                &acc_len,
                source,
                args[0].span,
                callback,
                module,
            )?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_reduce_string_callback_runtime_assoc_local_is_supported(source, module) =>
        {
            emit_array_reduce_runtime_assoc_dynamic_string_local(
                &acc_ptr,
                &acc_len,
                source,
                args[0].span,
                callback,
                module,
            )?;
        }
        _ => {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_reduce() currently supports string value-cell or associative arrays for string callbacks",
            ));
        }
    }

    module.body().line(&format!("local.get ${}", acc_ptr));
    module.body().line(&format!("local.get ${}", acc_len));
    Ok(ValueKind::Str)
}

fn emit_array_reduce_float_call(
    _expr: &Expr,
    args: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let acc = module.next_label("array_reduce_acc").trim_start_matches('$').to_string();
    module.declare_f64_local(acc.clone());
    emit_array_reduce_float_initial(&args[2], module)?;
    module.body().line(&format!("local.set ${}", acc));

    match &args[0].kind {
        ExprKind::ArrayLiteral(items) if array_filter_items_are_floats(items, module) => {
            emit_array_reduce_literal_floats(&acc, items, callback, module)?;
        }
        ExprKind::ArrayLiteral(items) if array_map_items_are_ints(items, module) => {
            emit_array_reduce_literal_ints_as_floats(&acc, items, callback, module)?;
        }
        ExprKind::ArrayLiteral(items) if array_filter_items_are_bools(items, module) => {
            emit_array_reduce_literal_bools_as_floats(&acc, items, callback, module)?;
        }
        ExprKind::ArrayLiteral(items) if array_reduce_float_callback_items_are_supported(items, module) => {
            emit_array_reduce_literal_int_bool_cells_as_floats(&acc, items, callback, module)?;
        }
        ExprKind::ArrayLiteralAssoc(items) if array_filter_assoc_items_match_shape(
            items,
            ArrayFilterCallbackShape::Float,
            module,
        ) =>
        {
            emit_array_reduce_assoc_literal_floats(&acc, items, args[0].span, callback, module)?;
        }
        ExprKind::ArrayLiteralAssoc(items) if array_filter_assoc_items_match_shape(
            items,
            ArrayFilterCallbackShape::Int,
            module,
        ) =>
        {
            emit_array_reduce_assoc_literal_ints_as_floats(&acc, items, args[0].span, callback, module)?;
        }
        ExprKind::ArrayLiteralAssoc(items) if array_filter_assoc_items_match_shape(
            items,
            ArrayFilterCallbackShape::Bool,
            module,
        ) =>
        {
            emit_array_reduce_assoc_literal_ints_as_floats(&acc, items, args[0].span, callback, module)?;
        }
        ExprKind::ArrayLiteralAssoc(items)
            if array_reduce_float_callback_assoc_items_are_supported(items, module) =>
        {
            emit_array_reduce_assoc_literal_ints_as_floats(&acc, items, args[0].span, callback, module)?;
        }
        ExprKind::FunctionCall { .. } | ExprKind::ExprCall { .. }
            if expression_has_array_type(&args[0], module) =>
        {
            emit_array_reduce_float_array_expr(&acc, &args[0], callback, module)?;
        }
        ExprKind::DynamicStaticMethodCall { receiver, method, .. }
            if dynamic_static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            let temp = materialize_dynamic_static_method_array_reduce_source(
                &args[0],
                receiver,
                method,
                "array_reduce_dynamic_static_method_source",
                module,
            )?;
            emit_array_reduce_float_staged_local(&acc, &temp, args[0].span, callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_floats(source, module) =>
        {
            emit_array_reduce_value_float_local(&acc, source, args[0].span, callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::CompactInt =>
        {
            emit_array_reduce_compact_int_as_float_local(&acc, source, args[0].span, callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_reduce_float_callback_value_cells_are_supported(source, module) =>
        {
            emit_array_reduce_value_int_as_float_local(&acc, source, args[0].span, callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_filter_assoc_local_matches_shape(
                    source,
                    ArrayFilterCallbackShape::Float,
                    module,
                ) =>
        {
            emit_array_reduce_assoc_float_local(&acc, source, args[0].span, callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_reduce_float_callback_assoc_local_is_supported(source, module) =>
        {
            emit_array_reduce_assoc_int_as_float_local(&acc, source, args[0].span, callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Float) =>
        {
            emit_array_reduce_runtime_assoc_float_local(&acc, source, args[0].span, callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && matches!(
                    module.array_runtime_value_cell_kind(source),
                    Some(ValueCellKind::Int | ValueCellKind::Bool | ValueCellKind::Str)
                ) =>
        {
            emit_array_reduce_runtime_assoc_int_as_float_local(&acc, source, args[0].span, callback, module)?;
        }
        _ => {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_reduce() currently supports float value-cell or associative arrays for float callbacks",
            ));
        }
    }

    module.body().line(&format!("local.get ${}", acc));
    Ok(ValueKind::Float)
}

fn emit_array_reduce_float_initial(
    initial: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if expression_is_floaty(initial, module) {
        return require_float(initial, module);
    }
    if expression_is_inty(initial, module) {
        require_int(initial, module)?;
        module.body().line("f64.convert_i64_s");
        return Ok(());
    }
    if expression_is_booly(initial, module) {
        emit_condition(initial, module)?;
        module.body().line("f64.convert_i32_s");
        return Ok(());
    }
    if static_string_value(initial, module).is_some() {
        return emit_static_string_numeric_operand(initial, true, module);
    }
    Err(CompileError::new(
        initial.span,
        "wasm32-web array_reduce() float callbacks require a float, integer, bool, or leading-numeric string initial value",
    ))
}

fn emit_array_reduce_bool_call(
    _expr: &Expr,
    args: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let acc = module.next_label("array_reduce_acc").trim_start_matches('$').to_string();
    module.declare_i32_local(acc.clone());
    emit_condition(&args[2], module)?;
    module.body().line(&format!("local.set ${}", acc));

    match &args[0].kind {
        ExprKind::ArrayLiteral(items) if array_filter_items_are_bools(items, module) => {
            emit_array_reduce_literal_bools(&acc, items, callback, module)?;
        }
        ExprKind::ArrayLiteral(items) if array_map_items_are_ints(items, module) => {
            emit_array_reduce_literal_ints_as_bools(&acc, items, callback, module)?;
        }
        ExprKind::ArrayLiteral(items) if array_filter_items_are_floats(items, module) => {
            emit_array_reduce_literal_floats_as_bools(&acc, items, callback, module)?;
        }
        ExprKind::ArrayLiteral(items) if array_map_items_are_strings(items, module) => {
            emit_array_reduce_literal_strings_as_bools(&acc, items, callback, module)?;
        }
        ExprKind::ArrayLiteral(items) if array_reduce_bool_callback_items_are_supported(items, module) => {
            emit_array_reduce_literal_truthy_cells(&acc, items, callback, module)?;
        }
        ExprKind::ArrayLiteralAssoc(items) if array_filter_assoc_items_match_shape(
            items,
            ArrayFilterCallbackShape::Bool,
            module,
        ) =>
        {
            emit_array_reduce_assoc_literal_bools(&acc, items, args[0].span, callback, module)?;
        }
        ExprKind::ArrayLiteralAssoc(items) if array_reduce_bool_callback_assoc_items_are_supported(items, module) => {
            emit_array_reduce_assoc_literal_truthy_cells(&acc, items, args[0].span, callback, module)?;
        }
        ExprKind::FunctionCall { .. } | ExprKind::ExprCall { .. }
            if expression_has_array_type(&args[0], module) =>
        {
            emit_array_reduce_bool_array_expr(&acc, &args[0], callback, module)?;
        }
        ExprKind::DynamicStaticMethodCall { receiver, method, .. }
            if dynamic_static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            let temp = materialize_dynamic_static_method_array_reduce_source(
                &args[0],
                receiver,
                method,
                "array_reduce_dynamic_static_method_source",
                module,
            )?;
            emit_array_reduce_bool_staged_local(&acc, &temp, args[0].span, callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::CompactInt =>
        {
            emit_array_reduce_compact_int_as_bool_local(&acc, source, args[0].span, callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_bools(source, module) =>
        {
            emit_array_reduce_value_bool_local(&acc, source, args[0].span, callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_reduce_bool_callback_value_cells_are_supported(source, module) =>
        {
            emit_array_reduce_value_truthy_local(&acc, source, args[0].span, callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_filter_assoc_local_matches_shape(
                    source,
                    ArrayFilterCallbackShape::Bool,
                    module,
                ) =>
        {
            emit_array_reduce_assoc_bool_local(&acc, source, args[0].span, callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_reduce_bool_callback_assoc_local_is_supported(source, module) =>
        {
            emit_array_reduce_assoc_truthy_local(&acc, source, args[0].span, callback, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Bool) =>
        {
            emit_array_reduce_runtime_assoc_bool_local(&acc, source, args[0].span, callback, module)?;
        }
        _ => {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_reduce() currently supports bool value-cell or associative arrays for bool callbacks",
            ));
        }
    }

    module.body().line(&format!("local.get ${}", acc));
    Ok(ValueKind::Bool)
}

fn emit_array_reduce_int_array_expr(
    acc: &str,
    source_expr: &Expr,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_reduce_int_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_array_assign(&temp, source_expr, module)?;
    emit_array_reduce_int_staged_local(acc, &temp, source_expr.span, callback, module)
}

fn emit_array_reduce_int_staged_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) == ArrayLayout::CompactInt {
        return emit_array_reduce_compact_int_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Value && array_map_value_cells_are_ints(source, module) {
        return emit_array_reduce_value_int_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Value && array_filter_value_cells_are_bools(source, module) {
        return emit_array_reduce_value_int_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Value && array_map_value_cells_are_strings(source, module) {
        return emit_array_reduce_value_numeric_string_as_int_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_filter_assoc_local_matches_shape(source, ArrayFilterCallbackShape::Int, module)
    {
        return emit_array_reduce_assoc_int_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_filter_assoc_local_matches_shape(source, ArrayFilterCallbackShape::Bool, module)
    {
        return emit_array_reduce_assoc_int_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && module
            .array_value_cell_kinds(source)
            .is_some_and(|kinds| kinds.iter().all(|kind| *kind == ValueCellKind::Str))
    {
        return emit_array_reduce_assoc_int_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && matches!(
            module.array_runtime_value_cell_kind(source),
            Some(ValueCellKind::Int | ValueCellKind::Bool | ValueCellKind::Str)
        )
    {
        return emit_array_reduce_runtime_assoc_int_local(acc, source, source_span, callback, module);
    }
    Err(CompileError::new(
        source_span,
        "wasm32-web array_reduce() int callbacks require a compact integer, integer value-cell, or integer associative array source",
    ))
}

fn emit_array_reduce_string_array_expr(
    acc_ptr: &str,
    acc_len: &str,
    source_expr: &Expr,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_reduce_string_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_array_assign(&temp, source_expr, module)?;
    match emit_array_reduce_string_staged_local(acc_ptr, acc_len, &temp, source_expr.span, callback, module) {
        Ok(()) => Ok(()),
        Err(err) if is_direct_array_keys_expr(source_expr) && module.array_layout(&temp) == ArrayLayout::Value => {
            emit_array_reduce_value_dynamic_string_local(acc_ptr, acc_len, &temp, callback, module).map_err(|_| err)
        }
        Err(err) => Err(err),
    }
}

fn emit_array_reduce_string_staged_local(
    acc_ptr: &str,
    acc_len: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) == ArrayLayout::Value && array_map_value_cells_are_strings(source, module) {
        return emit_array_reduce_value_string_local(
            acc_ptr,
            acc_len,
            source,
            source_span,
            callback,
            module,
        );
    }
    if module.array_layout(source) == ArrayLayout::Value
        && array_reduce_string_callback_value_cells_are_supported(source, module)
    {
        return emit_array_reduce_value_dynamic_string_local(acc_ptr, acc_len, source, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_filter_assoc_local_matches_shape(source, ArrayFilterCallbackShape::Str, module)
    {
        return emit_array_reduce_assoc_string_local(
            acc_ptr,
            acc_len,
            source,
            source_span,
            callback,
            module,
        );
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_reduce_string_callback_assoc_local_is_supported(source, module)
    {
        return emit_array_reduce_assoc_dynamic_string_local(
            acc_ptr,
            acc_len,
            source,
            source_span,
            callback,
            module,
        );
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Str)
    {
        return emit_array_reduce_runtime_assoc_string_local(
            acc_ptr,
            acc_len,
            source,
            source_span,
            callback,
            module,
        );
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_reduce_string_callback_runtime_assoc_local_is_supported(source, module)
    {
        return emit_array_reduce_runtime_assoc_dynamic_string_local(
            acc_ptr,
            acc_len,
            source,
            source_span,
            callback,
            module,
        );
    }
    Err(CompileError::new(
        source_span,
        "wasm32-web array_reduce() string callbacks require a string value-cell or associative array source",
    ))
}

fn materialize_method_array_reduce_source(
    source: &Expr,
    object: &Expr,
    method: &str,
    label: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let metadata = method_call_array_return_metadata(object, method, module).ok_or_else(|| {
        CompileError::new(
            source.span,
            "wasm32-web array_reduce() requires method array return metadata",
        )
    })?;
    materialize_array_reduce_source_with_metadata(source, label, metadata, module)
}

fn materialize_static_method_array_reduce_source(
    source: &Expr,
    receiver: &StaticReceiver,
    method: &str,
    label: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let metadata = static_method_call_array_return_metadata(receiver, method, module).ok_or_else(|| {
        CompileError::new(
            source.span,
            "wasm32-web array_reduce() requires static method array return metadata",
        )
    })?;
    materialize_array_reduce_source_with_metadata(source, label, metadata, module)
}

fn materialize_dynamic_static_method_array_reduce_source(
    source: &Expr,
    receiver: &StaticReceiver,
    method: &Expr,
    label: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let metadata = dynamic_static_method_call_array_return_metadata(receiver, method, module).ok_or_else(|| {
        CompileError::new(
            source.span,
            "wasm32-web array_reduce() requires dynamic static method array return metadata",
        )
    })?;
    materialize_array_reduce_source_with_metadata(source, label, metadata, module)
}

fn materialize_array_reduce_source_with_metadata(
    source: &Expr,
    label: &str,
    metadata: MethodArrayReturnMetadata,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let temp = module.next_label(label).trim_start_matches('$').to_string();
    module.declare_array_local(temp.clone());
    match emit_expr(source, module)? {
        ValueKind::Array => {
            module.body().line(&format!("local.set ${}_len", temp));
            module.body().line(&format!("local.set ${}_ptr", temp));
        }
        _ => {
            return Err(CompileError::new(
                source.span,
                "wasm32-web array_reduce() expected an array-returning method value",
            ));
        }
    }
    module.set_array_layout(&temp, metadata.layout);
    if let Some(len) = metadata.len {
        module.set_array_length(&temp, len);
    }
    module.set_array_value_cell_kinds(&temp, metadata.value_kinds);
    module.set_array_value_constants(&temp, metadata.value_constants);
    module.set_array_runtime_value_cell_kind(&temp, metadata.runtime_value_kind);
    module.set_array_nested_value_metadata(&temp, metadata.nested_values);
    module.set_array_key_kinds(&temp, metadata.key_kinds);
    module.set_array_key_values(&temp, metadata.key_values);
    Ok(temp)
}

fn is_direct_array_keys_expr(expr: &Expr) -> bool {
    matches!(
        &expr.kind,
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("array_keys")
    )
}

fn emit_array_reduce_bool_array_expr(
    acc: &str,
    source_expr: &Expr,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_reduce_bool_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_array_assign(&temp, source_expr, module)?;
    emit_array_reduce_bool_staged_local(acc, &temp, source_expr.span, callback, module)
}

fn emit_array_reduce_bool_staged_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) == ArrayLayout::CompactInt {
        return emit_array_reduce_compact_int_as_bool_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Value && array_filter_value_cells_are_bools(source, module) {
        return emit_array_reduce_value_bool_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Value
        && array_reduce_bool_callback_value_cells_are_supported(source, module)
    {
        return emit_array_reduce_value_truthy_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_filter_assoc_local_matches_shape(source, ArrayFilterCallbackShape::Bool, module)
    {
        return emit_array_reduce_assoc_bool_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_reduce_bool_callback_assoc_local_is_supported(source, module)
    {
        return emit_array_reduce_assoc_truthy_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Bool)
    {
        return emit_array_reduce_runtime_assoc_bool_local(acc, source, source_span, callback, module);
    }
    Err(CompileError::new(
        source_span,
        "wasm32-web array_reduce() bool callbacks require a bool value-cell or associative array source",
    ))
}

fn emit_array_reduce_float_array_expr(
    acc: &str,
    source_expr: &Expr,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_reduce_float_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_array_assign(&temp, source_expr, module)?;
    emit_array_reduce_float_staged_local(acc, &temp, source_expr.span, callback, module)
}

fn emit_array_reduce_float_staged_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) == ArrayLayout::CompactInt {
        return emit_array_reduce_compact_int_as_float_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Value && array_filter_value_cells_are_floats(source, module) {
        return emit_array_reduce_value_float_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Value
        && array_reduce_float_callback_value_cells_are_supported(source, module)
    {
        return emit_array_reduce_value_int_as_float_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_filter_assoc_local_matches_shape(source, ArrayFilterCallbackShape::Float, module)
    {
        return emit_array_reduce_assoc_float_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_reduce_float_callback_assoc_local_is_supported(source, module)
    {
        return emit_array_reduce_assoc_int_as_float_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Float)
    {
        return emit_array_reduce_runtime_assoc_float_local(acc, source, source_span, callback, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && matches!(
            module.array_runtime_value_cell_kind(source),
            Some(ValueCellKind::Int | ValueCellKind::Bool | ValueCellKind::Str)
        )
    {
        return emit_array_reduce_runtime_assoc_int_as_float_local(acc, source, source_span, callback, module);
    }
    Err(CompileError::new(
        source_span,
        "wasm32-web array_reduce() float callbacks require a float value-cell or associative array source",
    ))
}

fn emit_array_reduce_literal_strings(
    acc_ptr: &str,
    acc_len: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    for item in items {
        module.body().line(&format!("local.get ${}", acc_ptr));
        module.body().line(&format!("local.get ${}", acc_len));
        emit_string_value_to_stack(item, module)?;
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc_len));
        module.body().line(&format!("local.set ${}", acc_ptr));
    }
    Ok(())
}

fn emit_array_reduce_literal_dynamic_strings(
    acc_ptr: &str,
    acc_len: &str,
    items: &[Expr],
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_reduce_string_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_value_array_items_assign(&temp, items, module)?;
    emit_array_reduce_value_dynamic_string_local(acc_ptr, acc_len, &temp, callback, module).map_err(|_| {
        CompileError::new(
            source_span,
            "wasm32-web array_reduce() string callbacks require scalar string-coercible values",
        )
    })
}

fn emit_array_reduce_literal_floats(
    acc: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    for item in items {
        module.body().line(&format!("local.get ${}", acc));
        require_float(item, module)?;
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

fn emit_array_reduce_literal_ints_as_floats(
    acc: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    for item in items {
        module.body().line(&format!("local.get ${}", acc));
        require_int(item, module)?;
        module.body().line("f64.convert_i64_s");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

fn emit_array_reduce_literal_bools_as_floats(
    acc: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    for item in items {
        module.body().line(&format!("local.get ${}", acc));
        emit_condition(item, module)?;
        module.body().line("f64.convert_i32_u");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

fn emit_array_reduce_literal_int_bool_cells_as_floats(
    acc: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_reduce_float_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_value_array_items_assign(&temp, items, module)?;
    emit_array_reduce_value_int_as_float_local(acc, &temp, items.first().map_or(crate::span::Span::dummy(), |item| item.span), callback, module)
}

fn emit_array_reduce_literal_bools(
    acc: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    for item in items {
        module.body().line(&format!("local.get ${}", acc));
        emit_condition(item, module)?;
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

fn emit_array_reduce_literal_ints_as_bools(
    acc: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    for item in items {
        module.body().line(&format!("local.get ${}", acc));
        require_int(item, module)?;
        module.body().line("i64.const 0");
        module.body().line("i64.ne");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

fn emit_array_reduce_literal_floats_as_bools(
    acc: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    for item in items {
        module.body().line(&format!("local.get ${}", acc));
        require_float(item, module)?;
        module.body().line("f64.const 0");
        module.body().line("f64.ne");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

fn emit_array_reduce_literal_strings_as_bools(
    acc: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    for item in items {
        let ExprKind::StringLiteral(value) = &item.kind else {
            return Err(CompileError::new(
                item.span,
                "wasm32-web array_reduce() bool callbacks require string literals for direct string arrays",
            ));
        };
        module.body().line(&format!("local.get ${}", acc));
        module
            .body()
            .line(&format!("i32.const {}", i32::from(string_is_truthy(value))));
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

fn emit_array_reduce_literal_truthy_cells(
    acc: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_reduce_bool_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_value_array_items_assign(&temp, items, module)?;
    emit_array_reduce_value_truthy_local(
        acc,
        &temp,
        items.first().map_or(crate::span::Span::dummy(), |item| item.span),
        callback,
        module,
    )
}

fn emit_array_reduce_literal_bools_as_ints(
    acc: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    for item in items {
        module.body().line(&format!("local.get ${}", acc));
        emit_condition(item, module)?;
        module.body().line("i64.extend_i32_u");
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

fn emit_array_reduce_literal_numeric_strings_as_ints(
    acc: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    for item in items {
        module.body().line(&format!("local.get ${}", acc));
        emit_string_value_to_stack(item, module)?;
        emit_stack_string_numeric_int_arg("array_reduce_string_int", module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

fn emit_array_reduce_literal_ints(
    acc: &str,
    items: &[Expr],
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    for item in items {
        module.body().line(&format!("local.get ${}", acc));
        require_int(item, module)?;
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line(&format!("local.set ${}", acc));
    }
    Ok(())
}

fn emit_array_reduce_compact_int_as_bool_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(len) = module.array_length(source) {
        for index in 0..len {
            module.body().line(&format!("local.get ${}", acc));
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index * 8));
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(callback)));
            module.body().line(&format!("local.set ${}", acc));
        }
        return Ok(());
    }
    if module.array_layout(source) != ArrayLayout::CompactInt {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() bool callbacks over integer arrays require compact integer storage",
        ));
    }
    let index = module.next_label("array_reduce_int_bool_index");
    let done_label = module.next_label("array_reduce_int_bool_done");
    let loop_label = module.next_label("array_reduce_int_bool_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}", acc));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set ${}", acc));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_array_reduce_compact_int_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(len) = module.array_length(source) {
        for index in 0..len {
            module.body().line(&format!("local.get ${}", acc));
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index * 8));
            module.body().line("i32.add");
            module.body().line("i64.load");
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(callback)));
            module.body().line(&format!("local.set ${}", acc));
        }
        return Ok(());
    }
    emit_array_reduce_runtime_compact_int_loop(acc, source, source_span, callback, module)
}

fn emit_array_reduce_compact_int_local_instance(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(len) = module.array_length(source) {
        for index in 0..len {
            module.body().line(&format!("local.get ${}", capture_local));
            module.body().line(&format!("local.get ${}", acc));
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index * 8));
            module.body().line("i32.add");
            module.body().line("i64.load");
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(callback)));
            module.body().line(&format!("local.set ${}", acc));
        }
        return Ok(());
    }
    emit_array_reduce_runtime_compact_int_instance_loop(
        acc,
        source,
        source_span,
        callback,
        capture_local,
        module,
    )
}

fn emit_array_reduce_compact_int_as_float_local(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(len) = module.array_length(source) {
        for index in 0..len {
            module.body().line(&format!("local.get ${}", acc));
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index * 8));
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("f64.convert_i64_s");
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(callback)));
            module.body().line(&format!("local.set ${}", acc));
        }
        return Ok(());
    }
    if module.array_layout(source) != ArrayLayout::CompactInt {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() float callbacks over integer arrays require compact integer storage",
        ));
    }
    let index = module.next_label("array_reduce_int_float_index");
    let done_label = module.next_label("array_reduce_int_float_done");
    let loop_label = module.next_label("array_reduce_int_float_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}", acc));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("f64.convert_i64_s");
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set ${}", acc));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_array_reduce_runtime_compact_int_instance_loop(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::CompactInt {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over runtime-length arrays currently requires compact integer storage",
        ));
    }
    let index = module.next_label("array_reduce_instance_index");
    let done_label = module.next_label("array_reduce_instance_done");
    let loop_label = module.next_label("array_reduce_instance_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get ${}", acc));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set ${}", acc));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_array_reduce_runtime_compact_int_loop(
    acc: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::CompactInt {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_reduce() over runtime-length arrays currently requires compact integer storage",
        ));
    }
    let index = module.next_label("array_reduce_index");
    let done_label = module.next_label("array_reduce_done");
    let loop_label = module.next_label("array_reduce_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}", acc));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line(&format!("local.set ${}", acc));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}
