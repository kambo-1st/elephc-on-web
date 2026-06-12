//! Purpose:
//! Emits wasm32-web runtime base64 encode/decode helpers for string builtins.
//! Keeps the large base64 state machines separate from the broader string runtime emitters.
//!
//! Called from:
//! - `super::string_runtime` re-exports used by string builtin lowering paths.
//!
//! Key details:
//! - Helpers preserve existing stack/value-to-stack WAT contracts and reuse shared byte helpers.

use super::*;
use super::string_bytes::*;

pub(super) fn emit_runtime_base64_encode(var: &str, module: &mut WasmModule) {
    let idx = module.next_label("b64_idx");
    let b0 = module.next_label("b64_b0");
    let b1 = module.next_label("b64_b1");
    let b2 = module.next_label("b64_b2");
    let loop_label = module.next_label("b64_loop");
    let done_label = module.next_label("b64_done");
    let (table_ptr, _) = module.intern_string(
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
    );
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(b0.trim_start_matches('$').to_string());
    module.declare_i32_local(b1.trim_start_matches('$').to_string());
    module.declare_i32_local(b2.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_string_byte(var, &idx, module);
    module.body().line(&format!("local.set {}", b0));
    emit_optional_string_byte(var, &idx, 1, &b1, module);
    emit_optional_string_byte(var, &idx, 2, &b2, module);

    module.body().line(&format!("local.get {}", b0));
    module.body().line("i32.const 2");
    module.body().line("i32.shr_u");
    emit_write_base64_index(table_ptr, module);

    module.body().line(&format!("local.get {}", b0));
    module.body().line("i32.const 3");
    module.body().line("i32.and");
    module.body().line("i32.const 4");
    module.body().line("i32.shl");
    module.body().line(&format!("local.get {}", b1));
    module.body().line("i32.const 4");
    module.body().line("i32.shr_u");
    module.body().line("i32.or");
    emit_write_base64_index(table_ptr, module);

    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.lt_u");
    module.body().open("if");
    module.body().line(&format!("local.get {}", b1));
    module.body().line("i32.const 15");
    module.body().line("i32.and");
    module.body().line("i32.const 2");
    module.body().line("i32.shl");
    module.body().line(&format!("local.get {}", b2));
    module.body().line("i32.const 6");
    module.body().line("i32.shr_u");
    module.body().line("i32.or");
    emit_write_base64_index(table_ptr, module);
    module.body().line("else");
    module.body().line("i32.const 61");
    emit_write_stack_byte(module);
    module.body().close("end");

    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 2");
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.lt_u");
    module.body().open("if");
    module.body().line(&format!("local.get {}", b2));
    module.body().line("i32.const 63");
    module.body().line("i32.and");
    emit_write_base64_index(table_ptr, module);
    module.body().line("else");
    module.body().line("i32.const 61");
    emit_write_stack_byte(module);
    module.body().close("end");

    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 3");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_base64_encode_value_to_stack(var: &str, module: &mut WasmModule) {
    let idx = module.next_label("b64_value_idx");
    let out_ptr = module.next_label("b64_value_ptr");
    let out_len = module.next_label("b64_value_len");
    let out_idx = module.next_label("b64_value_out_idx");
    let b0 = module.next_label("b64_value_b0");
    let b1 = module.next_label("b64_value_b1");
    let b2 = module.next_label("b64_value_b2");
    let loop_label = module.next_label("b64_value_loop");
    let done_label = module.next_label("b64_value_done");
    let (table_ptr, _) = module.intern_string(
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
    );
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(out_len.trim_start_matches('$').to_string());
    module.declare_i32_local(out_idx.trim_start_matches('$').to_string());
    module.declare_i32_local(b0.trim_start_matches('$').to_string());
    module.declare_i32_local(b1.trim_start_matches('$').to_string());
    module.declare_i32_local(b2.trim_start_matches('$').to_string());

    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.const 1610612733");
    module.body().line("i32.gt_u");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.const 2");
    module.body().line("i32.add");
    module.body().line("i32.const 3");
    module.body().line("i32.div_u");
    module.body().line("i32.const 4");
    module.body().line("i32.mul");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_string_byte(var, &idx, module);
    module.body().line(&format!("local.set {}", b0));
    emit_optional_string_byte(var, &idx, 1, &b1, module);
    emit_optional_string_byte(var, &idx, 2, &b2, module);

    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", b0));
    module.body().line("i32.const 2");
    module.body().line("i32.shr_u");
    emit_base64_index_byte(table_ptr, module);
    module.body().line("i32.store8");
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_idx));

    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", b0));
    module.body().line("i32.const 3");
    module.body().line("i32.and");
    module.body().line("i32.const 4");
    module.body().line("i32.shl");
    module.body().line(&format!("local.get {}", b1));
    module.body().line("i32.const 4");
    module.body().line("i32.shr_u");
    module.body().line("i32.or");
    emit_base64_index_byte(table_ptr, module);
    module.body().line("i32.store8");
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_idx));

    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", b1));
    module.body().line("i32.const 15");
    module.body().line("i32.and");
    module.body().line("i32.const 2");
    module.body().line("i32.shl");
    module.body().line(&format!("local.get {}", b2));
    module.body().line("i32.const 6");
    module.body().line("i32.shr_u");
    module.body().line("i32.or");
    emit_base64_index_byte(table_ptr, module);
    module.body().line("else");
    module.body().line("i32.const 61");
    module.body().close("end");
    module.body().line("i32.store8");
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_idx));

    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 2");
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", b2));
    module.body().line("i32.const 63");
    module.body().line("i32.and");
    emit_base64_index_byte(table_ptr, module);
    module.body().line("else");
    module.body().line("i32.const 61");
    module.body().close("end");
    module.body().line("i32.store8");
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_idx));

    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 3");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_len));
}

pub(super) fn emit_runtime_base64_decode(var: &str, strict: bool, module: &mut WasmModule) {
    let idx = module.next_label("b64d_idx");
    let byte = module.next_label("b64d_byte");
    let slot = module.next_label("b64d_slot");
    let value = module.next_label("b64d_value");
    let v0 = module.next_label("b64d_v0");
    let v1 = module.next_label("b64d_v1");
    let v2 = module.next_label("b64d_v2");
    let v3 = module.next_label("b64d_v3");
    let loop_label = module.next_label("b64d_loop");
    let done_label = module.next_label("b64d_done");
    let invalid_label = module.next_label("b64d_invalid");
    for local in [&idx, &byte, &slot, &value, &v0, &v1, &v2, &v3] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().open(&format!("block {}", invalid_label));
    if strict {
        emit_runtime_base64_strict_prescan(var, &idx, &byte, &invalid_label, module);
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", slot));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_string_byte(var, &idx, module);
    module.body().line(&format!("local.set {}", byte));
    emit_base64_whitespace_condition(&byte, module);
    module.body().open("if");
    emit_increment_local(&idx, module);
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    emit_base64_data_condition(&byte, module);
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_increment_local(&idx, module);
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    emit_base64_value_or_padding(&byte, module);
    module.body().line(&format!("local.set {}", value));
    emit_assign_base64_slot(&slot, &value, &v0, &v1, &v2, &v3, module);
    emit_increment_local(&slot, module);
    module.body().line(&format!("local.get {}", slot));
    module.body().line("i32.const 4");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_write_base64_quartet(&v0, &v1, &v2, &v3, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", slot));
    module.body().close("end");
    emit_increment_local(&idx, module);
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    emit_flush_base64_tail(&slot, &v0, &v1, &v2, &v3, &invalid_label, strict, module);
    module.body().close("end");
}

pub(super) fn emit_runtime_base64_decode_value_to_stack(var: &str, strict: bool, module: &mut WasmModule) {
    let idx = module.next_label("b64d_value_idx");
    let byte = module.next_label("b64d_value_byte");
    let slot = module.next_label("b64d_value_slot");
    let value = module.next_label("b64d_value_value");
    let v0 = module.next_label("b64d_value_v0");
    let v1 = module.next_label("b64d_value_v1");
    let v2 = module.next_label("b64d_value_v2");
    let v3 = module.next_label("b64d_value_v3");
    let out_ptr = module.next_label("b64d_value_ptr");
    let out_len = module.next_label("b64d_value_len");
    let out_idx = module.next_label("b64d_value_out_idx");
    let loop_label = module.next_label("b64d_value_loop");
    let done_label = module.next_label("b64d_value_done");
    let invalid_label = module.next_label("b64d_value_invalid");
    for local in [
        &idx, &byte, &slot, &value, &v0, &v1, &v2, &v3, &out_ptr, &out_len, &out_idx,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.const 4");
    module.body().line("i32.div_u");
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("i32.const 3");
    module.body().line("i32.mul");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
    module.body().open(&format!("block {}", invalid_label));
    if strict {
        emit_runtime_base64_strict_prescan(var, &idx, &byte, &invalid_label, module);
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", slot));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_string_byte(var, &idx, module);
    module.body().line(&format!("local.set {}", byte));
    emit_base64_whitespace_condition(&byte, module);
    module.body().open("if");
    emit_increment_local(&idx, module);
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    emit_base64_data_condition(&byte, module);
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_increment_local(&idx, module);
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    emit_base64_value_or_padding(&byte, module);
    module.body().line(&format!("local.set {}", value));
    emit_assign_base64_slot(&slot, &value, &v0, &v1, &v2, &v3, module);
    emit_increment_local(&slot, module);
    module.body().line(&format!("local.get {}", slot));
    module.body().line("i32.const 4");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_store_base64_quartet(&v0, &v1, &v2, &v3, &out_ptr, &out_idx, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", slot));
    module.body().close("end");
    emit_increment_local(&idx, module);
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    emit_flush_base64_tail_to_memory(
        &slot,
        &v0,
        &v1,
        &v2,
        &v3,
        &out_ptr,
        &out_idx,
        &invalid_label,
        strict,
        module,
    );
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
}

pub(super) fn emit_runtime_base64_strict_prescan(
    var: &str,
    idx: &str,
    byte: &str,
    invalid_label: &str,
    module: &mut WasmModule,
) {
    let loop_label = module.next_label("b64d_scan_loop");
    let done_label = module.next_label("b64d_scan_done");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_string_byte(var, idx, module);
    module.body().line(&format!("local.set {}", byte));
    emit_base64_whitespace_condition(byte, module);
    emit_base64_data_condition(byte, module);
    module.body().line("i32.or");
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line(&format!("br {}", invalid_label));
    module.body().close("end");
    emit_increment_local(idx, module);
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_assign_base64_slot(
    slot: &str,
    value: &str,
    v0: &str,
    v1: &str,
    v2: &str,
    v3: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", slot));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line(&format!("local.get {}", value));
    module.body().line(&format!("local.set {}", v0));
    module.body().line("else");
    module.body().line(&format!("local.get {}", slot));
    module.body().line("i32.const 1");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", value));
    module.body().line(&format!("local.set {}", v1));
    module.body().line("else");
    module.body().line(&format!("local.get {}", slot));
    module.body().line("i32.const 2");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", value));
    module.body().line(&format!("local.set {}", v2));
    module.body().line("else");
    module.body().line(&format!("local.get {}", value));
    module.body().line(&format!("local.set {}", v3));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_flush_base64_tail(
    slot: &str,
    v0: &str,
    v1: &str,
    v2: &str,
    v3: &str,
    invalid_label: &str,
    strict: bool,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", slot));
    module.body().line("i32.const 2");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 64");
    module.body().line(&format!("local.set {}", v2));
    module.body().line("i32.const 64");
    module.body().line(&format!("local.set {}", v3));
    emit_write_base64_quartet(v0, v1, v2, v3, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", slot));
    module.body().line("i32.const 3");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 64");
    module.body().line(&format!("local.set {}", v3));
    emit_write_base64_quartet(v0, v1, v2, v3, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", slot));
    module.body().line("i32.eqz");
    module.body().line("i32.eqz");
    module.body().open("if");
    if strict {
        module.body().line(&format!("br {}", invalid_label));
    } else {
        module.body().line("unreachable");
    }
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_write_base64_quartet(v0: &str, v1: &str, v2: &str, v3: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", v2));
    module.body().line("i32.const 64");
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", v3));
    module.body().line("i32.const 64");
    module.body().line("i32.ne");
    module.body().line("i32.and");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", v0));
    module.body().line("i32.const 2");
    module.body().line("i32.shl");
    module.body().line(&format!("local.get {}", v1));
    module.body().line("i32.const 4");
    module.body().line("i32.shr_u");
    module.body().line("i32.or");
    emit_write_stack_byte(module);
    module.body().line(&format!("local.get {}", v2));
    module.body().line("i32.const 64");
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line(&format!("local.get {}", v1));
    module.body().line("i32.const 15");
    module.body().line("i32.and");
    module.body().line("i32.const 4");
    module.body().line("i32.shl");
    module.body().line(&format!("local.get {}", v2));
    module.body().line("i32.const 2");
    module.body().line("i32.shr_u");
    module.body().line("i32.or");
    emit_write_stack_byte(module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", v3));
    module.body().line("i32.const 64");
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line(&format!("local.get {}", v2));
    module.body().line("i32.const 3");
    module.body().line("i32.and");
    module.body().line("i32.const 6");
    module.body().line("i32.shl");
    module.body().line(&format!("local.get {}", v3));
    module.body().line("i32.or");
    emit_write_stack_byte(module);
    module.body().close("end");
}

pub(super) fn emit_flush_base64_tail_to_memory(
    slot: &str,
    v0: &str,
    v1: &str,
    v2: &str,
    v3: &str,
    out_ptr: &str,
    out_idx: &str,
    invalid_label: &str,
    strict: bool,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", slot));
    module.body().line("i32.const 2");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 64");
    module.body().line(&format!("local.set {}", v2));
    module.body().line("i32.const 64");
    module.body().line(&format!("local.set {}", v3));
    emit_store_base64_quartet(v0, v1, v2, v3, out_ptr, out_idx, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", slot));
    module.body().line("i32.const 3");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 64");
    module.body().line(&format!("local.set {}", v3));
    emit_store_base64_quartet(v0, v1, v2, v3, out_ptr, out_idx, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", slot));
    module.body().line("i32.eqz");
    module.body().line("i32.eqz");
    module.body().open("if");
    if strict {
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", out_idx));
        module.body().line(&format!("br {}", invalid_label));
    } else {
        module.body().line("unreachable");
    }
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_store_base64_quartet(
    v0: &str,
    v1: &str,
    v2: &str,
    v3: &str,
    out_ptr: &str,
    out_idx: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", v2));
    module.body().line("i32.const 64");
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", v3));
    module.body().line("i32.const 64");
    module.body().line("i32.ne");
    module.body().line("i32.and");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", v0));
    module.body().line("i32.const 2");
    module.body().line("i32.shl");
    module.body().line(&format!("local.get {}", v1));
    module.body().line("i32.const 4");
    module.body().line("i32.shr_u");
    module.body().line("i32.or");
    module.body().line("i32.store8");
    emit_increment_local(out_idx, module);
    module.body().line(&format!("local.get {}", v2));
    module.body().line("i32.const 64");
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", v1));
    module.body().line("i32.const 15");
    module.body().line("i32.and");
    module.body().line("i32.const 4");
    module.body().line("i32.shl");
    module.body().line(&format!("local.get {}", v2));
    module.body().line("i32.const 2");
    module.body().line("i32.shr_u");
    module.body().line("i32.or");
    module.body().line("i32.store8");
    emit_increment_local(out_idx, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", v3));
    module.body().line("i32.const 64");
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", v2));
    module.body().line("i32.const 3");
    module.body().line("i32.and");
    module.body().line("i32.const 6");
    module.body().line("i32.shl");
    module.body().line(&format!("local.get {}", v3));
    module.body().line("i32.or");
    module.body().line("i32.store8");
    emit_increment_local(out_idx, module);
    module.body().close("end");
}
