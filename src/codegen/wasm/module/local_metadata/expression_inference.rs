//! Purpose:
//! Provides small expression local-kind inference helpers for wasm32-web metadata.
//! Keeps fallback callable handling and branch type merging out of the main collector.
//!
//! Called from:
//! - `super::infer_assignment_local_kind()`.
//! - `super::infer_local_kind()` for ternary, match, and branch-like expressions.
//!
//! Key details:
//! - Dynamic callable helpers consume the same metadata maps as direct call inference.
//! - Branch merging is conservative and returns scalar locals only when all arms agree.

use super::*;

pub(super) fn infer_assignment_fallback_local_kind(
    expr: &Expr,
    locals: &HashMap<String, LocalKind>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    callable_targets: &HashMap<String, String>,
    string_static_values: &HashMap<String, String>,
    function_possible_static_string_returns: &HashMap<String, Vec<String>>,
    function_return_kinds: &HashMap<String, ValueKind>,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
    constants: &HashMap<String, ConstantValue>,
    class_constants: &HashMap<String, ConstantValue>,
) -> LocalKind {
    if callable_target_for_locals(
        expr,
        callable_targets,
        string_static_values,
        function_return_kinds,
        object_classes,
        constants,
        class_constants,
    )
    .is_some()
    {
        return LocalKind::Callable;
    }
    if let ExprKind::Ternary {
        then_expr,
        else_expr,
        ..
    } = &expr.kind {
        return infer_assignment_branch_local_kind(
            then_expr,
            else_expr,
            locals,
            array_key_values,
            callable_targets,
            string_static_values,
            function_possible_static_string_returns,
            function_return_kinds,
            object_classes,
            constants,
            class_constants,
        );
    }
    if let ExprKind::ShortTernary { value, default } = &expr.kind {
        return infer_assignment_branch_local_kind(
            value,
            default,
            locals,
            array_key_values,
            callable_targets,
            string_static_values,
            function_possible_static_string_returns,
            function_return_kinds,
            object_classes,
            constants,
            class_constants,
        );
    }
    if let ExprKind::FunctionCall { name, args } = &expr.kind {
        if name.eq_ignore_ascii_case("array_reduce") {
            return array_reduce_assignment_local_kind(
                args,
                locals,
                callable_targets,
                string_static_values,
                function_return_kinds,
                constants,
                class_constants,
            );
        }
        if name.eq_ignore_ascii_case("call_user_func") {
            return call_user_func_local_kind(
                args,
                locals,
                callable_targets,
                string_static_values,
                function_possible_static_string_returns,
                function_return_kinds,
                constants,
                class_constants,
            )
            .unwrap_or(LocalKind::I64);
        }
        if name.eq_ignore_ascii_case("call_user_func_array") {
            return call_user_func_array_local_kind(
                args,
                locals,
                array_key_values,
                callable_targets,
                string_static_values,
                function_possible_static_string_returns,
                function_return_kinds,
                constants,
                class_constants,
            )
            .unwrap_or(LocalKind::I64);
        }
    }
    if let ExprKind::ClosureCall { var, .. } = &expr.kind {
        if let Some(target) = callable_targets
            .get(var)
            .or_else(|| string_static_values.get(var))
        {
            return function_return_kinds
                .get(&function_key(target))
                .copied()
                .map(local_kind_for_value)
                .unwrap_or(LocalKind::I64);
        }
        return unknown_receiver_method_local_kind("__invoke", function_return_kinds);
    }
    infer_local_kind(expr, locals, function_return_kinds, constants, class_constants)
}

fn infer_assignment_branch_local_kind(
    left: &Expr,
    right: &Expr,
    locals: &HashMap<String, LocalKind>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    callable_targets: &HashMap<String, String>,
    string_static_values: &HashMap<String, String>,
    function_possible_static_string_returns: &HashMap<String, Vec<String>>,
    function_return_kinds: &HashMap<String, ValueKind>,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
    constants: &HashMap<String, ConstantValue>,
    class_constants: &HashMap<String, ConstantValue>,
) -> LocalKind {
    match (
        infer_assignment_fallback_local_kind(
            left,
            locals,
            array_key_values,
            callable_targets,
            string_static_values,
            function_possible_static_string_returns,
            function_return_kinds,
            object_classes,
            constants,
            class_constants,
        ),
        infer_assignment_fallback_local_kind(
            right,
            locals,
            array_key_values,
            callable_targets,
            string_static_values,
            function_possible_static_string_returns,
            function_return_kinds,
            object_classes,
            constants,
            class_constants,
        ),
    ) {
        (LocalKind::F64, _) | (_, LocalKind::F64) => LocalKind::F64,
        (LocalKind::I32, LocalKind::I32) => LocalKind::I32,
        (LocalKind::Str, LocalKind::Str) => LocalKind::Str,
        (LocalKind::Callable, LocalKind::Callable) => LocalKind::Callable,
        (left, right) if left == right => left,
        _ => LocalKind::Mixed,
    }
}

fn array_reduce_assignment_local_kind(
    args: &[Expr],
    locals: &HashMap<String, LocalKind>,
    callable_targets: &HashMap<String, String>,
    string_static_values: &HashMap<String, String>,
    function_return_kinds: &HashMap<String, ValueKind>,
    constants: &HashMap<String, ConstantValue>,
    class_constants: &HashMap<String, ConstantValue>,
) -> LocalKind {
    if let Some(callback) = args.get(1).and_then(|callback| {
        static_callback_name_for_assignment_locals(callback, callable_targets, string_static_values)
    }) {
        if let Some(kind) = array_map_callback_return_kind(&callback, function_return_kinds) {
            return local_kind_for_value(kind);
        }
    }
    args.get(2)
        .map(|initial| {
            infer_local_kind(
                initial,
                locals,
                function_return_kinds,
                constants,
                class_constants,
            )
        })
        .filter(|kind| matches!(kind, LocalKind::Str | LocalKind::I32 | LocalKind::F64))
        .unwrap_or(LocalKind::I64)
}

pub(super) fn infer_branch_local_kind(
    left: &Expr,
    right: &Expr,
    locals: &HashMap<String, LocalKind>,
    function_return_kinds: &HashMap<String, ValueKind>,
    constants: &HashMap<String, ConstantValue>,
    class_constants: &HashMap<String, ConstantValue>,
) -> LocalKind {
    match (
        infer_local_kind(
            left,
            locals,
            function_return_kinds,
            constants,
            class_constants,
        ),
        infer_local_kind(
            right,
            locals,
            function_return_kinds,
            constants,
            class_constants,
        ),
    ) {
        (LocalKind::F64, _) | (_, LocalKind::F64) => LocalKind::F64,
        (LocalKind::I32, LocalKind::I32) => LocalKind::I32,
        (LocalKind::Str, LocalKind::Str) => LocalKind::Str,
        (left, right) if left == right => left,
        _ => LocalKind::Mixed,
    }
}

pub(super) fn infer_many_local_kind(
    values: &[&Expr],
    locals: &HashMap<String, LocalKind>,
    function_return_kinds: &HashMap<String, ValueKind>,
    constants: &HashMap<String, ConstantValue>,
    class_constants: &HashMap<String, ConstantValue>,
) -> LocalKind {
    if values.is_empty() {
        return LocalKind::I64;
    }
    if values.iter().any(|value| {
        infer_local_kind(
            value,
            locals,
            function_return_kinds,
            constants,
            class_constants,
        ) == LocalKind::F64
    }) {
        return LocalKind::F64;
    }
    if values.iter().all(|value| {
        infer_local_kind(
            value,
            locals,
            function_return_kinds,
            constants,
            class_constants,
        ) == LocalKind::I32
    }) {
        return LocalKind::I32;
    }
    if values.iter().all(|value| {
        infer_local_kind(
            value,
            locals,
            function_return_kinds,
            constants,
            class_constants,
        ) == LocalKind::Str
    }) {
        return LocalKind::Str;
    }
    if values.iter().all(|value| {
        infer_local_kind(
            value,
            locals,
            function_return_kinds,
            constants,
            class_constants,
        ) == LocalKind::Array
    }) {
        return LocalKind::Array;
    }
    if values.iter().all(|value| {
        infer_local_kind(
            value,
            locals,
            function_return_kinds,
            constants,
            class_constants,
        ) == LocalKind::I64
    }) {
        return LocalKind::I64;
    }
    LocalKind::Mixed
}
