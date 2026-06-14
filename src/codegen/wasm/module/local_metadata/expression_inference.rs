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
    function_callable_return_targets: &HashMap<String, String>,
    function_possible_callable_return_targets: &HashMap<String, Vec<String>>,
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
            function_callable_return_targets,
            function_possible_callable_return_targets,
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
            function_callable_return_targets,
            function_possible_callable_return_targets,
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
    if let ExprKind::ExprCall { callee, args } = &expr.kind {
        if let Some(kind) = callable_return_expr_call_local_kind(
            callee,
            args,
            function_callable_return_targets,
            function_possible_callable_return_targets,
            function_return_kinds,
        ) {
            return kind;
        }
    }
    if let ExprKind::MethodCall { method, .. } = &expr.kind {
        if let Some(kind) =
            magic_call_fallback_local_kind(method, function_return_kinds, object_classes)
        {
            return kind;
        }
    }
    if let ExprKind::DynamicStaticMethodCall {
        receiver: StaticReceiver::Named(class_name),
        method,
        ..
    } = &expr.kind
    {
        if let Some(key) =
            dynamic_static_method_call_return_key(class_name.as_str(), method, string_static_values)
        {
            if let Some(kind) = function_return_kinds.get(&key).copied() {
                return local_kind_for_value(kind);
            }
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

fn magic_call_fallback_local_kind(
    method: &str,
    function_return_kinds: &HashMap<String, ValueKind>,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
) -> Option<LocalKind> {
    if object_classes.values().any(|class_info| {
        class_info
            .methods
            .iter()
            .any(|candidate| candidate.name.eq_ignore_ascii_case(method))
    }) {
        return None;
    }
    let mut kind = None;
    for class_info in object_classes.values() {
        let Some(method_info) = class_info
            .methods
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case("__call"))
        else {
            continue;
        };
        let key = method_call_return_key(&class_info.name, &method_info.name);
        let return_kind = function_return_kinds.get(&key).copied()?;
        let local_kind = local_kind_for_value(return_kind);
        if kind.is_some_and(|existing| existing != local_kind) {
            return None;
        }
        kind = Some(local_kind);
    }
    kind
}

fn infer_assignment_branch_local_kind(
    left: &Expr,
    right: &Expr,
    locals: &HashMap<String, LocalKind>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    callable_targets: &HashMap<String, String>,
    string_static_values: &HashMap<String, String>,
    function_callable_return_targets: &HashMap<String, String>,
    function_possible_callable_return_targets: &HashMap<String, Vec<String>>,
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
            function_callable_return_targets,
            function_possible_callable_return_targets,
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
            function_callable_return_targets,
            function_possible_callable_return_targets,
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

fn callable_return_expr_call_local_kind(
    callee: &Expr,
    args: &[Expr],
    function_callable_return_targets: &HashMap<String, String>,
    function_possible_callable_return_targets: &HashMap<String, Vec<String>>,
    function_return_kinds: &HashMap<String, ValueKind>,
) -> Option<LocalKind> {
    let key = match &callee.kind {
        ExprKind::FunctionCall { name, .. } => function_key(name.as_str()),
        ExprKind::StaticMethodCall {
            receiver: StaticReceiver::Named(class_name),
            method,
            ..
        } => static_method_call_return_key(class_name.as_str(), method),
        ExprKind::MethodCall { object, method, .. } => {
            if let ExprKind::NewObject { class_name, .. } = &object.kind {
                method_call_return_key(class_name.as_str(), method)
            } else {
                unique_callable_return_method_key(
                    method,
                    function_callable_return_targets,
                    function_possible_callable_return_targets,
                    function_return_kinds,
                )?
            }
        }
        ExprKind::NullsafeMethodCall { object, method, .. } => {
            if let ExprKind::NewObject { class_name, .. } = &object.kind {
                method_call_return_key(class_name.as_str(), method)
            } else {
                unique_callable_return_method_key(
                    method,
                    function_callable_return_targets,
                    function_possible_callable_return_targets,
                    function_return_kinds,
                )?
            }
        }
        _ => return None,
    };
    let targets = function_possible_callable_return_targets
        .get(&key)
        .cloned()
        .or_else(|| {
            function_callable_return_targets
                .get(&key)
                .map(|target| vec![target.clone()])
        })?;
    let mut kind = None;
    for target in targets {
        let target_kind = function_return_kinds.get(&function_key(&target)).copied()?;
        if kind.is_some_and(|existing| existing != target_kind) {
            return None;
        }
        kind = Some(target_kind);
    }
    let _ = args;
    kind.map(local_kind_for_value)
}

fn unique_callable_return_method_key(
    method: &str,
    function_callable_return_targets: &HashMap<String, String>,
    function_possible_callable_return_targets: &HashMap<String, Vec<String>>,
    function_return_kinds: &HashMap<String, ValueKind>,
) -> Option<String> {
    let suffix = format!("->{}", function_key(method));
    let mut found = None;
    let mut found_kind = None;
    for key in function_callable_return_targets
        .keys()
        .chain(function_possible_callable_return_targets.keys())
        .filter(|key| key.ends_with(&suffix))
    {
        let targets = function_possible_callable_return_targets
            .get(key)
            .cloned()
            .or_else(|| {
                function_callable_return_targets
                    .get(key)
                    .map(|target| vec![target.clone()])
            })?;
        let mut key_kind = None;
        for target in targets {
            let target_kind = function_return_kinds.get(&function_key(&target)).copied()?;
            if key_kind.is_some_and(|existing| existing != target_kind) {
                return None;
            }
            key_kind = Some(target_kind);
        }
        let key_kind = key_kind?;
        if found_kind.is_some_and(|existing| existing != key_kind) {
            return None;
        }
        found = Some(key.clone());
        found_kind = Some(key_kind);
    }
    found
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
