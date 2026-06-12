//! Purpose:
//! Emits wasm32-web helpers that compare PHP scalar cells by their string representation.
//! Keeps string-representation coercion separate from set-membership orchestration.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets::value_compare`.
//!
//! Key details:
//! - Helpers preserve PHP loose scalar/string comparison behavior and boxed value-cell layout.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_value_string_cell_same_scalar_string_repr_between(
    string_source_ptr: &str,
    string_index: &str,
    scalar_source_ptr: &str,
    scalar_index: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    let string_ptr = module.next_label("value_string_scalar_cmp_string_ptr");
    let string_len = module.next_label("value_string_scalar_cmp_string_len");
    let scalar_tag = module.next_label("value_string_scalar_cmp_scalar_tag");
    let scalar_payload = module.next_label("value_string_scalar_cmp_scalar_payload");
    let scalar_ptr = module.next_label("value_string_scalar_cmp_scalar_ptr");
    let scalar_len = module.next_label("value_string_scalar_cmp_scalar_len");
    for local in [&string_ptr, &string_len, &scalar_tag, &scalar_ptr, &scalar_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(scalar_payload.trim_start_matches('$').to_string());
    emit_load_value_cell_string_part(string_source_ptr, string_index, 8, &string_ptr, module);
    emit_load_value_cell_string_part(string_source_ptr, string_index, 12, &string_len, module);
    emit_load_value_cell_tag(scalar_source_ptr, scalar_index, &scalar_tag, module);
    emit_load_value_cell_payload(scalar_source_ptr, scalar_index, &scalar_payload, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    emit_string_parts_match_empty_scalar(&string_len, &scalar_tag, &scalar_payload, matched, module);
    emit_string_parts_match_true_scalar(&string_ptr, &string_len, &scalar_tag, &scalar_payload, matched, module);
    module.body().line(&format!("local.get {}", scalar_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_i64_local_string_cast_to_locals(&scalar_payload, &scalar_ptr, &scalar_len, module);
    emit_string_parts_equal(&string_ptr, &string_len, &scalar_ptr, &scalar_len, matched, module);
    module.body().close("end");
}

pub(in crate::codegen::wasm::expr) fn emit_value_string_cell_ptr_same_scalar_string_repr_between(
    string_cell: &str,
    scalar_cell: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    let string_ptr = module.next_label("value_ptr_string_scalar_cmp_string_ptr");
    let string_len = module.next_label("value_ptr_string_scalar_cmp_string_len");
    let scalar_tag = module.next_label("value_ptr_string_scalar_cmp_scalar_tag");
    let scalar_payload = module.next_label("value_ptr_string_scalar_cmp_scalar_payload");
    let scalar_ptr = module.next_label("value_ptr_string_scalar_cmp_scalar_ptr");
    let scalar_len = module.next_label("value_ptr_string_scalar_cmp_scalar_len");
    let numeric_out = module.next_label("value_ptr_string_scalar_cmp_numeric_out");
    let string_is_numeric = module.next_label("value_ptr_string_scalar_cmp_is_numeric");
    let string_number = module.next_label("value_ptr_string_scalar_cmp_string_number");
    let scalar_number = module.next_label("value_ptr_string_scalar_cmp_scalar_number");
    for local in [
        &string_ptr,
        &string_len,
        &scalar_tag,
        &scalar_ptr,
        &scalar_len,
        &numeric_out,
        &string_is_numeric,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(scalar_payload.trim_start_matches('$').to_string());
    for local in [&string_number, &scalar_number] {
        module.declare_f64_local(local.trim_start_matches('$').to_string());
    }
    emit_load_value_cell_ptr_string_part(string_cell, 8, &string_ptr, module);
    emit_load_value_cell_ptr_string_part(string_cell, 12, &string_len, module);
    emit_load_value_cell_ptr_tag(scalar_cell, &scalar_tag, module);
    emit_load_value_cell_ptr_payload(scalar_cell, &scalar_payload, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    emit_string_parts_match_empty_scalar(&string_len, &scalar_tag, &scalar_payload, matched, module);
    emit_string_parts_match_true_scalar(&string_ptr, &string_len, &scalar_tag, &scalar_payload, matched, module);
    module.body().line(&format!("local.get {}", scalar_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_i64_local_string_cast_to_locals(&scalar_payload, &scalar_ptr, &scalar_len, module);
    emit_string_parts_equal(&string_ptr, &string_len, &scalar_ptr, &scalar_len, matched, module);
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", numeric_out));
    module.body().line("global.get $heap");
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get {}", string_ptr));
    module.body().line(&format!("local.get {}", string_len));
    module.body().line(&format!("local.get {}", numeric_out));
    module.body().line("call $host_numeric_string_value");
    module.body().line(&format!("local.set {}", string_is_numeric));
    module.body().line(&format!("local.get {}", string_is_numeric));
    module.body().open("if");
    module.body().line(&format!("local.get {}", numeric_out));
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", string_number));
    module.body().line(&format!("local.get {}", scalar_payload));
    module.body().line("f64.convert_i64_s");
    module.body().line(&format!("local.set {}", scalar_number));
    module.body().line(&format!("local.get {}", string_number));
    module.body().line(&format!("local.get {}", scalar_number));
    module.body().line("f64.eq");
    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", matched));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", scalar_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", numeric_out));
    module.body().line("global.get $heap");
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get {}", string_ptr));
    module.body().line(&format!("local.get {}", string_len));
    module.body().line(&format!("local.get {}", numeric_out));
    module.body().line("call $host_numeric_string_value");
    module.body().line(&format!("local.set {}", string_is_numeric));
    module.body().line(&format!("local.get {}", string_is_numeric));
    module.body().open("if");
    module.body().line(&format!("local.get {}", numeric_out));
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", string_number));
    module.body().line(&format!("local.get {}", scalar_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", scalar_number));
    module.body().line(&format!("local.get {}", string_number));
    module.body().line(&format!("local.get {}", scalar_number));
    module.body().line("f64.eq");
    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", matched));
    module.body().close("end");
    module.body().close("end");
}

pub(in crate::codegen::wasm::expr) fn emit_string_parts_match_empty_scalar(
    string_len: &str,
    scalar_tag: &str,
    scalar_payload: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", string_len));
    module.body().line("i32.eqz");
    emit_value_scalar_cell_is_empty_string_repr(scalar_tag, scalar_payload, module);
    module.body().line("i32.and");
    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", matched));
}

pub(in crate::codegen::wasm::expr) fn emit_string_parts_match_true_scalar(
    string_ptr: &str,
    string_len: &str,
    scalar_tag: &str,
    scalar_payload: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", string_len));
    module.body().line("i32.const 1");
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", string_ptr));
    module.body().line("i32.load8_u");
    module.body().line("i32.const 49");
    module.body().line("i32.eq");
    module.body().line("i32.and");
    emit_value_scalar_cell_is_one_string_repr(scalar_tag, scalar_payload, module);
    module.body().line("i32.and");
    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", matched));
}

pub(in crate::codegen::wasm::expr) fn emit_i64_local_string_cast_to_locals(
    input_value: &str,
    out_ptr: &str,
    out_len: &str,
    module: &mut WasmModule,
) {
    let value = module.next_label("string_cast_cell_value");
    let digit = module.next_label("string_cast_cell_digit");
    let negative = module.next_label("string_cast_cell_negative");
    let write = module.next_label("string_cast_cell_write");
    let loop_label = module.next_label("string_cast_cell_loop");
    let done_label = module.next_label("string_cast_cell_done");
    for local in [&value, &digit] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    for local in [&negative, &write] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get {}", input_value));
    module.body().line(&format!("local.set {}", value));
    module.body().line(&format!("local.get {}", value));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().line(&format!("local.set {}", negative));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line("i32.const 20");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("i32.const 20");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", write));
    module.body().line(&format!("local.get {}", value));
    module.body().line("i64.eqz");
    module.body().open("if");
    module.body().line(&format!("local.get {}", write));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", write));
    module.body().line(&format!("local.get {}", write));
    module.body().line("i32.const 48");
    module.body().line("i32.store8");
    module.body().line("else");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", value));
    module.body().line("i64.eqz");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", value));
    module.body().line("i64.const 10");
    module.body().line("i64.rem_s");
    module.body().line(&format!("local.set {}", digit));
    module.body().line(&format!("local.get {}", digit));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().open("if");
    module.body().line("i64.const 0");
    module.body().line(&format!("local.get {}", digit));
    module.body().line("i64.sub");
    module.body().line(&format!("local.set {}", digit));
    module.body().close("end");
    module.body().line(&format!("local.get {}", write));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", write));
    module.body().line(&format!("local.get {}", write));
    module.body().line(&format!("local.get {}", digit));
    module.body().line("i32.wrap_i64");
    module.body().line("i32.const 48");
    module.body().line("i32.add");
    module.body().line("i32.store8");
    module.body().line(&format!("local.get {}", value));
    module.body().line("i64.const 10");
    module.body().line("i64.div_s");
    module.body().line(&format!("local.set {}", value));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", negative));
    module.body().open("if");
    module.body().line(&format!("local.get {}", write));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", write));
    module.body().line(&format!("local.get {}", write));
    module.body().line("i32.const 45");
    module.body().line("i32.store8");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("i32.const 20");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", write));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.get {}", write));
    module.body().line(&format!("local.set {}", out_ptr));
}

pub(in crate::codegen::wasm::expr) fn emit_string_parts_equal(
    left_ptr: &str,
    left_len: &str,
    right_ptr: &str,
    right_len: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    let offset = module.next_label("string_parts_equal_offset");
    let done_label = module.next_label("string_parts_equal_done");
    let loop_label = module.next_label("string_parts_equal_loop");
    module.declare_i32_local(offset.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("local.get {}", left_len));
    module.body().line(&format!("local.get {}", right_len));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", matched));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", offset));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", offset));
    module.body().line(&format!("local.get {}", left_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", left_ptr));
    module.body().line(&format!("local.get {}", offset));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.get {}", right_ptr));
    module.body().line(&format!("local.get {}", offset));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", offset));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", offset));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

pub(in crate::codegen::wasm::expr) fn emit_value_string_cell_ptrs_equal_between(
    left_cell: &str,
    right_cell: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    let left_ptr = module.next_label("value_ptr_unique_left_ptr");
    let left_len = module.next_label("value_ptr_unique_left_len");
    let right_ptr = module.next_label("value_ptr_unique_right_ptr");
    let right_len = module.next_label("value_ptr_unique_right_len");
    for local in [&left_ptr, &left_len, &right_ptr, &right_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_load_value_cell_ptr_string_part(left_cell, 8, &left_ptr, module);
    emit_load_value_cell_ptr_string_part(left_cell, 12, &left_len, module);
    emit_load_value_cell_ptr_string_part(right_cell, 8, &right_ptr, module);
    emit_load_value_cell_ptr_string_part(right_cell, 12, &right_len, module);
    emit_string_parts_equal(&left_ptr, &left_len, &right_ptr, &right_len, matched, module);
}

pub(in crate::codegen::wasm::expr) fn emit_value_scalar_cells_same_string_repr_between(
    left_source_ptr: &str,
    left_index: &str,
    right_source_ptr: &str,
    right_index: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    let left_tag = module.next_label("value_scalar_cmp_left_tag");
    let right_tag = module.next_label("value_scalar_cmp_right_tag");
    let left_payload = module.next_label("value_scalar_cmp_left_payload");
    let right_payload = module.next_label("value_scalar_cmp_right_payload");
    for local in [&left_tag, &right_tag] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left_payload, &right_payload] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    emit_load_value_cell_tag(left_source_ptr, left_index, &left_tag, module);
    emit_load_value_cell_tag(right_source_ptr, right_index, &right_tag, module);
    emit_load_value_cell_payload(left_source_ptr, left_index, &left_payload, module);
    emit_load_value_cell_payload(right_source_ptr, right_index, &right_payload, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    emit_value_scalar_cell_is_empty_string_repr(&left_tag, &left_payload, module);
    emit_value_scalar_cell_is_empty_string_repr(&right_tag, &right_payload, module);
    module.body().line("i32.and");
    module.body().line(&format!("local.set {}", matched));
    emit_value_scalar_cell_is_one_string_repr(&left_tag, &left_payload, module);
    emit_value_scalar_cell_is_one_string_repr(&right_tag, &right_payload, module);
    module.body().line("i32.and");
    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("local.get {}", left_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", right_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().line("i32.and");
    module.body().open("if");
    module.body().line(&format!("local.get {}", left_payload));
    module.body().line(&format!("local.get {}", right_payload));
    module.body().line("i64.eq");
    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", matched));
    module.body().close("end");
    module.body().line(&format!("local.get {}", left_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", right_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().line("i32.and");
    module.body().open("if");
    module.body().line(&format!("local.get {}", left_payload));
    module.body().line(&format!("local.get {}", right_payload));
    module.body().line("i64.eq");
    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", matched));
    module.body().close("end");
}

pub(in crate::codegen::wasm::expr) fn emit_value_scalar_cell_ptrs_same_string_repr_between(
    left_cell: &str,
    right_cell: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    let left_tag = module.next_label("value_ptr_scalar_cmp_left_tag");
    let right_tag = module.next_label("value_ptr_scalar_cmp_right_tag");
    let left_payload = module.next_label("value_ptr_scalar_cmp_left_payload");
    let right_payload = module.next_label("value_ptr_scalar_cmp_right_payload");
    for local in [&left_tag, &right_tag] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left_payload, &right_payload] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    emit_load_value_cell_ptr_tag(left_cell, &left_tag, module);
    emit_load_value_cell_ptr_tag(right_cell, &right_tag, module);
    emit_load_value_cell_ptr_payload(left_cell, &left_payload, module);
    emit_load_value_cell_ptr_payload(right_cell, &right_payload, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    emit_value_scalar_cell_is_empty_string_repr(&left_tag, &left_payload, module);
    emit_value_scalar_cell_is_empty_string_repr(&right_tag, &right_payload, module);
    module.body().line("i32.and");
    module.body().line(&format!("local.set {}", matched));
    emit_value_scalar_cell_is_one_string_repr(&left_tag, &left_payload, module);
    emit_value_scalar_cell_is_one_string_repr(&right_tag, &right_payload, module);
    module.body().line("i32.and");
    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("local.get {}", left_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", right_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().line("i32.and");
    module.body().open("if");
    module.body().line(&format!("local.get {}", left_payload));
    module.body().line(&format!("local.get {}", right_payload));
    module.body().line("i64.eq");
    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", matched));
    module.body().close("end");
    module.body().line(&format!("local.get {}", left_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", right_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().line("i32.and");
    module.body().open("if");
    module.body().line(&format!("local.get {}", left_payload));
    module.body().line(&format!("local.get {}", right_payload));
    module.body().line("i64.eq");
    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", matched));
    module.body().close("end");
}
