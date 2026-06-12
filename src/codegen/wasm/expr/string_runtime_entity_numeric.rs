//! Purpose:
//! Emits wasm32-web numeric HTML entity decode helpers.
//! Keeps decimal/hex entity parsing and quote-flag guards separate from named entity decode lowering.
//!
//! Called from:
//! - `super::string_runtime_entity_decode` while emitting html_entity_decode runtime paths.
//!
//! Key details:
//! - Helpers preserve ASCII-only decoded output and quote-flag behavior for output and value-to-stack paths.

use super::*;
use super::string_bytes::*;
use super::string_runtime_entity_decode::EntityQuoteFlags;

pub(super) fn emit_runtime_numeric_entity_decode_case(
    var: &str,
    idx: &str,
    scan: &str,
    byte: &str,
    value: &str,
    radix: &str,
    digit_seen: &str,
    flags: EntityQuoteFlags<'_>,
    loop_label: &str,
    module: &mut WasmModule,
) {
    let done_label = module.next_label("entity_numeric_done");
    let scan_loop = module.next_label("entity_numeric_loop");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 3");
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.lt_u");
    module.body().open("if");
    emit_load_string_byte_at_delta(var, idx, 1, module);
    module.body().line("i32.const 35");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", value));
    module.body().line("i32.const 10");
    module.body().line(&format!("local.set {}", radix));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", digit_seen));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 2");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    emit_load_string_byte(var, scan, module);
    module.body().line("i32.const 120");
    module.body().line("i32.eq");
    emit_load_string_byte(var, scan, module);
    module.body().line("i32.const 88");
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().open("if");
    module.body().line("i32.const 16");
    module.body().line(&format!("local.set {}", radix));
    emit_increment_local(scan, module);
    module.body().close("end");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", scan_loop));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_string_byte(var, scan, module);
    module.body().line(&format!("local.set {}", byte));
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 59");
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", digit_seen));
    module.body().line("i32.and");
    module.body().open("if");
    emit_runtime_numeric_entity_write(
        value,
        byte,
        idx,
        scan,
        &flags,
        &done_label,
        loop_label,
        module,
    );
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", radix));
    module.body().line("i32.const 16");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_hex_digit_condition(byte, module);
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", done_label));
    emit_hex_digit_value(byte, module);
    module.body().line(&format!("local.set {}", byte));
    module.body().line("else");
    emit_ascii_digit_condition(byte, module);
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 48");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", byte));
    module.body().close("end");
    module.body().line(&format!("local.get {}", value));
    module.body().line(&format!("local.get {}", radix));
    module.body().line("i32.mul");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", value));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", digit_seen));
    emit_increment_local(scan, module);
    module.body().line(&format!("br {}", scan_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_numeric_entity_decode_case_to_memory(
    var: &str,
    idx: &str,
    scan: &str,
    byte: &str,
    value: &str,
    radix: &str,
    digit_seen: &str,
    flags: EntityQuoteFlags<'_>,
    out_ptr: &str,
    out_idx: &str,
    loop_label: &str,
    module: &mut WasmModule,
) {
    let done_label = module.next_label("entity_value_numeric_done");
    let scan_loop = module.next_label("entity_value_numeric_loop");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 3");
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.lt_u");
    module.body().open("if");
    emit_load_string_byte_at_delta(var, idx, 1, module);
    module.body().line("i32.const 35");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", value));
    module.body().line("i32.const 10");
    module.body().line(&format!("local.set {}", radix));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", digit_seen));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 2");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    emit_load_string_byte(var, scan, module);
    module.body().line("i32.const 120");
    module.body().line("i32.eq");
    emit_load_string_byte(var, scan, module);
    module.body().line("i32.const 88");
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().open("if");
    module.body().line("i32.const 16");
    module.body().line(&format!("local.set {}", radix));
    emit_increment_local(scan, module);
    module.body().close("end");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", scan_loop));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_string_byte(var, scan, module);
    module.body().line(&format!("local.set {}", byte));
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 59");
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", digit_seen));
    module.body().line("i32.and");
    module.body().open("if");
    emit_runtime_numeric_entity_store(
        value, byte, idx, scan, &flags, out_ptr, out_idx, loop_label, module,
    );
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", radix));
    module.body().line("i32.const 16");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_hex_digit_condition(byte, module);
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", done_label));
    emit_hex_digit_value(byte, module);
    module.body().line(&format!("local.set {}", byte));
    module.body().line("else");
    emit_ascii_digit_condition(byte, module);
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 48");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", byte));
    module.body().close("end");
    module.body().line(&format!("local.get {}", value));
    module.body().line(&format!("local.get {}", radix));
    module.body().line("i32.mul");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", value));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", digit_seen));
    emit_increment_local(scan, module);
    module.body().line(&format!("br {}", scan_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

fn emit_runtime_numeric_entity_store(
    value: &str,
    byte: &str,
    idx: &str,
    scan: &str,
    flags: &EntityQuoteFlags<'_>,
    out_ptr: &str,
    out_idx: &str,
    loop_label: &str,
    module: &mut WasmModule,
) {
    let allowed = module.next_label("entity_value_numeric_allowed");
    module.declare_i32_local(allowed.trim_start_matches('$').to_string());
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", allowed));
    module.body().line(&format!("local.get {}", value));
    module.body().line("i32.const 127");
    module.body().line("i32.gt_u");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", allowed));
    module.body().close("end");
    emit_runtime_numeric_entity_store_quote_guard(value, &allowed, 39, 1, flags, module);
    emit_runtime_numeric_entity_store_quote_guard(value, &allowed, 34, 2, flags, module);
    module.body().line(&format!("local.get {}", allowed));
    module.body().open("if");
    emit_store_byte_local(value, out_ptr, out_idx, module);
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().line("else");
    module.body().line("i32.const 38");
    module.body().line(&format!("local.set {}", byte));
    module.body().close("end");
}

fn emit_runtime_numeric_entity_store_quote_guard(
    value: &str,
    allowed: &str,
    quote: i32,
    bit: i64,
    flags: &EntityQuoteFlags<'_>,
    module: &mut WasmModule,
) {
    match flags {
        EntityQuoteFlags::Static(flags) if flags & bit == 0 => {
            module.body().line(&format!("local.get {}", value));
            module.body().line(&format!("i32.const {}", quote));
            module.body().line("i32.eq");
            module.body().open("if");
            module.body().line("i32.const 0");
            module.body().line(&format!("local.set {}", allowed));
            module.body().close("end");
        }
        EntityQuoteFlags::Dynamic(flags) => {
            module.body().line(&format!("local.get {}", value));
            module.body().line(&format!("i32.const {}", quote));
            module.body().line("i32.eq");
            module.body().line(&format!("local.get {}", flags));
            module.body().line(&format!("i64.const {}", bit));
            module.body().line("i64.and");
            module.body().line("i64.eqz");
            module.body().line("i32.and");
            module.body().open("if");
            module.body().line("i32.const 0");
            module.body().line(&format!("local.set {}", allowed));
            module.body().close("end");
        }
        _ => {}
    }
}

fn emit_runtime_numeric_entity_write(
    value: &str,
    byte: &str,
    idx: &str,
    scan: &str,
    flags: &EntityQuoteFlags<'_>,
    done_label: &str,
    loop_label: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", value));
    module.body().line("i32.const 127");
    module.body().line("i32.le_u");
    module.body().open("if");
    emit_runtime_numeric_entity_quote_guard(value, byte, 39, 1, flags, done_label, module);
    emit_runtime_numeric_entity_quote_guard(value, byte, 34, 2, flags, done_label, module);
    module.body().line(&format!("local.get {}", value));
    emit_write_stack_byte(module);
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().line("else");
    module.body().line("i32.const 38");
    module.body().line(&format!("local.set {}", byte));
    module.body().close("end");
}

fn emit_runtime_numeric_entity_quote_guard(
    value: &str,
    byte: &str,
    quote: i32,
    bit: i64,
    flags: &EntityQuoteFlags<'_>,
    done_label: &str,
    module: &mut WasmModule,
) {
    match flags {
        EntityQuoteFlags::Static(flags) if flags & bit == 0 => {
            module.body().line(&format!("local.get {}", value));
            module.body().line(&format!("i32.const {}", quote));
            module.body().line("i32.eq");
            module.body().open("if");
            module.body().line("i32.const 38");
            module.body().line(&format!("local.set {}", byte));
            module.body().line(&format!("br {}", done_label));
            module.body().close("end");
        }
        EntityQuoteFlags::Dynamic(flags) => {
            module.body().line(&format!("local.get {}", value));
            module.body().line(&format!("i32.const {}", quote));
            module.body().line("i32.eq");
            module.body().line(&format!("local.get {}", flags));
            module.body().line(&format!("i64.const {}", bit));
            module.body().line("i64.and");
            module.body().line("i64.eqz");
            module.body().line("i32.and");
            module.body().open("if");
            module.body().line("i32.const 38");
            module.body().line(&format!("local.set {}", byte));
            module.body().line(&format!("br {}", done_label));
            module.body().close("end");
        }
        _ => {}
    }
}
