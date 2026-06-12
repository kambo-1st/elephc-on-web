//! Purpose:
//! Infers runtime value-cell metadata for wasm32-web array-producing expressions.
//! Keeps dynamic value-kind tracking separate from local metadata orchestration.
//!
//! Called from:
//! - `super::collect_stmt_locals()` and foreach metadata helpers.
//!
//! Key details:
//! - These helpers classify only shapes with known homogeneous runtime value cells.
//! - Unsupported or heterogeneous shapes stay unknown so lowering can reject or use broader paths.

use super::*;

pub(super) fn array_runtime_value_kind_for_assignment(
    value: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<ValueCellKind> {
    match &value.kind {
        ExprKind::FunctionCall { name, .. }
            if matches!(name.to_ascii_lowercase().as_str(), "explode" | "str_split") =>
        {
            Some(ValueCellKind::Str)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_keys") => {
            let key_kinds = match &args.first()?.kind {
                ExprKind::Variable(source) => array_key_kinds.get(source).cloned(),
                ExprKind::FunctionCall {
                    name: source_name,
                    args: source_args,
                } if matches!(
                    source_name.to_ascii_lowercase().as_str(),
                    "array_fill_keys" | "array_combine"
                ) =>
                {
                    if source_name.eq_ignore_ascii_case("array_fill_keys") {
                        direct_assoc_builder_key_kinds_for_foreach(
                            source_args,
                            array_value_kinds,
                            array_runtime_value_kinds,
                        )
                    } else {
                        direct_assoc_builder_key_kinds(source_args)
                    }
                }
                _ => None,
            };
            let key_kinds = key_kinds?;
            if key_kinds.iter().all(|kind| *kind == AssocKeyKind::Str) {
                Some(ValueCellKind::Str)
            } else if key_kinds.iter().all(|kind| *kind == AssocKeyKind::Int) {
                Some(ValueCellKind::Int)
            } else {
                None
            }
        }
        ExprKind::FunctionCall { name, args }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "array_unique" | "array_values" | "array_reverse" | "array_slice"
            ) =>
        {
            runtime_value_kind_for_first_arg(args, array_value_kinds, array_runtime_value_kinds)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_pad") => {
            let source_kind =
                runtime_value_kind_for_first_arg(args, array_value_kinds, array_runtime_value_kinds)?;
            let pad_kind = args.get(2).and_then(static_value_cell_kind_for_expr)?;
            (source_kind == pad_kind).then_some(source_kind)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_combine") => {
            runtime_value_kind_for_array_arg(
                args.get(1)?,
                array_value_kinds,
                array_runtime_value_kinds,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_fill_keys") => {
            args.get(1).and_then(static_value_cell_kind_for_expr)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_fill") => {
            args.get(2).and_then(static_value_cell_kind_for_expr)
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("array_chunk") => {
            Some(ValueCellKind::Array)
        }
        ExprKind::Variable(name) => array_runtime_value_kinds.get(name).copied(),
        _ => None,
    }
}

pub(super) fn array_keys_mixed_value_kinds_for_assignment(
    value: &Expr,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    php_normalized_key_arrays: &HashSet<String>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> bool {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return false;
    };
    if !name.eq_ignore_ascii_case("array_keys") {
        return false;
    }
    let Some(source) = args.first() else {
        return false;
    };
    let key_kinds = if expr_has_php_normalized_runtime_keys(
        source,
        php_normalized_key_arrays,
        array_value_kinds,
        array_runtime_value_kinds,
    ) {
        Some(vec![AssocKeyKind::Int, AssocKeyKind::Str])
    } else {
        match &source.kind {
            ExprKind::ArrayLiteralAssoc(items) => static_assoc_key_kinds_for_items(items),
            ExprKind::Variable(source) => array_key_kinds.get(source).cloned(),
            ExprKind::FunctionCall { name, args }
                if matches!(
                    name.to_ascii_lowercase().as_str(),
                    "array_fill_keys" | "array_combine"
                ) =>
            {
                direct_assoc_builder_php_normalized_key_kinds_for_foreach(
                    args,
                    array_value_kinds,
                    array_runtime_value_kinds,
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
            ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_filter") => {
                array_filter_foreach_key_kinds(
                    args,
                    array_key_kinds,
                    array_value_kinds,
                    array_runtime_value_kinds,
                    function_array_return_key_kinds,
                )
            }
            ExprKind::FunctionCall { name, .. } => function_array_return_key_kinds
                .get(&function_key(name))
                .cloned(),
            _ => None,
        }
    };
    key_kinds.is_some_and(|kinds| {
        kinds.iter().any(|kind| *kind == AssocKeyKind::Int)
            && kinds.iter().any(|kind| *kind == AssocKeyKind::Str)
    })
}

pub(super) fn array_map_runtime_value_kind_for_assignment(
    value: &Expr,
    function_return_kinds: &HashMap<String, ValueKind>,
    callable_targets: &HashMap<String, String>,
    string_static_values: &HashMap<String, String>,
    function_possible_static_string_returns: &HashMap<String, Vec<String>>,
) -> Option<ValueCellKind> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_map") || args.len() < 2 {
        return None;
    }
    match array_map_callback_expr_return_kind(
        args.first()?,
        function_return_kinds,
        callable_targets,
        string_static_values,
        function_possible_static_string_returns,
    )? {
        ValueKind::Int => Some(ValueCellKind::Int),
        ValueKind::Str => Some(ValueCellKind::Str),
        ValueKind::Bool => Some(ValueCellKind::Bool),
        ValueKind::Float => Some(ValueCellKind::Float),
        _ => None,
    }
}

pub(super) fn array_filter_array_map_runtime_value_kind_for_foreach(
    args: &[Expr],
    function_return_kinds: &HashMap<String, ValueKind>,
    callable_targets: &HashMap<String, String>,
    string_static_values: &HashMap<String, String>,
    function_possible_static_string_returns: &HashMap<String, Vec<String>>,
) -> Option<ValueCellKind> {
    if args.is_empty()
        || args.len() > 3
        || (args.len() == 3
            && !matches!(array_filter_metadata::mode_value_for_locals(&args[2]), Some(1 | 2)))
    {
        return None;
    }
    let ExprKind::FunctionCall {
        name: source_name,
        args: source_args,
    } = &args[0].kind
    else {
        return None;
    };
    if !source_name.eq_ignore_ascii_case("array_map") || source_args.len() < 2 {
        return None;
    }
    match array_map_callback_expr_return_kind(
        source_args.first()?,
        function_return_kinds,
        callable_targets,
        string_static_values,
        function_possible_static_string_returns,
    )? {
        ValueKind::Int => Some(ValueCellKind::Int),
        ValueKind::Str => Some(ValueCellKind::Str),
        ValueKind::Bool => Some(ValueCellKind::Bool),
        ValueKind::Float => Some(ValueCellKind::Float),
        _ => None,
    }
}

pub(super) fn array_filter_runtime_value_kind_for_assignment(
    value: &Expr,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_return_kinds: &HashMap<String, ValueKind>,
    callable_targets: &HashMap<String, String>,
    string_static_values: &HashMap<String, String>,
    function_possible_static_string_returns: &HashMap<String, Vec<String>>,
) -> Option<ValueCellKind> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_filter") || args.is_empty() || args.len() > 3 {
        return None;
    }
    if args.len() == 3
        && !matches!(array_filter_metadata::mode_value_for_locals(&args[2]), Some(1 | 2))
    {
        return None;
    }
    let source_kind = match &args[0].kind {
        ExprKind::Variable(source) => array_runtime_value_kinds.get(source).copied(),
        ExprKind::FunctionCall { name, .. } => array_return_expr_runtime_value_kind(
            &args[0],
            array_runtime_value_kinds,
        )
        .or_else(|| function_array_return_runtime_value_kinds.get(&function_key(name)).copied())
        .or_else(|| {
            array_filter_array_map_runtime_value_kind_for_foreach(
                args,
                function_return_kinds,
                callable_targets,
                string_static_values,
                function_possible_static_string_returns,
            )
        }),
        _ => None,
    }?;
    matches!(
        source_kind,
        ValueCellKind::Str
            | ValueCellKind::Bool
            | ValueCellKind::Float
            | ValueCellKind::Null
            | ValueCellKind::Array
    )
    .then_some(source_kind)
}
