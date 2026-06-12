//! Purpose:
//! Emits wasm32-web HTML entity decode runtime helpers for string builtins.
//! Keeps entity lookup, numeric entity parsing, and quote-flag handling out of the broad string runtime file.
//!
//! Called from:
//! - `super::string_runtime` re-exports used by string builtin lowering paths.
//!
//! Key details:
//! - Helpers preserve output and value-to-stack contracts for static and dynamic flag paths.

use super::*;
use super::string_bytes::*;

use super::string_runtime_entity_numeric::{
    emit_runtime_numeric_entity_decode_case, emit_runtime_numeric_entity_decode_case_to_memory,
};

pub(super) enum EntityQuoteFlags<'a> {
    Static(i64),
    Dynamic(&'a str),
}

fn emit_runtime_entity_decode_case(
    var: &str,
    idx: &str,
    entity: &str,
    decoded: i32,
    loop_label: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", entity.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.le_u");
    module.body().open("if");
    emit_runtime_literal_match_at(var, idx, entity, module);
    module.body().open("if");
    module.body().line(&format!("i32.const {}", decoded));
    emit_write_stack_byte(module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", entity.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_runtime_entity_decode_case_to_memory(
    var: &str,
    idx: &str,
    entity: &str,
    decoded: i32,
    out_ptr: &str,
    out_idx: &str,
    loop_label: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", entity.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.le_u");
    module.body().open("if");
    emit_runtime_literal_match_at(var, idx, entity, module);
    module.body().open("if");
    emit_store_byte_const(decoded, out_ptr, out_idx, module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", entity.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_runtime_entity_decode_string_case(
    var: &str,
    idx: &str,
    entity: &str,
    decoded: &str,
    loop_label: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", entity.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.le_u");
    module.body().open("if");
    emit_runtime_literal_match_at(var, idx, entity, module);
    module.body().open("if");
    let (decoded_ptr, decoded_len) = module.intern_string(decoded);
    emit_write_static_string(decoded_ptr, decoded_len, module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", entity.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_runtime_entity_decode_string_case_to_memory(
    var: &str,
    idx: &str,
    entity: &str,
    decoded: &str,
    out_ptr: &str,
    out_idx: &str,
    loop_label: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", entity.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.le_u");
    module.body().open("if");
    emit_runtime_literal_match_at(var, idx, entity, module);
    module.body().open("if");
    let (decoded_ptr, decoded_len) = module.intern_string(decoded);
    emit_copy_static_range_to_memory(decoded_ptr, decoded_len, out_ptr, out_idx, module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", entity.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_runtime_entity_decode_flagged_case_to_memory(
    var: &str,
    idx: &str,
    flags: &str,
    bit: i64,
    entity: &str,
    decoded: i32,
    out_ptr: &str,
    out_idx: &str,
    loop_label: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", flags));
    module.body().line(&format!("i64.const {}", bit));
    module.body().line("i64.and");
    module.body().line("i64.eqz");
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_runtime_entity_decode_case_to_memory(
        var, idx, entity, decoded, out_ptr, out_idx, loop_label, module,
    );
    module.body().close("end");
}

pub(super) fn emit_runtime_html_preserve_entity_case(
    var: &str,
    idx: &str,
    entity: &str,
    loop_label: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", entity.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.le_u");
    module.body().open("if");
    emit_runtime_literal_match_at(var, idx, entity, module);
    module.body().open("if");
    emit_write_literal_text(entity, module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", entity.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_html_preserve_numeric_entity_case(
    var: &str,
    idx: &str,
    loop_label: &str,
    module: &mut WasmModule,
) {
    let scan = module.next_label("html_entity_scan");
    let digit_seen = module.next_label("html_entity_digit_seen");
    let byte = module.next_label("html_entity_byte");
    let write = module.next_label("html_entity_write");
    let parse_loop = module.next_label("html_entity_parse_loop");
    let parse_done = module.next_label("html_entity_parse_done");
    let write_loop = module.next_label("html_entity_write_loop");
    let write_done = module.next_label("html_entity_write_done");
    for local in [&scan, &digit_seen, &byte, &write] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
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
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 2");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    emit_load_string_byte(var, &scan, module);
    module.body().line("i32.const 120");
    module.body().line("i32.eq");
    emit_load_string_byte(var, &scan, module);
    module.body().line("i32.const 88");
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().open("if");
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().close("end");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", digit_seen));
    module.body().open(&format!("block {}", parse_done));
    module.body().open(&format!("loop {}", parse_loop));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", parse_done));
    emit_load_string_byte(var, &scan, module);
    module.body().line(&format!("local.set {}", byte));
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 59");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", digit_seen));
    module.body().open("if");
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.set {}", write));
    module.body().open(&format!("block {}", write_done));
    module.body().open(&format!("loop {}", write_loop));
    module.body().line(&format!("local.get {}", write));
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.gt_u");
    module.body().line(&format!("br_if {}", write_done));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", write));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    emit_write_stack_byte(module);
    module.body().line(&format!("local.get {}", write));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", write));
    module.body().line(&format!("br {}", write_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().line(&format!("br {}", parse_done));
    module.body().close("end");
    emit_hex_digit_condition(&byte, module);
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", digit_seen));
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().line(&format!("br {}", parse_loop));
    module.body().close("end");
    module.body().line(&format!("br {}", parse_done));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

fn emit_runtime_entity_decode_flagged_case(
    var: &str,
    idx: &str,
    flags: &str,
    bit: i64,
    entity: &str,
    decoded: i32,
    loop_label: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", flags));
    module.body().line(&format!("i64.const {}", bit));
    module.body().line("i64.and");
    module.body().line("i64.eqz");
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_runtime_entity_decode_case(var, idx, entity, decoded, loop_label, module);
    module.body().close("end");
}


pub(super) fn emit_runtime_html_entity_decode(var: &str, flags: i64, module: &mut WasmModule) {
    let idx = module.next_label("entity_idx");
    let byte = module.next_label("entity_byte");
    let scan = module.next_label("entity_scan");
    let value = module.next_label("entity_value");
    let radix = module.next_label("entity_radix");
    let digit_seen = module.next_label("entity_digit_seen");
    let loop_label = module.next_label("entity_loop");
    let done_label = module.next_label("entity_done");
    for local in [&idx, &byte, &scan, &value, &radix, &digit_seen] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
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
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 38");
    module.body().line("i32.eq");
    module.body().open("if");
    if flags & 2 != 0 {
        emit_runtime_entity_decode_case(var, &idx, "&quot;", 34, &loop_label, module);
    }
    if flags & 1 != 0 {
        emit_runtime_entity_decode_case(var, &idx, "&#039;", 39, &loop_label, module);
        if html_decodes_apos(flags) {
            emit_runtime_entity_decode_case(var, &idx, "&apos;", 39, &loop_label, module);
        }
    }
    for (entity, decoded) in [("&lt;", 60), ("&gt;", 62), ("&amp;", 38)] {
        emit_runtime_entity_decode_case(var, &idx, entity, decoded, &loop_label, module);
    }
    for (entity, decoded) in html_utf8_entity_decodes() {
        emit_runtime_entity_decode_string_case(var, &idx, entity, decoded, &loop_label, module);
    }
    emit_runtime_numeric_entity_decode_case(
        var,
        &idx,
        &scan,
        &byte,
        &value,
        &radix,
        &digit_seen,
        EntityQuoteFlags::Static(flags),
        &loop_label,
        module,
    );
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

pub(super) fn emit_runtime_html_entity_decode_value_to_stack(var: &str, flags: i64, module: &mut WasmModule) {
    let idx = module.next_label("entity_value_idx");
    let byte = module.next_label("entity_value_byte");
    let scan = module.next_label("entity_value_scan");
    let value = module.next_label("entity_value_value");
    let radix = module.next_label("entity_value_radix");
    let digit_seen = module.next_label("entity_value_digit_seen");
    let out_ptr = module.next_label("entity_value_ptr");
    let out_idx = module.next_label("entity_value_out_idx");
    let loop_label = module.next_label("entity_value_loop");
    let done_label = module.next_label("entity_value_done");
    for local in [&idx, &byte, &scan, &value, &radix, &digit_seen, &out_ptr, &out_idx] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
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
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 38");
    module.body().line("i32.eq");
    module.body().open("if");
    if flags & 2 != 0 {
        emit_runtime_entity_decode_case_to_memory(
            var,
            &idx,
            "&quot;",
            34,
            &out_ptr,
            &out_idx,
            &loop_label,
            module,
        );
    }
    if flags & 1 != 0 {
        emit_runtime_entity_decode_case_to_memory(
            var,
            &idx,
            "&#039;",
            39,
            &out_ptr,
            &out_idx,
            &loop_label,
            module,
        );
        emit_runtime_entity_decode_case_to_memory(
            var,
            &idx,
            "&#39;",
            39,
            &out_ptr,
            &out_idx,
            &loop_label,
            module,
        );
        if html_decodes_apos(flags) {
            emit_runtime_entity_decode_case_to_memory(
                var,
                &idx,
                "&apos;",
                39,
                &out_ptr,
                &out_idx,
                &loop_label,
                module,
            );
        }
    }
    for (entity, decoded) in [("&lt;", 60), ("&gt;", 62), ("&amp;", 38)] {
        emit_runtime_entity_decode_case_to_memory(
            var,
            &idx,
            entity,
            decoded,
            &out_ptr,
            &out_idx,
            &loop_label,
            module,
        );
    }
    for (entity, decoded) in html_utf8_entity_decodes() {
        emit_runtime_entity_decode_string_case_to_memory(
            var,
            &idx,
            entity,
            decoded,
            &out_ptr,
            &out_idx,
            &loop_label,
            module,
        );
    }
    emit_runtime_numeric_entity_decode_case_to_memory(
        var,
        &idx,
        &scan,
        &byte,
        &value,
        &radix,
        &digit_seen,
        EntityQuoteFlags::Static(flags),
        &out_ptr,
        &out_idx,
        &loop_label,
        module,
    );
    module.body().close("end");
    emit_store_byte_local(&byte, &out_ptr, &out_idx, module);
    emit_increment_local(&idx, module);
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
}

pub(super) fn emit_runtime_html_entity_decode_dynamic(
    var: &str,
    flags_expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let flags = module.next_label("entity_flags");
    let idx = module.next_label("entity_idx");
    let byte = module.next_label("entity_byte");
    let scan = module.next_label("entity_scan");
    let value = module.next_label("entity_value");
    let radix = module.next_label("entity_radix");
    let digit_seen = module.next_label("entity_digit_seen");
    let loop_label = module.next_label("entity_loop");
    let done_label = module.next_label("entity_done");
    module.declare_i64_local(flags.trim_start_matches('$').to_string());
    for local in [&idx, &byte, &scan, &value, &radix, &digit_seen] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    require_int(flags_expr, module)?;
    module.body().line(&format!("local.set {}", flags));
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
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 38");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_runtime_entity_decode_flagged_case(
        var,
        &idx,
        &flags,
        2,
        "&quot;",
        34,
        &loop_label,
        module,
    );
    emit_runtime_entity_decode_flagged_case(
        var,
        &idx,
        &flags,
        1,
        "&#039;",
        39,
        &loop_label,
        module,
    );
    for (entity, decoded) in [("&lt;", 60), ("&gt;", 62), ("&amp;", 38)] {
        emit_runtime_entity_decode_case(var, &idx, entity, decoded, &loop_label, module);
    }
    for (entity, decoded) in html_utf8_entity_decodes() {
        emit_runtime_entity_decode_string_case(var, &idx, entity, decoded, &loop_label, module);
    }
    emit_runtime_numeric_entity_decode_case(
        var,
        &idx,
        &scan,
        &byte,
        &value,
        &radix,
        &digit_seen,
        EntityQuoteFlags::Dynamic(&flags),
        &loop_label,
        module,
    );
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
    Ok(())
}

pub(super) fn emit_runtime_html_entity_decode_dynamic_value_to_stack(
    var: &str,
    flags_expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let flags = module.next_label("entity_value_flags");
    let idx = module.next_label("entity_value_idx");
    let byte = module.next_label("entity_value_byte");
    let scan = module.next_label("entity_value_scan");
    let value = module.next_label("entity_value_value");
    let radix = module.next_label("entity_value_radix");
    let digit_seen = module.next_label("entity_value_digit_seen");
    let out_ptr = module.next_label("entity_value_ptr");
    let out_idx = module.next_label("entity_value_out_idx");
    let loop_label = module.next_label("entity_value_loop");
    let done_label = module.next_label("entity_value_done");
    module.declare_i64_local(flags.trim_start_matches('$').to_string());
    for local in [&idx, &byte, &scan, &value, &radix, &digit_seen, &out_ptr, &out_idx] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    require_int(flags_expr, module)?;
    module.body().line(&format!("local.set {}", flags));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
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
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 38");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_runtime_entity_decode_flagged_case_to_memory(
        var,
        &idx,
        &flags,
        2,
        "&quot;",
        34,
        &out_ptr,
        &out_idx,
        &loop_label,
        module,
    );
    emit_runtime_entity_decode_flagged_case_to_memory(
        var,
        &idx,
        &flags,
        1,
        "&#039;",
        39,
        &out_ptr,
        &out_idx,
        &loop_label,
        module,
    );
    emit_runtime_entity_decode_flagged_case_to_memory(
        var,
        &idx,
        &flags,
        1,
        "&#39;",
        39,
        &out_ptr,
        &out_idx,
        &loop_label,
        module,
    );
    for (entity, decoded) in [("&lt;", 60), ("&gt;", 62), ("&amp;", 38)] {
        emit_runtime_entity_decode_case_to_memory(
            var,
            &idx,
            entity,
            decoded,
            &out_ptr,
            &out_idx,
            &loop_label,
            module,
        );
    }
    for (entity, decoded) in html_utf8_entity_decodes() {
        emit_runtime_entity_decode_string_case_to_memory(
            var,
            &idx,
            entity,
            decoded,
            &out_ptr,
            &out_idx,
            &loop_label,
            module,
        );
    }
    emit_runtime_numeric_entity_decode_case_to_memory(
        var,
        &idx,
        &scan,
        &byte,
        &value,
        &radix,
        &digit_seen,
        EntityQuoteFlags::Dynamic(&flags),
        &out_ptr,
        &out_idx,
        &loop_label,
        module,
    );
    module.body().close("end");
    emit_store_byte_local(&byte, &out_ptr, &out_idx, module);
    emit_increment_local(&idx, module);
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    Ok(())
}
