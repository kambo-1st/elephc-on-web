//! Purpose:
//! Emits wasm32-web array transform helpers for value-cell string/scalar arrays.
//! Keeps array_unique/diff/intersect/flip value-cell lowering separate from assoc transforms.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets`.
//!
//! Key details:
//! - Helpers preserve PHP string-representation equality and retain exact metadata when available.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_known_value_string_array_value_set_assign(
    name: &str,
    source: &str,
    function_name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let known_kinds = module.array_value_cell_kinds(source);
    let runtime_kind = module.array_runtime_value_cell_kind(source);
    let exact_metadata = exact_indexed_value_set_metadata(source, function_name, args, module);
    let source_all_strings = if let Some(kinds) = known_kinds {
        if kinds.iter().any(|kind| matches!(kind, ValueCellKind::Array)) {
            return Err(CompileError::new(
                args[0].span,
                &format!("wasm32-web {function_name}() on value-cell arrays currently rejects array values"),
            ));
        }
        kinds.iter().all(|kind| matches!(kind, ValueCellKind::Str))
    } else if let Some(kind) = runtime_kind {
        if !matches!(
            kind,
            ValueCellKind::Int
                | ValueCellKind::Float
                | ValueCellKind::Bool
                | ValueCellKind::Null
                | ValueCellKind::Str
        ) {
            return Err(CompileError::new(
                args[0].span,
                &format!("wasm32-web {function_name}() on runtime value-cell arrays currently requires scalar values"),
            ));
        }
        matches!(kind, ValueCellKind::Str)
    } else {
        false
    };
    let compare_sets =
        prepare_value_string_compare_sources(function_name, args, source_all_strings, module)?;
    let keep_matching = function_name.eq_ignore_ascii_case("array_intersect");
    let source_ptr = preserve_array_ptr(source, function_name, module);
    let source_len = module.next_label("value_array_set_source_len");
    let index = module.next_label("value_array_set_index");
    let out_index = module.next_label("value_array_set_out_index");
    let found = module.next_label("value_array_set_found");
    let target_entry = module.next_label("value_array_set_target_entry");
    let done_label = module.next_label("value_array_set_done");
    let loop_label = module.next_label("value_array_set_loop");
    for local in [&index, &out_index, &found, &target_entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i32_local(source_len.trim_start_matches('$').to_string());
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_key_kinds(name, None);
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    if let Some(len) = module.array_length(source) {
        module.body().line(&format!("i32.const {}", len));
    } else {
        module.body().line(&format!("local.get ${}_len", source));
    }
    module.body().line(&format!("local.set {}", source_len));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_trap_if_value_cell_is_array(&source_ptr, &index, module);
    emit_value_string_set_membership(&source_ptr, &index, &found, &compare_sets, keep_matching, module);
    module.body().line(&format!("local.get {}", found));
    if !keep_matching {
        module.body().line("i32.eqz");
    }
    module.body().open("if");
    emit_store_value_cell_assoc_entry(name, &target_entry, &out_index, &source_ptr, &index, module);
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_index));
    module.body().line(&format!("local.set ${}_len", name));
    module.clear_array_length(name);
    module.set_array_runtime_value_cell_kind(name, runtime_kind);
    if let Some(metadata) = exact_metadata {
        apply_exact_unique_metadata(name, metadata, module);
    }
    Ok(())
}

pub(in crate::codegen::wasm::expr) fn emit_known_value_string_array_unique_assign(
    name: &str,
    source: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let sort_mode = array_unique_sort_mode(args, args[0].span, module)?;
    let known_kinds = module.array_value_cell_kinds(source);
    let runtime_kind = module.array_runtime_value_cell_kind(source);
    let exact_metadata = exact_unique_indexed_value_metadata(source, sort_mode, module);
    if let Some(kinds) = known_kinds {
        if kinds.iter().any(|kind| matches!(kind, ValueCellKind::Array)) {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_unique() on value-cell arrays currently rejects array values",
            ));
        }
    } else if let Some(kind) = runtime_kind {
        if !matches!(
            kind,
            ValueCellKind::Int
                | ValueCellKind::Float
                | ValueCellKind::Bool
                | ValueCellKind::Null
                | ValueCellKind::Str
        ) {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_unique() on runtime value-cell arrays currently requires scalar values",
            ));
        }
    }
    let source_ptr = preserve_array_ptr(source, "array_unique", module);
    let source_len = module.next_label("value_array_unique_source_len");
    let index = module.next_label("value_array_unique_index");
    let scan = module.next_label("value_array_unique_scan");
    let out_index = module.next_label("value_array_unique_out_index");
    let seen = module.next_label("value_array_unique_seen");
    let equal = module.next_label("value_array_unique_equal");
    let target_entry = module.next_label("value_array_unique_target_entry");
    let scan_entry = module.next_label("value_array_unique_scan_entry");
    let done_label = module.next_label("value_array_unique_done");
    let loop_label = module.next_label("value_array_unique_loop");
    let scan_done_label = module.next_label("value_array_unique_scan_done");
    let scan_loop_label = module.next_label("value_array_unique_scan_loop");
    for local in [&index, &scan, &out_index, &seen, &equal, &target_entry, &scan_entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i32_local(source_len.trim_start_matches('$').to_string());
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_key_kinds(name, None);
    if let Some(len) = module.array_length(source) {
        module.body().line(&format!("i32.const {}", len));
    } else {
        module.body().line(&format!("local.get ${}_len", source));
    }
    module.body().line(&format!("local.set {}", source_len));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_trap_if_value_cell_is_array(&source_ptr, &index, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", seen));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", scan));
    module.body().open(&format!("block {}", scan_done_label));
    module.body().open(&format!("loop {}", scan_loop_label));
    module.body().line(&format!("local.get {}", scan));
    if sort_mode == ArrayUniqueSortMode::Regular {
        module.body().line(&format!("local.get {}", out_index));
    } else {
        module.body().line(&format!("local.get {}", index));
    }
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", scan_done_label));
    match sort_mode {
        ArrayUniqueSortMode::String => {
            emit_value_cells_same_string_repr_between(&source_ptr, &index, &source_ptr, &scan, &equal, module);
        }
        ArrayUniqueSortMode::Numeric => {
            emit_value_cells_same_sort_numeric_between(&source_ptr, &index, &source_ptr, &scan, &equal, module);
        }
        ArrayUniqueSortMode::Regular => {
            emit_assoc_entry_address(&format!("${}_ptr", name), &scan, &scan_entry, module);
            emit_value_cell_and_assoc_entry_same_php_regular(&source_ptr, &index, &scan_entry, &equal, module);
        }
    }
    module.body().line(&format!("local.get {}", equal));
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", seen));
    module.body().line(&format!("br {}", scan_done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().line(&format!("br {}", scan_loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", seen));
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_store_value_cell_assoc_entry(name, &target_entry, &out_index, &source_ptr, &index, module);
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_index));
    module.body().line(&format!("local.set ${}_len", name));
    module.clear_array_length(name);
    module.set_array_runtime_value_cell_kind(name, runtime_kind);
    if let Some(metadata) = exact_metadata {
        apply_exact_unique_metadata(name, metadata, module);
    }
    Ok(())
}

fn emit_value_cell_and_assoc_entry_same_php_regular(
    source_ptr: &str,
    index: &str,
    entry: &str,
    equal: &str,
    module: &mut WasmModule,
) {
    let left_cell = module.next_label("value_unique_regular_left_cell");
    let right_cell = module.next_label("value_unique_regular_right_cell");
    for local in [&left_cell, &right_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_value_cell_address(source_ptr, index, &left_cell, module);
    emit_assoc_value_cell_address(entry, &right_cell, module);
    emit_value_cell_ptrs_php_loose_equal_between(&left_cell, &right_cell, equal, module);
}

fn emit_value_cells_same_sort_numeric_between(
    left_source_ptr: &str,
    left_index: &str,
    right_source_ptr: &str,
    right_index: &str,
    equal: &str,
    module: &mut WasmModule,
) {
    let left_cell = module.next_label("value_unique_numeric_left_cell");
    let right_cell = module.next_label("value_unique_numeric_right_cell");
    let left_number = module.next_label("value_unique_numeric_left_number");
    let right_number = module.next_label("value_unique_numeric_right_number");
    for local in [&left_cell, &right_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left_number, &right_number] {
        module.declare_f64_local(local.trim_start_matches('$').to_string());
    }
    emit_value_cell_address(left_source_ptr, left_index, &left_cell, module);
    emit_value_cell_address(right_source_ptr, right_index, &right_cell, module);
    emit_load_value_cell_sort_numeric_value(&left_cell, &left_number, module);
    emit_load_value_cell_sort_numeric_value(&right_cell, &right_number, module);
    module.body().line(&format!("local.get {}", left_number));
    module.body().line(&format!("local.get {}", right_number));
    module.body().line("f64.eq");
    module.body().line(&format!("local.set {}", equal));
}

fn emit_load_value_cell_sort_numeric_value(cell: &str, target: &str, module: &mut WasmModule) {
    let tag = module.next_label("value_unique_numeric_tag");
    module.declare_i32_local(tag.trim_start_matches('$').to_string());
    emit_load_value_cell_ptr_tag(cell, &tag, module);
    module.body().line("f64.const 0");
    module.body().line(&format!("local.set {}", target));
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("call $__rt_value_cell_payload_i64");
    module.body().line("f64.convert_i64_s");
    module.body().line(&format!("local.set {}", target));
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("call $__rt_value_cell_payload_f64");
    module.body().line(&format!("local.set {}", target));
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_value_cell_string_leading_numeric_value(cell, target, module);
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

pub(in crate::codegen::wasm::expr) fn emit_value_string_cells_equal_between(
    left_source_ptr: &str,
    left_index: &str,
    right_source_ptr: &str,
    right_index: &str,
    equal: &str,
    module: &mut WasmModule,
) {
    let left_ptr = module.next_label("value_unique_left_ptr");
    let left_len = module.next_label("value_unique_left_len");
    let right_ptr = module.next_label("value_unique_right_ptr");
    let right_len = module.next_label("value_unique_right_len");
    let offset = module.next_label("value_unique_offset");
    let done_label = module.next_label("value_unique_cmp_done");
    let loop_label = module.next_label("value_unique_cmp_loop");
    for local in [&left_ptr, &left_len, &right_ptr, &right_len, &offset] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_load_value_cell_string_part(left_source_ptr, left_index, 8, &left_ptr, module);
    emit_load_value_cell_string_part(left_source_ptr, left_index, 12, &left_len, module);
    emit_load_value_cell_string_part(right_source_ptr, right_index, 8, &right_ptr, module);
    emit_load_value_cell_string_part(right_source_ptr, right_index, 12, &right_len, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", equal));
    module.body().line(&format!("local.get {}", left_len));
    module.body().line(&format!("local.get {}", right_len));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", equal));
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
    module.body().line(&format!("local.set {}", equal));
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

pub(in crate::codegen::wasm::expr) fn emit_known_value_string_array_flip_assign(
    name: &str,
    source: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let known_kinds = module.array_value_cell_kinds(source);
    let runtime_kind = module.array_runtime_value_cell_kind(source);
    let exact_metadata = exact_indexed_value_flip_metadata(source, module);
    if let Some(kinds) = known_kinds {
        if kinds.iter().any(|kind| matches!(kind, ValueCellKind::Array)) {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_flip() on value-cell arrays currently rejects array values",
            ));
        }
    } else if matches!(runtime_kind, Some(kind) if !matches!(kind, ValueCellKind::Int | ValueCellKind::Str)) {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_flip() on runtime value-cell arrays currently requires integer or string values",
        ));
    }
    let source_ptr = preserve_array_ptr(source, "array_flip", module);
    let source_len = module.next_label("value_array_flip_source_len");
    let index = module.next_label("value_array_flip_index");
    let scan = module.next_label("value_array_flip_scan");
    let out_index = module.next_label("value_array_flip_out_index");
    let matched = module.next_label("value_array_flip_matched");
    let found = module.next_label("value_array_flip_found");
    let supported = module.next_label("value_array_flip_supported");
    let target_entry = module.next_label("value_array_flip_target_entry");
    let scan_entry = module.next_label("value_array_flip_scan_entry");
    let done_label = module.next_label("value_array_flip_done");
    let loop_label = module.next_label("value_array_flip_loop");
    let scan_done_label = module.next_label("value_array_flip_scan_done");
    let scan_loop_label = module.next_label("value_array_flip_scan_loop");
    for local in [
        &index,
        &scan,
        &out_index,
        &matched,
        &found,
        &supported,
        &target_entry,
        &scan_entry,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i32_local(source_len.trim_start_matches('$').to_string());
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_key_kinds(name, None);
    module.set_array_php_normalized_runtime_keys(name, true);
    if let Some(len) = module.array_length(source) {
        module.body().line(&format!("i32.const {}", len));
    } else {
        module.body().line(&format!("local.get ${}_len", source));
    }
    module.body().line(&format!("local.set {}", source_len));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_is_int_or_string(&source_ptr, &index, &supported, module);
    module.body().line(&format!("local.get {}", supported));
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", scan));
    module.body().open(&format!("block {}", scan_done_label));
    module.body().open(&format!("loop {}", scan_loop_label));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", scan_done_label));
    emit_assoc_entry_address(&format!("${}_ptr", name), &scan, &scan_entry, module);
    emit_assoc_entry_key_matches_value_cell(&scan_entry, &source_ptr, &index, &matched, module);
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("local.get {}", scan_entry));
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("br {}", scan_done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().line(&format!("br {}", scan_loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_assoc_entry_address(&format!("${}_ptr", name), &out_index, &target_entry, module);
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    module.body().close("end");
    emit_store_value_flip_entry(&target_entry, &source_ptr, &index, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_index));
    module.body().line(&format!("local.set ${}_len", name));
    module.clear_array_length(name);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Int));
    if let Some(metadata) = exact_metadata {
        apply_exact_unique_metadata(name, metadata, module);
    }
    Ok(())
}
