//! Purpose:
//! Computes WASM-only compile-time metadata for `array_filter` results.
//! Keeps key/value/nested shape inference out of the main module state file.
//!
//! Called from:
//! - `super::collect_stmt_locals()` and foreach local inference helpers.
//!
//! Key details:
//! - These helpers only track static metadata; emitted runtime behavior stays in wasm lowering.
//! - Unsupported dynamic shapes return `None` so later codegen can reject or use runtime paths.

use std::collections::HashMap;

use crate::parser::ast::{Expr, ExprKind};

use super::{
    array_transform_metadata::array_map_null_multi_metadata_for_assignment, function_key,
    normalized_static_assoc_items,
    static_assoc_key_kinds_for_items, static_assoc_key_value_for_expr,
    static_callback_name_for_assignment_locals, static_nested_array_metadata_for_expr,
    static_value_cell_kind_for_expr, static_value_cell_kinds_for_assoc_items,
    static_value_cell_kinds_for_items, static_value_cell_truthiness_for_filter, AssocKeyKind,
    AssocKeyValue, NestedArrayMetadata, ValueCellKind,
};

pub(super) fn default_value_kinds_for_assignment(
    value: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<Vec<ValueCellKind>> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_filter") || args.len() != 1 {
        return None;
    }
    match &args[0].kind {
        ExprKind::ArrayLiteral(items) => static_value_cell_kinds_for_items(items),
        ExprKind::Variable(source) => array_value_kinds.get(source).cloned(),
        ExprKind::FunctionCall {
            name: source_name,
            args: source_args,
        } if source_name.eq_ignore_ascii_case("array_map")
            && matches!(source_args.first().map(|arg| &arg.kind), Some(ExprKind::Null))
            && source_args.len() > 2 =>
        {
            array_map_null_multi_metadata_for_assignment(
                source_args,
                array_value_kinds,
                &HashMap::new(),
            )
            .map(|(kinds, _)| kinds)
        }
        ExprKind::FunctionCall { name, .. }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "class_parents" | "class_implements" | "class_uses"
            ) =>
        {
            Some(vec![ValueCellKind::Str])
        }
        _ => None,
    }
}

pub(super) fn default_nested_values_for_assignment(
    value: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
) -> Option<Vec<Option<NestedArrayMetadata>>> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_filter") || args.len() != 1 {
        return None;
    }
    let nested_values = match &args[0].kind {
        ExprKind::ArrayLiteral(items) => items
            .iter()
            .filter_map(|item| {
                static_value_cell_truthiness_for_filter(item)
                    .filter(|truthy| *truthy)
                    .map(|_| static_nested_array_metadata_for_expr(item))
            })
            .collect::<Vec<_>>(),
        ExprKind::ArrayLiteralAssoc(items) => {
            let normalized = normalized_static_assoc_items(items)?;
            normalized
                .iter()
                .filter_map(|(_, item)| {
                    static_value_cell_truthiness_for_filter(item)
                        .filter(|truthy| *truthy)
                        .map(|_| static_nested_array_metadata_for_expr(item))
                })
                .collect::<Vec<_>>()
        }
        ExprKind::Variable(source) => array_nested_values
            .get(source)?
            .iter()
            .filter_map(|metadata| {
                metadata
                    .as_ref()
                    .filter(|metadata| metadata.len != 0)
                    .map(|metadata| Some(metadata.clone()))
            })
            .collect::<Vec<_>>(),
        ExprKind::FunctionCall { .. } => super::array_metadata::nested_values_for_foreach_array_expr(
            &args[0],
            array_nested_values,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        )?
        .into_iter()
        .filter_map(|metadata| {
            metadata
                .filter(|metadata| metadata.len != 0)
                .map(Some)
        })
        .collect::<Vec<_>>(),
        _ => return None,
    };
    nested_values.iter().any(Option::is_some).then_some(nested_values)
}

pub(super) fn default_key_values_for_assignment(
    value: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) -> Option<Vec<AssocKeyValue>> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_filter") || args.len() != 1 {
        return None;
    }
    match &args[0].kind {
        ExprKind::ArrayLiteral(items) => {
            let mut keys = Vec::new();
            for (index, item) in items.iter().enumerate() {
                if static_value_cell_truthiness_for_filter(item)? {
                    keys.push(AssocKeyValue::Int(index as i64));
                }
            }
            Some(keys)
        }
        ExprKind::Variable(source) => {
            let mut keys = Vec::new();
            let source_keys = array_key_values.get(source);
            for (index, metadata) in array_nested_values.get(source)?.iter().enumerate() {
                if metadata.as_ref().is_some_and(|metadata| metadata.len != 0) {
                    keys.push(
                        source_keys
                            .and_then(|keys| keys.get(index))
                            .cloned()
                            .unwrap_or(AssocKeyValue::Int(index as i64)),
                    );
                }
            }
            Some(keys)
        }
        _ => None,
    }
}

pub(super) fn callback_value_kinds_for_assignment(
    value: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<Vec<ValueCellKind>> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_filter") || !(args.len() == 2 || args.len() == 3) {
        return None;
    }
    if args.len() == 3
        && !matches!(mode_value_for_locals(&args[2]), Some(1 | 2))
    {
        return None;
    }
    let values = match &args[0].kind {
        ExprKind::ArrayLiteral(items) => static_value_cell_kinds_for_items(items)?,
        ExprKind::Variable(source) => array_value_kinds.get(source)?.clone(),
        ExprKind::FunctionCall { name, .. }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "class_parents" | "class_implements" | "class_uses"
            ) =>
        {
            vec![ValueCellKind::Str]
        }
        _ => return None,
    };
    callback_value_kinds_are_homogeneous_supported(&values).then_some(values)
}

pub(super) fn function_return_value_kinds(
    value: &Expr,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> Option<Vec<ValueCellKind>> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_filter") || args.is_empty() || args.len() > 3 {
        return None;
    }
    if args.len() == 3
        && !matches!(mode_value_for_locals(&args[2]), Some(1 | 2))
    {
        return None;
    }
    let ExprKind::FunctionCall { name, .. } = &args[0].kind else {
        return None;
    };
    let key = function_key(name);
    let values = function_array_return_value_kinds
        .get(&key)
        .cloned()
        .or_else(|| {
            function_array_return_runtime_value_kinds
                .get(&key)
                .map(|kind| vec![*kind])
        })?;
    if args.len() == 1 || callback_value_kinds_are_homogeneous_supported(&values) {
        Some(values)
    } else {
        None
    }
}

pub(super) fn mode_value_for_locals(mode: &Expr) -> Option<i64> {
    match &mode.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::ConstRef(name) if name == "ARRAY_FILTER_USE_KEY" => Some(2),
        ExprKind::ConstRef(name) if name == "ARRAY_FILTER_USE_BOTH" => Some(1),
        _ => None,
    }
}

pub(super) fn default_assoc_metadata_for_assignment(
    value: &Expr,
) -> Option<(Vec<AssocKeyValue>, Vec<ValueCellKind>)> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_filter") || args.len() != 1 {
        return None;
    }
    let ExprKind::ArrayLiteralAssoc(items) = &args[0].kind else {
        return None;
    };
    let normalized = normalized_static_assoc_items(items)?;
    let mut keys = Vec::new();
    let mut values = Vec::new();
    for (key, value) in normalized {
        if static_value_cell_truthiness_for_filter(&value)? {
            keys.push(static_assoc_key_value_for_expr(&key)?);
            values.push(static_value_cell_kind_for_expr(&value)?);
        }
    }
    Some((keys, values))
}

pub(super) fn callback_assoc_metadata_for_assignment(
    value: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<(Vec<AssocKeyKind>, Vec<ValueCellKind>)> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_filter") || !(args.len() == 2 || args.len() == 3) {
        return None;
    }
    if args.len() == 3 && !matches!(mode_value_for_locals(&args[2]), Some(1 | 2)) {
        return None;
    }
    let key_mode = args.len() == 3 && mode_value_for_locals(&args[2]) == Some(2);
    match &args[0].kind {
        ExprKind::ArrayLiteralAssoc(items) => {
            let values = static_value_cell_kinds_for_assoc_items(items)?;
            if !callback_value_kinds_are_homogeneous_supported(&values) {
                return None;
            }
            let keys = static_assoc_key_kinds_for_items(items)?;
            if key_mode && assoc_key_kinds_are_mixed(&keys) {
                return None;
            }
            Some((keys, values))
        }
        ExprKind::Variable(source) => {
            let values = array_value_kinds.get(source)?.clone();
            if !callback_value_kinds_are_homogeneous_supported(&values) {
                return None;
            }
            let keys = array_key_kinds.get(source)?.clone();
            if key_mode && assoc_key_kinds_are_mixed(&keys) {
                return None;
            }
            Some((keys, values))
        }
        _ => None,
    }
}

pub(super) fn use_key_mixed_keys_for_assignment(
    value: &Expr,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> bool {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return false;
    };
    if !name.eq_ignore_ascii_case("array_filter")
        || args.len() != 3
        || mode_value_for_locals(&args[2]) != Some(2)
    {
        return false;
    }
    let key_kinds = match &args[0].kind {
        ExprKind::ArrayLiteralAssoc(items) => static_assoc_key_kinds_for_items(items),
        ExprKind::Variable(source) => array_key_kinds.get(source).cloned(),
        _ => None,
    };
    key_kinds.is_some_and(|kinds| assoc_key_kinds_are_mixed(&kinds))
}

fn assoc_key_kinds_are_mixed(kinds: &[AssocKeyKind]) -> bool {
    let Some(first) = kinds.first() else {
        return false;
    };
    kinds.iter().any(|kind| kind != first)
}

pub(super) fn callback_exact_nested_metadata_for_assignment(
    value: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    function_array_return_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    callable_targets: &HashMap<String, String>,
    string_static_values: &HashMap<String, String>,
) -> Option<(Vec<AssocKeyValue>, Vec<ValueCellKind>, Vec<Option<NestedArrayMetadata>>)> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_filter") || args.len() != 2 {
        return None;
    }
    let callback = static_callback_name_for_assignment_locals(
        &args[1],
        callable_targets,
        string_static_values,
    )?;
    if !callback.eq_ignore_ascii_case("is_array") && !callback.eq_ignore_ascii_case("is_iterable") {
        return None;
    }
    match &args[0].kind {
        ExprKind::ArrayLiteral(items) => {
            let mut keys = Vec::new();
            let mut values = Vec::new();
            let mut nested_values = Vec::new();
            for (index, item) in items.iter().enumerate() {
                if static_value_cell_kind_for_expr(item)? != ValueCellKind::Array {
                    return None;
                }
                keys.push(AssocKeyValue::Int(index as i64));
                values.push(ValueCellKind::Array);
                nested_values.push(static_nested_array_metadata_for_expr(item));
            }
            nested_values
                .iter()
                .any(Option::is_some)
                .then_some((keys, values, nested_values))
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            let normalized = normalized_static_assoc_items(items)?;
            let mut keys = Vec::new();
            let mut values = Vec::new();
            let mut nested_values = Vec::new();
            for (key, item) in normalized {
                if static_value_cell_kind_for_expr(&item)? != ValueCellKind::Array {
                    return None;
                }
                keys.push(static_assoc_key_value_for_expr(&key)?);
                values.push(ValueCellKind::Array);
                nested_values.push(static_nested_array_metadata_for_expr(&item));
            }
            nested_values
                .iter()
                .any(Option::is_some)
                .then_some((keys, values, nested_values))
        }
        ExprKind::Variable(source) => {
            let values = array_value_kinds.get(source)?;
            if !values.iter().all(|kind| *kind == ValueCellKind::Array) {
                return None;
            }
            let keys = array_key_values.get(source)?.clone();
            let nested_values = array_nested_values.get(source)?.clone();
            nested_values
                .iter()
                .any(Option::is_some)
                .then_some((keys, values.clone(), nested_values))
        }
        ExprKind::FunctionCall { name, .. } => {
            let key = function_key(name);
            let values = function_array_return_value_kinds.get(&key)?;
            if !values.iter().all(|kind| *kind == ValueCellKind::Array) {
                return None;
            }
            let keys = function_array_return_key_values.get(&key)?.clone();
            let nested_values = function_array_return_nested_values.get(&key)?.clone();
            nested_values
                .iter()
                .any(Option::is_some)
                .then_some((keys, values.clone(), nested_values))
        }
        _ => None,
    }
}

fn callback_value_kinds_are_homogeneous_supported(kinds: &[ValueCellKind]) -> bool {
    kinds.first().is_some_and(|first| {
        matches!(
            first,
            ValueCellKind::Int
                | ValueCellKind::Str
                | ValueCellKind::Bool
                | ValueCellKind::Float
                | ValueCellKind::Null
                | ValueCellKind::Array
        )
            && kinds.iter().all(|kind| kind == first)
    })
}
