//! Purpose:
//! Lowers PHP string-producing builtins into wasm32-web heap string values.
//! Keeps stack value materialization paths separate from direct output lowering.
//!
//! Called from:
//! - `crate::codegen::wasm::expr`
//!
//! Key details:
//! - Emits pointer/length pairs on the wasm stack for supported string builtin results.
//! - Unsupported runtime shapes return CompileError rather than silently guessing PHP behavior.

mod replace;

use super::*;
pub(super) use replace::emit_str_replace_string_builtin_value_to_stack;

pub(super) fn emit_same_len_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let lower = name.to_ascii_lowercase();
    if !matches!(
        lower.as_str(),
        "strtolower" | "strtoupper" | "lcfirst" | "ucfirst" | "ucwords" | "strrev"
    ) {
        return Ok(false);
    }
    let [arg] = args else {
        return Err(CompileError::new(
            call.span,
            &format!("wasm32-web {}() expects exactly one argument", name),
        ));
    };
    if let Some(value) = static_string_value(arg, module) {
        let result = match lower.as_str() {
            "strtolower" => ascii_lower(&value),
            "strtoupper" => ascii_upper(&value),
            "lcfirst" => ascii_lcfirst(&value),
            "ucfirst" => ascii_ucfirst(&value),
            "ucwords" => ascii_ucwords(&value),
            "strrev" => value.as_bytes().iter().rev().map(|b| char::from(*b)).collect(),
            _ => unreachable!(),
        };
        let (ptr, len) = module.intern_string(&result);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(true);
    }
    let Some(var) = runtime_string_arg_or_materialize(arg, "string_value_arg", module)? else {
        return Ok(false);
    };
    match lower.as_str() {
        "strtolower" => emit_runtime_same_len_string_value(&var, RuntimeStringValueTransform::Case(AsciiCase::Lower), module),
        "strtoupper" => emit_runtime_same_len_string_value(&var, RuntimeStringValueTransform::Case(AsciiCase::Upper), module),
        "lcfirst" => emit_runtime_same_len_string_value(&var, RuntimeStringValueTransform::FirstCase(AsciiCase::Lower), module),
        "ucfirst" => emit_runtime_same_len_string_value(&var, RuntimeStringValueTransform::FirstCase(AsciiCase::Upper), module),
        "ucwords" => emit_runtime_same_len_string_value(&var, RuntimeStringValueTransform::Ucwords, module),
        "strrev" => emit_runtime_same_len_string_value(&var, RuntimeStringValueTransform::Reverse, module),
        _ => unreachable!(),
    }
    Ok(true)
}

pub(super) fn emit_trim_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let lower = name.to_ascii_lowercase();
    let side = match lower.as_str() {
        "trim" => TrimSide::Both,
        "ltrim" => TrimSide::Left,
        "rtrim" => TrimSide::Right,
        _ => return Ok(false),
    };
    if args.is_empty() || args.len() > 2 {
        return Err(CompileError::new(
            call.span,
            &format!("wasm32-web {}() expects one or two arguments", name),
        ));
    }
    if static_string_value(&args[0], module).is_some() {
        let value = eval_output_string_builtin(call, name, args, module)?;
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(true);
    }

    let Some(var) = runtime_string_arg_or_materialize(&args[0], "trim_value_arg", module)? else {
        return Ok(false);
    };
    let charlist_var = args
        .get(1)
        .map(|arg| runtime_string_arg_or_materialize(arg, "trim_value_charlist", module))
        .transpose()?
        .flatten();
    let charlist = if charlist_var.is_none() {
        args.get(1)
            .map(|arg| static_ascii_string_arg(call, arg, module))
            .transpose()?
    } else {
        None
    };
    emit_runtime_trim_value_to_stack(&var, side, charlist.as_deref(), charlist_var.as_deref(), module);
    Ok(true)
}

pub(super) fn emit_substr_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !name.eq_ignore_ascii_case("substr") {
        return Ok(false);
    }
    if args.len() < 2 || args.len() > 3 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web substr() expects two or three arguments",
        ));
    }
    if static_string_value(&args[0], module).is_some() {
        let value = eval_output_string_builtin(call, name, args, module)?;
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(true);
    }
    let Some(var) = runtime_string_arg_or_materialize(&args[0], "substr_value_arg", module)? else {
        return Ok(false);
    };
    let start = module.next_label("substr_value_start");
    let end = module.next_label("substr_value_end");
    module.declare_i32_local(start.trim_start_matches('$').to_string());
    module.declare_i32_local(end.trim_start_matches('$').to_string());
    if let Some(offset) = static_int_value(&args[1]) {
        let length = args.get(2).and_then(static_int_value);
        if args.len() == 2 || length.is_some() {
            emit_runtime_substring_bounds(&var, offset, length, &start, &end, module);
        } else {
            emit_runtime_substring_dynamic_bounds(&var, &args[1], args.get(2), &start, &end, module)?;
        }
    } else {
        emit_runtime_substring_dynamic_bounds(&var, &args[1], args.get(2), &start, &end, module)?;
    }
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.sub");
    Ok(true)
}

pub(super) fn emit_str_repeat_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !name.eq_ignore_ascii_case("str_repeat") {
        return Ok(false);
    }
    let [string, times] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web str_repeat() expects exactly two arguments",
        ));
    };
    if static_string_value(string, module).is_some() {
        let value = eval_output_string_builtin(call, name, args, module)?;
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(true);
    }
    let Some(var) = runtime_string_arg_or_materialize(string, "str_repeat_value_arg", module)? else {
        return Ok(false);
    };
    let count = module.next_label("repeat_value_count");
    module.declare_i32_local(count.trim_start_matches('$').to_string());
    if let Some(times) = static_int_value(times) {
        if times < 0 || times > i64::from(i32::MAX) {
            return Err(CompileError::new(
                call.span,
                "wasm32-web str_repeat() requires a non-negative 32-bit repeat count",
            ));
        }
        module.body().line(&format!("i32.const {}", times));
        module.body().line(&format!("local.set {}", count));
    } else {
        emit_non_negative_i32_count(times, &count, module)?;
    }
    emit_runtime_str_repeat_value_to_stack(&var, &count, module);
    Ok(true)
}

pub(super) fn emit_substr_replace_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !name.eq_ignore_ascii_case("substr_replace") {
        return Ok(false);
    }
    if args.len() < 3 || args.len() > 4 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web substr_replace() expects three or four arguments",
        ));
    }
    if static_string_value(&args[0], module).is_some() {
        let value = eval_output_string_builtin(call, name, args, module)?;
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(true);
    }
    let Some(var) = runtime_string_arg_or_materialize(&args[0], "substr_replace_value_arg", module)? else {
        return Ok(false);
    };
    let replacement_var = runtime_string_arg_or_materialize(&args[1], "substr_replace_value_repl", module)?;
    let replacement_literal = if replacement_var.is_none() {
        Some(static_ascii_string_arg(call, &args[1], module)?)
    } else {
        None
    };
    let start = module.next_label("subrep_value_start");
    let end = module.next_label("subrep_value_end");
    module.declare_i32_local(start.trim_start_matches('$').to_string());
    module.declare_i32_local(end.trim_start_matches('$').to_string());
    if let Some(offset) = static_int_value(&args[2]) {
        let length = args.get(3).and_then(static_int_value);
        if args.len() == 3 || length.is_some() {
            emit_runtime_substring_bounds(&var, offset, length, &start, &end, module);
        } else {
            emit_runtime_substring_dynamic_bounds(&var, &args[2], args.get(3), &start, &end, module)?;
        }
    } else {
        emit_runtime_substring_dynamic_bounds(&var, &args[2], args.get(3), &start, &end, module)?;
    }
    if let Some(replacement_var) = replacement_var {
        emit_runtime_substr_replace_value_to_stack(
            &var,
            WasmReplacement::Variable(&replacement_var),
            &start,
            &end,
            module,
        );
    } else {
        let replacement = replacement_literal.as_deref().unwrap_or("");
        emit_runtime_substr_replace_value_to_stack(
            &var,
            WasmReplacement::Literal(replacement),
            &start,
            &end,
            module,
        );
    }
    Ok(true)
}

pub(super) fn emit_bin2hex_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !name.eq_ignore_ascii_case("bin2hex") {
        return Ok(false);
    }
    let [arg] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web bin2hex() expects exactly one argument",
        ));
    };
    if static_string_value(arg, module).is_some() {
        let value = eval_output_string_builtin(call, name, args, module)?;
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(true);
    }
    let Some(var) = runtime_string_arg_or_materialize(arg, "bin2hex_value_arg", module)? else {
        return Ok(false);
    };
    emit_runtime_bin2hex_value_to_stack(&var, module);
    Ok(true)
}

pub(super) fn emit_escape_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let lower = name.to_ascii_lowercase();
    if !matches!(lower.as_str(), "addslashes" | "stripslashes" | "nl2br") {
        return Ok(false);
    }
    let [arg] = args else {
        return Err(CompileError::new(
            call.span,
            &format!("wasm32-web {}() expects exactly one argument", name),
        ));
    };
    if static_string_value(arg, module).is_some() {
        let value = eval_output_string_builtin(call, name, args, module)?;
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(true);
    }
    let Some(var) = runtime_string_arg_or_materialize(arg, "escape_value_arg", module)? else {
        return Ok(false);
    };
    match lower.as_str() {
        "addslashes" => emit_runtime_addslashes_value_to_stack(&var, module),
        "stripslashes" => emit_runtime_stripslashes_value_to_stack(&var, module),
        "nl2br" => emit_runtime_nl2br_value_to_stack(&var, module),
        _ => unreachable!(),
    }
    Ok(true)
}

pub(super) fn emit_chr_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !name.eq_ignore_ascii_case("chr") {
        return Ok(false);
    }
    let [codepoint] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web chr() expects exactly one argument",
        ));
    };
    if static_int_value(codepoint).is_some() {
        let value = eval_output_string_builtin(call, name, args, module)?;
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(true);
    }
    emit_runtime_chr_value_to_stack(codepoint, module)?;
    Ok(true)
}

pub(super) fn emit_hex2bin_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !name.eq_ignore_ascii_case("hex2bin") {
        return Ok(false);
    }
    let [arg] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web hex2bin() expects exactly one argument",
        ));
    };
    if static_string_value(arg, module).is_some() {
        let value = eval_output_string_builtin(call, name, args, module)?;
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(true);
    }
    let Some(var) = runtime_string_arg_or_materialize(arg, "hex2bin_value_arg", module)? else {
        return Ok(false);
    };
    emit_runtime_hex2bin_value_to_stack(&var, module);
    Ok(true)
}

pub(super) fn emit_base64_encode_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !name.eq_ignore_ascii_case("base64_encode") {
        return Ok(false);
    }
    let [arg] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web base64_encode() expects exactly one argument",
        ));
    };
    if static_string_value(arg, module).is_some() {
        let value = eval_output_string_builtin(call, name, args, module)?;
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(true);
    }
    let Some(var) = runtime_string_arg_or_materialize(arg, "base64_encode_value_arg", module)? else {
        return Ok(false);
    };
    emit_runtime_base64_encode_value_to_stack(&var, module);
    Ok(true)
}

pub(super) fn emit_base64_decode_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !name.eq_ignore_ascii_case("base64_decode") {
        return Ok(false);
    }
    if args.is_empty() || args.len() > 2 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web base64_decode() expects one or two arguments",
        ));
    }
    let strict = args
        .get(1)
        .map(|arg| runtime_bool_arg(arg, module))
        .transpose()?
        .unwrap_or(RuntimeBoolArg::Static(false));
    if static_string_value(&args[0], module).is_some() && matches!(strict, RuntimeBoolArg::Static(_)) {
        let value = eval_output_string_builtin(call, name, args, module)?;
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(true);
    }
    let Some(var) = runtime_string_arg_or_materialize(&args[0], "base64_decode_value_arg", module)? else {
        return Ok(false);
    };
    emit_runtime_base64_decode_value_to_stack(&var, strict, module);
    Ok(true)
}

pub(super) fn emit_urlencode_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let space = match name.to_ascii_lowercase().as_str() {
        "urlencode" => SpaceEncoding::Plus,
        "rawurlencode" => SpaceEncoding::Percent20,
        _ => return Ok(false),
    };
    let [arg] = args else {
        return Err(CompileError::new(
            call.span,
            &format!("wasm32-web {}() expects exactly one argument", name),
        ));
    };
    if static_string_value(arg, module).is_some() {
        let value = eval_output_string_builtin(call, name, args, module)?;
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(true);
    }
    let Some(var) = runtime_string_arg_or_materialize(arg, "urlencode_value_arg", module)? else {
        return Ok(false);
    };
    emit_runtime_urlencode_value_to_stack(&var, space, module);
    Ok(true)
}

pub(super) fn emit_urldecode_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let space = match name.to_ascii_lowercase().as_str() {
        "urldecode" => SpaceEncoding::Plus,
        "rawurldecode" => SpaceEncoding::Percent20,
        _ => return Ok(false),
    };
    let [arg] = args else {
        return Err(CompileError::new(
            call.span,
            &format!("wasm32-web {}() expects exactly one argument", name),
        ));
    };
    if static_string_value(arg, module).is_some() {
        let value = eval_output_string_builtin(call, name, args, module)?;
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(true);
    }
    let Some(var) = runtime_string_arg_or_materialize(arg, "urldecode_value_arg", module)? else {
        return Ok(false);
    };
    emit_runtime_urldecode_value_to_stack(&var, space, module);
    Ok(true)
}

pub(super) fn emit_html_escape_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !matches!(name.to_ascii_lowercase().as_str(), "htmlspecialchars" | "htmlentities") {
        return Ok(false);
    }
    if args.is_empty() || args.len() > 4 {
        return Err(CompileError::new(
            call.span,
            &format!("wasm32-web {}() expects one to four arguments", name),
        ));
    }
    if args.iter().all(|arg| static_string_value(arg, module).is_some() || static_or_const_int_value(arg).is_some() || matches!(arg.kind, ExprKind::BoolLiteral(_))) {
        let value = eval_output_string_builtin(call, name, args, module)?;
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(true);
    }
    let Some(var) = string_arg_or_materialize(&args[0], "html_escape_value_arg", module)? else {
        return Ok(false);
    };
    let flags_arg = args.get(1);
    if let Some(encoding_arg) = args.get(2) {
        let Some(encoding) = static_or_tracked_string_value(encoding_arg, module) else {
            return Err(CompileError::new(
                encoding_arg.span,
                "wasm32-web htmlspecialchars()/htmlentities() string values currently require a static encoding",
            ));
        };
        if !html_encoding_is_ascii_compatible(&encoding) {
            return Err(CompileError::new(
                encoding_arg.span,
                "wasm32-web htmlspecialchars()/htmlentities() string values currently support UTF-8 or ISO-8859-1 only for ASCII-safe strings",
            ));
        }
    }
    let double_encode = args
        .get(3)
        .map(|arg| runtime_bool_arg(arg, module))
        .transpose()?
        .unwrap_or(RuntimeBoolArg::Static(true));
    if let Some(flags_arg) = flags_arg {
        if let Some(flags) = static_or_const_int_value(flags_arg) {
            emit_runtime_html_escape_value_to_stack(&var, name, flags, double_encode, module);
        } else {
            emit_runtime_html_escape_dynamic_value_to_stack(&var, flags_arg, double_encode, module)?;
        }
    } else {
        emit_runtime_html_escape_value_to_stack(&var, name, 3, double_encode, module);
    }
    Ok(true)
}

pub(super) fn emit_strstr_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !name.eq_ignore_ascii_case("strstr") {
        return Ok(false);
    }
    if args.len() != 2 && args.len() != 3 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web strstr() expects two or three arguments",
        ));
    }
    if args.iter().all(|arg| static_string_value(arg, module).is_some()) {
        let value = eval_output_string_builtin(call, name, args, module)?;
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(true);
    }
    let var_storage;
    let var = if let Some(var) = string_arg_or_materialize(&args[0], "strstr_value_arg", module)? {
        var
    } else if string_cast_value_supported(&args[0], module) {
        var_storage = materialize_string_cast_expr(&args[0], "strstr_value_arg", module)?;
        var_storage
    } else {
        return Ok(false);
    };
    let before_needle = args
        .get(2)
        .map(literal_bool_arg)
        .transpose()?
        .unwrap_or(false);
    if let Some(needle_var) = string_arg_or_materialize(&args[1], "strstr_value_needle", module)? {
        emit_runtime_strstr_var_value_to_stack(&var, &needle_var, before_needle, module);
    } else if string_cast_value_supported(&args[1], module) && static_string_value(&args[1], module).is_none() {
        let needle_var = materialize_string_cast_expr(&args[1], "strstr_value_needle", module)?;
        emit_runtime_strstr_var_value_to_stack(&var, &needle_var, before_needle, module);
    } else {
        let needle = static_ascii_string_arg(call, &args[1], module)?;
        emit_runtime_strstr_value_to_stack(&var, &needle, before_needle, module);
    }
    Ok(true)
}
