//! Purpose:
//! Evaluates literal URL and base64 string encodings for wasm32-web.
//! Keeps pure string transforms separate from runtime string builtin emission.
//!
//! Called from:
//! - `crate::codegen::wasm::expr`
//!
//! Key details:
//! - Base64 strict-mode and malformed percent decoding still report CompileError.

use super::*;

pub(super) fn url_encode(value: &str, space: SpaceEncoding) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => out.push(char::from(byte)),
            b' ' if matches!(space, SpaceEncoding::Plus) => out.push('+'),
            b'~' if matches!(space, SpaceEncoding::Percent20) => out.push('~'),
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

pub(super) fn url_decode(value: &str, space: SpaceEncoding) -> Result<String, CompileError> {
    let mut out = String::new();
    let bytes = value.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'+' if matches!(space, SpaceEncoding::Plus) => {
                out.push(' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                match (hex_nibble(bytes[index + 1]), hex_nibble(bytes[index + 2])) {
                    (Some(high), Some(low)) => {
                        out.push(char::from((high << 4) | low));
                        index += 3;
                    }
                    _ => {
                        out.push('%');
                        index += 1;
                    }
                }
            }
            byte => {
                out.push(char::from(byte));
                index += 1;
            }
        }
    }
    Ok(out)
}

pub(super) fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        out.push(char::from(TABLE[(b0 >> 2) as usize]));
        out.push(char::from(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize]));
        if chunk.len() > 1 {
            out.push(char::from(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize]));
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(char::from(TABLE[(b2 & 0x3f) as usize]));
        } else {
            out.push('=');
        }
    }
    out
}

pub(super) fn base64_decode(call: &Expr, value: &str, strict: bool) -> Result<String, CompileError> {
    let Some(mut clean) = normalize_base64_input(value, strict) else {
        return Ok(String::new());
    };
    match clean.len() % 4 {
        0 => {}
        2 => clean.extend_from_slice(b"=="),
        3 => clean.push(b'='),
        _ if strict => return Ok(String::new()),
        _ => {
            return Err(CompileError::new(
                call.span,
                "wasm32-web base64_decode() literal input has invalid padding",
            ));
        }
    }
    let mut out = Vec::new();
    for chunk in clean.chunks(4) {
        let a = base64_value(chunk[0], call)?;
        let b = base64_value(chunk[1], call)?;
        let c = if chunk[2] == b'=' { 64 } else { base64_value(chunk[2], call)? };
        let d = if chunk[3] == b'=' { 64 } else { base64_value(chunk[3], call)? };
        out.push((a << 2) | (b >> 4));
        if c != 64 {
            out.push(((b & 0x0f) << 4) | (c >> 2));
        }
        if d != 64 {
            out.push(((c & 0x03) << 6) | d);
        }
    }
    Ok(out.into_iter().map(char::from).collect())
}

fn base64_value(byte: u8, call: &Expr) -> Result<u8, CompileError> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(CompileError::new(
            call.span,
            "wasm32-web base64_decode() literal input must be base64",
        )),
    }
}
