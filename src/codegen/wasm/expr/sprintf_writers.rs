//! Purpose:
//! Provides low-level wasm writer helpers for printf()/sprintf() formatting.
//! Keeps padding, precision, radix, and scalar string writes out of the main dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::sprintf`
//!
//! Key details:
//! - Helpers emit wasm text for browser output and runtime string byte-length accounting.

use super::*;

pub(super) fn materialize_sprintf_prefixed_positive_number(
    var: &str,
    prefix: u8,
    module: &mut WasmModule,
) -> String {
    let local = module.next_label("sprintf_signed_number").trim_start_matches('$').to_string();
    let out_idx = module.next_label("sprintf_signed_idx");
    let needs_prefix = module.next_label("sprintf_signed_needs_prefix");
    module.declare_i32_local(format!("{}_ptr", local));
    module.declare_i32_local(format!("{}_len", local));
    module.declare_i32_local(out_idx.trim_start_matches('$').to_string());
    module.declare_i32_local(needs_prefix.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line("i32.load8_u");
    module.body().line("i32.const 45");
    module.body().line("i32.ne");
    module.body().line(&format!("local.set {}", needs_prefix));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get {}", needs_prefix));
    module.body().line("i32.add");
    module.body().line(&format!("local.set ${}_len", local));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set ${}_ptr", local));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get ${}_len", local));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
    module.body().line(&format!("local.get {}", needs_prefix));
    module.body().open("if");
    emit_store_byte_const(prefix.into(), &format!("${}_ptr", local), &out_idx, module);
    module.body().close("end");
    emit_copy_string_to_memory(var, &format!("${}_ptr", local), &out_idx, module);
    local
}

pub(super) fn materialize_sprintf_zero_padded_number(
    var: &str,
    width: usize,
    module: &mut WasmModule,
) -> String {
    let local = module.next_label("sprintf_zero_number").trim_start_matches('$').to_string();
    let out_idx = module.next_label("sprintf_zero_idx");
    let start = module.next_label("sprintf_zero_start");
    let end = module.next_label("sprintf_zero_end");
    let pad = module.next_label("sprintf_zero_pad");
    let has_sign = module.next_label("sprintf_zero_has_sign");
    let first = module.next_label("sprintf_zero_first");
    module.declare_i32_local(format!("{}_ptr", local));
    module.declare_i32_local(format!("{}_len", local));
    for local_name in [&out_idx, &start, &end, &pad, &has_sign, &first] {
        module.declare_i32_local(local_name.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("i32.const {}", width));
    module.body().line("i32.lt_u");
    module.body().open("if");
    module.body().line(&format!("i32.const {}", width));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", pad));
    module.body().line("else");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pad));
    module.body().close("end");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get {}", pad));
    module.body().line("i32.add");
    module.body().line(&format!("local.set ${}_len", local));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", first));
    module.body().line(&format!("local.get {}", first));
    module.body().line("i32.const 45");
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", first));
    module.body().line("i32.const 43");
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().line(&format!("local.get {}", first));
    module.body().line("i32.const 32");
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", has_sign));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set ${}_ptr", local));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get ${}_len", local));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", start));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.set {}", end));
    module.body().line(&format!("local.get {}", has_sign));
    module.body().open("if");
    emit_store_byte_local(&first, &format!("${}_ptr", local), &out_idx, module);
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", start));
    module.body().close("end");
    emit_copy_zeros_to_memory(&pad, &format!("${}_ptr", local), &out_idx, module);
    emit_copy_string_range_to_memory(var, &start, &end, &format!("${}_ptr", local), &out_idx, module);
    local
}

pub(super) fn emit_sprintf_radix_or_padded_arg(
    arg: &Expr,
    radix: i64,
    uppercase: bool,
    width: Option<usize>,
    left_align: bool,
    byte_len: Option<&str>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if width.is_none() && !left_align {
        emit_sprintf_unsigned_radix_arg(arg, radix, uppercase, byte_len, module)?;
        return Ok(());
    }
    let var = materialize_unsigned_radix_string_expr(
        arg,
        radix,
        uppercase,
        "sprintf_radix_arg",
        module,
    )?;
    emit_write_sprintf_string_var(&var, None, width, left_align, byte_len, module);
    Ok(())
}

pub(super) fn emit_write_sprintf_string_var(
    var: &str,
    precision: Option<usize>,
    width: Option<usize>,
    left_align: bool,
    byte_len: Option<&str>,
    module: &mut WasmModule,
) {
    if precision.is_none() && width.is_none() {
        emit_write_string_var(var, module);
        if let Some(byte_len) = byte_len {
            module.body().line(&format!("local.get {}", byte_len));
            module.body().line(&format!("local.get ${}_len", var));
            module.body().line("i64.extend_i32_u");
            module.body().line("i64.add");
            module.body().line(&format!("local.set {}", byte_len));
        }
        return;
    }
    let start = module.next_label("sprintf_precision_start");
    let end = module.next_label("sprintf_precision_end");
    let pad = module.next_label("sprintf_width_pad");
    module.declare_i32_local(start.trim_start_matches('$').to_string());
    module.declare_i32_local(end.trim_start_matches('$').to_string());
    module.declare_i32_local(pad.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", start));
    emit_sprintf_segment_len(var, precision, module);
    module.body().line(&format!("local.set {}", end));
    emit_sprintf_width_pad_len(&end, width, &pad, module);
    if !left_align {
        emit_write_sprintf_spaces(&pad, byte_len, module);
    }
    emit_write_string_range(var, &start, &end, module);
    if let Some(byte_len) = byte_len {
        module.body().line(&format!("local.get {}", byte_len));
        module.body().line(&format!("local.get {}", end));
        module.body().line("i64.extend_i32_u");
        module.body().line("i64.add");
        module.body().line(&format!("local.set {}", byte_len));
    }
    if left_align {
        emit_write_sprintf_spaces(&pad, byte_len, module);
    }
}

pub(super) fn emit_write_sprintf_spaces(count: &str, byte_len: Option<&str>, module: &mut WasmModule) {
    let written = module.next_label("sprintf_space_written");
    let loop_label = module.next_label("sprintf_space_loop");
    let done_label = module.next_label("sprintf_space_done");
    module.declare_i32_local(written.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", written));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", written));
    module.body().line(&format!("local.get {}", count));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_write_literal_text(" ", module);
    if let Some(byte_len) = byte_len {
        module.body().line(&format!("local.get {}", byte_len));
        module.body().line("i64.const 1");
        module.body().line("i64.add");
        module.body().line(&format!("local.set {}", byte_len));
    }
    module.body().line(&format!("local.get {}", written));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", written));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn precision_truncated_str(value: &str, precision: Option<usize>) -> &str {
    let Some(precision) = precision else {
        return value;
    };
    if precision >= value.len() {
        return value;
    }
    let end = value
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= precision)
        .last()
        .unwrap_or(0);
    &value[..end]
}

pub(super) fn format_sprintf_static_string_arg(
    value: &str,
    precision: Option<usize>,
    width: Option<usize>,
    left_align: bool,
) -> String {
    let mut formatted = precision_truncated_str(value, precision).to_string();
    let Some(width) = width else {
        return formatted;
    };
    if formatted.len() >= width {
        return formatted;
    }
    let padding = " ".repeat(width - formatted.len());
    if left_align {
        formatted.push_str(&padding);
        formatted
    } else {
        format!("{}{}", padding, formatted)
    }
}

pub(super) fn emit_write_bool_string_var(var: &str, byte_len: Option<&str>, module: &mut WasmModule) {
    module.body().line(&format!("local.get ${}", var));
    module.body().open("if");
    emit_write_literal_text("1", module);
    if let Some(byte_len) = byte_len {
        module.body().line(&format!("local.get {}", byte_len));
        module.body().line("i64.const 1");
        module.body().line("i64.add");
        module.body().line(&format!("local.set {}", byte_len));
    }
    module.body().close("end");
}

pub(super) fn emit_write_float_string_var(var: &str, byte_len: Option<&str>, module: &mut WasmModule) {
    let out_ptr = module.next_label("printf_float_string_ptr");
    let out_len = module.next_label("printf_float_string_len");
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(out_len.trim_start_matches('$').to_string());
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line("i32.const 64");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get ${}", var));
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("call $host_float_to_string");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("call $host_write");
    if let Some(byte_len) = byte_len {
        module.body().line(&format!("local.get {}", byte_len));
        module.body().line(&format!("local.get {}", out_len));
        module.body().line("i64.extend_i32_u");
        module.body().line("i64.add");
        module.body().line(&format!("local.set {}", byte_len));
    }
}

pub(super) fn emit_sprintf_char_arg(
    arg: &Expr,
    byte_len: Option<&str>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(var) = runtime_int_variable_arg(arg, module) {
        module.body().line(&format!("local.get ${}", var));
        module.body().line("i32.wrap_i64");
    } else if let Some(var) = runtime_float_variable_arg(arg, module) {
        module.body().line(&format!("local.get ${}", var));
        module.body().line("i64.trunc_f64_s");
        module.body().line("i32.wrap_i64");
    } else if let Some(var) = runtime_bool_variable_arg(arg, module) {
        module.body().line(&format!("local.get ${}", var));
    } else {
        module
            .body()
            .line(&format!("i32.const {}", literal_format_int_arg(arg)? as u8));
    }
    module.body().line("i32.const 255");
    module.body().line("i32.and");
    emit_write_stack_byte(module);
    if let Some(byte_len) = byte_len {
        module.body().line(&format!("local.get {}", byte_len));
        module.body().line("i64.const 1");
        module.body().line("i64.add");
        module.body().line(&format!("local.set {}", byte_len));
    }
    Ok(())
}

pub(super) fn emit_sprintf_unsigned_radix_arg(
    arg: &Expr,
    radix: i64,
    uppercase: bool,
    byte_len: Option<&str>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let number = module.next_label("printf_radix_number");
    let divisor = module.next_label("printf_radix_divisor");
    let digit = module.next_label("printf_radix_digit");
    let loop_label = module.next_label("printf_radix_loop");
    let done_label = module.next_label("printf_radix_done");
    let divisor_loop = module.next_label("printf_radix_divisor_loop");
    let divisor_done = module.next_label("printf_radix_divisor_done");
    module.declare_i64_local(number.trim_start_matches('$').to_string());
    module.declare_i64_local(divisor.trim_start_matches('$').to_string());
    module.declare_i64_local(digit.trim_start_matches('$').to_string());
    if let Some(var) = runtime_int_variable_arg(arg, module) {
        module.body().line(&format!("local.get ${}", var));
    } else if let Some(var) = runtime_float_variable_arg(arg, module) {
        module.body().line(&format!("local.get ${}", var));
        module.body().line("i64.trunc_f64_s");
    } else if let Some(var) = runtime_bool_variable_arg(arg, module) {
        module.body().line(&format!("local.get ${}", var));
        module.body().line("i64.extend_i32_s");
    } else {
        module
            .body()
            .line(&format!("i64.const {}", literal_format_int_arg(arg)?));
    }
    module.body().line(&format!("local.set {}", number));
    module.body().line(&format!("local.get {}", number));
    module.body().line("i64.eqz");
    module.body().open("if");
    module.body().line("i32.const 48");
    emit_write_stack_byte(module);
    if let Some(byte_len) = byte_len {
        module.body().line(&format!("local.get {}", byte_len));
        module.body().line("i64.const 1");
        module.body().line("i64.add");
        module.body().line(&format!("local.set {}", byte_len));
    }
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
    emit_write_stack_byte(module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", digit));
    module.body().line(&format!("i64.const {}", if uppercase { 55 } else { 87 }));
    module.body().line("i64.add");
    module.body().line("i32.wrap_i64");
    emit_write_stack_byte(module);
    module.body().close("end");
    if let Some(byte_len) = byte_len {
        module.body().line(&format!("local.get {}", byte_len));
        module.body().line("i64.const 1");
        module.body().line("i64.add");
        module.body().line(&format!("local.set {}", byte_len));
    }
    module.body().line(&format!("local.get {}", divisor));
    module.body().line(&format!("i64.const {}", radix));
    module.body().line("i64.div_u");
    module.body().line(&format!("local.set {}", divisor));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_add_runtime_int_byte_len(var: &str, byte_len: &str, module: &mut WasmModule) {
    let number = module.next_label("printf_num");
    let digits = module.next_label("printf_digits");
    let loop_label = module.next_label("printf_len_loop");
    let done_label = module.next_label("printf_len_done");
    module.declare_i64_local(number.trim_start_matches('$').to_string());
    module.declare_i64_local(digits.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}", var));
    module.body().line(&format!("local.set {}", number));
    module.body().line("i64.const 1");
    module.body().line(&format!("local.set {}", digits));
    module.body().line(&format!("local.get {}", number));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().open("if");
    module.body().line(&format!("local.get {}", digits));
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", digits));
    module.body().close("end");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", number));
    module.body().line("i64.const -10");
    module.body().line("i64.le_s");
    module.body().line(&format!("local.get {}", number));
    module.body().line("i64.const 10");
    module.body().line("i64.ge_s");
    module.body().line("i32.or");
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", number));
    module.body().line("i64.const 10");
    module.body().line("i64.div_s");
    module.body().line(&format!("local.set {}", number));
    module.body().line(&format!("local.get {}", digits));
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", digits));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", byte_len));
    module.body().line(&format!("local.get {}", digits));
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", byte_len));
}

