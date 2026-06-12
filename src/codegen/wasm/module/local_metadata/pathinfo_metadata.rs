//! Purpose:
//! Infers wasm local metadata for `pathinfo()` array-return shapes.
//! Keeps pathinfo flag and key handling separate from generic local inference.
//!
//! Called from:
//! - `super::infer_local_kind()` and assignment fallback inference.
//!
//! Key details:
//! - Only `PATHINFO_ALL` or omitted flags produce known associative array metadata.
//! - Known pathinfo string keys are treated as string value cells for direct accesses.

use super::*;

pub(super) fn pathinfo_call_returns_array(args: &[Expr]) -> bool {
    args.len() == 1 || args.get(1).and_then(pathinfo_static_flag_value) == Some(15)
}

pub(super) fn pathinfo_direct_array_access_value_kind(
    array: &Expr,
    index: &Expr,
) -> Option<ValueCellKind> {
    let ExprKind::FunctionCall { name, args } = &array.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("pathinfo") || !pathinfo_call_returns_array(args) {
        return None;
    }
    match static_assoc_key_value_for_expr(index)? {
        AssocKeyValue::Str(key)
            if matches!(
                key.as_str(),
                "dirname" | "basename" | "extension" | "filename"
            ) =>
        {
            Some(ValueCellKind::Str)
        }
        _ => None,
    }
}

fn pathinfo_static_flag_value(flag: &Expr) -> Option<i64> {
    match &flag.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::ConstRef(name) => match name.as_str() {
            "PATHINFO_DIRNAME" => Some(1),
            "PATHINFO_BASENAME" => Some(2),
            "PATHINFO_EXTENSION" => Some(4),
            "PATHINFO_FILENAME" => Some(8),
            "PATHINFO_ALL" => Some(15),
            _ => None,
        },
        ExprKind::Negate(inner) => pathinfo_static_flag_value(inner).map(|value| -value),
        ExprKind::BinaryOp { left, op, right } => {
            let left = pathinfo_static_flag_value(left)?;
            let right = pathinfo_static_flag_value(right)?;
            match op {
                BinOp::BitAnd => Some(left & right),
                BinOp::BitOr => Some(left | right),
                BinOp::BitXor => Some(left ^ right),
                _ => None,
            }
        }
        _ => None,
    }
}
