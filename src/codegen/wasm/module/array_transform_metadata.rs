//! Purpose:
//! Computes WASM-only compile-time metadata for array transforms used in assignments.
//! Covers set ops, `array_map(null)`, `array_merge`, and key-set transform metadata.
//!
//! Called from:
//! - `super::collect_stmt_locals()` and sibling wasm metadata helpers.
//!
//! Key details:
//! - These helpers only propagate known compile-time array shapes.
//! - Unknown dynamic shapes return `None` so runtime lowering or explicit rejection can handle them.

use std::collections::{HashMap, HashSet};

use crate::parser::ast::{Expr, ExprKind, StaticReceiver};

use super::{
    array_merge_foreach_key_local_kind, function_key, static_method_call_return_key,
    static_assoc_key_kinds_for_items, static_assoc_key_values_for_items,
    static_nested_array_metadata_for_assoc_items, static_nested_array_metadata_for_items,
    static_value_cell_kind_for_expr,
    static_value_cell_kinds_for_assoc_items, static_value_cell_kinds_for_items,
    value_kinds_for_foreach_source, ArrayLayout, AssocKeyKind, AssocKeyValue, LocalKind,
    NestedArrayMetadata, ValueCellKind,
};

pub(super) fn set_op_value_kinds_for_assignment(
    value: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<Vec<ValueCellKind>> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !matches!(name.to_ascii_lowercase().as_str(), "array_diff" | "array_intersect") {
        return None;
    }
    let ExprKind::Variable(source) = &args.first()?.kind else {
        return None;
    };
    let kinds = array_value_kinds.get(source)?;
    let first = *kinds.first()?;
    if matches!(
        first,
        ValueCellKind::Int
            | ValueCellKind::Float
            | ValueCellKind::Bool
            | ValueCellKind::Null
            | ValueCellKind::Str
    )
        && kinds.iter().all(|kind| *kind == first)
    {
        Some(kinds.clone())
    } else {
        None
    }
}

pub(super) fn copy_array_map_null_metadata(
    target: &str,
    source: &Expr,
    array_value_kinds: &mut HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &mut HashMap<String, ValueCellKind>,
    array_nested_values: &mut HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_key_kinds: &mut HashMap<String, Vec<AssocKeyKind>>,
    array_key_values: &mut HashMap<String, Vec<AssocKeyValue>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) {
    match &source.kind {
        ExprKind::ArrayLiteral(items) => {
            if let Some(kinds) = static_value_cell_kinds_for_items(items) {
                array_value_kinds.insert(target.to_string(), kinds);
            }
            array_runtime_value_kinds.remove(target);
            if let Some(metadata) = static_nested_array_metadata_for_items(items) {
                array_nested_values.insert(target.to_string(), metadata);
            } else {
                array_nested_values.remove(target);
            }
            array_key_kinds.remove(target);
            array_key_values.remove(target);
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            if let Some(kinds) = static_value_cell_kinds_for_assoc_items(items) {
                array_value_kinds.insert(target.to_string(), kinds);
            }
            array_runtime_value_kinds.remove(target);
            if let Some(metadata) = static_nested_array_metadata_for_assoc_items(items) {
                array_nested_values.insert(target.to_string(), metadata);
            } else {
                array_nested_values.remove(target);
            }
            if let Some(kinds) = static_assoc_key_kinds_for_items(items) {
                array_key_kinds.insert(target.to_string(), kinds);
            }
            if let Some(values) = static_assoc_key_values_for_items(items) {
                array_key_values.insert(target.to_string(), values);
            }
        }
        ExprKind::Variable(source_name) => {
            if let Some(kinds) = array_value_kinds.get(source_name).cloned() {
                array_value_kinds.insert(target.to_string(), kinds);
                array_runtime_value_kinds.remove(target);
            } else if let Some(kind) = array_runtime_value_kinds.get(source_name).copied() {
                array_value_kinds.remove(target);
                array_runtime_value_kinds.insert(target.to_string(), kind);
            }
            if let Some(metadata) = array_nested_values.get(source_name).cloned() {
                array_nested_values.insert(target.to_string(), metadata);
            } else {
                array_nested_values.remove(target);
            }
            if let Some(kinds) = array_key_kinds.get(source_name).cloned() {
                array_key_kinds.insert(target.to_string(), kinds);
            } else {
                array_key_kinds.remove(target);
            }
            if let Some(values) = array_key_values.get(source_name).cloned() {
                array_key_values.insert(target.to_string(), values);
            } else {
                array_key_values.remove(target);
            }
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_fill") => {
            if let Some(kind) = args.get(2).and_then(static_value_cell_kind_for_expr) {
                array_value_kinds.remove(target);
                array_runtime_value_kinds.insert(target.to_string(), kind);
            }
            array_nested_values.remove(target);
            array_key_kinds.remove(target);
            array_key_values.remove(target);
        }
        ExprKind::FunctionCall { name, .. } => {
            let key = function_key(name);
            if let Some(kinds) = function_array_return_value_kinds.get(&key) {
                array_value_kinds.insert(target.to_string(), kinds.clone());
                array_runtime_value_kinds.remove(target);
            } else if let Some(kind) = function_array_return_runtime_value_kinds.get(&key) {
                array_value_kinds.remove(target);
                array_runtime_value_kinds.insert(target.to_string(), *kind);
            } else {
                array_value_kinds.remove(target);
                array_runtime_value_kinds.remove(target);
            }
            if let Some(metadata) = function_array_return_nested_values.get(&key) {
                array_nested_values.insert(target.to_string(), metadata.clone());
            } else {
                array_nested_values.remove(target);
            }
            if let Some(kinds) = function_array_return_key_kinds.get(&key) {
                array_key_kinds.insert(target.to_string(), kinds.clone());
            } else {
                array_key_kinds.remove(target);
            }
            if let Some(values) = function_array_return_key_values.get(&key) {
                array_key_values.insert(target.to_string(), values.clone());
            } else {
                array_key_values.remove(target);
            }
        }
        _ => {}
    }
}

pub(super) fn array_map_null_multi_metadata_for_assignment(
    args: &[Expr],
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<(Vec<ValueCellKind>, Vec<Option<NestedArrayMetadata>>)> {
    let mut source_kinds = Vec::new();
    for source in args.iter().skip(1) {
        source_kinds.push(known_value_kinds_for_array_map_null_source(
            source,
            array_value_kinds,
            function_array_return_value_kinds,
        )?);
    }
    let len = source_kinds.iter().map(Vec::len).max()?;
    let nested = (0..len)
        .map(|index| {
            Some(NestedArrayMetadata {
                layout: ArrayLayout::Value,
                len: source_kinds.len(),
                value_kinds: Some(
                    source_kinds
                        .iter()
                        .map(|kinds| kinds.get(index).copied().unwrap_or(ValueCellKind::Null))
                        .collect(),
                ),
                key_values: None,
                nested_values: None,
            })
        })
        .collect();
    Some((vec![ValueCellKind::Array; len], nested))
}

pub(super) fn known_value_kinds_for_array_map_null_source(
    source: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<Vec<ValueCellKind>> {
    match &source.kind {
        ExprKind::ArrayLiteral(items) => static_value_cell_kinds_for_items(items),
        ExprKind::ArrayLiteralAssoc(items) => static_value_cell_kinds_for_assoc_items(items),
        ExprKind::Variable(name) => array_value_kinds.get(name).cloned(),
        ExprKind::FunctionCall { name, .. } => function_array_return_value_kinds
            .get(&function_key(name))
            .cloned(),
        _ => None,
    }
}

pub(super) fn set_op_runtime_value_kind_for_assignment(
    value: &Expr,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> Option<ValueCellKind> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !matches!(name.to_ascii_lowercase().as_str(), "array_diff" | "array_intersect") {
        return None;
    }
    let ExprKind::Variable(source) = &args.first()?.kind else {
        return None;
    };
    let kind = *array_runtime_value_kinds.get(source)?;
    matches!(
        kind,
        ValueCellKind::Int
            | ValueCellKind::Float
            | ValueCellKind::Bool
            | ValueCellKind::Null
            | ValueCellKind::Str
    )
    .then_some(kind)
}

pub(super) fn array_merge_value_kinds_for_assignment(
    value: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    string_static_values: &HashMap<String, String>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> Option<Vec<ValueCellKind>> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_merge") {
        return None;
    }
    let mut kinds = Vec::new();
    for arg in args {
        kinds.extend(value_kinds_for_foreach_source(
            arg,
            array_value_kinds,
            array_runtime_value_kinds,
        ).or_else(|| {
            dynamic_static_method_return_value_kinds_for_assignment(
                arg,
                string_static_values,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
            )
        })?);
    }
    (!kinds.is_empty()).then_some(kinds)
}

pub(super) fn array_merge_key_kinds_for_assignment(
    value: &Expr,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    php_normalized_key_arrays: &HashSet<String>,
    string_static_values: &HashMap<String, String>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<Vec<AssocKeyKind>> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_merge") {
        return None;
    }
    match array_merge_foreach_key_local_kind(
        args,
        array_key_kinds,
        array_value_kinds,
        array_runtime_value_kinds,
        php_normalized_key_arrays,
        string_static_values,
        function_array_return_key_kinds,
    ) {
        LocalKind::Str => Some(vec![AssocKeyKind::Str]),
        LocalKind::Mixed => Some(vec![AssocKeyKind::Int, AssocKeyKind::Str]),
        LocalKind::I64 => Some(vec![AssocKeyKind::Int]),
        _ => None,
    }
}

fn dynamic_static_method_return_value_kinds_for_assignment(
    source: &Expr,
    string_static_values: &HashMap<String, String>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> Option<Vec<ValueCellKind>> {
    let key = dynamic_static_method_return_key_for_assignment(source, string_static_values)?;
    function_array_return_value_kinds
        .get(&key)
        .cloned()
        .or_else(|| function_array_return_runtime_value_kinds.get(&key).map(|kind| vec![*kind]))
}

fn dynamic_static_method_return_key_for_assignment(
    source: &Expr,
    string_static_values: &HashMap<String, String>,
) -> Option<String> {
    let ExprKind::DynamicStaticMethodCall {
        receiver: StaticReceiver::Named(class_name),
        method,
        ..
    } = &source.kind else {
        return None;
    };
    let method = match &method.kind {
        ExprKind::StringLiteral(method) => method.clone(),
        ExprKind::Variable(name) => string_static_values.get(name)?.clone(),
        _ => return None,
    };
    Some(static_method_call_return_key(class_name.as_str(), &method))
}

pub(super) fn key_set_value_kinds_for_assignment(
    value: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<Vec<ValueCellKind>> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !matches!(
        name.to_ascii_lowercase().as_str(),
        "array_diff_key" | "array_intersect_key"
    ) {
        return None;
    }
    value_kinds_for_foreach_source(args.first()?, array_value_kinds, &HashMap::new())
}
