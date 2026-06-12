//! Purpose:
//! Infers associative key metadata for WASM array-building builtins.
//! Keeps PHP key normalization helpers out of the main local metadata collector.
//!
//! Called from:
//! - `super::collect_stmt_locals()` and foreach metadata inference helpers.
//!
//! Key details:
//! - Static string keys are normalized with PHP array-key rules before metadata is stored.
//! - Runtime string-key producers stay conservative when integer-string conversion is possible.

use super::*;

pub(in crate::codegen::wasm::module) fn assoc_key_kind_for_value(
    value: &AssocKeyValue,
) -> AssocKeyKind {
    match value {
        AssocKeyValue::Int(_) => AssocKeyKind::Int,
        AssocKeyValue::Str(_) => AssocKeyKind::Str,
    }
}

pub(super) fn direct_assoc_builder_key_kinds(args: &[Expr]) -> Option<Vec<AssocKeyKind>> {
    if let Some(values) = direct_assoc_builder_key_values(args) {
        return Some(values.iter().map(assoc_key_kind_for_value).collect());
    }
    match &args.first()?.kind {
        ExprKind::ArrayLiteral(keys) => keys.iter().map(static_assoc_key_kind_for_expr).collect(),
        _ => None,
    }
}

fn direct_assoc_builder_key_values(args: &[Expr]) -> Option<Vec<AssocKeyValue>> {
    match &args.first()?.kind {
        ExprKind::ArrayLiteral(keys) => {
            let mut normalized = Vec::<AssocKeyValue>::new();
            for key in keys {
                push_normalized_assoc_key(&mut normalized, static_assoc_key_value_for_expr(key)?);
            }
            Some(normalized)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("explode") => {
            static_explode_key_values(args)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("str_split") => {
            static_str_split_key_values(args)
        }
        _ => None,
    }
}

fn static_explode_key_values(args: &[Expr]) -> Option<Vec<AssocKeyValue>> {
    if args.len() < 2 || args.len() > 3 {
        return None;
    }
    let separator = static_string_literal(args.first()?)?;
    if separator.is_empty() {
        return None;
    }
    let value = static_string_literal(args.get(1)?)?;
    let limit = args.get(2).and_then(static_int_literal);
    let parts = split_static_explode(value, separator, limit);
    assoc_key_values_for_static_string_keys(parts)
}

fn static_str_split_key_values(args: &[Expr]) -> Option<Vec<AssocKeyValue>> {
    if args.is_empty() || args.len() > 2 {
        return None;
    }
    let value = static_string_literal(args.first()?)?;
    let chunk = args.get(1).and_then(static_int_literal).unwrap_or(1);
    if chunk <= 0 {
        return None;
    }
    let chunk = chunk as usize;
    let bytes = value.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let end = (index + chunk).min(bytes.len());
        out.push(String::from_utf8_lossy(&bytes[index..end]).to_string());
        index = end;
    }
    assoc_key_values_for_static_string_keys(out)
}

fn static_string_literal(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::StringLiteral(value) => Some(value.clone()),
        _ => None,
    }
}

fn static_int_literal(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => value.checked_neg(),
            _ => None,
        },
        _ => None,
    }
}

fn split_static_explode(value: String, separator: String, limit: Option<i64>) -> Vec<String> {
    let parts = value.split(separator.as_str()).collect::<Vec<_>>();
    match limit {
        None => parts.into_iter().map(str::to_string).collect(),
        Some(0) => vec![value],
        Some(limit) if limit > 0 => {
            if limit == 1 {
                vec![value]
            } else {
                value
                    .splitn(limit as usize, separator.as_str())
                    .map(str::to_string)
                    .collect()
            }
        }
        Some(limit) => {
            let keep = parts.len().saturating_sub((-limit) as usize);
            parts.into_iter().take(keep).map(str::to_string).collect()
        }
    }
}

fn assoc_key_values_for_static_string_keys(keys: Vec<String>) -> Option<Vec<AssocKeyValue>> {
    let mut normalized = Vec::<AssocKeyValue>::new();
    for key in keys {
        let key = php_array_int_key(&key)
            .map(AssocKeyValue::Int)
            .unwrap_or(AssocKeyValue::Str(key));
        push_normalized_assoc_key(&mut normalized, key);
    }
    Some(normalized)
}

fn push_normalized_assoc_key(keys: &mut Vec<AssocKeyValue>, key: AssocKeyValue) {
    if let Some(existing) = keys.iter_mut().find(|existing_key| **existing_key == key) {
        *existing = key;
    } else {
        keys.push(key);
    }
}

pub(super) fn assoc_builder_key_values_for_assignment(
    value: &Expr,
) -> Option<Vec<AssocKeyValue>> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !matches!(name.to_ascii_lowercase().as_str(), "array_fill_keys" | "array_combine") {
        return None;
    }
    direct_assoc_builder_key_values(args)
}

pub(in crate::codegen::wasm::module) fn direct_assoc_builder_key_kinds_for_foreach(
    args: &[Expr],
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> Option<Vec<AssocKeyKind>> {
    direct_assoc_builder_key_kinds(args).or_else(|| match &args.first()?.kind {
        ExprKind::FunctionCall { name, .. }
            if matches!(name.to_ascii_lowercase().as_str(), "explode" | "str_split") =>
        {
            Some(vec![AssocKeyKind::Str])
        }
        ExprKind::Variable(source)
            if array_runtime_value_kinds.get(source) == Some(&ValueCellKind::Str)
                || array_value_kinds
                    .get(source)
                    .is_some_and(|kinds| kinds.iter().all(|kind| *kind == ValueCellKind::Str)) =>
        {
            Some(vec![AssocKeyKind::Str])
        }
        _ => None,
    })
}

pub(super) fn direct_assoc_builder_php_normalized_key_kinds_for_foreach(
    args: &[Expr],
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> Option<Vec<AssocKeyKind>> {
    direct_assoc_builder_key_kinds(args).or_else(|| match &args.first()?.kind {
        ExprKind::FunctionCall { name, .. }
            if matches!(name.to_ascii_lowercase().as_str(), "explode" | "str_split") =>
        {
            Some(vec![AssocKeyKind::Int, AssocKeyKind::Str])
        }
        ExprKind::Variable(source)
            if array_runtime_value_kinds.get(source) == Some(&ValueCellKind::Str)
                || array_value_kinds
                    .get(source)
                    .is_some_and(|kinds| kinds.iter().all(|kind| *kind == ValueCellKind::Str)) =>
        {
            Some(vec![AssocKeyKind::Int, AssocKeyKind::Str])
        }
        _ => None,
    })
}

pub(super) fn assoc_builder_key_kinds_for_assignment(
    value: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> Option<Vec<AssocKeyKind>> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !matches!(name.to_ascii_lowercase().as_str(), "array_fill_keys" | "array_combine") {
        return None;
    }
    if name.eq_ignore_ascii_case("array_fill_keys") {
        return array_fill_keys_key_kinds_for_assignment(
            args,
            array_value_kinds,
            array_runtime_value_kinds,
        );
    }
    direct_assoc_builder_key_kinds(args).or_else(|| {
        let ExprKind::Variable(source) = &args.first()?.kind else {
            return None;
        };
        if array_runtime_value_kinds.get(source) == Some(&ValueCellKind::Str)
            || array_value_kinds
                .get(source)
                .is_some_and(|kinds| kinds.iter().all(|kind| *kind == ValueCellKind::Str))
        {
            Some(vec![AssocKeyKind::Str])
        } else {
            None
        }
    })
}

fn assoc_builder_has_php_normalized_runtime_keys(
    value: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> bool {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return false;
    };
    if !matches!(name.to_ascii_lowercase().as_str(), "array_fill_keys" | "array_combine") {
        return false;
    }
    if let Some(values) = direct_assoc_builder_key_values(args) {
        let has_int = values.iter().any(|value| matches!(value, AssocKeyValue::Int(_)));
        let has_str = values.iter().any(|value| matches!(value, AssocKeyValue::Str(_)));
        return has_int && has_str;
    }
    match &args.first().map(|arg| &arg.kind) {
        Some(ExprKind::FunctionCall { name, .. })
            if matches!(name.to_ascii_lowercase().as_str(), "explode" | "str_split") =>
        {
            true
        }
        Some(ExprKind::Variable(source)) => {
            array_runtime_value_kinds.get(source) == Some(&ValueCellKind::Str)
                || array_value_kinds
                    .get(source)
                    .is_some_and(|kinds| kinds.iter().all(|kind| *kind == ValueCellKind::Str))
        }
        _ => false,
    }
}

pub(super) fn expr_has_php_normalized_runtime_keys(
    value: &Expr,
    php_normalized_key_arrays: &HashSet<String>,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> bool {
    match &value.kind {
        ExprKind::Variable(name) => php_normalized_key_arrays.contains(name),
        ExprKind::FunctionCall { name, args }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "array_reverse" | "array_slice" | "array_filter" | "array_unique"
            ) =>
        {
            args.first().is_some_and(|source| {
                expr_has_php_normalized_runtime_keys(
                    source,
                    php_normalized_key_arrays,
                    array_value_kinds,
                    array_runtime_value_kinds,
                )
            })
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_map") => {
            args.get(1).is_some_and(|source| {
                expr_has_php_normalized_runtime_keys(
                    source,
                    php_normalized_key_arrays,
                    array_value_kinds,
                    array_runtime_value_kinds,
                )
            })
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_pad") => {
            args.first().is_some_and(|source| {
                expr_has_php_normalized_runtime_keys(
                    source,
                    php_normalized_key_arrays,
                    array_value_kinds,
                    array_runtime_value_kinds,
                )
            })
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_merge") => {
            args.iter().any(|source| {
                expr_has_php_normalized_runtime_keys(
                    source,
                    php_normalized_key_arrays,
                    array_value_kinds,
                    array_runtime_value_kinds,
                )
            })
        }
        _ => assoc_builder_has_php_normalized_runtime_keys(
            value,
            array_value_kinds,
            array_runtime_value_kinds,
        ),
    }
}

pub(super) fn expr_has_marked_php_normalized_runtime_keys(
    value: &Expr,
    php_normalized_key_arrays: &HashSet<String>,
) -> bool {
    match &value.kind {
        ExprKind::Variable(name) => php_normalized_key_arrays.contains(name),
        ExprKind::FunctionCall { name, args }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "array_reverse" | "array_slice" | "array_filter" | "array_unique"
            ) =>
        {
            args.first()
                .is_some_and(|source| expr_has_marked_php_normalized_runtime_keys(
                    source,
                    php_normalized_key_arrays,
                ))
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_map") => {
            args.get(1)
                .is_some_and(|source| expr_has_marked_php_normalized_runtime_keys(
                    source,
                    php_normalized_key_arrays,
                ))
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_pad") => {
            args.first()
                .is_some_and(|source| expr_has_marked_php_normalized_runtime_keys(
                    source,
                    php_normalized_key_arrays,
                ))
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_merge") => {
            args.iter().any(|source| {
                expr_has_marked_php_normalized_runtime_keys(source, php_normalized_key_arrays)
            })
        }
        _ => false,
    }
}

fn array_fill_keys_key_kinds_for_assignment(
    args: &[Expr],
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> Option<Vec<AssocKeyKind>> {
    direct_assoc_builder_key_kinds(args).or_else(|| match &args.first()?.kind {
        ExprKind::FunctionCall { name, .. }
            if matches!(name.to_ascii_lowercase().as_str(), "explode" | "str_split") =>
        {
            Some(vec![AssocKeyKind::Str])
        }
        ExprKind::Variable(source)
            if array_runtime_value_kinds.get(source) == Some(&ValueCellKind::Str)
                || array_value_kinds
                    .get(source)
                    .is_some_and(|kinds| kinds.iter().all(|kind| *kind == ValueCellKind::Str)) =>
        {
            Some(vec![AssocKeyKind::Str])
        }
        _ => None,
    })
}
