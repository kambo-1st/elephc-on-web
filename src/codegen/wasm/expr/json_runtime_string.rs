//! Purpose:
//! Emits runtime json_encode() string escaping for the wasm32-web backend.
//! Keeps byte-by-byte JSON string output separate from array/value JSON lowering.
//!
//! Called from:
//! - `super::json_encode` for JSON array keys and string value cells.
//! - `super::scalar_builtins` for direct json_encode() string calls.
//!
//! Key details:
//! - Preserves JSON flags for hex escaping, slash escaping, numeric checks, and zero-fraction output.

use super::*;

pub(super) fn emit_runtime_json_encode_string(var: &str, flags: i64, module: &mut WasmModule) {
    emit_runtime_json_encode_value_string_from_locals(
        &format!("${}_ptr", var),
        &format!("${}_len", var),
        flags,
        module,
    );
}

pub(super) fn emit_runtime_json_encode_value_string_from_locals(
    ptr_local: &str,
    len_local: &str,
    flags: i64,
    module: &mut WasmModule,
) {
    if flags & 32 == 0 {
        emit_runtime_json_encode_string_from_locals(ptr_local, len_local, flags, module);
        return;
    }
    let out_ptr = module.next_label("json_numeric_string_out");
    let is_numeric = module.next_label("json_numeric_string_is_numeric");
    let number = module.next_label("json_numeric_string_number");
    let float_like = module.next_label("json_numeric_string_float_like");
    for local in [&out_ptr, &is_numeric, &float_like] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_f64_local(number.trim_start_matches('$').to_string());
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get {}", ptr_local));
    module.body().line(&format!("local.get {}", len_local));
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("call $host_numeric_string_value");
    module.body().line(&format!("local.set {}", is_numeric));
    module.body().line(&format!("local.get {}", is_numeric));
    module.body().open("if");
    emit_runtime_string_has_json_float_marker(ptr_local, len_local, &float_like, module);
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", number));
    module.body().line(&format!("local.get {}", float_like));
    module.body().open("if");
    module.body().line(&format!("local.get {}", number));
    module.body().line("call $host_write_float");
    if flags & 1024 != 0 {
        module.body().line(&format!("local.get {}", number));
        module.body().line(&format!("local.get {}", number));
        module.body().line("f64.trunc");
        module.body().line("f64.eq");
        module.body().open("if");
        emit_write_literal_text(".0", module);
        module.body().close("end");
    }
    module.body().line("else");
    module.body().line(&format!("local.get {}", number));
    module.body().line("f64.trunc");
    module.body().line("i64.trunc_f64_s");
    module.body().line("call $host_write_int");
    module.body().close("end");
    module.body().line("else");
    emit_runtime_json_encode_string_from_locals(ptr_local, len_local, flags, module);
    module.body().close("end");
}

pub(super) fn emit_runtime_string_has_json_float_marker(
    ptr_local: &str,
    len_local: &str,
    out_local: &str,
    module: &mut WasmModule,
) {
    let index = module.next_label("json_numeric_marker_index");
    let byte = module.next_label("json_numeric_marker_byte");
    let loop_label = module.next_label("json_numeric_marker_loop");
    let done_label = module.next_label("json_numeric_marker_done");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_local));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len_local));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", ptr_local));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", byte));
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 46");
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 69");
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 101");
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", out_local));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_json_encode_string_from_locals(
    ptr_local: &str,
    len_local: &str,
    flags: i64,
    module: &mut WasmModule,
) {
    let idx = module.next_label("json_idx");
    let byte = module.next_label("json_byte");
    let loop_label = module.next_label("json_loop");
    let done_label = module.next_label("json_done");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    emit_write_literal_text("\"", module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get {}", len_local));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", ptr_local));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", byte));
    emit_runtime_json_encoded_byte(&byte, flags, module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    emit_write_literal_text("\"", module);
}

fn emit_runtime_json_encoded_byte(byte: &str, flags: i64, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 34");
    module.body().line("i32.eq");
    module.body().open("if");
    if flags & 8 != 0 {
        emit_write_literal_text("\\u0022", module);
    } else {
        emit_write_literal_text("\\\"", module);
    }
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 92");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_write_literal_text("\\\\", module);
    module.body().line("else");
    if flags & 64 == 0 {
        module.body().line(&format!("local.get {}", byte));
        module.body().line("i32.const 47");
        module.body().line("i32.eq");
        module.body().open("if");
        emit_write_literal_text("\\/", module);
        module.body().line("else");
    }
    emit_runtime_json_non_slash_byte(byte, flags, module);
    if flags & 64 == 0 {
        module.body().close("end");
    }
    module.body().close("end");
    module.body().close("end");
}

fn emit_runtime_json_non_slash_byte(byte: &str, flags: i64, module: &mut WasmModule) {
    if flags & 1 != 0 {
        module.body().line(&format!("local.get {}", byte));
        module.body().line("i32.const 60");
        module.body().line("i32.eq");
        module.body().open("if");
        emit_write_literal_text("\\u003C", module);
        module.body().line("else");
        module.body().line(&format!("local.get {}", byte));
        module.body().line("i32.const 62");
        module.body().line("i32.eq");
        module.body().open("if");
        emit_write_literal_text("\\u003E", module);
        module.body().line("else");
    }
    if flags & 2 != 0 {
        module.body().line(&format!("local.get {}", byte));
        module.body().line("i32.const 38");
        module.body().line("i32.eq");
        module.body().open("if");
        emit_write_literal_text("\\u0026", module);
        module.body().line("else");
    }
    if flags & 4 != 0 {
        module.body().line(&format!("local.get {}", byte));
        module.body().line("i32.const 39");
        module.body().line("i32.eq");
        module.body().open("if");
        emit_write_literal_text("\\u0027", module);
        module.body().line("else");
    }
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 10");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_write_literal_text("\\n", module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 13");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_write_literal_text("\\r", module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 9");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_write_literal_text("\\t", module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 8");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_write_literal_text("\\b", module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 12");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_write_literal_text("\\f", module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 32");
    module.body().line("i32.lt_u");
    module.body().open("if");
    emit_write_literal_text("\\u00", module);
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 4");
    module.body().line("i32.shr_u");
    emit_write_hex_nibble(module);
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 15");
    module.body().line("i32.and");
    emit_write_hex_nibble(module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    emit_write_stack_byte(module);
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    if flags & 4 != 0 {
        module.body().close("end");
    }
    if flags & 2 != 0 {
        module.body().close("end");
    }
    if flags & 1 != 0 {
        module.body().close("end");
        module.body().close("end");
    }
}
