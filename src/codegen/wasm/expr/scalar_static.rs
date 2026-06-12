//! Purpose:
//! Provides wasm32-web static scalar evaluation and PHP scalar comparison helpers.
//! Keeps constant scalar extraction, numeric-string parsing, and static compare rules out of expr.rs.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` scalar and array lowering helpers.
//!
//! Key details:
//! - Helpers mirror PHP loose/strict scalar comparison and numeric string coercion rules.

use super::*;

pub(in crate::codegen::wasm) fn static_or_tracked_scalar_value(expr: &Expr, module: &WasmModule) -> Option<ConstantValue> {
    match &expr.kind {
        ExprKind::Variable(name) => module.string_static_value(name).map(ConstantValue::Str),
        ExprKind::FunctionCall { name, args } if args.is_empty() => {
            module.function_static_string_return(name).map(ConstantValue::Str)
        }
        _ => static_scalar_value(expr, module),
    }
}

pub(in crate::codegen::wasm) fn static_scalar_value(expr: &Expr, module: &WasmModule) -> Option<ConstantValue> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(ConstantValue::Int(*value)),
        ExprKind::FloatLiteral(value) => Some(ConstantValue::Float(*value)),
        ExprKind::BoolLiteral(value) => Some(ConstantValue::Bool(*value)),
        ExprKind::StringLiteral(value) => Some(ConstantValue::Str(value.clone())),
        ExprKind::Null => Some(ConstantValue::Null),
        ExprKind::Negate(inner) => match static_scalar_value(inner, module)? {
            ConstantValue::Int(value) => Some(ConstantValue::Int(value.checked_neg()?)),
            ConstantValue::Float(value) => Some(ConstantValue::Float(-value)),
            _ => None,
        },
        ExprKind::ConstRef(name) => module.constant_value(name),
        ExprKind::ClassConstant { receiver } => module
            .class_name_for_receiver(receiver)
            .map(ConstantValue::Str),
        ExprKind::ScopedConstantAccess { receiver, name } => {
            module.class_constant_value(receiver, name)
        }
        _ => None,
    }
}

pub(in crate::codegen::wasm) fn compare_static_scalars(left: &ConstantValue, op: &BinOp, right: &ConstantValue) -> bool {
    match op {
        BinOp::StrictEq => static_scalar_same_type(left, right) && compare_same_type(left, op, right),
        BinOp::StrictNotEq => {
            !static_scalar_same_type(left, right) || compare_same_type(left, op, right)
        }
        BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq => {
            compare_loose_static_scalars(left, op, right)
        }
        _ => unreachable!(),
    }
}

pub(in crate::codegen::wasm) fn static_scalar_same_type(left: &ConstantValue, right: &ConstantValue) -> bool {
    matches!(
        (left, right),
        (ConstantValue::Int(_), ConstantValue::Int(_))
            | (ConstantValue::Float(_), ConstantValue::Float(_))
            | (ConstantValue::Bool(_), ConstantValue::Bool(_))
            | (ConstantValue::Str(_), ConstantValue::Str(_))
            | (ConstantValue::Null, ConstantValue::Null)
    )
}

pub(in crate::codegen::wasm) fn compare_same_type(left: &ConstantValue, op: &BinOp, right: &ConstantValue) -> bool {
    match (left, right) {
        (ConstantValue::Int(left), ConstantValue::Int(right)) => compare_i64(*left, op, *right),
        (ConstantValue::Float(left), ConstantValue::Float(right)) => compare_f64(*left, op, *right),
        (ConstantValue::Bool(left), ConstantValue::Bool(right)) => {
            compare_i64(i64::from(*left), op, i64::from(*right))
        }
        (ConstantValue::Str(left), ConstantValue::Str(right)) => {
            compare_ordering(left.as_bytes().cmp(right.as_bytes()), op)
        }
        (ConstantValue::Null, ConstantValue::Null) => compare_i64(0, op, 0),
        _ => false,
    }
}

pub(in crate::codegen::wasm) fn compare_loose_static_scalars(left: &ConstantValue, op: &BinOp, right: &ConstantValue) -> bool {
    if matches!(left, ConstantValue::Bool(_)) || matches!(right, ConstantValue::Bool(_)) {
        return compare_i64(
            i64::from(static_scalar_truthiness(left)),
            op,
            i64::from(static_scalar_truthiness(right)),
        );
    }
    if matches!(left, ConstantValue::Null) || matches!(right, ConstantValue::Null) {
        if matches!(op, BinOp::Eq | BinOp::NotEq) {
            let equal = static_null_loose_equal(left, right);
            return if matches!(op, BinOp::Eq) { equal } else { !equal };
        }
        return compare_i64(
            i64::from(static_scalar_truthiness(left)),
            op,
            i64::from(static_scalar_truthiness(right)),
        );
    }

    match (left, right) {
        (ConstantValue::Str(left), ConstantValue::Str(right)) => {
            match (php_numeric_string_value(left), php_numeric_string_value(right)) {
                (Some(left), Some(right)) => compare_f64(left, op, right),
                _ => compare_ordering(left.as_bytes().cmp(right.as_bytes()), op),
            }
        }
        (ConstantValue::Str(left), right) => compare_string_and_non_bool_scalar(left, op, right, false),
        (left, ConstantValue::Str(right)) => compare_string_and_non_bool_scalar(right, op, left, true),
        _ => compare_numeric_static_scalars(left, op, right),
    }
}

pub(in crate::codegen::wasm) fn static_null_loose_equal(left: &ConstantValue, right: &ConstantValue) -> bool {
    match (left, right) {
        (ConstantValue::Null, ConstantValue::Null) => true,
        (ConstantValue::Null, value) | (value, ConstantValue::Null) => match value {
            ConstantValue::Int(value) => *value == 0,
            ConstantValue::Float(value) => *value == 0.0,
            ConstantValue::Bool(value) => !*value,
            ConstantValue::Str(value) => value.is_empty(),
            ConstantValue::Null => true,
        },
        _ => false,
    }
}

pub(in crate::codegen::wasm) fn compare_string_and_non_bool_scalar(
    string: &str,
    op: &BinOp,
    scalar: &ConstantValue,
    scalar_is_left: bool,
) -> bool {
    if let Some(string_number) = php_numeric_string_value(string) {
        let scalar_number = static_scalar_numeric_as_float(scalar).unwrap_or(0.0);
        return if scalar_is_left {
            compare_f64(scalar_number, op, string_number)
        } else {
            compare_f64(string_number, op, scalar_number)
        };
    }

    let scalar_string = static_scalar_string_for_compare(scalar);
    let ordering = if scalar_is_left {
        scalar_string.as_bytes().cmp(string.as_bytes())
    } else {
        string.as_bytes().cmp(scalar_string.as_bytes())
    };
    compare_ordering(ordering, op)
}

pub(in crate::codegen::wasm) fn compare_numeric_static_scalars(left: &ConstantValue, op: &BinOp, right: &ConstantValue) -> bool {
    match (left, right) {
        (ConstantValue::Int(left), ConstantValue::Int(right)) => compare_i64(*left, op, *right),
        _ => compare_f64(
            static_scalar_numeric_as_float(left).unwrap_or(0.0),
            op,
            static_scalar_numeric_as_float(right).unwrap_or(0.0),
        ),
    }
}

pub(in crate::codegen::wasm) fn static_scalar_truthiness(value: &ConstantValue) -> bool {
    match value {
        ConstantValue::Int(value) => *value != 0,
        ConstantValue::Float(value) => *value != 0.0,
        ConstantValue::Bool(value) => *value,
        ConstantValue::Str(value) => string_is_truthy(value),
        ConstantValue::Null => false,
    }
}

pub(in crate::codegen::wasm) fn static_scalar_numeric_as_float(value: &ConstantValue) -> Option<f64> {
    match value {
        ConstantValue::Int(value) => Some(*value as f64),
        ConstantValue::Float(value) => Some(*value),
        _ => None,
    }
}

pub(in crate::codegen::wasm) fn static_scalar_string_for_compare(value: &ConstantValue) -> String {
    match value {
        ConstantValue::Int(value) => value.to_string(),
        ConstantValue::Float(value) => value.to_string(),
        ConstantValue::Bool(true) => "1".to_string(),
        ConstantValue::Bool(false) | ConstantValue::Null => String::new(),
        ConstantValue::Str(value) => value.clone(),
    }
}

pub(in crate::codegen::wasm) fn compare_i64(left: i64, op: &BinOp, right: i64) -> bool {
    match op {
        BinOp::Eq | BinOp::StrictEq => left == right,
        BinOp::NotEq | BinOp::StrictNotEq => left != right,
        BinOp::Lt => left < right,
        BinOp::Gt => left > right,
        BinOp::LtEq => left <= right,
        BinOp::GtEq => left >= right,
        _ => unreachable!(),
    }
}

pub(in crate::codegen::wasm) fn compare_f64(left: f64, op: &BinOp, right: f64) -> bool {
    match op {
        BinOp::Eq | BinOp::StrictEq => left == right,
        BinOp::NotEq | BinOp::StrictNotEq => left != right,
        BinOp::Lt => left < right,
        BinOp::Gt => left > right,
        BinOp::LtEq => left <= right,
        BinOp::GtEq => left >= right,
        _ => unreachable!(),
    }
}

pub(in crate::codegen::wasm) fn compare_ordering(ordering: std::cmp::Ordering, op: &BinOp) -> bool {
    match op {
        BinOp::Eq | BinOp::StrictEq => ordering.is_eq(),
        BinOp::NotEq | BinOp::StrictNotEq => !ordering.is_eq(),
        BinOp::Lt => ordering.is_lt(),
        BinOp::Gt => ordering.is_gt(),
        BinOp::LtEq => !ordering.is_gt(),
        BinOp::GtEq => !ordering.is_lt(),
        _ => unreachable!(),
    }
}

pub(in crate::codegen::wasm) fn php_numeric_string_value(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<f64>().ok()
}

pub(in crate::codegen::wasm) fn php_leading_numeric_string_value(value: &str) -> Option<(f64, bool)> {
    let trimmed = value.trim_start();
    let bytes = trimmed.as_bytes();
    let mut idx = 0;
    if idx < bytes.len() && matches!(bytes[idx], b'+' | b'-') {
        idx += 1;
    }
    let mut digits = false;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        digits = true;
        idx += 1;
    }
    let mut has_float_marker = false;
    if idx < bytes.len() && bytes[idx] == b'.' {
        has_float_marker = true;
        idx += 1;
        while idx < bytes.len() && bytes[idx].is_ascii_digit() {
            digits = true;
            idx += 1;
        }
    }
    if digits && idx < bytes.len() && matches!(bytes[idx], b'e' | b'E') {
        let exponent = idx;
        idx += 1;
        if idx < bytes.len() && matches!(bytes[idx], b'+' | b'-') {
            idx += 1;
        }
        let exponent_digits = idx;
        while idx < bytes.len() && bytes[idx].is_ascii_digit() {
            idx += 1;
        }
        if exponent_digits == idx {
            idx = exponent;
        } else {
            has_float_marker = true;
        }
    }
    if !digits {
        return None;
    }
    Some((trimmed[..idx].parse::<f64>().ok()?, has_float_marker))
}

pub(in crate::codegen::wasm) fn parse_php_string_cast_int(value: &str) -> i64 {
    parse_php_string_cast_float(value).trunc() as i64
}

pub(in crate::codegen::wasm) fn parse_php_string_cast_float(value: &str) -> f64 {
    php_leading_numeric_string_value(value)
        .map(|(number, _)| number)
        .unwrap_or(0.0)
}
