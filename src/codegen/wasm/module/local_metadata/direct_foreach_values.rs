//! Purpose:
//! Infers foreach value-local kinds for direct wasm32-web array-producing builtins.
//! Keeps small builtin-specific metadata rules out of the main local metadata pass.
//!
//! Called from:
//! - `super::foreach_value_local_kind()` and array transform metadata collection.
//!
//! Key details:
//! - These helpers only classify statically known value/key shapes.
//! - Unknown runtime layouts fall back conservatively to integer/mixed-compatible locals.

use super::*;

pub(super) fn array_pad_foreach_value_local_kind(
    args: &[Expr],
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> LocalKind {
    let (Some(source), Some(value)) = (args.first(), args.get(2)) else {
        return LocalKind::I64;
    };
    if let Some(source_kind) =
        runtime_value_kind_for_first_arg(args, array_value_kinds, array_runtime_value_kinds)
    {
        if static_value_cell_kind_for_expr(value) == Some(source_kind) {
            return local_kind_for_value_cell(source_kind);
        }
    }
    let mut kinds = match &source.kind {
        ExprKind::ArrayLiteral(items) => static_value_cell_kinds_for_items(items),
        ExprKind::Variable(name) => array_value_kinds.get(name).cloned(),
        _ => None,
    }
    .unwrap_or_default();
    let Some(pad_kind) = static_value_cell_kind_for_expr(value) else {
        return LocalKind::I64;
    };
    kinds.push(pad_kind);
    foreach_value_cell_local_kind(&kinds)
}

pub(super) fn array_fill_foreach_value_local_kind(args: &[Expr]) -> LocalKind {
    let Some(value) = args.get(2) else {
        return LocalKind::I64;
    };
    static_value_cell_kind_for_expr(value)
        .map(local_kind_for_value_cell)
        .unwrap_or(LocalKind::I64)
}

pub(super) fn range_foreach_value_local_kind(
    args: &[Expr],
    locals: &HashMap<String, LocalKind>,
) -> LocalKind {
    if args.len() < 2
        || !range_endpoint_is_stringy(&args[0], locals)
        || !range_endpoint_is_stringy(&args[1], locals)
    {
        return LocalKind::I64;
    }
    if matches!(&args[0].kind, ExprKind::StringLiteral(start) if start.is_empty())
        || matches!(&args[1].kind, ExprKind::StringLiteral(end) if end.is_empty())
    {
        return LocalKind::I64;
    }
    LocalKind::Str
}

fn range_endpoint_is_stringy(expr: &Expr, locals: &HashMap<String, LocalKind>) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_) => true,
        ExprKind::Variable(name) => locals.get(name) == Some(&LocalKind::Str),
        ExprKind::BinaryOp {
            left,
            op: BinOp::Concat,
            right,
        } => range_endpoint_is_stringy(left, locals) && range_endpoint_is_stringy(right, locals),
        _ => false,
    }
}

pub(super) fn array_keys_foreach_value_local_kind(
    args: &[Expr],
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    php_normalized_key_arrays: &HashSet<String>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> LocalKind {
    let Some(source) = args.first() else {
        return LocalKind::I64;
    };
    let key_kinds = match &source.kind {
        ExprKind::ArrayLiteral(_) => None,
        ExprKind::ArrayLiteralAssoc(items) => static_assoc_key_kinds_for_items(items),
        ExprKind::Variable(name) => array_key_kinds.get(name).cloned(),
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_map") => {
            array_map_foreach_key_kinds(
                args,
                array_key_kinds,
                array_value_kinds,
                array_runtime_value_kinds,
                function_array_return_value_kinds,
                function_array_return_key_kinds,
                None,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_filter") => {
            array_filter_foreach_key_kinds(
                args,
                array_key_kinds,
                array_value_kinds,
                array_runtime_value_kinds,
                function_array_return_key_kinds,
                None,
            )
        }
        ExprKind::FunctionCall { name, args }
            if matches!(name.to_ascii_lowercase().as_str(), "array_fill_keys" | "array_combine") =>
        {
            direct_assoc_builder_key_kinds_for_foreach(
                args,
                array_value_kinds,
                array_runtime_value_kinds,
            )
        }
        ExprKind::FunctionCall { name, .. }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "class_parents" | "class_implements" | "class_uses"
            ) =>
        {
            Some(vec![AssocKeyKind::Str])
        }
        ExprKind::FunctionCall { name, .. } => function_array_return_key_kinds
            .get(&function_key(name))
            .cloned(),
        _ => None,
    };
    if expr_has_marked_php_normalized_runtime_keys(source, php_normalized_key_arrays) {
        return LocalKind::Mixed;
    }
    let Some(key_kinds) = key_kinds else {
        return LocalKind::I64;
    };
    if key_kinds.iter().all(|kind| *kind == AssocKeyKind::Str) {
        LocalKind::Str
    } else if key_kinds.iter().all(|kind| *kind == AssocKeyKind::Int) {
        LocalKind::I64
    } else {
        LocalKind::Mixed
    }
}
