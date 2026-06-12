//! Purpose:
//! Parses and formats literal sprintf() calls for the wasm32-web backend.
//! Keeps compile-time format evaluation separate from runtime sprintf emission.
//!
//! Called from:
//! - `super::scalar_builtins`, `super::sprintf`, and `super::sscanf`.
//!
//! Key details:
//! - Matches PHP formatting for supported literal specs and shared general/scientific float formatting.

use super::*;

pub(super) fn eval_literal_sprintf(call: &Expr, args: &[Expr]) -> Result<String, CompileError> {
    if args.is_empty() {
        return Err(CompileError::new(
            call.span,
            "wasm32-web sprintf() expects a literal format",
        ));
    }
    let format = literal_string_arg(&args[0])?;
    let mut arg_index = 1usize;
    let mut out = String::new();
    let bytes = format.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            out.push(char::from(bytes[index]));
            index += 1;
            continue;
        }
        index += 1;
        if bytes.get(index) == Some(&b'%') {
            out.push('%');
            index += 1;
            continue;
        }
        let spec = parse_literal_sprintf_spec(call, bytes, &mut index)?;
        let Some(arg) = args.get(arg_index) else {
            return Err(CompileError::new(
                call.span,
                "wasm32-web sprintf() literal format has too few arguments",
            ));
        };
        arg_index += 1;
        out.push_str(&format_literal_sprintf_arg(call, &spec, arg)?);
    }
    if arg_index != args.len() {
        return Err(CompileError::new(
            call.span,
            "wasm32-web sprintf() literal format has unused arguments",
        ));
    }
    Ok(out)
}

#[derive(Clone, Copy, Default)]
struct LiteralSprintfSpec {
    left_align: bool,
    sign_plus: bool,
    sign_space: bool,
    zero_pad: bool,
    width: Option<usize>,
    precision: Option<usize>,
    spec: u8,
}

fn parse_literal_sprintf_spec(
    call: &Expr,
    bytes: &[u8],
    index: &mut usize,
) -> Result<LiteralSprintfSpec, CompileError> {
    let mut spec = LiteralSprintfSpec::default();
    while let Some(byte) = bytes.get(*index).copied() {
        match byte {
            b'-' => spec.left_align = true,
            b'+' => spec.sign_plus = true,
            b' ' => spec.sign_space = true,
            b'0' => spec.zero_pad = true,
            _ => break,
        }
        *index += 1;
    }
    let width_start = *index;
    while *index < bytes.len() && bytes[*index].is_ascii_digit() {
        *index += 1;
    }
    if *index > width_start {
        let width = std::str::from_utf8(&bytes[width_start..*index]).unwrap_or("0");
        spec.width = Some(width.parse::<usize>().unwrap_or(0));
    }
    if bytes.get(*index) == Some(&b'.') {
        *index += 1;
        let precision_start = *index;
        while *index < bytes.len() && bytes[*index].is_ascii_digit() {
            *index += 1;
        }
        let digits = std::str::from_utf8(&bytes[precision_start..*index]).unwrap_or("0");
        spec.precision = Some(digits.parse::<usize>().unwrap_or(0));
    }
    if bytes.get(*index) == Some(&b'l') {
        *index += 1;
    }
    let Some(format_spec) = bytes.get(*index).copied() else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web sprintf() literal format is incomplete",
        ));
    };
    *index += 1;
    spec.spec = format_spec;
    Ok(spec)
}

fn format_literal_sprintf_arg(
    call: &Expr,
    spec: &LiteralSprintfSpec,
    arg: &Expr,
) -> Result<String, CompileError> {
    let formatted = match spec.spec {
        b's' => {
            let mut value = literal_format_string_arg(arg)?;
            if let Some(precision) = spec.precision {
                value.truncate(precision);
            }
            apply_literal_sprintf_padding(value, spec, None)
        }
        b'd' => {
            let value = literal_format_int_arg(arg)?;
            let (prefix, digits) = signed_decimal_parts(value, spec);
            apply_literal_sprintf_padding(format!("{}{}", prefix, digits), spec, Some(prefix.len()))
        }
        b'c' => char::from(literal_format_int_arg(arg)? as u8).to_string(),
        b'b' => apply_literal_sprintf_padding(format!("{:b}", literal_unsigned_int_arg(arg)?), spec, None),
        b'o' => apply_literal_sprintf_padding(format!("{:o}", literal_unsigned_int_arg(arg)?), spec, None),
        b'u' => apply_literal_sprintf_padding(
            literal_unsigned_int_arg(arg)?.to_string(),
            spec,
            None,
        ),
        b'x' => apply_literal_sprintf_padding(format!("{:x}", literal_unsigned_int_arg(arg)?), spec, None),
        b'X' => apply_literal_sprintf_padding(format!("{:X}", literal_unsigned_int_arg(arg)?), spec, None),
        b'f' | b'F' => {
            let value = literal_numeric_arg(arg)?;
            let formatted = format!("{:.*}", spec.precision.unwrap_or(6), value);
            let sign_len = formatted
                .strip_prefix('-')
                .map(|_| 1)
                .or_else(|| spec.sign_plus.then_some(1))
                .unwrap_or(0);
            let formatted = add_positive_float_sign(formatted, spec);
            apply_literal_sprintf_padding(formatted, spec, Some(sign_len))
        }
        b'e' | b'E' => {
            let value = literal_numeric_arg(arg)?;
            let formatted = format_php_scientific(value, spec.precision.unwrap_or(6), spec.spec == b'E');
            let sign_len = formatted
                .strip_prefix('-')
                .map(|_| 1)
                .or_else(|| spec.sign_plus.then_some(1))
                .unwrap_or(0);
            let formatted = add_positive_float_sign(formatted, spec);
            apply_literal_sprintf_padding(formatted, spec, Some(sign_len))
        }
        b'g' | b'G' => {
            let value = literal_numeric_arg(arg)?;
            let formatted = format_php_general(value, spec.precision.unwrap_or(6), spec.spec == b'G');
            let sign_len = formatted
                .strip_prefix('-')
                .map(|_| 1)
                .or_else(|| spec.sign_plus.then_some(1))
                .unwrap_or(0);
            let formatted = add_positive_float_sign(formatted, spec);
            apply_literal_sprintf_padding(formatted, spec, Some(sign_len))
        }
        _ => {
            return Err(CompileError::new(
                call.span,
                "wasm32-web sprintf() literal format supports %s, %d, %c, %b, %o, %u, %x, %X, %f, %e, %E, %g, %G, and %%",
            ));
        }
    };
    Ok(formatted)
}

fn signed_decimal_parts(value: i64, spec: &LiteralSprintfSpec) -> (String, String) {
    if value < 0 {
        ("-".to_string(), value.unsigned_abs().to_string())
    } else if spec.sign_plus {
        ("+".to_string(), value.to_string())
    } else {
        (String::new(), value.to_string())
    }
}

fn add_positive_float_sign(mut formatted: String, spec: &LiteralSprintfSpec) -> String {
    if formatted.starts_with('-') {
        formatted
    } else if spec.sign_plus {
        formatted.insert(0, '+');
        formatted
    } else {
        formatted
    }
}

fn format_php_scientific(value: f64, precision: usize, uppercase: bool) -> String {
    let raw = if uppercase {
        format!("{:.*E}", precision, value)
    } else {
        format!("{:.*e}", precision, value)
    };
    let Some((mantissa, exponent)) = raw.split_once(if uppercase { 'E' } else { 'e' }) else {
        return raw;
    };
    let exp_value = exponent.parse::<i32>().unwrap_or(0);
    let exp_sign = if exp_value < 0 { '-' } else { '+' };
    format!(
        "{}{}{}{}",
        mantissa,
        if uppercase { 'E' } else { 'e' },
        exp_sign,
        exp_value.abs()
    )
}

pub(super) fn format_php_general(value: f64, precision: usize, uppercase: bool) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    let precision = precision.max(1);
    let scientific = if uppercase {
        format!("{:.*E}", precision - 1, value)
    } else {
        format!("{:.*e}", precision - 1, value)
    };
    let Some((mantissa, exponent)) = scientific.split_once(if uppercase { 'E' } else { 'e' }) else {
        return scientific;
    };
    let exp_value = exponent.parse::<i32>().unwrap_or(0);
    if exp_value < -4 || exp_value >= precision as i32 {
        let exp_sign = if exp_value < 0 { '-' } else { '+' };
        return format!(
            "{}{}{}{}",
            trim_float_fraction(mantissa, 1),
            if uppercase { 'E' } else { 'e' },
            exp_sign,
            exp_value.abs()
        );
    }
    let decimals = (precision as i32 - exp_value - 1).max(0) as usize;
    trim_float_fraction(&format!("{:.*}", decimals, value), 0)
}

fn trim_float_fraction(value: &str, min_fraction_digits: usize) -> String {
    let Some((whole, fraction)) = value.split_once('.') else {
        return value.to_string();
    };
    let mut trimmed = fraction.trim_end_matches('0').to_string();
    while trimmed.len() < min_fraction_digits {
        trimmed.push('0');
    }
    if trimmed.is_empty() {
        whole.to_string()
    } else {
        format!("{}.{}", whole, trimmed)
    }
}

fn apply_literal_sprintf_padding(
    formatted: String,
    spec: &LiteralSprintfSpec,
    sign_len: Option<usize>,
) -> String {
    let Some(width) = spec.width else {
        return formatted;
    };
    if formatted.len() >= width {
        return formatted;
    }
    let pad_len = width - formatted.len();
    let pad_char = if spec.zero_pad && !spec.left_align { '0' } else { ' ' };
    let padding: String = std::iter::repeat_n(pad_char, pad_len).collect();
    if spec.left_align {
        format!("{}{}", formatted, padding)
    } else if pad_char == '0' {
        if let Some(sign_len) = sign_len {
            if sign_len > 0 && sign_len <= formatted.len() {
                return format!("{}{}{}", &formatted[..sign_len], padding, &formatted[sign_len..]);
            }
        }
        format!("{}{}", padding, formatted)
    } else {
        format!("{}{}", padding, formatted)
    }
}

