//! Purpose:
//! Lowers WASM array mutator builtins such as push/pop/shift/unshift and
//! non-callback sort/shuffle variants.
//!
//! Called from:
//! - `super::emit_expr()` and string/output expression lowering.
//!
//! Key details:
//! - Mutating paths preserve WASM array metadata and call copy-on-write helpers
//!   before changing shared payloads.

use super::*;
use super::array_mutator_push::*;
use super::array_value_cells::reject_value_array_mutator;

pub(super) use super::array_mutator_sorts::{
    emit_array_key_sort_call, emit_array_sort_call, emit_assoc_array_value_sort_call,
    emit_load_assoc_key_payload_i32,
    emit_load_assoc_key_payload_i64, emit_load_assoc_value_payload_f64,
    emit_load_assoc_value_payload_i32, emit_load_assoc_value_payload_i64,
    emit_runtime_assoc_neighbor_entries, emit_runtime_assoc_neighbor_value_cells,
    emit_swap_assoc_entries, emit_swap_dynamic_assoc_entries,
};
pub(super) use super::array_mutator_shuffle::emit_array_shuffle_call;
pub(super) use super::array_mutator_unshift::emit_array_unshift_call;

pub(in crate::codegen::wasm) fn emit_array_push(
    name: &str,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(name) == ArrayLayout::Assoc {
        return emit_assoc_array_push(name, value, module);
    }
    if module.array_layout(name) == ArrayLayout::Value {
        return emit_value_array_push(name, value, module);
    }
    let Some(len) = module.array_length(name) else {
        return Err(CompileError::new(
            value.span,
            "wasm32-web array push requires a known indexed array length",
        ));
    };
    let old_ptr = module.next_label("array_push_old_ptr");
    let new_ptr = module.next_label("array_push_new_ptr");
    module.declare_i32_local(old_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(new_ptr.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.set {}", old_ptr));
    module.body().line(&format!("i32.const {}", len + 1));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set {}", new_ptr));
    for index in 0..len {
        module.body().line(&format!("local.get {}", new_ptr));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line(&format!("local.get {}", old_ptr));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line("i64.load");
        module.body().line("i64.store");
    }
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("i32.const {}", len * 8));
    module.body().line("i32.add");
    require_int(value, module)?;
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len + 1));
    module.body().line(&format!("local.set ${}_len", name));
    module.set_array_length(name, len + 1);
    Ok(())
}

pub(super) fn emit_array_push_call(
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if args.len() < 2 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_push() expects an array and at least one value",
        ));
    }
    let ExprKind::Variable(name) = &args[0].kind else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_push() requires an assigned indexed array variable",
        ));
    };
    if module.local_kind(name) == Some(LocalKind::Mixed) && module.mixed_value_cell_kind(name).is_none() {
        for value in &args[1..] {
            super::super::stmt::emit_unknown_mixed_array_push(name, value, module)?;
        }
        module.body().line(&format!("local.get ${}", name));
        module.body().line("i32.const 12");
        module.body().line("i32.add");
        module.body().line("i32.load");
        module.body().line("i64.extend_i32_u");
        return Ok(ValueKind::Int);
    }
    if module.local_kind(name) != Some(LocalKind::Array) {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_push() first argument must be an indexed array",
        ));
    }
    if module.array_layout(name) == ArrayLayout::Value {
        return emit_value_array_push_call(name, args, module);
    }
    if module.array_layout(name) == ArrayLayout::Assoc {
        return emit_assoc_array_push_call(name, args, module);
    }
    for value in &args[1..] {
        emit_array_push(name, value, module)?;
    }
    let len = module.array_length(name).ok_or_else(|| {
        CompileError::new(
            args[0].span,
            "wasm32-web array_push() requires a known indexed array length",
        )
    })?;
    module.body().line(&format!("i64.const {}", len));
    Ok(ValueKind::Int)
}

pub(super) fn emit_array_pop_call(
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let name = single_array_variable_arg(expr, args, "array_pop", module)?;
    if module.array_layout(&name) == ArrayLayout::Assoc {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_pop() on associative arrays is currently supported only in output position",
        ));
    }
    reject_value_array_mutator(args[0].span, &name, "array_pop", module)?;
    let len = module.array_length(&name).ok_or_else(|| {
        CompileError::new(
            args[0].span,
            "wasm32-web array_pop() requires a known indexed array length",
        )
    })?;
    if len == 0 {
        return Ok(ValueKind::Null);
    }
    let value = module.next_label("array_pop_value");
    module.declare_i64_local(value.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("i32.const {}", (len - 1) * 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", value));
    module.body().line(&format!("i32.const {}", len - 1));
    module.body().line(&format!("local.set ${}_len", name));
    module.set_array_length(&name, len - 1);
    module.body().line(&format!("local.get {}", value));
    Ok(ValueKind::Int)
}

pub(super) fn emit_array_shift_call(
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let name = single_array_variable_arg(expr, args, "array_shift", module)?;
    if module.array_layout(&name) == ArrayLayout::Assoc {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_shift() on associative arrays is currently supported only in output position",
        ));
    }
    reject_value_array_mutator(args[0].span, &name, "array_shift", module)?;
    let len = module.array_length(&name).ok_or_else(|| {
        CompileError::new(
            args[0].span,
            "wasm32-web array_shift() requires a known indexed array length",
        )
    })?;
    if len == 0 {
        return Ok(ValueKind::Null);
    }
    let value = module.next_label("array_shift_value");
    module.declare_i64_local(value.trim_start_matches('$').to_string());
    let source_ptr = preserve_array_ptr(&name, "array_shift", module);
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", value));
    emit_array_alloc_prelude(&name, len - 1, module);
    for index in 1..len {
        emit_array_store_load(&name, index - 1, &source_ptr, index, module);
    }
    module.body().line(&format!("local.get {}", value));
    Ok(ValueKind::Int)
}

pub(super) fn is_value_array_single_arg(args: &[Expr], module: &WasmModule) -> bool {
    matches!(
        args,
        [Expr {
            kind: ExprKind::Variable(name),
            ..
        }] if module.local_kind(name) == Some(LocalKind::Array)
            && module.array_layout(name) == ArrayLayout::Value
    )
}

pub(super) fn is_assoc_array_single_arg(args: &[Expr], module: &WasmModule) -> bool {
    matches!(
        args,
        [Expr {
            kind: ExprKind::Variable(name),
            ..
        }] if module.local_kind(name) == Some(LocalKind::Array)
            && module.array_layout(name) == ArrayLayout::Assoc
    )
}

pub(super) fn emit_output_assoc_array_pop_shift(
    expr: &Expr,
    function_name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let name = single_array_variable_arg(expr, args, function_name, module)?;
    let len = module.array_length(&name).ok_or_else(|| {
        CompileError::new(
            args[0].span,
            &format!("wasm32-web {function_name}() requires a known associative array length"),
        )
    })?;
    if len == 0 {
        return Ok(());
    }
    if function_name.eq_ignore_ascii_case("array_pop") {
        emit_output_assoc_array_pop(&name, len, module);
    } else {
        emit_output_assoc_array_shift(&name, len, module);
    }
    Ok(())
}

fn emit_output_assoc_array_pop(name: &str, len: usize, module: &mut WasmModule) {
    let cell = module.next_label("assoc_array_pop_cell");
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", (len - 1) * WASM_ASSOC_ENTRY_SIZE + 16));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
    emit_output_value_cell(&cell, module);
    emit_release_value_cell(&cell, module);
    module.body().line(&format!("i32.const {}", len - 1));
    module.body().line(&format!("local.set ${}_len", name));
    module.set_array_length(name, len - 1);
    let key_kinds = module.array_key_kinds(name).map(|kinds| kinds[..len - 1].to_vec());
    let key_values = module.array_key_values(name).map(|values| values[..len - 1].to_vec());
    let value_kinds = module
        .array_value_cell_kinds(name)
        .map(|kinds| kinds[..len - 1].to_vec());
    let value_constants = array_value_constants_after_pop_shift(name, len, true, module);
    module.set_array_key_kinds(name, key_kinds);
    module.set_array_key_values(name, key_values);
    module.set_array_value_cell_kinds(name, value_kinds);
    module.set_array_value_constants(name, value_constants);
}

fn emit_output_assoc_array_shift(name: &str, len: usize, module: &mut WasmModule) {
    let cell = module.next_label("assoc_array_shift_cell");
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    let source_ptr = preserve_array_ptr(name, "assoc_array_shift", module);
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line("i32.const 16");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
    emit_output_value_cell(&cell, module);
    emit_release_value_cell(&cell, module);
    module.body().line(&format!("i32.const {}", len - 1));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len - 1));
    module.body().line(&format!("local.set ${}_len", name));
    let key_kinds = module.array_key_kinds(name).map(|kinds| kinds.to_vec());
    let key_values = module.array_key_values(name).map(|values| values.to_vec());
    let mut next_int_key = 0i64;
    for index in 1..len {
        if key_kinds
            .as_ref()
            .and_then(|kinds| kinds.get(index))
            .is_some_and(|kind| *kind == AssocKeyKind::Int)
        {
            module.body().line(&format!("local.get ${}_ptr", name));
            module
                .body()
                .line(&format!("i32.const {}", (index - 1) * WASM_ASSOC_ENTRY_SIZE));
            module.body().line("i32.add");
            module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
            module.body().line("i32.store");
            module.body().line(&format!("local.get ${}_ptr", name));
            module
                .body()
                .line(&format!("i32.const {}", (index - 1) * WASM_ASSOC_ENTRY_SIZE + 8));
            module.body().line("i32.add");
            module.body().line(&format!("i64.const {}", next_int_key));
            module.body().line("i64.store");
            next_int_key += 1;
        } else {
            for offset in [0, 8] {
                module.body().line(&format!("local.get ${}_ptr", name));
                module.body().line(&format!(
                    "i32.const {}",
                    (index - 1) * WASM_ASSOC_ENTRY_SIZE + offset
                ));
                module.body().line("i32.add");
                module.body().line(&format!("local.get {}", source_ptr));
                module
                    .body()
                    .line(&format!("i32.const {}", index * WASM_ASSOC_ENTRY_SIZE + offset));
                module.body().line("i32.add");
                module.body().line("i64.load");
                module.body().line("i64.store");
            }
        }
        module.body().line(&format!("local.get ${}_ptr", name));
        module
            .body()
            .line(&format!("i32.const {}", (index - 1) * WASM_ASSOC_ENTRY_SIZE + 16));
        module.body().line("i32.add");
        module.body().line(&format!("local.get {}", source_ptr));
        module
            .body()
            .line(&format!("i32.const {}", index * WASM_ASSOC_ENTRY_SIZE + 16));
        module.body().line("i32.add");
        module.body().line("call $__rt_value_copy");
    }
    module.set_array_length(name, len - 1);
    let key_values = key_kinds.as_ref().zip(key_values.as_ref()).map(|(kinds, values)| {
        reindexed_assoc_key_values(&kinds[1..], &values[1..])
    });
    let key_kinds = key_kinds.map(|kinds| kinds[1..].to_vec());
    let value_kinds = module.array_value_cell_kinds(name).map(|kinds| kinds[1..].to_vec());
    let value_constants = array_value_constants_after_pop_shift(name, len, false, module);
    module.set_array_key_kinds(name, key_kinds);
    module.set_array_key_values(name, key_values);
    module.set_array_value_cell_kinds(name, value_kinds);
    module.set_array_value_constants(name, value_constants);
}

pub(in crate::codegen::wasm) fn emit_assoc_array_pop_shift_discard(
    expr: &Expr,
    function_name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let name = single_array_variable_arg(expr, args, function_name, module)?;
    let len = module.array_length(&name).ok_or_else(|| {
        CompileError::new(
            args[0].span,
            &format!("wasm32-web {function_name}() requires a known associative array length"),
        )
    })?;
    if len == 0 {
        return Ok(());
    }
    if function_name.eq_ignore_ascii_case("array_pop") {
        emit_assoc_array_pop_discard(&name, len, module);
    } else {
        emit_assoc_array_shift_discard(&name, len, module);
    }
    Ok(())
}

fn emit_assoc_array_pop_discard(name: &str, len: usize, module: &mut WasmModule) {
    let cell = module.next_label("assoc_array_pop_discard_cell");
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", (len - 1) * WASM_ASSOC_ENTRY_SIZE + 16));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
    emit_release_value_cell(&cell, module);
    module.body().line(&format!("i32.const {}", len - 1));
    module.body().line(&format!("local.set ${}_len", name));
    module.set_array_length(name, len - 1);
    let key_kinds = module.array_key_kinds(name).map(|kinds| kinds[..len - 1].to_vec());
    let key_values = module.array_key_values(name).map(|values| values[..len - 1].to_vec());
    let value_kinds = module
        .array_value_cell_kinds(name)
        .map(|kinds| kinds[..len - 1].to_vec());
    let value_constants = array_value_constants_after_pop_shift(name, len, true, module);
    module.set_array_key_kinds(name, key_kinds);
    module.set_array_key_values(name, key_values);
    module.set_array_value_cell_kinds(name, value_kinds);
    module.set_array_value_constants(name, value_constants);
}

fn emit_assoc_array_shift_discard(name: &str, len: usize, module: &mut WasmModule) {
    let cell = module.next_label("assoc_array_shift_discard_cell");
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    let source_ptr = preserve_array_ptr(name, "assoc_array_shift_discard", module);
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line("i32.const 16");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
    emit_release_value_cell(&cell, module);
    module.body().line(&format!("i32.const {}", len - 1));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len - 1));
    module.body().line(&format!("local.set ${}_len", name));
    let key_kinds = module.array_key_kinds(name).map(|kinds| kinds.to_vec());
    let key_values = module.array_key_values(name).map(|values| values.to_vec());
    let mut next_int_key = 0i64;
    for index in 1..len {
        if key_kinds
            .as_ref()
            .and_then(|kinds| kinds.get(index))
            .is_some_and(|kind| *kind == AssocKeyKind::Int)
        {
            module.body().line(&format!("local.get ${}_ptr", name));
            module
                .body()
                .line(&format!("i32.const {}", (index - 1) * WASM_ASSOC_ENTRY_SIZE));
            module.body().line("i32.add");
            module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
            module.body().line("i32.store");
            module.body().line(&format!("local.get ${}_ptr", name));
            module
                .body()
                .line(&format!("i32.const {}", (index - 1) * WASM_ASSOC_ENTRY_SIZE + 8));
            module.body().line("i32.add");
            module.body().line(&format!("i64.const {}", next_int_key));
            module.body().line("i64.store");
            next_int_key += 1;
        } else {
            for offset in [0, 8] {
                module.body().line(&format!("local.get ${}_ptr", name));
                module.body().line(&format!(
                    "i32.const {}",
                    (index - 1) * WASM_ASSOC_ENTRY_SIZE + offset
                ));
                module.body().line("i32.add");
                module.body().line(&format!("local.get {}", source_ptr));
                module
                    .body()
                    .line(&format!("i32.const {}", index * WASM_ASSOC_ENTRY_SIZE + offset));
                module.body().line("i32.add");
                module.body().line("i64.load");
                module.body().line("i64.store");
            }
        }
        module.body().line(&format!("local.get ${}_ptr", name));
        module
            .body()
            .line(&format!("i32.const {}", (index - 1) * WASM_ASSOC_ENTRY_SIZE + 16));
        module.body().line("i32.add");
        module.body().line(&format!("local.get {}", source_ptr));
        module
            .body()
            .line(&format!("i32.const {}", index * WASM_ASSOC_ENTRY_SIZE + 16));
        module.body().line("i32.add");
        module.body().line("call $__rt_value_copy");
    }
    module.set_array_length(name, len - 1);
    let key_values = key_kinds.as_ref().zip(key_values.as_ref()).map(|(kinds, values)| {
        reindexed_assoc_key_values(&kinds[1..], &values[1..])
    });
    let key_kinds = key_kinds.map(|kinds| kinds[1..].to_vec());
    let value_kinds = module.array_value_cell_kinds(name).map(|kinds| kinds[1..].to_vec());
    let value_constants = array_value_constants_after_pop_shift(name, len, false, module);
    module.set_array_key_kinds(name, key_kinds);
    module.set_array_key_values(name, key_values);
    module.set_array_value_cell_kinds(name, value_kinds);
    module.set_array_value_constants(name, value_constants);
}

pub(super) fn emit_output_value_array_pop_shift(
    expr: &Expr,
    function_name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let name = single_array_variable_arg(expr, args, function_name, module)?;
    let len = module.array_length(&name).ok_or_else(|| {
        CompileError::new(
            args[0].span,
            &format!("wasm32-web {function_name}() requires a known indexed array length"),
        )
    })?;
    if len == 0 {
        return Ok(());
    }
    if function_name.eq_ignore_ascii_case("array_pop") {
        return emit_output_value_array_pop(&name, len, module);
    }
    emit_output_value_array_shift(&name, len, module)
}

fn emit_output_value_array_pop(
    name: &str,
    len: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let cell = module.next_label("value_array_pop_cell");
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", (len - 1) * WASM_VALUE_CELL_SIZE));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
    emit_output_value_cell(&cell, module);
    emit_release_value_cell(&cell, module);
    module.body().line(&format!("i32.const {}", len - 1));
    module.body().line(&format!("local.set ${}_len", name));
    module.set_array_length(name, len - 1);
    let value_kinds = module.array_value_cell_kinds(name).map(|kinds| kinds[..len - 1].to_vec());
    let value_constants = array_value_constants_after_pop_shift(name, len, true, module);
    module.set_array_value_cell_kinds(name, value_kinds);
    module.set_array_value_constants(name, value_constants);
    Ok(())
}

fn emit_output_value_array_shift(
    name: &str,
    len: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let cell = module.next_label("value_array_shift_cell");
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    let source_ptr = preserve_array_ptr(name, "value_array_shift", module);
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.set {}", cell));
    emit_output_value_cell(&cell, module);
    emit_release_value_cell(&cell, module);
    module.body().line(&format!("i32.const {}", len - 1));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len - 1));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 1..len {
        emit_copy_static_value_cell(name, index - 1, &source_ptr, index, module);
    }
    module.set_array_length(name, len - 1);
    module.set_array_layout(name, ArrayLayout::Value);
    let value_kinds = module.array_value_cell_kinds(name).map(|kinds| kinds[1..].to_vec());
    let value_constants = array_value_constants_after_pop_shift(name, len, false, module);
    module.set_array_value_cell_kinds(name, value_kinds);
    module.set_array_value_constants(name, value_constants);
    Ok(())
}
