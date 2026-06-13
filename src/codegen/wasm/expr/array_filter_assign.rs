//! Purpose:
//! Lowers array_filter assignment forms for the wasm32-web backend.
//! Covers callback modes, default truthiness filtering, and materialized array-expression sources.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::emit_array_assign()`
//!
//! Key details:
//! - Filter lowering must preserve assoc keys and value-cell metadata for downstream consumers.

use super::*;
use super::array_filter_assign_default::emit_array_filter_default_assign;
use super::array_filter_assign_modes::emit_array_filter_mode_assign;
use super::array_filter_default::{
    emit_array_filter_default_value_object_local_assign, emit_array_filter_empty_value_object_local_assign,
};

enum ArrayFilterDynamicSource<'a> {
    Expr(&'a Expr),
    Local(&'a str, crate::span::Span),
}

fn instance_callback_for_array_filter(
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
        return capture_instance_callback_for_array_filter(callback, object, &method, module);
    }
    let ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) = &callback.kind else {
        if let Some((target, class_name)) = instance_callback_target(callback, "__invoke", module) {
            let capture_local = module
                .next_label("array_filter_invokable_object")
                .trim_start_matches('$')
                .to_string();
            module.declare_object_local(capture_local.clone());
            let kind = emit_expr(callback, module)?;
            if kind != ValueKind::Object {
                return Err(CompileError::new(
                    callback.span,
                    "wasm32-web array_filter() invokable callback requires an object receiver",
                ));
            }
            module.body().line(&format!("local.set ${}", capture_local));
            module.set_object_class_for_local(&capture_local, Some(class_name));
            return Ok(Some((target, capture_local)));
        }
        return Ok(None);
    };
    capture_instance_callback_for_array_filter(callback, object, method, module)
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

fn capture_instance_callback_for_array_filter(
    callback: &Expr,
    object: &Expr,
    method: &str,
    module: &mut WasmModule,
) -> Result<Option<(String, String)>, CompileError> {
    let Some((target, class_name)) = instance_callback_target(object, method, module) else {
        return Ok(None);
    };
    let capture_local = module
        .next_label("array_filter_callable_object")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(capture_local.clone());
    let kind = emit_expr(object, module)?;
    if kind != ValueKind::Object {
        return Err(CompileError::new(
            callback.span,
            "wasm32-web array_filter() instance callback requires an object receiver",
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

pub(in crate::codegen::wasm) fn emit_array_filter_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if args.len() == 1 {
        return emit_array_filter_default_assign(name, call, &args[0], module);
    }
    if args.len() == 3 {
        return emit_array_filter_mode_assign(name, call, args, module);
    }
    if args.len() != 2 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web array_filter() currently supports array and callback arguments only",
        ));
    }
    if let Some((callback, capture_local)) = instance_callback_for_array_filter(&args[1], module)? {
        let param_kinds = module.function_param_kinds(&callback).ok_or_else(|| {
            CompileError::new(
                args[1].span,
                "wasm32-web array_filter() instance callback metadata is missing",
            )
        })?;
        let callback_takes_int =
            param_kinds.as_slice() == [LocalKind::Object, LocalKind::I64]
                && matches!(
                    module.function_return_kind(&callback),
                    Some(ValueKind::Bool | ValueKind::Int)
                );
        let callback_takes_string =
            param_kinds.as_slice() == [LocalKind::Object, LocalKind::Str]
                && matches!(
                    module.function_return_kind(&callback),
                    Some(ValueKind::Bool | ValueKind::Int)
                );
        let callback_takes_float =
            param_kinds.as_slice() == [LocalKind::Object, LocalKind::F64]
                && matches!(
                    module.function_return_kind(&callback),
                    Some(ValueKind::Bool | ValueKind::Int)
                );
        let callback_takes_bool =
            param_kinds.as_slice() == [LocalKind::Object, LocalKind::I32]
                && matches!(
                    module.function_return_kind(&callback),
                    Some(ValueKind::Bool | ValueKind::Int)
                );
        let instance_shape = if callback_takes_int {
            Some(ArrayFilterCallbackShape::Int)
        } else if callback_takes_string {
            Some(ArrayFilterCallbackShape::Str)
        } else if callback_takes_float {
            Some(ArrayFilterCallbackShape::Float)
        } else if callback_takes_bool {
            Some(ArrayFilterCallbackShape::Bool)
        } else {
            None
        };
        if !callback_takes_int && !callback_takes_string && !callback_takes_float && !callback_takes_bool {
            return Err(CompileError::new(
                args[1].span,
                "wasm32-web array_filter() instance callbacks currently require one int, string, float, or bool predicate",
            ));
        }
        if let ExprKind::Variable(source) = &args[0].kind {
            if callback_takes_int
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::CompactInt
            {
                return emit_array_filter_compact_int_local_instance_assign(
                    name,
                    source,
                    args[0].span,
                    &callback,
                    &capture_local,
                    module,
                );
            }
            if callback_takes_string
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && (module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Str)
                    || module
                        .array_value_cell_kinds(source)
                        .is_some_and(|kinds| kinds.iter().all(|kind| *kind == ValueCellKind::Str)))
            {
                return emit_array_filter_value_string_local_instance_assign(
                    name,
                    source,
                    args[0].span,
                    &callback,
                    &capture_local,
                    module,
                );
            }
            if callback_takes_float
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && (module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Float)
                    || module.array_value_cell_kinds(source).is_some_and(|kinds| {
                        kinds.iter().all(|kind| *kind == ValueCellKind::Float)
                    }))
            {
                return emit_array_filter_value_float_local_instance_assign(
                    name,
                    source,
                    args[0].span,
                    &callback,
                    &capture_local,
                    module,
                );
            }
            if callback_takes_bool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && (module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Bool)
                    || module.array_value_cell_kinds(source).is_some_and(|kinds| {
                        kinds.iter().all(|kind| *kind == ValueCellKind::Bool)
                    }))
            {
                return emit_array_filter_value_bool_local_instance_assign(
                    name,
                    source,
                    args[0].span,
                    &callback,
                    &capture_local,
                    module,
                );
            }
            if let Some(shape) = instance_shape {
                if module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::Assoc
                    && array_filter_assoc_local_matches_shape(source, shape, module)
                {
                    return emit_array_filter_assoc_local_instance_assign(
                        name,
                        source,
                        args[0].span,
                        &callback,
                        &capture_local,
                        shape,
                        module,
                    );
                }
                if module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::Assoc
                    && array_filter_runtime_assoc_local_matches_shape(source, shape, module)
                {
                    return emit_array_filter_runtime_assoc_local_instance_assign(
                        name,
                        source,
                        args[0].span,
                        &callback,
                        &capture_local,
                        shape,
                        module,
                    );
                }
            }
        }
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_filter() instance callbacks currently require an assigned compact integer, value-cell, or associative array with callback-compatible values",
        ));
    }
    let Some(callback) = evaluated_static_callback_function_name(&args[1], module)? else {
        if emit_array_filter_static_callback_ternary_assign(name, args, module)? {
            return Ok(());
        }
        if emit_array_filter_dynamic_callable_descriptor_assign(name, args, module)? {
            return Ok(());
        }
        if emit_array_filter_dynamic_static_return_callback_assign(name, args, module)? {
            return Ok(());
        }
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web array_filter() currently requires a static string, direct first-class function callback, or simple callable variable alias",
        ));
    };
    if !module.has_function(&callback) && !array_filter_builtin_callback_is_supported(&callback) {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web array_filter() currently requires a user-defined callback or supported builtin callback",
        ));
    }
    let shape = array_filter_callback_shape(&callback, args[1].span, module)?;

    match &args[0].kind {
        ExprKind::ArrayLiteral(items)
            if shape == ArrayFilterCallbackShape::Int && !array_literal_needs_value_cells(items) =>
        {
            emit_array_filter_literal_ints_assign(name, items, &callback, module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayFilterCallbackShape::Numeric && !array_literal_needs_value_cells(items) =>
        {
            emit_array_filter_literal_ints_assign(name, items, &callback, module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayFilterCallbackShape::Int && array_map_items_are_ints(items, module) =>
        {
            emit_array_filter_literal_ints_assign(name, items, &callback, module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayFilterCallbackShape::Numeric && array_map_items_are_ints(items, module) =>
        {
            emit_array_filter_literal_ints_assign(name, items, &callback, module)
        }
        ExprKind::ArrayLiteral(items)
            if callback.eq_ignore_ascii_case("strlen") && !array_literal_needs_value_cells(items) =>
        {
            emit_array_filter_literal_ints_assign(name, items, &callback, module)
        }
        ExprKind::ArrayLiteral(items)
            if callback.eq_ignore_ascii_case("strlen") && array_map_items_are_ints(items, module) =>
        {
            emit_array_filter_literal_ints_assign(name, items, &callback, module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayFilterCallbackShape::Str && array_map_items_are_strings(items, module) =>
        {
            emit_array_filter_literal_strings_assign(name, items, &callback, module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayFilterCallbackShape::Numeric && array_map_items_are_strings(items, module) =>
        {
            emit_array_filter_literal_strings_assign(name, items, &callback, module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayFilterCallbackShape::Bool
                && !array_filter_type_predicate_callback(&callback)
                && array_filter_items_are_bools(items, module) =>
        {
            emit_array_filter_literal_scalar_predicate_assign(
                name,
                items,
                &callback,
                ValueCellKind::Bool,
                module,
            )
        }
        ExprKind::ArrayLiteral(items) if shape == ArrayFilterCallbackShape::Bool && array_filter_items_are_bools(items, module) => {
            emit_array_filter_literal_scalar_type_assign(name, items, ValueCellKind::Bool, module)
        }
        ExprKind::ArrayLiteral(items) if callback.eq_ignore_ascii_case("strlen") && array_filter_items_are_bools(items, module) => {
            emit_array_filter_literal_scalar_strlen_assign(name, items, ValueCellKind::Bool, module)
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayFilterCallbackShape::Float
                && !array_filter_type_predicate_callback(&callback)
                && array_filter_items_are_floats(items, module) =>
        {
            emit_array_filter_literal_scalar_predicate_assign(
                name,
                items,
                &callback,
                ValueCellKind::Float,
                module,
            )
        }
        ExprKind::ArrayLiteral(items)
            if shape == ArrayFilterCallbackShape::Float
                && !array_filter_type_predicate_callback(&callback)
                && array_map_items_are_strings(items, module) =>
        {
            emit_array_filter_literal_scalar_predicate_assign(
                name,
                items,
                &callback,
                ValueCellKind::Str,
                module,
            )
        }
        ExprKind::ArrayLiteral(items) if shape == ArrayFilterCallbackShape::Float && array_filter_items_are_floats(items, module) => {
            emit_array_filter_literal_scalar_type_assign(name, items, ValueCellKind::Float, module)
        }
        ExprKind::ArrayLiteral(items) if shape == ArrayFilterCallbackShape::Numeric && array_filter_items_are_floats(items, module) => {
            emit_array_filter_literal_scalar_type_assign(name, items, ValueCellKind::Float, module)
        }
        ExprKind::ArrayLiteral(items) if callback.eq_ignore_ascii_case("strlen") && array_filter_items_are_floats(items, module) => {
            emit_array_filter_literal_scalar_strlen_assign(name, items, ValueCellKind::Float, module)
        }
        ExprKind::ArrayLiteral(items) if shape == ArrayFilterCallbackShape::Null && array_filter_items_are_nulls(items, module) => {
            emit_array_filter_literal_scalar_type_assign(name, items, ValueCellKind::Null, module)
        }
        ExprKind::ArrayLiteral(items) if callback.eq_ignore_ascii_case("strlen") && array_filter_items_are_nulls(items, module) => {
            emit_array_filter_literal_scalar_strlen_assign(name, items, ValueCellKind::Null, module)
        }
        ExprKind::ArrayLiteral(items) if shape == ArrayFilterCallbackShape::Array && array_filter_items_are_arrays(items, module) => {
            emit_array_filter_literal_scalar_type_assign(name, items, ValueCellKind::Array, module)
        }
        ExprKind::ArrayLiteralAssoc(items)
            if callback.eq_ignore_ascii_case("strlen") && array_filter_strlen_assoc_items_are_supported(items, module) =>
        {
            emit_array_filter_assoc_strlen_literal_assign(name, items, call.span, module)
        }
        ExprKind::ArrayLiteralAssoc(items) if array_filter_assoc_items_match_shape(items, shape, module) => {
            emit_array_filter_assoc_literal_assign(name, items, call.span, &callback, shape, module)
        }
        ExprKind::ArrayLiteralAssoc(items)
            if shape == ArrayFilterCallbackShape::Float
                && !array_filter_type_predicate_callback(&callback)
                && value_cell_kinds_for_assoc_items(items, module).is_some_and(|kinds| {
                    kinds.iter().all(|kind| *kind == ValueCellKind::Str)
                }) =>
        {
            emit_array_filter_assoc_literal_assign(name, items, call.span, &callback, shape, module)
        }
        ExprKind::ArrayAccess { .. } if nested_array_metadata_for_access_expr(&args[0], module).is_some() => {
            let temp = materialize_nested_array_filter_source(&args[0], module)?;
            emit_array_filter_staged_assign(name, &temp, args[0].span, &callback, shape, module)
        }
        ExprKind::MethodCall { object, method, .. }
        | ExprKind::NullsafeMethodCall { object, method, .. }
            if method_call_array_return_metadata(object, method, module).is_some() =>
        {
            let temp = materialize_method_array_filter_source(&args[0], object, method, module)?;
            emit_array_filter_staged_assign(name, &temp, args[0].span, &callback, shape, module)
        }
        ExprKind::StaticMethodCall { receiver, method, .. }
            if static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            let temp = materialize_static_method_array_filter_source(&args[0], receiver, method, module)?;
            emit_array_filter_staged_assign(name, &temp, args[0].span, &callback, shape, module)
        }
        ExprKind::FunctionCall { .. } if expression_has_array_type(&args[0], module) => {
            emit_array_filter_array_expr_assign(name, &args[0], args[0].span, &callback, shape, module)
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
            emit_array_filter_array_expr_assign(name, &args[0], args[0].span, &callback, shape, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayFilterCallbackShape::Int
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::CompactInt =>
        {
            emit_array_filter_compact_int_local_assign(name, source, args[0].span, &callback, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayFilterCallbackShape::Numeric
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::CompactInt =>
        {
            emit_array_filter_compact_int_local_assign(name, source, args[0].span, &callback, module)
        }
        ExprKind::Variable(source)
            if callback.eq_ignore_ascii_case("strlen")
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::CompactInt =>
        {
            emit_array_filter_compact_int_local_assign(name, source, args[0].span, &callback, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayFilterCallbackShape::Int
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_ints(source, module) =>
        {
            emit_array_filter_value_int_local_assign(name, source, args[0].span, &callback, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayFilterCallbackShape::Numeric
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_ints(source, module) =>
        {
            emit_array_filter_value_int_local_assign(name, source, args[0].span, &callback, module)
        }
        ExprKind::Variable(source)
            if callback.eq_ignore_ascii_case("strlen")
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_ints(source, module) =>
        {
            emit_array_filter_value_int_local_assign(name, source, args[0].span, &callback, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayFilterCallbackShape::Str
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_filter_value_string_local_assign(name, source, args[0].span, &callback, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayFilterCallbackShape::Numeric
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_filter_value_string_local_assign(name, source, args[0].span, &callback, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayFilterCallbackShape::Bool
                && !array_filter_type_predicate_callback(&callback)
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_bools(source, module) =>
        {
            emit_array_filter_value_scalar_predicate_local_assign(
                name,
                source,
                args[0].span,
                &callback,
                ValueCellKind::Bool,
                module,
            )
        }
        ExprKind::Variable(source)
            if shape == ArrayFilterCallbackShape::Bool
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_bools(source, module) =>
        {
            emit_array_filter_value_scalar_type_local_assign(
                name,
                source,
                args[0].span,
                ValueCellKind::Bool,
                module,
            )
        }
        ExprKind::Variable(source)
            if callback.eq_ignore_ascii_case("strlen")
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_bools(source, module) =>
        {
            emit_array_filter_value_scalar_strlen_local_assign(
                name,
                source,
                args[0].span,
                ValueCellKind::Bool,
                module,
            )
        }
        ExprKind::Variable(source)
            if shape == ArrayFilterCallbackShape::Float
                && !array_filter_type_predicate_callback(&callback)
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_floats(source, module) =>
        {
            emit_array_filter_value_scalar_predicate_local_assign(
                name,
                source,
                args[0].span,
                &callback,
                ValueCellKind::Float,
                module,
            )
        }
        ExprKind::Variable(source)
            if shape == ArrayFilterCallbackShape::Float
                && !array_filter_type_predicate_callback(&callback)
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_filter_value_scalar_predicate_local_assign(
                name,
                source,
                args[0].span,
                &callback,
                ValueCellKind::Str,
                module,
            )
        }
        ExprKind::Variable(source)
            if shape == ArrayFilterCallbackShape::Float
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_floats(source, module) =>
        {
            emit_array_filter_value_scalar_type_local_assign(
                name,
                source,
                args[0].span,
                ValueCellKind::Float,
                module,
            )
        }
        ExprKind::Variable(source)
            if callback.eq_ignore_ascii_case("strlen")
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_floats(source, module) =>
        {
            emit_array_filter_value_scalar_strlen_local_assign(
                name,
                source,
                args[0].span,
                ValueCellKind::Float,
                module,
            )
        }
        ExprKind::Variable(source)
            if shape == ArrayFilterCallbackShape::Numeric
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_floats(source, module) =>
        {
            emit_array_filter_value_scalar_type_local_assign(
                name,
                source,
                args[0].span,
                ValueCellKind::Float,
                module,
            )
        }
        ExprKind::Variable(source)
            if shape == ArrayFilterCallbackShape::Null
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_nulls(source, module) =>
        {
            emit_array_filter_value_scalar_type_local_assign(
                name,
                source,
                args[0].span,
                ValueCellKind::Null,
                module,
            )
        }
        ExprKind::Variable(source)
            if callback.eq_ignore_ascii_case("strlen")
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_nulls(source, module) =>
        {
            emit_array_filter_value_scalar_strlen_local_assign(
                name,
                source,
                args[0].span,
                ValueCellKind::Null,
                module,
            )
        }
        ExprKind::Variable(source)
            if shape == ArrayFilterCallbackShape::Array
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_arrays(source, module) =>
        {
            emit_array_filter_value_scalar_type_local_assign(
                name,
                source,
                args[0].span,
                ValueCellKind::Array,
                module,
            )
        }
        ExprKind::Variable(source)
            if array_filter_object_false_callback(&callback)
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_filter_empty_value_object_local_assign(name, source, args[0].span, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayFilterCallbackShape::Object
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_filter_default_value_object_local_assign(name, source, args[0].span, module)
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && callback.eq_ignore_ascii_case("strlen")
                && array_filter_strlen_assoc_local_is_supported(source, module) =>
        {
            emit_array_filter_assoc_strlen_local_assign(name, source, args[0].span, module)
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_filter_assoc_local_matches_shape(source, shape, module) =>
        {
            emit_array_filter_assoc_local_assign(name, source, args[0].span, &callback, shape, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayFilterCallbackShape::Float
                && !array_filter_type_predicate_callback(&callback)
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_filter_assoc_local_has_string_values(source, module) =>
        {
            emit_array_filter_assoc_local_assign(name, source, args[0].span, &callback, shape, module)
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && callback.eq_ignore_ascii_case("strlen")
                && array_filter_strlen_runtime_assoc_local_is_supported(source, module) =>
        {
            emit_array_filter_runtime_assoc_strlen_local_assign(name, source, args[0].span, module)
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_filter_runtime_assoc_local_matches_shape(source, shape, module) =>
        {
            emit_array_filter_runtime_assoc_local_assign(name, source, args[0].span, &callback, shape, module)
        }
        ExprKind::Variable(source)
            if shape == ArrayFilterCallbackShape::Float
                && !array_filter_type_predicate_callback(&callback)
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Str) =>
        {
            emit_array_filter_runtime_assoc_local_assign(name, source, args[0].span, &callback, shape, module)
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Mixed)
                && module.mixed_value_cell_kind(source).is_none()
                && array_filter_unknown_mixed_callback_is_supported(&callback) =>
        {
            emit_array_filter_unknown_mixed_callback_assign(name, source, &callback, module)
        }
        _ => Err(CompileError::new(
            args[0].span,
            "wasm32-web array_filter() currently supports homogeneous int or string arrays",
        )),
    }
}

fn emit_array_filter_static_callback_ternary_assign(
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let [source_expr, callback_expr] = args else {
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
        if !module.has_function(callback) && !array_filter_builtin_callback_is_supported(callback) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web array_filter() callable ternary arms currently require user-defined or supported builtin callbacks",
            ));
        }
    }
    let then_shape = array_filter_callback_shape(&then_callback, then_expr.span, module)?;
    let else_shape = array_filter_callback_shape(&else_callback, else_expr.span, module)?;
    if then_shape != else_shape {
        return Err(CompileError::new(
            callback_expr.span,
            "wasm32-web array_filter() callable ternary arms currently require the same supported scalar callback shape",
        ));
    }
    let source = match &source_expr.kind {
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => {
            ArrayFilterDynamicSource::Expr(source_expr)
        }
        ExprKind::Variable(source_name)
            if module.local_kind(source_name) == Some(LocalKind::Array) =>
        {
            ArrayFilterDynamicSource::Local(source_name, source_expr.span)
        }
        _ => return Ok(false),
    };
    emit_condition(condition, module)?;
    module.body().open("if");
    emit_array_filter_dynamic_source_assign(name, &source, &then_callback, then_shape, module)?;
    module.body().line("else");
    emit_array_filter_dynamic_source_assign(name, &source, &else_callback, else_shape, module)?;
    module.body().close("end");
    Ok(true)
}

fn emit_array_filter_dynamic_static_return_callback_assign(
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let [source, callback_expr] = args else {
        return Ok(false);
    };
    let Some(callbacks) = dynamic_array_filter_callback_names(callback_expr, module) else {
        return Ok(false);
    };
    let source = match &source.kind {
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => {
            ArrayFilterDynamicSource::Expr(source)
        }
        ExprKind::Variable(source_name)
            if module.local_kind(source_name) == Some(LocalKind::Array) =>
        {
            ArrayFilterDynamicSource::Local(source_name, source.span)
        }
        _ => return Ok(false),
    };
    let mut callback_shapes = Vec::with_capacity(callbacks.len());
    for callback in &callbacks {
        if !module.has_function(callback) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic array_filter() callback helper can only return declared user functions",
            ));
        }
        callback_shapes.push(array_filter_callback_shape(callback, callback_expr.span, module)?);
    }

    let callback_ptr = module.next_label("array_filter_callback_ptr");
    let callback_len = module.next_label("array_filter_callback_len");
    let matched = module.next_label("array_filter_callback_matched");
    for local in [&callback_ptr, &callback_len, &matched] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_string_value_to_stack(callback_expr, module)?;
    module.body().line(&format!("local.set {}", callback_len));
    module.body().line(&format!("local.set {}", callback_ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));

    for (callback, shape) in callbacks.into_iter().zip(callback_shapes.into_iter()) {
        let candidate_ptr = module.next_label("array_filter_callback_candidate_ptr");
        let candidate_len = module.next_label("array_filter_callback_candidate_len");
        let candidate_match = module.next_label("array_filter_callback_candidate_match");
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
        emit_array_filter_dynamic_source_assign(name, &source, &callback, shape, module)?;
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

fn emit_array_filter_dynamic_callable_descriptor_assign(
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let [source, callback_expr] = args else {
        return Ok(false);
    };
    let Some(callbacks) = dynamic_array_filter_callable_descriptor_targets(callback_expr, module) else {
        return Ok(false);
    };
    let source = match &source.kind {
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => {
            ArrayFilterDynamicSource::Expr(source)
        }
        ExprKind::Variable(source_name)
            if module.local_kind(source_name) == Some(LocalKind::Array) =>
        {
            ArrayFilterDynamicSource::Local(source_name, source.span)
        }
        _ => return Ok(false),
    };
    let mut callback_shapes = Vec::with_capacity(callbacks.len());
    for callback in &callbacks {
        if !module.has_function(callback) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic array_filter() callable descriptor can only target declared user functions",
            ));
        }
        callback_shapes.push(array_filter_callback_shape(callback, callback_expr.span, module)?);
    }

    let callback_id = module.next_label("array_filter_callable_id");
    let matched = module.next_label("array_filter_callable_matched");
    for local in [&callback_id, &matched] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_array_filter_callable_descriptor(callback_expr, module)?;
    module.body().line(&format!("local.set {}", callback_id));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));

    for (callback, shape) in callbacks.into_iter().zip(callback_shapes.into_iter()) {
        let target_id = module.callable_target_id(&callback);
        module.body().line(&format!("local.get {}", callback_id));
        module.body().line(&format!("i32.const {}", target_id));
        module.body().line("i32.eq");
        module.body().open("if");
        emit_array_filter_dynamic_source_assign(name, &source, &callback, shape, module)?;
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

fn dynamic_array_filter_callback_names(expr: &Expr, module: &WasmModule) -> Option<Vec<String>> {
    match &expr.kind {
        ExprKind::FunctionCall { name, .. } => module
            .function_possible_static_string_returns(name.as_str())
            .map(<[_]>::to_vec),
        ExprKind::Variable(name) => module.possible_static_string_values(name).map(<[_]>::to_vec),
        _ => None,
    }
}

fn dynamic_array_filter_callable_descriptor_targets(
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

fn emit_array_filter_callable_descriptor(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if callable_return_expr_targets(expr, module).is_some() {
        let kind = emit_expr(expr, module)?;
        if kind != ValueKind::Callable {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web array_filter() callable descriptor expected a callable value",
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
            "wasm32-web array_filter() callable descriptor requires tracked callable metadata",
        )),
    }
}

fn emit_array_filter_dynamic_source_assign(
    name: &str,
    source: &ArrayFilterDynamicSource<'_>,
    callback: &str,
    shape: ArrayFilterCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match source {
        ArrayFilterDynamicSource::Expr(source_expr) => {
            emit_array_filter_array_expr_assign(name, source_expr, source_expr.span, callback, shape, module)
        }
        ArrayFilterDynamicSource::Local(source, source_span) => {
            emit_array_filter_staged_assign(name, source, *source_span, callback, shape, module)
        }
    }
}

fn emit_array_filter_array_expr_assign(
    name: &str,
    source_expr: &Expr,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayFilterCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_filter_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_array_assign(&temp, source_expr, module)?;
    emit_array_filter_staged_assign(name, &temp, source_span, callback, shape, module)
}

fn materialize_nested_array_filter_source(
    source: &Expr,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let Some(metadata) = nested_array_metadata_for_access_expr(source, module) else {
        return Err(CompileError::new(
            source.span,
            "wasm32-web array_filter() requires nested array metadata",
        ));
    };
    let temp = module
        .next_label("array_filter_nested_source")
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
                "wasm32-web array_filter() expected a nested array value",
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

fn materialize_method_array_filter_source(
    source: &Expr,
    object: &Expr,
    method: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let metadata = method_call_array_return_metadata(object, method, module).ok_or_else(|| {
        CompileError::new(
            source.span,
            "wasm32-web array_filter() requires method array return metadata",
        )
    })?;
    let temp = module
        .next_label("array_filter_method_source")
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
                "wasm32-web array_filter() expected an array-returning method value",
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

fn materialize_static_method_array_filter_source(
    source: &Expr,
    receiver: &StaticReceiver,
    method: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let metadata = static_method_call_array_return_metadata(receiver, method, module).ok_or_else(|| {
        CompileError::new(
            source.span,
            "wasm32-web array_filter() requires static method array return metadata",
        )
    })?;
    let temp = module
        .next_label("array_filter_static_method_source")
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
                "wasm32-web array_filter() expected an array-returning static method value",
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

fn emit_array_filter_staged_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayFilterCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match shape {
        ArrayFilterCallbackShape::Int if module.array_layout(source) == ArrayLayout::CompactInt => {
            emit_array_filter_compact_int_local_assign(name, source, source_span, callback, module)
        }
        ArrayFilterCallbackShape::Numeric if module.array_layout(source) == ArrayLayout::CompactInt => {
            emit_array_filter_compact_int_local_assign(name, source, source_span, callback, module)
        }
        _ if callback.eq_ignore_ascii_case("strlen") && module.array_layout(source) == ArrayLayout::CompactInt => {
            emit_array_filter_compact_int_local_assign(name, source, source_span, callback, module)
        }
        ArrayFilterCallbackShape::Int
            if module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_ints(source, module) =>
        {
            emit_array_filter_value_int_local_assign(name, source, source_span, callback, module)
        }
        ArrayFilterCallbackShape::Numeric
            if module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_ints(source, module) =>
        {
            emit_array_filter_value_int_local_assign(name, source, source_span, callback, module)
        }
        _ if callback.eq_ignore_ascii_case("strlen")
            && module.array_layout(source) == ArrayLayout::Value
            && array_map_value_cells_are_ints(source, module) =>
        {
            emit_array_filter_value_int_local_assign(name, source, source_span, callback, module)
        }
        ArrayFilterCallbackShape::Str
            if module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_filter_value_string_local_assign(name, source, source_span, callback, module)
        }
        ArrayFilterCallbackShape::Numeric
            if module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_filter_value_string_local_assign(name, source, source_span, callback, module)
        }
        ArrayFilterCallbackShape::Bool
            if module.array_layout(source) == ArrayLayout::Value
                && !array_filter_type_predicate_callback(callback)
                && array_filter_value_cells_are_bools(source, module) =>
        {
            emit_array_filter_value_scalar_predicate_local_assign(
                name,
                source,
                source_span,
                callback,
                ValueCellKind::Bool,
                module,
            )
        }
        ArrayFilterCallbackShape::Bool
            if module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_bools(source, module) =>
        {
            emit_array_filter_value_scalar_type_local_assign(
                name,
                source,
                source_span,
                ValueCellKind::Bool,
                module,
            )
        }
        _ if callback.eq_ignore_ascii_case("strlen")
            && module.array_layout(source) == ArrayLayout::Value
            && array_filter_value_cells_are_bools(source, module) =>
        {
            emit_array_filter_value_scalar_strlen_local_assign(
                name,
                source,
                source_span,
                ValueCellKind::Bool,
                module,
            )
        }
        ArrayFilterCallbackShape::Float
            if module.array_layout(source) == ArrayLayout::Value
                && !array_filter_type_predicate_callback(callback)
                && array_filter_value_cells_are_floats(source, module) =>
        {
            emit_array_filter_value_scalar_predicate_local_assign(
                name,
                source,
                source_span,
                callback,
                ValueCellKind::Float,
                module,
            )
        }
        ArrayFilterCallbackShape::Float
            if module.array_layout(source) == ArrayLayout::Value
                && !array_filter_type_predicate_callback(callback)
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_filter_value_scalar_predicate_local_assign(
                name,
                source,
                source_span,
                callback,
                ValueCellKind::Str,
                module,
            )
        }
        ArrayFilterCallbackShape::Float
            if module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_floats(source, module) =>
        {
            emit_array_filter_value_scalar_type_local_assign(
                name,
                source,
                source_span,
                ValueCellKind::Float,
                module,
            )
        }
        _ if callback.eq_ignore_ascii_case("strlen")
            && module.array_layout(source) == ArrayLayout::Value
            && array_filter_value_cells_are_floats(source, module) =>
        {
            emit_array_filter_value_scalar_strlen_local_assign(
                name,
                source,
                source_span,
                ValueCellKind::Float,
                module,
            )
        }
        ArrayFilterCallbackShape::Numeric
            if module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_floats(source, module) =>
        {
            emit_array_filter_value_scalar_type_local_assign(
                name,
                source,
                source_span,
                ValueCellKind::Float,
                module,
            )
        }
        ArrayFilterCallbackShape::Null
            if module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_nulls(source, module) =>
        {
            emit_array_filter_value_scalar_type_local_assign(
                name,
                source,
                source_span,
                ValueCellKind::Null,
                module,
            )
        }
        _ if callback.eq_ignore_ascii_case("strlen")
            && module.array_layout(source) == ArrayLayout::Value
            && array_filter_value_cells_are_nulls(source, module) =>
        {
            emit_array_filter_value_scalar_strlen_local_assign(
                name,
                source,
                source_span,
                ValueCellKind::Null,
                module,
            )
        }
        ArrayFilterCallbackShape::Array
            if module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_arrays(source, module) =>
        {
            emit_array_filter_value_scalar_type_local_assign(
                name,
                source,
                source_span,
                ValueCellKind::Array,
                module,
            )
        }
        ArrayFilterCallbackShape::Object
            if module.array_layout(source) == ArrayLayout::Value
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_filter_default_value_object_local_assign(name, source, source_span, module)
        }
        _ if array_filter_object_false_callback(callback)
            && module.array_layout(source) == ArrayLayout::Value
            && module.array_object_classes(source).is_some() =>
        {
            emit_array_filter_empty_value_object_local_assign(name, source, source_span, module)
        }
        _ if module.array_layout(source) == ArrayLayout::Assoc
            && callback.eq_ignore_ascii_case("strlen")
            && array_filter_strlen_assoc_local_is_supported(source, module) =>
        {
            emit_array_filter_assoc_strlen_local_assign(name, source, source_span, module)
        }
        _ if module.array_layout(source) == ArrayLayout::Assoc
            && array_filter_assoc_local_matches_shape(source, shape, module) =>
        {
            emit_array_filter_assoc_local_assign(name, source, source_span, callback, shape, module)
        }
        _ if shape == ArrayFilterCallbackShape::Float
            && !array_filter_type_predicate_callback(callback)
            && module.array_layout(source) == ArrayLayout::Assoc
            && array_filter_assoc_local_has_string_values(source, module) =>
        {
            emit_array_filter_assoc_local_assign(name, source, source_span, callback, shape, module)
        }
        _ if module.array_layout(source) == ArrayLayout::Assoc
            && callback.eq_ignore_ascii_case("strlen")
            && array_filter_strlen_runtime_assoc_local_is_supported(source, module) =>
        {
            emit_array_filter_runtime_assoc_strlen_local_assign(name, source, source_span, module)
        }
        _ if module.array_layout(source) == ArrayLayout::Assoc
            && array_filter_runtime_assoc_local_matches_shape(source, shape, module) =>
        {
            emit_array_filter_runtime_assoc_local_assign(name, source, source_span, callback, shape, module)
        }
        _ if shape == ArrayFilterCallbackShape::Float
            && !array_filter_type_predicate_callback(callback)
            && module.array_layout(source) == ArrayLayout::Assoc
            && module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Str) =>
        {
            emit_array_filter_runtime_assoc_local_assign(name, source, source_span, callback, shape, module)
        }
        _ => Err(CompileError::new(
            source_span,
            "wasm32-web array_filter() direct array expressions require homogeneous int or string arrays",
        )),
    }
}

fn array_filter_assoc_local_has_string_values(source: &str, module: &WasmModule) -> bool {
    module.array_length(source).is_some()
        && module.array_value_cell_kinds(source).is_some_and(|kinds| {
            kinds.iter().all(|kind| *kind == ValueCellKind::Str)
        })
}

fn array_filter_unknown_mixed_callback_is_supported(callback: &str) -> bool {
    matches!(
        callback.to_ascii_lowercase().as_str(),
        "is_int" | "is_string" | "is_bool" | "is_float" | "is_null" | "is_numeric"
    )
}

fn array_filter_unknown_mixed_callback_result_kind(callback: &str) -> Option<ValueCellKind> {
    match callback.to_ascii_lowercase().as_str() {
        "is_int" => Some(ValueCellKind::Int),
        "is_string" => Some(ValueCellKind::Str),
        "is_bool" => Some(ValueCellKind::Bool),
        "is_float" => Some(ValueCellKind::Float),
        "is_null" => Some(ValueCellKind::Null),
        _ => None,
    }
}

fn emit_array_filter_unknown_mixed_callback_assign(
    name: &str,
    source: &str,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_filter_unknown_mixed_callback_source")
        .trim_start_matches('$')
        .to_string();
    let heap_kind = module.next_label("array_filter_unknown_mixed_callback_heap_kind");
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
    module.set_array_runtime_value_cell_kind(&temp, None);
    emit_array_filter_unknown_mixed_indexed_callback_assign(name, &temp, callback, module);
    module.body().close("else");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.set_array_layout(&temp, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(&temp, None);
    module.set_array_runtime_value_cell_kind(&temp, None);
    module.set_array_key_kinds(&temp, None);
    module.set_array_runtime_key_kind(&temp, None);
    module.set_array_php_normalized_runtime_keys(&temp, true);
    emit_array_filter_unknown_mixed_assoc_callback_assign(name, &temp, callback, module);
    module.body().close("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_array_filter_unknown_mixed_indexed_callback_assign(
    name: &str,
    source: &str,
    callback: &str,
    module: &mut WasmModule,
) {
    let out_index = emit_array_filter_runtime_result_prelude(name, source, module);
    let index = module.next_label("array_filter_unknown_mixed_index");
    let value_cell = module.next_label("array_filter_unknown_mixed_value_cell");
    let matches = module.next_label("array_filter_unknown_mixed_matches");
    for local in [&index, &value_cell, &matches] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.set_array_runtime_value_cell_kind(
        name,
        array_filter_unknown_mixed_callback_result_kind(callback),
    );
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    let loop_label = module.next_label("array_filter_unknown_mixed_loop");
    let done_label = module.next_label("array_filter_unknown_mixed_done");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module
        .body()
        .line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", value_cell));
    emit_array_filter_unknown_mixed_cell_predicate(&value_cell, callback, &matches, module);
    module.body().line(&format!("local.get {}", matches));
    module.body().open("if");
    emit_array_filter_copy_value_entry_dynamic_key(name, source, &out_index, &index, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_array_filter_unknown_mixed_assoc_callback_assign(
    name: &str,
    source: &str,
    callback: &str,
    module: &mut WasmModule,
) {
    let out_index = module.next_label("array_filter_unknown_mixed_assoc_out_index");
    let index = module.next_label("array_filter_unknown_mixed_assoc_index");
    let source_entry = module.next_label("array_filter_unknown_mixed_assoc_source_entry");
    let target_entry = module.next_label("array_filter_unknown_mixed_assoc_target_entry");
    let value_cell = module.next_label("array_filter_unknown_mixed_assoc_value_cell");
    let matches = module.next_label("array_filter_unknown_mixed_assoc_matches");
    for local in [&out_index, &index, &source_entry, &target_entry, &value_cell, &matches] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.clear_array_length(name);
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    module.set_array_runtime_key_kind(name, module.array_runtime_key_kind(source));
    module.set_array_php_normalized_runtime_keys(
        name,
        module.array_has_php_normalized_runtime_keys(source),
    );
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(
        name,
        array_filter_unknown_mixed_callback_result_kind(callback),
    );
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    let loop_label = module.next_label("array_filter_unknown_mixed_assoc_loop");
    let done_label = module.next_label("array_filter_unknown_mixed_assoc_done");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", source_entry));
    emit_assoc_value_cell_address(&source_entry, &value_cell, module);
    emit_array_filter_unknown_mixed_cell_predicate(&value_cell, callback, &matches, module);
    module.body().line(&format!("local.get {}", matches));
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    copy_assoc_entry(&target_entry, &source_entry, module);
    emit_array_filter_increment_len(name, &out_index, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_array_filter_unknown_mixed_cell_predicate(
    value_cell: &str,
    callback: &str,
    matches: &str,
    module: &mut WasmModule,
) {
    let tag = module.next_label("array_filter_unknown_mixed_tag");
    module.declare_i32_local(tag.trim_start_matches('$').to_string());
    emit_load_value_cell_ptr_tag(value_cell, &tag, module);
    match callback.to_ascii_lowercase().as_str() {
        "is_int" => emit_array_filter_tag_match(&tag, WASM_VALUE_TAG_INT, matches, module),
        "is_string" => emit_array_filter_tag_match(&tag, WASM_VALUE_TAG_STRING, matches, module),
        "is_bool" => emit_array_filter_tag_match(&tag, WASM_VALUE_TAG_BOOL, matches, module),
        "is_float" => emit_array_filter_tag_match(&tag, WASM_VALUE_TAG_FLOAT, matches, module),
        "is_null" => emit_array_filter_tag_match(&tag, WASM_VALUE_TAG_NULL, matches, module),
        "is_numeric" => emit_array_filter_unknown_mixed_numeric_match(value_cell, &tag, matches, module),
        _ => unreachable!("unknown mixed array_filter callback support was checked"),
    }
}

fn emit_array_filter_tag_match(tag: &str, expected_tag: i32, matches: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", expected_tag));
    module.body().line("i32.eq");
    module.body().line(&format!("local.set {}", matches));
}

fn emit_array_filter_unknown_mixed_numeric_match(
    value_cell: &str,
    tag: &str,
    matches: &str,
    module: &mut WasmModule,
) {
    let ptr = module.next_label("array_filter_unknown_mixed_numeric_ptr");
    let len = module.next_label("array_filter_unknown_mixed_numeric_len");
    for local in [&ptr, &len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", matches));
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_load_value_cell_ptr_string_part(value_cell, 8, &ptr, module);
    emit_load_value_cell_ptr_string_part(value_cell, 12, &len, module);
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", len));
    emit_stack_string_is_numeric(module);
    module.body().line(&format!("local.set {}", matches));
    module.body().close("end");
}
