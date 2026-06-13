//! Purpose:
//! Lowers runtime-backed PHP string builtin calls that directly write output.
//! Keeps the broad string builtin dispatcher out of the main wasm expression file.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::emit_output_string_builtin()`
//!
//! Key details:
//! - Emits only wasm32-web helper calls and preserves existing fallback behavior.

use super::*;

pub(super) fn emit_output_string_builtin(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if emit_runtime_output_string_builtin(call, name, args, module)? {
        return Ok(());
    }
    let value = eval_output_string_builtin(call, name, args, module)?;
    let (ptr, len) = module.intern_string(&value);
    module.body().line(&format!("i32.const {}", ptr));
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $host_write");
    if name.eq_ignore_ascii_case("json_encode") {
        emit_json_last_error_none(module);
    }
    Ok(())
}

pub(super) fn emit_runtime_output_string_builtin(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "get_class" | "get_parent_class" => {
            if lower == "get_parent_class" {
                emit_get_parent_class_value_to_stack(call, args, module)?;
            } else {
                emit_get_class_value_to_stack(call, args, module)?;
            }
            module.body().line("call $host_write");
            Ok(true)
        }
        "json_last_error_msg" => {
            emit_json_last_error_msg_value_to_stack(call, args, module)?;
            module.body().line("call $host_write");
            Ok(true)
        }
        "strtolower" | "strtoupper" | "ucwords" | "strrev" | "addslashes" | "stripslashes"
        | "bin2hex" | "hex2bin" | "nl2br" | "urlencode" | "rawurlencode" | "urldecode"
        | "rawurldecode" | "base64_encode" => {
            let [arg] = args else {
                return Err(CompileError::new(
                    call.span,
                    &format!("wasm32-web {}() expects exactly one argument", name),
                ));
            };
            let Some(var) = runtime_string_arg_or_materialize(arg, "string_unary_arg", module)? else {
                return Ok(false);
            };
            match lower.as_str() {
                "strtolower" => emit_runtime_ascii_case_transform(&var, AsciiCase::Lower, module),
                "strtoupper" => emit_runtime_ascii_case_transform(&var, AsciiCase::Upper, module),
                "ucwords" => emit_runtime_ucwords(&var, module),
                "strrev" => emit_runtime_strrev(&var, module),
                "addslashes" => emit_runtime_addslashes(&var, module),
                "stripslashes" => emit_runtime_stripslashes(&var, module),
                "bin2hex" => emit_runtime_bin2hex(&var, module),
                "hex2bin" => emit_runtime_hex2bin(&var, module),
                "nl2br" => emit_runtime_nl2br(&var, module),
                "urlencode" => emit_runtime_urlencode(&var, SpaceEncoding::Plus, module),
                "rawurlencode" => emit_runtime_urlencode(&var, SpaceEncoding::Percent20, module),
                "urldecode" => emit_runtime_urldecode(&var, SpaceEncoding::Plus, module),
                "rawurldecode" => emit_runtime_urldecode(&var, SpaceEncoding::Percent20, module),
                "base64_encode" => emit_runtime_base64_encode(&var, module),
                _ => unreachable!(),
            }
            Ok(true)
        }
        "base64_decode" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web base64_decode() expects one or two arguments",
                ));
            }
            let Some(var) = runtime_string_arg_or_materialize(&args[0], "base64_decode_arg", module)? else {
                return Ok(false);
            };
            let strict = args
                .get(1)
                .map(|arg| runtime_bool_arg(arg, module))
                .transpose()?
                .unwrap_or(RuntimeBoolArg::Static(false));
            emit_runtime_base64_decode(&var, strict, module);
            Ok(true)
        }
        "lcfirst" | "ucfirst" => {
            let [arg] = args else {
                return Err(CompileError::new(
                    call.span,
                    &format!("wasm32-web {}() expects exactly one argument", name),
                ));
            };
            let Some(var) = runtime_string_arg_or_materialize(arg, "first_case_arg", module)? else {
                return Ok(false);
            };
            if lower == "lcfirst" {
                emit_runtime_first_case_transform(&var, AsciiCase::Lower, module);
            } else {
                emit_runtime_first_case_transform(&var, AsciiCase::Upper, module);
            }
            Ok(true)
        }
        "trim" | "ltrim" | "rtrim" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(
                    call.span,
                    &format!("wasm32-web {}() expects one or two arguments", name),
                ));
            }
            let Some(var) = runtime_string_arg_or_materialize(&args[0], "trim_arg", module)? else {
                return Ok(false);
            };
            let side = match lower.as_str() {
                "ltrim" => TrimSide::Left,
                "rtrim" => TrimSide::Right,
                _ => TrimSide::Both,
            };
            let charlist_var = args
                .get(1)
                .map(|arg| runtime_string_arg_or_materialize(arg, "trim_charlist", module))
                .transpose()?
                .flatten();
            if let Some(charlist_var) = charlist_var {
                emit_runtime_trim_var_charlist(&var, side, &charlist_var, module);
            } else {
                let charlist = args
                    .get(1)
                    .map(|arg| static_ascii_string_arg(call, arg, module))
                    .transpose()?;
                emit_runtime_trim(&var, side, charlist.as_deref(), module);
            }
            Ok(true)
        }
        "substr" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web substr() expects two or three arguments",
                ));
            }
            let Some(var) = runtime_string_arg_or_materialize(&args[0], "substr_arg", module)? else {
                return Ok(false);
            };
            let offset = static_int_value(&args[1]);
            let length = args.get(2).and_then(static_int_value);
            if let Some(offset) = offset {
                if args.len() == 2 || length.is_some() {
                    emit_runtime_substr(&var, offset, length, module);
                } else {
                    emit_runtime_substr_dynamic(&var, &args[1], args.get(2), module)?;
                }
            } else {
                emit_runtime_substr_dynamic(&var, &args[1], args.get(2), module)?;
            }
            Ok(true)
        }
        "str_repeat" => {
            let [string, times] = args else {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web str_repeat() expects exactly two arguments",
                ));
            };
            let Some(var) = runtime_string_arg_or_materialize(string, "str_repeat_arg", module)? else {
                return Ok(false);
            };
            if let Some(times) = static_int_value(times) {
                if times < 0 {
                    return Err(CompileError::new(
                        call.span,
                        "wasm32-web str_repeat() does not support negative repeat counts",
                    ));
                }
                emit_runtime_str_repeat(&var, times, module);
            } else {
                emit_runtime_str_repeat_dynamic(&var, times, module)?;
            }
            Ok(true)
        }
        "htmlspecialchars" | "htmlentities" => {
            if args.is_empty() || args.len() > 4 {
                return Err(CompileError::new(
                    call.span,
                    &format!("wasm32-web {}() expects one to four arguments", name),
                ));
            }
            let Some(var) = runtime_string_arg_or_materialize(&args[0], "html_escape_arg", module)? else {
                return Ok(false);
            };
            if args.len() > 2 {
                validate_html_encoding(call, args.get(2), module)?;
                let double_encode = args
                    .get(3)
                    .map(|arg| runtime_bool_arg(arg, module))
                    .transpose()?
                    .unwrap_or(RuntimeBoolArg::Static(true));
                emit_runtime_html_escape_with_double_encode(
                    &var,
                    name,
                    args.get(1),
                    double_encode,
                    module,
                )?;
                return Ok(true);
            }
            emit_runtime_html_escape_with_double_encode(
                &var,
                name,
                args.get(1),
                RuntimeBoolArg::Static(true),
                module,
            )?;
            Ok(true)
        }
        "html_entity_decode" => {
            if args.is_empty() || args.len() > 3 {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web html_entity_decode() expects one to three arguments",
                ));
            }
            let Some(var) = runtime_string_arg_or_materialize(&args[0], "html_decode_arg", module)? else {
                return Ok(false);
            };
            if args.len() > 2 {
                validate_html_encoding(call, args.get(2), module)?;
            }
            if let Some(flags_arg) = args.get(1) {
                if let Some(flags) = static_or_const_int_value(flags_arg) {
                    emit_runtime_html_entity_decode(&var, flags, module);
                } else {
                    emit_runtime_html_entity_decode_dynamic(&var, flags_arg, module)?;
                }
            } else {
                emit_runtime_html_entity_decode(&var, 3, module);
            }
            Ok(true)
        }
        "number_format" => {
            if args.is_empty() || args.len() > 4 {
                return Ok(false);
            }
            if emit_number_format_string_builtin_value_to_stack(call, "number_format", args, module)? {
                module.body().line("call $host_write");
                return Ok(true);
            }
            let Some(var) = runtime_number_format_int_arg_local(&args[0], "number_format_arg", module) else {
                return Ok(false);
            };
            let decimals = runtime_number_format_decimals(call, args.get(1), module)?;
            let dec_point = runtime_number_format_separator(call, args.get(2), ".", module)?;
            let thousands = runtime_number_format_separator(call, args.get(3), ",", module)?;
            emit_runtime_number_format_int(&var, decimals, dec_point, thousands, module);
            Ok(true)
        }
        "json_encode" => {
            if args.is_empty() || args.len() > 3 {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web json_encode() expects one to three arguments",
                ));
            }
            if emit_output_json_encode_array_local(call, args, module)? {
                return Ok(true);
            }
            let Some(var) = runtime_string_arg_or_materialize(&args[0], "json_encode_arg", module)? else {
                return Ok(false);
            };
            let flags = args
                .get(1)
                .map(static_or_const_int_value)
                .unwrap_or(Some(0))
                .ok_or_else(|| {
                    CompileError::new(
                        call.span,
                        "wasm32-web runtime json_encode() currently requires static flags",
                    )
                })?;
            if let Some(depth) = args.get(2) {
                const_or_literal_int_arg(depth)?;
            }
            emit_runtime_json_encode_string(&var, flags, module);
            emit_json_last_error_none(module);
            Ok(true)
        }
        "md5" | "sha1" | "hash" => {
            if emit_hash_string_builtin_value_to_stack(call, name, args, module)? {
                module.body().line("call $host_write");
                return Ok(true);
            }
            Ok(false)
        }
        "implode" => emit_output_implode_assigned_string_array(call, args, module),
        "sprintf" => emit_runtime_sprintf(call, args, module),
        "str_replace" | "str_ireplace" => {
            if args.len() != 3 && args.len() != 4 {
                return Err(CompileError::new(
                    call.span,
                    &format!("wasm32-web {}() expects three arguments plus an optional count local", name),
                ));
            }
            let count_local = str_replace_count_local(call, args, module)?;
            let search = &args[0];
            let replace = &args[1];
            let subject = &args[2];
            if let Some((value, count)) = eval_static_str_replace_args(call, name, args, module)? {
                if let Some(count_var) = count_local.as_deref() {
                    emit_set_i64_local_const(count_var, count, module);
                }
                let (ptr, len) = module.intern_string(&value);
                module.body().line(&format!("i32.const {}", ptr));
                module.body().line(&format!("i32.const {}", len));
                module.body().line("call $host_write");
                return Ok(true);
            }
            if let Some(count_var) = count_local.as_deref() {
                if let (Some(search), Some(replace), Some(subject)) = (
                    static_string_value(search, module),
                    static_string_value(replace, module),
                    static_string_value(subject, module),
                ) {
                    let (value, count) =
                        eval_literal_str_replace_with_count(name, &search, &replace, &subject);
                    emit_set_i64_local_const(count_var, count, module);
                    let (ptr, len) = module.intern_string(&value);
                    module.body().line(&format!("i32.const {}", ptr));
                    module.body().line(&format!("i32.const {}", len));
                    module.body().line("call $host_write");
                    return Ok(true);
                }
            }
            let Some(var) = string_coercion_arg_or_materialize(subject, "replace_subject", module)? else {
                return Ok(false);
            };
            let case_insensitive = name.eq_ignore_ascii_case("str_ireplace");
            let search_storage = string_coercion_arg_or_materialize(search, "replace_search", module)?;
            let replace_storage = string_coercion_arg_or_materialize(replace, "replace_value", module)?;
            match (
                search_storage.as_deref(),
                replace_storage.as_deref(),
            ) {
                (None, None) => {
                    let search = static_ascii_string_arg(call, search, module)?;
                    let replace = static_ascii_string_arg(call, replace, module)?;
                    emit_runtime_str_replace(
                        &var,
                        &search,
                        &replace,
                        case_insensitive,
                        count_local.as_deref(),
                        module,
                    );
                }
                (None, Some(replace_var)) => {
                    let search = static_ascii_string_arg(call, search, module)?;
                    emit_runtime_str_replace_var_replacement(
                        &var,
                        &search,
                        replace_var,
                        case_insensitive,
                        count_local.as_deref(),
                        module,
                    );
                }
                (Some(search_var), None) => {
                    let replace = static_ascii_string_arg(call, replace, module)?;
                    emit_runtime_str_replace_var_search(
                        &var,
                        search_var,
                        WasmReplacement::Literal(&replace),
                        case_insensitive,
                        count_local.as_deref(),
                        module,
                    );
                }
                (Some(search_var), Some(replace_var)) => {
                    emit_runtime_str_replace_var_search(
                        &var,
                        search_var,
                        WasmReplacement::Variable(replace_var),
                        case_insensitive,
                        count_local.as_deref(),
                        module,
                    );
                }
            }
            Ok(true)
        }
        "str_pad" => {
            if args.len() < 2 || args.len() > 4 {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web str_pad() expects two to four arguments",
                ));
            }
            let Some(var) = runtime_string_arg_or_materialize(&args[0], "str_pad_arg", module)? else {
                return Ok(false);
            };
            let pad_type = runtime_str_pad_type(call, args.get(3), module)?;
            let pad_var = args
                .get(2)
                .map(|arg| runtime_string_arg_or_materialize(arg, "str_pad_pad", module))
                .transpose()?
                .flatten();
            if let Some(pad_var) = pad_var {
                if let Some(target_len) = static_int_value(&args[1]) {
                    emit_runtime_str_pad_var_pad(&var, target_len, &pad_var, pad_type, module);
                } else {
                    emit_runtime_str_pad_dynamic_var_pad(
                        &var,
                        &args[1],
                        &pad_var,
                        pad_type,
                        module,
                    )?;
                }
            } else {
                let pad = args
                    .get(2)
                    .map(|arg| static_ascii_string_arg(call, arg, module))
                    .transpose()?
                    .unwrap_or_else(|| " ".to_string());
                if pad.is_empty() {
                    return Err(CompileError::new(
                        call.span,
                        "wasm32-web str_pad() requires a non-empty literal pad string",
                    ));
                }
                if let Some(target_len) = static_int_value(&args[1]) {
                    emit_runtime_str_pad(&var, target_len, &pad, pad_type, module);
                } else {
                    emit_runtime_str_pad_dynamic(&var, &args[1], &pad, pad_type, module)?;
                }
            }
            Ok(true)
        }
        "substr_replace" => {
            if args.len() != 3 && args.len() != 4 {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web substr_replace() expects three or four arguments",
                ));
            }
            let Some(var) = runtime_string_arg_or_materialize(&args[0], "substr_replace_arg", module)? else {
                return Ok(false);
            };
            if let Some(replacement_var) =
                runtime_string_arg_or_materialize(&args[1], "substr_replace_repl", module)?
            {
                if let (Some(offset), true) = (
                    static_int_value(&args[2]),
                    args.get(3).map(static_int_value).unwrap_or(Some(0)).is_some(),
                ) {
                    let length = args.get(3).map(literal_int_arg).transpose()?;
                    emit_runtime_substr_replace_var_replacement(
                        &var,
                        &replacement_var,
                        offset,
                        length,
                        module,
                    );
                } else {
                    emit_runtime_substr_replace_dynamic(
                        &var,
                        WasmReplacement::Variable(&replacement_var),
                        &args[2],
                        args.get(3),
                        module,
                    )?;
                }
            } else {
                let replacement = static_ascii_string_arg(call, &args[1], module)?;
                if let (Some(offset), true) = (
                    static_int_value(&args[2]),
                    args.get(3).map(static_int_value).unwrap_or(Some(0)).is_some(),
                ) {
                    let length = args.get(3).map(literal_int_arg).transpose()?;
                    emit_runtime_substr_replace(&var, &replacement, offset, length, module);
                } else {
                    emit_runtime_substr_replace_dynamic(
                        &var,
                        WasmReplacement::Literal(&replacement),
                        &args[2],
                        args.get(3),
                        module,
                    )?;
                }
            }
            Ok(true)
        }
        "wordwrap" => {
            if args.is_empty() || args.len() > 4 {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web wordwrap() expects one to four arguments",
                ));
            }
            let Some(var) = runtime_string_arg_or_materialize(&args[0], "wordwrap_arg", module)? else {
                return Ok(false);
            };
            let break_var_storage = args
                .get(2)
                .map(|arg| runtime_string_arg_or_materialize(arg, "wordwrap_break", module))
                .transpose()?
                .flatten();
            let literal_break_storage;
            let break_text = if let Some(break_var) = break_var_storage.as_deref() {
                WasmBreakText::Variable(break_var)
            } else {
                literal_break_storage = args
                    .get(2)
                    .map(|arg| static_ascii_string_arg(call, arg, module))
                    .transpose()?
                    .unwrap_or_else(|| "\n".to_string());
                WasmBreakText::Literal(&literal_break_storage)
            };
            let cut = args
                .get(3)
                .map(|arg| runtime_bool_arg(arg, module))
                .transpose()?
                .map(WasmWordwrapCut::from)
                .unwrap_or(WasmWordwrapCut::Static(false));
            match args.get(1) {
                Some(width) => {
                    if let Some(width) = static_int_value(width) {
                        emit_runtime_wordwrap(&var, width, break_text, cut, module);
                    } else {
                        emit_runtime_wordwrap_dynamic(&var, width, break_text, cut, module)?;
                    }
                }
                None => emit_runtime_wordwrap(&var, 75, break_text, cut, module),
            }
            Ok(true)
        }
        "basename" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web basename() expects one or two arguments",
                ));
            }
            let Some(var) = runtime_string_arg_or_materialize(&args[0], "basename_arg", module)? else {
                return Ok(false);
            };
            if let Some(suffix_arg) = args.get(1) {
                if let Some(suffix_var) =
                    runtime_string_arg_or_materialize(suffix_arg, "basename_suffix", module)?
                {
                    emit_runtime_basename_var_suffix(&var, &suffix_var, module);
                } else {
                    let suffix = static_ascii_string_arg(call, suffix_arg, module)?;
                    emit_runtime_basename(&var, Some(&suffix), module);
                }
            } else {
                emit_runtime_basename(&var, None, module);
            }
            Ok(true)
        }
        "dirname" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web dirname() expects one or two arguments",
                ));
            }
            let Some(var) = runtime_string_arg_or_materialize(&args[0], "dirname_arg", module)? else {
                return Ok(false);
            };
            if let Some(levels_expr) = args.get(1) {
                if let Some(levels) = static_int_value(levels_expr) {
                    if levels < 1 {
                        return Err(CompileError::new(
                            call.span,
                            "wasm32-web runtime dirname() levels must be at least one",
                        ));
                    }
                    emit_runtime_dirname(&var, levels, module);
                } else {
                    emit_runtime_dirname_dynamic(&var, levels_expr, module)?;
                }
            } else {
                emit_runtime_dirname(&var, 1, module);
            }
            Ok(true)
        }
        "pathinfo" => {
            if args.len() != 2 {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web pathinfo() currently requires a scalar flag argument",
                ));
            }
            let Some(var) = runtime_string_arg_or_materialize(&args[0], "pathinfo_arg", module)? else {
                return Ok(false);
            };
            if let Some(flag) = static_or_const_int_value(&args[1]) {
                emit_runtime_pathinfo_static_flag(call, &var, flag, module)?;
            } else {
                emit_runtime_pathinfo_dynamic_flag(&var, &args[1], module)?;
            }
            Ok(true)
        }
        "chr" => {
            let [codepoint] = args else {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web chr() expects exactly one argument",
                ));
            };
            emit_runtime_chr_output(codepoint, module)?;
            Ok(true)
        }
        "strstr" => {
            if args.len() != 2 && args.len() != 3 {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web strstr() expects two or three arguments",
                ));
            }
            let var_storage;
            let var = if let Some(var) = runtime_string_arg_or_materialize(&args[0], "strstr_arg", module)? {
                var
            } else if string_cast_value_supported(&args[0], module) {
                var_storage = materialize_string_cast_expr(&args[0], "strstr_arg", module)?;
                var_storage
            } else {
                return Ok(false);
            };
            let before_needle = args
                .get(2)
                .map(|arg| runtime_bool_arg(arg, module))
                .transpose()?
                .unwrap_or(RuntimeBoolArg::Static(false));
            if let Some(needle_var) =
                runtime_string_arg_or_materialize(&args[1], "strstr_needle", module)?
            {
                emit_runtime_strstr_var(&var, &needle_var, before_needle, module);
            } else if string_cast_value_supported(&args[1], module) && static_string_value(&args[1], module).is_none() {
                let needle_var = materialize_string_cast_expr(&args[1], "strstr_needle", module)?;
                emit_runtime_strstr_var(&var, &needle_var, before_needle, module);
            } else {
                let needle = static_ascii_string_arg(call, &args[1], module)?;
                emit_runtime_strstr(&var, &needle, before_needle, module);
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}
