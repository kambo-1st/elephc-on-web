//! Purpose:
//! Lowers array_map assignment forms for the wasm32-web backend.
//! Covers callback dispatch, null-callback zipping, and multi-source materialization.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::emit_array_assign()`
//!
//! Key details:
//! - Result metadata must track value-cell kinds and nested values for downstream array consumers.

use super::*;

enum ArrayMapDynamicSource<'a> {
    Expr(&'a Expr),
    Local(&'a str, crate::span::Span),
}

fn instance_callback_for_array_map(
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
    if let Some((target, class_name)) = instance_callback_target(callback, "__invoke", module) {
        let capture_local = module
            .next_label("array_map_invokable_object")
            .trim_start_matches('$')
            .to_string();
        module.declare_object_local(capture_local.clone());
        let kind = emit_expr(callback, module)?;
        if kind != ValueKind::Object {
            return Err(CompileError::new(
                callback.span,
                "wasm32-web array_map() invokable callback requires an object receiver",
            ));
        }
        module.body().line(&format!("local.set ${}", capture_local));
        module.set_object_class_for_local(&capture_local, Some(class_name));
        return Ok(Some((target, capture_local)));
    }
    if let Some((object, method)) = fixed_instance_callable_array_parts(callback, module) {
        return capture_instance_callback_for_array_map(callback, object, &method, module);
    }
    let ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) = &callback.kind else {
        return Ok(None);
    };
    capture_instance_callback_for_array_map(callback, object, method, module)
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

fn capture_instance_callback_for_array_map(
    callback: &Expr,
    object: &Expr,
    method: &str,
    module: &mut WasmModule,
) -> Result<Option<(String, String)>, CompileError> {
    let Some((target, class_name)) = instance_callback_target(object, method, module) else {
        return Ok(None);
    };
    let capture_local = module
        .next_label("array_map_callable_object")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(capture_local.clone());
    let kind = emit_expr(object, module)?;
    if kind != ValueKind::Object {
        return Err(CompileError::new(
            callback.span,
            "wasm32-web array_map() instance callback requires an object receiver",
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
    let class_name = object_class_name_for_expr(object, module)?;
    let (declaring_class, method_info) = module.object_method_in_hierarchy(&class_name, method)?;
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

pub(in crate::codegen::wasm) fn emit_array_map_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if args.len() > 2 && matches!(args[0].kind, ExprKind::Null) {
        return emit_array_map_null_two_array_assign(name, call, args, module);
    }
    if args.len() == 3 {
        return emit_array_map_two_array_assign(name, call, args, module);
    }
    if args.len() == 4 {
        return emit_array_map_three_array_assign(name, call, args, module);
    }
    if args.len() == 5 {
        return emit_array_map_four_array_assign(name, call, args, module);
    }
    if args.len() == 6 {
        return emit_array_map_five_array_assign(name, call, args, module);
    }
    if args.len() == 7 {
        return emit_array_map_six_array_assign(name, call, args, module);
    }
    if args.len() == 8 {
        return emit_array_map_seven_array_assign(name, call, args, module);
    }
    if args.len() == 9 {
        return emit_array_map_eight_array_assign(name, call, args, module);
    }
    if args.len() == 10 {
        return emit_array_map_nine_array_assign(name, call, args, module);
    }
    if args.len() > 10 {
        return emit_array_map_many_array_assign(name, call, args, module);
    }
    if args.len() != 2 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web array_map() currently supports one, two, three, four, five, selected six, and mixed callbacks for seven or more array arguments",
        ));
    }
    if matches!(args[0].kind, ExprKind::Null) {
        if !expression_has_array_type(&args[1], module) {
            return Err(CompileError::new(
                args[1].span,
                "wasm32-web array_map(null, ...) currently requires an array argument",
            ));
        }
        return emit_array_assign(name, &args[1], module);
    }
    if let Some((callback, capture_local)) = instance_callback_for_array_map(&args[0], module)? {
        let param_kinds = module.function_param_kinds(&callback).ok_or_else(|| {
            CompileError::new(
                args[0].span,
                "wasm32-web array_map() instance callback metadata is missing",
            )
        })?;
        let callback_takes_int_returns_int =
            param_kinds.as_slice() == [LocalKind::Object, LocalKind::I64]
                && module.function_return_kind(&callback) == Some(ValueKind::Int);
        let callback_takes_string_returns_string =
            param_kinds.as_slice() == [LocalKind::Object, LocalKind::Str]
                && module.function_return_kind(&callback) == Some(ValueKind::Str);
        let callback_takes_float_returns_float =
            param_kinds.as_slice() == [LocalKind::Object, LocalKind::F64]
                && module.function_return_kind(&callback) == Some(ValueKind::Float);
        let callback_takes_bool_returns_bool =
            param_kinds.as_slice() == [LocalKind::Object, LocalKind::I32]
                && module.function_return_kind(&callback) == Some(ValueKind::Bool);
        if !callback_takes_int_returns_int
            && !callback_takes_string_returns_string
            && !callback_takes_float_returns_float
            && !callback_takes_bool_returns_bool
        {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_map() instance callbacks currently require one int-to-int, string-to-string, float-to-float, or bool-to-bool callback",
            ));
        }
        if let ExprKind::Variable(source) = &args[1].kind {
            if callback_takes_int_returns_int
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::CompactInt
            {
                return emit_array_map_compact_int_local_instance_assign(
                    name,
                    source,
                    args[1].span,
                    &callback,
                    &capture_local,
                    module,
                );
            }
            if callback_takes_int_returns_int
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_map_assoc_local_matches_shape(source, ArrayMapCallbackShape::IntToInt, module)
            {
                return emit_array_map_assoc_local_instance_assign(
                    name,
                    source,
                    args[1].span,
                    &callback,
                    &capture_local,
                    ArrayMapCallbackShape::IntToInt,
                    module,
                );
            }
            if callback_takes_int_returns_int
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_map_runtime_assoc_local_matches_shape(source, ArrayMapCallbackShape::IntToInt, module)
            {
                return emit_array_map_runtime_assoc_local_instance_assign(
                    name,
                    source,
                    args[1].span,
                    &callback,
                    &capture_local,
                    ArrayMapCallbackShape::IntToInt,
                    module,
                );
            }
            if callback_takes_string_returns_string
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && (module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Str)
                    || module
                        .array_value_cell_kinds(source)
                        .is_some_and(|kinds| kinds.iter().all(|kind| *kind == ValueCellKind::Str)))
            {
                return emit_array_map_value_string_local_instance_assign(
                    name,
                    source,
                    args[1].span,
                    &callback,
                    &capture_local,
                    module,
                );
            }
            if callback_takes_float_returns_float
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && (module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Float)
                    || module.array_value_cell_kinds(source).is_some_and(|kinds| {
                        kinds.iter().all(|kind| *kind == ValueCellKind::Float)
                    }))
            {
                return emit_array_map_value_float_local_instance_assign(
                    name,
                    source,
                    args[1].span,
                    &callback,
                    &capture_local,
                    module,
                );
            }
            if callback_takes_bool_returns_bool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && (module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Bool)
                    || module.array_value_cell_kinds(source).is_some_and(|kinds| {
                        kinds.iter().all(|kind| *kind == ValueCellKind::Bool)
                    }))
            {
                return emit_array_map_value_bool_local_instance_assign(
                    name,
                    source,
                    args[1].span,
                    &callback,
                    &capture_local,
                    module,
                );
            }
            if callback_takes_bool_returns_bool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_map_assoc_local_matches_shape(source, ArrayMapCallbackShape::BoolToBool, module)
            {
                return emit_array_map_assoc_local_instance_assign(
                    name,
                    source,
                    args[1].span,
                    &callback,
                    &capture_local,
                    ArrayMapCallbackShape::BoolToBool,
                    module,
                );
            }
            if callback_takes_bool_returns_bool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_map_runtime_assoc_local_matches_shape(
                    source,
                    ArrayMapCallbackShape::BoolToBool,
                    module,
                )
            {
                return emit_array_map_runtime_assoc_local_instance_assign(
                    name,
                    source,
                    args[1].span,
                    &callback,
                    &capture_local,
                    ArrayMapCallbackShape::BoolToBool,
                    module,
                );
            }
            if callback_takes_float_returns_float
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && (module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Float)
                    || module.array_value_cell_kinds(source).is_some_and(|kinds| {
                        kinds.iter().all(|kind| *kind == ValueCellKind::Float)
                    }))
            {
                return emit_array_map_assoc_float_local_instance_assign(
                    name,
                    source,
                    args[1].span,
                    &callback,
                    &capture_local,
                    module,
                );
            }
            if callback_takes_string_returns_string
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_map_assoc_local_matches_shape(source, ArrayMapCallbackShape::StrToStr, module)
            {
                return emit_array_map_assoc_local_instance_assign(
                    name,
                    source,
                    args[1].span,
                    &callback,
                    &capture_local,
                    ArrayMapCallbackShape::StrToStr,
                    module,
                );
            }
            if callback_takes_string_returns_string
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_map_runtime_assoc_local_matches_shape(
                    source,
                    ArrayMapCallbackShape::StrToStr,
                    module,
                )
            {
                return emit_array_map_runtime_assoc_local_instance_assign(
                    name,
                    source,
                    args[1].span,
                    &callback,
                    &capture_local,
                    ArrayMapCallbackShape::StrToStr,
                    module,
                );
            }
        }
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web array_map() instance callbacks currently require an assigned compact integer, value-cell, or supported associative array",
        ));
    }
    let Some(callback) = evaluated_static_callback_function_name(&args[0], module)? else {
        if emit_array_map_static_callback_ternary_assign(name, args, module)? {
            return Ok(());
        }
        if emit_array_map_dynamic_callable_descriptor_assign(name, args, module)? {
            return Ok(());
        }
        if emit_array_map_dynamic_static_return_callback_assign(name, args, module)? {
            return Ok(());
        }
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() currently requires a static string, direct first-class function callback, or simple callable variable alias",
        ));
    };
    if !module.has_function(&callback) && !array_map_builtin_callback_is_supported(&callback) {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() currently requires a user-defined callback or supported builtin callback",
        ));
    }
    let shape = array_map_callback_shape(&callback, args[0].span, module)?;

    match &args[1].kind {
        ExprKind::ArrayLiteral(items)
            if shape == ArrayMapCallbackShape::IntToInt && !array_literal_needs_value_cells(items) =>
        {
            emit_array_map_literal_ints_assign(name, items, &callback, module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayMapCallbackShape::IntToInt && array_map_items_are_ints(items, module) =>
        {
            emit_array_map_literal_ints_assign(name, items, &callback, module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayMapCallbackShape::IntToInt && array_filter_items_are_bools(items, module) =>
        {
            emit_array_map_literal_bools_as_ints_assign(name, items, &callback, module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayMapCallbackShape::IntToInt && array_filter_items_are_floats(items, module) =>
        {
            emit_array_map_literal_floats_as_ints_assign(name, items, &callback, module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayMapCallbackShape::IntToInt && array_map_items_are_strings(items, module) =>
        {
            emit_array_map_literal_numeric_strings_as_ints_assign(name, items, &callback, module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayMapCallbackShape::IntToBool && array_map_items_are_ints(items, module) =>
        {
            emit_array_map_literal_int_bools_assign(name, items, &callback, module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayMapCallbackShape::NumericToBool && array_map_items_are_ints(items, module) =>
        {
            emit_array_map_literal_int_bools_assign(name, items, &callback, module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayMapCallbackShape::StrToInt
                && array_map_items_are_strings(items, module) =>
        {
            emit_array_map_literal_string_lengths_assign(name, items, module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayMapCallbackShape::StrToInt
                && array_map_strlen_items_are_supported(items, module) =>
        {
            emit_array_map_literal_scalar_lengths_assign(name, items, module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayMapCallbackShape::StrToBool
                && array_map_items_are_strings(items, module) =>
        {
            emit_array_map_literal_string_bools_assign(name, items, &callback, module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayMapCallbackShape::NumericToBool
                && array_map_items_are_strings(items, module) =>
        {
            emit_array_map_literal_string_bools_assign(name, items, &callback, module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayMapCallbackShape::StrToStr
                && array_map_items_are_strings(items, module) =>
        {
            emit_array_map_literal_strings_assign(name, items, &callback, module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayMapCallbackShape::BoolToBool && array_filter_items_are_bools(items, module) =>
        {
            if array_filter_type_predicate_callback(&callback) {
                emit_array_map_literal_scalar_type_bools_assign(name, items.len(), module)
            } else {
                emit_array_map_literal_scalar_predicate_bools_assign(
                    name,
                    items,
                    &callback,
                    ValueCellKind::Bool,
                    module,
                )
            }
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayMapCallbackShape::FloatToBool && array_filter_items_are_floats(items, module) =>
        {
            if array_filter_type_predicate_callback(&callback) {
                emit_array_map_literal_scalar_type_bools_assign(name, items.len(), module)
            } else {
                emit_array_map_literal_scalar_predicate_bools_assign(
                    name,
                    items,
                    &callback,
                    ValueCellKind::Float,
                    module,
                )
            }
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayMapCallbackShape::NumericToBool && array_filter_items_are_floats(items, module) =>
        {
            emit_array_map_literal_scalar_type_bools_assign(name, items.len(), module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayMapCallbackShape::NullToBool && array_filter_items_are_nulls(items, module) =>
        {
            emit_array_map_literal_scalar_type_bools_assign(name, items.len(), module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayMapCallbackShape::ArrayToBool && array_filter_items_are_arrays(items, module) =>
        {
            emit_array_map_literal_scalar_type_bools_assign(name, items.len(), module)
        }
        ExprKind::ArrayLiteralAssoc(items) if array_map_assoc_items_match_shape(items, shape, module) => {
            emit_array_map_assoc_literal_assign(name, items, args[1].span, &callback, shape, module)
        }
        ExprKind::ArrayLiteralAssoc(items)
            if shape == ArrayMapCallbackShape::StrToInt
                && array_map_strlen_assoc_items_are_supported(items, module) =>
        {
            emit_array_map_assoc_strlen_literal_assign(name, items, args[1].span, module)
        }
        ExprKind::ArrayAccess { .. } if nested_array_metadata_for_access_expr(&args[1], module).is_some() => {
            let temp = materialize_nested_array_map_source(&args[1], module)?;
            emit_array_map_staged_assign(name, &temp, args[1].span, &callback, shape, module)
        }
        ExprKind::MethodCall { object, method, .. }
        | ExprKind::NullsafeMethodCall { object, method, .. }
            if method_call_array_return_metadata(object, method, module).is_some() =>
        {
            let temp = materialize_method_array_map_source(&args[1], object, method, module)?;
            emit_array_map_staged_assign(name, &temp, args[1].span, &callback, shape, module)
        }
        ExprKind::StaticMethodCall { receiver, method, .. }
            if static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            let temp = materialize_static_method_array_map_source(&args[1], receiver, method, module)?;
            emit_array_map_staged_assign(name, &temp, args[1].span, &callback, shape, module)
        }
        ExprKind::FunctionCall { .. } if expression_has_array_type(&args[1], module) => {
            emit_array_map_array_expr_assign(name, &args[1], args[1].span, &callback, shape, module)
        }
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args: method_args,
        } if method.eq_ignore_ascii_case("cases")
            && method_args.is_empty()
            && module
                .class_name_for_receiver(receiver)
                .and_then(|class_name| module.enum_case_names(&class_name))
                .is_some() =>
        {
            emit_array_map_array_expr_assign(name, &args[1], args[1].span, &callback, shape, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::IntToInt
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::CompactInt =>
        {
            emit_array_map_compact_int_local_assign(name, source, args[1].span, &callback, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::IntToBool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::CompactInt =>
        {
            emit_array_map_compact_int_bools_local_assign(name, source, args[1].span, &callback, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::NumericToBool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::CompactInt =>
        {
            emit_array_map_compact_int_bools_local_assign(name, source, args[1].span, &callback, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::IntToInt
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_ints(source, module) =>
        {
            emit_array_map_value_int_local_assign(name, source, args[1].span, &callback, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::IntToInt
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_bools(source, module) =>
        {
            emit_array_map_value_int_local_assign(name, source, args[1].span, &callback, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::IntToInt
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_floats(source, module) =>
        {
            emit_array_map_value_floats_as_ints_local_assign(name, source, args[1].span, &callback, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::IntToInt
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_map_value_numeric_strings_as_ints_local_assign(name, source, args[1].span, &callback, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::IntToBool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_object_false_callback(&callback)
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_map_value_object_false_bools_local_assign(name, source, args[1].span, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::IntToBool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_ints(source, module) =>
        {
            emit_array_map_value_int_bools_local_assign(name, source, args[1].span, &callback, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::NumericToBool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_ints(source, module) =>
        {
            emit_array_map_value_int_bools_local_assign(name, source, args[1].span, &callback, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::StrToInt
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::CompactInt =>
        {
            emit_array_map_compact_int_strlen_lengths_assign(name, source, args[1].span, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::StrToInt
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_map_value_string_lengths_assign(name, source, args[1].span, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::StrToInt
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_strlen_value_cells_are_supported(source, module) =>
        {
            emit_array_map_value_scalar_lengths_assign(name, source, args[1].span, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::StrToBool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_object_false_callback(&callback)
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_map_value_object_false_bools_local_assign(name, source, args[1].span, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::StrToBool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_map_value_string_bools_local_assign(name, source, args[1].span, &callback, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::NumericToBool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_map_value_string_bools_local_assign(name, source, args[1].span, &callback, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::StrToStr
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_map_value_string_local_assign(name, source, args[1].span, &callback, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::BoolToBool
                && !array_filter_type_predicate_callback(&callback)
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_bools(source, module) =>
        {
            emit_array_map_value_scalar_predicate_bools_local_assign(
                name,
                source,
                args[1].span,
                &callback,
                ValueCellKind::Bool,
                module,
            )
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::BoolToBool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_object_false_callback(&callback)
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_map_value_object_false_bools_local_assign(name, source, args[1].span, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::BoolToBool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_bools(source, module) =>
        {
            emit_array_map_value_scalar_type_bools_local_assign(name, source, args[1].span, ValueCellKind::Bool, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::FloatToBool
                && !array_filter_type_predicate_callback(&callback)
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_floats(source, module) =>
        {
            emit_array_map_value_scalar_predicate_bools_local_assign(
                name,
                source,
                args[1].span,
                &callback,
                ValueCellKind::Float,
                module,
            )
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::FloatToBool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_object_false_callback(&callback)
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_map_value_object_false_bools_local_assign(name, source, args[1].span, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::FloatToBool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_floats(source, module) =>
        {
            emit_array_map_value_scalar_type_bools_local_assign(name, source, args[1].span, ValueCellKind::Float, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::NumericToBool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_object_false_callback(&callback)
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_map_value_object_false_bools_local_assign(name, source, args[1].span, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::NumericToBool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_floats(source, module) =>
        {
            emit_array_map_value_scalar_type_bools_local_assign(name, source, args[1].span, ValueCellKind::Float, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::NullToBool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_object_false_callback(&callback)
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_map_value_object_false_bools_local_assign(name, source, args[1].span, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::NullToBool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_nulls(source, module) =>
        {
            emit_array_map_value_scalar_type_bools_local_assign(name, source, args[1].span, ValueCellKind::Null, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::ArrayToBool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_object_false_callback(&callback)
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_map_value_object_false_bools_local_assign(name, source, args[1].span, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::ArrayToBool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_arrays(source, module) =>
        {
            emit_array_map_value_scalar_type_bools_local_assign(name, source, args[1].span, ValueCellKind::Array, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::ObjectToBool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_map_value_object_type_bools_local_assign(name, source, args[1].span, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::ObjectToStr
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_map_value_object_class_names_local_assign(name, source, args[1].span, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::ObjectToTypeStr
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_map_value_object_type_names_local_assign(name, source, args[1].span, module)
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_map_assoc_local_matches_shape(source, shape, module) =>
        {
            emit_array_map_assoc_local_assign(name, source, args[1].span, &callback, shape, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::StrToInt
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_map_strlen_assoc_local_is_supported(source, module) =>
        {
            emit_array_map_assoc_strlen_local_assign(name, source, args[1].span, module)
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_map_runtime_assoc_local_matches_shape(source, shape, module) =>
        {
            emit_array_map_runtime_assoc_local_assign(name, source, args[1].span, &callback, shape, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayMapCallbackShape::StrToInt
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_map_strlen_runtime_assoc_local_is_supported(source, module) =>
        {
            emit_array_map_runtime_assoc_strlen_local_assign(name, source, args[1].span, module)
        }
        _ => Err(CompileError::new(
            args[1].span,
            "wasm32-web array_map() currently supports homogeneous int or string arrays",
        )),
    }
}

fn emit_array_map_two_array_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if matches!(args[0].kind, ExprKind::Null) {
        return emit_array_map_null_two_array_assign(name, call, args, module);
    }
    let Some(callback) = evaluated_static_callback_function_name(&args[0], module)? else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() currently requires a static string, direct first-class function callback, or simple callable variable alias",
        ));
    };
    if !module.has_function(&callback) {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() currently requires a user-defined callback",
        ));
    }
    let shape = array_map_callback_shape(&callback, args[0].span, module)?;
    let left = materialize_array_map_multi_source(&args[1], "array_map_left", module)?;
    let right = materialize_array_map_multi_source(&args[2], "array_map_right", module)?;
    match shape {
        ArrayMapCallbackShape::IntIntToInt => {
            emit_array_map_two_compact_int_locals_assign(name, &left, &right, call.span, &callback, module)
        }
        ArrayMapCallbackShape::StrStrToStr => {
            emit_array_map_two_value_string_locals_assign(name, &left, &right, call.span, &callback, module)
        }
        ArrayMapCallbackShape::MixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedToFloat => {
            emit_array_map_two_mixed_locals_assign(name, &left, &right, call.span, &callback, module)
        }
        _ => Err(CompileError::new(
            args[0].span,
            "wasm32-web multi-array array_map() currently requires an int,int-to-int, string,string-to-string, or mixed,mixed-to-int/string/bool/float callback",
        )),
    }
}

fn emit_array_map_three_array_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if matches!(args[0].kind, ExprKind::Null) {
        return emit_array_map_null_two_array_assign(name, call, args, module);
    }
    let Some(callback) = evaluated_static_callback_function_name(&args[0], module)? else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() currently requires a static string, direct first-class function callback, or simple callable variable alias",
        ));
    };
    if !module.has_function(&callback) {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() currently requires a user-defined callback",
        ));
    }
    let shape = array_map_callback_shape(&callback, args[0].span, module)?;
    let first = materialize_array_map_multi_source(&args[1], "array_map_first", module)?;
    let second = materialize_array_map_multi_source(&args[2], "array_map_second", module)?;
    let third = materialize_array_map_multi_source(&args[3], "array_map_third", module)?;
    match shape {
        ArrayMapCallbackShape::IntIntIntToInt => emit_array_map_three_compact_int_locals_assign(
            name, &first, &second, &third, call.span, &callback, module,
        ),
        ArrayMapCallbackShape::StrStrStrToStr => emit_array_map_three_value_string_locals_assign(
            name, &first, &second, &third, call.span, &callback, module,
        ),
        ArrayMapCallbackShape::MixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedToFloat => {
            emit_array_map_three_mixed_locals_assign(name, &first, &second, &third, call.span, &callback, module)
        }
        _ => Err(CompileError::new(
            args[0].span,
            "wasm32-web three-array array_map() currently requires an int,int,int-to-int, string,string,string-to-string, or mixed,mixed,mixed-to-int/string/bool/float callback",
        )),
    }
}

fn emit_array_map_four_array_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if matches!(args[0].kind, ExprKind::Null) {
        return emit_array_map_null_two_array_assign(name, call, args, module);
    }
    let Some(callback) = evaluated_static_callback_function_name(&args[0], module)? else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() currently requires a static string, direct first-class function callback, or simple callable variable alias",
        ));
    };
    if !module.has_function(&callback) {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() currently requires a user-defined callback",
        ));
    }
    let shape = array_map_callback_shape(&callback, args[0].span, module)?;
    let first = materialize_array_map_multi_source(&args[1], "array_map_first", module)?;
    let second = materialize_array_map_multi_source(&args[2], "array_map_second", module)?;
    let third = materialize_array_map_multi_source(&args[3], "array_map_third", module)?;
    let fourth = materialize_array_map_multi_source(&args[4], "array_map_fourth", module)?;
    match shape {
        ArrayMapCallbackShape::IntIntIntIntToInt => emit_array_map_four_compact_int_locals_assign(
            name, &first, &second, &third, &fourth, call.span, &callback, module,
        ),
        ArrayMapCallbackShape::StrStrStrStrToStr => emit_array_map_four_value_string_locals_assign(
            name, &first, &second, &third, &fourth, call.span, &callback, module,
        ),
        ArrayMapCallbackShape::MixedMixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedMixedToFloat => {
            emit_array_map_four_mixed_locals_assign(name, &first, &second, &third, &fourth, call.span, &callback, module)
        }
        _ => Err(CompileError::new(
            args[0].span,
            "wasm32-web four-array array_map() currently requires an int,int,int,int-to-int, string,string,string,string-to-string, or mixed,mixed,mixed,mixed-to-int/string/bool/float callback",
        )),
    }
}

fn emit_array_map_five_array_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if matches!(args[0].kind, ExprKind::Null) {
        return emit_array_map_null_two_array_assign(name, call, args, module);
    }
    let Some(callback) = evaluated_static_callback_function_name(&args[0], module)? else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() currently requires a static string, direct first-class function callback, or simple callable variable alias",
        ));
    };
    if !module.has_function(&callback) {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() currently requires a user-defined callback",
        ));
    }
    let shape = array_map_callback_shape(&callback, args[0].span, module)?;
    let first = materialize_array_map_multi_source(&args[1], "array_map_first", module)?;
    let second = materialize_array_map_multi_source(&args[2], "array_map_second", module)?;
    let third = materialize_array_map_multi_source(&args[3], "array_map_third", module)?;
    let fourth = materialize_array_map_multi_source(&args[4], "array_map_fourth", module)?;
    let fifth = materialize_array_map_multi_source(&args[5], "array_map_fifth", module)?;
    match shape {
        ArrayMapCallbackShape::IntIntIntIntIntToInt => emit_array_map_five_compact_int_locals_assign(
            name, &first, &second, &third, &fourth, &fifth, call.span, &callback, module,
        ),
        ArrayMapCallbackShape::StrStrStrStrStrToStr => emit_array_map_five_value_string_locals_assign(
            name, &first, &second, &third, &fourth, &fifth, call.span, &callback, module,
        ),
        ArrayMapCallbackShape::MixedMixedMixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedToFloat => emit_array_map_five_mixed_locals_assign(
            name, &first, &second, &third, &fourth, &fifth, call.span, &callback, module,
        ),
        _ => Err(CompileError::new(
            args[0].span,
            "wasm32-web five-array array_map() currently requires an int,int,int,int,int-to-int, string,string,string,string,string-to-string, or mixed,mixed,mixed,mixed,mixed-to-int/string/bool/float callback",
        )),
    }
}

fn emit_array_map_six_array_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if matches!(args[0].kind, ExprKind::Null) {
        return emit_array_map_null_two_array_assign(name, call, args, module);
    }
    let Some(callback) = evaluated_static_callback_function_name(&args[0], module)? else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() currently requires a static string, direct first-class function callback, or simple callable variable alias",
        ));
    };
    if !module.has_function(&callback) {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() currently requires a user-defined callback",
        ));
    }
    let shape = array_map_callback_shape(&callback, args[0].span, module)?;
    let first = materialize_array_map_multi_source(&args[1], "array_map_first", module)?;
    let second = materialize_array_map_multi_source(&args[2], "array_map_second", module)?;
    let third = materialize_array_map_multi_source(&args[3], "array_map_third", module)?;
    let fourth = materialize_array_map_multi_source(&args[4], "array_map_fourth", module)?;
    let fifth = materialize_array_map_multi_source(&args[5], "array_map_fifth", module)?;
    let sixth = materialize_array_map_multi_source(&args[6], "array_map_sixth", module)?;
    match shape {
        ArrayMapCallbackShape::IntIntIntIntIntIntToInt => emit_array_map_six_compact_int_locals_assign(
            name, &first, &second, &third, &fourth, &fifth, &sixth, call.span, &callback, module,
        ),
        ArrayMapCallbackShape::StrStrStrStrStrStrToStr => emit_array_map_six_value_string_locals_assign(
            name, &first, &second, &third, &fourth, &fifth, &sixth, call.span, &callback, module,
        ),
        ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToFloat => emit_array_map_six_mixed_locals_assign(
            name, &first, &second, &third, &fourth, &fifth, &sixth, call.span, &callback, module,
        ),
        _ => Err(CompileError::new(
            args[0].span,
            "wasm32-web six-array array_map() currently requires an int,int,int,int,int,int-to-int, string,string,string,string,string,string-to-string, or mixed,mixed,mixed,mixed,mixed,mixed-to-int/string/bool/float callback",
        )),
    }
}

fn emit_array_map_seven_array_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(callback) = evaluated_static_callback_function_name(&args[0], module)? else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() currently requires a static string, direct first-class function callback, or simple callable variable alias",
        ));
    };
    if !module.has_function(&callback) {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() currently requires a user-defined callback",
        ));
    }
    let shape = array_map_callback_shape(&callback, args[0].span, module)?;
    let first = materialize_array_map_multi_source(&args[1], "array_map_first", module)?;
    let second = materialize_array_map_multi_source(&args[2], "array_map_second", module)?;
    let third = materialize_array_map_multi_source(&args[3], "array_map_third", module)?;
    let fourth = materialize_array_map_multi_source(&args[4], "array_map_fourth", module)?;
    let fifth = materialize_array_map_multi_source(&args[5], "array_map_fifth", module)?;
    let sixth = materialize_array_map_multi_source(&args[6], "array_map_sixth", module)?;
    let seventh = materialize_array_map_multi_source(&args[7], "array_map_seventh", module)?;
    match shape {
        ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToFloat => {
            emit_array_map_seven_mixed_locals_assign(
                name,
                &first,
                &second,
                &third,
                &fourth,
                &fifth,
                &sixth,
                &seventh,
                call.span,
                &callback,
                module,
            )
        }
        _ => Err(CompileError::new(
            args[0].span,
            "wasm32-web seven-array array_map() currently requires a mixed,mixed,mixed,mixed,mixed,mixed,mixed-to-int/string/bool/float callback",
        )),
    }
}

fn emit_array_map_eight_array_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(callback) = evaluated_static_callback_function_name(&args[0], module)? else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() currently requires a static string, direct first-class function callback, or simple callable variable alias",
        ));
    };
    if !module.has_function(&callback) {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() currently requires a user-defined callback",
        ));
    }
    let shape = array_map_callback_shape(&callback, args[0].span, module)?;
    let first = materialize_array_map_multi_source(&args[1], "array_map_first", module)?;
    let second = materialize_array_map_multi_source(&args[2], "array_map_second", module)?;
    let third = materialize_array_map_multi_source(&args[3], "array_map_third", module)?;
    let fourth = materialize_array_map_multi_source(&args[4], "array_map_fourth", module)?;
    let fifth = materialize_array_map_multi_source(&args[5], "array_map_fifth", module)?;
    let sixth = materialize_array_map_multi_source(&args[6], "array_map_sixth", module)?;
    let seventh = materialize_array_map_multi_source(&args[7], "array_map_seventh", module)?;
    let eighth = materialize_array_map_multi_source(&args[8], "array_map_eighth", module)?;
    match shape {
        ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToFloat => {
            emit_array_map_eight_mixed_locals_assign(
                name,
                &first,
                &second,
                &third,
                &fourth,
                &fifth,
                &sixth,
                &seventh,
                &eighth,
                call.span,
                &callback,
                module,
            )
        }
        _ => Err(CompileError::new(
            args[0].span,
            "wasm32-web eight-array array_map() currently requires a mixed,mixed,mixed,mixed,mixed,mixed,mixed,mixed-to-int/string/bool/float callback",
        )),
    }
}

fn emit_array_map_nine_array_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(callback) = evaluated_static_callback_function_name(&args[0], module)? else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() currently requires a static string, direct first-class function callback, or simple callable variable alias",
        ));
    };
    if !module.has_function(&callback) {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() currently requires a user-defined callback",
        ));
    }
    let shape = array_map_callback_shape(&callback, args[0].span, module)?;
    let first = materialize_array_map_multi_source(&args[1], "array_map_first", module)?;
    let second = materialize_array_map_multi_source(&args[2], "array_map_second", module)?;
    let third = materialize_array_map_multi_source(&args[3], "array_map_third", module)?;
    let fourth = materialize_array_map_multi_source(&args[4], "array_map_fourth", module)?;
    let fifth = materialize_array_map_multi_source(&args[5], "array_map_fifth", module)?;
    let sixth = materialize_array_map_multi_source(&args[6], "array_map_sixth", module)?;
    let seventh = materialize_array_map_multi_source(&args[7], "array_map_seventh", module)?;
    let eighth = materialize_array_map_multi_source(&args[8], "array_map_eighth", module)?;
    let ninth = materialize_array_map_multi_source(&args[9], "array_map_ninth", module)?;
    match shape {
        ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToFloat => {
            emit_array_map_nine_mixed_locals_assign(
                name,
                &first,
                &second,
                &third,
                &fourth,
                &fifth,
                &sixth,
                &seventh,
                &eighth,
                &ninth,
                call.span,
                &callback,
                module,
            )
        }
        _ => Err(CompileError::new(
            args[0].span,
            "wasm32-web nine-array array_map() currently requires a mixed,mixed,mixed,mixed,mixed,mixed,mixed,mixed,mixed-to-int/string/bool/float callback",
        )),
    }
}

fn emit_array_map_static_callback_ternary_assign(
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let [callback_expr, source_expr] = args else {
        return Ok(false);
    };
    let ExprKind::Ternary {
        condition,
        then_expr,
        else_expr,
    } = &callback_expr.kind
    else {
        return Ok(false);
    };
    let Some(then_callback) = static_callback_function_name(then_expr, module) else {
        return Ok(false);
    };
    let Some(else_callback) = static_callback_function_name(else_expr, module) else {
        return Ok(false);
    };
    if then_callback.eq_ignore_ascii_case(&else_callback) {
        return Ok(false);
    }
    for callback in [&then_callback, &else_callback] {
        if !module.has_function(callback) && !array_map_builtin_callback_is_supported(callback) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web array_map() callable ternary arms currently require user-defined or supported builtin callbacks",
            ));
        }
    }
    let then_shape = array_map_callback_shape(&then_callback, then_expr.span, module)?;
    let else_shape = array_map_callback_shape(&else_callback, else_expr.span, module)?;
    if then_shape != else_shape || !array_map_dynamic_callback_shape_is_supported(then_shape) {
        return Err(CompileError::new(
            callback_expr.span,
            "wasm32-web array_map() callable ternary arms currently require the same supported scalar callback shape",
        ));
    }
    let source = match &source_expr.kind {
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => {
            ArrayMapDynamicSource::Expr(source_expr)
        }
        ExprKind::Variable(source_name)
            if module.local_kind(source_name) == Some(LocalKind::Array) =>
        {
            ArrayMapDynamicSource::Local(source_name, source_expr.span)
        }
        _ => return Ok(false),
    };
    emit_condition(condition, module)?;
    module.body().open("if");
    emit_array_map_dynamic_source_assign(name, &source, &then_callback, then_shape, module)?;
    module.body().line("else");
    emit_array_map_dynamic_source_assign(name, &source, &else_callback, else_shape, module)?;
    module.body().close("end");
    Ok(true)
}

fn emit_array_map_dynamic_static_return_callback_assign(
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let [callback_expr, source] = args else {
        return Ok(false);
    };
    let Some(callbacks) = dynamic_array_map_callback_names(callback_expr, module) else {
        return Ok(false);
    };
    let source = match &source.kind {
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => {
            ArrayMapDynamicSource::Expr(source)
        }
        ExprKind::Variable(source_name)
            if module.local_kind(source_name) == Some(LocalKind::Array) =>
        {
            ArrayMapDynamicSource::Local(source_name, source.span)
        }
        _ => return Ok(false),
    };
    let mut callback_shapes = Vec::with_capacity(callbacks.len());
    for callback in &callbacks {
        if !module.has_function(callback) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic array_map() callback helper can only return declared user functions",
            ));
        }
        let shape = array_map_callback_shape(callback, callback_expr.span, module)?;
        if !array_map_dynamic_callback_shape_is_supported(shape) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic array_map() callback helper currently requires a scalar callback",
            ));
        }
        callback_shapes.push(shape);
    }

    let callback_ptr = module.next_label("array_map_callback_ptr");
    let callback_len = module.next_label("array_map_callback_len");
    let matched = module.next_label("array_map_callback_matched");
    for local in [&callback_ptr, &callback_len, &matched] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_string_value_to_stack(callback_expr, module)?;
    module.body().line(&format!("local.set {}", callback_len));
    module.body().line(&format!("local.set {}", callback_ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));

    for (callback, shape) in callbacks.into_iter().zip(callback_shapes.into_iter()) {
        let candidate_ptr = module.next_label("array_map_callback_candidate_ptr");
        let candidate_len = module.next_label("array_map_callback_candidate_len");
        let candidate_match = module.next_label("array_map_callback_candidate_match");
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
        emit_array_map_dynamic_source_assign(name, &source, &callback, shape, module)?;
        module.body().line("i32.const 1");
        module.body().line(&format!("local.set {}", matched));
        module.body().close("end");
    }

    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    Ok(true)
}

fn emit_array_map_dynamic_callable_descriptor_assign(
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let [callback_expr, source] = args else {
        return Ok(false);
    };
    let Some(callbacks) = dynamic_array_map_callable_descriptor_targets(callback_expr, module) else {
        return Ok(false);
    };
    let source = match &source.kind {
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => {
            ArrayMapDynamicSource::Expr(source)
        }
        ExprKind::Variable(source_name)
            if module.local_kind(source_name) == Some(LocalKind::Array) =>
        {
            ArrayMapDynamicSource::Local(source_name, source.span)
        }
        _ => return Ok(false),
    };
    let mut callback_shapes = Vec::with_capacity(callbacks.len());
    for callback in &callbacks {
        if !module.has_function(callback) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic array_map() callable descriptor can only target declared user functions",
            ));
        }
        let shape = array_map_callback_shape(callback, callback_expr.span, module)?;
        if !array_map_dynamic_callback_shape_is_supported(shape) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic array_map() callable descriptor currently requires a scalar callback",
            ));
        }
        callback_shapes.push(shape);
    }

    let callback_id = module.next_label("array_map_callable_id");
    let matched = module.next_label("array_map_callable_matched");
    for local in [&callback_id, &matched] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_array_map_callable_descriptor(callback_expr, module)?;
    module.body().line(&format!("local.set {}", callback_id));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));

    for (callback, shape) in callbacks.into_iter().zip(callback_shapes.into_iter()) {
        let target_id = module.callable_target_id(&callback);
        module.body().line(&format!("local.get {}", callback_id));
        module.body().line(&format!("i32.const {}", target_id));
        module.body().line("i32.eq");
        module.body().open("if");
        emit_array_map_dynamic_source_assign(name, &source, &callback, shape, module)?;
        module.body().line("i32.const 1");
        module.body().line(&format!("local.set {}", matched));
        module.body().close("end");
    }

    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    Ok(true)
}

fn dynamic_array_map_callback_names(expr: &Expr, module: &WasmModule) -> Option<Vec<String>> {
    match &expr.kind {
        ExprKind::FunctionCall { name, .. } => module
            .function_possible_static_string_returns(name.as_str())
            .map(<[_]>::to_vec),
        ExprKind::Variable(name) => module.possible_static_string_values(name).map(<[_]>::to_vec),
        _ => None,
    }
}

fn dynamic_array_map_callable_descriptor_targets(
    expr: &Expr,
    module: &WasmModule,
) -> Option<Vec<String>> {
    match &expr.kind {
        ExprKind::FunctionCall { name, .. } => module
            .function_possible_callable_return_targets(name.as_str())
            .map(<[_]>::to_vec)
            .or_else(|| {
                module
                    .function_callable_return_target(name.as_str())
                    .map(|target| vec![target])
            }),
        ExprKind::Variable(name) => module
            .possible_callable_targets(name)
            .map(<[_]>::to_vec)
            .or_else(|| module.callable_target(name).map(|target| vec![target])),
        _ => None,
    }
}

fn emit_array_map_callable_descriptor(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match &expr.kind {
        ExprKind::FunctionCall { name, args } => {
            emit_user_function_args(expr, name, args, module)?;
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(name)));
            Ok(())
        }
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
            "wasm32-web array_map() callable descriptor requires tracked callable metadata",
        )),
    }
}

fn array_map_dynamic_callback_shape_is_supported(shape: ArrayMapCallbackShape) -> bool {
    matches!(
        shape,
        ArrayMapCallbackShape::IntToInt
            | ArrayMapCallbackShape::IntToBool
            | ArrayMapCallbackShape::NumericToBool
            | ArrayMapCallbackShape::StrToInt
            | ArrayMapCallbackShape::StrToBool
            | ArrayMapCallbackShape::StrToStr
            | ArrayMapCallbackShape::BoolToBool
            | ArrayMapCallbackShape::FloatToBool
            | ArrayMapCallbackShape::NullToBool
            | ArrayMapCallbackShape::ArrayToBool
    )
}

fn emit_array_map_dynamic_source_assign(
    name: &str,
    source: &ArrayMapDynamicSource<'_>,
    callback: &str,
    shape: ArrayMapCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match source {
        ArrayMapDynamicSource::Expr(source_expr) => {
            emit_array_map_array_expr_assign(name, source_expr, source_expr.span, callback, shape, module)
        }
        ArrayMapDynamicSource::Local(source, source_span) => {
            emit_array_map_staged_assign(name, source, *source_span, callback, shape, module)
        }
    }
}

fn emit_array_map_many_array_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(callback) = evaluated_static_callback_function_name(&args[0], module)? else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() currently requires a static string, direct first-class function callback, or simple callable variable alias",
        ));
    };
    if !module.has_function(&callback) {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() currently requires a user-defined callback",
        ));
    }
    let Some(param_kinds) = module.function_param_kinds(&callback) else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_map() callback metadata is missing",
        ));
    };
    let source_count = args.len() - 1;
    if param_kinds.len() != source_count
        || !param_kinds
            .iter()
            .all(|kind| matches!(kind, LocalKind::Mixed))
        || !matches!(
            module.function_return_kind(&callback),
            Some(ValueKind::Int | ValueKind::Str | ValueKind::Bool | ValueKind::Float)
        )
    {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web high-arity array_map() currently requires all-mixed parameters and int/string/bool/float callback results",
        ));
    }
    let mut sources = Vec::with_capacity(source_count);
    for (index, arg) in args.iter().enumerate().skip(1) {
        sources.push(materialize_array_map_multi_source(
            arg,
            &format!("array_map_source_{index}"),
            module,
        )?);
    }
    let source_refs: Vec<_> = sources.iter().map(String::as_str).collect();
    emit_array_map_many_mixed_locals_assign(
        name,
        &source_refs,
        "many",
        call.span,
        &callback,
        module,
    )
}

pub(in crate::codegen::wasm) fn materialize_array_map_multi_source(
    source: &Expr,
    label: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    if let ExprKind::Variable(name) = &source.kind {
        return Ok(name.clone());
    }
    if !expression_has_array_type(source, module) {
        return Err(CompileError::new(
            source.span,
            "wasm32-web multi-array array_map() currently requires array sources",
        ));
    }
    let temp = module.next_label(label).trim_start_matches('$').to_string();
    module.declare_array_local(temp.clone());
    emit_array_assign(&temp, source, module)?;
    Ok(temp)
}

fn emit_array_map_array_expr_assign(
    name: &str,
    source_expr: &Expr,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayMapCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_map_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_array_assign(&temp, source_expr, module)?;
    emit_array_map_staged_assign(name, &temp, source_span, callback, shape, module)
}

fn materialize_nested_array_map_source(
    source: &Expr,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let Some(metadata) = nested_array_metadata_for_access_expr(source, module) else {
        return Err(CompileError::new(
            source.span,
            "wasm32-web array_map() requires nested array metadata",
        ));
    };
    let temp = module
        .next_label("array_map_nested_source")
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
                "wasm32-web array_map() expected a nested array value",
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

fn materialize_method_array_map_source(
    source: &Expr,
    object: &Expr,
    method: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let metadata = method_call_array_return_metadata(object, method, module).ok_or_else(|| {
        CompileError::new(
            source.span,
            "wasm32-web array_map() requires method array return metadata",
        )
    })?;
    let temp = module
        .next_label("array_map_method_source")
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
                "wasm32-web array_map() expected an array-returning method value",
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

fn materialize_static_method_array_map_source(
    source: &Expr,
    receiver: &StaticReceiver,
    method: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let metadata = static_method_call_array_return_metadata(receiver, method, module).ok_or_else(|| {
        CompileError::new(
            source.span,
            "wasm32-web array_map() requires static method array return metadata",
        )
    })?;
    let temp = module
        .next_label("array_map_static_method_source")
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
                "wasm32-web array_map() expected an array-returning static method value",
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

fn emit_array_map_staged_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayMapCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match shape {
        ArrayMapCallbackShape::IntToInt if module.array_layout(source) == ArrayLayout::CompactInt => {
            emit_array_map_compact_int_local_assign(name, source, source_span, callback, module)
        }
        ArrayMapCallbackShape::IntToBool if module.array_layout(source) == ArrayLayout::CompactInt => {
            emit_array_map_compact_int_bools_local_assign(name, source, source_span, callback, module)
        }
        ArrayMapCallbackShape::NumericToBool if module.array_layout(source) == ArrayLayout::CompactInt => {
            emit_array_map_compact_int_bools_local_assign(name, source, source_span, callback, module)
        }
        ArrayMapCallbackShape::StrToInt if module.array_layout(source) == ArrayLayout::CompactInt => {
            emit_array_map_compact_int_strlen_lengths_assign(name, source, source_span, module)
        }
        ArrayMapCallbackShape::IntToInt
            if module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_ints(source, module) =>
        {
            emit_array_map_value_int_local_assign(name, source, source_span, callback, module)
        }
        ArrayMapCallbackShape::IntToInt
            if module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_bools(source, module) =>
        {
            emit_array_map_value_int_local_assign(name, source, source_span, callback, module)
        }
        ArrayMapCallbackShape::IntToInt
            if module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_floats(source, module) =>
        {
            emit_array_map_value_floats_as_ints_local_assign(name, source, source_span, callback, module)
        }
        ArrayMapCallbackShape::IntToInt
            if module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_map_value_numeric_strings_as_ints_local_assign(name, source, source_span, callback, module)
        }
        ArrayMapCallbackShape::IntToBool
            if module.array_layout(source) == ArrayLayout::Value
                && array_filter_object_false_callback(callback)
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_map_value_object_false_bools_local_assign(name, source, source_span, module)
        }
        ArrayMapCallbackShape::IntToBool
            if module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_ints(source, module) =>
        {
            emit_array_map_value_int_bools_local_assign(name, source, source_span, callback, module)
        }
        ArrayMapCallbackShape::NumericToBool
            if module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_ints(source, module) =>
        {
            emit_array_map_value_int_bools_local_assign(name, source, source_span, callback, module)
        }
        ArrayMapCallbackShape::StrToInt
            if module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_map_value_string_lengths_assign(name, source, source_span, module)
        }
        ArrayMapCallbackShape::StrToInt
            if module.array_layout(source) == ArrayLayout::Value
                && array_map_strlen_value_cells_are_supported(source, module) =>
        {
            emit_array_map_value_scalar_lengths_assign(name, source, source_span, module)
        }
        ArrayMapCallbackShape::StrToBool
            if module.array_layout(source) == ArrayLayout::Value
                && array_filter_object_false_callback(callback)
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_map_value_object_false_bools_local_assign(name, source, source_span, module)
        }
        ArrayMapCallbackShape::StrToBool
            if module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_map_value_string_bools_local_assign(name, source, source_span, callback, module)
        }
        ArrayMapCallbackShape::NumericToBool
            if module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_map_value_string_bools_local_assign(name, source, source_span, callback, module)
        }
        ArrayMapCallbackShape::StrToStr
            if module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_map_value_string_local_assign(name, source, source_span, callback, module)
        }
        ArrayMapCallbackShape::BoolToBool
            if module.array_layout(source) == ArrayLayout::Value
                && !array_filter_type_predicate_callback(callback)
                && array_filter_value_cells_are_bools(source, module) =>
        {
            emit_array_map_value_scalar_predicate_bools_local_assign(
                name,
                source,
                source_span,
                callback,
                ValueCellKind::Bool,
                module,
            )
        }
        ArrayMapCallbackShape::BoolToBool
            if module.array_layout(source) == ArrayLayout::Value
                && array_filter_object_false_callback(callback)
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_map_value_object_false_bools_local_assign(name, source, source_span, module)
        }
        ArrayMapCallbackShape::BoolToBool
            if module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_bools(source, module) =>
        {
            emit_array_map_value_scalar_type_bools_local_assign(name, source, source_span, ValueCellKind::Bool, module)
        }
        ArrayMapCallbackShape::FloatToBool
            if module.array_layout(source) == ArrayLayout::Value
                && !array_filter_type_predicate_callback(callback)
                && array_filter_value_cells_are_floats(source, module) =>
        {
            emit_array_map_value_scalar_predicate_bools_local_assign(
                name,
                source,
                source_span,
                callback,
                ValueCellKind::Float,
                module,
            )
        }
        ArrayMapCallbackShape::FloatToBool
            if module.array_layout(source) == ArrayLayout::Value
                && array_filter_object_false_callback(callback)
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_map_value_object_false_bools_local_assign(name, source, source_span, module)
        }
        ArrayMapCallbackShape::FloatToBool
            if module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_floats(source, module) =>
        {
            emit_array_map_value_scalar_type_bools_local_assign(name, source, source_span, ValueCellKind::Float, module)
        }
        ArrayMapCallbackShape::NumericToBool
            if module.array_layout(source) == ArrayLayout::Value
                && array_filter_object_false_callback(callback)
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_map_value_object_false_bools_local_assign(name, source, source_span, module)
        }
        ArrayMapCallbackShape::NumericToBool
            if module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_floats(source, module) =>
        {
            emit_array_map_value_scalar_type_bools_local_assign(name, source, source_span, ValueCellKind::Float, module)
        }
        ArrayMapCallbackShape::NullToBool
            if module.array_layout(source) == ArrayLayout::Value
                && array_filter_object_false_callback(callback)
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_map_value_object_false_bools_local_assign(name, source, source_span, module)
        }
        ArrayMapCallbackShape::NullToBool
            if module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_nulls(source, module) =>
        {
            emit_array_map_value_scalar_type_bools_local_assign(name, source, source_span, ValueCellKind::Null, module)
        }
        ArrayMapCallbackShape::ArrayToBool
            if module.array_layout(source) == ArrayLayout::Value
                && array_filter_object_false_callback(callback)
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_map_value_object_false_bools_local_assign(name, source, source_span, module)
        }
        ArrayMapCallbackShape::ArrayToBool
            if module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_arrays(source, module) =>
        {
            emit_array_map_value_scalar_type_bools_local_assign(name, source, source_span, ValueCellKind::Array, module)
        }
        ArrayMapCallbackShape::ObjectToBool
            if module.array_layout(source) == ArrayLayout::Value
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_map_value_object_type_bools_local_assign(name, source, source_span, module)
        }
        ArrayMapCallbackShape::ObjectToStr
            if module.array_layout(source) == ArrayLayout::Value
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_map_value_object_class_names_local_assign(name, source, source_span, module)
        }
        ArrayMapCallbackShape::ObjectToTypeStr
            if module.array_layout(source) == ArrayLayout::Value
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_map_value_object_type_names_local_assign(name, source, source_span, module)
        }
        _ if module.array_layout(source) == ArrayLayout::Assoc
            && array_map_assoc_local_matches_shape(source, shape, module) =>
        {
            emit_array_map_assoc_local_assign(name, source, source_span, callback, shape, module)
        }
        _ if shape == ArrayMapCallbackShape::StrToInt
            && module.array_layout(source) == ArrayLayout::Assoc
            && array_map_strlen_assoc_local_is_supported(source, module) =>
        {
            emit_array_map_assoc_strlen_local_assign(name, source, source_span, module)
        }
        _ if module.array_layout(source) == ArrayLayout::Assoc
            && array_map_runtime_assoc_local_matches_shape(source, shape, module) =>
        {
            emit_array_map_runtime_assoc_local_assign(name, source, source_span, callback, shape, module)
        }
        _ if shape == ArrayMapCallbackShape::StrToInt
            && module.array_layout(source) == ArrayLayout::Assoc
            && array_map_strlen_runtime_assoc_local_is_supported(source, module) =>
        {
            emit_array_map_runtime_assoc_strlen_local_assign(name, source, source_span, module)
        }
        _ => Err(CompileError::new(
            source_span,
            "wasm32-web array_map() direct array expressions require homogeneous int or string arrays",
        )),
    }
}
