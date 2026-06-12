//! Purpose:
//! Lowers WASM non-callback array sort and shuffle mutator builtins.
//! Keeps sorting-specific compare/swap helpers separate from push/pop/shift code.
//!
//! Called from:
//! - `super::array_mutators` re-exports used by expression and callback-sort lowering.
//!
//! Key details:
//! - Mutating paths preserve copy-on-write uniqueness and keep array metadata reordered with payloads.

use super::*;
use super::array_value_cells::{
    emit_array_compare_swap, emit_copy_value_cell_slot, emit_ensure_unique_array_payload,
    emit_load_static_value_cell_tag, emit_load_value_cell_half, emit_load_value_string_part,
    emit_store_value_cell_half, emit_value_array_scalar_compare_swap,
    emit_value_array_string_compare_swap,
};
pub(super) use super::array_mutator_assoc_entries::{
    emit_load_assoc_key_payload_i32, emit_load_assoc_key_payload_i64,
    emit_load_assoc_value_payload_f64, emit_load_assoc_value_payload_i32,
    emit_load_assoc_value_payload_i64, emit_runtime_assoc_neighbor_entries,
    emit_runtime_assoc_neighbor_value_cells, emit_swap_assoc_entries,
    emit_swap_dynamic_assoc_entries,
};
pub(super) use super::array_mutator_assoc_value_sort::{
    emit_assoc_array_value_sort_call, emit_dynamic_value_cell_numeric,
    emit_dynamic_value_cell_php_bool, emit_dynamic_value_cell_string_cast_to_locals,
    emit_dynamic_value_cell_string_parts, emit_dynamic_value_cell_tag,
};

pub(super) fn emit_array_sort_call(
    expr: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let array = single_array_variable_arg(expr, args, name, module)?;
    if module.array_layout(&array) == ArrayLayout::Value {
        return emit_value_array_sort_call(args[0].span, &array, name, module);
    }
    let Some(len) = module.array_length(&array) else {
        return Err(CompileError::new(
            args[0].span,
            &format!("wasm32-web {name}() requires a known indexed array length"),
        ));
    };
    emit_ensure_unique_array_payload(&array, module);
    let left = module.next_label("array_sort_left");
    let right = module.next_label("array_sort_right");
    module.declare_i64_local(left.trim_start_matches('$').to_string());
    module.declare_i64_local(right.trim_start_matches('$').to_string());
    for pass in 0..len {
        for index in 0..len.saturating_sub(1 + pass) {
            emit_array_compare_swap(&array, index, name.eq_ignore_ascii_case("rsort"), &left, &right, module);
        }
    }
    module.body().line("i32.const 1");
    Ok(ValueKind::Bool)
}


pub(super) fn emit_array_key_sort_call(
    expr: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let array = single_array_variable_arg(expr, args, name, module)?;
    if module.array_layout(&array) != ArrayLayout::Assoc {
        module.body().line("i32.const 1");
        return Ok(ValueKind::Bool);
    }
    let Some(key_values) = module.array_key_values(&array).map(|values| values.to_vec()) else {
        if module.array_runtime_key_kind(&array) == Some(AssocKeyKind::Int) {
            emit_dynamic_assoc_array_int_key_sort_call(
                &array,
                name.eq_ignore_ascii_case("krsort"),
                module,
            );
            module.set_array_key_kinds(&array, None);
            module.set_array_key_values(&array, None);
            module.set_array_value_cell_kinds(&array, None);
            module.body().line("i32.const 1");
            return Ok(ValueKind::Bool);
        }
        if module.array_runtime_key_kind(&array) == Some(AssocKeyKind::Str) {
            emit_dynamic_assoc_array_string_key_sort_call(
                &array,
                name.eq_ignore_ascii_case("krsort"),
                module,
            );
            module.set_array_key_kinds(&array, None);
            module.set_array_key_values(&array, None);
            module.set_array_value_cell_kinds(&array, None);
            module.body().line("i32.const 1");
            return Ok(ValueKind::Bool);
        }
        return Err(CompileError::new(
            args[0].span,
            &format!("wasm32-web {name}() requires statically-known associative keys"),
        ));
    };
    let descending = name.eq_ignore_ascii_case("krsort");
    let mut order: Vec<usize> = (0..key_values.len()).collect();
    order.sort_by(|left, right| {
        let ordering = php_regular_assoc_key_compare(&key_values[*left], &key_values[*right]);
        if descending {
            ordering.reverse()
        } else {
            ordering
        }
    });
    emit_assoc_array_reorder_entries(&array, &order, module);
    if let Some(kinds) = module.array_key_kinds(&array).map(|kinds| kinds.to_vec()) {
        module.set_array_key_kinds(
            &array,
            Some(order.iter().map(|index| kinds[*index]).collect()),
        );
    }
    module.set_array_key_values(
        &array,
        Some(order.iter().map(|index| key_values[*index].clone()).collect()),
    );
    if let Some(kinds) = module.array_value_cell_kinds(&array).map(|kinds| kinds.to_vec()) {
        module.set_array_value_cell_kinds(
            &array,
            Some(order.iter().map(|index| kinds[*index]).collect()),
        );
    }
    module.body().line("i32.const 1");
    Ok(ValueKind::Bool)
}

fn php_regular_assoc_key_compare(left: &AssocKeyValue, right: &AssocKeyValue) -> std::cmp::Ordering {
    match (left, right) {
        (AssocKeyValue::Int(left), AssocKeyValue::Int(right)) => left.cmp(right),
        (AssocKeyValue::Str(left), AssocKeyValue::Str(right)) => {
            match (php_full_numeric_key_string(left), php_full_numeric_key_string(right)) {
                (Some(left), Some(right)) => left.partial_cmp(&right).unwrap_or(std::cmp::Ordering::Equal),
                _ => left.cmp(right),
            }
        }
        (AssocKeyValue::Int(left), AssocKeyValue::Str(right)) => {
            if let Some(right) = php_full_numeric_key_string(right) {
                (*left as f64).partial_cmp(&right).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                left.to_string().cmp(right)
            }
        }
        (AssocKeyValue::Str(left), AssocKeyValue::Int(right)) => {
            if let Some(left) = php_full_numeric_key_string(left) {
                left.partial_cmp(&(*right as f64)).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                left.cmp(&right.to_string())
            }
        }
    }
}

fn php_full_numeric_key_string(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<f64>().ok().filter(|value| value.is_finite())
}

fn emit_dynamic_assoc_array_int_key_sort_call(
    name: &str,
    descending: bool,
    module: &mut WasmModule,
) {
    let pass = module.next_label("assoc_key_sort_pass");
    let index = module.next_label("assoc_key_sort_index");
    let limit = module.next_label("assoc_key_sort_limit");
    let left_entry = module.next_label("assoc_key_sort_left_entry");
    let right_entry = module.next_label("assoc_key_sort_right_entry");
    let left_key = module.next_label("assoc_key_sort_left_key");
    let right_key = module.next_label("assoc_key_sort_right_key");
    let done_label = module.next_label("assoc_key_sort_done");
    let pass_loop = module.next_label("assoc_key_sort_pass_loop");
    let inner_done = module.next_label("assoc_key_sort_inner_done");
    let inner_loop = module.next_label("assoc_key_sort_inner_loop");
    for local in [&pass, &index, &limit, &left_entry, &right_entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left_key, &right_key] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    emit_ensure_unique_array_payload(name, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pass));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", pass_loop));
    module.body().line(&format!("local.get {}", pass));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.const 1");
    module.body().line("i32.le_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", limit));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", inner_done));
    module.body().open(&format!("loop {}", inner_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", limit));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", inner_done));
    emit_runtime_assoc_neighbor_entries(name, &index, &left_entry, &right_entry, module);
    emit_dynamic_assoc_int_key_compare_swap(
        &left_entry,
        &right_entry,
        &left_key,
        &right_key,
        descending,
        module,
    );
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", inner_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", pass));
    module.body().line(&format!("br {}", pass_loop));
    module.body().close("end");
    module.body().close("end");
}

fn emit_dynamic_assoc_array_string_key_sort_call(
    name: &str,
    descending: bool,
    module: &mut WasmModule,
) {
    let pass = module.next_label("assoc_string_key_sort_pass");
    let index = module.next_label("assoc_string_key_sort_index");
    let limit = module.next_label("assoc_string_key_sort_limit");
    let left_entry = module.next_label("assoc_string_key_sort_left_entry");
    let right_entry = module.next_label("assoc_string_key_sort_right_entry");
    let done_label = module.next_label("assoc_string_key_sort_done");
    let pass_loop = module.next_label("assoc_string_key_sort_pass_loop");
    let inner_done = module.next_label("assoc_string_key_sort_inner_done");
    let inner_loop = module.next_label("assoc_string_key_sort_inner_loop");
    for local in [&pass, &index, &limit, &left_entry, &right_entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_ensure_unique_array_payload(name, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pass));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", pass_loop));
    module.body().line(&format!("local.get {}", pass));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.const 1");
    module.body().line("i32.le_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", limit));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", inner_done));
    module.body().open(&format!("loop {}", inner_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", limit));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", inner_done));
    emit_runtime_assoc_neighbor_entries(name, &index, &left_entry, &right_entry, module);
    emit_dynamic_assoc_string_key_compare_swap(&left_entry, &right_entry, descending, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", inner_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", pass));
    module.body().line(&format!("br {}", pass_loop));
    module.body().close("end");
    module.body().close("end");
}

fn emit_dynamic_assoc_string_key_compare_swap(
    left_entry: &str,
    right_entry: &str,
    descending: bool,
    module: &mut WasmModule,
) {
    let left = module
        .next_label("assoc_string_key_sort_left")
        .trim_start_matches('$')
        .to_string();
    let right = module
        .next_label("assoc_string_key_sort_right")
        .trim_start_matches('$')
        .to_string();
    for local in [
        format!("{left}_ptr"),
        format!("{left}_len"),
        format!("{right}_ptr"),
        format!("{right}_len"),
    ] {
        module.declare_i32_local(local);
    }
    emit_load_dynamic_assoc_string_key(left_entry, &left, module);
    emit_load_dynamic_assoc_string_key(right_entry, &right, module);
    emit_runtime_lexical_string_comparison(
        &left,
        &right,
        if descending { &BinOp::Lt } else { &BinOp::Gt },
        module,
    );
    module.body().open("if");
    emit_swap_dynamic_assoc_entries(left_entry, right_entry, module);
    module.body().close("end");
}

fn emit_load_dynamic_assoc_string_key(entry: &str, target: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set ${}_ptr", target));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set ${}_len", target));
}

fn emit_dynamic_assoc_int_key_compare_swap(
    left_entry: &str,
    right_entry: &str,
    left_key: &str,
    right_key: &str,
    descending: bool,
    module: &mut WasmModule,
) {
    emit_load_dynamic_assoc_key_i64(left_entry, left_key, module);
    emit_load_dynamic_assoc_key_i64(right_entry, right_key, module);
    module.body().line(&format!("local.get {}", left_key));
    module.body().line(&format!("local.get {}", right_key));
    module.body().line(if descending { "i64.lt_s" } else { "i64.gt_s" });
    module.body().open("if");
    emit_swap_dynamic_assoc_entries(left_entry, right_entry, module);
    module.body().close("end");
}

fn emit_load_dynamic_assoc_key_i64(entry: &str, target: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", target));
}

fn emit_assoc_array_reorder_entries(
    name: &str,
    order: &[usize],
    module: &mut WasmModule,
) {
    let old_ptr = preserve_array_ptr(name, "assoc_key_sort_old_ptr", module);
    let new_ptr = module.next_label("assoc_key_sort_new_ptr");
    let target_entry = module.next_label("assoc_key_sort_target_entry");
    let source_entry = module.next_label("assoc_key_sort_source_entry");
    for local in [&new_ptr, &target_entry, &source_entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("i32.const {}", order.len()));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set {}", new_ptr));
    for (target_index, source_index) in order.iter().enumerate() {
        emit_assoc_entry_address_const_index(&new_ptr, target_index, &target_entry, module);
        emit_assoc_entry_address_const_index(&old_ptr, *source_index, &source_entry, module);
        copy_assoc_entry(&target_entry, &source_entry, module);
    }
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.set ${}_ptr", name));
}


fn emit_value_array_sort_call(
    span: crate::span::Span,
    array: &str,
    name: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let Some(kinds) = module.array_value_cell_kinds(array) else {
        if let Some(kind) = module.array_runtime_value_cell_kind(array) {
            if matches!(
                kind,
                ValueCellKind::Int
                    | ValueCellKind::Float
                    | ValueCellKind::Bool
                    | ValueCellKind::Null
                    | ValueCellKind::Str
            ) {
                emit_runtime_value_array_sort_call(array, name.eq_ignore_ascii_case("rsort"), module);
                module.body().line("i32.const 1");
                return Ok(ValueKind::Bool);
            }
        }
        return Err(CompileError::new(
            span,
            &format!("wasm32-web {name}() requires known value-cell element types"),
        ));
    };
    let kinds = kinds.to_vec();
    if kinds.iter().any(|kind| matches!(kind, ValueCellKind::Array)) {
        return Err(CompileError::new(
            span,
            &format!("wasm32-web {name}() does not support array value cells yet"),
        ));
    }
    let all_strings = kinds.iter().all(|kind| matches!(kind, ValueCellKind::Str));
    let all_ints = kinds.iter().all(|kind| matches!(kind, ValueCellKind::Int));
    let all_numeric_scalar = kinds
        .iter()
        .all(|kind| matches!(
            kind,
            ValueCellKind::Int | ValueCellKind::Float | ValueCellKind::Bool | ValueCellKind::Null
        ));
    let mixed_string_scalar = kinds
        .iter()
        .any(|kind| matches!(kind, ValueCellKind::Str))
        && kinds
            .iter()
            .any(|kind| {
                matches!(
                    kind,
                    ValueCellKind::Int | ValueCellKind::Float | ValueCellKind::Bool | ValueCellKind::Null
                )
            })
        && kinds
            .iter()
            .all(|kind| {
                matches!(
                    kind,
                    ValueCellKind::Int
                        | ValueCellKind::Float
                        | ValueCellKind::Bool
                        | ValueCellKind::Null
                        | ValueCellKind::Str
                )
            });
    if !all_strings && !all_numeric_scalar && !mixed_string_scalar {
        return Err(CompileError::new(
            span,
            &format!("wasm32-web {name}() does not support mixed value-cell sort comparisons yet"),
        ));
    }
    let Some(len) = module.array_length(array) else {
        return Err(CompileError::new(
            span,
            &format!("wasm32-web {name}() requires a known indexed array length"),
        ));
    };
    emit_ensure_unique_array_payload(array, module);
    for pass in 0..len {
        for index in 0..len.saturating_sub(1 + pass) {
            if all_strings {
                emit_value_array_string_compare_swap(
                    array,
                    index,
                    name.eq_ignore_ascii_case("rsort"),
                    module,
                );
            } else if all_ints {
                emit_value_array_scalar_compare_swap(
                    array,
                    index,
                    name.eq_ignore_ascii_case("rsort"),
                    module,
                );
            } else if mixed_string_scalar {
                emit_value_array_mixed_string_scalar_compare_swap(
                    array,
                    index,
                    name.eq_ignore_ascii_case("rsort"),
                    module,
                );
            } else {
                emit_value_array_numeric_scalar_compare_swap(
                    array,
                    index,
                    name.eq_ignore_ascii_case("rsort"),
                    module,
                );
            }
        }
    }
    module.body().line("i32.const 1");
    Ok(ValueKind::Bool)
}

fn emit_runtime_value_array_sort_call(name: &str, descending: bool, module: &mut WasmModule) {
    let pass = module.next_label("value_array_sort_pass");
    let index = module.next_label("value_array_sort_index");
    let limit = module.next_label("value_array_sort_limit");
    let left_cell = module.next_label("value_array_sort_left_cell");
    let right_cell = module.next_label("value_array_sort_right_cell");
    let done_label = module.next_label("value_array_sort_done");
    let pass_loop = module.next_label("value_array_sort_pass_loop");
    let inner_done = module.next_label("value_array_sort_inner_done");
    let inner_loop = module.next_label("value_array_sort_inner_loop");
    for local in [&pass, &index, &limit, &left_cell, &right_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_ensure_unique_array_payload(name, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pass));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", pass_loop));
    module.body().line(&format!("local.get {}", pass));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.const 1");
    module.body().line("i32.le_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", limit));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", inner_done));
    module.body().open(&format!("loop {}", inner_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", limit));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", inner_done));
    emit_runtime_value_array_neighbor_cells(name, &index, &left_cell, &right_cell, module);
    if module.array_runtime_value_cell_kind(name) == Some(ValueCellKind::Str) {
        emit_dynamic_value_array_string_compare_swap(&left_cell, &right_cell, descending, module);
    } else {
        emit_dynamic_value_array_mixed_scalar_compare_swap(&left_cell, &right_cell, descending, module);
    }
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", inner_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", pass));
    module.body().line(&format!("br {}", pass_loop));
    module.body().close("end");
    module.body().close("end");
    module.set_array_value_cell_kinds(name, None);
}

fn emit_runtime_value_array_neighbor_cells(
    name: &str,
    index: &str,
    left_cell: &str,
    right_cell: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", left_cell));
    module.body().line(&format!("local.get {}", left_cell));
    module.body().line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", right_cell));
}

fn emit_dynamic_value_array_string_compare_swap(
    left_cell: &str,
    right_cell: &str,
    descending: bool,
    module: &mut WasmModule,
) {
    let left = module
        .next_label("value_array_sort_left_string")
        .trim_start_matches('$')
        .to_string();
    let right = module
        .next_label("value_array_sort_right_string")
        .trim_start_matches('$')
        .to_string();
    for local in [
        format!("{left}_ptr"),
        format!("{left}_len"),
        format!("{right}_ptr"),
        format!("{right}_len"),
    ] {
        module.declare_i32_local(local);
    }
    emit_dynamic_value_cell_string_parts(left_cell, &left, module);
    emit_dynamic_value_cell_string_parts(right_cell, &right, module);
    emit_runtime_lexical_string_comparison(
        &left,
        &right,
        if descending { &BinOp::Lt } else { &BinOp::Gt },
        module,
    );
    module.body().open("if");
    emit_swap_dynamic_value_cells(left_cell, right_cell, module);
    module.body().close("end");
}

fn emit_dynamic_value_array_mixed_scalar_compare_swap(
    left_cell: &str,
    right_cell: &str,
    descending: bool,
    module: &mut WasmModule,
) {
    let left_tag = module.next_label("value_array_mixed_sort_left_tag");
    let right_tag = module.next_label("value_array_mixed_sort_right_tag");
    let left_number = module.next_label("value_array_mixed_sort_left_number");
    let right_number = module.next_label("value_array_mixed_sort_right_number");
    let left_is_numeric = module.next_label("value_array_mixed_sort_left_is_numeric");
    let right_is_numeric = module.next_label("value_array_mixed_sort_right_is_numeric");
    let left_boolish = module.next_label("value_array_mixed_sort_left_boolish");
    let right_boolish = module.next_label("value_array_mixed_sort_right_boolish");
    let left_bool = module.next_label("value_array_mixed_sort_left_bool");
    let right_bool = module.next_label("value_array_mixed_sort_right_bool");
    let should_swap = module.next_label("value_array_mixed_sort_should_swap");
    let left = module
        .next_label("value_array_mixed_sort_left_string")
        .trim_start_matches('$')
        .to_string();
    let right = module
        .next_label("value_array_mixed_sort_right_string")
        .trim_start_matches('$')
        .to_string();
    for local in [
        &left_tag,
        &right_tag,
        &left_is_numeric,
        &right_is_numeric,
        &left_boolish,
        &right_boolish,
        &left_bool,
        &right_bool,
        &should_swap,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left_number, &right_number] {
        module.declare_f64_local(local.trim_start_matches('$').to_string());
    }
    for local in [
        format!("{left}_ptr"),
        format!("{left}_len"),
        format!("{right}_ptr"),
        format!("{right}_len"),
    ] {
        module.declare_i32_local(local);
    }
    emit_dynamic_value_cell_tag(left_cell, &left_tag, module);
    emit_dynamic_value_cell_tag(right_cell, &right_tag, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", should_swap));
    emit_value_tag_is_numeric(&left_tag, module);
    emit_value_tag_is_numeric(&right_tag, module);
    module.body().line("i32.and");
    module.body().open("if");
    emit_dynamic_value_cell_numeric(left_cell, &left_tag, &left_number, module);
    emit_dynamic_value_cell_numeric(right_cell, &right_tag, &right_number, module);
    module.body().line(&format!("local.get {}", left_number));
    module.body().line(&format!("local.get {}", right_number));
    module
        .body()
        .line(if descending { "f64.lt" } else { "f64.gt" });
    module.body().line(&format!("local.set {}", should_swap));
    module.body().line("else");
    emit_value_tag_is_boolish(&left_tag, &left_boolish, module);
    emit_value_tag_is_boolish(&right_tag, &right_boolish, module);
    module.body().line(&format!("local.get {}", left_boolish));
    module.body().line(&format!("local.get {}", right_boolish));
    module.body().line("i32.or");
    module.body().open("if");
    emit_dynamic_value_cell_php_bool(left_cell, &left_tag, &left_bool, module);
    emit_dynamic_value_cell_php_bool(right_cell, &right_tag, &right_bool, module);
    module.body().line(&format!("local.get {}", left_bool));
    module.body().line(&format!("local.get {}", right_bool));
    module
        .body()
        .line(if descending { "i32.lt_u" } else { "i32.gt_u" });
    module.body().line(&format!("local.set {}", should_swap));
    module.body().line("else");
    emit_dynamic_value_cell_string_cast_to_locals(left_cell, &left_tag, &left, module);
    emit_dynamic_value_cell_string_cast_to_locals(right_cell, &right_tag, &right, module);
    emit_runtime_numeric_string_value(&left, &left_is_numeric, &left_number, module);
    emit_runtime_numeric_string_value(&right, &right_is_numeric, &right_number, module);
    module.body().line(&format!("local.get {}", left_is_numeric));
    module.body().line(&format!("local.get {}", right_is_numeric));
    module.body().line("i32.and");
    module.body().open("if");
    module.body().line(&format!("local.get {}", left_number));
    module.body().line(&format!("local.get {}", right_number));
    module
        .body()
        .line(if descending { "f64.lt" } else { "f64.gt" });
    module.body().line(&format!("local.set {}", should_swap));
    module.body().line("else");
    emit_runtime_lexical_string_comparison(
        &left,
        &right,
        if descending { &BinOp::Lt } else { &BinOp::Gt },
        module,
    );
    module.body().line(&format!("local.set {}", should_swap));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", should_swap));
    module.body().open("if");
    emit_swap_dynamic_value_cells(left_cell, right_cell, module);
    module.body().close("end");
}

fn emit_swap_dynamic_value_cells(left_cell: &str, right_cell: &str, module: &mut WasmModule) {
    let low = module.next_label("value_array_sort_swap_low");
    let high = module.next_label("value_array_sort_swap_high");
    for local in [&low, &high] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get {}", left_cell));
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", low));
    module.body().line(&format!("local.get {}", left_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", high));
    module.body().line(&format!("local.get {}", left_cell));
    module.body().line(&format!("local.get {}", right_cell));
    module.body().line("i64.load");
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", left_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", right_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", right_cell));
    module.body().line(&format!("local.get {}", low));
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", right_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", high));
    module.body().line("i64.store");
}

fn emit_value_array_mixed_string_scalar_compare_swap(
    name: &str,
    index: usize,
    descending: bool,
    module: &mut WasmModule,
) {
    let left_tag = module.next_label("value_string_sort_left_tag");
    let right_tag = module.next_label("value_string_sort_right_tag");
    let left_number = module.next_label("value_string_sort_left_number");
    let right_number = module.next_label("value_string_sort_right_number");
    let left_is_numeric = module.next_label("value_string_sort_left_is_numeric");
    let right_is_numeric = module.next_label("value_string_sort_right_is_numeric");
    let left_boolish = module.next_label("value_string_sort_left_boolish");
    let right_boolish = module.next_label("value_string_sort_right_boolish");
    let left_bool = module.next_label("value_string_sort_left_bool");
    let right_bool = module.next_label("value_string_sort_right_bool");
    let should_swap = module.next_label("value_string_sort_should_swap");
    let low = module.next_label("value_string_sort_low");
    let high = module.next_label("value_string_sort_high");
    let left_string = module
        .next_label("value_string_sort_left_string")
        .trim_start_matches('$')
        .to_string();
    let right_string = module
        .next_label("value_string_sort_right_string")
        .trim_start_matches('$')
        .to_string();
    for local in [
        &left_tag,
        &right_tag,
        &left_is_numeric,
        &right_is_numeric,
        &left_boolish,
        &right_boolish,
        &left_bool,
        &right_bool,
        &should_swap,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left_number, &right_number] {
        module.declare_f64_local(local.trim_start_matches('$').to_string());
    }
    for local in [&low, &high] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    for local in [
        format!("{left_string}_ptr"),
        format!("{left_string}_len"),
        format!("{right_string}_ptr"),
        format!("{right_string}_len"),
    ] {
        module.declare_i32_local(local);
    }

    emit_load_static_value_cell_tag(name, index, &left_tag, module);
    emit_load_static_value_cell_tag(name, index + 1, &right_tag, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", should_swap));
    emit_value_tag_is_numeric(&left_tag, module);
    emit_value_tag_is_numeric(&right_tag, module);
    module.body().line("i32.and");
    module.body().open("if");
    emit_load_static_value_cell_numeric(name, index, &left_tag, &left_number, module);
    emit_load_static_value_cell_numeric(name, index + 1, &right_tag, &right_number, module);
    module.body().line(&format!("local.get {}", left_number));
    module.body().line(&format!("local.get {}", right_number));
    module
        .body()
        .line(if descending { "f64.lt" } else { "f64.gt" });
    module.body().line(&format!("local.set {}", should_swap));
    module.body().line("else");
    emit_value_tag_is_boolish(&left_tag, &left_boolish, module);
    emit_value_tag_is_boolish(&right_tag, &right_boolish, module);
    module.body().line(&format!("local.get {}", left_boolish));
    module.body().line(&format!("local.get {}", right_boolish));
    module.body().line("i32.or");
    module.body().open("if");
    emit_load_static_value_cell_php_bool(name, index, &left_tag, &left_bool, module);
    emit_load_static_value_cell_php_bool(name, index + 1, &right_tag, &right_bool, module);
    module.body().line(&format!("local.get {}", left_bool));
    module.body().line(&format!("local.get {}", right_bool));
    module
        .body()
        .line(if descending { "i32.lt_u" } else { "i32.gt_u" });
    module.body().line(&format!("local.set {}", should_swap));
    module.body().line("else");
    emit_static_value_cell_string_cast_to_locals(name, index, &left_tag, &left_string, module);
    emit_static_value_cell_string_cast_to_locals(name, index + 1, &right_tag, &right_string, module);
    emit_runtime_numeric_string_value(&left_string, &left_is_numeric, &left_number, module);
    emit_runtime_numeric_string_value(&right_string, &right_is_numeric, &right_number, module);
    module.body().line(&format!("local.get {}", left_is_numeric));
    module.body().line(&format!("local.get {}", right_is_numeric));
    module.body().line("i32.and");
    module.body().open("if");
    module.body().line(&format!("local.get {}", left_number));
    module.body().line(&format!("local.get {}", right_number));
    module
        .body()
        .line(if descending { "f64.lt" } else { "f64.gt" });
    module.body().line(&format!("local.set {}", should_swap));
    module.body().line("else");
    emit_runtime_lexical_string_comparison(
        &left_string,
        &right_string,
        if descending { &BinOp::Lt } else { &BinOp::Gt },
        module,
    );
    module.body().line(&format!("local.set {}", should_swap));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", should_swap));
    module.body().open("if");
    emit_load_value_cell_half(name, index, 0, &low, module);
    emit_load_value_cell_half(name, index, 8, &high, module);
    emit_copy_value_cell_slot(name, index, index + 1, module);
    emit_store_value_cell_half(name, index + 1, 0, &low, module);
    emit_store_value_cell_half(name, index + 1, 8, &high, module);
    module.body().close("end");
}

pub(super) fn emit_value_tag_is_boolish(tag: &str, target: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", target));
}

fn emit_load_static_value_cell_php_bool(
    name: &str,
    index: usize,
    tag: &str,
    target: &str,
    module: &mut WasmModule,
) {
    let ptr = module.next_label("value_php_bool_ptr");
    let len = module.next_label("value_php_bool_len");
    let byte = module.next_label("value_php_bool_byte");
    for local in [&ptr, &len, &byte] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", target));
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_load_value_string_part(name, index, 8, "i32.load", &ptr, module);
    emit_load_value_string_part(name, index, 12, "i32.load", &len, module);
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 0");
    module.body().line("i32.gt_u");
    module.body().line(&format!("local.set {}", target));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 1");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", byte));
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 48");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", target));
    module.body().close("end");
    module.body().close("end");
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE + 8));
    module.body().line("i32.add");
    module.body().line("f64.load");
    module.body().line("f64.const 0");
    module.body().line("f64.ne");
    module.body().line(&format!("local.set {}", target));
    module.body().line("else");
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE + 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.eqz");
    module.body().line("i32.eqz");
    module.body().line(&format!("local.set {}", target));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

fn emit_static_value_cell_string_cast_to_locals(
    name: &str,
    index: usize,
    tag: &str,
    target: &str,
    module: &mut WasmModule,
) {
    let (true_ptr, true_len) = module.intern_string("1");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_load_value_string_part(name, index, 8, "i32.load", &format!("${target}_ptr"), module);
    emit_load_value_string_part(name, index, 12, "i32.load", &format!("${target}_len"), module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE + 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.eqz");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${target}_ptr"));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${target}_len"));
    module.body().line("else");
    module.body().line(&format!("i32.const {}", true_ptr));
    module.body().line(&format!("local.set ${target}_ptr"));
    module.body().line(&format!("i32.const {}", true_len));
    module.body().line(&format!("local.set ${target}_len"));
    module.body().close("end");
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${target}_ptr"));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${target}_len"));
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE + 8));
    module.body().line("i32.add");
    module.body().line("f64.load");
    emit_f64_stack_string_cast_value_to_stack("value_sort_float_string", module);
    module.body().line(&format!("local.set ${target}_len"));
    module.body().line(&format!("local.set ${target}_ptr"));
    module.body().line("else");
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE + 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    emit_i64_stack_string_cast_value_to_stack("value_sort_int_string", module);
    module.body().line(&format!("local.set ${target}_len"));
    module.body().line(&format!("local.set ${target}_ptr"));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

fn emit_value_array_numeric_scalar_compare_swap(
    name: &str,
    index: usize,
    descending: bool,
    module: &mut WasmModule,
) {
    let left_tag = module.next_label("value_mixed_sort_left_tag");
    let right_tag = module.next_label("value_mixed_sort_right_tag");
    let left_value = module.next_label("value_mixed_sort_left");
    let right_value = module.next_label("value_mixed_sort_right");
    let left_bool = module.next_label("value_mixed_sort_left_bool");
    let right_bool = module.next_label("value_mixed_sort_right_bool");
    let should_swap = module.next_label("value_mixed_sort_should_swap");
    let low = module.next_label("value_mixed_sort_low");
    let high = module.next_label("value_mixed_sort_high");
    for local in [&left_tag, &right_tag, &left_bool, &right_bool, &should_swap] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left_value, &right_value] {
        module.declare_f64_local(local.trim_start_matches('$').to_string());
    }
    for local in [&low, &high] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    emit_load_static_value_cell_tag(name, index, &left_tag, module);
    emit_load_static_value_cell_tag(name, index + 1, &right_tag, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", should_swap));
    emit_value_tag_is_numeric(&left_tag, module);
    emit_value_tag_is_numeric(&right_tag, module);
    module.body().line("i32.and");
    module.body().open("if");
    emit_load_static_value_cell_numeric(name, index, &left_tag, &left_value, module);
    emit_load_static_value_cell_numeric(name, index + 1, &right_tag, &right_value, module);
    module.body().line(&format!("local.get {}", left_value));
    module.body().line(&format!("local.get {}", right_value));
    module
        .body()
        .line(if descending { "f64.lt" } else { "f64.gt" });
    module.body().line(&format!("local.set {}", should_swap));
    module.body().line("else");
    emit_load_static_value_cell_truthy_scalar(name, index, &left_tag, &left_bool, module);
    emit_load_static_value_cell_truthy_scalar(name, index + 1, &right_tag, &right_bool, module);
    module.body().line(&format!("local.get {}", left_bool));
    module.body().line(&format!("local.get {}", right_bool));
    module
        .body()
        .line(if descending { "i32.lt_u" } else { "i32.gt_u" });
    module.body().line(&format!("local.set {}", should_swap));
    module.body().close("end");
    module.body().line(&format!("local.get {}", should_swap));
    module.body().open("if");
    emit_load_value_cell_half(name, index, 0, &low, module);
    emit_load_value_cell_half(name, index, 8, &high, module);
    emit_copy_value_cell_slot(name, index, index + 1, module);
    emit_store_value_cell_half(name, index + 1, 0, &low, module);
    emit_store_value_cell_half(name, index + 1, 8, &high, module);
    module.body().close("end");
}

pub(super) fn emit_value_tag_is_numeric(tag: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().line("i32.or");
}

fn emit_load_static_value_cell_numeric(
    name: &str,
    index: usize,
    tag: &str,
    target: &str,
    module: &mut WasmModule,
) {
    emit_load_static_value_cell_tag(name, index, tag, module);
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE + 8));
    module.body().line("i32.add");
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", target));
    module.body().line("else");
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE + 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("f64.convert_i64_s");
    module.body().line(&format!("local.set {}", target));
    module.body().close("end");
}

fn emit_load_static_value_cell_truthy_scalar(
    name: &str,
    index: usize,
    tag: &str,
    target: &str,
    module: &mut WasmModule,
) {
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", target));
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE + 8));
    module.body().line("i32.add");
    module.body().line("f64.load");
    module.body().line("f64.const 0");
    module.body().line("f64.ne");
    module.body().line(&format!("local.set {}", target));
    module.body().line("else");
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE + 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.eqz");
    module.body().line("i32.eqz");
    module.body().line(&format!("local.set {}", target));
    module.body().close("end");
    module.body().close("end");
}
