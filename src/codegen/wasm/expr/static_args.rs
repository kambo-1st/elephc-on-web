//! Purpose:
//! Provides static and literal argument helpers for wasm32-web expression lowering.
//! Keeps PHP constant, scalar literal, and formatting argument normalization out of the dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr`
//! - sibling wasm expression modules through `super::*`
//!
//! Key details:
//! - Converts only PHP-compatible static values and preserves CompileError diagnostics for unsupported forms.
//! - Centralizes wasm-local PHP constants used by string, JSON, path, and array helpers.

use super::*;
use crate::names::Name;

pub(super) fn normalize_static_string_args(args: &[Expr], module: &WasmModule) -> Vec<Expr> {
    args.iter()
        .map(|arg| normalize_static_string_expr(arg, module))
        .collect()
}

pub(super) fn normalize_static_string_expr(expr: &Expr, module: &WasmModule) -> Expr {
    if let Some(value) = static_string_value(expr, module) {
        return Expr::new(ExprKind::StringLiteral(value), expr.span);
    }
    if let Some(value) = static_scalar_value(expr, module) {
        return Expr::new(expr_kind_for_static_scalar(value), expr.span);
    }
    match &expr.kind {
        ExprKind::FunctionCall { name, args } => Expr::new(
            ExprKind::FunctionCall {
                name: name.clone(),
                args: normalize_static_string_args(args, module),
            },
            expr.span,
        ),
        ExprKind::ArrayLiteral(items) => Expr::new(
            ExprKind::ArrayLiteral(normalize_static_string_args(items, module)),
            expr.span,
        ),
        ExprKind::ArrayLiteralAssoc(items) => Expr::new(
            ExprKind::ArrayLiteralAssoc(
                items
                    .iter()
                    .map(|(key, value)| {
                        (
                            normalize_static_string_expr(key, module),
                            normalize_static_string_expr(value, module),
                        )
                    })
                    .collect(),
            ),
            expr.span,
        ),
        _ => expr.clone(),
    }
}

pub(super) fn expr_kind_for_static_scalar(value: ConstantValue) -> ExprKind {
    match value {
        ConstantValue::Int(value) => ExprKind::IntLiteral(value),
        ConstantValue::Float(value) => ExprKind::FloatLiteral(value),
        ConstantValue::Bool(value) => ExprKind::BoolLiteral(value),
        ConstantValue::Str(value) => ExprKind::StringLiteral(value),
        ConstantValue::Null => ExprKind::Null,
    }
}

pub(super) fn literal_string_arg(expr: &Expr) -> Result<&str, CompileError> {
    match &expr.kind {
        ExprKind::StringLiteral(value) if value.is_ascii() => Ok(value),
        ExprKind::StringLiteral(_) => Err(CompileError::new(
            expr.span,
            "wasm32-web output string builtin currently requires ASCII string literals",
        )),
        _ => Err(CompileError::new(
            expr.span,
            "wasm32-web output string builtin currently requires string literal arguments",
        )),
    }
}

pub(super) fn static_ascii_string_arg(
    call: &Expr,
    expr: &Expr,
    module: &WasmModule,
) -> Result<String, CompileError> {
    if let Some(err) = unsupported_string_coercion_expr(expr) {
        return Err(err);
    }
    if let Some(value) = static_scalar_cast_string(expr, module) {
        if value.is_ascii() {
            return Ok(value);
        }
        return Err(CompileError::new(
            expr.span,
            "wasm32-web output string builtin currently requires ASCII string values",
        ));
    }
    if let Some(value) = static_string_value(expr, module) {
        if value.is_ascii() {
            return Ok(value);
        }
        return Err(CompileError::new(
            expr.span,
            "wasm32-web output string builtin currently requires ASCII string values",
        ));
    }
    match &expr.kind {
        ExprKind::StringLiteral(value) if value.is_ascii() => Ok(value.clone()),
        ExprKind::StringLiteral(_) => Err(CompileError::new(
            expr.span,
            "wasm32-web output string builtin currently requires ASCII string literals",
        )),
        _ => Err(CompileError::new(
            call.span,
            "wasm32-web output string builtin currently requires static string arguments",
        )),
    }
}

pub(super) fn literal_int_arg(expr: &Expr) -> Result<i64, CompileError> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Ok(*value),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => Ok(-*value),
            _ => Err(CompileError::new(
                expr.span,
                "wasm32-web output string builtin currently requires literal integer arguments",
            )),
        },
        _ => Err(CompileError::new(
            expr.span,
            "wasm32-web output string builtin currently requires literal integer arguments",
        )),
    }
}

pub(super) fn static_int_value(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => Some(-*value),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn static_or_const_int_value(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::ConstRef(name) => const_int_value(name),
        ExprKind::BinaryOp { left, op, right } => {
            let left = static_or_const_int_value(left)?;
            let right = static_or_const_int_value(right)?;
            match op {
                BinOp::BitAnd => Some(left & right),
                BinOp::BitOr => Some(left | right),
                BinOp::BitXor => Some(left ^ right),
                _ => None,
            }
        }
        _ => static_int_value(expr),
    }
}

pub(super) fn static_or_const_or_i64_local_value(expr: &Expr, module: &WasmModule) -> Option<i64> {
    match &expr.kind {
        ExprKind::Variable(name) => module.i64_static_value(name),
        _ => static_or_module_const_int_value(expr, module),
    }
}

pub(super) fn static_or_module_const_int_value(expr: &Expr, module: &WasmModule) -> Option<i64> {
    match &expr.kind {
        ExprKind::ConstRef(name) => match module.constant_value(name) {
            Some(ConstantValue::Int(value)) => Some(value),
            _ => const_int_value(name),
        },
        ExprKind::BinaryOp { left, op, right } => {
            let left = static_or_module_const_int_value(left, module)?;
            let right = static_or_module_const_int_value(right, module)?;
            match op {
                BinOp::BitAnd => Some(left & right),
                BinOp::BitOr => Some(left | right),
                BinOp::BitXor => Some(left ^ right),
                _ => None,
            }
        }
        _ => static_int_value(expr),
    }
}

pub(super) fn static_or_const_or_f64_local_value(expr: &Expr, module: &WasmModule) -> Option<f64> {
    match &expr.kind {
        ExprKind::Variable(name) => module.f64_static_value(name),
        ExprKind::FloatLiteral(value) => Some(*value),
        ExprKind::Negate(inner) => static_or_const_or_f64_local_value(inner, module).map(|value| -value),
        ExprKind::ConstRef(name) => match module.constant_value(name) {
            Some(ConstantValue::Float(value)) => Some(value),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn static_or_const_or_i32_bool_value(expr: &Expr, module: &WasmModule) -> Option<bool> {
    match &expr.kind {
        ExprKind::Variable(name) => module.bool_static_value(name),
        _ => literal_bool_arg(expr).ok(),
    }
}

pub(super) fn static_bool_value_cell_needle(expr: &Expr, module: &WasmModule) -> Option<bool> {
    match &expr.kind {
        ExprKind::BoolLiteral(value) => Some(*value),
        ExprKind::Variable(name) => module.bool_static_value(name),
        _ => None,
    }
}

pub(super) fn literal_bool_arg(expr: &Expr) -> Result<bool, CompileError> {
    match &expr.kind {
        ExprKind::BoolLiteral(value) => Ok(*value),
        ExprKind::IntLiteral(value) => Ok(*value != 0),
        _ => Err(CompileError::new(
            expr.span,
            "wasm32-web output string builtin currently requires literal boolean arguments",
        )),
    }
}

pub(super) fn const_or_literal_int_arg(expr: &Expr) -> Result<i64, CompileError> {
    static_or_const_int_value(expr).ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web output string builtin currently requires literal integer arguments",
        )
    })
}

pub(super) fn const_int_value(name: &Name) -> Option<i64> {
    match name.as_str() {
        "PATHINFO_DIRNAME" => Some(1),
        "PATHINFO_BASENAME" => Some(2),
        "PATHINFO_EXTENSION" => Some(4),
        "PATHINFO_FILENAME" => Some(8),
        "PATHINFO_ALL" => Some(15),
        "ENT_NOQUOTES" => Some(0),
        "ENT_HTML_QUOTE_SINGLE" => Some(1),
        "ENT_HTML_QUOTE_DOUBLE" => Some(2),
        "ENT_COMPAT" => Some(2),
        "ENT_QUOTES" => Some(3),
        "ENT_HTML401" => Some(0),
        "ENT_XML1" => Some(16),
        "ENT_XHTML" => Some(32),
        "ENT_HTML5" => Some(48),
        "ENT_SUBSTITUTE" => Some(8),
        "STR_PAD_LEFT" => Some(0),
        "STR_PAD_RIGHT" => Some(1),
        "STR_PAD_BOTH" => Some(2),
        "SORT_REGULAR" => Some(0),
        "SORT_NUMERIC" => Some(1),
        "SORT_STRING" => Some(2),
        "ARRAY_FILTER_USE_BOTH" => Some(1),
        "ARRAY_FILTER_USE_KEY" => Some(2),
        "JSON_HEX_TAG" => Some(1),
        "JSON_HEX_AMP" => Some(2),
        "JSON_HEX_APOS" => Some(4),
        "JSON_HEX_QUOT" => Some(8),
        "JSON_FORCE_OBJECT" => Some(16),
        "JSON_NUMERIC_CHECK" => Some(32),
        "JSON_UNESCAPED_SLASHES" => Some(64),
        "JSON_PRETTY_PRINT" => Some(128),
        "JSON_UNESCAPED_UNICODE" => Some(256),
        "JSON_PARTIAL_OUTPUT_ON_ERROR" => Some(512),
        "JSON_PRESERVE_ZERO_FRACTION" => Some(1024),
        "JSON_UNESCAPED_LINE_TERMINATORS" => Some(2048),
        "JSON_INVALID_UTF8_IGNORE" => Some(1_048_576),
        "JSON_INVALID_UTF8_SUBSTITUTE" => Some(2_097_152),
        "JSON_THROW_ON_ERROR" => Some(4_194_304),
        "JSON_OBJECT_AS_ARRAY" => Some(1),
        "JSON_BIGINT_AS_STRING" => Some(2),
        "JSON_ERROR_NONE" => Some(0),
        "JSON_ERROR_DEPTH" => Some(1),
        "JSON_ERROR_STATE_MISMATCH" => Some(2),
        "JSON_ERROR_CTRL_CHAR" => Some(3),
        "JSON_ERROR_SYNTAX" => Some(4),
        "JSON_ERROR_UTF8" => Some(5),
        "JSON_ERROR_RECURSION" => Some(6),
        "JSON_ERROR_INF_OR_NAN" => Some(7),
        "JSON_ERROR_UNSUPPORTED_TYPE" => Some(8),
        "JSON_ERROR_INVALID_PROPERTY_NAME" => Some(9),
        "JSON_ERROR_UTF16" => Some(10),
        "JSON_ERROR_NON_BACKED_ENUM" => Some(11),
        _ => None,
    }
}

#[derive(Clone, Copy)]
pub(super) enum SpaceEncoding {
    Plus,
    Percent20,
}


pub(super) fn normalize_base64_input(value: &str, strict: bool) -> Option<Vec<u8>> {
    let mut clean = Vec::new();
    for byte in value.bytes() {
        if matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/' | b'=') {
            clean.push(byte);
        } else if byte.is_ascii_whitespace() {
            continue;
        } else if strict {
            return None;
        }
    }
    Some(clean)
}


pub(super) fn literal_format_string_arg(expr: &Expr) -> Result<String, CompileError> {
    match &expr.kind {
        ExprKind::StringLiteral(value) if value.is_ascii() => Ok(value.clone()),
        ExprKind::IntLiteral(value) => Ok(value.to_string()),
        ExprKind::FloatLiteral(value) => Ok(value.to_string()),
        ExprKind::BoolLiteral(true) => Ok("1".to_string()),
        ExprKind::BoolLiteral(false) | ExprKind::Null => Ok(String::new()),
        ExprKind::Negate(inner) => literal_format_string_arg(inner).map(|value| format!("-{}", value)),
        _ => Err(CompileError::new(
            expr.span,
            "wasm32-web sprintf() currently requires literal scalar arguments",
        )),
    }
}

pub(super) fn static_format_string_arg(
    call: &Expr,
    expr: &Expr,
    module: &WasmModule,
) -> Result<String, CompileError> {
    if let Some(err) = unsupported_string_coercion_expr(expr) {
        return Err(err);
    }
    if let Some(value) = static_or_tracked_string_value(expr, module) {
        if value.is_ascii() {
            return Ok(value);
        }
        return Err(CompileError::new(
            expr.span,
            "wasm32-web sprintf() currently requires ASCII string values",
        ));
    }
    literal_format_string_arg(expr).map_err(|_| {
        CompileError::new(
            call.span,
            "wasm32-web sprintf() currently requires static scalar arguments",
        )
    })
}

pub(super) fn static_or_tracked_ascii_string_arg(
    call: &Expr,
    expr: &Expr,
    module: &WasmModule,
) -> Result<String, CompileError> {
    if let Some(err) = unsupported_string_coercion_expr(expr) {
        return Err(err);
    }
    let value = static_or_tracked_string_value(expr, module)
        .ok_or_else(|| {
            CompileError::new(
                call.span,
                "wasm32-web output string builtin currently requires static string arguments",
            )
        })?;
    if value.is_ascii() {
        Ok(value)
    } else {
        Err(CompileError::new(
            expr.span,
            "wasm32-web output string builtin currently requires ASCII string values",
        ))
    }
}

pub(super) fn literal_format_int_arg(expr: &Expr) -> Result<i64, CompileError> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Ok(*value),
        ExprKind::FloatLiteral(value) => Ok(*value as i64),
        ExprKind::BoolLiteral(value) => Ok(i64::from(*value)),
        ExprKind::Null => Ok(0),
        ExprKind::Negate(inner) => literal_format_int_arg(inner).map(|value| -value),
        _ => Err(CompileError::new(
            expr.span,
            "wasm32-web sprintf() currently requires literal scalar arguments",
        )),
    }
}

pub(super) fn literal_unsigned_int_arg(expr: &Expr) -> Result<u64, CompileError> {
    Ok(literal_format_int_arg(expr)? as u64)
}

pub(super) fn literal_php_array_int_key(value: &str) -> Option<i64> {
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

