//! Purpose:
//! Lowers WASM non-callback associative array value sorting (`asort`/`arsort`).
//! Keeps associative value comparison policy separate from indexed/value-array sorting.
//!
//! Called from:
//! - `super::array_mutators` re-exports used by expression builtin lowering.
//!
//! Key details:
//! - Sorting mutates associative entries after COW uniqueness and preserves key/value metadata order.

use super::*;
use super::array_mutator_assoc_entries::*;
use super::array_mutator_sorts::{emit_value_tag_is_boolish, emit_value_tag_is_numeric};
use super::array_value_cells::emit_ensure_unique_array_payload;

pub(super) fn emit_assoc_array_value_sort_call(
    expr: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let array = single_array_variable_arg(expr, args, name, module)?;
    if module.array_layout(&array) != ArrayLayout::Assoc {
        return Err(CompileError::new(
            args[0].span,
            &format!("wasm32-web {name}() currently supports associative arrays only"),
        ));
    }
    let kinds = module.array_value_cell_kinds(&array).map(|kinds| kinds.to_vec());
    let Some(kinds) = kinds else {
        match module.array_runtime_value_cell_kind(&array) {
            Some(ValueCellKind::Str) => {
                emit_dynamic_assoc_array_string_value_sort_call(
                    &array,
                    name.eq_ignore_ascii_case("arsort"),
                    module,
                );
            }
            _ => {
                emit_dynamic_assoc_array_mixed_scalar_value_sort_call(
                    &array,
                    name.eq_ignore_ascii_case("arsort"),
                    module,
                );
            }
        }
        module.body().line("i32.const 1");
        return Ok(ValueKind::Bool);
    };
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
            .any(|kind| !matches!(kind, ValueCellKind::Str))
        && kinds.iter().all(|kind| {
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
            args[0].span,
            &format!("wasm32-web {name}() currently supports scalar associative values"),
        ));
    }
    let Some(len) = module.array_length(&array) else {
        return Err(CompileError::new(
            args[0].span,
            &format!("wasm32-web {name}() requires a known associative array length"),
        ));
    };
    emit_ensure_unique_array_payload(&array, module);
    for pass in 0..len {
        for index in 0..len.saturating_sub(1 + pass) {
            if all_strings {
                emit_assoc_array_string_value_compare_swap(
                    &array,
                    index,
                    name.eq_ignore_ascii_case("arsort"),
                    module,
                );
            } else if all_ints {
                emit_assoc_array_int_value_compare_swap(
                    &array,
                    index,
                    name.eq_ignore_ascii_case("arsort"),
                    module,
                );
            } else if mixed_string_scalar {
                emit_assoc_array_mixed_string_scalar_value_compare_swap(
                    &array,
                    index,
                    name.eq_ignore_ascii_case("arsort"),
                    module,
                );
            } else {
                emit_assoc_array_numeric_scalar_value_compare_swap(
                    &array,
                    index,
                    name.eq_ignore_ascii_case("arsort"),
                    module,
                );
            }
        }
    }
    module.body().line("i32.const 1");
    Ok(ValueKind::Bool)
}

fn emit_dynamic_assoc_array_string_value_sort_call(
    name: &str,
    descending: bool,
    module: &mut WasmModule,
) {
    let pass = module.next_label("assoc_string_value_sort_pass");
    let index = module.next_label("assoc_string_value_sort_index");
    let limit = module.next_label("assoc_string_value_sort_limit");
    let left_entry = module.next_label("assoc_string_value_sort_left_entry");
    let right_entry = module.next_label("assoc_string_value_sort_right_entry");
    let left_cell = module.next_label("assoc_string_value_sort_left_cell");
    let right_cell = module.next_label("assoc_string_value_sort_right_cell");
    let done_label = module.next_label("assoc_string_value_sort_done");
    let pass_loop = module.next_label("assoc_string_value_sort_pass_loop");
    let inner_done = module.next_label("assoc_string_value_sort_inner_done");
    let inner_loop = module.next_label("assoc_string_value_sort_inner_loop");
    for local in [
        &pass,
        &index,
        &limit,
        &left_entry,
        &right_entry,
        &left_cell,
        &right_cell,
    ] {
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
    emit_runtime_assoc_neighbor_value_cells(
        name,
        &index,
        &left_entry,
        &right_entry,
        &left_cell,
        &right_cell,
        module,
    );
    emit_dynamic_assoc_string_cells_compare_swap(
        &left_entry,
        &right_entry,
        &left_cell,
        &right_cell,
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

fn emit_dynamic_assoc_array_mixed_scalar_value_sort_call(
    name: &str,
    descending: bool,
    module: &mut WasmModule,
) {
    let pass = module.next_label("assoc_mixed_value_sort_pass");
    let index = module.next_label("assoc_mixed_value_sort_index");
    let limit = module.next_label("assoc_mixed_value_sort_limit");
    let left_entry = module.next_label("assoc_mixed_value_sort_left_entry");
    let right_entry = module.next_label("assoc_mixed_value_sort_right_entry");
    let left_cell = module.next_label("assoc_mixed_value_sort_left_cell");
    let right_cell = module.next_label("assoc_mixed_value_sort_right_cell");
    let done_label = module.next_label("assoc_mixed_value_sort_done");
    let pass_loop = module.next_label("assoc_mixed_value_sort_pass_loop");
    let inner_done = module.next_label("assoc_mixed_value_sort_inner_done");
    let inner_loop = module.next_label("assoc_mixed_value_sort_inner_loop");
    for local in [
        &pass,
        &index,
        &limit,
        &left_entry,
        &right_entry,
        &left_cell,
        &right_cell,
    ] {
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
    emit_runtime_assoc_neighbor_value_cells(
        name,
        &index,
        &left_entry,
        &right_entry,
        &left_cell,
        &right_cell,
        module,
    );
    emit_dynamic_assoc_mixed_scalar_cells_compare_swap(
        &left_entry,
        &right_entry,
        &left_cell,
        &right_cell,
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

fn emit_dynamic_assoc_string_cells_compare_swap(
    left_entry: &str,
    right_entry: &str,
    left_cell: &str,
    right_cell: &str,
    descending: bool,
    module: &mut WasmModule,
) {
    let left = module
        .next_label("assoc_string_value_sort_left")
        .trim_start_matches('$')
        .to_string();
    let right = module
        .next_label("assoc_string_value_sort_right")
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
    emit_swap_dynamic_assoc_entries(left_entry, right_entry, module);
    module.body().close("end");
}

fn emit_dynamic_assoc_mixed_scalar_cells_compare_swap(
    left_entry: &str,
    right_entry: &str,
    left_cell: &str,
    right_cell: &str,
    descending: bool,
    module: &mut WasmModule,
) {
    let left_tag = module.next_label("assoc_mixed_sort_left_tag");
    let right_tag = module.next_label("assoc_mixed_sort_right_tag");
    let left_number = module.next_label("assoc_mixed_sort_left_number");
    let right_number = module.next_label("assoc_mixed_sort_right_number");
    let left_is_numeric = module.next_label("assoc_mixed_sort_left_is_numeric");
    let right_is_numeric = module.next_label("assoc_mixed_sort_right_is_numeric");
    let left_boolish = module.next_label("assoc_mixed_sort_left_boolish");
    let right_boolish = module.next_label("assoc_mixed_sort_right_boolish");
    let left_bool = module.next_label("assoc_mixed_sort_left_bool");
    let right_bool = module.next_label("assoc_mixed_sort_right_bool");
    let should_swap = module.next_label("assoc_mixed_sort_should_swap");
    let left = module
        .next_label("assoc_mixed_sort_left_string")
        .trim_start_matches('$')
        .to_string();
    let right = module
        .next_label("assoc_mixed_sort_right_string")
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
    emit_swap_dynamic_assoc_entries(left_entry, right_entry, module);
    module.body().close("end");
}

pub(super) fn emit_dynamic_value_cell_tag(cell: &str, target: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", target));
}

pub(super) fn emit_dynamic_value_cell_string_parts(cell: &str, target: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set ${}_ptr", target));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set ${}_len", target));
}

pub(super) fn emit_dynamic_value_cell_php_bool(
    cell: &str,
    tag: &str,
    target: &str,
    module: &mut WasmModule,
) {
    let ptr = module.next_label("dynamic_assoc_bool_ptr");
    let len = module.next_label("dynamic_assoc_bool_len");
    let byte = module.next_label("dynamic_assoc_bool_byte");
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
    emit_dynamic_value_cell_payload_i32(cell, 8, &ptr, module);
    emit_dynamic_value_cell_payload_i32(cell, 12, &len, module);
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
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("f64.load");
    module.body().line("f64.const 0");
    module.body().line("f64.ne");
    module.body().line(&format!("local.set {}", target));
    module.body().line("else");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.eqz");
    module.body().line("i32.eqz");
    module.body().line(&format!("local.set {}", target));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_dynamic_value_cell_string_cast_to_locals(
    cell: &str,
    tag: &str,
    target: &str,
    module: &mut WasmModule,
) {
    let (true_ptr, true_len) = module.intern_string("1");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_dynamic_value_cell_payload_i32(cell, 8, &format!("${target}_ptr"), module);
    emit_dynamic_value_cell_payload_i32(cell, 12, &format!("${target}_len"), module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
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
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("f64.load");
    emit_f64_stack_string_cast_value_to_stack("dynamic_assoc_sort_float_string", module);
    module.body().line(&format!("local.set ${target}_len"));
    module.body().line(&format!("local.set ${target}_ptr"));
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    emit_i64_stack_string_cast_value_to_stack("dynamic_assoc_sort_int_string", module);
    module.body().line(&format!("local.set ${target}_len"));
    module.body().line(&format!("local.set ${target}_ptr"));
    module.body().line("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

fn emit_dynamic_value_cell_payload_i32(
    cell: &str,
    offset: usize,
    target: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("i32.const {}", offset));
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", target));
}

pub(super) fn emit_dynamic_value_cell_numeric(
    cell: &str,
    tag: &str,
    target: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", tag));
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", target));
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("f64.convert_i64_s");
    module.body().line(&format!("local.set {}", target));
    module.body().line("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
}

fn emit_assoc_array_mixed_string_scalar_value_compare_swap(
    name: &str,
    index: usize,
    descending: bool,
    module: &mut WasmModule,
) {
    let left_tag = module.next_label("assoc_string_sort_left_tag");
    let right_tag = module.next_label("assoc_string_sort_right_tag");
    let left_number = module.next_label("assoc_string_sort_left_number");
    let right_number = module.next_label("assoc_string_sort_right_number");
    let left_is_numeric = module.next_label("assoc_string_sort_left_is_numeric");
    let right_is_numeric = module.next_label("assoc_string_sort_right_is_numeric");
    let left_boolish = module.next_label("assoc_string_sort_left_boolish");
    let right_boolish = module.next_label("assoc_string_sort_right_boolish");
    let left_bool = module.next_label("assoc_string_sort_left_bool");
    let right_bool = module.next_label("assoc_string_sort_right_bool");
    let should_swap = module.next_label("assoc_string_sort_should_swap");
    let left_string = module
        .next_label("assoc_string_sort_left_string")
        .trim_start_matches('$')
        .to_string();
    let right_string = module
        .next_label("assoc_string_sort_right_string")
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
        format!("{left_string}_ptr"),
        format!("{left_string}_len"),
        format!("{right_string}_ptr"),
        format!("{right_string}_len"),
    ] {
        module.declare_i32_local(local);
    }
    emit_load_assoc_value_tag(name, index, &left_tag, module);
    emit_load_assoc_value_tag(name, index + 1, &right_tag, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", should_swap));
    emit_value_tag_is_numeric(&left_tag, module);
    emit_value_tag_is_numeric(&right_tag, module);
    module.body().line("i32.and");
    module.body().open("if");
    emit_load_assoc_value_numeric(name, index, &left_tag, &left_number, module);
    emit_load_assoc_value_numeric(name, index + 1, &right_tag, &right_number, module);
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
    emit_load_assoc_value_php_bool(name, index, &left_tag, &left_bool, module);
    emit_load_assoc_value_php_bool(name, index + 1, &right_tag, &right_bool, module);
    module.body().line(&format!("local.get {}", left_bool));
    module.body().line(&format!("local.get {}", right_bool));
    module
        .body()
        .line(if descending { "i32.lt_u" } else { "i32.gt_u" });
    module.body().line(&format!("local.set {}", should_swap));
    module.body().line("else");
    emit_assoc_value_string_cast_to_locals(name, index, &left_tag, &left_string, module);
    emit_assoc_value_string_cast_to_locals(name, index + 1, &right_tag, &right_string, module);
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
    emit_swap_assoc_entries(name, index, module);
    module.body().close("end");
}

fn emit_load_assoc_value_php_bool(
    name: &str,
    index: usize,
    tag: &str,
    target: &str,
    module: &mut WasmModule,
) {
    let ptr = module.next_label("assoc_php_bool_ptr");
    let len = module.next_label("assoc_php_bool_len");
    let byte = module.next_label("assoc_php_bool_byte");
    let float_value = module.next_label("assoc_php_bool_float_value");
    let scalar_value = module.next_label("assoc_php_bool_scalar_value");
    for local in [&ptr, &len, &byte] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_f64_local(float_value.trim_start_matches('$').to_string());
    module.declare_i64_local(scalar_value.trim_start_matches('$').to_string());
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
    emit_load_assoc_value_payload_i32(name, index, 8, &ptr, module);
    emit_load_assoc_value_payload_i32(name, index, 12, &len, module);
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
    emit_load_assoc_value_payload_f64(name, index, &float_value, module);
    module.body().line(&format!("local.get {}", float_value));
    module.body().line("f64.const 0");
    module.body().line("f64.ne");
    module.body().line(&format!("local.set {}", target));
    module.body().line("else");
    emit_load_assoc_value_payload_i64(name, index, &scalar_value, module);
    module.body().line(&format!("local.get {}", scalar_value));
    module.body().line("i64.eqz");
    module.body().line("i32.eqz");
    module.body().line(&format!("local.set {}", target));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

fn emit_assoc_value_string_cast_to_locals(
    name: &str,
    index: usize,
    tag: &str,
    target: &str,
    module: &mut WasmModule,
) {
    let (true_ptr, true_len) = module.intern_string("1");
    let float_value = module.next_label("assoc_value_sort_cast_float");
    let scalar_value = module.next_label("assoc_value_sort_cast_scalar");
    module.declare_f64_local(float_value.trim_start_matches('$').to_string());
    module.declare_i64_local(scalar_value.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_load_assoc_value_payload_i32(name, index, 8, &format!("${target}_ptr"), module);
    emit_load_assoc_value_payload_i32(name, index, 12, &format!("${target}_len"), module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_load_assoc_value_payload_i64(name, index, &scalar_value, module);
    module.body().line(&format!("local.get {}", scalar_value));
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
    emit_load_assoc_value_payload_f64(name, index, &float_value, module);
    module.body().line(&format!("local.get {}", float_value));
    emit_f64_stack_string_cast_value_to_stack("assoc_value_sort_float_string", module);
    module.body().line(&format!("local.set ${target}_len"));
    module.body().line(&format!("local.set ${target}_ptr"));
    module.body().line("else");
    emit_load_assoc_value_payload_i64(name, index, &scalar_value, module);
    module.body().line(&format!("local.get {}", scalar_value));
    emit_i64_stack_string_cast_value_to_stack("assoc_value_sort_int_string", module);
    module.body().line(&format!("local.set ${target}_len"));
    module.body().line(&format!("local.set ${target}_ptr"));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

fn emit_assoc_array_int_value_compare_swap(
    name: &str,
    index: usize,
    descending: bool,
    module: &mut WasmModule,
) {
    let left = module.next_label("assoc_value_sort_left");
    let right = module.next_label("assoc_value_sort_right");
    for local in [&left, &right] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    emit_load_assoc_value_payload_i64(name, index, &left, module);
    emit_load_assoc_value_payload_i64(name, index + 1, &right, module);
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(if descending { "i64.lt_s" } else { "i64.gt_s" });
    module.body().open("if");
    emit_swap_assoc_entries(name, index, module);
    module.body().close("end");
}

fn emit_assoc_array_numeric_scalar_value_compare_swap(
    name: &str,
    index: usize,
    descending: bool,
    module: &mut WasmModule,
) {
    let left_tag = module.next_label("assoc_mixed_sort_left_tag");
    let right_tag = module.next_label("assoc_mixed_sort_right_tag");
    let left_value = module.next_label("assoc_mixed_sort_left");
    let right_value = module.next_label("assoc_mixed_sort_right");
    let left_bool = module.next_label("assoc_mixed_sort_left_bool");
    let right_bool = module.next_label("assoc_mixed_sort_right_bool");
    let should_swap = module.next_label("assoc_mixed_sort_should_swap");
    for local in [&left_tag, &right_tag, &left_bool, &right_bool, &should_swap] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left_value, &right_value] {
        module.declare_f64_local(local.trim_start_matches('$').to_string());
    }
    emit_load_assoc_value_tag(name, index, &left_tag, module);
    emit_load_assoc_value_tag(name, index + 1, &right_tag, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", should_swap));
    emit_value_tag_is_numeric(&left_tag, module);
    emit_value_tag_is_numeric(&right_tag, module);
    module.body().line("i32.and");
    module.body().open("if");
    emit_load_assoc_value_numeric(name, index, &left_tag, &left_value, module);
    emit_load_assoc_value_numeric(name, index + 1, &right_tag, &right_value, module);
    module.body().line(&format!("local.get {}", left_value));
    module.body().line(&format!("local.get {}", right_value));
    module
        .body()
        .line(if descending { "f64.lt" } else { "f64.gt" });
    module.body().line(&format!("local.set {}", should_swap));
    module.body().line("else");
    emit_load_assoc_value_truthy_scalar(name, index, &left_tag, &left_bool, module);
    emit_load_assoc_value_truthy_scalar(name, index + 1, &right_tag, &right_bool, module);
    module.body().line(&format!("local.get {}", left_bool));
    module.body().line(&format!("local.get {}", right_bool));
    module
        .body()
        .line(if descending { "i32.lt_u" } else { "i32.gt_u" });
    module.body().line(&format!("local.set {}", should_swap));
    module.body().close("end");
    module.body().line(&format!("local.get {}", should_swap));
    module.body().open("if");
    emit_swap_assoc_entries(name, index, module);
    module.body().close("end");
}

fn emit_assoc_array_string_value_compare_swap(
    name: &str,
    index: usize,
    descending: bool,
    module: &mut WasmModule,
) {
    let left_ptr = module.next_label("assoc_value_sort_left_ptr");
    let left_len = module.next_label("assoc_value_sort_left_len");
    let right_ptr = module.next_label("assoc_value_sort_right_ptr");
    let right_len = module.next_label("assoc_value_sort_right_len");
    let byte_index = module.next_label("assoc_value_sort_byte_index");
    let left_byte = module.next_label("assoc_value_sort_left_byte");
    let right_byte = module.next_label("assoc_value_sort_right_byte");
    let cmp = module.next_label("assoc_value_sort_cmp");
    let loop_label = module.next_label("assoc_value_sort_cmp_loop");
    let done_label = module.next_label("assoc_value_sort_cmp_done");
    for local in [
        &left_ptr,
        &left_len,
        &right_ptr,
        &right_len,
        &byte_index,
        &left_byte,
        &right_byte,
        &cmp,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_load_assoc_value_payload_i32(name, index, 8, &left_ptr, module);
    emit_load_assoc_value_payload_i32(name, index, 12, &left_len, module);
    emit_load_assoc_value_payload_i32(name, index + 1, 8, &right_ptr, module);
    emit_load_assoc_value_payload_i32(name, index + 1, 12, &right_len, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", byte_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", cmp));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", byte_index));
    module.body().line(&format!("local.get {}", left_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("local.get {}", byte_index));
    module.body().line(&format!("local.get {}", right_len));
    module.body().line("i32.ge_u");
    module.body().line("i32.or");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", left_ptr));
    module.body().line(&format!("local.get {}", byte_index));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", left_byte));
    module.body().line(&format!("local.get {}", right_ptr));
    module.body().line(&format!("local.get {}", byte_index));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", right_byte));
    module.body().line(&format!("local.get {}", left_byte));
    module.body().line(&format!("local.get {}", right_byte));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line(&format!("local.get {}", left_byte));
    module.body().line(&format!("local.get {}", right_byte));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", cmp));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", byte_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", byte_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", cmp));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line(&format!("local.get {}", left_len));
    module.body().line(&format!("local.get {}", right_len));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", cmp));
    module.body().close("end");
    module.body().line(&format!("local.get {}", cmp));
    module.body().line("i32.const 0");
    module
        .body()
        .line(if descending { "i32.lt_s" } else { "i32.gt_s" });
    module.body().open("if");
    emit_swap_assoc_entries(name, index, module);
    module.body().close("end");
}
