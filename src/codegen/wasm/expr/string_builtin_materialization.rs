//! Purpose:
//! Emits string-returning builtin calls as wasm32-web `(ptr, len)` stack values.
//! Keeps the long builtin materialization chain out of general string assignment/value lowering.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::string_materialization`
//!
//! Key details:
//! - Returns `true` only when it emitted a string value on the stack.
//! - Preserves json last-error side effects for folded/static `json_encode` paths.

use super::*;

pub(in crate::codegen::wasm) fn emit_string_builtin_value_to_stack_if_supported(
    value: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !is_output_string_builtin(name) {
        return Ok(false);
    }
    if emit_same_len_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_trim_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_substr_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_str_repeat_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_str_pad_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_substr_replace_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_chr_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_escape_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_bin2hex_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_hex2bin_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_base64_encode_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_base64_decode_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_urlencode_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_urldecode_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_str_replace_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_strstr_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_html_escape_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_wordwrap_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_html_entity_decode_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_hash_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_implode_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_number_format_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if emit_sprintf_string_builtin_value_to_stack(value, name, args, module)? {
        return Ok(true);
    }
    if name.eq_ignore_ascii_case("json_last_error_msg") {
        emit_json_last_error_msg_value_to_stack(value, args, module)?;
        return Ok(true);
    }
    if name.eq_ignore_ascii_case("get_class") {
        emit_get_class_value_to_stack(value, args, module)?;
        return Ok(true);
    }
    if name.eq_ignore_ascii_case("get_parent_class") {
        emit_get_parent_class_value_to_stack(value, args, module)?;
        return Ok(true);
    }
    if name.eq_ignore_ascii_case("json_encode") && emit_json_encode_array_value_to_stack(value, args, module)? {
        return Ok(true);
    }
    let value = eval_output_string_builtin(value, name, args, module)?;
    let (ptr, len) = module.intern_string(&value);
    module.body().line(&format!("i32.const {}", ptr));
    module.body().line(&format!("i32.const {}", len));
    if name.eq_ignore_ascii_case("json_encode") {
        emit_json_last_error_none(module);
    }
    Ok(true)
}
