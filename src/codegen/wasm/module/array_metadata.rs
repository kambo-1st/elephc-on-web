//! Purpose:
//! Computes WASM-only array metadata for nested value-cell arrays.
//! Keeps array_chunk/array_merge/foreach metadata planning out of module state plumbing.
//!
//! Called from:
//! - `super::collect_stmt_locals()`
//!
//! Key details:
//! - These helpers are compile-time metadata only; emitted runtime behavior remains in expr/stmt lowering.

use std::collections::HashMap;

use crate::parser::ast::{Expr, ExprKind};

use super::{
    array_transform_metadata::array_map_null_multi_metadata_for_assignment, function_key,
    static_assoc_key_values_for_items,
    static_nested_array_metadata_for_assoc_items, static_nested_array_metadata_for_items,
    static_value_cell_kind_for_expr, static_value_cell_kinds_for_assoc_items,
    static_value_cell_kinds_for_items, ArrayLayout, AssocKeyValue, NestedArrayMetadata,
    ValueCellKind,
};

pub(super) fn array_merge_nested_values_for_assignment(
    value: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
) -> Option<Vec<Option<NestedArrayMetadata>>> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_merge") {
        return None;
    }
    let mut nested_values = Vec::new();
    for arg in args {
        nested_values.extend(nested_values_for_array_expr(arg, array_nested_values)?);
    }
    (!nested_values.is_empty()).then_some(nested_values)
}

pub(super) fn array_chunk_nested_values_for_assignment(
    value: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) -> Option<Vec<Option<NestedArrayMetadata>>> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_chunk") {
        return None;
    }
    let source = args.first()?;
    let chunk_size = literal_positive_usize(args.get(1)?)?;
    if let Some(source_nested_values) = nested_values_for_array_expr(source, array_nested_values) {
        return Some(chunked_nested_values_for_foreach(&source_nested_values, chunk_size));
    }
    let source_value_kinds =
        value_kinds_for_nested_chunk_source(source, array_value_kinds, function_array_return_value_kinds)?;
    let source_key_values =
        key_values_for_nested_chunk_source(source, array_key_values, function_array_return_key_values);
    Some(chunked_scalar_values_for_foreach(
        &source_value_kinds,
        source_key_values.as_deref(),
        chunk_size,
        literal_true(args.get(2)),
    ))
}

fn nested_values_for_array_expr(
    value: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
) -> Option<Vec<Option<NestedArrayMetadata>>> {
    match &value.kind {
        ExprKind::ArrayLiteral(items) => static_nested_array_metadata_for_items(items),
        ExprKind::ArrayLiteralAssoc(items) => static_nested_array_metadata_for_assoc_items(items),
        ExprKind::Variable(source) => array_nested_values.get(source).cloned(),
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("array_merge") => {
            array_merge_nested_values_for_assignment(value, array_nested_values)
        }
        _ => None,
    }
}

pub(super) fn foreach_array_value_metadata_for_locals(
    array: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_runtime_nested_values: &HashMap<String, NestedArrayMetadata>,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) -> Option<NestedArrayMetadata> {
    nested_values_for_foreach_array_expr(
        array,
        array_nested_values,
        array_value_kinds,
        array_key_values,
        function_array_return_value_kinds,
        function_array_return_key_values,
    )
    .and_then(|values| values.into_iter().flatten().next())
    .or_else(|| {
        array_chunk_runtime_nested_value_for_assignment(
            array,
            array_value_kinds,
            array_runtime_value_kinds,
            function_array_return_value_kinds,
            function_array_return_runtime_value_kinds,
        )
    })
    .or_else(|| match &array.kind {
        ExprKind::Variable(name) => array_runtime_nested_values.get(name).cloned(),
        _ => None,
    })
}

pub(super) fn nested_values_for_foreach_array_expr(
    value: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) -> Option<Vec<Option<NestedArrayMetadata>>> {
    match &value.kind {
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("array_map")
                && matches!(args.first().map(|arg| &arg.kind), Some(ExprKind::Null))
                && args.len() > 2 =>
        {
            array_map_null_multi_metadata_for_assignment(
                args,
                array_value_kinds,
                function_array_return_value_kinds,
            )
            .map(|(_, nested)| nested)
        }
        ExprKind::FunctionCall { name, args }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "array_values" | "array_slice"
            ) =>
        {
            nested_values_for_foreach_array_expr(
                args.first()?,
                array_nested_values,
                array_value_kinds,
                array_key_values,
                function_array_return_value_kinds,
                function_array_return_key_values,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_reverse") => {
            let mut nested_values = nested_values_for_foreach_array_expr(
                args.first()?,
                array_nested_values,
                array_value_kinds,
                array_key_values,
                function_array_return_value_kinds,
                function_array_return_key_values,
            )?;
            nested_values.reverse();
            Some(nested_values)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_merge") => {
            let mut nested_values = Vec::new();
            for arg in args {
                nested_values.extend(nested_values_for_foreach_array_expr(
                    arg,
                    array_nested_values,
                    array_value_kinds,
                    array_key_values,
                    function_array_return_value_kinds,
                    function_array_return_key_values,
                )?);
            }
            (!nested_values.is_empty()).then_some(nested_values)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_filter") => {
            let source_nested_values = nested_values_for_foreach_array_expr(
                args.first()?,
                array_nested_values,
                array_value_kinds,
                array_key_values,
                function_array_return_value_kinds,
                function_array_return_key_values,
            )?;
            let nested_values = source_nested_values
                .into_iter()
                .filter_map(|metadata| {
                    metadata
                        .filter(|metadata| metadata.len != 0)
                        .map(Some)
                })
                .collect::<Vec<_>>();
            nested_values.iter().any(Option::is_some).then_some(nested_values)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_chunk") => {
            let source = args.first()?;
            let chunk_size = literal_positive_usize(args.get(1)?)?;
            if let Some(source_nested_values) = nested_values_for_foreach_array_expr(
                source,
                array_nested_values,
                array_value_kinds,
                array_key_values,
                function_array_return_value_kinds,
                function_array_return_key_values,
            ) {
                return Some(chunked_nested_values_for_foreach(
                    &source_nested_values,
                    chunk_size,
                ));
            }
            let source_value_kinds =
                value_kinds_for_nested_chunk_source(source, array_value_kinds, function_array_return_value_kinds)?;
            let source_key_values =
                key_values_for_nested_chunk_source(source, array_key_values, function_array_return_key_values);
            Some(chunked_scalar_values_for_foreach(
                &source_value_kinds,
                source_key_values.as_deref(),
                chunk_size,
                literal_true(args.get(2)),
            ))
        }
        _ => nested_values_for_array_expr(value, array_nested_values),
    }
}

fn key_values_for_nested_chunk_source(
    source: &Expr,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    function_array_return_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) -> Option<Vec<AssocKeyValue>> {
    match &source.kind {
        ExprKind::ArrayLiteralAssoc(items) => static_assoc_key_values_for_items(items),
        ExprKind::Variable(name) => array_key_values.get(name).cloned(),
        ExprKind::FunctionCall { name, .. } => function_array_return_key_values
            .get(&function_key(name))
            .cloned(),
        _ => None,
    }
}

fn value_kinds_for_nested_chunk_source(
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

pub(super) fn array_chunk_runtime_nested_value_for_assignment(
    value: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> Option<NestedArrayMetadata> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_chunk") || literal_positive_usize(args.get(1)?).is_some() {
        return None;
    }
    let source = args.first()?;
    let preserve_keys = literal_true(args.get(2));
    let kind = runtime_chunk_source_kind(
        source,
        array_value_kinds,
        array_runtime_value_kinds,
        function_array_return_value_kinds,
        function_array_return_runtime_value_kinds,
    );
    if kind.is_none()
        && preserve_keys
        && runtime_chunk_source_has_value_cells(
            source,
            array_value_kinds,
            function_array_return_value_kinds,
        )
    {
        return Some(NestedArrayMetadata {
            layout: ArrayLayout::Assoc,
            len: usize::MAX,
            value_kinds: None,
            key_values: None,
            nested_values: None,
        });
    }
    let kind = kind?;
    Some(NestedArrayMetadata {
        layout: if preserve_keys {
            ArrayLayout::Assoc
        } else {
            ArrayLayout::Value
        },
        len: usize::MAX,
        value_kinds: Some(vec![kind]),
        key_values: None,
        nested_values: None,
    })
}

fn runtime_chunk_source_has_value_cells(
    source: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> bool {
    if value_kinds_for_nested_chunk_source(source, array_value_kinds, function_array_return_value_kinds)
        .is_some()
    {
        return true;
    }
    match &source.kind {
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_combine") => {
            let Some(value_arg) = args.get(1) else {
                return false;
            };
            value_kinds_for_nested_chunk_source(
                value_arg,
                array_value_kinds,
                function_array_return_value_kinds,
            )
            .is_some()
        }
        _ => false,
    }
}

fn runtime_chunk_source_kind(
    source: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> Option<ValueCellKind> {
    value_kinds_for_nested_chunk_source(source, array_value_kinds, function_array_return_value_kinds)
        .and_then(|kinds| {
            let first = kinds.first().copied()?;
            kinds.iter().all(|kind| *kind == first).then_some(first)
        })
        .or_else(|| match &source.kind {
            ExprKind::Variable(name) => array_runtime_value_kinds.get(name).copied(),
            ExprKind::FunctionCall { name, args } => direct_runtime_builder_value_kind(name, args)
                .or_else(|| {
                    function_array_return_runtime_value_kinds
                        .get(&function_key(name))
                        .copied()
                }),
            _ => None,
        })
}

fn direct_runtime_builder_value_kind(name: &str, args: &[Expr]) -> Option<ValueCellKind> {
    match name.to_ascii_lowercase().as_str() {
        "array_fill" => static_value_cell_kind_for_expr(args.get(2)?),
        "array_fill_keys" => static_value_cell_kind_for_expr(args.get(1)?),
        "array_combine" => {
            let ExprKind::ArrayLiteral(items) = &args.get(1)?.kind else {
                return None;
            };
            let kinds = static_value_cell_kinds_for_items(items)?;
            let first = kinds.first().copied()?;
            kinds.iter().all(|kind| *kind == first).then_some(first)
        }
        _ => None,
    }
}

fn literal_true(value: Option<&Expr>) -> bool {
    matches!(value.map(|expr| &expr.kind), Some(ExprKind::BoolLiteral(true)))
}

fn literal_positive_usize(value: &Expr) -> Option<usize> {
    let ExprKind::IntLiteral(value) = value.kind else {
        return None;
    };
    usize::try_from(value).ok().filter(|value| *value > 0)
}

fn chunked_nested_values_for_foreach(
    source_nested_values: &[Option<NestedArrayMetadata>],
    chunk_size: usize,
) -> Vec<Option<NestedArrayMetadata>> {
    source_nested_values
        .chunks(chunk_size)
        .map(|chunk| {
            let value_kinds = chunk
                .iter()
                .all(Option::is_some)
                .then(|| vec![ValueCellKind::Array; chunk.len()]);
            Some(NestedArrayMetadata {
                layout: ArrayLayout::Value,
                len: chunk.len(),
                value_kinds,
                key_values: None,
                nested_values: Some(chunk.to_vec()),
            })
        })
        .collect()
}

fn chunked_scalar_values_for_foreach(
    source_value_kinds: &[ValueCellKind],
    source_key_values: Option<&[AssocKeyValue]>,
    chunk_size: usize,
    preserve_keys: bool,
) -> Vec<Option<NestedArrayMetadata>> {
    source_value_kinds
        .chunks(chunk_size)
        .enumerate()
        .map(|(chunk_index, chunk)| {
            let start = chunk_index * chunk_size;
            Some(NestedArrayMetadata {
                layout: if preserve_keys {
                    ArrayLayout::Assoc
                } else {
                    ArrayLayout::Value
                },
                len: chunk.len(),
                value_kinds: Some(chunk.to_vec()),
                key_values: preserve_keys.then(|| {
                    source_key_values
                        .map(|keys| keys[start..start + chunk.len()].to_vec())
                        .unwrap_or_else(|| {
                            (start..start + chunk.len())
                                .map(|index| AssocKeyValue::Int(index as i64))
                                .collect()
                        })
                }),
                nested_values: None,
            })
        })
        .collect()
}
