//! Purpose:
//! Infers WASM local kinds for `array_rand()` results from known array key metadata.
//! Keeps random-key result typing separate from the main module state file.
//!
//! Called from:
//! - `super::infer_assignment_local_kind()` when assigning `array_rand()` results.
//!
//! Key details:
//! - This is compile-time metadata only; runtime random selection remains in WAT lowering.
//! - Mixed or PHP-normalized key sets produce `LocalKind::Mixed` instead of assuming integers.

use std::collections::{HashMap, HashSet};

use crate::parser::ast::{Expr, ExprKind, StaticReceiver};

use super::{
    array_access_metadata, array_filter_foreach_key_kinds, assoc_key_kind_for_value,
    direct_assoc_builder_key_kinds_for_foreach, function_key, method_call_return_key,
    static_assoc_key_kinds_for_items, static_method_call_return_key, ArrayLayout, AssocKeyKind,
    AssocKeyValue, LocalKind, NestedArrayMetadata, ValueCellKind,
};

pub(super) fn local_kind_for_source(
    source: &Expr,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    php_normalized_key_arrays: &HashSet<String>,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    string_static_values: &HashMap<String, String>,
    function_array_return_layouts: &HashMap<String, ArrayLayout>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_return_kinds: &HashMap<String, super::ValueKind>,
    function_callable_return_targets: &HashMap<String, String>,
    function_possible_callable_return_targets: &HashMap<String, Vec<String>>,
) -> Option<LocalKind> {
    match &source.kind {
        ExprKind::ArrayLiteralAssoc(items) => {
            local_kind_from_key_kinds(&static_assoc_key_kinds_for_items(items)?)
        }
        ExprKind::ArrayAccess { .. } => {
            let Some(metadata) = array_access_metadata::array_access_metadata_for_locals(
                source,
                array_nested_values,
                array_key_values,
            ) else {
                return Some(LocalKind::Mixed);
            };
            if metadata.layout != ArrayLayout::Assoc {
                return Some(LocalKind::I64);
            }
            let Some(keys) = metadata.key_values.as_ref() else {
                return Some(LocalKind::Mixed);
            };
            let kinds = keys.iter().map(assoc_key_kind_for_value).collect::<Vec<_>>();
            local_kind_from_key_kinds(&kinds)
        }
        ExprKind::Variable(name) => {
            if php_normalized_key_arrays.contains(name) {
                return Some(LocalKind::Mixed);
            }
            if let Some(kinds) = array_key_kinds.get(name) {
                return local_kind_from_key_kinds(kinds);
            }
            let kinds = array_key_values
                .get(name)?
                .iter()
                .map(assoc_key_kind_for_value)
                .collect::<Vec<_>>();
            local_kind_from_key_kinds(&kinds)
        }
        ExprKind::FunctionCall { name, .. }
            if function_array_return_layouts
                .get(&function_key(name))
                .is_some_and(|layout| *layout == ArrayLayout::Assoc) =>
        {
            function_array_return_key_kinds
                .get(&function_key(name))
                .and_then(|kinds| local_kind_from_key_kinds(kinds))
        }
        ExprKind::DynamicStaticMethodCall {
            receiver: StaticReceiver::Named(class_name),
            method,
            ..
        } => dynamic_static_method_call_return_key(class_name.as_str(), method, string_static_values)
            .and_then(|key| {
                if function_array_return_layouts
                    .get(&key)
                    .is_some_and(|layout| *layout == ArrayLayout::Assoc)
                {
                    function_array_return_key_kinds
                        .get(&key)
                        .and_then(|kinds| local_kind_from_key_kinds(kinds))
                } else {
                    None
                }
            }),
        ExprKind::ExprCall { callee, .. } => callable_expr_source_key(
            callee,
            function_return_kinds,
            function_callable_return_targets,
            function_possible_callable_return_targets,
        )
        .and_then(|key| {
                if function_array_return_layouts
                    .get(&key)
                    .is_some_and(|layout| *layout == ArrayLayout::Assoc)
                {
                    function_array_return_key_kinds
                        .get(&key)
                        .and_then(|kinds| local_kind_from_key_kinds(kinds))
                } else {
                    None
                }
            }),
        ExprKind::FunctionCall { name, args }
            if matches!(name.to_ascii_lowercase().as_str(), "array_fill_keys" | "array_combine") =>
        {
            direct_assoc_builder_key_kinds_for_foreach(
                args,
                array_value_kinds,
                array_runtime_value_kinds,
            )
            .as_deref()
            .and_then(local_kind_from_key_kinds)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_filter") => {
            array_filter_foreach_key_kinds(
                args,
                array_key_kinds,
                array_value_kinds,
                array_runtime_value_kinds,
                function_array_return_key_kinds,
            )
            .as_deref()
            .and_then(local_kind_from_key_kinds)
        }
        _ => None,
    }
}

fn callable_expr_source_key(
    callee: &Expr,
    function_return_kinds: &HashMap<String, super::ValueKind>,
    function_callable_return_targets: &HashMap<String, String>,
    function_possible_callable_return_targets: &HashMap<String, Vec<String>>,
) -> Option<String> {
    let descriptor = match &callee.kind {
        ExprKind::FunctionCall { name, .. } => function_key(name),
        ExprKind::StaticMethodCall {
            receiver: StaticReceiver::Named(class_name),
            method,
            ..
        } => static_method_call_return_key(class_name.as_str(), method),
        ExprKind::MethodCall { object, method, .. }
        | ExprKind::NullsafeMethodCall { object, method, .. } => {
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
    if function_return_kinds.get(&descriptor) != Some(&super::ValueKind::Callable) {
        return None;
    }
    let targets = function_possible_callable_return_targets
        .get(&descriptor)
        .cloned()
        .or_else(|| {
            function_callable_return_targets
                .get(&descriptor)
                .map(|target| vec![target.clone()])
        })?;
    let mut selected = None;
    for target in targets {
        let key = function_key(&target);
        if function_return_kinds.get(&key) != Some(&super::ValueKind::Array) {
            return None;
        }
        if selected.as_deref().is_some_and(|existing| existing != key) {
            return None;
        }
        selected = Some(key);
    }
    selected
}

fn unique_callable_return_method_key(
    method: &str,
    function_callable_return_targets: &HashMap<String, String>,
    function_possible_callable_return_targets: &HashMap<String, Vec<String>>,
    function_return_kinds: &HashMap<String, super::ValueKind>,
) -> Option<String> {
    let suffix = format!("->{}", function_key(method));
    let mut selected = None;
    let mut selected_targets = None;
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
        if !targets
            .iter()
            .all(|target| function_return_kinds.get(&function_key(target)) == Some(&super::ValueKind::Array))
        {
            return None;
        }
        if selected_targets
            .as_ref()
            .is_some_and(|existing: &Vec<String>| existing != &targets)
        {
            return None;
        }
        selected = Some(key.clone());
        selected_targets = Some(targets);
    }
    selected
}

fn dynamic_static_method_call_return_key(
    class_name: &str,
    method: &Expr,
    string_static_values: &HashMap<String, String>,
) -> Option<String> {
    let method = match &method.kind {
        ExprKind::StringLiteral(method) => method.as_str(),
        ExprKind::Variable(name) => string_static_values.get(name)?.as_str(),
        _ => return None,
    };
    Some(static_method_call_return_key(class_name, method))
}

fn local_kind_from_key_kinds(kinds: &[AssocKeyKind]) -> Option<LocalKind> {
    if kinds.iter().all(|kind| *kind == AssocKeyKind::Int) {
        Some(LocalKind::I64)
    } else {
        Some(LocalKind::Mixed)
    }
}
