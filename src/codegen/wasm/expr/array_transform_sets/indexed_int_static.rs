//! Purpose:
//! Plans static indexed-integer array set results for wasm32-web transforms.
//! Keeps compile-time item calculation separate from the wasm emission loops.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets`.
//!
//! Key details:
//! - These helpers preserve PHP source indexes for array_unique/diff/intersect.

use super::*;

pub(super) fn static_unique_int_array_items(
    expr: &Expr,
    items: &[Expr],
) -> Result<Vec<(Expr, Expr)>, CompileError> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let Some(value) = static_int_value(item) else {
            return Err(CompileError::new(
                item.span,
                "wasm32-web array_unique() currently requires static integer literal values",
            ));
        };
        if seen.contains(&value) {
            continue;
        }
        seen.push(value);
        out.push((
            Expr::new(ExprKind::IntLiteral(index as i64), expr.span),
            item.clone(),
        ));
    }
    Ok(out)
}

pub(super) fn static_array_diff_int_items(
    expr: &Expr,
    source_items: &[Expr],
    args: &[Expr],
) -> Result<Vec<(Expr, Expr)>, CompileError> {
    if args.len() < 2 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_diff() expects at least two arguments",
        ));
    }
    let mut compare_values = Vec::new();
    for arg in &args[1..] {
        let ExprKind::ArrayLiteral(items) = &arg.kind else {
            return Err(CompileError::new(
                arg.span,
                "wasm32-web array_diff() currently requires direct indexed integer literals",
            ));
        };
        for item in items {
            let Some(value) = static_int_value(item) else {
                return Err(CompileError::new(
                    item.span,
                    "wasm32-web array_diff() currently requires static integer literal values",
                ));
            };
            compare_values.push(value);
        }
    }
    let mut out = Vec::new();
    for (index, item) in source_items.iter().enumerate() {
        let Some(value) = static_int_value(item) else {
            return Err(CompileError::new(
                item.span,
                "wasm32-web array_diff() currently requires static integer literal values",
            ));
        };
        if !compare_values.contains(&value) {
            out.push((
                Expr::new(ExprKind::IntLiteral(index as i64), expr.span),
                item.clone(),
            ));
        }
    }
    Ok(out)
}

pub(super) fn static_array_intersect_int_items(
    expr: &Expr,
    source_items: &[Expr],
    args: &[Expr],
) -> Result<Vec<(Expr, Expr)>, CompileError> {
    if args.len() < 2 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_intersect() expects at least two arguments",
        ));
    }
    let mut compare_sets = Vec::new();
    for arg in &args[1..] {
        let ExprKind::ArrayLiteral(items) = &arg.kind else {
            return Err(CompileError::new(
                arg.span,
                "wasm32-web array_intersect() currently requires direct indexed integer literals",
            ));
        };
        let mut values = Vec::new();
        for item in items {
            let Some(value) = static_int_value(item) else {
                return Err(CompileError::new(
                    item.span,
                    "wasm32-web array_intersect() currently requires static integer literal values",
                ));
            };
            values.push(value);
        }
        compare_sets.push(values);
    }
    let mut out = Vec::new();
    for (index, item) in source_items.iter().enumerate() {
        let Some(value) = static_int_value(item) else {
            return Err(CompileError::new(
                item.span,
                "wasm32-web array_intersect() currently requires static integer literal values",
            ));
        };
        if compare_sets.iter().all(|values| values.contains(&value)) {
            out.push((
                Expr::new(ExprKind::IntLiteral(index as i64), expr.span),
                item.clone(),
            ));
        }
    }
    Ok(out)
}
