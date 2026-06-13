//! Purpose:
//! Infers local result kinds for dynamic callable helpers in wasm metadata.
//! Keeps `call_user_func*` argument reconstruction out of the main local collector.
//!
//! Called from:
//! - `super::infer_assignment_fallback_local_kind()`.
//!
//! Key details:
//! - Named arguments from packed arrays are normalized through the shared call planner.
//! - The result kind is inferred by rebuilding a normal call expression once the target is known.

use super::*;

pub(super) fn call_user_func_local_kind(
    args: &[Expr],
    locals: &HashMap<String, LocalKind>,
    callable_targets: &HashMap<String, String>,
    string_static_values: &HashMap<String, String>,
    function_possible_static_string_returns: &HashMap<String, Vec<String>>,
    function_return_kinds: &HashMap<String, ValueKind>,
    constants: &HashMap<String, ConstantValue>,
    class_constants: &HashMap<String, ConstantValue>,
) -> Option<LocalKind> {
    let callback = args.first()?;
    let call_args = args[1..].to_vec();
    if let Some(kind) = dynamic_call_user_function_local_kind(
        callback,
        function_possible_static_string_returns,
        string_static_values,
        function_return_kinds,
    ) {
        return Some(kind);
    }
    call_user_function_local_kind(
        callback,
        call_args,
        locals,
        callable_targets,
        string_static_values,
        function_return_kinds,
        constants,
        class_constants,
    )
}

pub(super) fn call_user_func_array_local_kind(
    args: &[Expr],
    locals: &HashMap<String, LocalKind>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    callable_targets: &HashMap<String, String>,
    string_static_values: &HashMap<String, String>,
    function_return_kinds: &HashMap<String, ValueKind>,
    constants: &HashMap<String, ConstantValue>,
    class_constants: &HashMap<String, ConstantValue>,
) -> Option<LocalKind> {
    let [callback, packed_args] = args else {
        return None;
    };
    let (call_args, has_named_arg) = match &packed_args.kind {
        ExprKind::ArrayLiteral(call_args) => (call_args.clone(), false),
        ExprKind::ArrayLiteralAssoc(items) => {
            call_user_func_array_assoc_literal_args_for_kind(items, packed_args.span, constants)?
        }
        ExprKind::FunctionCall { name, args }
            if matches!(name.to_ascii_lowercase().as_str(), "range" | "array_fill") =>
        {
            call_user_func_array_direct_builder_args_for_kind(
                name.as_str(),
                args,
                packed_args.span,
                constants,
            )?
        }
        ExprKind::Variable(name)
            if locals.get(name) == Some(&LocalKind::Array) && array_key_values.contains_key(name) =>
        {
            call_user_func_array_assoc_local_args_for_kind(
                name,
                array_key_values.get(name)?,
                packed_args.span,
            )
        }
        ExprKind::Variable(name) if locals.get(name) == Some(&LocalKind::Array) => (Vec::new(), false),
        _ => return None,
    };
    if let Some(kind) = instance_callable_method_local_kind(callback, function_return_kinds) {
        return Some(kind);
    }
    if let Some(kind) = static_callable_ternary_local_kind(callback, function_return_kinds) {
        return Some(kind);
    }
    let target =
        call_user_function_target_for_kind(callback, callable_targets, string_static_values, constants)?;
    let call_args = if has_named_arg
        && !function_return_kinds.contains_key(&function_key(&target))
        && instance_method_from_callable_symbol(&target).is_none()
    {
        normalize_builtin_named_call_args_for_kind(&target, &call_args, packed_args.span)?
    } else {
        call_args
    };
    call_user_function_local_kind(
        callback,
        call_args,
        locals,
        callable_targets,
        string_static_values,
        function_return_kinds,
        constants,
        class_constants,
    )
}

fn call_user_func_array_assoc_literal_args_for_kind(
    items: &[(Expr, Expr)],
    span: crate::span::Span,
    constants: &HashMap<String, ConstantValue>,
) -> Option<(Vec<Expr>, bool)> {
    let mut has_named_arg = false;
    let mut call_args = Vec::with_capacity(items.len());
    for (key, value) in items {
        if static_or_const_int_value_for_metadata(key, constants).is_some() {
            call_args.push(value.clone());
            continue;
        }
        let name = static_string_for_metadata(key, constants)?;
        has_named_arg = true;
        call_args.push(Expr::new(
            ExprKind::NamedArg {
                name,
                value: Box::new(value.clone()),
            },
            span,
        ));
    }
    Some((call_args, has_named_arg))
}

fn call_user_func_array_direct_builder_args_for_kind(
    name: &str,
    args: &[Expr],
    span: crate::span::Span,
    constants: &HashMap<String, ConstantValue>,
) -> Option<(Vec<Expr>, bool)> {
    if name.eq_ignore_ascii_case("range") {
        return static_int_range_args_for_kind(args, span, constants).map(|items| (items, false));
    }
    if !name.eq_ignore_ascii_case("array_fill") || args.len() != 3 {
        return None;
    }
    static_array_fill_args_for_kind(args, constants).map(|items| (items, false))
}

fn static_int_range_args_for_kind(
    args: &[Expr],
    span: crate::span::Span,
    constants: &HashMap<String, ConstantValue>,
) -> Option<Vec<Expr>> {
    if !(2..=3).contains(&args.len()) {
        return None;
    }
    let start = static_or_const_int_value_for_metadata(&args[0], constants)?;
    let end = static_or_const_int_value_for_metadata(&args[1], constants)?;
    let step = if let Some(step_arg) = args.get(2) {
        static_or_const_int_value_for_metadata(step_arg, constants)?.abs()
    } else {
        1
    };
    if step == 0 {
        return None;
    }
    let mut values = Vec::new();
    let mut current = start;
    if start <= end {
        while current <= end {
            values.push(Expr::new(ExprKind::IntLiteral(current), span));
            let next = current.saturating_add(step);
            if next <= current {
                break;
            }
            current = next;
        }
    } else {
        while current >= end {
            values.push(Expr::new(ExprKind::IntLiteral(current), span));
            let next = current.saturating_sub(step);
            if next >= current {
                break;
            }
            current = next;
        }
    }
    Some(values)
}

fn static_array_fill_args_for_kind(
    args: &[Expr],
    constants: &HashMap<String, ConstantValue>,
) -> Option<Vec<Expr>> {
    let count = static_or_const_int_value_for_metadata(&args[1], constants)?;
    let count = usize::try_from(count).ok()?;
    array_fill_value_is_static_scalar_for_kind(&args[2]).then(|| vec![args[2].clone(); count])
}

fn array_fill_value_is_static_scalar_for_kind(value: &Expr) -> bool {
    matches!(
        value.kind,
        ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::Null
            | ExprKind::ConstRef(_)
    )
}

fn call_user_func_array_assoc_local_args_for_kind(
    name: &str,
    key_values: &[AssocKeyValue],
    span: crate::span::Span,
) -> (Vec<Expr>, bool) {
    let mut has_named_arg = false;
    let args = key_values
        .iter()
        .map(|key| {
            let index = match key {
                AssocKeyValue::Int(value) => Expr::new(ExprKind::IntLiteral(*value), span),
                AssocKeyValue::Str(value) => Expr::new(ExprKind::StringLiteral(value.clone()), span),
            };
            let access = Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(Expr::new(ExprKind::Variable(name.to_string()), span)),
                    index: Box::new(index),
                },
                span,
            );
            match key {
                AssocKeyValue::Int(_) => access,
                AssocKeyValue::Str(value) => {
                    has_named_arg = true;
                    Expr::new(
                        ExprKind::NamedArg {
                            name: value.clone(),
                            value: Box::new(access),
                        },
                        span,
                    )
                }
            }
        })
        .collect();
    (args, has_named_arg)
}

fn normalize_builtin_named_call_args_for_kind(
    target: &str,
    call_args: &[Expr],
    span: crate::span::Span,
) -> Option<Vec<Expr>> {
    let sig = builtin_call_sig(target)?;
    call_args::plan_call_args(&sig, call_args, span, true, false)
        .ok()
        .map(|plan| plan.normalized_args())
}

fn call_user_function_target_for_kind(
    callback: &Expr,
    callable_targets: &HashMap<String, String>,
    string_static_values: &HashMap<String, String>,
    constants: &HashMap<String, ConstantValue>,
) -> Option<String> {
    match &callback.kind {
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => Some(name.to_string()),
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod {
            receiver: StaticReceiver::Named(class_name),
            method,
        }) => Some(static_method_callable_symbol(class_name.as_str(), method)),
        ExprKind::Variable(name) => callable_targets
            .get(name)
            .cloned()
            .or_else(|| string_static_values.get(name).cloned()),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_target = call_user_function_target_for_kind(
                then_expr,
                callable_targets,
                string_static_values,
                constants,
            )?;
            let else_target = call_user_function_target_for_kind(
                else_expr,
                callable_targets,
                string_static_values,
                constants,
            )?;
            then_target
                .eq_ignore_ascii_case(&else_target)
                .then_some(then_target)
        }
        _ => static_string_for_metadata(callback, constants),
    }
}

fn dynamic_call_user_function_local_kind(
    callback: &Expr,
    function_possible_static_string_returns: &HashMap<String, Vec<String>>,
    string_static_values: &HashMap<String, String>,
    function_return_kinds: &HashMap<String, ValueKind>,
) -> Option<LocalKind> {
    let callbacks = match &callback.kind {
        ExprKind::FunctionCall { name, .. } => {
            function_possible_static_string_returns.get(&function_key(name.as_str()))?
        }
        ExprKind::Variable(name) => {
            if string_static_values.contains_key(name) {
                return None;
            }
            return None;
        }
        _ => return None,
    };
    let mut local_kind = None;
    for callback in callbacks {
        let kind = function_return_kinds
            .get(&function_key(callback.as_str()))
            .copied()
            .map(local_kind_for_value)?;
        if local_kind.is_some_and(|existing| existing != kind) {
            return None;
        }
        local_kind = Some(kind);
    }
    local_kind
}

fn static_method_callable_symbol(class_name: &str, method_name: &str) -> String {
    format!(
        "__wasm_static_method_{}_{}",
        function_key(class_name),
        function_key(method_name)
    )
}

fn instance_method_from_callable_symbol(target: &str) -> Option<&str> {
    target.strip_prefix("__wasm_instance_method_")
}

fn call_user_function_local_kind(
    callback: &Expr,
    call_args: Vec<Expr>,
    locals: &HashMap<String, LocalKind>,
    callable_targets: &HashMap<String, String>,
    string_static_values: &HashMap<String, String>,
    function_return_kinds: &HashMap<String, ValueKind>,
    constants: &HashMap<String, ConstantValue>,
    class_constants: &HashMap<String, ConstantValue>,
) -> Option<LocalKind> {
    if let Some(kind) = instance_callable_method_local_kind(callback, function_return_kinds) {
        return Some(kind);
    }
    let target =
        call_user_function_target_for_kind(callback, callable_targets, string_static_values, constants)?;
    if let Some(method) = instance_method_from_callable_symbol(&target) {
        return unknown_receiver_method_local_kind_for_call(method, function_return_kinds);
    }
    if let Some(kind) = function_return_kinds
        .get(&function_key(target.as_str()))
        .copied()
        .map(local_kind_for_value)
    {
        return Some(kind);
    }
    if matches!(
        target.to_ascii_lowercase().as_str(),
        "call_user_func" | "call_user_func_array"
    ) {
        return None;
    }
    let call = Expr::new(
        ExprKind::FunctionCall {
            name: Name::from(target),
            args: call_args,
        },
        callback.span,
    );
    Some(infer_local_kind(
        &call,
        locals,
        function_return_kinds,
        constants,
        class_constants,
    ))
}

fn instance_callable_method_local_kind(
    callback: &Expr,
    function_return_kinds: &HashMap<String, ValueKind>,
) -> Option<LocalKind> {
    match &callback.kind {
        ExprKind::FirstClassCallable(CallableTarget::Method { method, .. }) => {
            unknown_receiver_method_local_kind_for_call(method, function_return_kinds)
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_method = instance_callable_method_name(then_expr)?;
            let else_method = instance_callable_method_name(else_expr)?;
            let then_kind =
                unknown_receiver_method_local_kind_for_call(then_method, function_return_kinds)?;
            let else_kind =
                unknown_receiver_method_local_kind_for_call(else_method, function_return_kinds)?;
            (then_kind == else_kind).then_some(then_kind)
        }
        _ => None,
    }
}

fn instance_callable_method_name(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::FirstClassCallable(CallableTarget::Method { method, .. }) => Some(method),
        _ => None,
    }
}

fn static_callable_ternary_local_kind(
    callback: &Expr,
    function_return_kinds: &HashMap<String, ValueKind>,
) -> Option<LocalKind> {
    let ExprKind::Ternary {
        then_expr,
        else_expr,
        ..
    } = &callback.kind
    else {
        return None;
    };
    let then_target = static_method_callable_target_for_kind(then_expr)?;
    let else_target = static_method_callable_target_for_kind(else_expr)?;
    let then_kind = function_return_kinds
        .get(&function_key(&then_target))
        .copied()
        .map(local_kind_for_value)?;
    let else_kind = function_return_kinds
        .get(&function_key(&else_target))
        .copied()
        .map(local_kind_for_value)?;
    (then_kind == else_kind).then_some(then_kind)
}

fn static_method_callable_target_for_kind(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod {
            receiver: StaticReceiver::Named(class_name),
            method,
        }) => Some(static_method_callable_symbol(class_name.as_str(), method)),
        _ => None,
    }
}

fn unknown_receiver_method_local_kind_for_call(
    method: &str,
    function_return_kinds: &HashMap<String, ValueKind>,
) -> Option<LocalKind> {
    let suffix = format!("->{}", function_key(method));
    let mut kind = None;
    for (key, value_kind) in function_return_kinds {
        if !key.ends_with(&suffix) {
            continue;
        }
        let candidate = local_kind_for_value(*value_kind);
        if kind.is_some_and(|existing| existing != candidate) {
            return None;
        }
        kind = Some(candidate);
    }
    kind
}
