//! Purpose:
//! Evaluates and shares HTML entity escaping/decoding helpers for wasm32-web.
//! Keeps literal HTML string semantics out of the expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` output string builtin paths.
//! - `crate::codegen::wasm::expr::string_runtime` runtime HTML emitters.
//!
//! Key details:
//! - Encoding support intentionally stays limited to ASCII-compatible UTF-8/ISO-8859-1 cases.

use super::*;

pub(super) fn emit_html_entity_decode_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !name.eq_ignore_ascii_case("html_entity_decode") {
        return Ok(false);
    }
    if args.is_empty() || args.len() > 3 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web html_entity_decode() expects one to three arguments",
        ));
    }
    if args.iter().all(|arg| static_string_value(arg, module).is_some() || static_or_const_int_value(arg).is_some()) {
        let value = eval_output_string_builtin(call, name, args, module)?;
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(true);
    }
    let Some(var) = string_arg_or_materialize(&args[0], "html_entity_decode_value_arg", module)? else {
        return Ok(false);
    };
    if let Some(encoding_arg) = args.get(2) {
        let Some(encoding) = static_or_tracked_string_value(encoding_arg, module) else {
            return Err(CompileError::new(
                encoding_arg.span,
                "wasm32-web html_entity_decode() string values currently require a static encoding",
            ));
        };
        if !html_encoding_is_ascii_compatible(&encoding) {
            return Err(CompileError::new(
                encoding_arg.span,
                "wasm32-web html_entity_decode() string values currently support UTF-8 or ISO-8859-1 only for ASCII-safe strings",
            ));
        }
    }
    if let Some(flags_arg) = args.get(1) {
        if let Some(flags) = static_or_const_int_value(flags_arg) {
            emit_runtime_html_entity_decode_value_to_stack(&var, flags, module);
        } else {
            emit_runtime_html_entity_decode_dynamic_value_to_stack(&var, flags_arg, module)?;
        }
    } else {
        emit_runtime_html_entity_decode_value_to_stack(&var, 3, module);
    }
    Ok(true)
}

pub(super) fn validate_html_encoding(
    call: &Expr,
    encoding: Option<&Expr>,
    module: &WasmModule,
) -> Result<(), CompileError> {
    let Some(encoding) = encoding else {
        return Ok(());
    };
    let encoding = static_or_tracked_string_value(encoding, module).ok_or_else(|| {
        CompileError::new(
            encoding.span,
            "wasm32-web HTML helpers currently support static UTF-8 or ISO-8859-1 encoding only for ASCII-safe strings",
        )
    })?;
    if html_encoding_is_ascii_compatible(&encoding) {
        Ok(())
    } else {
        Err(CompileError::new(
            call.span,
            "wasm32-web HTML helpers currently support literal UTF-8 or ISO-8859-1 encoding only for ASCII-safe strings",
        ))
    }
}

pub(super) fn html_encoding_is_ascii_compatible(encoding: &str) -> bool {
    matches!(
        encoding.to_ascii_lowercase().as_str(),
        "utf-8" | "utf8" | "iso-8859-1" | "iso8859-1"
    )
}

pub(super) fn html_single_quote_entity(name: &str, flags: i64) -> &'static str {
    let doc_type = flags & 48;
    if name.eq_ignore_ascii_case("htmlspecialchars") {
        if doc_type == 0 {
            "&#039;"
        } else {
            "&apos;"
        }
    } else if doc_type == 16 || doc_type == 48 {
        "&apos;"
    } else {
        "&#039;"
    }
}

pub(super) fn html_decodes_apos(flags: i64) -> bool {
    flags & 48 != 0
}

pub(super) fn html_utf8_entity_decodes() -> &'static [(&'static str, &'static str)] {
    &[
        ("&nbsp;", "\u{00a0}"),
        ("&cent;", "\u{00a2}"),
        ("&pound;", "\u{00a3}"),
        ("&yen;", "\u{00a5}"),
        ("&sect;", "\u{00a7}"),
        ("&copy;", "\u{00a9}"),
        ("&reg;", "\u{00ae}"),
        ("&deg;", "\u{00b0}"),
        ("&plusmn;", "\u{00b1}"),
        ("&laquo;", "\u{00ab}"),
        ("&raquo;", "\u{00bb}"),
        ("&times;", "\u{00d7}"),
        ("&divide;", "\u{00f7}"),
        ("&euro;", "\u{20ac}"),
        ("&ndash;", "\u{2013}"),
        ("&mdash;", "\u{2014}"),
        ("&lsquo;", "\u{2018}"),
        ("&rsquo;", "\u{2019}"),
        ("&ldquo;", "\u{201c}"),
        ("&rdquo;", "\u{201d}"),
        ("&hellip;", "\u{2026}"),
        ("&trade;", "\u{2122}"),
    ]
}

pub(super) fn html_utf8_entity_encodes() -> &'static [(&'static str, &'static str)] {
    &[
        ("\u{00a0}", "&nbsp;"),
        ("\u{00a2}", "&cent;"),
        ("\u{00a3}", "&pound;"),
        ("\u{00a5}", "&yen;"),
        ("\u{00a7}", "&sect;"),
        ("\u{00a9}", "&copy;"),
        ("\u{00ae}", "&reg;"),
        ("\u{00b0}", "&deg;"),
        ("\u{00b1}", "&plusmn;"),
        ("\u{00ab}", "&laquo;"),
        ("\u{00bb}", "&raquo;"),
        ("\u{00d7}", "&times;"),
        ("\u{00f7}", "&divide;"),
        ("\u{20ac}", "&euro;"),
        ("\u{2013}", "&ndash;"),
        ("\u{2014}", "&mdash;"),
        ("\u{2019}", "&rsquo;"),
        ("\u{201c}", "&OpenCurlyDoubleQuote;"),
        ("\u{201d}", "&rdquo;"),
        ("\u{2026}", "&hellip;"),
        ("\u{2122}", "&trade;"),
    ]
}

pub(super) fn html_escape(name: &str, value: &str, flags: i64, double_encode: bool) -> String {
    let mut escaped = String::new();
    let bytes = value.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if !double_encode && bytes[index] == b'&' {
            if let Some(entity_len) = html_entity_len(&bytes[index..]) {
                escaped.push_str(&value[index..index + entity_len]);
                index += entity_len;
                continue;
            }
        }
        if name.eq_ignore_ascii_case("htmlentities") {
            if let Some((decoded, entity)) = html_utf8_entity_encodes()
                .iter()
                .find(|(decoded, _)| bytes[index..].starts_with(decoded.as_bytes()))
            {
                escaped.push_str(entity);
                index += decoded.len();
                continue;
            }
        }
        let Some(ch) = value[index..].chars().next() else {
            break;
        };
        match ch {
            '&' => escaped.push_str("&amp;"),
            '"' if flags & 2 != 0 => escaped.push_str("&quot;"),
            '\'' if flags & 1 != 0 => escaped.push_str(html_single_quote_entity(name, flags)),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            _ => escaped.push(ch),
        }
        index += ch.len_utf8();
    }
    escaped
}

pub(super) fn html_entity_len(bytes: &[u8]) -> Option<usize> {
    for entity in ["&amp;", "&lt;", "&gt;", "&quot;", "&#039;", "&#39;"] {
        if bytes.starts_with(entity.as_bytes()) {
            return Some(entity.len());
        }
    }
    for (entity, _) in html_utf8_entity_decodes() {
        if bytes.starts_with(entity.as_bytes()) {
            return Some(entity.len());
        }
    }
    numeric_html_entity(bytes).map(|(_, len)| len)
}

pub(super) fn html_entity_decode(value: &str, flags: i64) -> String {
    let mut decoded = String::new();
    let bytes = value.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'&' {
            if flags & 2 != 0 && bytes[index..].starts_with(b"&quot;") {
                decoded.push('"');
                index += "&quot;".len();
                continue;
            }
            if flags & 1 != 0 && bytes[index..].starts_with(b"&#039;") {
                decoded.push('\'');
                index += "&#039;".len();
                continue;
            }
            if flags & 1 != 0 && html_decodes_apos(flags) && bytes[index..].starts_with(b"&apos;")
            {
                decoded.push('\'');
                index += "&apos;".len();
                continue;
            }
            if bytes[index..].starts_with(b"&lt;") {
                decoded.push('<');
                index += "&lt;".len();
                continue;
            }
            if bytes[index..].starts_with(b"&gt;") {
                decoded.push('>');
                index += "&gt;".len();
                continue;
            }
            if bytes[index..].starts_with(b"&amp;") {
                decoded.push('&');
                index += "&amp;".len();
                continue;
            }
            if let Some((decoded_entity, len)) = html_utf8_entity_decodes()
                .iter()
                .find_map(|(entity, decoded_entity)| {
                    bytes[index..]
                        .starts_with(entity.as_bytes())
                        .then_some((*decoded_entity, entity.len()))
                })
            {
                decoded.push_str(decoded_entity);
                index += len;
                continue;
            }
            if let Some((ch, len)) = numeric_html_entity(&bytes[index..]) {
                if ch == '\'' && flags & 1 == 0 {
                    decoded.push('&');
                    index += 1;
                    continue;
                }
                if ch == '"' && flags & 2 == 0 {
                    decoded.push('&');
                    index += 1;
                    continue;
                }
                decoded.push(ch);
                index += len;
                continue;
            }
        }
        decoded.push(char::from(bytes[index]));
        index += 1;
    }
    decoded
}

pub(super) fn numeric_html_entity(bytes: &[u8]) -> Option<(char, usize)> {
    if !bytes.starts_with(b"&#") {
        return None;
    }
    let mut index = 2usize;
    let radix = if matches!(bytes.get(index), Some(b'x') | Some(b'X')) {
        index += 1;
        16
    } else {
        10
    };
    let digits_start = index;
    while index < bytes.len()
        && match radix {
            16 => bytes[index].is_ascii_hexdigit(),
            _ => bytes[index].is_ascii_digit(),
        }
    {
        index += 1;
    }
    if digits_start == index || bytes.get(index) != Some(&b';') {
        return None;
    }
    let text = std::str::from_utf8(&bytes[digits_start..index]).ok()?;
    let codepoint = u32::from_str_radix(text, radix).ok()?;
    let ch = char::from_u32(codepoint)?;
    Some((ch, index + 1))
}
