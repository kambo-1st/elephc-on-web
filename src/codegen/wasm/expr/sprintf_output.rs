//! Purpose:
//! Emits wasm32-web runtime printf/sprintf output loops.
//! Keeps direct host-write formatting separate from format validation and value planning.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::sprintf` through its re-export.
//! - `crate::codegen::wasm::expr::sprintf::emit_printf_call`.
//!
//! Key details:
//! - Streams formatted parts directly to host writes.
//! - Optional byte-length accounting is used by `printf()` return-value lowering.

use super::*;
use super::sprintf::{parse_runtime_sprintf_spec, sprintf_runtime_string_args_supported};
use super::sprintf_runtime_helpers::*;
use super::sprintf_writers::*;

pub(super) fn emit_runtime_sprintf_parts(
    call: &Expr,
    format: &str,
    args: &[Expr],
    module: &mut WasmModule,
    byte_len: Option<&str>,
) -> Result<(), CompileError> {
    let mut arg_index = 1usize;
    let mut chunk_start = 0usize;
    let bytes = format.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            index += 1;
            continue;
        }
        if bytes[index + 1] == b'%' {
            emit_write_sprintf_literal(&format[chunk_start..index], byte_len, module);
            emit_write_sprintf_literal("%", byte_len, module);
            index += 2;
            chunk_start = index;
            continue;
        }
        let spec = parse_runtime_sprintf_spec(call, bytes, index)?
            .expect("sprintf format was validated before emission");
        emit_write_sprintf_literal(&format[chunk_start..index], byte_len, module);
        let arg = &args[arg_index];
        match spec.spec {
            b's' => {
                if let Some(var) = runtime_string_arg_or_materialize(arg, "printf_string_arg", module)? {
                    emit_write_sprintf_string_var(&var, spec.precision, spec.width, spec.left_align, byte_len, module);
                } else if (spec.precision.is_some() || spec.width.is_some())
                    && string_cast_value_supported(arg, module)
                {
                    let var = materialize_string_cast_expr(arg, "printf_string_arg", module)?;
                    emit_write_sprintf_string_var(&var, spec.precision, spec.width, spec.left_align, byte_len, module);
                } else if let Some(var) = runtime_int_variable_arg(arg, module) {
                    module.body().line(&format!("local.get ${}", var));
                    module.body().line("call $host_write_int");
                    if let Some(byte_len) = byte_len {
                        emit_add_runtime_int_byte_len(var, byte_len, module);
                    }
                } else if let Some(var) = runtime_float_variable_arg(arg, module) {
                    emit_write_float_string_var(var, byte_len, module);
                } else if let Some(var) = runtime_bool_variable_arg(arg, module) {
                    emit_write_bool_string_var(var, byte_len, module);
                } else {
                    let value = static_format_string_arg(call, arg, module)?;
                    let formatted =
                        format_sprintf_static_string_arg(&value, spec.precision, spec.width, spec.left_align);
                    emit_write_sprintf_literal(
                        &formatted,
                        byte_len,
                        module,
                    );
                }
            }
            b'd' => {
                if spec.width.is_some() || spec.left_align || spec.sign_plus || spec.sign_space {
                    emit_write_sprintf_int_string_arg(
                        arg,
                        "printf_int_arg",
                        runtime_sprintf_sign_prefix(spec),
                        spec.zero_pad,
                        spec.width,
                        spec.left_align,
                        byte_len,
                        module,
                    )?;
                    arg_index += 1;
                    index = spec.end;
                    chunk_start = index;
                    continue;
                }
                if let Some(var) = runtime_int_variable_arg(arg, module) {
                    module.body().line(&format!("local.get ${}", var));
                    module.body().line("call $host_write_int");
                    if let Some(byte_len) = byte_len {
                        emit_add_runtime_int_byte_len(var, byte_len, module);
                    }
                } else if let Some(var) = runtime_float_variable_arg(arg, module) {
                    let coerced = module.next_label("printf_float_int");
                    let coerced_name = coerced.trim_start_matches('$').to_string();
                    module.declare_i64_local(coerced_name.clone());
                    module.body().line(&format!("local.get ${}", var));
                    module.body().line("i64.trunc_f64_s");
                    module.body().line(&format!("local.set ${}", coerced_name));
                    module.body().line(&format!("local.get ${}", coerced_name));
                    module.body().line("call $host_write_int");
                    if let Some(byte_len) = byte_len {
                        emit_add_runtime_int_byte_len(&coerced_name, byte_len, module);
                    }
                } else if let Some(var) = runtime_bool_variable_arg(arg, module) {
                    module.body().line(&format!("local.get ${}", var));
                    module.body().line("i64.extend_i32_s");
                    module.body().line("call $host_write_int");
                    if let Some(byte_len) = byte_len {
                        module.body().line(&format!("local.get {}", byte_len));
                        module.body().line("i64.const 1");
                        module.body().line("i64.add");
                        module.body().line(&format!("local.set {}", byte_len));
                    }
                } else {
                    let value = literal_format_int_arg(arg)?;
                    module.body().line(&format!("i64.const {}", value));
                    module.body().line("call $host_write_int");
                    if let Some(byte_len) = byte_len {
                        module.body().line(&format!("local.get {}", byte_len));
                        module.body().line(&format!("i64.const {}", value.to_string().len()));
                        module.body().line("i64.add");
                        module.body().line(&format!("local.set {}", byte_len));
                    }
                }
            }
            b'c' => {
                emit_sprintf_char_arg(arg, byte_len, module)?;
            }
            b'b' => emit_sprintf_radix_or_padded_arg(arg, 2, false, spec.width, spec.left_align, byte_len, module)?,
            b'o' => emit_sprintf_radix_or_padded_arg(arg, 8, false, spec.width, spec.left_align, byte_len, module)?,
            b'u' => emit_sprintf_radix_or_padded_arg(arg, 10, false, spec.width, spec.left_align, byte_len, module)?,
            b'x' => emit_sprintf_radix_or_padded_arg(arg, 16, false, spec.width, spec.left_align, byte_len, module)?,
            b'X' => emit_sprintf_radix_or_padded_arg(arg, 16, true, spec.width, spec.left_align, byte_len, module)?,
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' => {
                emit_write_sprintf_float_string_arg(
                    arg,
                    "printf_float_arg",
                    spec.spec,
                    spec.precision.unwrap_or(6),
                    runtime_sprintf_sign_prefix(spec),
                    spec.zero_pad,
                    spec.width,
                    spec.left_align,
                    byte_len,
                    module,
                )?;
            }
            _ => unreachable!("sprintf format was validated before emission"),
        }
        arg_index += 1;
        index = spec.end;
        chunk_start = index;
    }
    emit_write_sprintf_literal(&format[chunk_start..], byte_len, module);
    Ok(())
}

pub(super) fn emit_runtime_printf(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let Some(format_arg) = args.first() else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web printf() expects a static string format",
        ));
    };
    let format_is_tracked = static_string_value(format_arg, module).is_none();
    let format = static_or_tracked_ascii_string_arg(call, format_arg, module)?;
    if !sprintf_runtime_string_args_supported(call, &format, args, module, format_is_tracked)? {
        return Ok(false);
    }
    let byte_len = module.next_label("printf_len");
    module.declare_i64_local(byte_len.trim_start_matches('$').to_string());
    module.body().line("i64.const 0");
    module.body().line(&format!("local.set {}", byte_len));
    emit_runtime_sprintf_parts(call, &format, args, module, Some(&byte_len))?;
    module.body().line(&format!("local.get {}", byte_len));
    Ok(true)
}

#[allow(dead_code)]
fn emit_runtime_sprintf_parts_old(
    call: &Expr,
    format: &str,
    args: &[Expr],
    module: &mut WasmModule,
    byte_len: Option<&str>,
) -> Result<(), CompileError> {
    let mut arg_index = 1usize;
    let mut chunk_start = 0usize;
    let bytes = format.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            index += 1;
            continue;
        }
        if bytes[index + 1] == b'%' {
            emit_write_sprintf_literal(&format[chunk_start..index], byte_len, module);
            emit_write_sprintf_literal("%", byte_len, module);
            index += 2;
            chunk_start = index;
            continue;
        }
        let spec = parse_runtime_sprintf_spec(call, bytes, index)?
            .expect("sprintf format was validated before emission");
        emit_write_sprintf_literal(&format[chunk_start..index], byte_len, module);
        let arg = &args[arg_index];
        match spec.spec {
            b's' => {
                if let Some(var) = runtime_string_arg_or_materialize(arg, "printf_string_arg", module)? {
                    emit_write_sprintf_string_var(&var, spec.precision, spec.width, spec.left_align, byte_len, module);
                } else if (spec.precision.is_some() || spec.width.is_some())
                    && string_cast_value_supported(arg, module)
                {
                    let var = materialize_string_cast_expr(arg, "printf_string_arg", module)?;
                    emit_write_sprintf_string_var(&var, spec.precision, spec.width, spec.left_align, byte_len, module);
                } else if let Some(var) = runtime_int_variable_arg(arg, module) {
                    module.body().line(&format!("local.get ${}", var));
                    module.body().line("call $host_write_int");
                    if let Some(byte_len) = byte_len {
                        emit_add_runtime_int_byte_len(var, byte_len, module);
                    }
                } else if let Some(var) = runtime_float_variable_arg(arg, module) {
                    emit_write_float_string_var(var, byte_len, module);
                } else if let Some(var) = runtime_bool_variable_arg(arg, module) {
                    emit_write_bool_string_var(var, byte_len, module);
                } else {
                    let value = static_format_string_arg(call, arg, module)?;
                    let formatted =
                        format_sprintf_static_string_arg(&value, spec.precision, spec.width, spec.left_align);
                    emit_write_sprintf_literal(
                        &formatted,
                        byte_len,
                        module,
                    );
                }
            }
            b'd' => {
                if spec.width.is_some() || spec.left_align || spec.sign_plus || spec.sign_space {
                    emit_write_sprintf_int_string_arg(
                        arg,
                        "printf_int_arg",
                        runtime_sprintf_sign_prefix(spec),
                        spec.zero_pad,
                        spec.width,
                        spec.left_align,
                        byte_len,
                        module,
                    )?;
                    arg_index += 1;
                    index = spec.end;
                    chunk_start = index;
                    continue;
                }
                if let Some(var) = runtime_int_variable_arg(arg, module) {
                    module.body().line(&format!("local.get ${}", var));
                    module.body().line("call $host_write_int");
                    if let Some(byte_len) = byte_len {
                        emit_add_runtime_int_byte_len(var, byte_len, module);
                    }
                } else if let Some(var) = runtime_float_variable_arg(arg, module) {
                    let coerced = module.next_label("printf_float_int");
                    let coerced_name = coerced.trim_start_matches('$').to_string();
                    module.declare_i64_local(coerced_name.clone());
                    module.body().line(&format!("local.get ${}", var));
                    module.body().line("i64.trunc_f64_s");
                    module.body().line(&format!("local.set ${}", coerced_name));
                    module.body().line(&format!("local.get ${}", coerced_name));
                    module.body().line("call $host_write_int");
                    if let Some(byte_len) = byte_len {
                        emit_add_runtime_int_byte_len(&coerced_name, byte_len, module);
                    }
                } else if let Some(var) = runtime_bool_variable_arg(arg, module) {
                    module.body().line(&format!("local.get ${}", var));
                    module.body().line("i64.extend_i32_s");
                    module.body().line("call $host_write_int");
                    if let Some(byte_len) = byte_len {
                        module.body().line(&format!("local.get {}", byte_len));
                        module.body().line("i64.const 1");
                        module.body().line("i64.add");
                        module.body().line(&format!("local.set {}", byte_len));
                    }
                } else {
                    let value = literal_format_int_arg(arg)?;
                    module.body().line(&format!("i64.const {}", value));
                    module.body().line("call $host_write_int");
                    if let Some(byte_len) = byte_len {
                        module.body().line(&format!("local.get {}", byte_len));
                        module.body().line(&format!("i64.const {}", value.to_string().len()));
                        module.body().line("i64.add");
                        module.body().line(&format!("local.set {}", byte_len));
                    }
                }
            }
            b'c' => {
                emit_sprintf_char_arg(arg, byte_len, module)?;
            }
            b'b' => emit_sprintf_radix_or_padded_arg(arg, 2, false, spec.width, spec.left_align, byte_len, module)?,
            b'o' => emit_sprintf_radix_or_padded_arg(arg, 8, false, spec.width, spec.left_align, byte_len, module)?,
            b'u' => emit_sprintf_radix_or_padded_arg(arg, 10, false, spec.width, spec.left_align, byte_len, module)?,
            b'x' => emit_sprintf_radix_or_padded_arg(arg, 16, false, spec.width, spec.left_align, byte_len, module)?,
            b'X' => emit_sprintf_radix_or_padded_arg(arg, 16, true, spec.width, spec.left_align, byte_len, module)?,
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' => {
                let var = materialize_sprintf_float_string_expr(
                    arg,
                    "printf_float_arg",
                    spec.spec,
                    spec.precision.unwrap_or(6),
                    module,
                )?;
                emit_write_sprintf_string_var(&var, None, spec.width, spec.left_align, byte_len, module);
            }
            _ => unreachable!("sprintf format was validated before emission"),
        }
        arg_index += 1;
        index = spec.end;
        chunk_start = index;
    }
    emit_write_sprintf_literal(&format[chunk_start..], byte_len, module);
    Ok(())
}
