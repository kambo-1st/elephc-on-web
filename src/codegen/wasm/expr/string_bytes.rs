//! Purpose:
//! Provides shared wasm32-web byte, text, and encoded-character emission helpers.
//! Keeps low-level string byte primitives out of the main expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` and focused string/array output helpers.
//!
//! Key details:
//! - Helpers are emission-only and preserve the caller's existing control flow.
//! - Byte writers are shared by string runtime, JSON, sprintf, path, padding, and search lowerings.

use super::*;

pub(super) fn emit_load_string_byte(var: &str, index_local: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", index_local));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
}

pub(super) fn emit_load_string_byte_at_delta(
    var: &str,
    index_local: &str,
    delta: i32,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", index_local));
    module.body().line(&format!("i32.const {}", delta));
    module.body().line("i32.add");
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
}

pub(super) fn emit_optional_string_byte(
    var: &str,
    index_local: &str,
    delta: i32,
    target_local: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", index_local));
    module.body().line(&format!("i32.const {}", delta));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", index_local));
    module.body().line(&format!("i32.const {}", delta));
    module.body().line("i32.add");
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("else");
    module.body().line("i32.const 0");
    module.body().close("end");
    module.body().line(&format!("local.set {}", target_local));
}

pub(super) fn emit_ascii_case_value(byte_local: &str, case: AsciiCase, module: &mut WasmModule) {
    let (lower, upper, delta, op) = match case {
        AsciiCase::Lower => (65, 90, 32, "i32.add"),
        AsciiCase::Upper => (97, 122, 32, "i32.sub"),
    };
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line(&format!("i32.const {}", lower));
    module.body().line("i32.ge_u");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line(&format!("i32.const {}", upper));
    module.body().line("i32.le_u");
    module.body().line("i32.and");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line(&format!("i32.const {}", delta));
    module.body().line(op);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().close("end");
}

pub(super) fn emit_addslashes_escape_condition(byte_local: &str, module: &mut WasmModule) {
    for byte in [0, 34, 39, 92] {
        module.body().line(&format!("local.get {}", byte_local));
        module.body().line(&format!("i32.const {}", byte));
        module.body().line("i32.eq");
    }
    for _ in 1..4 {
        module.body().line("i32.or");
    }
}

pub(super) fn emit_write_hex_nibble(module: &mut WasmModule) {
    let nibble = module.next_label("hex_nibble");
    module.declare_i32_local(nibble.trim_start_matches('$').to_string());
    module.body().line(&format!("local.set {}", nibble));
    emit_hex_nibble_byte(&nibble, module);
    emit_write_stack_byte(module);
}

pub(super) fn emit_hex_nibble_byte(nibble: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", nibble));
    module.body().line("i32.const 10");
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", nibble));
    module.body().line("i32.const 48");
    module.body().line("i32.add");
    module.body().line("else");
    module.body().line(&format!("local.get {}", nibble));
    module.body().line("i32.const 87");
    module.body().line("i32.add");
    module.body().close("end");
}

pub(super) fn emit_write_base64_index(table_ptr: usize, module: &mut WasmModule) {
    emit_base64_index_byte(table_ptr, module);
    emit_write_stack_byte(module);
}

pub(super) fn emit_base64_index_byte(table_ptr: usize, module: &mut WasmModule) {
    module.body().line(&format!("i32.const {}", table_ptr));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
}

pub(super) fn emit_base64_value_or_padding(byte_local: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 61");
    module.body().line("i32.eq");
    module.body().open("if (result i32)");
    module.body().line("i32.const 64");
    module.body().line("else");
    emit_base64_value(byte_local, module);
    module.body().close("end");
}

pub(super) fn emit_base64_whitespace_condition(byte_local: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 9");
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 10");
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 13");
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 32");
    module.body().line("i32.eq");
    module.body().line("i32.or");
}

pub(super) fn emit_base64_data_condition(byte_local: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 65");
    module.body().line("i32.ge_u");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 90");
    module.body().line("i32.le_u");
    module.body().line("i32.and");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 97");
    module.body().line("i32.ge_u");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 122");
    module.body().line("i32.le_u");
    module.body().line("i32.and");
    module.body().line("i32.or");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 48");
    module.body().line("i32.ge_u");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 57");
    module.body().line("i32.le_u");
    module.body().line("i32.and");
    module.body().line("i32.or");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 43");
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 47");
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 61");
    module.body().line("i32.eq");
    module.body().line("i32.or");
}

pub(super) fn emit_base64_value(byte_local: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 65");
    module.body().line("i32.ge_u");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 90");
    module.body().line("i32.le_u");
    module.body().line("i32.and");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 65");
    module.body().line("i32.sub");
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 97");
    module.body().line("i32.ge_u");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 122");
    module.body().line("i32.le_u");
    module.body().line("i32.and");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 71");
    module.body().line("i32.sub");
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 48");
    module.body().line("i32.ge_u");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 57");
    module.body().line("i32.le_u");
    module.body().line("i32.and");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 4");
    module.body().line("i32.add");
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 43");
    module.body().line("i32.eq");
    module.body().open("if (result i32)");
    module.body().line("i32.const 62");
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 47");
    module.body().line("i32.eq");
    module.body().open("if (result i32)");
    module.body().line("i32.const 63");
    module.body().line("else");
    module.body().line("unreachable");
    module.body().line("i32.const 0");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_output_string_index(
    expr: &Expr,
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_string_index_to_stack(expr, array, index, module)?;
    module.body().line("call $host_write");
    Ok(())
}

pub(super) fn emit_string_index_to_stack(
    expr: &Expr,
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if is_array_value_expr(array) {
        return Err(array_unsupported(expr));
    }
    if let Some(value) = static_string_value(expr, module) {
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(());
    }
    let Some(var) = runtime_string_arg_or_materialize(array, "string_index_source", module)? else {
        return Err(CompileError::new(
            array.span,
            "wasm32-web string indexing currently requires a string expression",
        ));
    };
    let idx64 = module.next_label("string_index_i64");
    let result_ptr = module.next_label("string_index_ptr");
    let result_len = module.next_label("string_index_len");
    module.declare_i64_local(idx64.trim_start_matches('$').to_string());
    module.declare_i32_local(result_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(result_len.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", result_ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", result_len));
    require_int(index, module)?;
    module.body().line(&format!("local.set {}", idx64));
    module.body().line(&format!("local.get {}", idx64));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i64.extend_i32_u");
    module.body().line(&format!("local.get {}", idx64));
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", idx64));
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx64));
    module.body().line("i64.const 0");
    module.body().line("i64.ge_s");
    module.body().line(&format!("local.get {}", idx64));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i64.extend_i32_u");
    module.body().line("i64.lt_s");
    module.body().line("i32.and");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", idx64));
    module.body().line("i32.wrap_i64");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", result_ptr));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", result_len));
    module.body().close("end");
    module.body().line(&format!("local.get {}", result_ptr));
    module.body().line(&format!("local.get {}", result_len));
    Ok(())
}

pub(super) fn emit_write_pad_bytes(pad: &str, count_local: &str, module: &mut WasmModule) {
    let written = module.next_label("pad_written");
    let pad_index = module.next_label("pad_index");
    let loop_label = module.next_label("pad_loop");
    let done_label = module.next_label("pad_done");
    let (pad_ptr, pad_len) = module.intern_string(pad);
    module.declare_i32_local(written.trim_start_matches('$').to_string());
    module.declare_i32_local(pad_index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", written));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pad_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", written));
    module.body().line(&format!("local.get {}", count_local));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("i32.const {}", pad_ptr));
    module.body().line(&format!("local.get {}", pad_index));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    emit_write_stack_byte(module);
    module.body().line(&format!("local.get {}", written));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", written));
    module.body().line(&format!("local.get {}", pad_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("i32.const {}", pad_len));
    module.body().line("i32.rem_u");
    module.body().line(&format!("local.set {}", pad_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_write_pad_bytes_var(pad_var: &str, count_local: &str, module: &mut WasmModule) {
    let written = module.next_label("pad_written");
    let pad_index = module.next_label("pad_index");
    let loop_label = module.next_label("pad_loop");
    let done_label = module.next_label("pad_done");
    module.declare_i32_local(written.trim_start_matches('$').to_string());
    module.declare_i32_local(pad_index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", written));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pad_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", written));
    module.body().line(&format!("local.get {}", count_local));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", pad_var));
    module.body().line(&format!("local.get {}", pad_index));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    emit_write_stack_byte(module);
    module.body().line(&format!("local.get {}", written));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", written));
    module.body().line(&format!("local.get {}", pad_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", pad_var));
    module.body().line("i32.rem_u");
    module.body().line(&format!("local.set {}", pad_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_write_percent_encoded_byte(byte_local: &str, module: &mut WasmModule) {
    module.body().line("i32.const 37");
    emit_write_stack_byte(module);
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 4");
    module.body().line("i32.shr_u");
    emit_write_upper_hex_nibble(module);
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 15");
    module.body().line("i32.and");
    emit_write_upper_hex_nibble(module);
}

pub(super) fn emit_write_upper_hex_nibble(module: &mut WasmModule) {
    let nibble = module.next_label("hex_nibble");
    module.declare_i32_local(nibble.trim_start_matches('$').to_string());
    module.body().line(&format!("local.set {}", nibble));
    emit_upper_hex_nibble_byte(&nibble, module);
    emit_write_stack_byte(module);
}

pub(super) fn emit_upper_hex_nibble_byte(nibble: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", nibble));
    module.body().line("i32.const 10");
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", nibble));
    module.body().line("i32.const 48");
    module.body().line("i32.add");
    module.body().line("else");
    module.body().line(&format!("local.get {}", nibble));
    module.body().line("i32.const 55");
    module.body().line("i32.add");
    module.body().close("end");
}

pub(super) fn emit_url_safe_condition(byte_local: &str, space: SpaceEncoding, module: &mut WasmModule) {
    emit_ascii_alpha_condition(byte_local, module);
    emit_ascii_digit_condition(byte_local, module);
    module.body().line("i32.or");
    for byte in [45, 95, 46] {
        module.body().line(&format!("local.get {}", byte_local));
        module.body().line(&format!("i32.const {}", byte));
        module.body().line("i32.eq");
        module.body().line("i32.or");
    }
    if matches!(space, SpaceEncoding::Percent20) {
        module.body().line(&format!("local.get {}", byte_local));
        module.body().line("i32.const 126");
        module.body().line("i32.eq");
        module.body().line("i32.or");
    }
}

pub(super) fn emit_hex_digit_condition(byte_local: &str, module: &mut WasmModule) {
    emit_ascii_digit_condition(byte_local, module);
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 65");
    module.body().line("i32.ge_u");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 70");
    module.body().line("i32.le_u");
    module.body().line("i32.and");
    module.body().line("i32.or");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 97");
    module.body().line("i32.ge_u");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 102");
    module.body().line("i32.le_u");
    module.body().line("i32.and");
    module.body().line("i32.or");
}

pub(super) fn emit_hex_digit_value(byte_local: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 57");
    module.body().line("i32.le_u");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 48");
    module.body().line("i32.sub");
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 95");
    module.body().line("i32.gt_u");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 87");
    module.body().line("i32.sub");
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 55");
    module.body().line("i32.sub");
    module.body().close("end");
    module.body().close("end");
}


pub(super) fn emit_write_stack_byte(module: &mut WasmModule) {
    let byte = module.next_label("write_byte");
    let scratch = module.scratch_byte_offset();
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    module.body().line(&format!("local.set {}", byte));
    module.body().line(&format!("i32.const {}", scratch));
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.store8");
    module.body().line(&format!("i32.const {}", scratch));
    module.body().line("i32.const 1");
    module.body().line("call $host_write");
}

pub(super) fn emit_increment_local(local: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", local));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", local));
}

pub(super) fn emit_store_byte_const(value: i32, out_ptr: &str, out_idx: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.add");
    module.body().line(&format!("i32.const {}", value));
    module.body().line("i32.store8");
    emit_increment_local(out_idx, module);
}

pub(super) fn emit_store_byte_local(byte: &str, out_ptr: &str, out_idx: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.store8");
    emit_increment_local(out_idx, module);
}

pub(super) fn emit_store_percent_encoded_byte(byte: &str, out_ptr: &str, out_idx: &str, module: &mut WasmModule) {
    let nibble = module.next_label("percent_nibble");
    module.declare_i32_local(nibble.trim_start_matches('$').to_string());
    emit_store_byte_const(37, out_ptr, out_idx, module);
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 4");
    module.body().line("i32.shr_u");
    module.body().line(&format!("local.set {}", nibble));
    emit_upper_hex_nibble_byte(&nibble, module);
    module.body().line("i32.store8");
    emit_increment_local(out_idx, module);
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 15");
    module.body().line("i32.and");
    module.body().line(&format!("local.set {}", nibble));
    emit_upper_hex_nibble_byte(&nibble, module);
    module.body().line("i32.store8");
    emit_increment_local(out_idx, module);
}

pub(super) fn emit_write_static_string(ptr: usize, len: usize, module: &mut WasmModule) {
    module.body().line(&format!("i32.const {}", ptr));
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $host_write");
}

pub(super) fn emit_write_string_var(var: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("call $host_write");
}

pub(super) fn emit_write_string_range(var: &str, start_local: &str, end_local: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", start_local));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", end_local));
    module.body().line(&format!("local.get {}", start_local));
    module.body().line("i32.sub");
    module.body().line("call $host_write");
}

pub(super) fn emit_write_literal_text(text: &str, module: &mut WasmModule) {
    let (ptr, len) = module.intern_string(text);
    emit_write_static_string(ptr, len, module);
}
