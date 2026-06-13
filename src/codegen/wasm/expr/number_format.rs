//! Purpose:
//! Lowers number_format() string/output emission for wasm32-web.
//! Owns literal evaluation plus runtime integer and host-backed float formatting paths.
//!
//! Called from:
//! - `crate::codegen::wasm::expr`
//!
//! Key details:
//! - Keeps separator/decimal argument normalization with the code that consumes it.

use super::*;

pub(super) fn emit_number_format_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !name.eq_ignore_ascii_case("number_format") {
        return Ok(false);
    }
    if args.is_empty() || args.len() > 4 {
        return Ok(false);
    }
    let decimals = runtime_number_format_decimals(call, args.get(1), module)?;
    let dec_point = runtime_number_format_separator(call, args.get(2), ".", module)?;
    let thousands = runtime_number_format_separator(call, args.get(3), ",", module)?;
    if let Some(var) = runtime_number_format_int_arg_local(&args[0], "number_format_value_arg", module) {
        emit_runtime_number_format_int_value_to_stack(&var, decimals, dec_point, thousands, module);
        return Ok(true);
    }
    if expression_is_floaty(&args[0], module) {
        emit_runtime_number_format_float_value_to_stack(
            &args[0], decimals, dec_point, thousands, module,
        )?;
        return Ok(true);
    }
    Ok(false)
}

fn emit_runtime_number_format_int_value_to_stack(
    var: &str,
    decimals: RuntimeNumberFormatDecimals<'_>,
    dec_point: RuntimeStringSeparator<'_>,
    thousands: RuntimeStringSeparator<'_>,
    module: &mut WasmModule,
) {
    let out_ptr = module.next_label("number_format_value_ptr");
    let out_len = module.next_label("number_format_value_len");
    let negative = module.next_label("number_format_value_negative");
    let digits = module.next_label("number_format_value_digits");
    let divisor = module.next_label("number_format_value_divisor");
    let remaining = module.next_label("number_format_value_remaining");
    let digit = module.next_label("number_format_value_digit");
    let byte = module.next_label("number_format_value_byte");
    let decimal_remaining = module.next_label("number_format_value_decimal_remaining");
    let decimal_loop = module.next_label("number_format_value_decimal_loop");
    let decimal_done = module.next_label("number_format_value_decimal_done");
    let count_loop = module.next_label("number_format_value_count_loop");
    let count_done = module.next_label("number_format_value_count_done");
    let divisor_loop = module.next_label("number_format_value_divisor_loop");
    let divisor_done = module.next_label("number_format_value_divisor_done");
    let write_loop = module.next_label("number_format_value_write_loop");
    let write_done = module.next_label("number_format_value_write_done");
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(out_len.trim_start_matches('$').to_string());
    module.declare_i64_local(negative.trim_start_matches('$').to_string());
    module.declare_i64_local(digits.trim_start_matches('$').to_string());
    module.declare_i64_local(divisor.trim_start_matches('$').to_string());
    module.declare_i64_local(remaining.trim_start_matches('$').to_string());
    module.declare_i64_local(digit.trim_start_matches('$').to_string());
    module.declare_i64_local(decimal_remaining.trim_start_matches('$').to_string());
    module.declare_i32_local(byte.trim_start_matches('$').to_string());

    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line("i32.const 128");
    module.body().line("i32.add");
    match &decimals {
        RuntimeNumberFormatDecimals::Static(decimals) if *decimals > 0 => {
            module.body().line(&format!("i32.const {}", decimals));
            module.body().line("i32.add");
            emit_add_separator_capacity(&dec_point, module);
        }
        RuntimeNumberFormatDecimals::Variable(var) => {
            module.body().line(&format!("local.get ${}", var));
            module.body().line("i64.const 0");
            module.body().line("i64.lt_s");
            module.body().open("if");
            module.body().line("unreachable");
            module.body().close("end");
            module.body().line(&format!("local.get ${}", var));
            module.body().line("i32.wrap_i64");
            module.body().line("i32.add");
            emit_add_separator_capacity(&dec_point, module);
        }
        _ => {}
    }
    emit_add_separator_capacity(&thousands, module);
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_len));

    module.body().line(&format!("local.get ${}", var));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().open("if");
    emit_store_byte_const(45, &out_ptr, &out_len, module);
    module.body().line(&format!("local.get ${}", var));
    module.body().line(&format!("local.set {}", negative));
    module.body().line("else");
    module.body().line("i64.const 0");
    module.body().line(&format!("local.get ${}", var));
    module.body().line("i64.sub");
    module.body().line(&format!("local.set {}", negative));
    module.body().close("end");
    module.body().line("i64.const 1");
    module.body().line(&format!("local.set {}", digits));
    module.body().line(&format!("local.get {}", negative));
    module.body().line(&format!("local.set {}", remaining));
    module.body().open(&format!("block {}", count_done));
    module.body().open(&format!("loop {}", count_loop));
    module.body().line(&format!("local.get {}", remaining));
    module.body().line("i64.const -10");
    module.body().line("i64.gt_s");
    module.body().line(&format!("br_if {}", count_done));
    module.body().line(&format!("local.get {}", remaining));
    module.body().line("i64.const 10");
    module.body().line("i64.div_s");
    module.body().line(&format!("local.set {}", remaining));
    module.body().line(&format!("local.get {}", digits));
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", digits));
    module.body().line(&format!("br {}", count_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line("i64.const 1");
    module.body().line(&format!("local.set {}", divisor));
    module.body().line(&format!("local.get {}", digits));
    module.body().line("i64.const 1");
    module.body().line("i64.sub");
    module.body().line(&format!("local.set {}", remaining));
    module.body().open(&format!("block {}", divisor_done));
    module.body().open(&format!("loop {}", divisor_loop));
    module.body().line(&format!("local.get {}", remaining));
    module.body().line("i64.eqz");
    module.body().line(&format!("br_if {}", divisor_done));
    module.body().line(&format!("local.get {}", divisor));
    module.body().line("i64.const 10");
    module.body().line("i64.mul");
    module.body().line(&format!("local.set {}", divisor));
    module.body().line(&format!("local.get {}", remaining));
    module.body().line("i64.const 1");
    module.body().line("i64.sub");
    module.body().line(&format!("local.set {}", remaining));
    module.body().line(&format!("br {}", divisor_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", digits));
    module.body().line(&format!("local.set {}", remaining));
    module.body().open(&format!("block {}", write_done));
    module.body().open(&format!("loop {}", write_loop));
    module.body().line(&format!("local.get {}", remaining));
    module.body().line("i64.eqz");
    module.body().line(&format!("br_if {}", write_done));
    module.body().line("i64.const 0");
    module.body().line(&format!("local.get {}", negative));
    module.body().line(&format!("local.get {}", divisor));
    module.body().line("i64.div_s");
    module.body().line("i64.const 10");
    module.body().line("i64.rem_s");
    module.body().line("i64.sub");
    module.body().line(&format!("local.set {}", digit));
    module.body().line(&format!("local.get {}", digit));
    module.body().line("i64.const 48");
    module.body().line("i64.add");
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", byte));
    emit_store_byte_local(&byte, &out_ptr, &out_len, module);
    module.body().line(&format!("local.get {}", remaining));
    module.body().line("i64.const 1");
    module.body().line("i64.sub");
    module.body().line(&format!("local.set {}", remaining));
    module.body().line(&format!("local.get {}", remaining));
    module.body().line("i64.eqz");
    module.body().line("i32.eqz");
    module.body().line(&format!("local.get {}", remaining));
    module.body().line("i64.const 3");
    module.body().line("i64.rem_u");
    module.body().line("i64.eqz");
    module.body().line("i32.and");
    module.body().open("if");
    emit_store_runtime_separator_if_nonempty(&thousands, &out_ptr, &out_len, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", divisor));
    module.body().line("i64.const 10");
    module.body().line("i64.div_u");
    module.body().line(&format!("local.set {}", divisor));
    module.body().line(&format!("br {}", write_loop));
    module.body().close("end");
    module.body().close("end");
    match decimals {
        RuntimeNumberFormatDecimals::Static(decimals) if decimals > 0 => {
            emit_store_runtime_separator(&dec_point, &out_ptr, &out_len, module);
            for _ in 0..decimals {
                emit_store_byte_const(48, &out_ptr, &out_len, module);
            }
        }
        RuntimeNumberFormatDecimals::Variable(var) => {
            module.body().line(&format!("local.get ${}", var));
            module.body().line("i64.const 0");
            module.body().line("i64.gt_s");
            module.body().open("if");
            emit_store_runtime_separator(&dec_point, &out_ptr, &out_len, module);
            module.body().line(&format!("local.get ${}", var));
            module.body().line(&format!("local.set {}", decimal_remaining));
            module.body().open(&format!("block {}", decimal_done));
            module.body().open(&format!("loop {}", decimal_loop));
            module.body().line(&format!("local.get {}", decimal_remaining));
            module.body().line("i64.eqz");
            module.body().line(&format!("br_if {}", decimal_done));
            emit_store_byte_const(48, &out_ptr, &out_len, module);
            module.body().line(&format!("local.get {}", decimal_remaining));
            module.body().line("i64.const 1");
            module.body().line("i64.sub");
            module.body().line(&format!("local.set {}", decimal_remaining));
            module.body().line(&format!("br {}", decimal_loop));
            module.body().close("end");
            module.body().close("end");
            module.body().close("end");
        }
        _ => {}
    }
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_len));
}

fn emit_add_separator_capacity(separator: &RuntimeStringSeparator<'_>, module: &mut WasmModule) {
    match separator {
        RuntimeStringSeparator::Literal(value) => {
            module.body().line(&format!("i32.const {}", value.len() * 8));
        }
        RuntimeStringSeparator::Variable(var) => {
            module.body().line(&format!("local.get ${}_len", var));
            module.body().line("i32.const 8");
            module.body().line("i32.mul");
        }
    }
    module.body().line("i32.add");
}

fn emit_runtime_number_format_float_value_to_stack(
    arg: &Expr,
    decimals: RuntimeNumberFormatDecimals<'_>,
    dec_point: RuntimeStringSeparator<'_>,
    thousands: RuntimeStringSeparator<'_>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let out_ptr = module.next_label("number_format_float_value_ptr");
    let out_len = module.next_label("number_format_float_value_len");
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(out_len.trim_start_matches('$').to_string());

    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line("i32.const 768");
    module.body().line("i32.add");
    emit_add_separator_capacity(&dec_point, module);
    emit_add_separator_capacity(&thousands, module);
    module.body().line("global.set $heap");

    require_float(arg, module)?;
    emit_runtime_number_format_decimals_i32(&decimals, module);
    emit_runtime_number_format_separator_ptr_len(&dec_point, module);
    emit_runtime_number_format_separator_ptr_len(&thousands, module);
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("call $host_number_format_float");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_len));
    Ok(())
}

fn emit_runtime_number_format_decimals_i32(
    decimals: &RuntimeNumberFormatDecimals<'_>,
    module: &mut WasmModule,
) {
    match decimals {
        RuntimeNumberFormatDecimals::Static(decimals) => {
            module.body().line(&format!("i32.const {}", decimals));
        }
        RuntimeNumberFormatDecimals::Variable(var) => {
            module.body().line(&format!("local.get ${}", var));
            module.body().line("i64.const 0");
            module.body().line("i64.lt_s");
            module.body().open("if");
            module.body().line("unreachable");
            module.body().close("end");
            module.body().line(&format!("local.get ${}", var));
            module.body().line("i32.wrap_i64");
        }
    }
}

fn emit_runtime_number_format_separator_ptr_len(
    separator: &RuntimeStringSeparator<'_>,
    module: &mut WasmModule,
) {
    match separator {
        RuntimeStringSeparator::Literal(value) => {
            let (ptr, len) = module.intern_string(value);
            module.body().line(&format!("i32.const {}", ptr));
            module.body().line(&format!("i32.const {}", len));
        }
        RuntimeStringSeparator::Variable(var) => {
            module.body().line(&format!("local.get ${}_ptr", var));
            module.body().line(&format!("local.get ${}_len", var));
        }
    }
}

fn emit_store_runtime_separator(
    separator: &RuntimeStringSeparator<'_>,
    out_ptr: &str,
    out_len: &str,
    module: &mut WasmModule,
) {
    match separator {
        RuntimeStringSeparator::Literal(value) => {
            let local = module
                .next_label("number_format_separator")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(format!("{}_ptr", local));
            module.declare_i32_local(format!("{}_len", local));
            let (ptr, len) = module.intern_string(value);
            module.body().line(&format!("i32.const {}", ptr));
            module.body().line(&format!("local.set ${}_ptr", local));
            module.body().line(&format!("i32.const {}", len));
            module.body().line(&format!("local.set ${}_len", local));
            emit_copy_string_to_memory(&local, out_ptr, out_len, module);
        }
        RuntimeStringSeparator::Variable(var) => emit_copy_string_to_memory(var, out_ptr, out_len, module),
    }
}

fn emit_store_runtime_separator_if_nonempty(
    separator: &RuntimeStringSeparator<'_>,
    out_ptr: &str,
    out_len: &str,
    module: &mut WasmModule,
) {
    match separator {
        RuntimeStringSeparator::Literal(value) if value.is_empty() => {}
        _ => emit_store_runtime_separator(separator, out_ptr, out_len, module),
    }
}

#[derive(Clone)]
pub(super) enum RuntimeNumberFormatDecimals<'a> {
    Static(i64),
    Variable(Cow<'a, str>),
}

#[derive(Clone)]
pub(super) enum RuntimeStringSeparator<'a> {
    Literal(Cow<'a, str>),
    Variable(&'a str),
}

pub(super) fn runtime_number_format_decimals<'a>(
    call: &Expr,
    arg: Option<&'a Expr>,
    module: &mut WasmModule,
) -> Result<RuntimeNumberFormatDecimals<'a>, CompileError> {
    let Some(arg) = arg else {
        return Ok(RuntimeNumberFormatDecimals::Static(0));
    };
    if let Some(decimals) = static_int_value(arg) {
        if decimals < 0 {
            return Err(CompileError::new(
                call.span,
                "wasm32-web number_format() decimals must be non-negative",
            ));
        }
        return Ok(RuntimeNumberFormatDecimals::Static(decimals));
    }
    if let ExprKind::BoolLiteral(value) = &arg.kind {
        return Ok(RuntimeNumberFormatDecimals::Static(i64::from(*value)));
    }
    if let Some(decimals) = static_float_decimal_value(arg) {
        if decimals < 0 {
            return Err(CompileError::new(
                call.span,
                "wasm32-web number_format() decimals must be non-negative",
            ));
        }
        return Ok(RuntimeNumberFormatDecimals::Static(decimals));
    }
    if let Some(var) = runtime_int_variable_arg(arg, module) {
        return Ok(RuntimeNumberFormatDecimals::Variable(Cow::Borrowed(var)));
    }
    if let Some(var) = runtime_float_variable_arg(arg, module) {
        let local = module
            .next_label("number_format_decimals_float")
            .trim_start_matches('$')
            .to_string();
        module.declare_i64_local(local.clone());
        module.body().line(&format!("local.get ${}", var));
        module.body().line("f64.trunc");
        module.body().line("i64.trunc_f64_s");
        module.body().line(&format!("local.set ${}", local));
        return Ok(RuntimeNumberFormatDecimals::Variable(Cow::Owned(local)));
    }
    if let Some(var) = runtime_bool_variable_arg(arg, module) {
        let local = module
            .next_label("number_format_decimals_bool")
            .trim_start_matches('$')
            .to_string();
        module.declare_i64_local(local.clone());
        module.body().line(&format!("local.get ${}", var));
        module.body().line("i64.extend_i32_s");
        module.body().line(&format!("local.set ${}", local));
        return Ok(RuntimeNumberFormatDecimals::Variable(Cow::Owned(local)));
    }
    let local = module
        .next_label("number_format_decimals_expr")
        .trim_start_matches('$')
        .to_string();
    module.declare_i64_local(local.clone());
    require_int(arg, module)?;
    module.body().line(&format!("local.set ${}", local));
    return Ok(RuntimeNumberFormatDecimals::Variable(Cow::Owned(local)));
}

fn static_float_decimal_value(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::FloatLiteral(value) => Some(*value as i64),
        ExprKind::Negate(inner) => static_float_decimal_value(inner).map(|value| -value),
        _ => None,
    }
}

pub(super) fn runtime_number_format_int_arg_local(
    arg: &Expr,
    prefix: &str,
    module: &mut WasmModule,
) -> Option<String> {
    if let Some(var) = runtime_int_variable_arg(arg, module) {
        return Some(var.to_string());
    }
    if let Some(var) = runtime_bool_variable_arg(arg, module) {
        let local = module.next_label(prefix).trim_start_matches('$').to_string();
        module.declare_i64_local(local.clone());
        module.body().line(&format!("local.get ${}", var));
        module.body().line("i64.extend_i32_s");
        module.body().line(&format!("local.set ${}", local));
        return Some(local);
    }
    None
}

pub(super) fn runtime_number_format_separator<'a>(
    call: &Expr,
    arg: Option<&'a Expr>,
    default: &'a str,
    module: &WasmModule,
) -> Result<RuntimeStringSeparator<'a>, CompileError> {
    let Some(arg) = arg else {
        return Ok(RuntimeStringSeparator::Literal(Cow::Borrowed(default)));
    };
    if let Some(var) = runtime_string_variable_arg(arg, module) {
        return Ok(RuntimeStringSeparator::Variable(var));
    }
    Ok(RuntimeStringSeparator::Literal(Cow::Owned(static_ascii_string_arg(
        call, arg, module,
    )?)))
}

pub(super) fn emit_runtime_number_format_int(
    var: &str,
    decimals: RuntimeNumberFormatDecimals<'_>,
    dec_point: RuntimeStringSeparator<'_>,
    thousands: RuntimeStringSeparator<'_>,
    module: &mut WasmModule,
) {
    let negative = module.next_label("number_format_negative");
    let digits = module.next_label("number_format_digits");
    let divisor = module.next_label("number_format_divisor");
    let remaining = module.next_label("number_format_remaining");
    let digit = module.next_label("number_format_digit");
    let count_loop = module.next_label("number_format_count_loop");
    let count_done = module.next_label("number_format_count_done");
    let divisor_loop = module.next_label("number_format_divisor_loop");
    let divisor_done = module.next_label("number_format_divisor_done");
    let write_loop = module.next_label("number_format_write_loop");
    let write_done = module.next_label("number_format_write_done");
    module.declare_i64_local(negative.trim_start_matches('$').to_string());
    module.declare_i64_local(digits.trim_start_matches('$').to_string());
    module.declare_i64_local(divisor.trim_start_matches('$').to_string());
    module.declare_i64_local(remaining.trim_start_matches('$').to_string());
    module.declare_i64_local(digit.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}", var));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().open("if");
    module.body().line("i32.const 45");
    emit_write_stack_byte(module);
    module.body().line(&format!("local.get ${}", var));
    module.body().line(&format!("local.set {}", negative));
    module.body().line("else");
    module.body().line("i64.const 0");
    module.body().line(&format!("local.get ${}", var));
    module.body().line("i64.sub");
    module.body().line(&format!("local.set {}", negative));
    module.body().close("end");
    module.body().line("i64.const 1");
    module.body().line(&format!("local.set {}", digits));
    module.body().line(&format!("local.get {}", negative));
    module.body().line(&format!("local.set {}", remaining));
    module.body().open(&format!("block {}", count_done));
    module.body().open(&format!("loop {}", count_loop));
    module.body().line(&format!("local.get {}", remaining));
    module.body().line("i64.const -10");
    module.body().line("i64.gt_s");
    module.body().line(&format!("br_if {}", count_done));
    module.body().line(&format!("local.get {}", remaining));
    module.body().line("i64.const 10");
    module.body().line("i64.div_s");
    module.body().line(&format!("local.set {}", remaining));
    module.body().line(&format!("local.get {}", digits));
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", digits));
    module.body().line(&format!("br {}", count_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line("i64.const 1");
    module.body().line(&format!("local.set {}", divisor));
    module.body().line(&format!("local.get {}", digits));
    module.body().line("i64.const 1");
    module.body().line("i64.sub");
    module.body().line(&format!("local.set {}", remaining));
    module.body().open(&format!("block {}", divisor_done));
    module.body().open(&format!("loop {}", divisor_loop));
    module.body().line(&format!("local.get {}", remaining));
    module.body().line("i64.eqz");
    module.body().line(&format!("br_if {}", divisor_done));
    module.body().line(&format!("local.get {}", divisor));
    module.body().line("i64.const 10");
    module.body().line("i64.mul");
    module.body().line(&format!("local.set {}", divisor));
    module.body().line(&format!("local.get {}", remaining));
    module.body().line("i64.const 1");
    module.body().line("i64.sub");
    module.body().line(&format!("local.set {}", remaining));
    module.body().line(&format!("br {}", divisor_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", digits));
    module.body().line(&format!("local.set {}", remaining));
    module.body().open(&format!("block {}", write_done));
    module.body().open(&format!("loop {}", write_loop));
    module.body().line(&format!("local.get {}", remaining));
    module.body().line("i64.eqz");
    module.body().line(&format!("br_if {}", write_done));
    module.body().line("i64.const 0");
    module.body().line(&format!("local.get {}", negative));
    module.body().line(&format!("local.get {}", divisor));
    module.body().line("i64.div_s");
    module.body().line("i64.const 10");
    module.body().line("i64.rem_s");
    module.body().line("i64.sub");
    module.body().line(&format!("local.set {}", digit));
    module.body().line(&format!("local.get {}", digit));
    module.body().line("i64.const 48");
    module.body().line("i64.add");
    module.body().line("i32.wrap_i64");
    emit_write_stack_byte(module);
    module.body().line(&format!("local.get {}", remaining));
    module.body().line("i64.const 1");
    module.body().line("i64.sub");
    module.body().line(&format!("local.set {}", remaining));
    module.body().line(&format!("local.get {}", remaining));
    module.body().line("i64.eqz");
    module.body().line("i32.eqz");
    module.body().line(&format!("local.get {}", remaining));
    module.body().line("i64.const 3");
    module.body().line("i64.rem_u");
    module.body().line("i64.eqz");
    module.body().line("i32.and");
    module.body().open("if");
    emit_write_runtime_separator_if_nonempty(thousands, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", divisor));
    module.body().line("i64.const 10");
    module.body().line("i64.div_u");
    module.body().line(&format!("local.set {}", divisor));
    module.body().line(&format!("br {}", write_loop));
    module.body().close("end");
    module.body().close("end");
    emit_runtime_number_format_decimals(decimals, dec_point, module);
}

fn emit_write_runtime_separator(separator: RuntimeStringSeparator<'_>, module: &mut WasmModule) {
    match separator {
        RuntimeStringSeparator::Literal(value) => {
            let (ptr, len) = module.intern_string(&value);
            emit_write_static_string(ptr, len, module);
        }
        RuntimeStringSeparator::Variable(var) => emit_write_string_var(var, module),
    }
}

fn emit_write_runtime_separator_if_nonempty(
    separator: RuntimeStringSeparator<'_>,
    module: &mut WasmModule,
) {
    match separator {
        RuntimeStringSeparator::Literal(value) => {
            if !value.is_empty() {
                let (ptr, len) = module.intern_string(&value);
                emit_write_static_string(ptr, len, module);
            }
        }
        RuntimeStringSeparator::Variable(var) => {
            module.body().line(&format!("local.get ${}_len", var));
            module.body().line("i32.eqz");
            module.body().line("i32.eqz");
            module.body().open("if");
            emit_write_string_var(var, module);
            module.body().close("end");
        }
    }
}

fn emit_runtime_number_format_decimals(
    decimals: RuntimeNumberFormatDecimals<'_>,
    dec_point: RuntimeStringSeparator<'_>,
    module: &mut WasmModule,
) {
    match decimals {
        RuntimeNumberFormatDecimals::Static(decimals) => {
            if decimals > 0 {
                emit_write_runtime_separator(dec_point, module);
                for _ in 0..decimals {
                    module.body().line("i32.const 48");
                    emit_write_stack_byte(module);
                }
            }
        }
        RuntimeNumberFormatDecimals::Variable(var) => {
            let remaining = module.next_label("number_format_decimal_remaining");
            let loop_label = module.next_label("number_format_decimal_loop");
            let done_label = module.next_label("number_format_decimal_done");
            module.declare_i64_local(remaining.trim_start_matches('$').to_string());
            module.body().line(&format!("local.get ${}", var));
            module.body().line("i64.const 0");
            module.body().line("i64.lt_s");
            module.body().open("if");
            module.body().line("unreachable");
            module.body().close("end");
            module.body().line(&format!("local.get ${}", var));
            module.body().line("i64.const 0");
            module.body().line("i64.gt_s");
            module.body().open("if");
            emit_write_runtime_separator(dec_point, module);
            module.body().line(&format!("local.get ${}", var));
            module.body().line(&format!("local.set {}", remaining));
            module.body().open(&format!("block {}", done_label));
            module.body().open(&format!("loop {}", loop_label));
            module.body().line(&format!("local.get {}", remaining));
            module.body().line("i64.eqz");
            module.body().line(&format!("br_if {}", done_label));
            module.body().line("i32.const 48");
            emit_write_stack_byte(module);
            module.body().line(&format!("local.get {}", remaining));
            module.body().line("i64.const 1");
            module.body().line("i64.sub");
            module.body().line(&format!("local.set {}", remaining));
            module.body().line(&format!("br {}", loop_label));
            module.body().close("end");
            module.body().close("end");
            module.body().close("end");
        }
    }
}

pub(super) fn eval_literal_number_format(call: &Expr, args: &[Expr]) -> Result<String, CompileError> {
    if args.is_empty() || args.len() > 4 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web number_format() expects one to four literal arguments",
        ));
    }
    let value = literal_number_format_numeric_arg(&args[0])?;
    let decimals = args
        .get(1)
        .map(literal_number_format_decimals_arg)
        .transpose()?
        .unwrap_or(0);
    let dec_point = args.get(2).map(literal_string_arg).transpose()?.unwrap_or(".");
    let thousands = args.get(3).map(literal_string_arg).transpose()?.unwrap_or(",");
    if decimals < 0 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web number_format() literal decimals must be non-negative",
        ));
    }
    let formatted = format!("{:.*}", decimals as usize, value);
    let (sign, digits) = formatted
        .strip_prefix('-')
        .map(|rest| ("-", rest))
        .unwrap_or(("", formatted.as_str()));
    let (whole, fraction) = digits
        .split_once('.')
        .map(|(whole, fraction)| (whole, Some(fraction)))
        .unwrap_or((digits, None));
    let mut grouped = String::new();
    for (index, byte) in whole.bytes().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push_str(&thousands.chars().rev().collect::<String>());
        }
        grouped.push(char::from(byte));
    }
    let whole = grouped.chars().rev().collect::<String>();
    if let Some(fraction) = fraction {
        Ok(format!("{}{}{}{}", sign, whole, dec_point, fraction))
    } else {
        Ok(format!("{}{}", sign, whole))
    }
}

fn literal_number_format_numeric_arg(expr: &Expr) -> Result<f64, CompileError> {
    match &expr.kind {
        ExprKind::BoolLiteral(value) => Ok(if *value { 1.0 } else { 0.0 }),
        _ => literal_numeric_arg(expr),
    }
}

fn literal_number_format_decimals_arg(expr: &Expr) -> Result<i64, CompileError> {
    match &expr.kind {
        ExprKind::BoolLiteral(value) => Ok(i64::from(*value)),
        _ => literal_int_arg(expr),
    }
}
