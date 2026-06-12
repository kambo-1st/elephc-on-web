//! Purpose:
//! Emits wasm32-web printf/sprintf runtime formatting paths.
//! Keeps format parsing and runtime formatting dispatch out of the main expr module.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` string value and printf call paths.
//! - `crate::codegen::wasm::expr::string_builtins` for output `sprintf()`.
//!
//! Key details:
//! - Uses existing wasm string materializers from the parent module via `super::*`.
//! - Preserves fallback behavior for literal-only formatting.

use super::*;

use super::sprintf_runtime_helpers::*;
use super::sprintf_writers::*;
pub(super) fn emit_printf_call(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if emit_runtime_printf(call, args, module)? {
        return Ok(ValueKind::Int);
    }
    let value = eval_literal_sprintf(call, args)?;
    let byte_len = value.len();
    let (ptr, len) = module.intern_string(&value);
    module.body().line(&format!("i32.const {}", ptr));
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $host_write");
    module.body().line(&format!("i64.const {}", byte_len));
    Ok(ValueKind::Int)
}

#[derive(Clone, Copy)]
pub(super) struct RuntimeSprintfSpec {
    pub(super) spec: u8,
    pub(super) left_align: bool,
    pub(super) sign_plus: bool,
    pub(super) sign_space: bool,
    pub(super) zero_pad: bool,
    pub(super) width: Option<usize>,
    pub(super) precision: Option<usize>,
    pub(super) end: usize,
}

pub(super) fn parse_runtime_sprintf_spec(
    call: &Expr,
    bytes: &[u8],
    percent_index: usize,
) -> Result<Option<RuntimeSprintfSpec>, CompileError> {
    let mut index = percent_index + 1;
    if index >= bytes.len() {
        return Err(CompileError::new(
            call.span,
            "wasm32-web sprintf() literal format is incomplete",
        ));
    }
    let mut left_align = false;
    let mut sign_plus = false;
    let mut sign_space = false;
    let mut zero_pad = false;
    while index < bytes.len() {
        match bytes[index] {
            b'-' => left_align = true,
            b'+' => sign_plus = true,
            b' ' => sign_space = true,
            b'0' => zero_pad = true,
            _ => break,
        }
        index += 1;
    }
    let width_start = index;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    let width = if index > width_start {
        let digits = std::str::from_utf8(&bytes[width_start..index]).unwrap_or("0");
        Some(digits.parse::<usize>().unwrap_or(0))
    } else {
        None
    };
    if index >= bytes.len() {
        return Err(CompileError::new(
            call.span,
            "wasm32-web sprintf() literal format is incomplete",
        ));
    }
    if bytes[index] == b'.' {
        index += 1;
        let precision_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        let precision_end = index;
        if bytes.get(index) == Some(&b'l') {
            index += 1;
        }
        let Some(spec) = bytes.get(index).copied() else {
            return Err(CompileError::new(
                call.span,
                "wasm32-web sprintf() literal format is incomplete",
            ));
        };
        if !matches!(spec, b's' | b'f' | b'F' | b'e' | b'E' | b'g' | b'G') {
            return Ok(None);
        }
        let digits = std::str::from_utf8(&bytes[precision_start..precision_end]).unwrap_or("0");
        return Ok(Some(RuntimeSprintfSpec {
            spec,
            left_align,
            sign_plus,
            sign_space,
            zero_pad,
            width,
            precision: Some(digits.parse::<usize>().unwrap_or(0)),
            end: index + 1,
        }));
    }
    if bytes[index] == b'l' {
        index += 1;
        if index >= bytes.len() {
            return Err(CompileError::new(
                call.span,
                "wasm32-web sprintf() literal format is incomplete",
            ));
        }
    }
    let spec = bytes[index];
    if matches!(
        spec,
        b's' | b'd' | b'c' | b'b' | b'o' | b'u' | b'x' | b'X' | b'f' | b'F' | b'e' | b'E' | b'g' | b'G'
    ) {
        Ok(Some(RuntimeSprintfSpec {
            spec,
            left_align,
            sign_plus,
            sign_space,
            zero_pad,
            width,
            precision: None,
            end: index + 1,
        }))
    } else {
        Ok(None)
    }
}

pub(super) use super::sprintf_segments::*;

pub(super) fn emit_runtime_sprintf(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let Some(format_arg) = args.first() else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web sprintf() expects a static string format",
        ));
    };
    let format_is_tracked = static_string_value(format_arg, module).is_none();
    let format = static_or_tracked_ascii_string_arg(call, format_arg, module)?;
    let mut arg_index = 1usize;
    let mut chunk_start = 0usize;
    let bytes = format.as_bytes();
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
            index += 2;
            continue;
        }
        let Some(spec) = parse_runtime_sprintf_spec(call, bytes, index)? else {
            return Ok(false);
        };
        if spec.zero_pad
            && (!matches!(spec.spec, b'd' | b'f' | b'F' | b'e' | b'E' | b'g' | b'G')
                || spec.left_align
                || spec.width.is_none())
        {
            return Ok(false);
        }
        if (spec.sign_plus || spec.sign_space)
            && !matches!(spec.spec, b'd' | b'f' | b'F' | b'e' | b'E' | b'g' | b'G')
        {
            return Ok(false);
        }
        let Some(arg) = args.get(arg_index) else {
            return Err(CompileError::new(
                call.span,
                "wasm32-web sprintf() literal format has too few arguments",
            ));
        };
        match spec.spec {
            b's' => {
                if spec.precision.is_some() {
                    if !runtime_format_string_arg_supported(call, arg, module)?
                        && !string_cast_value_supported(arg, module)
                    {
                        return Ok(false);
                    }
                    saw_runtime_arg |= runtime_format_string_arg_is_runtime(arg, module);
                    saw_runtime_arg |= static_format_string_arg(call, arg, module).is_err();
                } else {
                    if !runtime_format_string_arg_supported(call, arg, module)?
                        && runtime_int_variable_arg(arg, module).is_none()
                        && runtime_float_variable_arg(arg, module).is_none()
                        && runtime_bool_variable_arg(arg, module).is_none()
                    {
                        return Ok(false);
                    }
                    saw_runtime_arg |= runtime_format_string_arg_is_runtime(arg, module);
                    saw_runtime_arg |= runtime_int_variable_arg(arg, module).is_some();
                    saw_runtime_arg |= runtime_float_variable_arg(arg, module).is_some();
                    saw_runtime_arg |= runtime_bool_variable_arg(arg, module).is_some();
                }
            }
            b'd' => {
                if runtime_int_variable_arg(arg, module).is_none()
                    && runtime_float_variable_arg(arg, module).is_none()
                    && runtime_bool_variable_arg(arg, module).is_none()
                    && literal_format_int_arg(arg).is_err()
                {
                    return Ok(false);
                }
                saw_runtime_arg |= runtime_int_variable_arg(arg, module).is_some();
                saw_runtime_arg |= runtime_float_variable_arg(arg, module).is_some();
                saw_runtime_arg |= runtime_bool_variable_arg(arg, module).is_some();
            }
            b'c' => {
                if runtime_int_variable_arg(arg, module).is_none()
                    && runtime_bool_variable_arg(arg, module).is_none()
                    && runtime_float_variable_arg(arg, module).is_none()
                    && literal_format_int_arg(arg).is_err()
                {
                    return Ok(false);
                }
                saw_runtime_arg |= runtime_int_variable_arg(arg, module).is_some();
                saw_runtime_arg |= runtime_bool_variable_arg(arg, module).is_some();
                saw_runtime_arg |= runtime_float_variable_arg(arg, module).is_some();
            }
            b'b' | b'o' | b'u' | b'x' | b'X' => {
                if runtime_int_variable_arg(arg, module).is_none()
                    && runtime_bool_variable_arg(arg, module).is_none()
                    && runtime_float_variable_arg(arg, module).is_none()
                    && literal_format_int_arg(arg).is_err()
                {
                    return Ok(false);
                }
                saw_runtime_arg |= runtime_int_variable_arg(arg, module).is_some();
                saw_runtime_arg |= runtime_bool_variable_arg(arg, module).is_some();
                saw_runtime_arg |= runtime_float_variable_arg(arg, module).is_some();
            }
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' => {
                if runtime_float_variable_arg(arg, module).is_none()
                    && runtime_int_variable_arg(arg, module).is_none()
                    && runtime_bool_variable_arg(arg, module).is_none()
                    && literal_numeric_arg(arg).is_err()
                {
                    return Ok(false);
                }
                saw_runtime_arg |= runtime_float_variable_arg(arg, module).is_some();
                saw_runtime_arg |= runtime_int_variable_arg(arg, module).is_some();
                saw_runtime_arg |= runtime_bool_variable_arg(arg, module).is_some();
            }
            _ => return Ok(false),
        }
        arg_index += 1;
        index = spec.end;
    }
    if arg_index != args.len() {
        return Err(CompileError::new(
            call.span,
            "wasm32-web sprintf() literal format has unused arguments",
        ));
    }
    if !saw_runtime_arg && !format_is_tracked {
        return Ok(false);
    }

    arg_index = 1;
    index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            index += 1;
            continue;
        }
        if bytes[index + 1] == b'%' {
            emit_write_literal_text(&format[chunk_start..index], module);
            emit_write_literal_text("%", module);
            index += 2;
            chunk_start = index;
            continue;
        }
        let spec = parse_runtime_sprintf_spec(call, bytes, index)?
            .expect("sprintf format was validated before emission");
        emit_write_literal_text(&format[chunk_start..index], module);
        let arg = &args[arg_index];
        match spec.spec {
            b's' => {
                if let Some(var) = runtime_string_arg_or_materialize(arg, "sprintf_string_arg", module)? {
                    emit_write_sprintf_string_var(&var, spec.precision, spec.width, spec.left_align, None, module);
                } else if (spec.precision.is_some() || spec.width.is_some())
                    && string_cast_value_supported(arg, module)
                {
                    let var = materialize_string_cast_expr(arg, "sprintf_string_arg", module)?;
                    emit_write_sprintf_string_var(&var, spec.precision, spec.width, spec.left_align, None, module);
                } else if let Some(var) = runtime_int_variable_arg(arg, module) {
                    module.body().line(&format!("local.get ${}", var));
                    module.body().line("call $host_write_int");
                } else if let Some(var) = runtime_float_variable_arg(arg, module) {
                    emit_write_float_string_var(var, None, module);
                } else if let Some(var) = runtime_bool_variable_arg(arg, module) {
                    emit_write_bool_string_var(var, None, module);
                } else {
                    let value = static_format_string_arg(call, arg, module)?;
                    emit_write_literal_text(
                        &format_sprintf_static_string_arg(&value, spec.precision, spec.width, spec.left_align),
                        module,
                    );
                }
            }
            b'd' => {
                if spec.width.is_some() || spec.left_align || spec.sign_plus || spec.sign_space {
                    emit_write_sprintf_int_string_arg(
                        arg,
                        "sprintf_int_arg",
                        runtime_sprintf_sign_prefix(spec),
                        spec.zero_pad,
                        spec.width,
                        spec.left_align,
                        None,
                        module,
                    )?;
                    arg_index += 1;
                    index = spec.end;
                    chunk_start = index;
                    continue;
                }
                if let Some(var) = runtime_int_variable_arg(arg, module) {
                    module.body().line(&format!("local.get ${}", var));
                } else if let Some(var) = runtime_float_variable_arg(arg, module) {
                    module.body().line(&format!("local.get ${}", var));
                    module.body().line("i64.trunc_f64_s");
                } else if let Some(var) = runtime_bool_variable_arg(arg, module) {
                    module.body().line(&format!("local.get ${}", var));
                    module.body().line("i64.extend_i32_s");
                } else {
                    module.body().line(&format!("i64.const {}", literal_format_int_arg(arg)?));
                }
                module.body().line("call $host_write_int");
            }
            b'c' => {
                emit_sprintf_char_arg(arg, None, module)?;
            }
            b'b' => emit_sprintf_radix_or_padded_arg(arg, 2, false, spec.width, spec.left_align, None, module)?,
            b'o' => emit_sprintf_radix_or_padded_arg(arg, 8, false, spec.width, spec.left_align, None, module)?,
            b'u' => emit_sprintf_radix_or_padded_arg(arg, 10, false, spec.width, spec.left_align, None, module)?,
            b'x' => emit_sprintf_radix_or_padded_arg(arg, 16, false, spec.width, spec.left_align, None, module)?,
            b'X' => emit_sprintf_radix_or_padded_arg(arg, 16, true, spec.width, spec.left_align, None, module)?,
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' => {
                emit_write_sprintf_float_string_arg(
                    arg,
                    "sprintf_float_arg",
                    spec.spec,
                    spec.precision.unwrap_or(6),
                    runtime_sprintf_sign_prefix(spec),
                    spec.zero_pad,
                    spec.width,
                    spec.left_align,
                    None,
                    module,
                )?;
            }
            _ => unreachable!("sprintf format was validated before emission"),
        }
        arg_index += 1;
        index = spec.end;
        chunk_start = index;
    }
    emit_write_literal_text(&format[chunk_start..], module);
    Ok(true)
}

pub(super) fn sprintf_runtime_string_args_supported(
    call: &Expr,
    format: &str,
    args: &[Expr],
    module: &WasmModule,
    force_emit: bool,
) -> Result<bool, CompileError> {
    let mut arg_index = 1usize;
    let bytes = format.as_bytes();
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
            index += 2;
            continue;
        }
        let Some(spec) = parse_runtime_sprintf_spec(call, bytes, index)? else {
            return Ok(false);
        };
        if spec.zero_pad
            && (!matches!(spec.spec, b'd' | b'f' | b'F' | b'e' | b'E' | b'g' | b'G')
                || spec.left_align
                || spec.width.is_none())
        {
            return Ok(false);
        }
        if (spec.sign_plus || spec.sign_space)
            && !matches!(spec.spec, b'd' | b'f' | b'F' | b'e' | b'E' | b'g' | b'G')
        {
            return Ok(false);
        }
        let Some(arg) = args.get(arg_index) else {
            return Err(CompileError::new(
                call.span,
                "wasm32-web sprintf() literal format has too few arguments",
            ));
        };
        match spec.spec {
            b's' => {
                if spec.precision.is_some() {
                    if !runtime_format_string_arg_supported(call, arg, module)?
                        && !string_cast_value_supported(arg, module)
                    {
                        return Ok(false);
                    }
                    saw_runtime_arg |= runtime_format_string_arg_is_runtime(arg, module);
                    saw_runtime_arg |= static_format_string_arg(call, arg, module).is_err();
                } else {
                    if !runtime_format_string_arg_supported(call, arg, module)?
                        && runtime_int_variable_arg(arg, module).is_none()
                        && runtime_float_variable_arg(arg, module).is_none()
                        && runtime_bool_variable_arg(arg, module).is_none()
                    {
                        return Ok(false);
                    }
                    saw_runtime_arg |= runtime_format_string_arg_is_runtime(arg, module);
                    saw_runtime_arg |= runtime_int_variable_arg(arg, module).is_some();
                    saw_runtime_arg |= runtime_float_variable_arg(arg, module).is_some();
                    saw_runtime_arg |= runtime_bool_variable_arg(arg, module).is_some();
                }
            }
            b'd' => {
                if runtime_int_variable_arg(arg, module).is_none()
                    && runtime_float_variable_arg(arg, module).is_none()
                    && runtime_bool_variable_arg(arg, module).is_none()
                    && literal_format_int_arg(arg).is_err()
                {
                    return Ok(false);
                }
                saw_runtime_arg |= runtime_int_variable_arg(arg, module).is_some();
                saw_runtime_arg |= runtime_float_variable_arg(arg, module).is_some();
                saw_runtime_arg |= runtime_bool_variable_arg(arg, module).is_some();
            }
            b'c' => {
                if runtime_int_variable_arg(arg, module).is_none()
                    && runtime_bool_variable_arg(arg, module).is_none()
                    && runtime_float_variable_arg(arg, module).is_none()
                    && literal_format_int_arg(arg).is_err()
                {
                    return Ok(false);
                }
                saw_runtime_arg |= runtime_int_variable_arg(arg, module).is_some();
                saw_runtime_arg |= runtime_bool_variable_arg(arg, module).is_some();
                saw_runtime_arg |= runtime_float_variable_arg(arg, module).is_some();
            }
            b'b' | b'o' | b'u' | b'x' | b'X' => {
                if runtime_int_variable_arg(arg, module).is_none()
                    && runtime_bool_variable_arg(arg, module).is_none()
                    && runtime_float_variable_arg(arg, module).is_none()
                    && literal_format_int_arg(arg).is_err()
                {
                    return Ok(false);
                }
                saw_runtime_arg |= runtime_int_variable_arg(arg, module).is_some();
                saw_runtime_arg |= runtime_bool_variable_arg(arg, module).is_some();
                saw_runtime_arg |= runtime_float_variable_arg(arg, module).is_some();
            }
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' => {
                if runtime_float_variable_arg(arg, module).is_none()
                    && runtime_int_variable_arg(arg, module).is_none()
                    && runtime_bool_variable_arg(arg, module).is_none()
                    && literal_numeric_arg(arg).is_err()
                {
                    return Ok(false);
                }
                saw_runtime_arg |= runtime_float_variable_arg(arg, module).is_some();
                saw_runtime_arg |= runtime_int_variable_arg(arg, module).is_some();
                saw_runtime_arg |= runtime_bool_variable_arg(arg, module).is_some();
            }
            _ => return Ok(false),
        }
        arg_index += 1;
        index = spec.end;
    }
    if arg_index != args.len() {
        return Err(CompileError::new(
            call.span,
            "wasm32-web sprintf() literal format has unused arguments",
        ));
    }
    Ok(saw_runtime_arg || force_emit)
}

pub(super) use super::sprintf_output::*;
