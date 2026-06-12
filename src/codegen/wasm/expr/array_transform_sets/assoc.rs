//! Purpose:
//! Emits wasm32-web associative-array transform helpers for set operations, unique, and flip.
//! Keeps associative WAT loops and entry comparison/storage helpers out of the transform dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets`.
//!
//! Key details:
//! - Helpers preserve PHP key normalization, value-cell string-representation comparisons, and exact metadata.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_known_assoc_array_value_set_assign(
    name: &str,
    source: &str,
    function_name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let source_kinds = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec());
    let exact_metadata = exact_assoc_value_set_metadata(source, function_name, args, module);
    if let Some(metadata) = exact_metadata {
        return emit_exact_assoc_metadata_assign(name, metadata, args[0].span, module);
    }
    let source_kind = source_kinds
        .as_deref()
        .and_then(homogeneous_assoc_value_set_kind);
    if source_kinds
        .as_deref()
        .is_some_and(|kinds| !assoc_value_set_source_kinds_supported(kinds))
    {
        return Err(CompileError::new(
            args[0].span,
            &format!(
                "wasm32-web {function_name}() on associative arrays currently requires scalar values"
            ),
        ));
    }
    let source_len = module
        .array_length(source)
        .filter(|len| *len != usize::MAX && module.array_key_values(source).is_some());
    let mut compare_sources = Vec::new();
    for arg in &args[1..] {
        let compare = match &arg.kind {
            ExprKind::Variable(compare) => compare.clone(),
            ExprKind::ArrayLiteral(items) => {
                let temp = module
                    .next_label("assoc_value_set_indexed_literal_mask")
                    .trim_start_matches('$')
                    .to_string();
                let assoc_items: Vec<_> = items
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        (
                            Expr::new(ExprKind::IntLiteral(index as i64), value.span),
                            value.clone(),
                        )
                    })
                    .collect();
                module.declare_array_local(temp.clone());
                emit_assoc_array_items_assign(&temp, &assoc_items, module)?;
                temp
            }
            ExprKind::ArrayLiteralAssoc(items) => {
                let temp = module
                    .next_label("assoc_value_set_literal_mask")
                    .trim_start_matches('$')
                    .to_string();
                module.declare_array_local(temp.clone());
                emit_assoc_array_items_assign(&temp, items, module)?;
                temp
            }
            _ => {
                return Err(CompileError::new(
                    arg.span,
                    &format!(
                        "wasm32-web {function_name}() currently requires associative mask arrays"
                    ),
                ));
            }
        };
        if module.local_kind(&compare) != Some(LocalKind::Array)
            || module.array_layout(&compare) != ArrayLayout::Assoc
        {
            return Err(CompileError::new(
                arg.span,
                &format!("wasm32-web {function_name}() currently requires associative mask arrays"),
            ));
        }
        let Some(compare_kinds) = module.array_value_cell_kinds(&compare) else {
            return Err(CompileError::new(
                arg.span,
                &format!("wasm32-web {function_name}() associative masks require known value types"),
            ));
        };
        let mask_supported = if let Some(source_kinds) = source_kinds.as_deref() {
            assoc_value_set_mask_kinds_supported(source_kinds, compare_kinds)
        } else {
            assoc_value_set_source_kinds_supported(compare_kinds)
        };
        if !mask_supported {
            return Err(CompileError::new(
                arg.span,
                &format!(
                    "wasm32-web {function_name}() associative masks currently require supported scalar values"
                ),
            ));
        }
        let ptr = preserve_array_ptr(&compare, function_name, module);
        let len = module.next_label("assoc_value_set_compare_len");
        module.declare_i32_local(len.trim_start_matches('$').to_string());
        module.body().line(&format!("local.get ${}_len", compare));
        module.body().line(&format!("local.set {}", len));
        compare_sources.push((ptr, len));
    }
    let keep_matching = function_name.eq_ignore_ascii_case("array_intersect");
    let source_ptr = preserve_array_ptr(source, function_name, module);
    let index = module.next_label("assoc_value_set_index");
    let out_index = module.next_label("assoc_value_set_out_index");
    let found = module.next_label("assoc_value_set_found");
    let target_entry = module.next_label("assoc_value_set_target_entry");
    let source_entry = module.next_label("assoc_value_set_source_entry");
    let done_label = module.next_label("assoc_value_set_done");
    let loop_label = module.next_label("assoc_value_set_loop");
    for local in [&index, &out_index, &found, &target_entry, &source_entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(
        name,
        source_kind.and_then(|kind| source_len.map(|len| vec![kind; len])),
    );
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    module.set_array_runtime_key_kind(name, assoc_value_set_runtime_key_kind(source, module));
    module.set_array_php_normalized_runtime_keys(
        name,
        module.array_has_php_normalized_runtime_keys(source),
    );
    if let Some(source_len) = source_len {
        module.body().line(&format!("i32.const {}", source_len));
    } else {
        module.body().line(&format!("local.get ${}_len", source));
    }
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    if let Some(source_len) = source_len {
        module.body().line(&format!("i32.const {}", source_len));
    } else {
        module.body().line(&format!("local.get ${}_len", source));
    }
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_assoc_entry_address(&source_ptr, &index, &source_entry, module);
    emit_assoc_value_cell_address(&source_entry, &target_entry, module);
    emit_trap_if_value_cell_ptr_is_array(&target_entry, module);
    emit_assoc_value_set_membership(
        &source_entry,
        &found,
        &compare_sources,
        keep_matching,
        module,
    );
    module.body().line(&format!("local.get {}", found));
    if !keep_matching {
        module.body().line("i32.eqz");
    }
    module.body().open("if");
    emit_assoc_entry_address(&format!("${}_ptr", name), &out_index, &target_entry, module);
    copy_assoc_entry(&target_entry, &source_entry, module);
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
    Ok(())
}

pub(in crate::codegen::wasm::expr) fn homogeneous_assoc_value_set_kind(kinds: &[ValueCellKind]) -> Option<ValueCellKind> {
    let first = *kinds.first()?;
    if !matches!(
        first,
        ValueCellKind::Int
            | ValueCellKind::Float
            | ValueCellKind::Bool
            | ValueCellKind::Null
            | ValueCellKind::Str
    ) {
        return None;
    }
    kinds.iter().all(|kind| *kind == first).then_some(first)
}

fn assoc_value_set_runtime_key_kind(source: &str, module: &WasmModule) -> Option<AssocKeyKind> {
    module.array_runtime_key_kind(source).or_else(|| {
        let key_kinds = module.array_key_kinds(source)?;
        let first = key_kinds.first().copied()?;
        key_kinds.iter().all(|kind| *kind == first).then_some(first)
    })
}

pub(in crate::codegen::wasm::expr) fn assoc_value_set_source_kinds_supported(kinds: &[ValueCellKind]) -> bool {
    kinds.iter().all(|kind| {
        matches!(
            kind,
            ValueCellKind::Int
                | ValueCellKind::Float
                | ValueCellKind::Bool
                | ValueCellKind::Null
                | ValueCellKind::Str
        )
    })
}

pub(in crate::codegen::wasm::expr) fn assoc_value_set_mask_kinds_supported(
    source_kinds: &[ValueCellKind],
    compare_kinds: &[ValueCellKind],
) -> bool {
    if source_kinds.iter().any(|kind| *kind == ValueCellKind::Float) {
        return source_kinds.iter().all(|kind| *kind == ValueCellKind::Float)
            && compare_kinds.iter().all(|kind| *kind == ValueCellKind::Float);
    }
    compare_kinds.iter().all(|kind| {
        matches!(
            kind,
            ValueCellKind::Int | ValueCellKind::Bool | ValueCellKind::Null | ValueCellKind::Str
        )
    })
}

pub(in crate::codegen::wasm::expr) fn emit_assoc_value_set_membership(
    source_entry: &str,
    found: &str,
    compare_sources: &[(String, String)],
    require_all_sets: bool,
    module: &mut WasmModule,
) {
    module
        .body()
        .line(&format!("i32.const {}", i32::from(require_all_sets)));
    module.body().line(&format!("local.set {}", found));
    for (compare_ptr, compare_len) in compare_sources {
        let set_found = module.next_label("assoc_value_set_one_found");
        let candidate_found = module.next_label("assoc_value_set_candidate_found");
        let compare_index = module.next_label("assoc_value_set_compare_index");
        let compare_entry = module.next_label("assoc_value_set_compare_entry");
        let done_label = module.next_label("assoc_value_set_compare_done");
        let loop_label = module.next_label("assoc_value_set_compare_loop");
        for local in [&set_found, &candidate_found, &compare_index, &compare_entry] {
            module.declare_i32_local(local.trim_start_matches('$').to_string());
        }
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", set_found));
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", compare_index));
        module.body().open(&format!("block {}", done_label));
        module.body().open(&format!("loop {}", loop_label));
        module.body().line(&format!("local.get {}", compare_index));
        module.body().line(&format!("local.get {}", compare_len));
        module.body().line("i32.ge_u");
        module.body().line(&format!("br_if {}", done_label));
        emit_assoc_entry_address(compare_ptr, &compare_index, &compare_entry, module);
        emit_assoc_entry_values_same_string_repr(source_entry, &compare_entry, &candidate_found, module);
        module.body().line(&format!("local.get {}", candidate_found));
        module.body().open("if");
        module.body().line("i32.const 1");
        module.body().line(&format!("local.set {}", set_found));
        module.body().line(&format!("br {}", done_label));
        module.body().close("end");
        module.body().line(&format!("local.get {}", compare_index));
        module.body().line("i32.const 1");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", compare_index));
        module.body().line(&format!("br {}", loop_label));
        module.body().close("end");
        module.body().close("end");
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

pub(in crate::codegen::wasm::expr) fn emit_known_assoc_array_unique_assign(
    name: &str,
    source: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let sort_mode = array_unique_sort_mode(args, args[0].span, module)?;
    let value_kinds = module.array_value_cell_kinds(source);
    let runtime_value_kind = module.array_runtime_value_cell_kind(source);
    let exact_metadata = exact_unique_assoc_value_metadata(source, sort_mode, module);
    let values_are_supported = if let Some(kinds) = value_kinds {
        !kinds.iter().any(|kind| matches!(kind, ValueCellKind::Array))
    } else if let Some(kind) = runtime_value_kind {
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
                "wasm32-web array_unique() on associative arrays currently rejects array values",
            ));
        }
        true
    } else {
        true
    };
    if !values_are_supported {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_unique() on associative arrays currently rejects array values",
        ));
    }
    let source_ptr = preserve_array_ptr(source, "array_unique", module);
    let source_len = module.next_label("assoc_array_unique_source_len");
    let index = module.next_label("assoc_array_unique_index");
    let scan = module.next_label("assoc_array_unique_scan");
    let out_index = module.next_label("assoc_array_unique_out_index");
    let seen = module.next_label("assoc_array_unique_seen");
    let equal = module.next_label("assoc_array_unique_equal");
    let source_entry = module.next_label("assoc_array_unique_source_entry");
    let scan_entry = module.next_label("assoc_array_unique_scan_entry");
    let target_entry = module.next_label("assoc_array_unique_target_entry");
    let done_label = module.next_label("assoc_array_unique_done");
    let loop_label = module.next_label("assoc_array_unique_loop");
    let scan_done_label = module.next_label("assoc_array_unique_scan_done");
    let scan_loop_label = module.next_label("assoc_array_unique_scan_loop");
    for local in [
        &index,
        &scan,
        &out_index,
        &seen,
        &equal,
        &source_entry,
        &scan_entry,
        &target_entry,
        &source_len,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    module.set_array_runtime_key_kind(name, module.array_runtime_key_kind(source));
    module.set_array_php_normalized_runtime_keys(
        name,
        module.array_has_php_normalized_runtime_keys(source),
    );
    if let Some(key_kinds) = module.array_key_kinds(source).map(|kinds| kinds.to_vec()) {
        if let Some(first) = key_kinds.first().copied() {
            if key_kinds.iter().all(|kind| *kind == first) {
                module.set_array_key_kinds(name, Some(key_kinds));
            }
        }
    }
    module.set_array_runtime_value_cell_kind(name, runtime_value_kind);
    if let Some(len) = module
        .array_length(source)
        .filter(|len| *len != usize::MAX && module.array_key_values(source).is_some())
    {
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
    emit_assoc_entry_address(&source_ptr, &index, &source_entry, module);
    emit_assoc_value_cell_address(&source_entry, &target_entry, module);
    emit_trap_if_value_cell_ptr_is_array(&target_entry, module);
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
    if sort_mode == ArrayUniqueSortMode::Regular {
        emit_assoc_entry_address(&format!("${}_ptr", name), &scan, &scan_entry, module);
    } else {
        emit_assoc_entry_address(&source_ptr, &scan, &scan_entry, module);
    }
    match sort_mode {
        ArrayUniqueSortMode::String => {
            emit_assoc_entry_values_same_string_repr(&source_entry, &scan_entry, &equal, module);
        }
        ArrayUniqueSortMode::Numeric => {
            emit_assoc_entry_values_same_sort_numeric(&source_entry, &scan_entry, &equal, module);
        }
        ArrayUniqueSortMode::Regular => {
            emit_assoc_entry_values_same_php_regular(&source_entry, &scan_entry, &equal, module);
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
    emit_assoc_entry_address(&format!("${}_ptr", name), &out_index, &target_entry, module);
    copy_assoc_entry(&target_entry, &source_entry, module);
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
    if let Some(metadata) = exact_metadata {
        apply_exact_unique_metadata(name, metadata, module);
    }
    Ok(())
}

fn emit_assoc_entry_values_same_php_regular(
    left_entry: &str,
    right_entry: &str,
    equal: &str,
    module: &mut WasmModule,
) {
    let left_cell = module.next_label("assoc_unique_regular_left_cell");
    let right_cell = module.next_label("assoc_unique_regular_right_cell");
    for local in [&left_cell, &right_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_assoc_value_cell_address(left_entry, &left_cell, module);
    emit_assoc_value_cell_address(right_entry, &right_cell, module);
    emit_value_cell_ptrs_php_loose_equal_between(&left_cell, &right_cell, equal, module);
}

fn emit_assoc_entry_values_same_sort_numeric(
    left_entry: &str,
    right_entry: &str,
    equal: &str,
    module: &mut WasmModule,
) {
    let left_cell = module.next_label("assoc_unique_numeric_left_cell");
    let right_cell = module.next_label("assoc_unique_numeric_right_cell");
    let left_number = module.next_label("assoc_unique_numeric_left_number");
    let right_number = module.next_label("assoc_unique_numeric_right_number");
    for local in [&left_cell, &right_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left_number, &right_number] {
        module.declare_f64_local(local.trim_start_matches('$').to_string());
    }
    emit_assoc_value_cell_address(left_entry, &left_cell, module);
    emit_assoc_value_cell_address(right_entry, &right_cell, module);
    emit_load_assoc_unique_numeric_value(&left_cell, &left_number, module);
    emit_load_assoc_unique_numeric_value(&right_cell, &right_number, module);
    module.body().line(&format!("local.get {}", left_number));
    module.body().line(&format!("local.get {}", right_number));
    module.body().line("f64.eq");
    module.body().line(&format!("local.set {}", equal));
}

fn emit_load_assoc_unique_numeric_value(cell: &str, target: &str, module: &mut WasmModule) {
    let tag = module.next_label("assoc_unique_numeric_tag");
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
