//! Purpose:
//! Evaluates literal/static PHP string transforms for wasm32-web.
//! Keeps pure string semantics separate from expression dispatch and runtime emission.
//!
//! Called from:
//! - `crate::codegen::wasm::expr`
//!
//! Key details:
//! - Helpers preserve byte-oriented PHP string behavior for the supported ASCII/static subset.

use super::*;
use super::sscanf::eval_literal_sscanf;

pub(super) fn ascii_lower(value: &str) -> String {
    let bytes = value
        .as_bytes()
        .iter()
        .map(|byte| byte.to_ascii_lowercase())
        .collect::<Vec<_>>();
    String::from_utf8(bytes).expect("ASCII-only byte transforms preserve UTF-8 validity")
}

pub(super) fn ascii_upper(value: &str) -> String {
    let bytes = value
        .as_bytes()
        .iter()
        .map(|byte| byte.to_ascii_uppercase())
        .collect::<Vec<_>>();
    String::from_utf8(bytes).expect("ASCII-only byte transforms preserve UTF-8 validity")
}

pub(super) fn ascii_lcfirst(value: &str) -> String {
    let mut bytes = value.as_bytes().to_vec();
    if let Some(first) = bytes.first_mut() {
        *first = first.to_ascii_lowercase();
    }
    String::from_utf8(bytes).expect("ASCII-only byte transforms preserve UTF-8 validity")
}

pub(super) fn ascii_ucfirst(value: &str) -> String {
    let mut bytes = value.as_bytes().to_vec();
    if let Some(first) = bytes.first_mut() {
        *first = first.to_ascii_uppercase();
    }
    String::from_utf8(bytes).expect("ASCII-only byte transforms preserve UTF-8 validity")
}

pub(super) fn ascii_ucwords(value: &str) -> String {
    let mut bytes = value.as_bytes().to_vec();
    let mut at_word_start = true;
    for byte in &mut bytes {
        if byte.is_ascii_whitespace() {
            at_word_start = true;
        } else if at_word_start {
            *byte = byte.to_ascii_uppercase();
            at_word_start = false;
        }
    }
    String::from_utf8(bytes).expect("ASCII-only byte transforms preserve UTF-8 validity")
}

pub(super) fn addslashes(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if matches!(byte, b'\'' | b'"' | b'\\' | 0) {
            out.push('\\');
        }
        out.push(char::from(byte));
    }
    out
}

pub(super) fn stripslashes(value: &str) -> String {
    let mut out = String::new();
    let mut escaping = false;
    for byte in value.bytes() {
        if escaping {
            out.push(char::from(byte));
            escaping = false;
        } else if byte == b'\\' {
            escaping = true;
        } else {
            out.push(char::from(byte));
        }
    }
    if escaping {
        out.push('\\');
    }
    out
}

pub(super) fn bin2hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(HEX[(byte >> 4) as usize]));
        out.push(char::from(HEX[(byte & 0x0f) as usize]));
    }
    out
}

pub(super) fn hex2bin(call: &Expr, value: &str) -> Result<String, CompileError> {
    if value.len() % 2 != 0 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web hex2bin() literal input must have an even length",
        ));
    }
    let mut out = String::new();
    for pair in value.as_bytes().chunks_exact(2) {
        let high = hex_nibble(pair[0]).ok_or_else(|| {
            CompileError::new(call.span, "wasm32-web hex2bin() literal input must be hex")
        })?;
        let low = hex_nibble(pair[1]).ok_or_else(|| {
            CompileError::new(call.span, "wasm32-web hex2bin() literal input must be hex")
        })?;
        out.push(char::from((high << 4) | low));
    }
    Ok(out)
}

pub(super) fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}







pub(super) fn eval_literal_implode(call: &Expr, args: &[Expr]) -> Result<String, CompileError> {
    let (separator, values): (String, Vec<String>) = match args {
        [array] => ("".to_string(), eval_literal_string_array(call, array)?),
        [separator, array] => (literal_format_string_arg(separator)?, eval_literal_string_array(call, array)?),
        _ => {
            return Err(CompileError::new(
                call.span,
                "wasm32-web implode() expects one or two literal arguments",
            ));
        }
    };
    Ok(values.join(separator.as_str()))
}

pub(super) fn eval_literal_string_array(call: &Expr, expr: &Expr) -> Result<Vec<String>, CompileError> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => items.iter().map(literal_format_string_arg).collect(),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .map(|(_, value)| literal_format_string_arg(value))
            .collect(),
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("explode") => {
            eval_literal_explode(call, args)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("str_split") => {
            eval_literal_str_split(call, args)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("sscanf") => {
            eval_literal_sscanf(call, args)
        }
        _ => Err(CompileError::new(
            expr.span,
            "wasm32-web implode() currently requires a literal array, explode(), str_split(), or sscanf()",
        )),
    }
}

pub(super) fn eval_literal_explode(call: &Expr, args: &[Expr]) -> Result<Vec<String>, CompileError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web explode() expects two or three literal arguments",
        ));
    }
    let separator = literal_string_arg(&args[0])?;
    if separator.is_empty() {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web explode() separator must be non-empty",
        ));
    }
    let value = literal_string_arg(&args[1])?;
    let limit = args.get(2).map(literal_int_arg).transpose()?;
    let parts: Vec<&str> = value.split(separator).collect();
    let result = match limit {
        None => parts.into_iter().map(str::to_string).collect(),
        Some(0) => vec![value.to_string()],
        Some(limit) if limit > 0 => {
            if limit == 1 {
                vec![value.to_string()]
            } else {
                value
                    .splitn(limit as usize, separator)
                    .map(str::to_string)
                    .collect()
            }
        }
        Some(limit) => {
            let keep = parts.len().saturating_sub((-limit) as usize);
            parts.into_iter().take(keep).map(str::to_string).collect()
        }
    };
    Ok(result)
}

pub(super) fn eval_static_explode(
    call: &Expr,
    args: &[Expr],
    module: &WasmModule,
) -> Result<Vec<String>, CompileError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web explode() expects two or three static string arguments",
        ));
    }
    let separator = static_ascii_string_arg(call, &args[0], module)?;
    if separator.is_empty() {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web explode() separator must be non-empty",
        ));
    }
    let value = static_ascii_string_arg(call, &args[1], module)?;
    let limit = args
        .get(2)
        .map(|arg| {
            static_or_module_const_int_value(arg, module).ok_or_else(|| {
                CompileError::new(
                    arg.span,
                    "wasm32-web explode() limit must be a static integer",
                )
            })
        })
        .transpose()?;
    eval_explode_parts(value, separator, limit)
}

pub(super) fn eval_explode_parts(
    value: String,
    separator: String,
    limit: Option<i64>,
) -> Result<Vec<String>, CompileError> {
    let parts = value.split(separator.as_str()).collect::<Vec<_>>();
    let result = match limit {
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
    };
    Ok(result)
}

pub(super) fn eval_literal_str_split(call: &Expr, args: &[Expr]) -> Result<Vec<String>, CompileError> {
    if args.is_empty() || args.len() > 2 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web str_split() expects one or two literal arguments",
        ));
    }
    let value = literal_string_arg(&args[0])?;
    let length = args.get(1).map(literal_int_arg).transpose()?.unwrap_or(1);
    if length <= 0 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web str_split() length must be greater than zero",
        ));
    }
    Ok(value
        .as_bytes()
        .chunks(length as usize)
        .map(|chunk| chunk.iter().map(|byte| char::from(*byte)).collect())
        .collect())
}

pub(super) fn eval_static_str_split(
    call: &Expr,
    args: &[Expr],
    module: &WasmModule,
) -> Result<Vec<String>, CompileError> {
    if args.is_empty() || args.len() > 2 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web str_split() expects one or two static arguments",
        ));
    }
    let value = static_ascii_string_arg(call, &args[0], module)?;
    let length = args
        .get(1)
        .map(|arg| {
            static_or_module_const_int_value(arg, module).ok_or_else(|| {
                CompileError::new(
                    arg.span,
                    "wasm32-web str_split() length must be a static integer",
                )
            })
        })
        .transpose()?
        .unwrap_or(1);
    if length <= 0 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web str_split() length must be greater than zero",
        ));
    }
    Ok(value
        .as_bytes()
        .chunks(length as usize)
        .map(|chunk| chunk.iter().map(|byte| char::from(*byte)).collect())
        .collect())
}








pub(super) fn digit_allowed_for_radix(byte: u8, radix: u32) -> bool {
    char::from(byte).to_digit(radix).is_some()
}


pub(super) fn bytes_to_ascii_string(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| char::from(*byte)).collect()
}





pub(super) fn nl2br(value: &str) -> String {
    let mut out = String::new();
    let bytes = value.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                out.push_str("<br />\r\n");
                index += 2;
            }
            b'\n' => {
                out.push_str("<br />\n");
                index += 1;
            }
            b'\r' => {
                out.push_str("<br />\r");
                index += 1;
            }
            byte => {
                out.push(char::from(byte));
                index += 1;
            }
        }
    }
    out
}

#[derive(Clone, Copy)]
pub(super) enum TrimSide {
    Left,
    Right,
    Both,
}

pub(super) fn trim_php_default(value: &str, side: TrimSide) -> String {
    fn is_trimmed(byte: u8) -> bool {
        matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0 | 0x0b)
    }
    let bytes = value.as_bytes();
    let mut start = 0usize;
    let mut end = bytes.len();
    if matches!(side, TrimSide::Left | TrimSide::Both) {
        while start < end && is_trimmed(bytes[start]) {
            start += 1;
        }
    }
    if matches!(side, TrimSide::Right | TrimSide::Both) {
        while end > start && is_trimmed(bytes[end - 1]) {
            end -= 1;
        }
    }
    bytes[start..end].iter().copied().map(char::from).collect()
}

pub(super) fn eval_literal_trim(call: &Expr, name: &str, args: &[Expr]) -> Result<String, CompileError> {
    if args.is_empty() || args.len() > 2 {
        return Err(CompileError::new(
            call.span,
            &format!("wasm32-web {}() expects one or two arguments", name),
        ));
    }
    let value = literal_string_arg(&args[0])?;
    let side = match name.to_ascii_lowercase().as_str() {
        "ltrim" => TrimSide::Left,
        "rtrim" => TrimSide::Right,
        _ => TrimSide::Both,
    };
    let Some(charlist) = args.get(1) else {
        return Ok(trim_php_default(value, side));
    };
    let charlist = literal_string_arg(charlist)?;
    Ok(trim_literal_charlist(value, charlist, side))
}

pub(super) fn trim_literal_charlist(value: &str, charlist: &str, side: TrimSide) -> String {
    let bytes = value.as_bytes();
    let chars = charlist.as_bytes();
    let mut start = 0usize;
    let mut end = bytes.len();
    if matches!(side, TrimSide::Left | TrimSide::Both) {
        while start < end && chars.contains(&bytes[start]) {
            start += 1;
        }
    }
    if matches!(side, TrimSide::Right | TrimSide::Both) {
        while end > start && chars.contains(&bytes[end - 1]) {
            end -= 1;
        }
    }
    bytes[start..end].iter().copied().map(char::from).collect()
}

pub(super) fn eval_literal_substr(call: &Expr, args: &[Expr]) -> Result<String, CompileError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web substr() expects two or three arguments",
        ));
    }
    let value = literal_string_arg(&args[0])?;
    let offset = literal_int_arg(&args[1])?;
    let length = args.get(2).map(literal_int_arg).transpose()?;
    let bytes = value.as_bytes();
    let len = bytes.len() as i64;
    let start = if offset < 0 { (len + offset).max(0) } else { offset.min(len) };
    let mut end = match length {
        Some(length) if length >= 0 => (start + length).min(len),
        Some(length) => (len + length).max(start),
        None => len,
    };
    if end < start {
        end = start;
    }
    Ok(bytes[start as usize..end as usize]
        .iter()
        .copied()
        .map(char::from)
        .collect())
}

pub(super) fn eval_literal_str_pad(call: &Expr, args: &[Expr]) -> Result<String, CompileError> {
    if args.len() < 2 || args.len() > 4 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web str_pad() expects two to four arguments",
        ));
    }
    let input = literal_string_arg(&args[0])?;
    let target_len = literal_int_arg(&args[1])?;
    let pad = args.get(2).map(literal_string_arg).transpose()?.unwrap_or(" ");
    if pad.is_empty() {
        return Err(CompileError::new(
            call.span,
            "wasm32-web str_pad() requires a non-empty literal pad string",
        ));
    }
    let pad_type = args.get(3).map(const_or_literal_int_arg).transpose()?.unwrap_or(1);
    let target_len = target_len.max(0) as usize;
    if input.len() >= target_len {
        return Ok(input.to_string());
    }
    let needed = target_len - input.len();
    let (left, right) = match pad_type {
        0 => (needed, 0),
        2 => (needed / 2, needed - (needed / 2)),
        _ => (0, needed),
    };
    Ok(format!(
        "{}{}{}",
        repeated_pad(pad, left),
        input,
        repeated_pad(pad, right)
    ))
}

pub(super) fn repeated_pad(pad: &str, len: usize) -> String {
    let mut out = String::new();
    while out.len() < len {
        out.push_str(pad);
    }
    out.truncate(len);
    out
}

pub(super) fn eval_literal_substr_replace(call: &Expr, args: &[Expr]) -> Result<String, CompileError> {
    if args.len() != 3 && args.len() != 4 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web substr_replace() expects three or four arguments",
        ));
    }
    let value = literal_string_arg(&args[0])?;
    let replacement = literal_string_arg(&args[1])?;
    let offset = literal_int_arg(&args[2])?;
    let length = args.get(3).map(literal_int_arg).transpose()?;
    let bytes = value.as_bytes();
    let len = bytes.len() as i64;
    let start = if offset < 0 { (len + offset).max(0) } else { offset.min(len) };
    let end = match length {
        Some(length) if length < 0 => (len + length).max(start),
        Some(length) => (start + length).min(len),
        None => len,
    };
    let prefix = &value[..start as usize];
    let suffix = &value[end as usize..];
    Ok(format!("{}{}{}", prefix, replacement, suffix))
}

pub(super) fn eval_literal_str_replace(
    call: &Expr,
    name: &str,
    args: &[Expr],
) -> Result<String, CompileError> {
    let [search, replace, subject] = args else {
        return Err(CompileError::new(
            call.span,
            &format!("wasm32-web {}() expects exactly three arguments", name),
        ));
    };
    let search = literal_string_arg(search)?;
    let replace = literal_string_arg(replace)?;
    let subject = literal_string_arg(subject)?;
    let (value, _) = eval_literal_str_replace_with_count(name, search, replace, subject);
    Ok(value)
}

pub(super) fn eval_static_str_replace_args(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &WasmModule,
) -> Result<Option<(String, i64)>, CompileError> {
    if args.len() != 3 && args.len() != 4 {
        return Ok(None);
    }
    let Some(subject) = static_string_coercion_value(&args[2], module) else {
        return Ok(None);
    };
    let Some(search_values) = static_string_coercion_arg_or_array(&args[0], module) else {
        return Ok(None);
    };
    let Some(replace_values) = static_string_coercion_arg_or_array(&args[1], module) else {
        return Ok(None);
    };
    if !matches!(args[0].kind, ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_))
        && matches!(args[1].kind, ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_))
    {
        return Err(CompileError::new(
            call.span,
            "wasm32-web str_replace() array replacement requires an array search argument",
        ));
    }
    let replacement_is_array =
        matches!(args[1].kind, ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_));
    let mut value = subject;
    let mut total_count = 0i64;
    for (index, search) in search_values.iter().enumerate() {
        let replace = if replacement_is_array {
            replace_values.get(index).map_or("", String::as_str)
        } else {
            replace_values.first().map_or("", String::as_str)
        };
        let (next, count) = eval_literal_str_replace_with_count(name, search, replace, &value);
        value = next;
        total_count += count;
    }
    Ok(Some((value, total_count)))
}

pub(super) fn static_string_coercion_arg_or_array(expr: &Expr, module: &WasmModule) -> Option<Vec<String>> {
    if let Some(value) = static_string_coercion_value(expr, module) {
        return Some(vec![value]);
    }
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => items
            .iter()
            .map(|item| static_string_coercion_value(item, module))
            .collect(),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .map(|(_, item)| static_string_coercion_value(item, module))
            .collect(),
        _ => None,
    }
}

pub(super) fn eval_literal_str_replace_with_count(
    name: &str,
    search: &str,
    replace: &str,
    subject: &str,
) -> (String, i64) {
    if search.is_empty() {
        return (subject.to_string(), 0);
    }
    if name.eq_ignore_ascii_case("str_ireplace") {
        return ascii_case_insensitive_replace_with_count(subject, search, replace);
    }
    literal_replace_with_count(subject, search, replace)
}

pub(super) fn literal_replace_with_count(subject: &str, search: &str, replace: &str) -> (String, i64) {
    let mut out = String::new();
    let mut count = 0i64;
    let mut index = 0usize;
    while index < subject.len() {
        if subject[index..].starts_with(search) {
            out.push_str(replace);
            index += search.len();
            count += 1;
        } else {
            out.push(char::from(subject.as_bytes()[index]));
            index += 1;
        }
    }
    (out, count)
}

pub(super) fn ascii_case_insensitive_replace_with_count(
    subject: &str,
    search: &str,
    replace: &str,
) -> (String, i64) {
    let subject_lower = ascii_lower(subject);
    let search_lower = ascii_lower(search);
    let mut out = String::new();
    let mut count = 0i64;
    let mut index = 0usize;
    while index < subject.len() {
        if subject_lower[index..].starts_with(&search_lower) {
            out.push_str(replace);
            index += search.len();
            count += 1;
        } else {
            out.push(char::from(subject.as_bytes()[index]));
            index += 1;
        }
    }
    (out, count)
}

pub(super) fn eval_literal_strstr(
    call: &Expr,
    args: &[Expr],
    module: &WasmModule,
) -> Result<String, CompileError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web strstr() expects two or three arguments",
        ));
    }
    let haystack = static_ascii_string_arg(call, &args[0], module)?;
    let needle = static_ascii_string_arg(call, &args[1], module)?;
    let before_needle = args.get(2).map(literal_bool_arg).transpose()?.unwrap_or(false);
    let Some(index) = haystack.find(&needle) else {
        return Ok(String::new());
    };
    if before_needle {
        Ok(haystack[..index].to_string())
    } else {
        Ok(haystack[index..].to_string())
    }
}

pub(super) fn eval_literal_wordwrap(call: &Expr, args: &[Expr]) -> Result<String, CompileError> {
    if args.is_empty() || args.len() > 4 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web wordwrap() expects one to four arguments",
        ));
    }
    let value = literal_string_arg(&args[0])?;
    let width = args.get(1).map(literal_int_arg).transpose()?.unwrap_or(75);
    let break_text = args.get(2).map(literal_string_arg).transpose()?.unwrap_or("\n");
    let cut = args.get(3).map(literal_bool_arg).transpose()?.unwrap_or(false);
    if width <= 0 || value.len() <= width as usize {
        return Ok(value.to_string());
    }
    let width = width as usize;
    let mut out = String::new();
    let mut line_start = 0usize;
    for word in value.split(' ') {
        if cut && word.len() > width {
            if out.len() > line_start {
                out.push_str(break_text);
                line_start = out.len();
            }
            let mut chunk_start = 0usize;
            while word.len() - chunk_start > width {
                out.push_str(&word[chunk_start..chunk_start + width]);
                out.push_str(break_text);
                line_start = out.len();
                chunk_start += width;
            }
            out.push_str(&word[chunk_start..]);
            continue;
        }
        let pending = if out.len() == line_start { word.len() } else { word.len() + 1 };
        if out.len() + pending - line_start > width && out.len() > line_start {
            out.push_str(break_text);
            line_start = out.len();
            out.push_str(word);
        } else {
            if out.len() > line_start {
                out.push(' ');
            }
            out.push_str(word);
        }
    }
    Ok(out)
}
