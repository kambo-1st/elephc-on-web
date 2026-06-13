//! Purpose:
//! Tracks static callback and string values used by wasm32-web local metadata.
//! Keeps callable-name resolution separate from array shape inference.
//!
//! Called from:
//! - `super::collect_stmt_locals()` and callback array metadata helpers.
//!
//! Key details:
//! - Only statically known function callbacks are classified here.
//! - Dynamic callable values stay unknown so runtime lowering or rejection handles them.

use super::*;

pub(in crate::codegen::wasm::module) fn array_map_callback_return_kind(
    callback: &str,
    function_return_kinds: &HashMap<String, ValueKind>,
) -> Option<ValueKind> {
    if matches!(
        callback.to_ascii_lowercase().as_str(),
        "is_int" | "is_string" | "is_bool" | "is_null" | "is_float" | "is_numeric"
            | "is_array" | "is_iterable" | "is_object" | "boolval"
    ) {
        return Some(ValueKind::Bool);
    }
    if callback.eq_ignore_ascii_case("get_class") || callback.eq_ignore_ascii_case("gettype") {
        return Some(ValueKind::Str);
    }
    function_return_kinds.get(&function_key(callback)).copied()
}

pub(in crate::codegen::wasm::module) fn array_map_callback_expr_return_kind(
    expr: &Expr,
    function_return_kinds: &HashMap<String, ValueKind>,
    callable_targets: &HashMap<String, String>,
    string_static_values: &HashMap<String, String>,
    function_possible_static_string_returns: &HashMap<String, Vec<String>>,
) -> Option<ValueKind> {
    if let Some(callback) =
        static_callback_name_for_assignment_locals(expr, callable_targets, string_static_values)
    {
        return array_map_callback_return_kind(&callback, function_return_kinds);
    }
    let ExprKind::FunctionCall { name, .. } = &expr.kind else {
        return None;
    };
    let callbacks = function_possible_static_string_returns.get(&function_key(name.as_str()))?;
    let mut kinds = callbacks
        .iter()
        .map(|callback| array_map_callback_return_kind(callback, function_return_kinds));
    let first = kinds.next().flatten()?;
    kinds.all(|kind| kind == Some(first)).then_some(first)
}

pub(in crate::codegen::wasm::module) fn static_callback_name_for_locals(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::StringLiteral(name) => Some(name.clone()),
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => Some(name.to_string()),
        ExprKind::FunctionCall { name, args } if args.is_empty() => Some(name.to_string()),
        ExprKind::Variable(_) => None,
        _ => None,
    }
}

pub(in crate::codegen::wasm::module) fn static_callback_name_for_assignment_locals(
    expr: &Expr,
    callable_targets: &HashMap<String, String>,
    string_static_values: &HashMap<String, String>,
) -> Option<String> {
    match &expr.kind {
        ExprKind::Variable(name) => callable_targets
            .get(name)
            .cloned()
            .or_else(|| string_static_values.get(name).cloned()),
        _ => static_callback_name_for_locals(expr),
    }
}

pub(super) fn static_string_for_assignment_locals(
    expr: &Expr,
    string_static_values: &HashMap<String, String>,
    function_static_string_returns: &HashMap<String, String>,
    constants: &HashMap<String, ConstantValue>,
) -> Option<String> {
    match &expr.kind {
        ExprKind::Variable(name) => string_static_values.get(name).cloned(),
        ExprKind::FunctionCall { name, args } if args.is_empty() => {
            function_static_string_returns.get(&function_key(name)).cloned()
        }
        ExprKind::FunctionCall { name, args } => {
            let key = static_string_return_call_key_for_expr(name, args, constants)?;
            function_static_string_returns.get(&key).cloned()
        }
        _ => static_string_for_metadata(expr, constants),
    }
}

pub(super) fn callable_target_for_locals(
    expr: &Expr,
    callable_targets: &HashMap<String, String>,
) -> Option<String> {
    match &expr.kind {
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => Some(name.to_string()),
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod {
            receiver: StaticReceiver::Named(class_name),
            method,
        }) => Some(static_method_callable_symbol(class_name.as_str(), method)),
        ExprKind::FirstClassCallable(CallableTarget::Method { method, .. }) => {
            Some(instance_method_callable_symbol(method))
        }
        ExprKind::Variable(name) => callable_targets.get(name).cloned(),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_target = callable_target_for_locals(then_expr, callable_targets)?;
            let else_target = callable_target_for_locals(else_expr, callable_targets)?;
            then_target
                .eq_ignore_ascii_case(&else_target)
                .then_some(then_target)
        }
        _ => None,
    }
}

fn static_method_callable_symbol(class_name: &str, method_name: &str) -> String {
    format!(
        "__wasm_static_method_{}_{}",
        function_key(class_name),
        function_key(method_name)
    )
}

fn instance_method_callable_symbol(method_name: &str) -> String {
    format!("__wasm_instance_method_{}", function_key(method_name))
}
