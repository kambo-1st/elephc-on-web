//! Purpose:
//! Lowers WASM callback-based array sorting builtins (`usort`, `uasort`, and
//! `uksort`) over supported indexed and associative array shapes.
//!
//! Called from:
//! - `super::emit_expr()` for PHP builtin call lowering.
//!
//! Key details:
//! - Sorting mutates array payloads after ensuring copy-on-write uniqueness, and
//!   callback signatures decide whether integer or string values/keys are valid.

use super::*;
use super::array_sort_usort::*;
use super::array_sort_uasort::*;
use super::array_sort_uksort::*;
use super::array_value_cells::emit_ensure_unique_array_payload;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortValueCallbackShape {
    Int,
    Float,
    String,
    Bool,
}

fn invalidate_assoc_sort_key_values(source: &str, module: &mut WasmModule) {
    let runtime_key_kind = module.array_runtime_key_kind(source);
    let homogeneous_static_key_kind = module
        .array_key_kinds(source)
        .and_then(|kinds| kinds.first().copied().filter(|first| kinds.iter().all(|kind| kind == first)));
    module.set_array_key_values(source, None);
    if let Some(kind) = runtime_key_kind.or(homogeneous_static_key_kind) {
        module.set_array_runtime_key_kind(source, Some(kind));
    } else {
        module.set_array_key_kinds(source, None);
    }
}

fn instance_callback_for_array_sort(
    callback: &Expr,
    label_prefix: &str,
    builtin: &str,
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
        return capture_instance_callback_for_array_sort(
            callback,
            object,
            &method,
            label_prefix,
            builtin,
            module,
        );
    }
    if let ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) = &callback.kind
    {
        return capture_instance_callback_for_array_sort(
            callback,
            object,
            method,
            label_prefix,
            builtin,
            module,
        );
    };
    let Some((target, class_name)) = instance_callback_target(callback, "__invoke", module) else {
        return Ok(None);
    };
    let capture_local = module.next_label(label_prefix).trim_start_matches('$').to_string();
    module.declare_object_local(capture_local.clone());
    let kind = emit_expr(callback, module)?;
    if kind != ValueKind::Object {
        return Err(CompileError::new(
            callback.span,
            &format!("wasm32-web {builtin} instance callback requires an object receiver"),
        ));
    }
    module.body().line(&format!("local.set ${}", capture_local));
    module.set_object_class_for_local(&capture_local, Some(class_name));
    Ok(Some((target, capture_local)))
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

fn capture_instance_callback_for_array_sort(
    callback: &Expr,
    object: &Expr,
    method: &str,
    label_prefix: &str,
    builtin: &str,
    module: &mut WasmModule,
) -> Result<Option<(String, String)>, CompileError> {
    let Some((target, class_name)) = instance_callback_target(object, method, module) else {
        return Ok(None);
    };
    let capture_local = module.next_label(label_prefix).trim_start_matches('$').to_string();
    module.declare_object_local(capture_local.clone());
    let kind = emit_expr(object, module)?;
    if kind != ValueKind::Object {
        return Err(CompileError::new(
            callback.span,
            &format!("wasm32-web {builtin} instance callback requires an object receiver"),
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
    Some((method_info.symbol.clone(), class_name))
}

pub(super) fn emit_usort_call(
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if args.len() != 2 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web usort() currently supports array and callback arguments only",
        ));
    }
    let ExprKind::Variable(source) = &args[0].kind else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web usort() currently requires an assigned array variable",
        ));
    };
    if let Some((callback, capture_local)) =
        instance_callback_for_array_sort(&args[1], "usort_callable_object", "usort()", module)?
    {
            let Some(param_kinds) = module.function_param_kinds(&callback) else {
                return Err(CompileError::new(
                    args[1].span,
                    "wasm32-web usort() instance callback metadata is missing",
                ));
            };
            let callback_takes_ints =
                param_kinds.as_slice() == [LocalKind::Object, LocalKind::I64, LocalKind::I64];
            let callback_takes_floats =
                param_kinds.as_slice() == [LocalKind::Object, LocalKind::F64, LocalKind::F64];
            let callback_takes_strings =
                param_kinds.as_slice() == [LocalKind::Object, LocalKind::Str, LocalKind::Str];
            let callback_takes_bools =
                param_kinds.as_slice() == [LocalKind::Object, LocalKind::I32, LocalKind::I32];
            if (!callback_takes_ints && !callback_takes_floats && !callback_takes_strings && !callback_takes_bools)
                || module.function_return_kind(&callback) != Some(ValueKind::Int)
            {
                return Err(CompileError::new(
                    args[1].span,
                    "wasm32-web usort() instance callbacks currently require two int, two float, two string, or two bool arguments and int return",
                ));
            }
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::CompactInt
                && callback_takes_ints
            {
                emit_usort_compact_int_local_instance(
                    source,
                    args[0].span,
                    &callback,
                    &capture_local,
                    module,
                )?;
                module.body().line("i32.const 1");
                return Ok(ValueKind::Bool);
            }
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && callback_takes_floats
                && array_filter_value_cells_are_floats(source, module)
            {
                emit_usort_value_float_local_instance(
                    source,
                    args[0].span,
                    &callback,
                    &capture_local,
                    module,
                )?;
                module.body().line("i32.const 1");
                return Ok(ValueKind::Bool);
            }
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && callback_takes_strings
                && array_map_value_cells_are_strings(source, module)
            {
                emit_usort_value_string_local_instance(
                    source,
                    args[0].span,
                    &callback,
                    &capture_local,
                    module,
                )?;
                module.body().line("i32.const 1");
                return Ok(ValueKind::Bool);
            }
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && callback_takes_bools
                && array_filter_value_cells_are_bools(source, module)
            {
                emit_usort_value_bool_local_instance(
                    source,
                    args[0].span,
                    &callback,
                    &capture_local,
                    module,
                )?;
                module.body().line("i32.const 1");
                return Ok(ValueKind::Bool);
            }
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web usort() instance callbacks currently require an assigned compact integer array or float/string/bool value-cell array",
            ));
    }
    let Some(callback) = evaluated_static_callback_function_name(&args[1], module)? else {
        if emit_usort_static_callback_ternary_call(source, &args[1], args[0].span, module)? {
            return Ok(ValueKind::Bool);
        }
        if emit_usort_dynamic_static_return_callback_call(source, &args[1], args[0].span, module)?
        {
            return Ok(ValueKind::Bool);
        }
        if emit_usort_dynamic_callable_descriptor_call(source, &args[1], args[0].span, module)? {
            return Ok(ValueKind::Bool);
        }
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web usort() currently requires a static string, direct first-class function callback, or simple callable variable alias",
        ));
    };
    if !module.has_function(&callback) {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web usort() currently requires a user-defined callback",
        ));
    }
    let Some(param_kinds) = module.function_param_kinds(&callback) else {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web usort() callback metadata is missing",
        ));
    };
    let callback_takes_ints = param_kinds.as_slice() == [LocalKind::I64, LocalKind::I64];
    let callback_takes_floats = param_kinds.as_slice() == [LocalKind::F64, LocalKind::F64];
    let callback_takes_strings = param_kinds.as_slice() == [LocalKind::Str, LocalKind::Str];
    let callback_takes_bools = param_kinds.as_slice() == [LocalKind::I32, LocalKind::I32];
    if (!callback_takes_ints && !callback_takes_floats && !callback_takes_strings && !callback_takes_bools)
        || module.function_return_kind(&callback) != Some(ValueKind::Int)
    {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web usort() currently requires a two-int, two-float, two-string, or two-bool argument int callback",
        ));
    }
    if module.local_kind(source) != Some(LocalKind::Array) {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web usort() currently requires an array variable",
        ));
    }
    match module.array_layout(source) {
        ArrayLayout::CompactInt => emit_usort_compact_int_local(source, args[0].span, &callback, module)?,
        ArrayLayout::Value if callback_takes_ints && array_map_value_cells_are_ints(source, module) => {
            emit_usort_value_int_local(source, args[0].span, &callback, module)?;
        }
        ArrayLayout::Value if callback_takes_floats && array_filter_value_cells_are_floats(source, module) => {
            emit_usort_value_float_local(source, args[0].span, &callback, module)?;
        }
        ArrayLayout::Value if callback_takes_strings && array_map_value_cells_are_strings(source, module) => {
            emit_usort_value_string_local(source, args[0].span, &callback, module)?;
        }
        ArrayLayout::Value if callback_takes_bools && array_filter_value_cells_are_bools(source, module) => {
            emit_usort_value_bool_local(source, args[0].span, &callback, module)?;
        }
        _ => {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web usort() currently supports integer, float, string, or bool arrays",
            ));
        }
    }

    module.body().line("i32.const 1");
    Ok(ValueKind::Bool)
}

fn emit_usort_static_callback_ternary_call(
    source: &str,
    callback_expr: &Expr,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
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
                "wasm32-web usort() callable ternary arms currently require user-defined callbacks",
            ));
        }
    }
    let then_shape = usort_callback_shape(&then_callback, then_expr.span, module)?;
    let else_shape = usort_callback_shape(&else_callback, else_expr.span, module)?;
    if then_shape != else_shape {
        return Err(CompileError::new(
            callback_expr.span,
            "wasm32-web usort() callable ternary arms currently require the same supported scalar callback shape",
        ));
    }
    if !usort_local_supports_shape(source, then_shape, module) {
        return Ok(false);
    }

    emit_condition(condition, module)?;
    module.body().open("if");
    emit_usort_supported_local(source, source_span, &then_callback, then_shape, module)?;
    module.body().line("else");
    emit_usort_supported_local(source, source_span, &else_callback, then_shape, module)?;
    module.body().close("end");
    module.body().line("i32.const 1");
    Ok(true)
}

fn usort_callback_shape(
    callback: &str,
    span: crate::span::Span,
    module: &WasmModule,
) -> Result<SortValueCallbackShape, CompileError> {
    let Some(param_kinds) = module.function_param_kinds(callback) else {
        return Err(CompileError::new(
            span,
            "wasm32-web usort() callable ternary callback metadata is missing",
        ));
    };
    if module.function_return_kind(callback) != Some(ValueKind::Int) {
        return Err(CompileError::new(
            span,
            "wasm32-web usort() callable ternary callbacks currently require int return",
        ));
    }
    match param_kinds.as_slice() {
        [LocalKind::I64, LocalKind::I64] => Ok(SortValueCallbackShape::Int),
        [LocalKind::F64, LocalKind::F64] => Ok(SortValueCallbackShape::Float),
        [LocalKind::Str, LocalKind::Str] => Ok(SortValueCallbackShape::String),
        [LocalKind::I32, LocalKind::I32] => Ok(SortValueCallbackShape::Bool),
        _ => Err(CompileError::new(
            span,
            "wasm32-web usort() callable ternary callbacks currently require two int, two float, two string, or two bool arguments",
        )),
    }
}

fn usort_local_supports_shape(
    source: &str,
    shape: SortValueCallbackShape,
    module: &WasmModule,
) -> bool {
    if module.local_kind(source) != Some(LocalKind::Array) {
        return false;
    }
    match (module.array_layout(source), shape) {
        (ArrayLayout::CompactInt, SortValueCallbackShape::Int) => true,
        (ArrayLayout::Value, SortValueCallbackShape::Int) => {
            array_map_value_cells_are_ints(source, module)
        }
        (ArrayLayout::Value, SortValueCallbackShape::Float) => {
            array_filter_value_cells_are_floats(source, module)
        }
        (ArrayLayout::Value, SortValueCallbackShape::String) => {
            array_map_value_cells_are_strings(source, module)
        }
        (ArrayLayout::Value, SortValueCallbackShape::Bool) => {
            array_filter_value_cells_are_bools(source, module)
        }
        _ => false,
    }
}

fn emit_usort_supported_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    shape: SortValueCallbackShape,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match (module.array_layout(source), shape) {
        (ArrayLayout::CompactInt, SortValueCallbackShape::Int) => {
            emit_usort_compact_int_local(source, source_span, callback, module)
        }
        (ArrayLayout::Value, SortValueCallbackShape::Int) => {
            emit_usort_value_int_local(source, source_span, callback, module)
        }
        (ArrayLayout::Value, SortValueCallbackShape::Float) => {
            emit_usort_value_float_local(source, source_span, callback, module)
        }
        (ArrayLayout::Value, SortValueCallbackShape::String) => {
            emit_usort_value_string_local(source, source_span, callback, module)
        }
        (ArrayLayout::Value, SortValueCallbackShape::Bool) => {
            emit_usort_value_bool_local(source, source_span, callback, module)
        }
        _ => Err(CompileError::new(
            source_span,
            "wasm32-web usort() callable ternary currently supports integer, float, string, or bool arrays",
        )),
    }
}

fn emit_usort_dynamic_static_return_callback_call(
    source: &str,
    callback_expr: &Expr,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let Some(callbacks) = dynamic_sort_callback_names(callback_expr, module) else {
        return Ok(false);
    };
    if module.local_kind(source) != Some(LocalKind::Array) {
        return Ok(false);
    }
    let layout = module.array_layout(source);
    let value_kinds = module.array_value_cell_kinds(source);
    let runtime_value_kind = module.array_runtime_value_cell_kind(source);
    let supported_compact_ints = layout == ArrayLayout::CompactInt;
    let supported_value_ints = layout == ArrayLayout::Value
        && (value_kinds
            .as_ref()
            .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Int)))
            || runtime_value_kind == Some(ValueCellKind::Int));
    let supported_value_floats = layout == ArrayLayout::Value
        && (value_kinds
            .as_ref()
            .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Float)))
            || runtime_value_kind == Some(ValueCellKind::Float));
    let supported_value_strings = layout == ArrayLayout::Value
        && (value_kinds
            .as_ref()
            .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Str)))
            || runtime_value_kind == Some(ValueCellKind::Str));
    let supported_value_bools = layout == ArrayLayout::Value
        && (value_kinds
            .as_ref()
            .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Bool)))
            || runtime_value_kind == Some(ValueCellKind::Bool));
    if !supported_compact_ints
        && !supported_value_ints
        && !supported_value_floats
        && !supported_value_strings
        && !supported_value_bools
    {
        return Ok(false);
    }
    for callback in &callbacks {
        if !module.has_function(callback) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic usort() callback helper can only return declared user functions",
            ));
        }
        let Some(param_kinds) = module.function_param_kinds(callback) else {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic usort() callback helper metadata is missing",
            ));
        };
        let expected_params = if supported_value_strings {
            [LocalKind::Str, LocalKind::Str]
        } else if supported_value_floats {
            [LocalKind::F64, LocalKind::F64]
        } else if supported_value_bools {
            [LocalKind::I32, LocalKind::I32]
        } else {
            [LocalKind::I64, LocalKind::I64]
        };
        if param_kinds.as_slice() != expected_params
            || module.function_return_kind(callback) != Some(ValueKind::Int)
        {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic usort() callback helper currently requires value-matching comparator callbacks",
            ));
        }
    }

    let callback_ptr = module.next_label("usort_callback_ptr");
    let callback_len = module.next_label("usort_callback_len");
    let matched = module.next_label("usort_callback_matched");
    for local in [&callback_ptr, &callback_len, &matched] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_string_value_to_stack(callback_expr, module)?;
    module.body().line(&format!("local.set {}", callback_len));
    module.body().line(&format!("local.set {}", callback_ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));

    for callback in callbacks {
        let candidate_ptr = module.next_label("usort_callback_candidate_ptr");
        let candidate_len = module.next_label("usort_callback_candidate_len");
        let candidate_match = module.next_label("usort_callback_candidate_match");
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
        if supported_value_strings {
            emit_usort_value_string_local(source, source_span, &callback, module)?;
        } else if supported_value_floats {
            emit_usort_value_float_local(source, source_span, &callback, module)?;
        } else if supported_value_bools {
            emit_usort_value_bool_local(source, source_span, &callback, module)?;
        } else if supported_value_ints {
            emit_usort_value_int_local(source, source_span, &callback, module)?;
        } else {
            emit_usort_compact_int_local(source, source_span, &callback, module)?;
        }
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

fn emit_usort_dynamic_callable_descriptor_call(
    source: &str,
    callback_expr: &Expr,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let Some(callbacks) = dynamic_sort_callable_descriptor_targets(callback_expr, module) else {
        return Ok(false);
    };
    if module.local_kind(source) != Some(LocalKind::Array) {
        return Ok(false);
    }
    let mut callback_shape = None;
    for callback in &callbacks {
        if !module.has_function(callback) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic usort() callable descriptor can only target declared user functions",
            ));
        }
        let shape = usort_callback_shape(callback, callback_expr.span, module)?;
        if callback_shape.is_some_and(|existing| existing != shape) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic usort() callable descriptor currently requires callbacks with matching comparator signatures",
            ));
        }
        callback_shape = Some(shape);
    }
    let Some(callback_shape) = callback_shape else {
        return Ok(false);
    };
    if !usort_local_supports_shape(source, callback_shape, module) {
        return Ok(false);
    }

    let callback_id = module.next_label("usort_callable_id");
    let matched = module.next_label("usort_callable_matched");
    for local in [&callback_id, &matched] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_sort_callable_descriptor(callback_expr, module)?;
    module.body().line(&format!("local.set {}", callback_id));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));

    for callback in callbacks {
        let target_id = module.callable_target_id(&callback);
        module.body().line(&format!("local.get {}", callback_id));
        module.body().line(&format!("i32.const {}", target_id));
        module.body().line("i32.eq");
        module.body().open("if");
        emit_usort_supported_local(source, source_span, &callback, callback_shape, module)?;
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

fn dynamic_sort_callback_names(expr: &Expr, module: &WasmModule) -> Option<Vec<String>> {
    match &expr.kind {
        ExprKind::FunctionCall { name, .. } => module
            .function_possible_static_string_returns(name.as_str())
            .map(<[_]>::to_vec),
        ExprKind::Variable(name) => module.possible_static_string_values(name).map(<[_]>::to_vec),
        _ => None,
    }
}

fn dynamic_sort_callable_descriptor_targets(
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

fn emit_sort_callable_descriptor(
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
            "wasm32-web sort callable descriptor requires tracked callable metadata",
        )),
    }
}

pub(super) fn emit_uasort_call(
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if args.len() != 2 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web uasort() currently supports array and callback arguments only",
        ));
    }
    let ExprKind::Variable(source) = &args[0].kind else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web uasort() currently requires an assigned array variable",
        ));
    };
    if let Some((callback, capture_local)) =
        instance_callback_for_array_sort(&args[1], "uasort_callable_object", "uasort()", module)?
    {
            let Some(param_kinds) = module.function_param_kinds(&callback) else {
                return Err(CompileError::new(
                    args[1].span,
                    "wasm32-web uasort() instance callback metadata is missing",
                ));
            };
            let callback_takes_ints =
                param_kinds.as_slice() == [LocalKind::Object, LocalKind::I64, LocalKind::I64];
            let callback_takes_floats =
                param_kinds.as_slice() == [LocalKind::Object, LocalKind::F64, LocalKind::F64];
            let callback_takes_strings =
                param_kinds.as_slice() == [LocalKind::Object, LocalKind::Str, LocalKind::Str];
            let callback_takes_bools =
                param_kinds.as_slice() == [LocalKind::Object, LocalKind::I32, LocalKind::I32];
            if (!callback_takes_ints && !callback_takes_floats && !callback_takes_strings && !callback_takes_bools)
                || module.function_return_kind(&callback) != Some(ValueKind::Int)
            {
                return Err(CompileError::new(
                    args[1].span,
                    "wasm32-web uasort() instance callbacks currently require two int, two float, two string, or two bool arguments and int return",
                ));
            }
            if module.local_kind(source) != Some(LocalKind::Array)
                || module.array_layout(source) != ArrayLayout::Assoc
            {
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web uasort() instance callbacks currently require an associative array variable",
                ));
            }
            let value_kinds = module.array_value_cell_kinds(source);
            let runtime_value_kind = module.array_runtime_value_cell_kind(source);
            let supported_values = if callback_takes_strings {
                value_kinds
                    .as_ref()
                    .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Str)))
                    || runtime_value_kind == Some(ValueCellKind::Str)
            } else if callback_takes_floats {
                value_kinds
                    .as_ref()
                    .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Float)))
                    || runtime_value_kind == Some(ValueCellKind::Float)
            } else if callback_takes_bools {
                value_kinds
                    .as_ref()
                    .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Bool)))
                    || runtime_value_kind == Some(ValueCellKind::Bool)
            } else {
                value_kinds
                    .as_ref()
                    .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Int)))
                    || runtime_value_kind == Some(ValueCellKind::Int)
            };
            if !supported_values {
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web uasort() instance callbacks currently require matching integer, float, string, or bool associative values",
                ));
            }
            emit_ensure_unique_array_payload(source, module);
            if let Some(len) = module.array_length(source) {
                for pass in 0..len {
                    for index in 0..len.saturating_sub(1 + pass) {
                        if callback_takes_strings {
                            emit_uasort_string_compare_swap_instance(
                                source,
                                index,
                                &callback,
                                &capture_local,
                                module,
                            );
                        } else if callback_takes_floats {
                            emit_uasort_float_compare_swap_instance(
                                source,
                                index,
                                &callback,
                                &capture_local,
                                module,
                            );
                        } else if callback_takes_bools {
                            emit_uasort_bool_compare_swap_instance(
                                source,
                                index,
                                &callback,
                                &capture_local,
                                module,
                            );
                        } else {
                            emit_uasort_int_compare_swap_instance(
                                source,
                                index,
                                &callback,
                                &capture_local,
                                module,
                            );
                        }
                    }
                }
            } else if callback_takes_strings {
                emit_uasort_runtime_string_compare_sort_instance(
                    source,
                    &callback,
                    &capture_local,
                    module,
                );
            } else if callback_takes_floats {
                emit_uasort_runtime_float_compare_sort_instance(
                    source,
                    &callback,
                    &capture_local,
                    module,
                );
            } else if callback_takes_bools {
                emit_uasort_runtime_bool_compare_sort_instance(
                    source,
                    &callback,
                    &capture_local,
                    module,
                );
            } else {
                emit_uasort_runtime_int_compare_sort_instance(
                    source,
                    &callback,
                    &capture_local,
                    module,
                );
            }
            invalidate_assoc_sort_key_values(source, module);
            module.body().line("i32.const 1");
            return Ok(ValueKind::Bool);
    }
    let Some(callback) = evaluated_static_callback_function_name(&args[1], module)? else {
        if emit_uasort_static_callback_ternary_call(source, &args[1], module)? {
            return Ok(ValueKind::Bool);
        }
        if emit_uasort_dynamic_static_return_callback_call(source, &args[1], module)? {
            return Ok(ValueKind::Bool);
        }
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web uasort() currently requires a static string, direct first-class function callback, or simple callable variable alias",
        ));
    };
    if !module.has_function(&callback) {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web uasort() currently requires a user-defined callback",
        ));
    }
    let Some(param_kinds) = module.function_param_kinds(&callback) else {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web uasort() callback metadata is missing",
        ));
    };
    let callback_takes_ints = param_kinds.as_slice() == [LocalKind::I64, LocalKind::I64];
    let callback_takes_floats = param_kinds.as_slice() == [LocalKind::F64, LocalKind::F64];
    let callback_takes_strings = param_kinds.as_slice() == [LocalKind::Str, LocalKind::Str];
    let callback_takes_bools = param_kinds.as_slice() == [LocalKind::I32, LocalKind::I32];
    if (!callback_takes_ints && !callback_takes_floats && !callback_takes_strings && !callback_takes_bools)
        || module.function_return_kind(&callback) != Some(ValueKind::Int)
    {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web uasort() currently requires a two-int, two-float, two-string, or two-bool argument int callback",
        ));
    }
    if module.local_kind(source) != Some(LocalKind::Array)
        || module.array_layout(source) != ArrayLayout::Assoc
    {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web uasort() currently requires an associative array variable",
        ));
    }
    let value_kinds = module.array_value_cell_kinds(source);
    let runtime_value_kind = module.array_runtime_value_cell_kind(source);
    let supported_values = if callback_takes_ints {
        value_kinds
            .as_ref()
            .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Int)))
            || runtime_value_kind == Some(ValueCellKind::Int)
    } else if callback_takes_floats {
        value_kinds
            .as_ref()
            .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Float)))
            || runtime_value_kind == Some(ValueCellKind::Float)
    } else if callback_takes_strings {
        value_kinds
            .as_ref()
            .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Str)))
            || runtime_value_kind == Some(ValueCellKind::Str)
    } else {
        value_kinds
            .as_ref()
            .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Bool)))
            || runtime_value_kind == Some(ValueCellKind::Bool)
    };
    if !supported_values
    {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web uasort() currently supports integer, float, string, or bool associative values",
        ));
    }
    emit_ensure_unique_array_payload(source, module);
    if let Some(len) = module.array_length(source) {
        for pass in 0..len {
            for index in 0..len.saturating_sub(1 + pass) {
                if callback_takes_strings {
                    emit_uasort_string_compare_swap(source, index, &callback, module);
                } else if callback_takes_floats {
                    emit_uasort_float_compare_swap(source, index, &callback, module);
                } else if callback_takes_bools {
                    emit_uasort_bool_compare_swap(source, index, &callback, module);
                } else {
                    emit_uasort_int_compare_swap(source, index, &callback, module);
                }
            }
        }
    } else if callback_takes_strings {
        emit_uasort_runtime_string_compare_sort(source, &callback, module);
    } else if callback_takes_floats {
        emit_uasort_runtime_float_compare_sort(source, &callback, module);
    } else if callback_takes_bools {
        emit_uasort_runtime_bool_compare_sort(source, &callback, module);
    } else {
        emit_uasort_runtime_int_compare_sort(source, &callback, module);
    }
    invalidate_assoc_sort_key_values(source, module);
    module.body().line("i32.const 1");
    Ok(ValueKind::Bool)
}

fn emit_uasort_static_callback_ternary_call(
    source: &str,
    callback_expr: &Expr,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
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
                "wasm32-web uasort() callable ternary arms currently require user-defined callbacks",
            ));
        }
    }
    let then_shape = usort_callback_shape(&then_callback, then_expr.span, module)?;
    let else_shape = usort_callback_shape(&else_callback, else_expr.span, module)?;
    if then_shape != else_shape {
        return Err(CompileError::new(
            callback_expr.span,
            "wasm32-web uasort() callable ternary arms currently require the same supported scalar callback shape",
        ));
    }
    if !uasort_local_supports_shape(source, then_shape, module) {
        return Ok(false);
    }

    emit_condition(condition, module)?;
    module.body().open("if");
    emit_uasort_supported_local(source, &then_callback, then_shape, module);
    module.body().line("else");
    emit_uasort_supported_local(source, &else_callback, then_shape, module);
    module.body().close("end");
    invalidate_assoc_sort_key_values(source, module);
    module.body().line("i32.const 1");
    Ok(true)
}

fn uasort_local_supports_shape(
    source: &str,
    shape: SortValueCallbackShape,
    module: &WasmModule,
) -> bool {
    if module.local_kind(source) != Some(LocalKind::Array)
        || module.array_layout(source) != ArrayLayout::Assoc
    {
        return false;
    }
    let value_kinds = module.array_value_cell_kinds(source);
    let runtime_value_kind = module.array_runtime_value_cell_kind(source);
    match shape {
        SortValueCallbackShape::Int => {
            value_kinds
                .as_ref()
                .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Int)))
                || runtime_value_kind == Some(ValueCellKind::Int)
        }
        SortValueCallbackShape::Float => {
            value_kinds
                .as_ref()
                .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Float)))
                || runtime_value_kind == Some(ValueCellKind::Float)
        }
        SortValueCallbackShape::String => {
            value_kinds
                .as_ref()
                .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Str)))
                || runtime_value_kind == Some(ValueCellKind::Str)
        }
        SortValueCallbackShape::Bool => {
            value_kinds
                .as_ref()
                .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Bool)))
                || runtime_value_kind == Some(ValueCellKind::Bool)
        }
    }
}

fn emit_uasort_supported_local(
    source: &str,
    callback: &str,
    shape: SortValueCallbackShape,
    module: &mut WasmModule,
) {
    emit_ensure_unique_array_payload(source, module);
    if let Some(len) = module.array_length(source) {
        for pass in 0..len {
            for index in 0..len.saturating_sub(1 + pass) {
                match shape {
                    SortValueCallbackShape::String => {
                        emit_uasort_string_compare_swap(source, index, callback, module);
                    }
                    SortValueCallbackShape::Float => {
                        emit_uasort_float_compare_swap(source, index, callback, module);
                    }
                    SortValueCallbackShape::Bool => {
                        emit_uasort_bool_compare_swap(source, index, callback, module);
                    }
                    SortValueCallbackShape::Int => {
                        emit_uasort_int_compare_swap(source, index, callback, module);
                    }
                }
            }
        }
    } else {
        match shape {
            SortValueCallbackShape::String => {
                emit_uasort_runtime_string_compare_sort(source, callback, module);
            }
            SortValueCallbackShape::Float => {
                emit_uasort_runtime_float_compare_sort(source, callback, module);
            }
            SortValueCallbackShape::Bool => {
                emit_uasort_runtime_bool_compare_sort(source, callback, module);
            }
            SortValueCallbackShape::Int => {
                emit_uasort_runtime_int_compare_sort(source, callback, module);
            }
        }
    }
}

fn emit_uasort_dynamic_static_return_callback_call(
    source: &str,
    callback_expr: &Expr,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let Some(callbacks) = dynamic_sort_callback_names(callback_expr, module) else {
        return Ok(false);
    };
    if module.local_kind(source) != Some(LocalKind::Array)
        || module.array_layout(source) != ArrayLayout::Assoc
    {
        return Ok(false);
    }
    let value_kinds = module.array_value_cell_kinds(source);
    let runtime_value_kind = module.array_runtime_value_cell_kind(source);
    let supported_int_values = value_kinds
        .as_ref()
        .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Int)))
        || runtime_value_kind == Some(ValueCellKind::Int);
    let supported_float_values = value_kinds
        .as_ref()
        .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Float)))
        || runtime_value_kind == Some(ValueCellKind::Float);
    let supported_string_values = value_kinds
        .as_ref()
        .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Str)))
        || runtime_value_kind == Some(ValueCellKind::Str);
    let supported_bool_values = value_kinds
        .as_ref()
        .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Bool)))
        || runtime_value_kind == Some(ValueCellKind::Bool);
    if !supported_int_values && !supported_float_values && !supported_string_values && !supported_bool_values {
        return Ok(false);
    }
    for callback in &callbacks {
        if !module.has_function(callback) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic uasort() callback helper can only return declared user functions",
            ));
        }
        let Some(param_kinds) = module.function_param_kinds(callback) else {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic uasort() callback helper metadata is missing",
            ));
        };
        let expected_params = if supported_string_values {
            [LocalKind::Str, LocalKind::Str]
        } else if supported_float_values {
            [LocalKind::F64, LocalKind::F64]
        } else if supported_bool_values {
            [LocalKind::I32, LocalKind::I32]
        } else {
            [LocalKind::I64, LocalKind::I64]
        };
        if param_kinds.as_slice() != expected_params
            || module.function_return_kind(callback) != Some(ValueKind::Int)
        {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic uasort() callback helper currently requires value-matching comparator callbacks",
            ));
        }
    }

    let callback_ptr = module.next_label("uasort_callback_ptr");
    let callback_len = module.next_label("uasort_callback_len");
    let matched = module.next_label("uasort_callback_matched");
    for local in [&callback_ptr, &callback_len, &matched] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_string_value_to_stack(callback_expr, module)?;
    module.body().line(&format!("local.set {}", callback_len));
    module.body().line(&format!("local.set {}", callback_ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    emit_ensure_unique_array_payload(source, module);

    for callback in callbacks {
        let candidate_ptr = module.next_label("uasort_callback_candidate_ptr");
        let candidate_len = module.next_label("uasort_callback_candidate_len");
        let candidate_match = module.next_label("uasort_callback_candidate_match");
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
        if let Some(len) = module.array_length(source) {
            for pass in 0..len {
                for index in 0..len.saturating_sub(1 + pass) {
                    if supported_string_values {
                        emit_uasort_string_compare_swap(source, index, &callback, module);
                    } else if supported_float_values {
                        emit_uasort_float_compare_swap(source, index, &callback, module);
                    } else if supported_bool_values {
                        emit_uasort_bool_compare_swap(source, index, &callback, module);
                    } else {
                        emit_uasort_int_compare_swap(source, index, &callback, module);
                    }
                }
            }
        } else if supported_string_values {
            emit_uasort_runtime_string_compare_sort(source, &callback, module);
        } else if supported_float_values {
            emit_uasort_runtime_float_compare_sort(source, &callback, module);
        } else if supported_bool_values {
            emit_uasort_runtime_bool_compare_sort(source, &callback, module);
        } else {
            emit_uasort_runtime_int_compare_sort(source, &callback, module);
        }
        module.body().line("i32.const 1");
        module.body().line(&format!("local.set {}", matched));
        module.body().close("end");
    }

    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    invalidate_assoc_sort_key_values(source, module);
    module.body().line("i32.const 1");
    Ok(true)
}

pub(super) fn emit_uksort_call(
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if args.len() != 2 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web uksort() currently supports array and callback arguments only",
        ));
    }
    let ExprKind::Variable(source) = &args[0].kind else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web uksort() currently requires an assigned array variable",
        ));
    };
    if let Some((callback, capture_local)) =
        instance_callback_for_array_sort(&args[1], "uksort_callable_object", "uksort()", module)?
    {
            let Some(param_kinds) = module.function_param_kinds(&callback) else {
                return Err(CompileError::new(
                    args[1].span,
                    "wasm32-web uksort() instance callback metadata is missing",
                ));
            };
            let callback_takes_ints =
                param_kinds.as_slice() == [LocalKind::Object, LocalKind::I64, LocalKind::I64];
            let callback_takes_strings =
                param_kinds.as_slice() == [LocalKind::Object, LocalKind::Str, LocalKind::Str];
            if (!callback_takes_ints && !callback_takes_strings)
                || module.function_return_kind(&callback) != Some(ValueKind::Int)
            {
                return Err(CompileError::new(
                    args[1].span,
                    "wasm32-web uksort() instance callbacks currently require two int or two string arguments and int return",
                ));
            }
            if module.local_kind(source) != Some(LocalKind::Array)
                || module.array_layout(source) != ArrayLayout::Assoc
            {
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web uksort() instance callbacks currently require an associative array variable",
                ));
            }
            let key_kinds = module.array_key_kinds(source);
            let runtime_key_kind = module.array_runtime_key_kind(source);
            let supported_keys = if callback_takes_strings {
                key_kinds
                    .as_ref()
                    .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, AssocKeyKind::Str)))
                    || runtime_key_kind == Some(AssocKeyKind::Str)
            } else {
                key_kinds
                    .as_ref()
                    .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, AssocKeyKind::Int)))
                    || runtime_key_kind == Some(AssocKeyKind::Int)
            };
            if !supported_keys {
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web uksort() instance callbacks currently require matching integer or string associative keys",
                ));
            }
            emit_ensure_unique_array_payload(source, module);
            if let Some(len) = module.array_length(source) {
                for pass in 0..len {
                    for index in 0..len.saturating_sub(1 + pass) {
                        if callback_takes_strings {
                            emit_uksort_string_compare_swap_instance(
                                source,
                                index,
                                &callback,
                                &capture_local,
                                module,
                            );
                        } else {
                            emit_uksort_int_compare_swap_instance(
                                source,
                                index,
                                &callback,
                                &capture_local,
                                module,
                            );
                        }
                    }
                }
            } else if callback_takes_strings {
                emit_uksort_runtime_string_compare_sort_instance(
                    source,
                    &callback,
                    &capture_local,
                    module,
                );
            } else {
                emit_uksort_runtime_int_compare_sort_instance(
                    source,
                    &callback,
                    &capture_local,
                    module,
                );
            }
            invalidate_assoc_sort_key_values(source, module);
            module.body().line("i32.const 1");
            return Ok(ValueKind::Bool);
    }
    let Some(callback) = evaluated_static_callback_function_name(&args[1], module)? else {
        if emit_uksort_static_callback_ternary_call(source, &args[1], module)? {
            return Ok(ValueKind::Bool);
        }
        if emit_uksort_dynamic_static_return_callback_call(source, &args[1], module)? {
            return Ok(ValueKind::Bool);
        }
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web uksort() currently requires a static string, direct first-class function callback, or simple callable variable alias",
        ));
    };
    if !module.has_function(&callback) {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web uksort() currently requires a user-defined callback",
        ));
    }
    let Some(param_kinds) = module.function_param_kinds(&callback) else {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web uksort() callback metadata is missing",
        ));
    };
    let callback_takes_ints = param_kinds.as_slice() == [LocalKind::I64, LocalKind::I64];
    let callback_takes_floats = param_kinds.as_slice() == [LocalKind::F64, LocalKind::F64];
    let callback_takes_strings = param_kinds.as_slice() == [LocalKind::Str, LocalKind::Str];
    if (!callback_takes_ints && !callback_takes_floats && !callback_takes_strings)
        || module.function_return_kind(&callback) != Some(ValueKind::Int)
    {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web uksort() currently requires a two-int, two-float, or two-string argument int callback",
        ));
    }
    if module.local_kind(source) != Some(LocalKind::Array)
        || module.array_layout(source) != ArrayLayout::Assoc
    {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web uksort() currently requires an associative array variable",
        ));
    }
    let key_kinds = module.array_key_kinds(source);
    let runtime_key_kind = module.array_runtime_key_kind(source);
    let supported_keys = if callback_takes_ints || callback_takes_floats {
        key_kinds
            .as_ref()
            .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, AssocKeyKind::Int)))
            || runtime_key_kind == Some(AssocKeyKind::Int)
    } else {
        key_kinds
            .as_ref()
            .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, AssocKeyKind::Str)))
            || runtime_key_kind == Some(AssocKeyKind::Str)
    };
    if !supported_keys
    {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web uksort() currently supports integer or string associative keys",
        ));
    }
    emit_ensure_unique_array_payload(source, module);
    if let Some(len) = module.array_length(source) {
        for pass in 0..len {
            for index in 0..len.saturating_sub(1 + pass) {
                if callback_takes_strings {
                    emit_uksort_string_compare_swap(source, index, &callback, module);
                } else if callback_takes_floats {
                    emit_uksort_float_compare_swap(source, index, &callback, module);
                } else {
                    emit_uksort_int_compare_swap(source, index, &callback, module);
                }
            }
        }
    } else if callback_takes_strings {
        emit_uksort_runtime_string_compare_sort(source, &callback, module);
    } else if callback_takes_floats {
        emit_uksort_runtime_float_compare_sort(source, &callback, module);
    } else {
        emit_uksort_runtime_int_compare_sort(source, &callback, module);
    }
    invalidate_assoc_sort_key_values(source, module);
    module.body().line("i32.const 1");
    Ok(ValueKind::Bool)
}

fn emit_uksort_static_callback_ternary_call(
    source: &str,
    callback_expr: &Expr,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
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
                "wasm32-web uksort() callable ternary arms currently require user-defined callbacks",
            ));
        }
    }
    let then_shape = usort_callback_shape(&then_callback, then_expr.span, module)?;
    let else_shape = usort_callback_shape(&else_callback, else_expr.span, module)?;
    if then_shape != else_shape {
        return Err(CompileError::new(
            callback_expr.span,
            "wasm32-web uksort() callable ternary arms currently require the same supported key callback shape",
        ));
    }
    if !uksort_local_supports_shape(source, then_shape, module) {
        return Ok(false);
    }

    emit_condition(condition, module)?;
    module.body().open("if");
    emit_uksort_supported_local(source, &then_callback, then_shape, module);
    module.body().line("else");
    emit_uksort_supported_local(source, &else_callback, then_shape, module);
    module.body().close("end");
    invalidate_assoc_sort_key_values(source, module);
    module.body().line("i32.const 1");
    Ok(true)
}

fn uksort_local_supports_shape(
    source: &str,
    shape: SortValueCallbackShape,
    module: &WasmModule,
) -> bool {
    if module.local_kind(source) != Some(LocalKind::Array)
        || module.array_layout(source) != ArrayLayout::Assoc
    {
        return false;
    }
    let key_kinds = module.array_key_kinds(source);
    let runtime_key_kind = module.array_runtime_key_kind(source);
    match shape {
        SortValueCallbackShape::Int | SortValueCallbackShape::Float => {
            key_kinds
                .as_ref()
                .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, AssocKeyKind::Int)))
                || runtime_key_kind == Some(AssocKeyKind::Int)
        }
        SortValueCallbackShape::String => {
            key_kinds
                .as_ref()
                .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, AssocKeyKind::Str)))
                || runtime_key_kind == Some(AssocKeyKind::Str)
        }
        SortValueCallbackShape::Bool => false,
    }
}

fn emit_uksort_supported_local(
    source: &str,
    callback: &str,
    shape: SortValueCallbackShape,
    module: &mut WasmModule,
) {
    emit_ensure_unique_array_payload(source, module);
    if let Some(len) = module.array_length(source) {
        for pass in 0..len {
            for index in 0..len.saturating_sub(1 + pass) {
                match shape {
                    SortValueCallbackShape::String => {
                        emit_uksort_string_compare_swap(source, index, callback, module);
                    }
                    SortValueCallbackShape::Float => {
                        emit_uksort_float_compare_swap(source, index, callback, module);
                    }
                    SortValueCallbackShape::Int => {
                        emit_uksort_int_compare_swap(source, index, callback, module);
                    }
                    SortValueCallbackShape::Bool => {}
                }
            }
        }
    } else {
        match shape {
            SortValueCallbackShape::String => {
                emit_uksort_runtime_string_compare_sort(source, callback, module);
            }
            SortValueCallbackShape::Float => {
                emit_uksort_runtime_float_compare_sort(source, callback, module);
            }
            SortValueCallbackShape::Int => {
                emit_uksort_runtime_int_compare_sort(source, callback, module);
            }
            SortValueCallbackShape::Bool => {}
        }
    }
}

fn emit_uksort_dynamic_static_return_callback_call(
    source: &str,
    callback_expr: &Expr,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let Some(callbacks) = dynamic_sort_callback_names(callback_expr, module) else {
        return Ok(false);
    };
    if module.local_kind(source) != Some(LocalKind::Array)
        || module.array_layout(source) != ArrayLayout::Assoc
    {
        return Ok(false);
    }
    let key_kinds = module.array_key_kinds(source);
    let runtime_key_kind = module.array_runtime_key_kind(source);
    let supported_int_keys = key_kinds
        .as_ref()
        .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, AssocKeyKind::Int)))
        || runtime_key_kind == Some(AssocKeyKind::Int);
    let supported_string_keys = key_kinds
        .as_ref()
        .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, AssocKeyKind::Str)))
        || runtime_key_kind == Some(AssocKeyKind::Str);
    if !supported_int_keys && !supported_string_keys {
        return Ok(false);
    }
    for callback in &callbacks {
        if !module.has_function(callback) {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic uksort() callback helper can only return declared user functions",
            ));
        }
        let Some(param_kinds) = module.function_param_kinds(callback) else {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic uksort() callback helper metadata is missing",
            ));
        };
        let expected_params = if supported_string_keys {
            [LocalKind::Str, LocalKind::Str]
        } else {
            [LocalKind::I64, LocalKind::I64]
        };
        if param_kinds.as_slice() != expected_params
            || module.function_return_kind(callback) != Some(ValueKind::Int)
        {
            return Err(CompileError::new(
                callback_expr.span,
                "wasm32-web dynamic uksort() callback helper currently requires key-matching comparator callbacks",
            ));
        }
    }

    let callback_ptr = module.next_label("uksort_callback_ptr");
    let callback_len = module.next_label("uksort_callback_len");
    let matched = module.next_label("uksort_callback_matched");
    for local in [&callback_ptr, &callback_len, &matched] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_string_value_to_stack(callback_expr, module)?;
    module.body().line(&format!("local.set {}", callback_len));
    module.body().line(&format!("local.set {}", callback_ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    emit_ensure_unique_array_payload(source, module);

    for callback in callbacks {
        let candidate_ptr = module.next_label("uksort_callback_candidate_ptr");
        let candidate_len = module.next_label("uksort_callback_candidate_len");
        let candidate_match = module.next_label("uksort_callback_candidate_match");
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
        if let Some(len) = module.array_length(source) {
            for pass in 0..len {
                for index in 0..len.saturating_sub(1 + pass) {
                    if supported_string_keys {
                        emit_uksort_string_compare_swap(source, index, &callback, module);
                    } else {
                        emit_uksort_int_compare_swap(source, index, &callback, module);
                    }
                }
            }
        } else if supported_string_keys {
            emit_uksort_runtime_string_compare_sort(source, &callback, module);
        } else {
            emit_uksort_runtime_int_compare_sort(source, &callback, module);
        }
        module.body().line("i32.const 1");
        module.body().line(&format!("local.set {}", matched));
        module.body().close("end");
    }

    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    invalidate_assoc_sort_key_values(source, module);
    module.body().line("i32.const 1");
    Ok(true)
}
