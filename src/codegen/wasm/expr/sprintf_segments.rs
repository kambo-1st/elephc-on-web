//! Purpose:
//! Plans wasm32-web string-value segments for runtime `sprintf()` materialization.
//! Keeps value-building segment analysis separate from printf/sprintf output emission.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::sprintf` through its re-export.
//! - `crate::codegen::wasm::expr::string_values` for materialized `sprintf()` values.
//!
//! Key details:
//! - Returns `None` when a format/argument combination should use literal fallback.
//! - Materializes runtime arguments once while preserving width, precision, and alignment metadata.

use super::*;
use super::sprintf::parse_runtime_sprintf_spec;
use super::sprintf_runtime_helpers::{
    runtime_format_string_arg_is_runtime, runtime_sprintf_sign_prefix,
};
use super::sprintf_writers::{
    materialize_sprintf_prefixed_positive_number, materialize_sprintf_zero_padded_number,
};

pub(super) enum SprintfStringSegment {
    Literal(String),
    Variable {
        var: String,
        precision: Option<usize>,
        width: Option<usize>,
        left_align: bool,
    },
}

pub(super) fn sprintf_string_value_segments(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<Option<Vec<SprintfStringSegment>>, CompileError> {
    let Some(format_arg) = args.first() else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web sprintf() expects at least one argument",
        ));
    };
    let format_is_tracked = static_string_value(format_arg, module).is_none();
    let format = static_or_tracked_ascii_string_arg(call, format_arg, module)?;
    let bytes = format.as_bytes();
    let mut segments = Vec::new();
    let mut arg_index = 1usize;
    let mut chunk_start = 0usize;
    let mut index = 0usize;
    let mut saw_runtime_arg = false;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            index += 1;
            continue;
        }
        if index + 1 >= bytes.len() {
            return Err(CompileError::new(
                call.span,
                "wasm32-web sprintf() literal format is incomplete",
            ));
        }
        if bytes[index + 1] == b'%' {
            if chunk_start < index {
                segments.push(SprintfStringSegment::Literal(format[chunk_start..index].to_string()));
            }
            segments.push(SprintfStringSegment::Literal("%".to_string()));
            index += 2;
            chunk_start = index;
            continue;
        }
        let Some(spec) = parse_runtime_sprintf_spec(call, bytes, index)? else {
            return Ok(None);
        };
        if spec.zero_pad
            && (!matches!(spec.spec, b'd' | b'f' | b'F' | b'e' | b'E' | b'g' | b'G')
                || spec.left_align
                || spec.width.is_none())
        {
            return Ok(None);
        }
        if (spec.sign_plus || spec.sign_space)
            && !matches!(spec.spec, b'd' | b'f' | b'F' | b'e' | b'E' | b'g' | b'G')
        {
            return Ok(None);
        }
        if chunk_start < index {
            segments.push(SprintfStringSegment::Literal(format[chunk_start..index].to_string()));
        }
        let Some(arg) = args.get(arg_index) else {
            return Err(CompileError::new(
                call.span,
                "wasm32-web sprintf() literal format has too few arguments",
            ));
        };
        let var = if spec.spec == b's' {
            if let Some(var) = string_arg_or_materialize(arg, "sprintf_value_arg", module)? {
                saw_runtime_arg |= runtime_format_string_arg_is_runtime(arg, module);
                var
            } else if string_cast_value_supported(arg, module) {
                saw_runtime_arg |= !matches!(
                    &arg.kind,
                    ExprKind::StringLiteral(_)
                        | ExprKind::IntLiteral(_)
                        | ExprKind::FloatLiteral(_)
                        | ExprKind::BoolLiteral(_)
                        | ExprKind::Null
                );
                materialize_string_cast_expr(arg, "sprintf_value_scalar", module)?
            } else {
                return Ok(None);
            }
        } else if spec.spec == b'd' {
            if !expression_is_inty(arg, module)
                && !expression_is_booly(arg, module)
                && !expression_is_floaty(arg, module)
                && literal_format_int_arg(arg).is_err()
            {
                return Ok(None);
            }
            saw_runtime_arg |= static_int_value(arg).is_none() && literal_format_int_arg(arg).is_err();
            let var = materialize_int_string_expr(arg, "sprintf_value_int", module)?;
            let var = if let Some(prefix) = runtime_sprintf_sign_prefix(spec) {
                materialize_sprintf_prefixed_positive_number(&var, prefix, module)
            } else {
                var
            };
            if spec.zero_pad {
                materialize_sprintf_zero_padded_number(&var, spec.width.unwrap_or(0), module)
            } else {
                var
            }
        } else if spec.spec == b'c' {
            if literal_format_int_arg(arg).is_err()
                && !expression_is_inty(arg, module)
                && !expression_is_booly(arg, module)
                && !expression_is_floaty(arg, module)
            {
                return Ok(None);
            }
            saw_runtime_arg |= literal_format_int_arg(arg).is_err();
            materialize_char_string_expr(arg, "sprintf_value_char", module)?
        } else if matches!(spec.spec, b'f' | b'F' | b'e' | b'E' | b'g' | b'G') {
            if !expression_is_floaty(arg, module)
                && !expression_is_inty(arg, module)
                && !expression_is_booly(arg, module)
                && literal_numeric_arg(arg).is_err()
            {
                return Ok(None);
            }
            saw_runtime_arg |= literal_numeric_arg(arg).is_err();
            let var = materialize_sprintf_float_string_expr(
                arg,
                "sprintf_value_float",
                spec.spec,
                spec.precision.unwrap_or(6),
                module,
            )?;
            let var = if let Some(prefix) = runtime_sprintf_sign_prefix(spec) {
                materialize_sprintf_prefixed_positive_number(&var, prefix, module)
            } else {
                var
            };
            if spec.zero_pad {
                materialize_sprintf_zero_padded_number(&var, spec.width.unwrap_or(0), module)
            } else {
                var
            }
        } else {
            if literal_format_int_arg(arg).is_err()
                && !expression_is_inty(arg, module)
                && !expression_is_booly(arg, module)
                && !expression_is_floaty(arg, module)
            {
                return Ok(None);
            }
            saw_runtime_arg |= literal_format_int_arg(arg).is_err();
            let (radix, uppercase) = match spec.spec {
                b'b' => (2, false),
                b'o' => (8, false),
                b'u' => (10, false),
                b'x' => (16, false),
                b'X' => (16, true),
                _ => unreachable!("sprintf radix format was validated"),
            };
            materialize_unsigned_radix_string_expr(
                arg,
                radix,
                uppercase,
                "sprintf_value_radix",
                module,
            )?
        };
        let (precision, width, left_align) = if !spec.zero_pad
            && spec.spec != b'c'
            && (spec.width.is_some() || spec.left_align || spec.spec == b's')
        {
            let precision = if spec.spec == b's' { spec.precision } else { None };
            (precision, spec.width, spec.left_align)
        } else {
            (None, None, false)
        };
        segments.push(SprintfStringSegment::Variable {
            var,
            precision,
            width,
            left_align,
        });
        arg_index += 1;
        index = spec.end;
        chunk_start = index;
    }
    if chunk_start < format.len() {
        segments.push(SprintfStringSegment::Literal(format[chunk_start..].to_string()));
    }
    if arg_index != args.len() {
        return Err(CompileError::new(
            call.span,
            "wasm32-web sprintf() literal format has unused arguments",
        ));
    }
    Ok((saw_runtime_arg || format_is_tracked).then_some(segments))
}
