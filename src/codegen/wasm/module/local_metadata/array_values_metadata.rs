//! Purpose:
//! Infers foreach value-local metadata for wasm32-web `array_values()` expressions.
//! Keeps value-reindexing transform rules separate from local metadata orchestration.
//!
//! Called from:
//! - `super::foreach_value_local_kind()` for direct and nested `array_values()` consumers.
//!
//! Key details:
//! - Reuses source value-cell metadata when `array_values()` reindexes without changing values.
//! - Delegates nested transforms such as `array_map()`, `array_keys()`, and `array_pad()`.

use super::*;

pub(super) fn array_values_foreach_value_local_kind(
    args: &[Expr],
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_return_kinds: &HashMap<String, ValueKind>,
    function_possible_static_string_returns: &HashMap<String, Vec<String>>,
    callable_targets: &HashMap<String, String>,
    string_static_values: &HashMap<String, String>,
) -> LocalKind {
    let Some(source) = args.first() else {
        return LocalKind::I64;
    };
    if let ExprKind::FunctionCall { name, args } = &source.kind {
        if matches!(
            name.to_ascii_lowercase().as_str(),
            "class_parents" | "class_implements" | "class_uses"
        ) {
            return LocalKind::Str;
        }
        if name.eq_ignore_ascii_case("array_map") {
            return array_map_foreach_value_local_kind(
                args,
                array_value_kinds,
                array_runtime_value_kinds,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_return_kinds,
                function_possible_static_string_returns,
                callable_targets,
                string_static_values,
            );
        }
        if name.eq_ignore_ascii_case("array_reverse") {
            let Some(source) = args.first() else {
                return LocalKind::I64;
            };
            return value_kinds_for_foreach_source(
                source,
                array_value_kinds,
                array_runtime_value_kinds,
            )
            .as_deref()
            .map(foreach_value_cell_local_kind)
            .unwrap_or(LocalKind::I64);
        }
        if name.eq_ignore_ascii_case("array_keys") {
            return array_keys_foreach_value_local_kind(
                args,
                &HashMap::new(),
                array_value_kinds,
                array_runtime_value_kinds,
                &HashSet::new(),
                &HashMap::new(),
                &HashMap::new(),
            );
        }
        if name.eq_ignore_ascii_case("array_values") {
            let Some(source) = args.first() else {
                return LocalKind::I64;
            };
            return value_kinds_for_foreach_source(
                source,
                array_value_kinds,
                array_runtime_value_kinds,
            )
            .as_deref()
            .map(foreach_value_cell_local_kind)
            .unwrap_or(LocalKind::I64);
        }
        if name.eq_ignore_ascii_case("array_unique") {
            let Some(source) = args.first() else {
                return LocalKind::I64;
            };
            return value_kinds_for_foreach_source(
                source,
                array_value_kinds,
                array_runtime_value_kinds,
            )
            .as_deref()
            .map(foreach_value_cell_local_kind)
            .unwrap_or(LocalKind::I64);
        }
        if name.eq_ignore_ascii_case("array_merge") {
            return value_kinds_for_foreach_source(
                source,
                array_value_kinds,
                array_runtime_value_kinds,
            )
            .as_deref()
            .map(foreach_value_cell_local_kind)
            .unwrap_or(LocalKind::I64);
        }
        if name.eq_ignore_ascii_case("array_pad") {
            return array_pad_foreach_value_local_kind(args, array_value_kinds, array_runtime_value_kinds);
        }
        if matches!(name.to_ascii_lowercase().as_str(), "array_diff" | "array_intersect") {
            return value_kinds_for_foreach_source(
                source,
                array_value_kinds,
                array_runtime_value_kinds,
            )
            .as_deref()
            .map(foreach_value_cell_local_kind)
            .unwrap_or(LocalKind::I64);
        }
        if matches!(
            name.to_ascii_lowercase().as_str(),
            "array_diff_key" | "array_intersect_key"
        ) {
            return value_kinds_for_foreach_source(
                source,
                array_value_kinds,
                array_runtime_value_kinds,
            )
            .as_deref()
            .map(foreach_value_cell_local_kind)
            .unwrap_or(LocalKind::I64);
        }
        if name.eq_ignore_ascii_case("array_column") {
            return array_column_value_kinds(args)
                .as_deref()
                .map(foreach_value_cell_local_kind)
                .unwrap_or(LocalKind::I64);
        }
        if name.eq_ignore_ascii_case("array_flip") {
            return array_flip_value_kinds(args, &HashMap::new())
                .as_deref()
                .map(foreach_value_cell_local_kind)
                .unwrap_or(LocalKind::I64);
        }
    }
    let kinds = match &source.kind {
        ExprKind::ArrayLiteral(items) => static_value_cell_kinds_for_items(items),
        ExprKind::ArrayLiteralAssoc(items) => static_value_cell_kinds_for_assoc_items(items),
        ExprKind::Variable(name) => {
            if let Some(kinds) = array_value_kinds.get(name).cloned() {
                Some(kinds)
            } else {
                array_runtime_value_kinds.get(name).map(|kind| vec![*kind])
            }
        }
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
        ExprKind::FunctionCall { name, .. }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "class_parents" | "class_implements" | "class_uses"
            ) =>
        {
            Some(vec![ValueCellKind::Str])
        }
        _ => None,
    };
    kinds
        .as_deref()
        .map(foreach_value_cell_local_kind)
        .unwrap_or(LocalKind::I64)
}
