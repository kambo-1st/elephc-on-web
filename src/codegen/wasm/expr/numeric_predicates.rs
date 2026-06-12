//! Purpose:
//! Lowers numeric predicate builtins for wasm32-web.
//! Keeps is_numeric scanning and float predicate host checks out of the main expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` scalar builtin dispatch.
//!
//! Key details:
//! - Runtime string numeric checks preserve PHP-compatible whitespace/sign/fraction/exponent scanning.

use super::*;

pub(super) fn emit_is_numeric_call(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let [arg] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web is_numeric() expects exactly one argument",
        ));
    };
    if let Some(value) = static_string_value(arg, module) {
        module
            .body()
            .line(&format!("i32.const {}", i32::from(string_literal_is_numeric(&value))));
        return Ok(ValueKind::Bool);
    }
    if let Some(var) = runtime_string_arg_or_materialize(arg, "is_numeric_arg", module)? {
        emit_runtime_is_numeric(&var, module);
        return Ok(ValueKind::Bool);
    }
    if let Some(cell) = materialize_mixed_value_cell(arg, module)? {
        emit_mixed_is_numeric(&cell, module);
        return Ok(ValueKind::Bool);
    }
    match &arg.kind {
        ExprKind::StringLiteral(value) => {
            module
                .body()
                .line(&format!("i32.const {}", i32::from(string_literal_is_numeric(value))));
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Str) => {
            emit_runtime_is_numeric(name, module);
        }
        _ => {
            let kind = emit_expr(arg, module)?;
            module.body().line("drop");
            let numeric = matches!(kind, ValueKind::Int | ValueKind::Float);
            module.body().line(&format!("i32.const {}", i32::from(numeric)));
        }
    }
    Ok(ValueKind::Bool)
}

fn emit_mixed_is_numeric(cell: &str, module: &mut WasmModule) {
    let result = module.next_label("mixed_numeric_result");
    let string = module
        .next_label("mixed_numeric_string")
        .trim_start_matches('$')
        .to_string();
    let done = module.next_label("mixed_numeric_done");
    module.declare_i32_local(result.trim_start_matches('$').to_string());
    module.declare_i32_local(format!("{}_ptr", string));
    module.declare_i32_local(format!("{}_len", string));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", result));
    module.body().open(&format!("block {}", done));
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", result));
    module.body().line(&format!("br {}", done));
    module.body().close("end");
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", result));
    module.body().line(&format!("br {}", done));
    module.body().close("end");
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set ${}_ptr", string));
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set ${}_len", string));
    emit_runtime_is_numeric(&string, module);
    module.body().line(&format!("local.set {}", result));
    module.body().line(&format!("br {}", done));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", result));
}

pub(super) fn emit_runtime_is_numeric(var: &str, module: &mut WasmModule) {
    let idx = module.next_label("numeric_idx");
    let end = module.next_label("numeric_end");
    let last = module.next_label("numeric_last");
    let byte = module.next_label("numeric_byte");
    let digits = module.next_label("numeric_digits");
    let exp_digits = module.next_label("numeric_exp_digits");
    let ok = module.next_label("numeric_ok");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(end.trim_start_matches('$').to_string());
    module.declare_i32_local(last.trim_start_matches('$').to_string());
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    module.declare_i32_local(digits.trim_start_matches('$').to_string());
    module.declare_i32_local(exp_digits.trim_start_matches('$').to_string());
    module.declare_i32_local(ok.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.set {}", end));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", ok));
    emit_trim_numeric_leading(var, &idx, &end, &byte, module);
    emit_trim_numeric_trailing(var, &idx, &end, &last, &byte, module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.ge_u");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", ok));
    module.body().line("else");
    emit_skip_numeric_sign(var, &idx, &end, &byte, module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.ge_u");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", ok));
    module.body().line("else");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", digits));
    emit_scan_numeric_digits(var, &idx, &end, &byte, &digits, module);
    emit_scan_numeric_fraction(var, &idx, &end, &byte, &digits, module);
    module.body().line(&format!("local.get {}", digits));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", ok));
    module.body().line("else");
    emit_scan_numeric_exponent(var, &idx, &end, &byte, &exp_digits, &ok, module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", ok));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", ok));
}

fn emit_trim_numeric_leading(
    var: &str,
    idx: &str,
    end: &str,
    byte: &str,
    module: &mut WasmModule,
) {
    let loop_label = module.next_label("numeric_ltrim_loop");
    let done_label = module.next_label("numeric_ltrim_done");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_string_byte(var, idx, module);
    module.body().line(&format!("local.set {}", byte));
    emit_byte_class_condition(byte, ByteClass::Space, module);
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_trim_numeric_trailing(
    var: &str,
    idx: &str,
    end: &str,
    last: &str,
    byte: &str,
    module: &mut WasmModule,
) {
    let loop_label = module.next_label("numeric_rtrim_loop");
    let done_label = module.next_label("numeric_rtrim_done");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.le_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", last));
    emit_load_string_byte(var, last, module);
    module.body().line(&format!("local.set {}", byte));
    emit_byte_class_condition(byte, ByteClass::Space, module);
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", last));
    module.body().line(&format!("local.set {}", end));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_skip_numeric_sign(
    var: &str,
    idx: &str,
    end: &str,
    byte: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.lt_u");
    module.body().open("if");
    emit_load_string_byte(var, idx, module);
    module.body().line(&format!("local.set {}", byte));
    emit_byte_equals(byte, 43, module);
    emit_byte_equals(byte, 45, module);
    module.body().line("i32.or");
    module.body().open("if");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().close("end");
    module.body().close("end");
}

fn emit_scan_numeric_fraction(
    var: &str,
    idx: &str,
    end: &str,
    byte: &str,
    digits: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.lt_u");
    module.body().open("if");
    emit_load_string_byte(var, idx, module);
    module.body().line(&format!("local.set {}", byte));
    emit_byte_equals(byte, 46, module);
    module.body().open("if");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    emit_scan_numeric_digits(var, idx, end, byte, digits, module);
    module.body().close("end");
    module.body().close("end");
}

fn emit_scan_numeric_exponent(
    var: &str,
    idx: &str,
    end: &str,
    byte: &str,
    exp_digits: &str,
    ok: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.lt_u");
    module.body().open("if");
    emit_load_string_byte(var, idx, module);
    module.body().line(&format!("local.set {}", byte));
    emit_byte_equals(byte, 69, module);
    emit_byte_equals(byte, 101, module);
    module.body().line("i32.or");
    module.body().open("if");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    emit_skip_numeric_sign(var, idx, end, byte, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", exp_digits));
    emit_scan_numeric_digits(var, idx, end, byte, exp_digits, module);
    module.body().line(&format!("local.get {}", exp_digits));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", ok));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

fn emit_scan_numeric_digits(
    var: &str,
    idx: &str,
    end: &str,
    byte: &str,
    digits: &str,
    module: &mut WasmModule,
) {
    let loop_label = module.next_label("numeric_digit_loop");
    let done_label = module.next_label("numeric_digit_done");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_string_byte(var, idx, module);
    module.body().line(&format!("local.set {}", byte));
    emit_ascii_digit_condition(byte, module);
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", digits));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_byte_equals(byte: &str, expected: i32, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", byte));
    module.body().line(&format!("i32.const {}", expected));
    module.body().line("i32.eq");
}

pub(super) fn emit_float_predicate_call(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let [arg] = args else {
        return Err(CompileError::new(
            call.span,
            &format!("wasm32-web {}() expects exactly one argument", name),
        ));
    };
    if !expression_is_floaty(arg, module) {
        match emit_expr(arg, module)? {
            ValueKind::Int => {
                module.body().line("drop");
                module.body().line(&format!(
                    "i32.const {}",
                    i32::from(name.eq_ignore_ascii_case("is_finite"))
                ));
                return Ok(ValueKind::Bool);
            }
            _ => {
                return Err(CompileError::new(
                    arg.span,
                    "wasm32-web float predicates currently require int or float inputs",
                ));
            }
        }
    }

    let temp = module.next_label("float_pred").trim_start_matches('$').to_string();
    module.declare_f64_local(temp.clone());
    require_float(arg, module)?;
    module.body().line(&format!("local.set ${}", temp));
    match name.to_ascii_lowercase().as_str() {
        "is_nan" => {
            module.body().line(&format!("local.get ${}", temp));
            module.body().line(&format!("local.get ${}", temp));
            module.body().line("f64.ne");
        }
        "is_finite" => {
            module.body().line(&format!("local.get ${}", temp));
            module.body().line("f64.abs");
            module.body().line("f64.const inf");
            module.body().line("f64.lt");
        }
        "is_infinite" => {
            module.body().line(&format!("local.get ${}", temp));
            module.body().line("f64.abs");
            module.body().line("f64.const inf");
            module.body().line("f64.eq");
        }
        _ => unreachable!(),
    }
    Ok(ValueKind::Bool)
}

fn string_literal_is_numeric(value: &str) -> bool {
    let bytes = value.trim().as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut idx = 0;
    if matches!(bytes[idx], b'+' | b'-') {
        idx += 1;
    }
    let mut digits = false;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        digits = true;
        idx += 1;
    }
    if idx < bytes.len() && bytes[idx] == b'.' {
        idx += 1;
        while idx < bytes.len() && bytes[idx].is_ascii_digit() {
            digits = true;
            idx += 1;
        }
    }
    if !digits {
        return false;
    }
    if idx < bytes.len() && matches!(bytes[idx], b'e' | b'E') {
        idx += 1;
        if idx < bytes.len() && matches!(bytes[idx], b'+' | b'-') {
            idx += 1;
        }
        let exp_start = idx;
        while idx < bytes.len() && bytes[idx].is_ascii_digit() {
            idx += 1;
        }
        if idx == exp_start {
            return false;
        }
    }
    idx == bytes.len()
}
