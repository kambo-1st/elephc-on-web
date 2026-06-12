//! Purpose:
//! Materializes wasm32-web string arguments and temporary string locals.
//! Keeps staging helpers separate from direct string assignment and cast lowering.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` string builtins, sprintf, JSON, and array helpers.
//!
//! Key details:
//! - Helpers preserve ptr/len local layout and special array_search key string semantics.

use super::*;

pub(in crate::codegen::wasm) fn materialize_runtime_string_expr(
    value: &Expr,
    prefix: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let local = module.next_label(prefix).trim_start_matches('$').to_string();
    module.declare_i32_local(format!("{}_ptr", local));
    module.declare_i32_local(format!("{}_len", local));
    emit_string_value_to_locals(
        value,
        &format!("${}_ptr", local),
        &format!("${}_len", local),
        module,
    )?;
    Ok(local)
}

pub(in crate::codegen::wasm) fn materialize_string_cast_expr(
    value: &Expr,
    prefix: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let local = module.next_label(prefix).trim_start_matches('$').to_string();
    module.declare_i32_local(format!("{}_ptr", local));
    module.declare_i32_local(format!("{}_len", local));
    emit_string_cast_value_to_stack(value, value, module)?;
    module.body().line(&format!("local.set ${}_len", local));
    module.body().line(&format!("local.set ${}_ptr", local));
    Ok(local)
}

pub(in crate::codegen::wasm) fn materialize_int_string_expr(
    value: &Expr,
    prefix: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let local = module.next_label(prefix).trim_start_matches('$').to_string();
    module.declare_i32_local(format!("{}_ptr", local));
    module.declare_i32_local(format!("{}_len", local));
    if let Ok(value) = literal_format_int_arg(value) {
        module.body().line(&format!("i64.const {}", value));
        emit_i64_stack_string_cast_value_to_stack("string_cast", module);
    } else if expression_is_booly(value, module) {
        emit_condition(value, module)?;
        module.body().line("i64.extend_i32_s");
        emit_i64_stack_string_cast_value_to_stack("string_cast", module);
    } else if expression_is_floaty(value, module) {
        require_float(value, module)?;
        module.body().line("i64.trunc_f64_s");
        emit_i64_stack_string_cast_value_to_stack("string_cast", module);
    } else {
        emit_int_string_cast_value_to_stack(value, module)?;
    }
    module.body().line(&format!("local.set ${}_len", local));
    module.body().line(&format!("local.set ${}_ptr", local));
    Ok(local)
}

pub(in crate::codegen::wasm) fn materialize_fixed_float_string_expr(
    value: &Expr,
    prefix: &str,
    precision: usize,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    materialize_host_float_string_expr(value, prefix, precision, None, "$host_format_float_fixed", module)
}

pub(in crate::codegen::wasm) fn materialize_scientific_float_string_expr(
    value: &Expr,
    prefix: &str,
    precision: usize,
    uppercase: bool,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    materialize_host_float_string_expr(
        value,
        prefix,
        precision,
        Some(uppercase),
        "$host_format_float_scientific",
        module,
    )
}

pub(in crate::codegen::wasm) fn materialize_general_float_string_expr(
    value: &Expr,
    prefix: &str,
    precision: usize,
    uppercase: bool,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    materialize_host_float_string_expr(
        value,
        prefix,
        precision,
        Some(uppercase),
        "$host_format_float_general",
        module,
    )
}

pub(in crate::codegen::wasm) fn materialize_sprintf_float_string_expr(
    value: &Expr,
    prefix: &str,
    spec: u8,
    precision: usize,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    match spec {
        b'f' | b'F' => materialize_fixed_float_string_expr(value, prefix, precision, module),
        b'e' | b'E' => {
            materialize_scientific_float_string_expr(value, prefix, precision, spec == b'E', module)
        }
        b'g' | b'G' => materialize_general_float_string_expr(value, prefix, precision, spec == b'G', module),
        _ => unreachable!("sprintf float format was validated"),
    }
}

pub(in crate::codegen::wasm) fn materialize_host_float_string_expr(
    value: &Expr,
    prefix: &str,
    precision: usize,
    uppercase: Option<bool>,
    host: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let local = module.next_label(prefix).trim_start_matches('$').to_string();
    module.declare_i32_local(format!("{}_ptr", local));
    module.declare_i32_local(format!("{}_len", local));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set ${}_ptr", local));
    module.body().line("global.get $heap");
    module.body().line("i32.const 64");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    if let Ok(value) = literal_numeric_arg(value) {
        module.body().line(&format!("f64.const {}", wasm_float_literal(value)));
    } else if expression_is_floaty(value, module) {
        require_float(value, module)?;
    } else if expression_is_booly(value, module) {
        emit_condition(value, module)?;
        module.body().line("f64.convert_i32_s");
    } else {
        require_int(value, module)?;
        module.body().line("f64.convert_i64_s");
    }
    module.body().line(&format!("i32.const {}", precision));
    if let Some(uppercase) = uppercase {
        module.body().line(&format!("i32.const {}", i32::from(uppercase)));
    }
    module.body().line(&format!("local.get ${}_ptr", local));
    module.body().line(&format!("call {}", host));
    module.body().line(&format!("local.set ${}_len", local));
    Ok(local)
}

pub(in crate::codegen::wasm) fn materialize_char_string_expr(
    value: &Expr,
    prefix: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let local = module.next_label(prefix).trim_start_matches('$').to_string();
    module.declare_i32_local(format!("{}_ptr", local));
    module.declare_i32_local(format!("{}_len", local));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set ${}_ptr", local));
    module.body().line("global.get $heap");
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set ${}_len", local));
    module.body().line(&format!("local.get ${}_ptr", local));
    if let Ok(value) = literal_format_int_arg(value) {
        module.body().line(&format!("i32.const {}", value as u8));
    } else if expression_is_booly(value, module) {
        emit_condition(value, module)?;
    } else if expression_is_floaty(value, module) {
        require_float(value, module)?;
        module.body().line("i64.trunc_f64_s");
        module.body().line("i32.wrap_i64");
    } else {
        require_int(value, module)?;
        module.body().line("i32.wrap_i64");
    }
    module.body().line("i32.const 255");
    module.body().line("i32.and");
    module.body().line("i32.store8");
    Ok(local)
}

pub(in crate::codegen::wasm) fn materialize_unsigned_radix_string_expr(
    value: &Expr,
    radix: i64,
    uppercase: bool,
    prefix: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let local = module.next_label(prefix).trim_start_matches('$').to_string();
    let number = module.next_label("sprintf_value_radix_number");
    let divisor = module.next_label("sprintf_value_radix_divisor");
    let digit = module.next_label("sprintf_value_radix_digit");
    let byte = module.next_label("sprintf_value_radix_byte");
    let out_idx = module.next_label("sprintf_value_radix_idx");
    let loop_label = module.next_label("sprintf_value_radix_loop");
    let done_label = module.next_label("sprintf_value_radix_done");
    let divisor_loop = module.next_label("sprintf_value_radix_divisor_loop");
    let divisor_done = module.next_label("sprintf_value_radix_divisor_done");
    module.declare_i32_local(format!("{}_ptr", local));
    module.declare_i32_local(format!("{}_len", local));
    module.declare_i64_local(number.trim_start_matches('$').to_string());
    module.declare_i64_local(divisor.trim_start_matches('$').to_string());
    module.declare_i64_local(digit.trim_start_matches('$').to_string());
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    module.declare_i32_local(out_idx.trim_start_matches('$').to_string());
    if let Ok(value) = literal_format_int_arg(value) {
        module.body().line(&format!("i64.const {}", value));
    } else if expression_is_booly(value, module) {
        emit_condition(value, module)?;
        module.body().line("i64.extend_i32_s");
    } else if expression_is_floaty(value, module) {
        require_float(value, module)?;
        module.body().line("i64.trunc_f64_s");
    } else {
        require_int(value, module)?;
    }
    module.body().line(&format!("local.set {}", number));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set ${}_ptr", local));
    module.body().line("global.get $heap");
    module.body().line("i32.const 65");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
    module.body().line(&format!("local.get {}", number));
    module.body().line("i64.eqz");
    module.body().open("if");
    emit_store_byte_const(48, &format!("${}_ptr", local), &out_idx, module);
    module.body().line("else");
    module.body().line("i64.const 1");
    module.body().line(&format!("local.set {}", divisor));
    module.body().open(&format!("block {}", divisor_done));
    module.body().open(&format!("loop {}", divisor_loop));
    module.body().line(&format!("local.get {}", divisor));
    module.body().line(&format!("local.get {}", number));
    module.body().line(&format!("i64.const {}", radix));
    module.body().line("i64.div_u");
    module.body().line("i64.gt_u");
    module.body().line(&format!("br_if {}", divisor_done));
    module.body().line(&format!("local.get {}", divisor));
    module.body().line(&format!("i64.const {}", radix));
    module.body().line("i64.mul");
    module.body().line(&format!("local.set {}", divisor));
    module.body().line(&format!("br {}", divisor_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", divisor));
    module.body().line("i64.eqz");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", number));
    module.body().line(&format!("local.get {}", divisor));
    module.body().line("i64.div_u");
    module.body().line(&format!("i64.const {}", radix));
    module.body().line("i64.rem_u");
    module.body().line(&format!("local.set {}", digit));
    module.body().line(&format!("local.get {}", digit));
    module.body().line("i64.const 10");
    module.body().line("i64.lt_u");
    module.body().open("if");
    module.body().line(&format!("local.get {}", digit));
    module.body().line("i64.const 48");
    module.body().line("i64.add");
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", byte));
    module.body().line("else");
    module.body().line(&format!("local.get {}", digit));
    module.body().line(&format!("i64.const {}", if uppercase { 55 } else { 87 }));
    module.body().line("i64.add");
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", byte));
    module.body().close("end");
    emit_store_byte_local(&byte, &format!("${}_ptr", local), &out_idx, module);
    module.body().line(&format!("local.get {}", divisor));
    module.body().line(&format!("i64.const {}", radix));
    module.body().line("i64.div_u");
    module.body().line(&format!("local.set {}", divisor));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line(&format!("local.set ${}_len", local));
    Ok(local)
}

pub(in crate::codegen::wasm) fn runtime_string_arg_or_materialize(
    value: &Expr,
    prefix: &str,
    module: &mut WasmModule,
) -> Result<Option<String>, CompileError> {
    if let Some(err) = unsupported_string_coercion_expr(value) {
        return Err(err);
    }
    if let Some(var) = runtime_string_variable_arg(value, module) {
        return Ok(Some(var.to_string()));
    }
    if expression_is_stringy(value, module) && static_string_value(value, module).is_none() {
        return materialize_runtime_string_expr(value, prefix, module).map(Some);
    }
    Ok(None)
}

pub(in crate::codegen::wasm) fn string_arg_or_materialize(
    value: &Expr,
    prefix: &str,
    module: &mut WasmModule,
) -> Result<Option<String>, CompileError> {
    if let Some(err) = unsupported_string_coercion_expr(value) {
        return Err(err);
    }
    if let Some(var) = materialize_array_search_int_key_string_expr(value, prefix, module)? {
        return Ok(Some(var));
    }
    if let Some(var) = materialize_array_search_string_key_string_expr(value, prefix, module)? {
        return Ok(Some(var));
    }
    if let Some(var) = runtime_string_variable_arg(value, module) {
        return Ok(Some(var.to_string()));
    }
    if expression_is_stringy(value, module) {
        return materialize_runtime_string_expr(value, prefix, module).map(Some);
    }
    Ok(None)
}

pub(in crate::codegen::wasm) fn materialize_array_search_int_key_string_expr(
    value: &Expr,
    prefix: &str,
    module: &mut WasmModule,
) -> Result<Option<String>, CompileError> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return Ok(None);
    };
    if !name.eq_ignore_ascii_case("array_search") {
        return Ok(None);
    }
    let (needle, haystack, strict) = match args.as_slice() {
        [needle, haystack, strict] => {
            let Some(strict) = static_or_const_or_i32_bool_value(strict, module) else {
                return Err(CompileError::new(
                    strict.span,
                    "wasm32-web array_search() strict argument must be a static bool",
                ));
            };
            (needle, haystack, strict)
        }
        [needle, haystack] => (needle, haystack, false),
        _ => {
            return Err(CompileError::new(
                value.span,
                "wasm32-web array_search() expects two or three arguments",
            ))
        }
    };
    let temp;
    let var = match &haystack.kind {
        ExprKind::Variable(var)
            if module.local_kind(var) == Some(LocalKind::Array)
                && module.array_layout(var) == ArrayLayout::Assoc =>
        {
            var.as_str()
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            temp = module
                .next_label("array_search_int_key_string_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_assoc_array_items_assign(&temp, items, module)?;
            temp.as_str()
        }
        ExprKind::FunctionCall { name, .. }
            if module.has_function(name)
                && module.function_return_kind(name) == Some(ValueKind::Array)
                && module.function_array_return_layout(name) == ArrayLayout::Assoc =>
        {
            temp = module
                .next_label("array_search_int_key_string_return")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, haystack, module)?;
            temp.as_str()
        }
        _ if expression_has_array_type(haystack, module) && !expression_is_arrayy(haystack, module) => {
            temp = module
                .next_label("array_search_int_key_string_direct")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, haystack, module)?;
            temp.as_str()
        }
        _ => return Ok(None),
    };
    let Some(key_kinds) = module.array_key_kinds(var) else {
        return Ok(None);
    };
    if !key_kinds.iter().all(|kind| *kind == AssocKeyKind::Int)
        || !negative_key_array_search_needle_is_supported(needle, module)
    {
        return Ok(None);
    }
    if !strict {
        let has_scalar_value_metadata = module
            .array_runtime_value_cell_kind(var)
            .is_some_and(|kind| kind != ValueCellKind::Array)
            || module
                .array_value_cell_kinds(var)
                .is_some_and(|kinds| kinds.iter().all(|kind| *kind != ValueCellKind::Array));
        if !has_scalar_value_metadata {
            return Ok(None);
        }
    }

    let local = module.next_label(prefix).trim_start_matches('$').to_string();
    let result = module.next_label("array_search_int_key_string_result");
    let found = module.next_label("array_search_int_key_string_found");
    module.declare_i32_local(format!("{}_ptr", local));
    module.declare_i32_local(format!("{}_len", local));
    module.declare_i64_local(result.trim_start_matches('$').to_string());
    module.declare_i32_local(found.trim_start_matches('$').to_string());
    if strict {
        emit_assoc_array_search_negative_key_found(value, var, needle, &result, &found, module)?;
    } else {
        emit_assoc_array_search_loose_scalar_packed_key(value, var, needle, &result, None, Some(&found), module)?;
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}_ptr", local));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}_len", local));
    module.body().line(&format!("local.get {}", found));
    module.body().open("if");
    module.body().line(&format!("local.get {}", result));
    emit_i64_stack_string_cast_value_to_stack("array_search_int_key_string_cast", module);
    module.body().line(&format!("local.set ${}_len", local));
    module.body().line(&format!("local.set ${}_ptr", local));
    module.body().close("end");
    Ok(Some(local))
}

pub(in crate::codegen::wasm) fn materialize_array_search_string_key_string_expr(
    value: &Expr,
    prefix: &str,
    module: &mut WasmModule,
) -> Result<Option<String>, CompileError> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return Ok(None);
    };
    if !name.eq_ignore_ascii_case("array_search") {
        return Ok(None);
    }
    let (needle, haystack, strict) = match args.as_slice() {
        [needle, haystack, strict] => {
            let Some(strict) = static_or_const_or_i32_bool_value(strict, module) else {
                return Err(CompileError::new(
                    strict.span,
                    "wasm32-web array_search() strict argument must be a static bool",
                ));
            };
            (needle, haystack, strict)
        }
        [needle, haystack] => (needle, haystack, false),
        _ => {
            return Err(CompileError::new(
                value.span,
                "wasm32-web array_search() expects two or three arguments",
            ))
        }
    };
    let temp;
    let var = match &haystack.kind {
        ExprKind::Variable(var)
            if module.local_kind(var) == Some(LocalKind::Array)
                && module.array_layout(var) == ArrayLayout::Assoc =>
        {
            var.as_str()
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            temp = module
                .next_label("array_search_string_cast_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_assoc_array_items_assign(&temp, items, module)?;
            temp.as_str()
        }
        ExprKind::FunctionCall { name, .. }
            if module.has_function(name)
                && module.function_return_kind(name) == Some(ValueKind::Array)
                && module.function_array_return_layout(name) == ArrayLayout::Assoc =>
        {
            temp = module
                .next_label("array_search_string_cast_return")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, haystack, module)?;
            temp.as_str()
        }
        _ if expression_has_array_type(haystack, module) && !expression_is_arrayy(haystack, module) => {
            temp = module
                .next_label("array_search_string_cast_direct")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, haystack, module)?;
            temp.as_str()
        }
        _ => return Ok(None),
    };
    let has_string_keys = module
        .array_key_kinds(var)
        .is_some_and(|kinds| kinds.iter().all(|kind| *kind == AssocKeyKind::Str))
        || module.array_runtime_key_kind(var) == Some(AssocKeyKind::Str);
    if !has_string_keys {
        return Ok(None);
    }

    let local = module.next_label(prefix).trim_start_matches('$').to_string();
    module.declare_i32_local(format!("{}_ptr", local));
    module.declare_i32_local(format!("{}_len", local));
    if !strict {
        let Some(key) = static_loose_assoc_string_key_search_result(var, needle, module) else {
            return Ok(None);
        };
        if let Some(key) = key {
            let (key_ptr, key_len) = module.intern_string(&key);
            module.body().line(&format!("i32.const {}", key_ptr));
            module.body().line(&format!("local.set ${}_ptr", local));
            module.body().line(&format!("i32.const {}", key_len));
            module.body().line(&format!("local.set ${}_len", local));
            return Ok(Some(local));
        }
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set ${}_ptr", local));
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set ${}_len", local));
        return Ok(Some(local));
    }
    let result = module.next_label("array_search_string_cast_result");
    module.declare_i64_local(result.trim_start_matches('$').to_string());
    emit_assoc_array_search_packed_key(value, var, needle, module)?;
    module.body().line(&format!("local.set {}", result));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}_ptr", local));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}_len", local));
    module.body().line(&format!("local.get {}", result));
    module.body().line("i64.const 0");
    module.body().line("i64.ge_s");
    module.body().open("if");
    module.body().line(&format!("local.get {}", result));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set ${}_ptr", local));
    module.body().line(&format!("local.get {}", result));
    module.body().line("i64.const 32");
    module.body().line("i64.shr_u");
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set ${}_len", local));
    module.body().close("end");
    Ok(Some(local))
}
