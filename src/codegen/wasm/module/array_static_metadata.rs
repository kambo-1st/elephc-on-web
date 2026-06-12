//! Purpose:
//! Computes static WASM metadata for array literals, PHP array keys, and value cells.
//! Keeps literal normalization and compile-time key/value inference out of module state.
//!
//! Called from:
//! - `super` metadata collectors and sibling array metadata helpers.
//!
//! Key details:
//! - PHP duplicate-key and numeric-string key normalization is handled here.
//! - Unknown runtime-dependent expressions return `None` so callers preserve runtime paths.

use crate::parser::ast::{Expr, ExprKind};

use super::{AssocKeyKind, AssocKeyValue, ValueCellKind};

pub(super) fn static_value_cell_kinds_for_items(items: &[Expr]) -> Option<Vec<ValueCellKind>> {
    items.iter().map(static_value_cell_kind_for_expr).collect()
}

pub(super) fn static_value_cell_kinds_for_assoc_items(
    items: &[(Expr, Expr)],
) -> Option<Vec<ValueCellKind>> {
    let normalized = normalized_static_assoc_items(items)?;
    normalized
        .iter()
        .map(|(_, value)| static_value_cell_kind_for_expr(value))
        .collect()
}

pub(super) fn static_assoc_key_kinds_for_items(
    items: &[(Expr, Expr)],
) -> Option<Vec<AssocKeyKind>> {
    let normalized = normalized_static_assoc_items(items)?;
    normalized
        .iter()
        .map(|(key, _)| static_assoc_key_kind_for_expr(key))
        .collect()
}

pub(super) fn static_assoc_key_values_for_items(
    items: &[(Expr, Expr)],
) -> Option<Vec<AssocKeyValue>> {
    let normalized = normalized_static_assoc_items(items)?;
    normalized
        .iter()
        .map(|(key, _)| static_assoc_key_value_for_expr(key))
        .collect()
}

pub(super) fn normalized_static_assoc_items(items: &[(Expr, Expr)]) -> Option<Vec<(Expr, Expr)>> {
    let mut normalized: Vec<(AssocKeyValue, Expr, Expr)> = Vec::new();
    for (key, value) in items {
        let key_value = static_assoc_key_value_for_expr(key)?;
        if let Some((_, _, existing_value)) = normalized
            .iter_mut()
            .find(|(existing_key, _, _)| *existing_key == key_value)
        {
            *existing_value = value.clone();
        } else {
            normalized.push((key_value, key.clone(), value.clone()));
        }
    }
    Some(
        normalized
            .into_iter()
            .map(|(_, key, value)| (key, value))
            .collect(),
    )
}

pub(super) fn static_assoc_key_kind_for_expr(key: &Expr) -> Option<AssocKeyKind> {
    match &key.kind {
        ExprKind::IntLiteral(_) => Some(AssocKeyKind::Int),
        ExprKind::Negate(inner) if matches!(inner.kind, ExprKind::IntLiteral(_)) => {
            Some(AssocKeyKind::Int)
        }
        ExprKind::StringLiteral(value) => {
            if php_array_int_key(value).is_some() {
                Some(AssocKeyKind::Int)
            } else {
                Some(AssocKeyKind::Str)
            }
        }
        _ => None,
    }
}

pub(super) fn static_assoc_key_value_for_expr(key: &Expr) -> Option<AssocKeyValue> {
    match &key.kind {
        ExprKind::IntLiteral(value) => Some(AssocKeyValue::Int(*value)),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => value.checked_neg().map(AssocKeyValue::Int),
            _ => None,
        },
        ExprKind::StringLiteral(value) => php_array_int_key(value)
            .map(AssocKeyValue::Int)
            .or_else(|| Some(AssocKeyValue::Str(value.clone()))),
        _ => None,
    }
}

pub(super) fn php_array_int_key(value: &str) -> Option<i64> {
    if value.is_empty() || value.starts_with('+') {
        return None;
    }
    let digits = value.strip_prefix('-').unwrap_or(value);
    if digits.is_empty() || (digits.len() > 1 && digits.starts_with('0')) {
        return None;
    }
    if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse::<i64>().ok()
}

pub(super) fn static_value_cell_kind_for_expr(value: &Expr) -> Option<ValueCellKind> {
    match &value.kind {
        ExprKind::Null => Some(ValueCellKind::Null),
        ExprKind::FloatLiteral(_) => Some(ValueCellKind::Float),
        ExprKind::Negate(inner) if matches!(inner.kind, ExprKind::FloatLiteral(_)) => {
            Some(ValueCellKind::Float)
        }
        ExprKind::StringLiteral(_) => Some(ValueCellKind::Str),
        ExprKind::BoolLiteral(_) => Some(ValueCellKind::Bool),
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => Some(ValueCellKind::Array),
        ExprKind::IntLiteral(_) | ExprKind::ConstRef(_) => Some(ValueCellKind::Int),
        ExprKind::Negate(inner)
            if matches!(inner.kind, ExprKind::IntLiteral(_) | ExprKind::ConstRef(_)) =>
        {
            Some(ValueCellKind::Int)
        }
        _ => None,
    }
}

pub(super) fn static_value_cell_truthiness_for_filter(value: &Expr) -> Option<bool> {
    match &value.kind {
        ExprKind::Null => Some(false),
        ExprKind::BoolLiteral(value) => Some(*value),
        ExprKind::StringLiteral(value) => Some(!value.is_empty() && value != "0"),
        ExprKind::IntLiteral(value) => Some(*value != 0),
        ExprKind::Negate(inner) if matches!(inner.kind, ExprKind::IntLiteral(_)) => {
            static_value_cell_truthiness_for_filter(inner)
        }
        ExprKind::FloatLiteral(value) => Some(*value != 0.0),
        ExprKind::Negate(inner) if matches!(inner.kind, ExprKind::FloatLiteral(_)) => {
            static_value_cell_truthiness_for_filter(inner)
        }
        ExprKind::ArrayLiteral(items) => Some(!items.is_empty()),
        ExprKind::ArrayLiteralAssoc(items) => {
            normalized_static_assoc_items(items).map(|items| !items.is_empty())
        }
        _ => None,
    }
}

pub(super) fn static_or_const_int_value_for_locals(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        _ => None,
    }
}
