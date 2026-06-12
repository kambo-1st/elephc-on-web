//! Purpose:
//! Evaluates literal sscanf() calls for wasm32-web string/array lowering.
//! Owns static scan parsing for supported scanf conversions and null result slots.
//!
//! Called from:
//! - `crate::codegen::wasm::expr`
//!
//! Key details:
//! - Unsupported scan conversions still return CompileError so wasm never silently miscompiles.

use super::*;

pub(super) fn eval_literal_sscanf(call: &Expr, args: &[Expr]) -> Result<Vec<String>, CompileError> {
    let [input, format] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web sscanf() expects exactly two literal arguments",
        ));
    };
    let input = literal_string_arg(input)?.as_bytes();
    let format = literal_string_arg(format)?.as_bytes();
    let mut input_index = 0usize;
    let mut format_index = 0usize;
    let mut out = Vec::new();
    while format_index < format.len() {
        let byte = format[format_index];
        if byte.is_ascii_whitespace() {
            while format_index < format.len() && format[format_index].is_ascii_whitespace() {
                format_index += 1;
            }
            while input_index < input.len() && input[input_index].is_ascii_whitespace() {
                input_index += 1;
            }
            continue;
        }
        if byte != b'%' {
            if input.get(input_index) != Some(&byte) {
                push_literal_sscanf_null_outputs(&mut out, format, format_index);
                break;
            }
            input_index += 1;
            format_index += 1;
            continue;
        }
        format_index += 1;
        let suppress_assignment = if format.get(format_index) == Some(&b'*') {
            format_index += 1;
            true
        } else {
            false
        };
        let mut width = None;
        let width_start = format_index;
        while format_index < format.len() && format[format_index].is_ascii_digit() {
            format_index += 1;
        }
        if width_start != format_index {
            let width_text = bytes_to_ascii_string(&format[width_start..format_index]);
            width = width_text.parse::<usize>().ok().filter(|value| *value > 0);
        }
        while matches!(format.get(format_index), Some(b'h' | b'l' | b'L')) {
            format_index += 1;
        }
        let Some(spec) = format.get(format_index).copied() else {
            return Err(CompileError::new(
                call.span,
                "wasm32-web sscanf() literal format is incomplete",
            ));
        };
        format_index += 1;
        if !matches!(
            spec,
            b'%' | b'n' | b'c' | b's' | b'd' | b'u' | b'i' | b'f' | b'e' | b'E' | b'g'
                | b'o' | b'x' | b'X' | b'['
        ) {
            return Err(CompileError::new(
                call.span,
                &format!(
                    "wasm32-web sscanf() does not support scan conversion {}",
                    char::from(spec)
                ),
            ));
        }
        let mut scanset = None;
        let mut scanset_negated = false;
        if spec == b'[' {
            if format.get(format_index) == Some(&b'^') {
                scanset_negated = true;
                format_index += 1;
            }
            let scanset_start = format_index;
            if format.get(format_index) == Some(&b']') {
                format_index += 1;
            }
            while format_index < format.len() && format[format_index] != b']' {
                format_index += 1;
            }
            if scanset_start == format_index || format.get(format_index) != Some(&b']') {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web sscanf() literal scanset format is incomplete",
                ));
            }
            scanset = Some(&format[scanset_start..format_index]);
            format_index += 1;
        }
        if spec == b'%' {
            if input.get(input_index) != Some(&b'%') {
                push_literal_sscanf_null_outputs(&mut out, format, format_index);
                break;
            }
            input_index += 1;
            continue;
        }
        if spec == b'n' {
            if !suppress_assignment {
                out.push(input_index.to_string());
            }
            continue;
        }
        if !matches!(spec, b'c' | b'[') {
            while input_index < input.len() && input[input_index].is_ascii_whitespace() {
                input_index += 1;
            }
        }
        if let Some(scanset) = scanset {
            let Some(value) = scan_literal_sscanf_scanset(
                input,
                &mut input_index,
                scanset,
                scanset_negated,
                width,
            ) else {
                push_literal_sscanf_current_and_remaining_null_outputs(
                    &mut out,
                    format,
                    format_index,
                    suppress_assignment,
                );
                break;
            };
            if !suppress_assignment {
                out.push(value);
            }
            continue;
        }
        if spec != b'c' {
            while input_index < input.len() && input[input_index].is_ascii_whitespace() {
                input_index += 1;
            }
        }
        let Some(value) = scan_literal_sscanf_value(input, &mut input_index, spec, width) else {
            push_literal_sscanf_current_and_remaining_null_outputs(
                &mut out,
                format,
                format_index,
                suppress_assignment,
            );
            break;
        };
        if !suppress_assignment {
            out.push(value);
        }
    }
    Ok(out)
}

fn push_literal_sscanf_current_and_remaining_null_outputs(
    out: &mut Vec<String>,
    format: &[u8],
    format_index: usize,
    suppress_assignment: bool,
) {
    if !suppress_assignment {
        out.push(String::new());
    }
    push_literal_sscanf_null_outputs(out, format, format_index);
}

fn push_literal_sscanf_null_outputs(out: &mut Vec<String>, format: &[u8], mut index: usize) {
    while index < format.len() {
        if format[index] != b'%' {
            index += 1;
            continue;
        }
        index += 1;
        if format.get(index) == Some(&b'%') {
            index += 1;
            continue;
        }
        let suppress_assignment = if format.get(index) == Some(&b'*') {
            index += 1;
            true
        } else {
            false
        };
        while index < format.len() && format[index].is_ascii_digit() {
            index += 1;
        }
        while matches!(format.get(index), Some(b'h' | b'l' | b'L')) {
            index += 1;
        }
        let Some(spec) = format.get(index).copied() else {
            break;
        };
        index += 1;
        if spec == b'[' {
            if format.get(index) == Some(&b'^') {
                index += 1;
            }
            if format.get(index) == Some(&b']') {
                index += 1;
            }
            while index < format.len() && format[index] != b']' {
                index += 1;
            }
            if format.get(index) == Some(&b']') {
                index += 1;
            }
        }
        if !suppress_assignment {
            out.push(String::new());
        }
    }
}

fn scan_literal_sscanf_value(
    input: &[u8],
    index: &mut usize,
    spec: u8,
    width: Option<usize>,
) -> Option<String> {
    match spec {
        b'c' => {
            let count = width.unwrap_or(1);
            if *index + count > input.len() {
                None
            } else {
                let start = *index;
                *index += count;
                Some(bytes_to_ascii_string(&input[start..*index]))
            }
        }
        b's' => {
            let start = *index;
            while *index < input.len()
                && !input[*index].is_ascii_whitespace()
                && width.is_none_or(|limit| *index - start < limit)
            {
                *index += 1;
            }
            (start != *index).then(|| bytes_to_ascii_string(&input[start..*index]))
        }
        b'd' | b'u' => {
            let start = *index;
            if matches!(input.get(*index), Some(b'+') | Some(b'-')) {
                *index += 1;
            }
            let negative = input.get(start) == Some(&b'-');
            let sign_len = usize::from(matches!(input.get(start), Some(b'+') | Some(b'-')));
            let digits_start = *index;
            while *index < input.len()
                && input[*index].is_ascii_digit()
                && width.is_none_or(|limit| *index - start < limit)
            {
                *index += 1;
            }
            if digits_start == *index {
                *index = start;
                None
            } else if spec == b'u' {
                let text = bytes_to_ascii_string(&input[start + sign_len..*index]);
                let value = text.parse::<u64>().ok()?;
                Some(if negative {
                    0u64.wrapping_sub(value).to_string()
                } else {
                    value.to_string()
                })
            } else {
                let text = bytes_to_ascii_string(&input[start + sign_len..*index]);
                let value = text.parse::<i64>().ok()?;
                Some(if negative { -value } else { value }.to_string())
            }
        }
        b'i' => scan_literal_sscanf_int_auto_base(input, index, width),
        b'f' | b'e' | b'E' | b'g' => scan_literal_sscanf_float(input, index, width)
            .and_then(|value| value.parse::<f64>().ok())
            .map(|value| format_php_general(value, 14, true)),
        b'o' => {
            let start = *index;
            if matches!(input.get(*index), Some(b'+') | Some(b'-')) {
                *index += 1;
            }
            let negative = input.get(start) == Some(&b'-');
            let sign_len = usize::from(matches!(input.get(start), Some(b'+') | Some(b'-')));
            let digits_start = *index;
            while *index < input.len()
                && matches!(input[*index], b'0'..=b'7')
                && width.is_none_or(|limit| *index - start < limit)
            {
                *index += 1;
            }
            if digits_start == *index {
                *index = start;
                None
            } else {
                let text = bytes_to_ascii_string(&input[start + sign_len..*index]);
                i64::from_str_radix(&text, 8)
                    .ok()
                    .map(|value| if negative { -value } else { value }.to_string())
            }
        }
        b'x' | b'X' => {
            let start = *index;
            if matches!(input.get(*index), Some(b'+') | Some(b'-')) {
                *index += 1;
            }
            let negative = input.get(start) == Some(&b'-');
            let sign_len = usize::from(matches!(input.get(start), Some(b'+') | Some(b'-')));
            let prefixed = sign_len == 0
                && input.get(*index) == Some(&b'0')
                && matches!(input.get(*index + 1), Some(b'x') | Some(b'X'))
                && width.is_none_or(|limit| limit >= 2);
            if prefixed {
                *index += 2;
            }
            let digits_start = *index;
            while *index < input.len()
                && input[*index].is_ascii_hexdigit()
                && width.is_none_or(|limit| *index - start < limit)
            {
                *index += 1;
            }
            if digits_start == *index && !prefixed {
                *index = start;
                None
            } else {
                let text_start = if prefixed {
                    digits_start
                } else {
                    start + sign_len
                };
                let text = bytes_to_ascii_string(&input[text_start..*index]);
                let value = if text.is_empty() {
                    0
                } else {
                    i64::from_str_radix(&text, 16).ok()?
                };
                Some(if negative { -value } else { value }.to_string())
            }
        }
        _ => None,
    }
}

fn scan_literal_sscanf_scanset(
    input: &[u8],
    index: &mut usize,
    scanset: &[u8],
    negated: bool,
    width: Option<usize>,
) -> Option<String> {
    let start = *index;
    while *index < input.len()
        && (sscanf_scanset_contains(scanset, input[*index]) != negated)
        && width.is_none_or(|limit| *index - start < limit)
    {
        *index += 1;
    }
    (start != *index).then(|| bytes_to_ascii_string(&input[start..*index]))
}

fn sscanf_scanset_contains(scanset: &[u8], byte: u8) -> bool {
    let mut index = 0usize;
    while index < scanset.len() {
        if index + 2 < scanset.len() && scanset[index + 1] == b'-' {
            let start = scanset[index];
            let end = scanset[index + 2];
            if (start <= byte && byte <= end) || (end <= byte && byte <= start) {
                return true;
            }
            index += 3;
            continue;
        }
        if scanset[index] == byte {
            return true;
        }
        index += 1;
    }
    false
}

fn scan_literal_sscanf_int_auto_base(
    input: &[u8],
    index: &mut usize,
    width: Option<usize>,
) -> Option<String> {
    let start = *index;
    if matches!(input.get(*index), Some(b'+') | Some(b'-')) {
        *index += 1;
    }
    let negative = input.get(start) == Some(&b'-');
    let sign_len = usize::from(matches!(input.get(start), Some(b'+') | Some(b'-')));
    let remaining_width = width.map(|limit| limit.saturating_sub(sign_len));
    let (radix, digits_start) = if sign_len == 0
        && input.get(*index) == Some(&b'0')
        && matches!(input.get(*index + 1), Some(b'x') | Some(b'X'))
        && remaining_width.is_none_or(|limit| limit >= 2)
    {
        *index += 2;
        (16, *index)
    } else if input.get(*index) == Some(&b'0') {
        *index += 1;
        (8, *index - 1)
    } else {
        (10, *index)
    };
    while *index < input.len()
        && input[*index].is_ascii_hexdigit()
        && digit_allowed_for_radix(input[*index], radix)
        && width.is_none_or(|limit| *index - start < limit)
    {
        *index += 1;
    }
    if digits_start == *index {
        *index = start;
        return None;
    }
    let text_start = if radix == 16 { digits_start } else { start + sign_len };
    let text = bytes_to_ascii_string(&input[text_start..*index]);
    i64::from_str_radix(&text, radix)
        .ok()
        .map(|value| if negative { -value } else { value }.to_string())
}

fn scan_literal_sscanf_float(
    input: &[u8],
    index: &mut usize,
    width: Option<usize>,
) -> Option<String> {
    let start = *index;
    if matches!(input.get(*index), Some(b'+') | Some(b'-')) {
        *index += 1;
    }
    let mut saw_digit = false;
    while *index < input.len()
        && input[*index].is_ascii_digit()
        && width.is_none_or(|limit| *index - start < limit)
    {
        saw_digit = true;
        *index += 1;
    }
    if input.get(*index) == Some(&b'.') && width.is_none_or(|limit| *index - start < limit) {
        *index += 1;
        while *index < input.len()
            && input[*index].is_ascii_digit()
            && width.is_none_or(|limit| *index - start < limit)
        {
            saw_digit = true;
            *index += 1;
        }
    }
    if matches!(input.get(*index), Some(b'e') | Some(b'E'))
        && width.is_none_or(|limit| *index - start < limit)
    {
        let exponent = *index;
        *index += 1;
        if matches!(input.get(*index), Some(b'+') | Some(b'-'))
            && width.is_none_or(|limit| *index - start < limit)
        {
            *index += 1;
        }
        let exponent_digits = *index;
        while *index < input.len()
            && input[*index].is_ascii_digit()
            && width.is_none_or(|limit| *index - start < limit)
        {
            *index += 1;
        }
        if exponent_digits == *index {
            *index = exponent;
        }
    }
    if saw_digit {
        Some(bytes_to_ascii_string(&input[start..*index]))
    } else {
        *index = start;
        None
    }
}
