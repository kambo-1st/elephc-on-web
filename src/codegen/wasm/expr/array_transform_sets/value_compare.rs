//! Purpose:
//! Emits value-cell comparison and set-membership helpers for wasm32-web array transforms.
//! Handles PHP string-representation matching for array_diff, array_intersect, unique, and search surfaces.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets`.
//! - Sibling array search/contains emitters through the array transform helper re-export.
//!
//! Key details:
//! - PHP loose comparison and scalar string representation rules must remain centralized here.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_trap_if_value_cell_is_array(
    source_ptr: &str,
    index: &str,
    module: &mut WasmModule,
) {
    let tag = module.next_label("value_cell_array_guard_tag");
    module.declare_i32_local(tag.trim_start_matches('$').to_string());
    emit_load_value_cell_tag(source_ptr, index, &tag, module);
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
}

pub(in crate::codegen::wasm::expr) fn emit_trap_if_value_cell_ptr_is_array(
    cell: &str,
    module: &mut WasmModule,
) {
    let tag = module.next_label("value_cell_ptr_array_guard_tag");
    module.declare_i32_local(tag.trim_start_matches('$').to_string());
    emit_load_value_cell_ptr_tag(cell, &tag, module);
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
}

pub(in crate::codegen::wasm::expr) fn emit_value_string_set_membership(
    source_ptr: &str,
    index: &str,
    found: &str,
    compare_sets: &[ValueStringCompareSource],
    require_all_sets: bool,
    module: &mut WasmModule,
) {
    module
        .body()
        .line(&format!("i32.const {}", i32::from(require_all_sets)));
    module.body().line(&format!("local.set {}", found));
    for set in compare_sets {
        let set_found = module.next_label("value_array_set_one_found");
        let candidate_found = module.next_label("value_array_set_candidate_found");
        let compare_index = module.next_label("value_array_set_compare_index");
        module.declare_i32_local(set_found.trim_start_matches('$').to_string());
        module.declare_i32_local(candidate_found.trim_start_matches('$').to_string());
        module.declare_i32_local(compare_index.trim_start_matches('$').to_string());
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", set_found));
        match set {
            ValueStringCompareSource::Static(values) => {
                for candidate in values {
                    emit_value_cell_matches_static_string_repr(
                        source_ptr,
                        index,
                        candidate,
                        &candidate_found,
                        module,
                    );
                    module.body().line(&format!("local.get {}", set_found));
                    module.body().line(&format!("local.get {}", candidate_found));
                    module.body().line("i32.or");
                    module.body().line(&format!("local.set {}", set_found));
                }
            }
            ValueStringCompareSource::RuntimeCompactInt { ptr, len } => {
                emit_value_set_runtime_compact_int_source_match(
                    source_ptr,
                    index,
                    ptr,
                    len,
                    &set_found,
                    &candidate_found,
                    &compare_index,
                    module,
                );
            }
            ValueStringCompareSource::Runtime { ptr, len } => {
                emit_value_string_set_runtime_source_match(
                    source_ptr,
                    index,
                    ptr,
                    len,
                    &set_found,
                    &candidate_found,
                    &compare_index,
                    module,
                );
            }
            ValueStringCompareSource::RuntimeScalar { ptr, len } => {
                emit_value_scalar_set_runtime_source_match(
                    source_ptr,
                    index,
                    ptr,
                    len,
                    &set_found,
                    &candidate_found,
                    &compare_index,
                    module,
                );
            }
            ValueStringCompareSource::RuntimeAny { ptr, len } => {
                emit_value_any_set_runtime_source_match(
                    source_ptr,
                    index,
                    ptr,
                    len,
                    &set_found,
                    &candidate_found,
                    &compare_index,
                    module,
                );
            }
        }
        if require_all_sets {
            module.body().line(&format!("local.get {}", found));
            module.body().line(&format!("local.get {}", set_found));
            module.body().line("i32.and");
            module.body().line(&format!("local.set {}", found));
        } else {
            module.body().line(&format!("local.get {}", found));
            module.body().line(&format!("local.get {}", set_found));
            module.body().line("i32.or");
            module.body().line(&format!("local.set {}", found));
        }
    }
}

pub(in crate::codegen::wasm::expr) fn emit_value_cell_matches_i64_string_repr(
    source_ptr: &str,
    index: &str,
    int_value: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    let tag = module.next_label("value_int_cmp_tag");
    let payload = module.next_label("value_int_cmp_payload");
    let int_ptr = module.next_label("value_int_cmp_ptr");
    let int_len = module.next_label("value_int_cmp_len");
    let string_ptr = module.next_label("value_int_cmp_string_ptr");
    let string_len = module.next_label("value_int_cmp_string_len");
    for local in [&tag, &int_ptr, &int_len, &string_ptr, &string_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(payload.trim_start_matches('$').to_string());
    emit_load_value_cell_tag(source_ptr, index, &tag, module);
    emit_load_value_cell_payload(source_ptr, index, &payload, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", payload));
    module.body().line(&format!("local.get {}", int_value));
    module.body().line("i64.eq");
    module.body().line(&format!("local.set {}", matched));
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_load_value_cell_string_part(source_ptr, index, 8, &string_ptr, module);
    emit_load_value_cell_string_part(source_ptr, index, 12, &string_len, module);
    emit_i64_local_string_cast_to_locals(int_value, &int_ptr, &int_len, module);
    emit_string_parts_equal(&string_ptr, &string_len, &int_ptr, &int_len, matched, module);
    module.body().line("else");
    emit_value_scalar_cell_is_empty_string_repr(&tag, &payload, module);
    module.body().line(&format!("local.get {}", int_value));
    module.body().line("i64.eqz");
    module.body().line("i32.and");
    module.body().line(&format!("local.set {}", matched));
    emit_value_scalar_cell_is_one_string_repr(&tag, &payload, module);
    module.body().line(&format!("local.get {}", int_value));
    module.body().line("i64.const 1");
    module.body().line("i64.eq");
    module.body().line("i32.and");
    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", matched));
    module.body().close("end");
    module.body().close("end");
}

pub(in crate::codegen::wasm::expr) fn emit_value_cells_same_string_repr_between(
    left_source_ptr: &str,
    left_index: &str,
    right_source_ptr: &str,
    right_index: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    let left_tag = module.next_label("value_any_cmp_left_tag");
    let right_tag = module.next_label("value_any_cmp_right_tag");
    for local in [&left_tag, &right_tag] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_load_value_cell_tag(left_source_ptr, left_index, &left_tag, module);
    emit_load_value_cell_tag(right_source_ptr, right_index, &right_tag, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("local.get {}", left_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", right_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_value_string_cells_equal_between(
        left_source_ptr,
        left_index,
        right_source_ptr,
        right_index,
        matched,
        module,
    );
    module.body().line("else");
    emit_value_string_cell_same_scalar_string_repr_between(
        left_source_ptr,
        left_index,
        right_source_ptr,
        right_index,
        matched,
        module,
    );
    module.body().close("end");
    module.body().line("else");
    module.body().line(&format!("local.get {}", right_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_value_string_cell_same_scalar_string_repr_between(
        right_source_ptr,
        right_index,
        left_source_ptr,
        left_index,
        matched,
        module,
    );
    module.body().line("else");
    emit_value_scalar_cells_same_string_repr_between(
        left_source_ptr,
        left_index,
        right_source_ptr,
        right_index,
        matched,
        module,
    );
    module.body().close("end");
    module.body().close("end");
}

pub(in crate::codegen::wasm::expr) fn emit_value_cell_ptrs_same_string_repr_between(
    left_cell: &str,
    right_cell: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    let left_tag = module.next_label("value_ptr_cmp_left_tag");
    let right_tag = module.next_label("value_ptr_cmp_right_tag");
    for local in [&left_tag, &right_tag] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_load_value_cell_ptr_tag(left_cell, &left_tag, module);
    emit_load_value_cell_ptr_tag(right_cell, &right_tag, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("local.get {}", left_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", right_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_value_string_cell_ptrs_equal_between(left_cell, right_cell, matched, module);
    module.body().line("else");
    emit_value_string_cell_ptr_same_scalar_string_repr_between(left_cell, right_cell, matched, module);
    module.body().close("end");
    module.body().line("else");
    module.body().line(&format!("local.get {}", right_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_value_string_cell_ptr_same_scalar_string_repr_between(right_cell, left_cell, matched, module);
    module.body().line("else");
    emit_value_scalar_cell_ptrs_same_string_repr_between(left_cell, right_cell, matched, module);
    module.body().close("end");
    module.body().close("end");
}

pub(in crate::codegen::wasm::expr) fn emit_value_cell_matches_static_string_repr(
    source_ptr: &str,
    index: &str,
    candidate: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    emit_value_cell_tag_equals(source_ptr, index, WASM_VALUE_TAG_STRING, module);
    module.body().open("if");
    emit_value_string_cell_matches_static(source_ptr, index, candidate, matched, module);
    module.body().close("end");
    if let Ok(value) = candidate.parse::<i64>() {
        if value.to_string() == candidate {
            emit_value_cell_tag_equals(source_ptr, index, WASM_VALUE_TAG_INT, module);
            module.body().open("if");
            emit_load_value_cell_i64_payload(source_ptr, index, module);
            module.body().line(&format!("i64.const {}", value));
            module.body().line("i64.eq");
            module.body().line(&format!("local.get {}", matched));
            module.body().line("i32.or");
            module.body().line(&format!("local.set {}", matched));
            module.body().close("end");
        }
    }
    if candidate == "1" || candidate.is_empty() {
        emit_value_cell_tag_equals(source_ptr, index, WASM_VALUE_TAG_BOOL, module);
        module.body().open("if");
        emit_load_value_cell_i64_payload(source_ptr, index, module);
        if candidate == "1" {
            module.body().line("i64.const 1");
            module.body().line("i64.eq");
        } else {
            module.body().line("i64.eqz");
        }
        module.body().line(&format!("local.get {}", matched));
        module.body().line("i32.or");
        module.body().line(&format!("local.set {}", matched));
        module.body().close("end");
    }
    if candidate.is_empty() {
        emit_value_cell_tag_equals(source_ptr, index, WASM_VALUE_TAG_NULL, module);
        module.body().line(&format!("local.get {}", matched));
        module.body().line("i32.or");
        module.body().line(&format!("local.set {}", matched));
    }
}

pub(in crate::codegen::wasm::expr) fn emit_value_string_cell_matches_static(
    source_ptr: &str,
    index: &str,
    candidate: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    let (candidate_ptr, candidate_len) = module.intern_string(candidate);
    let value_ptr = module.next_label("value_set_string_ptr");
    let value_len = module.next_label("value_set_string_len");
    let offset = module.next_label("value_set_string_offset");
    let done_label = module.next_label("value_set_string_done");
    let loop_label = module.next_label("value_set_string_loop");
    for local in [&value_ptr, &value_len, &offset] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_load_value_cell_string_part(source_ptr, index, 8, &value_ptr, module);
    emit_load_value_cell_string_part(source_ptr, index, 12, &value_len, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("local.get {}", value_len));
    module.body().line(&format!("i32.const {}", candidate_len));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", matched));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", offset));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", offset));
    module.body().line(&format!("i32.const {}", candidate_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", value_ptr));
    module.body().line(&format!("local.get {}", offset));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("i32.const {}", candidate_ptr));
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

pub(in crate::codegen::wasm::expr) fn emit_value_set_membership(
    value: &str,
    found: &str,
    compare_sets: &[Vec<i64>],
    require_all_sets: bool,
    module: &mut WasmModule,
) {
    module
        .body()
        .line(&format!("i32.const {}", i32::from(require_all_sets)));
    module.body().line(&format!("local.set {}", found));
    for values in compare_sets {
        let set_found = module.next_label("array_value_set_one_found");
        module.declare_i32_local(set_found.trim_start_matches('$').to_string());
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", set_found));
        for candidate in values {
            module.body().line(&format!("local.get {}", value));
            module.body().line(&format!("i64.const {}", candidate));
            module.body().line("i64.eq");
            module.body().line(&format!("local.get {}", set_found));
            module.body().line("i32.or");
            module.body().line(&format!("local.set {}", set_found));
        }
        if require_all_sets {
            module.body().line(&format!("local.get {}", found));
            module.body().line(&format!("local.get {}", set_found));
            module.body().line("i32.and");
            module.body().line(&format!("local.set {}", found));
        } else {
            module.body().line(&format!("local.get {}", found));
            module.body().line(&format!("local.get {}", set_found));
            module.body().line("i32.or");
            module.body().line(&format!("local.set {}", found));
        }
    }
}
