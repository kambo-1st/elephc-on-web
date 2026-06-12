//! Purpose:
//! Emits JSON string escaping into preallocated wasm memory buffers.
//! Separates byte-copy and string-to-memory encoding from JSON array traversal.
//!
//! Called from:
//! - `super::json_encode` while materializing JSON array values to stack memory.
//!
//! Key details:
//! - Updates caller-provided output pointer/index locals and preserves JSON numeric-check flag behavior.

use super::*;
use super::json_runtime_string::emit_runtime_string_has_json_float_marker;

pub(super) fn emit_json_encode_value_string_locals_to_memory(
    ptr_local: &str,
    len_local: &str,
    out_ptr: &str,
    out_len: &str,
    flags: i64,
    module: &mut WasmModule,
) {
    if flags & 32 == 0 {
        emit_json_encode_string_locals_to_memory(ptr_local, len_local, out_ptr, out_len, flags, module);
        return;
    }
    let number_out = module.next_label("json_mem_numeric_out");
    let is_numeric = module.next_label("json_mem_numeric_is_numeric");
    let number = module.next_label("json_mem_numeric_number");
    let float_like = module.next_label("json_mem_numeric_float_like");
    let part = module.next_label("json_mem_numeric_part");
    for local in [&number_out, &is_numeric, &float_like] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_f64_local(number.trim_start_matches('$').to_string());
    module.declare_i32_local(format!("{}_ptr", part.trim_start_matches('$')));
    module.declare_i32_local(format!("{}_len", part.trim_start_matches('$')));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", number_out));
    module.body().line("global.get $heap");
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get {}", ptr_local));
    module.body().line(&format!("local.get {}", len_local));
    module.body().line(&format!("local.get {}", number_out));
    module.body().line("call $host_numeric_string_value");
    module.body().line(&format!("local.set {}", is_numeric));
    module.body().line(&format!("local.get {}", is_numeric));
    module.body().open("if");
    emit_runtime_string_has_json_float_marker(ptr_local, len_local, &float_like, module);
    module.body().line(&format!("local.get {}", number_out));
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", number));
    module.body().line(&format!("local.get {}", float_like));
    module.body().open("if");
    module.body().line(&format!("local.get {}", number));
    emit_f64_stack_string_cast_value_to_stack("json_mem_numeric_float", module);
    module.body().line(&format!("local.set ${}_len", part.trim_start_matches('$')));
    module.body().line(&format!("local.set ${}_ptr", part.trim_start_matches('$')));
    emit_copy_string_to_memory(part.trim_start_matches('$'), out_ptr, out_len, module);
    if flags & 1024 != 0 {
        module.body().line(&format!("local.get {}", number));
        module.body().line(&format!("local.get {}", number));
        module.body().line("f64.trunc");
        module.body().line("f64.eq");
        module.body().open("if");
        emit_copy_literal_text_to_memory(".0", out_ptr, out_len, module);
        module.body().close("end");
    }
    module.body().line("else");
    module.body().line(&format!("local.get {}", number));
    module.body().line("f64.trunc");
    module.body().line("i64.trunc_f64_s");
    emit_i64_stack_string_cast_value_to_stack("json_mem_numeric_int", module);
    module.body().line(&format!("local.set ${}_len", part.trim_start_matches('$')));
    module.body().line(&format!("local.set ${}_ptr", part.trim_start_matches('$')));
    emit_copy_string_to_memory(part.trim_start_matches('$'), out_ptr, out_len, module);
    module.body().close("end");
    module.body().line("else");
    emit_json_encode_string_locals_to_memory(ptr_local, len_local, out_ptr, out_len, flags, module);
    module.body().close("end");
}

pub(super) fn emit_json_encode_string_locals_to_memory(
    ptr_local: &str,
    len_local: &str,
    out_ptr: &str,
    out_len: &str,
    flags: i64,
    module: &mut WasmModule,
) {
    let idx = module.next_label("json_mem_string_idx");
    let byte = module.next_label("json_mem_string_byte");
    let loop_label = module.next_label("json_mem_string_loop");
    let done_label = module.next_label("json_mem_string_done");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    emit_store_byte_const(34, out_ptr, out_len, module);
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
    emit_json_encoded_byte_to_memory(&byte, out_ptr, out_len, flags, module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    emit_store_byte_const(34, out_ptr, out_len, module);
}

fn emit_json_encoded_byte_to_memory(
    byte: &str,
    out_ptr: &str,
    out_len: &str,
    flags: i64,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 34");
    module.body().line("i32.eq");
    module.body().open("if");
    if flags & 8 != 0 {
        emit_copy_literal_text_to_memory("\\u0022", out_ptr, out_len, module);
    } else {
        emit_copy_literal_text_to_memory("\\\"", out_ptr, out_len, module);
    }
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 92");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_copy_literal_text_to_memory("\\\\", out_ptr, out_len, module);
    module.body().line("else");
    if flags & 64 == 0 {
        module.body().line(&format!("local.get {}", byte));
        module.body().line("i32.const 47");
        module.body().line("i32.eq");
        module.body().open("if");
        emit_copy_literal_text_to_memory("\\/", out_ptr, out_len, module);
        module.body().line("else");
    }
    emit_json_non_slash_byte_to_memory(byte, out_ptr, out_len, flags, module);
    if flags & 64 == 0 {
        module.body().close("end");
    }
    module.body().close("end");
    module.body().close("end");
}

fn emit_json_non_slash_byte_to_memory(
    byte: &str,
    out_ptr: &str,
    out_len: &str,
    flags: i64,
    module: &mut WasmModule,
) {
    if flags & 1 != 0 {
        module.body().line(&format!("local.get {}", byte));
        module.body().line("i32.const 60");
        module.body().line("i32.eq");
        module.body().open("if");
        emit_copy_literal_text_to_memory("\\u003C", out_ptr, out_len, module);
        module.body().line("else");
        module.body().line(&format!("local.get {}", byte));
        module.body().line("i32.const 62");
        module.body().line("i32.eq");
        module.body().open("if");
        emit_copy_literal_text_to_memory("\\u003E", out_ptr, out_len, module);
        module.body().line("else");
    }
    if flags & 2 != 0 {
        module.body().line(&format!("local.get {}", byte));
        module.body().line("i32.const 38");
        module.body().line("i32.eq");
        module.body().open("if");
        emit_copy_literal_text_to_memory("\\u0026", out_ptr, out_len, module);
        module.body().line("else");
    }
    if flags & 4 != 0 {
        module.body().line(&format!("local.get {}", byte));
        module.body().line("i32.const 39");
        module.body().line("i32.eq");
        module.body().open("if");
        emit_copy_literal_text_to_memory("\\u0027", out_ptr, out_len, module);
        module.body().line("else");
    }
    emit_json_control_or_plain_byte_to_memory(byte, out_ptr, out_len, module);
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

fn emit_json_control_or_plain_byte_to_memory(
    byte: &str,
    out_ptr: &str,
    out_len: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 10");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_copy_literal_text_to_memory("\\n", out_ptr, out_len, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 13");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_copy_literal_text_to_memory("\\r", out_ptr, out_len, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 9");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_copy_literal_text_to_memory("\\t", out_ptr, out_len, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 8");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_copy_literal_text_to_memory("\\b", out_ptr, out_len, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 12");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_copy_literal_text_to_memory("\\f", out_ptr, out_len, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 32");
    module.body().line("i32.lt_u");
    module.body().open("if");
    emit_copy_literal_text_to_memory("\\u00", out_ptr, out_len, module);
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 4");
    module.body().line("i32.shr_u");
    emit_store_hex_nibble_to_memory(out_ptr, out_len, module);
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 15");
    module.body().line("i32.and");
    emit_store_hex_nibble_to_memory(out_ptr, out_len, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    emit_store_byte_local_stack(out_ptr, out_len, module);
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

fn emit_store_hex_nibble_to_memory(out_ptr: &str, out_len: &str, module: &mut WasmModule) {
    let nibble = module.next_label("json_hex_nibble");
    module.declare_i32_local(nibble.trim_start_matches('$').to_string());
    module.body().line(&format!("local.set {}", nibble));
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
    emit_store_byte_local_stack(out_ptr, out_len, module);
}

fn emit_store_byte_local_stack(out_ptr: &str, out_idx: &str, module: &mut WasmModule) {
    let byte = module.next_label("store_stack_byte");
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    module.body().line(&format!("local.set {}", byte));
    emit_store_byte_local(&byte, out_ptr, out_idx, module);
}

pub(super) fn emit_copy_literal_text_to_memory(
    text: &str,
    out_ptr: &str,
    out_idx: &str,
    module: &mut WasmModule,
) {
    let (ptr, len) = module.intern_string(text);
    emit_copy_static_range_to_memory(ptr, len, out_ptr, out_idx, module);
}
