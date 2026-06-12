//! Purpose:
//! Infers foreach key metadata for wasm32-web array transforms that preserve keys.
//! Keeps recursive transform-key rules separate from local metadata orchestration.
//!
//! Called from:
//! - `super::foreach_key_local_kind()` and downstream array transform metadata helpers.
//!
//! Key details:
//! - Reuses callback, flip, merge, and associative-builder key metadata.
//! - Falls back to reverse-style key inference when a transform does not need special handling.

use super::*;

fn array_reverse_foreach_key_kinds(
    args: &[Expr],
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<Vec<AssocKeyKind>> {
    let source = args.first()?;
    let key_kinds = match &source.kind {
        ExprKind::ArrayLiteral(_) => None,
        ExprKind::ArrayLiteralAssoc(items) => static_assoc_key_kinds_for_items(items),
        ExprKind::Variable(name) => array_key_kinds.get(name).cloned(),
        ExprKind::FunctionCall { name, .. } => function_array_return_key_kinds
            .get(&function_key(name))
            .cloned(),
        _ => None,
    }?;
    if key_kinds.iter().all(|kind| *kind == AssocKeyKind::Str) {
        Some(key_kinds)
    } else {
        None
    }
}

pub(super) fn key_preserving_transform_foreach_key_kinds(
    args: &[Expr],
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<Vec<AssocKeyKind>> {
    let source = args.first()?;
    match &source.kind {
        ExprKind::FunctionCall { name, args }
            if matches!(name.to_ascii_lowercase().as_str(), "array_fill_keys" | "array_combine") =>
        {
            direct_assoc_builder_key_kinds_for_foreach(
                args,
                array_value_kinds,
                array_runtime_value_kinds,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_filter") => {
            array_filter_foreach_key_kinds(
                args,
                array_key_kinds,
                array_value_kinds,
                array_runtime_value_kinds,
                function_array_return_key_kinds,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_map") => {
            array_map_foreach_key_kinds(
                args,
                array_key_kinds,
                array_value_kinds,
                array_runtime_value_kinds,
                function_array_return_value_kinds,
                function_array_return_key_kinds,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_reverse") => {
            key_preserving_transform_foreach_key_kinds(
                args,
                array_key_kinds,
                array_value_kinds,
                array_runtime_value_kinds,
                function_array_return_value_kinds,
                function_array_return_key_kinds,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_unique") => {
            key_preserving_transform_foreach_key_kinds(
                args,
                array_key_kinds,
                array_value_kinds,
                array_runtime_value_kinds,
                function_array_return_value_kinds,
                function_array_return_key_kinds,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_flip") => {
            array_flip_foreach_key_kinds(args, array_value_kinds)
        }
        ExprKind::FunctionCall { name, args }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "array_diff_key" | "array_intersect_key"
            ) =>
        {
            key_preserving_transform_foreach_key_kinds(
                args,
                array_key_kinds,
                array_value_kinds,
                array_runtime_value_kinds,
                function_array_return_value_kinds,
                function_array_return_key_kinds,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_merge") => {
            match array_merge_foreach_key_local_kind(
                args,
                array_key_kinds,
                array_value_kinds,
                array_runtime_value_kinds,
                &HashSet::new(),
            ) {
                LocalKind::Str => Some(vec![AssocKeyKind::Str]),
                LocalKind::Mixed => Some(vec![AssocKeyKind::Int, AssocKeyKind::Str]),
                _ => Some(vec![AssocKeyKind::Int]),
            }
        }
        _ => array_reverse_foreach_key_kinds(
            args,
            array_key_kinds,
            function_array_return_key_kinds,
        ),
    }
}
