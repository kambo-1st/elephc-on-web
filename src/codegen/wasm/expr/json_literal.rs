//! Purpose:
//! Evaluates literal json_encode() calls for the wasm32-web backend.
//! Keeps compile-time JSON array/scalar encoding and quote escaping separate from runtime emitters.
//!
//! Called from:
//! - `super::scalar_builtins` for literal json_encode() lowering.
//! - `super::json_encode` for shared JSON string/key quoting.
//!
//! Key details:
//! - Preserves PHP JSON flags for hex escaping, numeric checks, pretty printing, and Unicode escaping.

use super::*;

pub(super) fn eval_literal_json_encode(call: &Expr, args: &[Expr]) -> Result<String, CompileError> {
    if args.is_empty() || args.len() > 3 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web json_encode() expects one to three literal arguments",
        ));
    }
    let flags = args.get(1).map(const_or_literal_int_arg).transpose()?.unwrap_or(0);
    Ok(match &args[0].kind {
        ExprKind::Null => "null".to_string(),
        ExprKind::BoolLiteral(value) => value.to_string(),
        ExprKind::IntLiteral(value) => value.to_string(),
        ExprKind::FloatLiteral(value) => {
            if flags & 1024 != 0 && value.fract() == 0.0 {
                format!("{:.1}", value)
            } else {
                value.to_string()
            }
        }
        ExprKind::StringLiteral(value) => json_encode_string_literal(value, flags),
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => {
            literal_json_encode_expr(&args[0], flags, 0)?
        }
        _ => {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web json_encode() currently supports literal scalar and array inputs",
            ));
        }
    })
}

fn literal_json_encode_expr(expr: &Expr, flags: i64, depth: usize) -> Result<String, CompileError> {
    match &expr.kind {
        ExprKind::Null => Ok("null".to_string()),
        ExprKind::BoolLiteral(value) => Ok(value.to_string()),
        ExprKind::IntLiteral(value) => Ok(value.to_string()),
        ExprKind::FloatLiteral(value) => {
            if flags & 1024 != 0 && value.fract() == 0.0 {
                Ok(format!("{:.1}", value))
            } else {
                Ok(value.to_string())
            }
        }
        ExprKind::StringLiteral(value) => Ok(json_encode_string_literal(value, flags)),
        ExprKind::ArrayLiteral(items) => literal_json_encode_list(items, flags, depth),
        ExprKind::ArrayLiteralAssoc(items) => literal_json_encode_assoc(items, flags, depth),
        _ => Err(CompileError::new(
            expr.span,
            "wasm32-web json_encode() array support requires literal scalar or nested array values",
        )),
    }
}

fn literal_json_encode_list(items: &[Expr], flags: i64, depth: usize) -> Result<String, CompileError> {
    let force_object = flags & 16 != 0;
    let pretty = flags & 128 != 0;
    if pretty {
        return literal_json_encode_pretty_list(items, flags, depth, force_object);
    }
    let mut out = if force_object {
        String::from("{")
    } else {
        String::from("[")
    };
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        if force_object {
            out.push_str(&json_quote(&index.to_string(), flags));
            out.push(':');
        }
        out.push_str(&literal_json_encode_expr(item, flags, depth + 1)?);
    }
    out.push(if force_object { '}' } else { ']' });
    Ok(out)
}

fn literal_json_encode_assoc(
    items: &[(Expr, Expr)],
    flags: i64,
    depth: usize,
) -> Result<String, CompileError> {
    let items = normalized_literal_json_assoc_items(items)?;
    if flags & 16 == 0 && literal_json_assoc_is_list(&items) {
        let values = items
            .iter()
            .map(|(_, value)| (*value).clone())
            .collect::<Vec<_>>();
        return literal_json_encode_list(&values, flags, depth);
    }
    if flags & 128 != 0 {
        return literal_json_encode_pretty_assoc(&items, flags, depth);
    }
    let mut out = String::from("{");
    for (index, (key, value)) in items.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&json_quote(&key.json_name(), flags));
        out.push(':');
        out.push_str(&literal_json_encode_expr(value, flags, depth + 1)?);
    }
    out.push('}');
    Ok(out)
}

fn literal_json_encode_pretty_list(
    items: &[Expr],
    flags: i64,
    depth: usize,
    force_object: bool,
) -> Result<String, CompileError> {
    if items.is_empty() {
        return Ok(if force_object {
            "{}".to_string()
        } else {
            "[]".to_string()
        });
    }
    let mut out = if force_object {
        String::from("{\n")
    } else {
        String::from("[\n")
    };
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            out.push_str(",\n");
        }
        out.push_str(&json_indent(depth + 1));
        if force_object {
            out.push_str(&json_quote(&index.to_string(), flags));
            out.push_str(": ");
        }
        out.push_str(&literal_json_encode_expr(item, flags, depth + 1)?);
    }
    out.push('\n');
    out.push_str(&json_indent(depth));
    out.push(if force_object { '}' } else { ']' });
    Ok(out)
}

fn literal_json_encode_pretty_assoc(
    items: &[(LiteralJsonKey, &Expr)],
    flags: i64,
    depth: usize,
) -> Result<String, CompileError> {
    if items.is_empty() {
        return Ok("{}".to_string());
    }
    let mut out = String::from("{\n");
    for (index, (key, value)) in items.iter().enumerate() {
        if index > 0 {
            out.push_str(",\n");
        }
        out.push_str(&json_indent(depth + 1));
        out.push_str(&json_quote(&key.json_name(), flags));
        out.push_str(": ");
        out.push_str(&literal_json_encode_expr(value, flags, depth + 1)?);
    }
    out.push('\n');
    out.push_str(&json_indent(depth));
    out.push('}');
    Ok(out)
}

fn json_indent(depth: usize) -> String {
    "    ".repeat(depth)
}

fn literal_json_assoc_is_list(items: &[(LiteralJsonKey, &Expr)]) -> bool {
    items
        .iter()
        .enumerate()
        .all(|(index, (key, _))| *key == LiteralJsonKey::Int(index as i64))
}

fn normalized_literal_json_assoc_items(
    items: &[(Expr, Expr)],
) -> Result<Vec<(LiteralJsonKey, &Expr)>, CompileError> {
    let mut normalized: Vec<(LiteralJsonKey, &Expr)> = Vec::new();
    for (key, value) in items {
        let key = literal_json_key(key)?;
        if let Some((_, existing_value)) = normalized
            .iter_mut()
            .find(|(existing_key, _)| *existing_key == key)
        {
            *existing_value = value;
        } else {
            normalized.push((key, value));
        }
    }
    Ok(normalized)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LiteralJsonKey {
    Int(i64),
    Str(String),
}

impl LiteralJsonKey {
    fn json_name(&self) -> String {
        match self {
            LiteralJsonKey::Int(value) => value.to_string(),
            LiteralJsonKey::Str(value) => value.clone(),
        }
    }
}

fn literal_json_key(key: &Expr) -> Result<LiteralJsonKey, CompileError> {
    if let Some(value) = static_int_value(key) {
        return Ok(LiteralJsonKey::Int(value));
    }
    if let ExprKind::StringLiteral(value) = &key.kind {
        return Ok(literal_json_string_key(value));
    }
    Err(CompileError::new(
        key.span,
        "wasm32-web json_encode() array object keys must be literal strings or integers",
    ))
}

fn literal_json_string_key(value: &str) -> LiteralJsonKey {
    if let Some(value) = literal_php_array_int_key(value) {
        LiteralJsonKey::Int(value)
    } else {
        LiteralJsonKey::Str(value.to_string())
    }
}

fn json_encode_string_literal(value: &str, flags: i64) -> String {
    if flags & 32 != 0 {
        if let Some(value) = json_numeric_check_literal(value, flags) {
            return value;
        }
    }
    json_quote(value, flags)
}

fn json_numeric_check_literal(value: &str, flags: i64) -> Option<String> {
    let trimmed = value.trim();
    if !json_numeric_literal_shape(trimmed) {
        return None;
    }
    let parsed = trimmed.parse::<f64>().ok()?;
    if !parsed.is_finite() {
        return None;
    }
    let preserve_zero_fraction = flags & 1024 != 0;
    let has_float_syntax = trimmed.contains('.') || trimmed.contains('e') || trimmed.contains('E');
    if !has_float_syntax {
        return trimmed.parse::<i64>().map(|value| value.to_string()).ok();
    }
    if parsed == 0.0 && trimmed.starts_with('-') {
        return Some(if preserve_zero_fraction {
            "-0.0".to_string()
        } else {
            "-0".to_string()
        });
    }
    let mut out = if parsed.fract() == 0.0 {
        (parsed as i64).to_string()
    } else {
        parsed.to_string()
    };
    if preserve_zero_fraction && !out.contains('.') && !out.contains('e') && !out.contains('E') {
        out.push_str(".0");
    }
    Some(out)
}

fn json_numeric_literal_shape(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    if matches!(bytes.get(index), Some(b'+') | Some(b'-')) {
        index += 1;
    }
    let mut saw_digit = false;
    while matches!(bytes.get(index), Some(byte) if byte.is_ascii_digit()) {
        saw_digit = true;
        index += 1;
    }
    if matches!(bytes.get(index), Some(b'.')) {
        index += 1;
        while matches!(bytes.get(index), Some(byte) if byte.is_ascii_digit()) {
            saw_digit = true;
            index += 1;
        }
    }
    if !saw_digit {
        return false;
    }
    if matches!(bytes.get(index), Some(b'e') | Some(b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+') | Some(b'-')) {
            index += 1;
        }
        let exponent_start = index;
        while matches!(bytes.get(index), Some(byte) if byte.is_ascii_digit()) {
            index += 1;
        }
        if index == exponent_start {
            return false;
        }
    }
    index == bytes.len()
}

pub(super) fn json_quote(value: &str, flags: i64) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' if flags & 8 != 0 => out.push_str("\\u0022"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '/' if flags & 64 == 0 => out.push_str("\\/"),
            '\'' if flags & 4 != 0 => out.push_str("\\u0027"),
            '<' | '>' if flags & 1 != 0 => out.push_str(&format!("\\u{:04X}", ch as u32)),
            '&' if flags & 2 != 0 => out.push_str("\\u0026"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000c}' => out.push_str("\\f"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            '\u{2028}' | '\u{2029}' if flags & 2048 == 0 => {
                out.push_str(&format!("\\u{:04x}", ch as u32));
            }
            ch if (ch as u32) > 0x7f && flags & 256 == 0 => {
                push_json_unicode_escape(&mut out, ch);
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn push_json_unicode_escape(out: &mut String, ch: char) {
    let value = ch as u32;
    if value <= 0xffff {
        out.push_str(&format!("\\u{value:04x}"));
        return;
    }
    let scalar = value - 0x1_0000;
    let high = 0xd800 + (scalar >> 10);
    let low = 0xdc00 + (scalar & 0x3ff);
    out.push_str(&format!("\\u{high:04x}\\u{low:04x}"));
}
