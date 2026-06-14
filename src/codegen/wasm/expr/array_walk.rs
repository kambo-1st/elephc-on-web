//! Purpose:
//! Lowers WASM `array_walk()` calls over supported static and runtime array
//! shapes.
//!
//! Called from:
//! - `super::emit_expr()` for PHP builtin call lowering.
//!
//! Key details:
//! - Callback metadata determines whether values and keys are materialized, and
//!   unsupported dynamic callback shapes remain compile errors.

mod assoc;

use super::*;
use super::array_walk_value::*;
use assoc::{
    array_walk_assoc_local_matches_shape, array_walk_runtime_assoc_local_matches_shape,
    emit_array_walk_assoc_local, emit_array_walk_assoc_local_instance,
    emit_array_walk_runtime_assoc_local, emit_array_walk_runtime_assoc_local_instance,
};

pub(super) fn emit_array_walk_call(
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if args.len() != 2 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_walk() currently supports array and callback arguments only",
        ));
    }
    if let Some((callback, capture_local)) = instance_callback_for_array_walk(&args[1], module)? {
            let shape = array_walk_instance_callback_shape(&callback, args[1].span, module)?;
            if let ExprKind::Variable(source) = &args[0].kind {
                if shape.accepts_int_value()
                    && shape.accepts_indexed_key()
                    && module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::CompactInt
                {
                    emit_array_walk_compact_int_local_instance(
                        source,
                        args[0].span,
                        &callback,
                        &capture_local,
                        shape,
                        module,
                    )?;
                    module.body().line("i32.const 1");
                    return Ok(ValueKind::Bool);
                }
                if shape.accepts_string_value()
                    && shape.accepts_indexed_key()
                    && module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::Value
                    && (module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Str)
                        || module
                            .array_value_cell_kinds(source)
                            .is_some_and(|kinds| kinds.iter().all(|kind| *kind == ValueCellKind::Str)))
                {
                    emit_array_walk_value_string_local_instance(
                        source,
                        args[0].span,
                        &callback,
                        &capture_local,
                        shape,
                        module,
                    )?;
                    module.body().line("i32.const 1");
                    return Ok(ValueKind::Bool);
                }
                if shape.accepts_float_value()
                    && shape.accepts_indexed_key()
                    && module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::Value
                    && (module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Float)
                        || module.array_value_cell_kinds(source).is_some_and(|kinds| {
                            kinds.iter().all(|kind| *kind == ValueCellKind::Float)
                        }))
                {
                    emit_array_walk_value_float_local_instance(
                        source,
                        args[0].span,
                        &callback,
                        &capture_local,
                        shape,
                        module,
                    )?;
                    module.body().line("i32.const 1");
                    return Ok(ValueKind::Bool);
                }
                if shape.accepts_bool_value()
                    && shape.accepts_indexed_key()
                    && module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::Value
                    && (module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Bool)
                        || module.array_value_cell_kinds(source).is_some_and(|kinds| {
                            kinds.iter().all(|kind| *kind == ValueCellKind::Bool)
                        }))
                {
                    emit_array_walk_value_bool_local_instance(
                        source,
                        args[0].span,
                        &callback,
                        &capture_local,
                        shape,
                        module,
                    )?;
                    module.body().line("i32.const 1");
                    return Ok(ValueKind::Bool);
                }
                if module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::Assoc
                    && array_walk_assoc_local_matches_shape(source, shape, module)
                {
                    emit_array_walk_assoc_local_instance(
                        source,
                        args[0].span,
                        &callback,
                        &capture_local,
                        shape,
                        module,
                    )?;
                    module.body().line("i32.const 1");
                    return Ok(ValueKind::Bool);
                }
                if module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::Assoc
                    && array_walk_runtime_assoc_local_matches_shape(source, shape, module)
                {
                    emit_array_walk_runtime_assoc_local_instance(
                        source,
                        args[0].span,
                        &callback,
                        &capture_local,
                        shape,
                        module,
                    )?;
                    module.body().line("i32.const 1");
                    return Ok(ValueKind::Bool);
                }
            }
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_walk() instance callbacks currently require an assigned compact integer, scalar value-cell, or homogeneous associative array",
            ));
    }
    let Some(callback) = evaluated_static_callback_function_name(&args[1], module)? else {
        if emit_array_walk_static_callback_ternary_call(args, module)? {
            return Ok(ValueKind::Bool);
        }
        if emit_array_walk_dynamic_static_return_callback_call(args, module)? {
            return Ok(ValueKind::Bool);
        }
        if emit_array_walk_dynamic_callable_descriptor_call(args, module)? {
            return Ok(ValueKind::Bool);
        }
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web array_walk() currently requires a static string, direct first-class function callback, or simple callable variable alias",
        ));
    };
    if !module.has_function(&callback) {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web array_walk() currently requires a user-defined callback",
        ));
    }
    let shape = array_walk_callback_shape(&callback, args[1].span, module)?;

    match &args[0].kind {
        ExprKind::Variable(source)
            if shape.accepts_int_value()
                && shape.accepts_indexed_key()
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::CompactInt =>
        {
            emit_array_walk_compact_int_local(source, args[0].span, &callback, shape, module)?;
        }
        ExprKind::Variable(source)
            if shape.accepts_mixed_value()
                && shape.accepts_indexed_key()
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::CompactInt =>
        {
            emit_array_walk_mixed_compact_int_local(source, args[0].span, &callback, shape, module)?;
        }
        ExprKind::Variable(source)
            if shape.accepts_mixed_value()
                && shape.accepts_indexed_key()
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value =>
        {
            emit_array_walk_mixed_value_local(source, args[0].span, &callback, shape, module)?;
        }
        ExprKind::Variable(source)
            if shape.accepts_int_value()
                && shape.accepts_indexed_key()
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_ints(source, module) =>
        {
            emit_array_walk_value_int_local(source, args[0].span, &callback, shape, module)?;
        }
        ExprKind::Variable(source)
            if shape.accepts_int_value()
                && shape.accepts_indexed_key()
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_walk_value_numeric_string_as_int_local(source, args[0].span, &callback, shape, module)?;
        }
        ExprKind::Variable(source)
            if shape.accepts_float_value()
                && shape.accepts_indexed_key()
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_floats(source, module) =>
        {
            emit_array_walk_value_float_local(source, args[0].span, &callback, shape, module)?;
        }
        ExprKind::Variable(source)
            if shape.accepts_float_value()
                && shape.accepts_indexed_key()
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_walk_value_numeric_string_as_float_local(source, args[0].span, &callback, shape, module)?;
        }
        ExprKind::Variable(source)
            if shape.accepts_bool_value()
                && shape.accepts_indexed_key()
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_bools(source, module) =>
        {
            emit_array_walk_value_bool_local(source, args[0].span, &callback, shape, module)?;
        }
        ExprKind::Variable(source)
            if shape.accepts_string_value()
                && shape.accepts_indexed_key()
                && module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_walk_value_string_local(source, args[0].span, &callback, shape, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_walk_assoc_local_matches_shape(source, shape, module) =>
        {
            emit_array_walk_assoc_local(source, args[0].span, &callback, shape, module)?;
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_walk_runtime_assoc_local_matches_shape(source, shape, module) =>
        {
            emit_array_walk_runtime_assoc_local(source, args[0].span, &callback, shape, module)?;
        }
        ExprKind::FunctionCall { .. } | ExprKind::ExprCall { .. }
            if expression_has_array_type(&args[0], module) =>
        {
            emit_array_walk_array_expr(&args[0], args[0].span, &callback, shape, module)?;
        }
        ExprKind::MethodCall { object, method, .. }
        | ExprKind::NullsafeMethodCall { object, method, .. }
            if method_call_array_return_metadata(object, method, module).is_some() =>
        {
            emit_array_walk_array_expr(&args[0], args[0].span, &callback, shape, module)?;
        }
        ExprKind::StaticMethodCall { receiver, method, .. }
            if static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            emit_array_walk_array_expr(&args[0], args[0].span, &callback, shape, module)?;
        }
        _ => {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_walk() currently supports homogeneous int, float, bool, or string arrays",
            ));
        }
    }

    module.body().line("i32.const 1");
    Ok(ValueKind::Bool)
}

fn emit_array_walk_static_callback_ternary_call(
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
        if !module.has_function(callback) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web array_walk() callable ternary arms currently require user-defined callbacks",
            ));
        }
    }
    let then_shape = array_walk_callback_shape(&then_callback, then_expr.span, module)?;
    let else_shape = array_walk_callback_shape(&else_callback, else_expr.span, module)?;
    if then_shape != else_shape {
        return Err(CompileError::new(
            callback_expr.span,
            "wasm32-web array_walk() callable ternary arms currently require the same supported scalar callback shape",
        ));
    }
    let source_storage;
    let source = if let ExprKind::Variable(source) = &source_expr.kind {
        source
    } else if expression_has_array_type(source_expr, module) {
        source_storage = module
            .next_label("array_walk_ternary_source")
            .trim_start_matches('$')
            .to_string();
        module.declare_array_local(source_storage.clone());
        emit_array_assign(&source_storage, source_expr, module)?;
        &source_storage
    } else {
        return Ok(false);
    };
    if !array_walk_local_supports_shape(source, then_shape, module) {
        return Ok(false);
    }

    emit_condition(condition, module)?;
    module.body().open("if");
    emit_array_walk_supported_local(source, source_expr.span, &then_callback, then_shape, module)?;
    module.body().line("else");
    emit_array_walk_supported_local(source, source_expr.span, &else_callback, then_shape, module)?;
    module.body().close("end");
    module.body().line("i32.const 1");
    Ok(true)
}

fn instance_callback_for_array_walk(
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
        return capture_instance_callback_for_array_walk(callback, object, &method, module);
    }
    let ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) = &callback.kind else {
        if let Some((target, class_name)) = instance_callback_target(callback, "__invoke", module) {
            let capture_local = module
                .next_label("array_walk_invokable_object")
                .trim_start_matches('$')
                .to_string();
            module.declare_object_local(capture_local.clone());
            let kind = emit_expr(callback, module)?;
            if kind != ValueKind::Object {
                return Err(CompileError::new(
                    callback.span,
                    "wasm32-web array_walk() invokable callback requires an object receiver",
                ));
            }
            module.body().line(&format!("local.set ${}", capture_local));
            module.set_object_class_for_local(&capture_local, Some(class_name));
            return Ok(Some((target, capture_local)));
        }
        return Ok(None);
    };
    capture_instance_callback_for_array_walk(callback, object, method, module)
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

fn capture_instance_callback_for_array_walk(
    callback: &Expr,
    object: &Expr,
    method: &str,
    module: &mut WasmModule,
) -> Result<Option<(String, String)>, CompileError> {
    let capture_local = module
        .next_label("array_walk_callable_object")
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
            "wasm32-web array_walk() instance callback requires an object receiver",
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

fn array_walk_instance_callback_shape(
    callback: &str,
    span: crate::span::Span,
    module: &WasmModule,
) -> Result<ArrayWalkCallbackShape, CompileError> {
    let Some(param_kinds) = module.function_param_kinds(callback) else {
        return Err(CompileError::new(
            span,
            "wasm32-web array_walk() instance callback metadata is missing",
        ));
    };
    let callback_return = module.function_return_kind(callback);
    let walk_return_is_ignored =
        matches!(callback_return, Some(ValueKind::Int) | Some(ValueKind::Bool));
    match (param_kinds.as_slice(), walk_return_is_ignored) {
        ([LocalKind::Object, LocalKind::I64], true) => Ok(ArrayWalkCallbackShape::Int),
        ([LocalKind::Object, LocalKind::F64], true) => Ok(ArrayWalkCallbackShape::Float),
        ([LocalKind::Object, LocalKind::I32], true) => Ok(ArrayWalkCallbackShape::Bool),
        ([LocalKind::Object, LocalKind::Str], true) => Ok(ArrayWalkCallbackShape::Str),
        ([LocalKind::Object, LocalKind::I64, LocalKind::I64], true) => {
            Ok(ArrayWalkCallbackShape::IntWithIntKey)
        }
        ([LocalKind::Object, LocalKind::F64, LocalKind::I64], true) => {
            Ok(ArrayWalkCallbackShape::FloatWithIntKey)
        }
        ([LocalKind::Object, LocalKind::I32, LocalKind::I64], true) => {
            Ok(ArrayWalkCallbackShape::BoolWithIntKey)
        }
        ([LocalKind::Object, LocalKind::Str, LocalKind::I64], true) => {
            Ok(ArrayWalkCallbackShape::StrWithIntKey)
        }
        ([LocalKind::Object, LocalKind::I64, LocalKind::Str], true) => {
            Ok(ArrayWalkCallbackShape::IntWithStrKey)
        }
        ([LocalKind::Object, LocalKind::F64, LocalKind::Str], true) => {
            Ok(ArrayWalkCallbackShape::FloatWithStrKey)
        }
        ([LocalKind::Object, LocalKind::I32, LocalKind::Str], true) => {
            Ok(ArrayWalkCallbackShape::BoolWithStrKey)
        }
        ([LocalKind::Object, LocalKind::Str, LocalKind::Str], true) => {
            Ok(ArrayWalkCallbackShape::StrWithStrKey)
        }
        _ => Err(CompileError::new(
            span,
            "wasm32-web array_walk() instance callbacks currently require int/float/bool/string value or int/float/bool/string value with int/string key parameters",
        )),
    }
}

fn array_walk_callback_shape(
    callback: &str,
    span: crate::span::Span,
    module: &WasmModule,
) -> Result<ArrayWalkCallbackShape, CompileError> {
    let Some(param_kinds) = module.function_param_kinds(callback) else {
        return Err(CompileError::new(
            span,
            "wasm32-web array_walk() callback metadata is missing",
        ));
    };
    let callback_return = module.function_return_kind(callback);
    let walk_return_is_ignored =
        matches!(callback_return, Some(ValueKind::Int) | Some(ValueKind::Bool));
    match (param_kinds.as_slice(), walk_return_is_ignored) {
        ([LocalKind::I64], true) => Ok(ArrayWalkCallbackShape::Int),
        ([LocalKind::F64], true) => Ok(ArrayWalkCallbackShape::Float),
        ([LocalKind::I32], true) => Ok(ArrayWalkCallbackShape::Bool),
        ([LocalKind::Str], true) => Ok(ArrayWalkCallbackShape::Str),
        ([LocalKind::Mixed], true) => Ok(ArrayWalkCallbackShape::Mixed),
        ([LocalKind::I64, LocalKind::I64], true) => Ok(ArrayWalkCallbackShape::IntWithIntKey),
        ([LocalKind::F64, LocalKind::I64], true) => Ok(ArrayWalkCallbackShape::FloatWithIntKey),
        ([LocalKind::I32, LocalKind::I64], true) => Ok(ArrayWalkCallbackShape::BoolWithIntKey),
        ([LocalKind::Str, LocalKind::I64], true) => Ok(ArrayWalkCallbackShape::StrWithIntKey),
        ([LocalKind::Mixed, LocalKind::I64], true) => Ok(ArrayWalkCallbackShape::MixedWithIntKey),
        ([LocalKind::I64, LocalKind::Str], true) => Ok(ArrayWalkCallbackShape::IntWithStrKey),
        ([LocalKind::F64, LocalKind::Str], true) => Ok(ArrayWalkCallbackShape::FloatWithStrKey),
        ([LocalKind::I32, LocalKind::Str], true) => Ok(ArrayWalkCallbackShape::BoolWithStrKey),
        ([LocalKind::Str, LocalKind::Str], true) => Ok(ArrayWalkCallbackShape::StrWithStrKey),
        ([LocalKind::Mixed, LocalKind::Str], true) => Ok(ArrayWalkCallbackShape::MixedWithStrKey),
        _ => Err(CompileError::new(
            span,
            "wasm32-web array_walk() currently requires a one-scalar-argument callback or an indexed value/key callback",
        )),
    }
}

fn emit_array_walk_dynamic_static_return_callback_call(
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let [source_expr, callback_expr] = args else {
        return Ok(false);
    };
    let Some(callbacks) = dynamic_array_walk_callback_names(callback_expr, module) else {
        return Ok(false);
    };
    let source_storage;
    let source = if let ExprKind::Variable(source) = &source_expr.kind {
        source
    } else if expression_has_array_type(source_expr, module) {
        source_storage = module
            .next_label("array_walk_dynamic_source")
            .trim_start_matches('$')
            .to_string();
        module.declare_array_local(source_storage.clone());
        emit_array_assign(&source_storage, source_expr, module)?;
        &source_storage
    } else {
        return Ok(false);
    };
    if module.local_kind(source) != Some(LocalKind::Array) {
        return Ok(false);
    }
    for callback in &callbacks {
        if !module.has_function(callback) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic array_walk() callback helper can only return declared user functions",
            ));
        }
        let shape = array_walk_callback_shape(callback, callback_expr.span, module)?;
        if !array_walk_local_supports_shape(source, shape, module) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic array_walk() callback helper currently requires callbacks matching the source array values",
            ));
        }
    }

    let callback_ptr = module.next_label("array_walk_callback_ptr");
    let callback_len = module.next_label("array_walk_callback_len");
    let matched = module.next_label("array_walk_callback_matched");
    for local in [&callback_ptr, &callback_len, &matched] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_string_value_to_stack(callback_expr, module)?;
    module.body().line(&format!("local.set {}", callback_len));
    module.body().line(&format!("local.set {}", callback_ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));

    for callback in callbacks {
        let candidate_ptr = module.next_label("array_walk_callback_candidate_ptr");
        let candidate_len = module.next_label("array_walk_callback_candidate_len");
        let candidate_match = module.next_label("array_walk_callback_candidate_match");
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
        let shape = array_walk_callback_shape(&callback, callback_expr.span, module)?;
        emit_array_walk_supported_local(source, source_expr.span, &callback, shape, module)?;
        module.body().line("i32.const 1");
        module.body().line(&format!("local.set {}", matched));
        module.body().close("end");
    }

    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line("i32.const 1");
    Ok(true)
}

fn emit_array_walk_dynamic_callable_descriptor_call(
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let [source_expr, callback_expr] = args else {
        return Ok(false);
    };
    let Some(callbacks) = dynamic_array_walk_callable_descriptor_targets(callback_expr, module) else {
        return Ok(false);
    };
    let source_storage;
    let source = if let ExprKind::Variable(source) = &source_expr.kind {
        source
    } else if expression_has_array_type(source_expr, module) {
        source_storage = module
            .next_label("array_walk_callable_source")
            .trim_start_matches('$')
            .to_string();
        module.declare_array_local(source_storage.clone());
        emit_array_assign(&source_storage, source_expr, module)?;
        &source_storage
    } else {
        return Ok(false);
    };
    if module.local_kind(source) != Some(LocalKind::Array) {
        return Ok(false);
    }
    for callback in &callbacks {
        if !module.has_function(callback) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic array_walk() callable descriptor can only target declared user functions",
            ));
        }
        let shape = array_walk_callback_shape(callback, callback_expr.span, module)?;
        if !array_walk_local_supports_shape(source, shape, module) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic array_walk() callable descriptor currently requires callbacks matching the source array values",
            ));
        }
    }

    let callback_id = module.next_label("array_walk_callable_id");
    let matched = module.next_label("array_walk_callable_matched");
    for local in [&callback_id, &matched] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_array_walk_callable_descriptor(callback_expr, module)?;
    module.body().line(&format!("local.set {}", callback_id));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));

    for callback in callbacks {
        let target_id = module.callable_target_id(&callback);
        module.body().line(&format!("local.get {}", callback_id));
        module.body().line(&format!("i32.const {}", target_id));
        module.body().line("i32.eq");
        module.body().open("if");
        let shape = array_walk_callback_shape(&callback, callback_expr.span, module)?;
        emit_array_walk_supported_local(source, source_expr.span, &callback, shape, module)?;
        module.body().line("i32.const 1");
        module.body().line(&format!("local.set {}", matched));
        module.body().close("end");
    }

    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line("i32.const 1");
    Ok(true)
}

fn dynamic_array_walk_callback_names(expr: &Expr, module: &WasmModule) -> Option<Vec<String>> {
    match &expr.kind {
        ExprKind::FunctionCall { name, .. } => module
            .function_possible_static_string_returns(name.as_str())
            .map(<[_]>::to_vec),
        ExprKind::Variable(name) => module.possible_static_string_values(name).map(<[_]>::to_vec),
        _ => None,
    }
}

fn dynamic_array_walk_callable_descriptor_targets(
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

fn emit_array_walk_callable_descriptor(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if callable_return_expr_targets(expr, module).is_some() {
        let kind = emit_expr(expr, module)?;
        if kind != ValueKind::Callable {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web array_walk() callable descriptor expected a callable value",
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
            "wasm32-web array_walk() callable descriptor requires tracked callable metadata",
        )),
    }
}

fn array_walk_local_supports_shape(
    source: &str,
    shape: ArrayWalkCallbackShape,
    module: &WasmModule,
) -> bool {
    if module.local_kind(source) != Some(LocalKind::Array) {
        return false;
    }
    if shape.accepts_int_value()
        && shape.accepts_indexed_key()
        && module.array_layout(source) == ArrayLayout::CompactInt
    {
        return true;
    }
    if shape.accepts_mixed_value()
        && shape.accepts_indexed_key()
        && matches!(module.array_layout(source), ArrayLayout::CompactInt | ArrayLayout::Value)
    {
        return true;
    }
    if shape.accepts_int_value()
        && shape.accepts_indexed_key()
        && module.array_layout(source) == ArrayLayout::Value
        && (array_map_value_cells_are_ints(source, module) || array_map_value_cells_are_strings(source, module))
    {
        return true;
    }
    if shape.accepts_float_value()
        && shape.accepts_indexed_key()
        && module.array_layout(source) == ArrayLayout::Value
        && (array_filter_value_cells_are_floats(source, module)
            || array_map_value_cells_are_strings(source, module))
    {
        return true;
    }
    if shape.accepts_bool_value()
        && shape.accepts_indexed_key()
        && module.array_layout(source) == ArrayLayout::Value
        && array_filter_value_cells_are_bools(source, module)
    {
        return true;
    }
    if shape.accepts_string_value()
        && shape.accepts_indexed_key()
        && module.array_layout(source) == ArrayLayout::Value
        && array_map_value_cells_are_strings(source, module)
    {
        return true;
    }
    module.array_layout(source) == ArrayLayout::Assoc
        && (array_walk_assoc_local_matches_shape(source, shape, module)
            || array_walk_runtime_assoc_local_matches_shape(source, shape, module))
}

fn emit_array_walk_supported_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if shape.accepts_int_value()
        && shape.accepts_indexed_key()
        && module.array_layout(source) == ArrayLayout::CompactInt
    {
        return emit_array_walk_compact_int_local(source, source_span, callback, shape, module);
    }
    if shape.accepts_mixed_value()
        && shape.accepts_indexed_key()
        && module.array_layout(source) == ArrayLayout::CompactInt
    {
        return emit_array_walk_mixed_compact_int_local(source, source_span, callback, shape, module);
    }
    if shape.accepts_mixed_value()
        && shape.accepts_indexed_key()
        && module.array_layout(source) == ArrayLayout::Value
    {
        return emit_array_walk_mixed_value_local(source, source_span, callback, shape, module);
    }
    if shape.accepts_int_value()
        && shape.accepts_indexed_key()
        && module.array_layout(source) == ArrayLayout::Value
        && array_map_value_cells_are_ints(source, module)
    {
        return emit_array_walk_value_int_local(source, source_span, callback, shape, module);
    }
    if shape.accepts_int_value()
        && shape.accepts_indexed_key()
        && module.array_layout(source) == ArrayLayout::Value
        && array_map_value_cells_are_strings(source, module)
    {
        return emit_array_walk_value_numeric_string_as_int_local(source, source_span, callback, shape, module);
    }
    if shape.accepts_float_value()
        && shape.accepts_indexed_key()
        && module.array_layout(source) == ArrayLayout::Value
        && array_filter_value_cells_are_floats(source, module)
    {
        return emit_array_walk_value_float_local(source, source_span, callback, shape, module);
    }
    if shape.accepts_float_value()
        && shape.accepts_indexed_key()
        && module.array_layout(source) == ArrayLayout::Value
        && array_map_value_cells_are_strings(source, module)
    {
        return emit_array_walk_value_numeric_string_as_float_local(source, source_span, callback, shape, module);
    }
    if shape.accepts_bool_value()
        && shape.accepts_indexed_key()
        && module.array_layout(source) == ArrayLayout::Value
        && array_filter_value_cells_are_bools(source, module)
    {
        return emit_array_walk_value_bool_local(source, source_span, callback, shape, module);
    }
    if shape.accepts_string_value()
        && shape.accepts_indexed_key()
        && module.array_layout(source) == ArrayLayout::Value
        && array_map_value_cells_are_strings(source, module)
    {
        return emit_array_walk_value_string_local(source, source_span, callback, shape, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_walk_assoc_local_matches_shape(source, shape, module)
    {
        return emit_array_walk_assoc_local(source, source_span, callback, shape, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_walk_runtime_assoc_local_matches_shape(source, shape, module)
    {
        return emit_array_walk_runtime_assoc_local(source, source_span, callback, shape, module);
    }
    Err(CompileError::new(
        source_span,
        "wasm32-web dynamic array_walk() callback helper currently requires callbacks matching the source array values",
    ))
}

fn emit_array_walk_array_expr(
    source_expr: &Expr,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_walk_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_array_assign(&temp, source_expr, module)?;
    if shape == ArrayWalkCallbackShape::Int
        && module.array_layout(&temp) == ArrayLayout::CompactInt
    {
        return emit_array_walk_compact_int_local(&temp, source_span, callback, shape, module);
    }
    if shape.accepts_int_value()
        && shape.accepts_indexed_key()
        && module.array_layout(&temp) == ArrayLayout::CompactInt
    {
        return emit_array_walk_compact_int_local(&temp, source_span, callback, shape, module);
    }
    if shape.accepts_mixed_value()
        && shape.accepts_indexed_key()
        && module.array_layout(&temp) == ArrayLayout::CompactInt
    {
        return emit_array_walk_mixed_compact_int_local(&temp, source_span, callback, shape, module);
    }
    if shape.accepts_mixed_value()
        && shape.accepts_indexed_key()
        && module.array_layout(&temp) == ArrayLayout::Value
    {
        return emit_array_walk_mixed_value_local(&temp, source_span, callback, shape, module);
    }
    if shape.accepts_int_value()
        && shape.accepts_indexed_key()
        && module.array_layout(&temp) == ArrayLayout::Value
        && array_map_value_cells_are_ints(&temp, module)
    {
        return emit_array_walk_value_int_local(&temp, source_span, callback, shape, module);
    }
    if shape.accepts_int_value()
        && shape.accepts_indexed_key()
        && module.array_layout(&temp) == ArrayLayout::Value
        && array_map_value_cells_are_strings(&temp, module)
    {
        return emit_array_walk_value_numeric_string_as_int_local(&temp, source_span, callback, shape, module);
    }
    if shape.accepts_float_value()
        && shape.accepts_indexed_key()
        && module.array_layout(&temp) == ArrayLayout::Value
        && array_filter_value_cells_are_floats(&temp, module)
    {
        return emit_array_walk_value_float_local(&temp, source_span, callback, shape, module);
    }
    if shape.accepts_float_value()
        && shape.accepts_indexed_key()
        && module.array_layout(&temp) == ArrayLayout::Value
        && array_map_value_cells_are_strings(&temp, module)
    {
        return emit_array_walk_value_numeric_string_as_float_local(&temp, source_span, callback, shape, module);
    }
    if shape.accepts_bool_value()
        && shape.accepts_indexed_key()
        && module.array_layout(&temp) == ArrayLayout::Value
        && array_filter_value_cells_are_bools(&temp, module)
    {
        return emit_array_walk_value_bool_local(&temp, source_span, callback, shape, module);
    }
    if shape.accepts_string_value()
        && shape.accepts_indexed_key()
        && module.array_layout(&temp) == ArrayLayout::Value
        && array_map_value_cells_are_strings(&temp, module)
    {
        return emit_array_walk_value_string_local(&temp, source_span, callback, shape, module);
    }
    if module.array_layout(&temp) == ArrayLayout::Assoc
        && array_walk_assoc_local_matches_shape(&temp, shape, module)
    {
        return emit_array_walk_assoc_local(&temp, source_span, callback, shape, module);
    }
    if module.array_layout(&temp) == ArrayLayout::Assoc
        && array_walk_runtime_assoc_local_matches_shape(&temp, shape, module)
    {
        return emit_array_walk_runtime_assoc_local(&temp, source_span, callback, shape, module);
    }
    if is_direct_array_keys_expr(source_expr)
        && shape.accepts_string_value()
        && shape.accepts_indexed_key()
        && module.array_layout(&temp) == ArrayLayout::Value
    {
        return emit_array_walk_value_dynamic_string_local(&temp, callback, shape, module);
    }
    Err(CompileError::new(
        source_span,
        "wasm32-web array_walk() over direct array expressions requires homogeneous int, float, bool, or string arrays",
    ))
}

fn is_direct_array_keys_expr(expr: &Expr) -> bool {
    matches!(
        &expr.kind,
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("array_keys")
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ArrayWalkCallbackShape {
    Int,
    Float,
    Bool,
    Str,
    Mixed,
    IntWithIntKey,
    FloatWithIntKey,
    BoolWithIntKey,
    StrWithIntKey,
    MixedWithIntKey,
    IntWithStrKey,
    FloatWithStrKey,
    BoolWithStrKey,
    StrWithStrKey,
    MixedWithStrKey,
}

impl ArrayWalkCallbackShape {
    fn accepts_int_value(self) -> bool {
        matches!(self, Self::Int | Self::IntWithIntKey | Self::IntWithStrKey)
    }

    fn accepts_string_value(self) -> bool {
        matches!(self, Self::Str | Self::StrWithIntKey | Self::StrWithStrKey)
    }

    fn accepts_float_value(self) -> bool {
        matches!(self, Self::Float | Self::FloatWithIntKey | Self::FloatWithStrKey)
    }

    fn accepts_bool_value(self) -> bool {
        matches!(self, Self::Bool | Self::BoolWithIntKey | Self::BoolWithStrKey)
    }

    fn accepts_mixed_value(self) -> bool {
        matches!(self, Self::Mixed | Self::MixedWithIntKey | Self::MixedWithStrKey)
    }

    fn accepts_indexed_key(self) -> bool {
        !self.needs_string_key()
    }

    fn needs_int_key(self) -> bool {
        matches!(
            self,
            Self::IntWithIntKey
                | Self::FloatWithIntKey
                | Self::BoolWithIntKey
                | Self::StrWithIntKey
                | Self::MixedWithIntKey
        )
    }

    fn needs_string_key(self) -> bool {
        matches!(
            self,
            Self::IntWithStrKey
                | Self::FloatWithStrKey
                | Self::BoolWithStrKey
                | Self::StrWithStrKey
                | Self::MixedWithStrKey
        )
    }
}

fn emit_array_walk_mixed_compact_int_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::CompactInt {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_walk() mixed callbacks over compact arrays require integer storage",
        ));
    }
    let mixed_arg = module.next_label("array_walk_mixed_compact_arg");
    module.declare_i32_local(mixed_arg.trim_start_matches('$').to_string());
    emit_alloc_mixed_cell(mixed_arg.trim_start_matches('$'), module);
    if let Some(len) = module.array_length(source) {
        for index in 0..len {
            module.body().line(&format!("local.get {}", mixed_arg));
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index * 8));
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("call $__rt_value_store_int");
            module.body().line(&format!("local.get {}", mixed_arg));
            emit_array_walk_static_int_key_arg(index, shape, module);
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(callback)));
            module.body().line("drop");
        }
    } else {
        emit_array_walk_runtime_mixed_compact_int_local(source, callback, shape, &mixed_arg, module);
    }
    module.body().line(&format!("local.get {}", mixed_arg));
    module.body().line("call $__rt_value_release");
    Ok(())
}

fn emit_array_walk_runtime_mixed_compact_int_local(
    source: &str,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    mixed_arg: &str,
    module: &mut WasmModule,
) {
    let index = module.next_label("array_walk_mixed_compact_index");
    let done_label = module.next_label("array_walk_mixed_compact_done");
    let loop_label = module.next_label("array_walk_mixed_compact_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", mixed_arg));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("call $__rt_value_store_int");
    module.body().line(&format!("local.get {}", mixed_arg));
    emit_array_walk_runtime_int_key_arg(&index, shape, module);
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("drop");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_array_walk_static_int_key_arg(
    index: usize,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) {
    if shape.needs_int_key() {
        module.body().line(&format!("i64.const {}", index));
    }
}

pub(super) fn emit_array_walk_runtime_int_key_arg(
    index: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) {
    if shape.needs_int_key() {
        module.body().line(&format!("local.get {}", index));
        module.body().line("i64.extend_i32_u");
    }
}

fn emit_array_walk_compact_int_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_walk_runtime_compact_int_local(source, source_span, callback, shape, module);
    };
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line("i64.load");
        emit_array_walk_static_int_key_arg(index, shape, module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("drop");
    }
    Ok(())
}

fn emit_array_walk_compact_int_local_instance(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_array_walk_runtime_compact_int_local_instance(
            source,
            source_span,
            callback,
            capture_local,
            shape,
            module,
        );
    };
    for index in 0..len {
        module.body().line(&format!("local.get ${}", capture_local));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line("i64.load");
        emit_array_walk_static_int_key_arg(index, shape, module);
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(callback)));
        module.body().line("drop");
    }
    Ok(())
}

fn emit_array_walk_runtime_compact_int_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::CompactInt {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_walk() over runtime-length arrays currently requires compact integer storage",
        ));
    }
    let index = module.next_label("array_walk_index");
    let done_label = module.next_label("array_walk_done");
    let loop_label = module.next_label("array_walk_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    emit_array_walk_runtime_int_key_arg(&index, shape, module);
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("drop");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_array_walk_runtime_compact_int_local_instance(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    shape: ArrayWalkCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::CompactInt {
        return Err(CompileError::new(
            source_span,
            "wasm32-web array_walk() over runtime-length arrays currently requires compact integer storage",
        ));
    }
    let index = module.next_label("array_walk_instance_index");
    let done_label = module.next_label("array_walk_instance_done");
    let loop_label = module.next_label("array_walk_instance_loop");
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
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    emit_array_walk_runtime_int_key_arg(&index, shape, module);
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("drop");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}
