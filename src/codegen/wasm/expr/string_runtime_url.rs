//! Purpose:
//! Emits wasm32-web runtime URL encode/decode helpers for string builtins.
//! Keeps percent-encoding state machines separate from the broader string runtime emitters.
//!
//! Called from:
//! - `super::string_runtime` re-exports used by string builtin lowering paths.
//!
//! Key details:
//! - Helpers preserve output and value-to-stack contracts for urlencode/rawurlencode and decode variants.

use super::*;
use super::string_bytes::*;

pub(super) fn emit_runtime_urlencode(var: &str, space: SpaceEncoding, module: &mut WasmModule) {
    let idx = module.next_label("url_idx");
    let byte = module.next_label("url_byte");
    let loop_label = module.next_label("url_loop");
    let done_label = module.next_label("url_done");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_string_byte(var, &idx, module);
    module.body().line(&format!("local.set {}", byte));
    emit_url_safe_condition(&byte, space, module);
    module.body().open("if");
    module.body().line(&format!("local.get {}", byte));
    emit_write_stack_byte(module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 32");
    module.body().line("i32.eq");
    module.body().open("if");
    if matches!(space, SpaceEncoding::Plus) {
        module.body().line("i32.const 43");
        emit_write_stack_byte(module);
    } else {
        emit_write_percent_encoded_byte(&byte, module);
    }
    module.body().line("else");
    emit_write_percent_encoded_byte(&byte, module);
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_urlencode_value_to_stack(var: &str, space: SpaceEncoding, module: &mut WasmModule) {
    let idx = module.next_label("url_value_idx");
    let byte = module.next_label("url_value_byte");
    let out_ptr = module.next_label("url_value_ptr");
    let out_len = module.next_label("url_value_len");
    let out_idx = module.next_label("url_value_out_idx");
    let loop_label = module.next_label("url_value_loop");
    let done_label = module.next_label("url_value_done");
    for local in [&idx, &byte, &out_ptr, &out_len, &out_idx] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.const 715827882");
    module.body().line("i32.gt_u");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get ${}_len", var));
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
    module.body().line(&format!("local.set {}", byte));
    emit_url_safe_condition(&byte, space, module);
    module.body().open("if");
    emit_store_byte_local(&byte, &out_ptr, &out_idx, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 32");
    module.body().line("i32.eq");
    module.body().open("if");
    if matches!(space, SpaceEncoding::Plus) {
        emit_store_byte_const(43, &out_ptr, &out_idx, module);
    } else {
        emit_store_percent_encoded_byte(&byte, &out_ptr, &out_idx, module);
    }
    module.body().line("else");
    emit_store_percent_encoded_byte(&byte, &out_ptr, &out_idx, module);
    module.body().close("end");
    module.body().close("end");
    emit_increment_local(&idx, module);
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
}

pub(super) fn emit_runtime_urldecode(var: &str, space: SpaceEncoding, module: &mut WasmModule) {
    let idx = module.next_label("urld_idx");
    let byte = module.next_label("urld_byte");
    let high = module.next_label("urld_high");
    let low = module.next_label("urld_low");
    let loop_label = module.next_label("urld_loop");
    let done_label = module.next_label("urld_done");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    module.declare_i32_local(high.trim_start_matches('$').to_string());
    module.declare_i32_local(low.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_string_byte(var, &idx, module);
    module.body().line(&format!("local.set {}", byte));
    if matches!(space, SpaceEncoding::Plus) {
        module.body().line(&format!("local.get {}", byte));
        module.body().line("i32.const 43");
        module.body().line("i32.eq");
        module.body().open("if");
        module.body().line("i32.const 32");
        emit_write_stack_byte(module);
        module.body().line(&format!("local.get {}", idx));
        module.body().line("i32.const 1");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", idx));
        module.body().line(&format!("br {}", loop_label));
        module.body().close("end");
    }
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 37");
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 2");
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.lt_u");
    module.body().line("i32.and");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", high));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.const 2");
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", low));
    emit_hex_digit_condition(&high, module);
    emit_hex_digit_condition(&low, module);
    module.body().line("i32.and");
    module.body().open("if");
    emit_hex_digit_value(&high, module);
    module.body().line("i32.const 4");
    module.body().line("i32.shl");
    emit_hex_digit_value(&low, module);
    module.body().line("i32.or");
    emit_write_stack_byte(module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 3");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", byte));
    emit_write_stack_byte(module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_urldecode_value_to_stack(var: &str, space: SpaceEncoding, module: &mut WasmModule) {
    let idx = module.next_label("urld_value_idx");
    let byte = module.next_label("urld_value_byte");
    let high = module.next_label("urld_value_high");
    let low = module.next_label("urld_value_low");
    let decoded = module.next_label("urld_value_decoded");
    let out_ptr = module.next_label("urld_value_ptr");
    let out_idx = module.next_label("urld_value_out_idx");
    let loop_label = module.next_label("urld_value_loop");
    let done_label = module.next_label("urld_value_done");
    for local in [&idx, &byte, &high, &low, &decoded, &out_ptr, &out_idx] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get ${}_len", var));
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
    module.body().line(&format!("local.set {}", byte));
    if matches!(space, SpaceEncoding::Plus) {
        module.body().line(&format!("local.get {}", byte));
        module.body().line("i32.const 43");
        module.body().line("i32.eq");
        module.body().open("if");
        emit_store_byte_const(32, &out_ptr, &out_idx, module);
        emit_increment_local(&idx, module);
        module.body().line(&format!("br {}", loop_label));
        module.body().close("end");
    }
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 37");
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 2");
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.lt_u");
    module.body().line("i32.and");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", high));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.const 2");
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", low));
    emit_hex_digit_condition(&high, module);
    emit_hex_digit_condition(&low, module);
    module.body().line("i32.and");
    module.body().open("if");
    emit_hex_digit_value(&high, module);
    module.body().line("i32.const 4");
    module.body().line("i32.shl");
    emit_hex_digit_value(&low, module);
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", decoded));
    emit_store_byte_local(&decoded, &out_ptr, &out_idx, module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 3");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    emit_store_byte_local(&byte, &out_ptr, &out_idx, module);
    emit_increment_local(&idx, module);
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
}
