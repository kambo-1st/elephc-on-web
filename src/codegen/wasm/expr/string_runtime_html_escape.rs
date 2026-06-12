//! Purpose:
//! Emits wasm32-web HTML escape runtime helpers for string builtins.
//! Keeps htmlspecialchars/htmlentities escaping and double-encode handling out of the broad string runtime file.
//!
//! Called from:
//! - `super::string_runtime` re-exports used by string builtin lowering paths.
//!
//! Key details:
//! - Helpers preserve output and value-to-stack contracts for static and dynamic flag paths.

use super::*;
use super::string_bytes::*;
use super::string_runtime_entity_decode::{
    emit_runtime_html_preserve_entity_case, emit_runtime_html_preserve_numeric_entity_case,
};

pub(super) fn emit_runtime_html_escape_with_double_encode(
    var: &str,
    name: &str,
    flags_arg: Option<&Expr>,
    double_encode: RuntimeBoolArg<'_>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(flags_arg) = flags_arg {
        if let Some(flags) = static_or_const_int_value(flags_arg) {
            emit_runtime_html_escape(var, name, flags, double_encode, module);
        } else {
            emit_runtime_html_escape_dynamic(var, flags_arg, double_encode, module)?;
        }
    } else {
        emit_runtime_html_escape(var, name, 3, double_encode, module);
    }
    Ok(())
}

pub(super) fn emit_runtime_html_escape(
    var: &str,
    name: &str,
    flags: i64,
    double_encode: RuntimeBoolArg<'_>,
    module: &mut WasmModule,
) {
    let idx = module.next_label("html_idx");
    let byte = module.next_label("html_byte");
    let loop_label = module.next_label("html_loop");
    let done_label = module.next_label("html_done");
    let (amp_ptr, amp_len) = module.intern_string("&amp;");
    let (quot_ptr, quot_len) = module.intern_string("&quot;");
    let (apos_ptr, apos_len) = module.intern_string(html_single_quote_entity(name, flags));
    let (lt_ptr, lt_len) = module.intern_string("&lt;");
    let (gt_ptr, gt_len) = module.intern_string("&gt;");
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
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 38");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_runtime_html_amp_escape(var, &idx, double_encode, &loop_label, amp_ptr, amp_len, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 34");
    module.body().line("i32.eq");
    module.body().open("if");
    if flags & 2 != 0 {
        emit_write_static_string(quot_ptr, quot_len, module);
    } else {
        module.body().line(&format!("local.get {}", byte));
        emit_write_stack_byte(module);
    }
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 39");
    module.body().line("i32.eq");
    module.body().open("if");
    if flags & 1 != 0 {
        emit_write_static_string(apos_ptr, apos_len, module);
    } else {
        module.body().line(&format!("local.get {}", byte));
        emit_write_stack_byte(module);
    }
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 60");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_write_static_string(lt_ptr, lt_len, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 62");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_write_static_string(gt_ptr, gt_len, module);
    module.body().line("else");
    if name.eq_ignore_ascii_case("htmlentities") {
        for (decoded, entity) in html_utf8_entity_encodes() {
            emit_runtime_htmlentities_utf8_escape_case(var, &idx, decoded, entity, &loop_label, module);
        }
    }
    module.body().line(&format!("local.get {}", byte));
    emit_write_stack_byte(module);
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
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

pub(super) fn emit_runtime_html_escape_value_to_stack(
    var: &str,
    name: &str,
    flags: i64,
    double_encode: RuntimeBoolArg<'_>,
    module: &mut WasmModule,
) {
    let idx = module.next_label("html_value_idx");
    let byte = module.next_label("html_value_byte");
    let out_ptr = module.next_label("html_value_ptr");
    let out_len = module.next_label("html_value_len");
    let out_idx = module.next_label("html_value_out_idx");
    let loop_label = module.next_label("html_value_loop");
    let done_label = module.next_label("html_value_done");
    let (amp_ptr, amp_len) = module.intern_string("&amp;");
    let (quot_ptr, quot_len) = module.intern_string("&quot;");
    let (apos_ptr, apos_len) = module.intern_string(html_single_quote_entity(name, flags));
    let (lt_ptr, lt_len) = module.intern_string("&lt;");
    let (gt_ptr, gt_len) = module.intern_string("&gt;");
    for local in [&idx, &byte, &out_ptr, &out_len, &out_idx] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.const 357913941");
    module.body().line("i32.gt_u");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.const 6");
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
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 38");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_runtime_html_amp_escape_to_memory(
        var,
        &idx,
        double_encode,
        &loop_label,
        amp_ptr,
        amp_len,
        &out_ptr,
        &out_idx,
        module,
    );
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 34");
    module.body().line("i32.eq");
    module.body().open("if");
    if flags & 2 != 0 {
        emit_copy_static_range_to_memory(quot_ptr, quot_len, &out_ptr, &out_idx, module);
    } else {
        emit_store_byte_local(&byte, &out_ptr, &out_idx, module);
    }
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 39");
    module.body().line("i32.eq");
    module.body().open("if");
    if flags & 1 != 0 {
        emit_copy_static_range_to_memory(apos_ptr, apos_len, &out_ptr, &out_idx, module);
    } else {
        emit_store_byte_local(&byte, &out_ptr, &out_idx, module);
    }
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 60");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_copy_static_range_to_memory(lt_ptr, lt_len, &out_ptr, &out_idx, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 62");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_copy_static_range_to_memory(gt_ptr, gt_len, &out_ptr, &out_idx, module);
    module.body().line("else");
    if name.eq_ignore_ascii_case("htmlentities") {
        for (decoded, entity) in html_utf8_entity_encodes() {
            emit_runtime_htmlentities_utf8_escape_case_to_memory(
                var,
                &idx,
                decoded,
                entity,
                &out_ptr,
                &out_idx,
                &loop_label,
                module,
            );
        }
    }
    emit_store_byte_local(&byte, &out_ptr, &out_idx, module);
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    emit_increment_local(&idx, module);
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
}

pub(super) fn emit_runtime_html_escape_dynamic_value_to_stack(
    var: &str,
    flags_expr: &Expr,
    double_encode: RuntimeBoolArg<'_>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let flags = module.next_label("html_value_flags");
    let idx = module.next_label("html_value_idx");
    let byte = module.next_label("html_value_byte");
    let out_ptr = module.next_label("html_value_ptr");
    let out_len = module.next_label("html_value_len");
    let out_idx = module.next_label("html_value_out_idx");
    let loop_label = module.next_label("html_value_loop");
    let done_label = module.next_label("html_value_done");
    let (amp_ptr, amp_len) = module.intern_string("&amp;");
    let (quot_ptr, quot_len) = module.intern_string("&quot;");
    let (apos_ptr, apos_len) = module.intern_string("&#039;");
    let (lt_ptr, lt_len) = module.intern_string("&lt;");
    let (gt_ptr, gt_len) = module.intern_string("&gt;");
    module.declare_i64_local(flags.trim_start_matches('$').to_string());
    for local in [&idx, &byte, &out_ptr, &out_len, &out_idx] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    require_int(flags_expr, module)?;
    module.body().line(&format!("local.set {}", flags));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.const 357913941");
    module.body().line("i32.gt_u");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.const 6");
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
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 38");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_runtime_html_amp_escape_to_memory(
        var,
        &idx,
        double_encode,
        &loop_label,
        amp_ptr,
        amp_len,
        &out_ptr,
        &out_idx,
        module,
    );
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 34");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_runtime_html_flagged_quote_to_memory(
        &flags, 2, quot_ptr, quot_len, &byte, &out_ptr, &out_idx, module,
    );
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 39");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_runtime_html_flagged_quote_to_memory(
        &flags, 1, apos_ptr, apos_len, &byte, &out_ptr, &out_idx, module,
    );
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 60");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_copy_static_range_to_memory(lt_ptr, lt_len, &out_ptr, &out_idx, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 62");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_copy_static_range_to_memory(gt_ptr, gt_len, &out_ptr, &out_idx, module);
    module.body().line("else");
    emit_store_byte_local(&byte, &out_ptr, &out_idx, module);
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    emit_increment_local(&idx, module);
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    Ok(())
}

pub(super) fn emit_runtime_html_flagged_quote_to_memory(
    flags: &str,
    bit: i64,
    entity_ptr: usize,
    entity_len: usize,
    byte: &str,
    out_ptr: &str,
    out_idx: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", flags));
    module.body().line(&format!("i64.const {}", bit));
    module.body().line("i64.and");
    module.body().line("i64.eqz");
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_copy_static_range_to_memory(entity_ptr, entity_len, out_ptr, out_idx, module);
    module.body().line("else");
    emit_store_byte_local(byte, out_ptr, out_idx, module);
    module.body().close("end");
}

pub(super) fn emit_runtime_html_amp_escape_to_memory(
    var: &str,
    idx: &str,
    double_encode: RuntimeBoolArg<'_>,
    loop_label: &str,
    amp_ptr: usize,
    amp_len: usize,
    out_ptr: &str,
    out_idx: &str,
    module: &mut WasmModule,
) {
    emit_runtime_bool_value(double_encode, module);
    module.body().open("if");
    emit_copy_static_range_to_memory(amp_ptr, amp_len, out_ptr, out_idx, module);
    module.body().line("else");
    for entity in ["&amp;", "&lt;", "&gt;", "&quot;", "&#039;", "&#39;"] {
        emit_runtime_html_preserve_entity_case_to_memory(
            var, idx, entity, loop_label, out_ptr, out_idx, module,
        );
    }
    emit_runtime_html_preserve_numeric_entity_case_to_memory(
        var, idx, loop_label, out_ptr, out_idx, module,
    );
    emit_copy_static_range_to_memory(amp_ptr, amp_len, out_ptr, out_idx, module);
    module.body().close("end");
}

pub(super) fn emit_runtime_html_preserve_entity_case_to_memory(
    var: &str,
    idx: &str,
    entity: &str,
    loop_label: &str,
    out_ptr: &str,
    out_idx: &str,
    module: &mut WasmModule,
) {
    let (entity_ptr, entity_len) = module.intern_string(entity);
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", entity.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.le_u");
    module.body().open("if");
    emit_runtime_literal_match_at(var, idx, entity, module);
    module.body().open("if");
    emit_copy_static_range_to_memory(entity_ptr, entity_len, out_ptr, out_idx, module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", entity.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_html_preserve_numeric_entity_case_to_memory(
    var: &str,
    idx: &str,
    loop_label: &str,
    out_ptr: &str,
    out_idx: &str,
    module: &mut WasmModule,
) {
    let scan = module.next_label("html_entity_scan");
    let digit_seen = module.next_label("html_entity_digit_seen");
    let byte = module.next_label("html_entity_byte");
    let copy = module.next_label("html_entity_copy");
    let parse_loop = module.next_label("html_entity_parse_loop");
    let parse_done = module.next_label("html_entity_parse_done");
    let copy_loop = module.next_label("html_entity_copy_loop");
    let copy_done = module.next_label("html_entity_copy_done");
    for local in [&scan, &digit_seen, &byte, &copy] {
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
    emit_increment_local(&scan, module);
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
    module.body().line(&format!("local.set {}", copy));
    module.body().open(&format!("block {}", copy_done));
    module.body().open(&format!("loop {}", copy_loop));
    module.body().line(&format!("local.get {}", copy));
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.gt_u");
    module.body().line(&format!("br_if {}", copy_done));
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", copy));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.store8");
    emit_increment_local(out_idx, module);
    emit_increment_local(&copy, module);
    module.body().line(&format!("br {}", copy_loop));
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
    emit_increment_local(&scan, module);
    module.body().line(&format!("br {}", parse_loop));
    module.body().close("end");
    module.body().line(&format!("br {}", parse_done));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_html_escape_dynamic(
    var: &str,
    flags_expr: &Expr,
    double_encode: RuntimeBoolArg<'_>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let flags = module.next_label("html_flags");
    let idx = module.next_label("html_idx");
    let byte = module.next_label("html_byte");
    let loop_label = module.next_label("html_loop");
    let done_label = module.next_label("html_done");
    let (amp_ptr, amp_len) = module.intern_string("&amp;");
    let (quot_ptr, quot_len) = module.intern_string("&quot;");
    let (apos_ptr, apos_len) = module.intern_string("&#039;");
    let (lt_ptr, lt_len) = module.intern_string("&lt;");
    let (gt_ptr, gt_len) = module.intern_string("&gt;");
    module.declare_i64_local(flags.trim_start_matches('$').to_string());
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
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
    emit_runtime_html_amp_escape(var, &idx, double_encode, &loop_label, amp_ptr, amp_len, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 34");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_runtime_html_flagged_quote(&flags, 2, quot_ptr, quot_len, &byte, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 39");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_runtime_html_flagged_quote(&flags, 1, apos_ptr, apos_len, &byte, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 60");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_write_static_string(lt_ptr, lt_len, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 62");
    module.body().line("i32.eq");
    module.body().open("if");
    emit_write_static_string(gt_ptr, gt_len, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    emit_write_stack_byte(module);
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_runtime_html_amp_escape(
    var: &str,
    idx: &str,
    double_encode: RuntimeBoolArg<'_>,
    loop_label: &str,
    amp_ptr: usize,
    amp_len: usize,
    module: &mut WasmModule,
) {
    emit_runtime_bool_value(double_encode, module);
    module.body().open("if");
    emit_write_static_string(amp_ptr, amp_len, module);
    module.body().line("else");
    for entity in ["&amp;", "&lt;", "&gt;", "&quot;", "&#039;", "&#39;"] {
        emit_runtime_html_preserve_entity_case(var, idx, entity, loop_label, module);
    }
    emit_runtime_html_preserve_numeric_entity_case(var, idx, loop_label, module);
    emit_write_static_string(amp_ptr, amp_len, module);
    module.body().close("end");
}

pub(super) fn emit_runtime_htmlentities_utf8_escape_case(
    var: &str,
    idx: &str,
    decoded: &str,
    entity: &str,
    loop_label: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", decoded.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.le_u");
    module.body().open("if");
    emit_runtime_literal_match_at(var, idx, decoded, module);
    module.body().open("if");
    let (entity_ptr, entity_len) = module.intern_string(entity);
    emit_write_static_string(entity_ptr, entity_len, module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", decoded.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_htmlentities_utf8_escape_case_to_memory(
    var: &str,
    idx: &str,
    decoded: &str,
    entity: &str,
    out_ptr: &str,
    out_idx: &str,
    loop_label: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", decoded.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.le_u");
    module.body().open("if");
    emit_runtime_literal_match_at(var, idx, decoded, module);
    module.body().open("if");
    let (entity_ptr, entity_len) = module.intern_string(entity);
    emit_copy_static_range_to_memory(entity_ptr, entity_len, out_ptr, out_idx, module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", decoded.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_bool_value(value: RuntimeBoolArg<'_>, module: &mut WasmModule) {
    match value {
        RuntimeBoolArg::Static(value) => {
            module.body().line(&format!("i32.const {}", i32::from(value)));
        }
        RuntimeBoolArg::Variable(var) => {
            module.body().line(&format!("local.get ${}", var));
        }
    }
}

pub(super) fn emit_runtime_html_flagged_quote(
    flags: &str,
    bit: i64,
    entity_ptr: usize,
    entity_len: usize,
    byte: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", flags));
    module.body().line(&format!("i64.const {}", bit));
    module.body().line("i64.and");
    module.body().line("i64.eqz");
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_write_static_string(entity_ptr, entity_len, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", byte));
    emit_write_stack_byte(module);
    module.body().close("end");
}
