//! Purpose:
//! Provides runtime printf()/sprintf() argument validation and write helpers.
//! Keeps reusable dynamic formatting helpers separate from the main formatting loops.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::sprintf`
//!
//! Key details:
//! - Helpers preserve runtime byte-length accounting for printf() return values.

use super::*;
use super::sprintf::RuntimeSprintfSpec;
use super::sprintf_writers::*;

pub(super) fn runtime_format_string_arg_supported(
    call: &Expr,
    arg: &Expr,
    module: &WasmModule,
) -> Result<bool, CompileError> {
    if let Some(err) = unsupported_string_coercion_expr(arg) {
        return Err(err);
    }
    Ok(runtime_format_string_arg_is_runtime(arg, module)
        || static_format_string_arg(call, arg, module).is_ok())
}

pub(super) fn runtime_format_string_arg_is_runtime(arg: &Expr, module: &WasmModule) -> bool {
    runtime_string_variable_arg(arg, module).is_some()
        || (expression_is_stringy(arg, module) && static_string_value(arg, module).is_none())
}

pub(super) fn emit_write_sprintf_literal(text: &str, byte_len: Option<&str>, module: &mut WasmModule) {
    emit_write_literal_text(text, module);
    if let Some(byte_len) = byte_len {
        module.body().line(&format!("local.get {}", byte_len));
        module.body().line(&format!("i64.const {}", text.len()));
        module.body().line("i64.add");
        module.body().line(&format!("local.set {}", byte_len));
    }
}

pub(super) fn emit_write_sprintf_int_string_arg(
    arg: &Expr,
    prefix: &str,
    sign_prefix: Option<u8>,
    zero_pad: bool,
    width: Option<usize>,
    left_align: bool,
    byte_len: Option<&str>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let mut var = materialize_int_string_expr(arg, prefix, module)?;
    if let Some(sign_prefix) = sign_prefix {
        var = materialize_sprintf_prefixed_positive_number(&var, sign_prefix, module);
    }
    if zero_pad && !left_align {
        if let Some(width) = width {
            var = materialize_sprintf_zero_padded_number(&var, width, module);
        }
        emit_write_sprintf_string_var(&var, None, None, false, byte_len, module);
    } else {
        emit_write_sprintf_string_var(&var, None, width, left_align, byte_len, module);
    }
    Ok(())
}

pub(super) fn emit_write_sprintf_float_string_arg(
    arg: &Expr,
    prefix: &str,
    spec: u8,
    precision: usize,
    sign_prefix: Option<u8>,
    zero_pad: bool,
    width: Option<usize>,
    left_align: bool,
    byte_len: Option<&str>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let mut var = materialize_sprintf_float_string_expr(arg, prefix, spec, precision, module)?;
    if let Some(sign_prefix) = sign_prefix {
        var = materialize_sprintf_prefixed_positive_number(&var, sign_prefix, module);
    }
    if zero_pad && !left_align {
        if let Some(width) = width {
            var = materialize_sprintf_zero_padded_number(&var, width, module);
        }
        emit_write_sprintf_string_var(&var, None, None, false, byte_len, module);
    } else {
        emit_write_sprintf_string_var(&var, None, width, left_align, byte_len, module);
    }
    Ok(())
}

pub(super) fn runtime_sprintf_sign_prefix(spec: RuntimeSprintfSpec) -> Option<u8> {
    if spec.sign_plus {
        Some(b'+')
    } else if spec.sign_space && spec.width.is_some() && !spec.zero_pad {
        Some(b' ')
    } else {
        None
    }
}
