//! Purpose:
//! Infers value-cell kinds from foreach sources and array-returning expressions.
//! Keeps recursive source-shape discovery separate from the main local metadata collector.
//!
//! Called from:
//! - `super::foreach_value_local_kind()` and sibling array transform metadata helpers.
//!
//! Key details:
//! - Unknown dynamic sources return `None` so callers keep their existing conservative fallback.
//! - Homogeneous runtime arrays are represented by a single repeated `ValueCellKind`.

use super::*;

pub(super) fn array_return_value_kinds_for_foreach_source(
    source: &Expr,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> Option<Vec<ValueCellKind>> {
    match &source.kind {
        ExprKind::FunctionCall { name, .. } => {
            let key = function_key(name);
            function_array_return_value_kinds
                .get(&key)
                .cloned()
                .or_else(|| {
                    function_array_return_runtime_value_kinds
                        .get(&key)
                        .map(|kind| vec![*kind])
                })
        }
        _ => None,
    }
}

pub(super) fn array_filter_array_map_null_value_kinds_for_foreach(
    args: &[Expr],
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<Vec<ValueCellKind>> {
    if args.len() != 1 {
        return None;
    }
    let ExprKind::FunctionCall {
        name: source_name,
        args: source_args,
    } = &args[0].kind
    else {
        return None;
    };
    if !source_name.eq_ignore_ascii_case("array_map")
        || !matches!(source_args.first().map(|arg| &arg.kind), Some(ExprKind::Null))
        || source_args.len() <= 2
    {
        return None;
    }
    array_transform_metadata::array_map_null_multi_metadata_for_assignment(
        source_args,
        array_value_kinds,
        function_array_return_value_kinds,
    )
    .map(|(kinds, _)| kinds)
}

pub(in crate::codegen::wasm::module) fn value_kinds_for_foreach_source(
    source: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> Option<Vec<ValueCellKind>> {
    match &source.kind {
        ExprKind::ArrayLiteral(items) => static_value_cell_kinds_for_items(items),
        ExprKind::ArrayLiteralAssoc(items) => static_value_cell_kinds_for_assoc_items(items),
        ExprKind::Variable(name) => array_value_kinds
            .get(name)
            .cloned()
            .or_else(|| array_runtime_value_kinds.get(name).map(|kind| vec![*kind])),
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_combine") => {
            args.get(1)
                .and_then(|value| {
                    runtime_value_kind_for_array_arg(value, array_value_kinds, array_runtime_value_kinds)
                })
                .map(|kind| vec![kind])
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_fill_keys") => {
            args.get(1)
                .and_then(static_value_cell_kind_for_expr)
                .map(|kind| vec![kind])
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_fill") => {
            args.get(2)
                .and_then(static_value_cell_kind_for_expr)
                .map(|kind| vec![kind])
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_filter") => {
            args.first()
                .and_then(|source| {
                    value_kinds_for_foreach_source(
                        source,
                        array_value_kinds,
                        array_runtime_value_kinds,
                    )
                })
        }
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("array_map")
                && matches!(args.first().map(|arg| &arg.kind), Some(ExprKind::Null)) =>
        {
            if args.len() > 2 {
                return array_map_null_row_kinds_for_foreach(
                    args,
                    array_value_kinds,
                    &HashMap::new(),
                );
            }
            args.get(1)
                .and_then(|source| {
                    value_kinds_for_foreach_source(
                        source,
                        array_value_kinds,
                        array_runtime_value_kinds,
                    )
                })
        }
        ExprKind::FunctionCall { name, args }
            if matches!(name.to_ascii_lowercase().as_str(), "array_diff" | "array_intersect") =>
        {
            args.first().and_then(|source| {
                value_kinds_for_foreach_source(
                    source,
                    array_value_kinds,
                    array_runtime_value_kinds,
                )
            })
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_merge") => {
            let mut kinds = Vec::new();
            for arg in args {
                kinds.extend(value_kinds_for_foreach_source(
                    arg,
                    array_value_kinds,
                    array_runtime_value_kinds,
                )?);
            }
            Some(kinds)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_pad") => {
            let source_kind =
                runtime_value_kind_for_first_arg(args, array_value_kinds, array_runtime_value_kinds)?;
            let pad_kind = args.get(2).and_then(static_value_cell_kind_for_expr)?;
            (source_kind == pad_kind).then_some(vec![source_kind])
        }
        ExprKind::FunctionCall { name, args }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "array_diff_key" | "array_intersect_key"
            ) =>
        {
            args.first().and_then(|source| {
                value_kinds_for_foreach_source(
                    source,
                    array_value_kinds,
                    array_runtime_value_kinds,
                )
            })
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_column") => {
            array_column_value_kinds(args)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_flip") => {
            array_flip_value_kinds(args, &HashMap::new())
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

pub(super) fn array_map_null_row_kinds_for_foreach(
    args: &[Expr],
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<Vec<ValueCellKind>> {
    let len = args
        .iter()
        .skip(1)
        .filter_map(|source| {
            array_transform_metadata::known_value_kinds_for_array_map_null_source(
                source,
                array_value_kinds,
                function_array_return_value_kinds,
            )
            .map(|kinds| kinds.len())
        })
        .max()?;
    Some(vec![ValueCellKind::Array; len])
}

pub(super) fn runtime_value_kind_for_array_arg(
    value: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> Option<ValueCellKind> {
    match &value.kind {
        ExprKind::ArrayLiteral(items) => {
            let kinds = static_value_cell_kinds_for_items(items)?;
            let first = *kinds.first()?;
            kinds.iter().all(|kind| *kind == first).then_some(first)
        }
        ExprKind::Variable(name) => array_runtime_value_kinds.get(name).copied().or_else(|| {
            let kinds = array_value_kinds.get(name)?;
            let first = *kinds.first()?;
            kinds.iter().all(|kind| *kind == first).then_some(first)
        }),
        ExprKind::FunctionCall { .. } => array_return_expr_runtime_value_kind(value, array_runtime_value_kinds),
        _ => None,
    }
}

pub(super) fn runtime_value_kind_for_first_arg(
    args: &[Expr],
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> Option<ValueCellKind> {
    let source = args.first()?;
    match &source.kind {
        ExprKind::ArrayLiteral(items) => homogeneous_value_cell_kind(static_value_cell_kinds_for_items(items)?),
        ExprKind::ArrayLiteralAssoc(items) => {
            homogeneous_value_cell_kind(static_value_cell_kinds_for_assoc_items(items)?)
        }
        ExprKind::Variable(name) => {
            if let Some(kinds) = array_value_kinds.get(name) {
                homogeneous_value_cell_kind(kinds.clone())
            } else {
                array_runtime_value_kinds.get(name).copied()
            }
        }
        ExprKind::FunctionCall { .. } => array_return_expr_runtime_value_kind(source, array_runtime_value_kinds),
        _ => None,
    }
}

fn homogeneous_value_cell_kind(kinds: Vec<ValueCellKind>) -> Option<ValueCellKind> {
    let first = kinds.first().copied()?;
    kinds.iter().all(|kind| *kind == first).then_some(first)
}
