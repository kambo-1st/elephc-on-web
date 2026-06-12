//! Purpose:
//! Shapes static array transform and set-operation item lists for wasm32-web.
//! Keeps literal array_flip, array_unique, and array_reverse item rewriting out of the emitter.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets`.
//!
//! Key details:
//! - PHP key normalization and duplicate-key last-wins behavior must match runtime array semantics.

use super::*;

pub(super) fn static_array_flip_items(
    expr: &Expr,
    items: &[Expr],
    module: &WasmModule,
) -> Result<Vec<(Expr, Expr)>, CompileError> {
    let mut out: Vec<(AssocKeyValue, Expr)> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let Some(key) = static_array_flip_key(item, module) else {
            continue;
        };
        let value = Expr::new(ExprKind::IntLiteral(index as i64), expr.span);
        if let Some((_, existing_value)) = out.iter_mut().find(|(existing_key, _)| *existing_key == key) {
            *existing_value = value;
        } else {
            out.push((key, value));
        }
    }
    Ok(out
        .into_iter()
        .map(|(key, value)| {
            (assoc_key_value_expr(key, expr.span), value)
        })
        .collect())
}

pub(super) fn static_assoc_array_flip_items(
    expr: &Expr,
    items: &[(Expr, Expr)],
    module: &WasmModule,
) -> Result<Vec<(Expr, Expr)>, CompileError> {
    let mut out: Vec<(AssocKeyValue, Expr)> = Vec::new();
    for (key, value) in items {
        let Some(flipped_key) = static_array_flip_key(value, module) else {
            continue;
        };
        let source_key = static_assoc_key_value(key, module)?;
        if let Some((_, existing_value)) = out
            .iter_mut()
            .find(|(existing_key, _)| *existing_key == flipped_key)
        {
            *existing_value = static_assoc_key_expr(source_key.clone(), key.span);
        } else {
            out.push((flipped_key, static_assoc_key_expr(source_key, key.span)));
        }
    }
    Ok(out
        .into_iter()
        .map(|(key, value)| {
            (assoc_key_value_expr(key, expr.span), value)
        })
        .collect())
}

pub(super) fn static_array_flip_key(value: &Expr, module: &WasmModule) -> Option<AssocKeyValue> {
    if let Some(value) = static_int_value(value) {
        return Some(AssocKeyValue::Int(value));
    }
    static_string_value(value, module).map(|value| {
        literal_php_array_int_key(&value)
            .map(AssocKeyValue::Int)
            .unwrap_or(AssocKeyValue::Str(value))
    })
}

pub(super) fn assoc_key_value_expr(value: AssocKeyValue, span: Span) -> Expr {
    match value {
        AssocKeyValue::Int(value) => Expr::new(ExprKind::IntLiteral(value), span),
        AssocKeyValue::Str(value) => Expr::new(ExprKind::StringLiteral(value), span),
    }
}

pub(super) fn static_assoc_key_expr(value: StaticAssocKey, span: Span) -> Expr {
    match value {
        StaticAssocKey::Int(value) => Expr::new(ExprKind::IntLiteral(value), span),
        StaticAssocKey::Str(value) => Expr::new(ExprKind::StringLiteral(value), span),
    }
}

pub(super) fn static_assoc_array_unique_items(
    expr: &Expr,
    items: &[(Expr, Expr)],
    module: &WasmModule,
) -> Result<Vec<(Expr, Expr)>, CompileError> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for (key, value) in items {
        static_assoc_key_value(key, module)?;
        let Some(compare_value) = static_value_cell_compare_string(value, module) else {
            return Err(CompileError::new(
                value.span,
                "wasm32-web array_unique() on associative literals currently requires static scalar values",
            ));
        };
        if seen.contains(&compare_value) {
            continue;
        }
        seen.push(compare_value);
        out.push((key.clone(), value.clone()));
    }
    if out.len() > items.len() {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_unique() produced an invalid associative result",
        ));
    }
    Ok(out)
}

pub(super) fn static_array_unique_regular_items(
    expr: &Expr,
    items: &[Expr],
    module: &WasmModule,
) -> Result<Vec<(Expr, Expr)>, CompileError> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for (index, value) in items.iter().enumerate() {
        let scalar = static_scalar_value(value, module).ok_or_else(|| {
            CompileError::new(
                value.span,
                "wasm32-web array_unique() SORT_REGULAR on literals currently requires static scalar values",
            )
        })?;
        if seen
            .iter()
            .any(|existing| compare_static_scalars(existing, &BinOp::Eq, &scalar))
        {
            continue;
        }
        seen.push(scalar);
        out.push((
            Expr::new(ExprKind::IntLiteral(index as i64), expr.span),
            value.clone(),
        ));
    }
    Ok(out)
}

pub(super) fn static_array_unique_numeric_items(
    expr: &Expr,
    items: &[Expr],
    module: &WasmModule,
) -> Result<Vec<(Expr, Expr)>, CompileError> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for (index, value) in items.iter().enumerate() {
        let scalar = static_scalar_value(value, module).ok_or_else(|| {
            CompileError::new(
                value.span,
                "wasm32-web array_unique() SORT_NUMERIC on literals currently requires static scalar values",
            )
        })?;
        let numeric = array_unique_numeric_value(&scalar);
        if seen.iter().any(|existing| *existing == numeric) {
            continue;
        }
        seen.push(numeric);
        out.push((
            Expr::new(ExprKind::IntLiteral(index as i64), expr.span),
            value.clone(),
        ));
    }
    Ok(out)
}

pub(super) fn static_assoc_array_unique_regular_items(
    expr: &Expr,
    items: &[(Expr, Expr)],
    module: &WasmModule,
) -> Result<Vec<(Expr, Expr)>, CompileError> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for (key, value) in items {
        static_assoc_key_value(key, module)?;
        let scalar = static_scalar_value(value, module).ok_or_else(|| {
            CompileError::new(
                value.span,
                "wasm32-web array_unique() SORT_REGULAR on literals currently requires static scalar values",
            )
        })?;
        if seen
            .iter()
            .any(|existing| compare_static_scalars(existing, &BinOp::Eq, &scalar))
        {
            continue;
        }
        seen.push(scalar);
        out.push((key.clone(), value.clone()));
    }
    if out.len() > items.len() {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_unique() SORT_REGULAR produced an invalid associative result",
        ));
    }
    Ok(out)
}

pub(super) fn static_assoc_array_unique_numeric_items(
    expr: &Expr,
    items: &[(Expr, Expr)],
    module: &WasmModule,
) -> Result<Vec<(Expr, Expr)>, CompileError> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for (key, value) in items {
        static_assoc_key_value(key, module)?;
        let scalar = static_scalar_value(value, module).ok_or_else(|| {
            CompileError::new(
                value.span,
                "wasm32-web array_unique() SORT_NUMERIC on literals currently requires static scalar values",
            )
        })?;
        let numeric = array_unique_numeric_value(&scalar);
        if seen.iter().any(|existing| *existing == numeric) {
            continue;
        }
        seen.push(numeric);
        out.push((key.clone(), value.clone()));
    }
    if out.len() > items.len() {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_unique() SORT_NUMERIC produced an invalid associative result",
        ));
    }
    Ok(out)
}

fn array_unique_numeric_value(value: &ConstantValue) -> f64 {
    match value {
        ConstantValue::Int(value) => *value as f64,
        ConstantValue::Float(value) => *value,
        ConstantValue::Bool(value) => i32::from(*value) as f64,
        ConstantValue::Null => 0.0,
        ConstantValue::Str(value) => php_leading_numeric_string_value(value)
            .map(|(value, _)| value)
            .unwrap_or(0.0),
    }
}

pub(super) fn reversed_assoc_default_items(
    expr: &Expr,
    items: &[(Expr, Expr)],
    module: &WasmModule,
) -> Result<Vec<(Expr, Expr)>, CompileError> {
    let mut next_int_key = 0i64;
    let mut out = Vec::with_capacity(items.len());
    for (key, value) in items.iter().rev() {
        let reversed_key = if static_int_value(key).is_some() {
            let key = Expr::new(ExprKind::IntLiteral(next_int_key), key.span);
            next_int_key += 1;
            key
        } else if static_string_value(key, module).is_some() {
            key.clone()
        } else {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web array_reverse() on associative literals currently requires static integer or string keys",
            ));
        };
        out.push((reversed_key, value.clone()));
    }
    Ok(out)
}

pub(super) fn reversed_indexed_preserve_key_items(expr: &Expr, items: &[Expr]) -> Vec<(Expr, Expr)> {
    items
        .iter()
        .enumerate()
        .rev()
        .map(|(index, value)| {
            (
                Expr::new(ExprKind::IntLiteral(index as i64), expr.span),
                value.clone(),
            )
        })
        .collect()
}

pub(super) fn reversed_assoc_preserve_key_items(
    expr: &Expr,
    items: &[(Expr, Expr)],
) -> Result<Vec<(Expr, Expr)>, CompileError> {
    let mut out = Vec::with_capacity(items.len());
    for (key, value) in items.iter().rev() {
        if static_int_value(key).is_none() && !matches!(key.kind, ExprKind::StringLiteral(_)) {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web array_reverse() on associative literals currently requires static integer or string keys",
            ));
        }
        out.push((key.clone(), value.clone()));
    }
    Ok(out)
}
