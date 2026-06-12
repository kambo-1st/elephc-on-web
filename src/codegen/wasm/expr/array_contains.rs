//! Purpose:
//! Emits wasm32-web containment scans for `in_array()` over compact, associative,
//! and value-cell array layouts.
//!
//! Called from:
//! - `super::array_membership::emit_in_array_call()`.
//!
//! Key details:
//! - Strict paths compare typed scalar payloads, while loose paths delegate to
//!   boxed value-cell PHP comparison helpers.
//! - Unsupported scalar or nested shapes remain compile errors rather than
//!   falling back to integer-only behavior.

use super::*;
use super::array_contains_assoc::*;
use super::array_contains_values::*;

pub(super) fn emit_array_contains_int(
    array: &Expr,
    needle: &Expr,
    strict: bool,
    found: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let ExprKind::ArrayLiteralAssoc(items) = &array.kind {
        if !strict {
            let values = assoc_array_value_exprs(items);
            if let Some(index) = static_loose_value_array_search_index(&values, needle, module) {
                module.body().line(&format!("i32.const {}", i32::from(index.is_some())));
                module.body().line(&format!("local.set {}", found));
                return Ok(());
            }
            let temp = module
                .next_label("assoc_in_array_loose_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_assoc_array_items_assign(&temp, items, module)?;
            let has_scalar_value_metadata = module
                .array_runtime_value_cell_kind(&temp)
                .is_some_and(|kind| kind != ValueCellKind::Array)
                || module
                    .array_value_cell_kinds(&temp)
                    .is_some_and(|kinds| kinds.iter().all(|kind| *kind != ValueCellKind::Array));
            if has_scalar_value_metadata {
                return emit_assoc_array_contains_loose_scalar(&temp, needle, found, module);
            }
            return Err(CompileError::new(
                array.span,
                "wasm32-web loose in_array() over associative arrays requires scalar value metadata",
            ));
        }
        let temp = module
            .next_label("assoc_in_array_source")
            .trim_start_matches('$')
            .to_string();
        module.declare_array_local(temp.clone());
        emit_assoc_array_items_assign(&temp, items, module)?;
        return emit_assoc_array_contains_scalar(&temp, needle, found, module);
    }
    if let ExprKind::ArrayLiteral(items) = &array.kind {
        if !strict {
            if let Some(index) = static_loose_value_array_search_index(items, needle, module) {
                module.body().line(&format!("i32.const {}", i32::from(index.is_some())));
                module.body().line(&format!("local.set {}", found));
                return Ok(());
            }
            let temp = module
                .next_label("value_in_array_loose_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_value_array_items_assign(&temp, items, module)?;
            return emit_value_array_contains_loose_scalar(&temp, needle, found, module);
        }
        let temp = module
            .next_label("value_in_array_source")
            .trim_start_matches('$')
            .to_string();
        module.declare_array_local(temp.clone());
        emit_value_array_items_assign(&temp, items, module)?;
        return emit_value_array_contains_scalar(&temp, needle, found, module);
    }
    if let ExprKind::FunctionCall { name: function_name, .. } = &array.kind {
        if module.has_function(function_name)
            && module.function_return_kind(function_name) == Some(ValueKind::Array)
            && matches!(
                module.function_array_return_layout(function_name),
                ArrayLayout::Assoc | ArrayLayout::Value
            )
        {
            let temp = module
                .next_label("in_array_return_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                if !strict {
                    if let Some(index) = static_loose_array_constant_search_index(&temp, needle, module) {
                        module.body().line(&format!("i32.const {}", i32::from(index.is_some())));
                        module.body().line(&format!("local.set {}", found));
                        return Ok(());
                    }
                    if module.array_runtime_value_cell_kind(&temp).is_some() {
                        return emit_assoc_array_contains_loose_scalar(&temp, needle, found, module);
                    }
                    return Err(CompileError::new(
                        array.span,
                        "wasm32-web loose in_array() over returned associative arrays requires scalar value metadata",
                    ));
                }
                return emit_assoc_array_contains_scalar(&temp, needle, found, module);
            }
            if !strict {
                if let Some(index) = static_loose_array_constant_search_index(&temp, needle, module) {
                    module.body().line(&format!("i32.const {}", i32::from(index.is_some())));
                    module.body().line(&format!("local.set {}", found));
                    return Ok(());
                }
                return emit_value_array_contains_loose_scalar(&temp, needle, found, module);
            }
            return emit_value_array_contains_scalar(&temp, needle, found, module);
        }
    }
    if expression_has_array_type(array, module) && !expression_is_arrayy(array, module) {
        let temp = module
            .next_label("in_array_direct_source")
            .trim_start_matches('$')
            .to_string();
        module.declare_array_local(temp.clone());
        emit_array_assign(&temp, array, module)?;
        if !strict {
            return match module.array_layout(&temp) {
                ArrayLayout::Assoc => {
                    if let Some(index) = static_loose_array_constant_search_index(&temp, needle, module) {
                        module.body().line(&format!("i32.const {}", i32::from(index.is_some())));
                        module.body().line(&format!("local.set {}", found));
                        Ok(())
                    } else if module.array_runtime_value_cell_kind(&temp).is_some() {
                        emit_assoc_array_contains_loose_scalar(&temp, needle, found, module)
                    } else if module.array_has_php_normalized_runtime_keys(&temp) {
                        emit_assoc_array_contains_loose_scalar(&temp, needle, found, module)
                    } else {
                        Err(CompileError::new(
                            array.span,
                            "wasm32-web loose in_array() over direct associative builders requires scalar value metadata",
                        ))
                    }
                }
                ArrayLayout::Value => {
                    if let Some(index) = static_loose_array_constant_search_index(&temp, needle, module) {
                        module.body().line(&format!("i32.const {}", i32::from(index.is_some())));
                        module.body().line(&format!("local.set {}", found));
                        Ok(())
                    } else {
                        emit_value_array_contains_loose_scalar(&temp, needle, found, module)
                    }
                }
                ArrayLayout::CompactInt => {
                    emit_compact_int_array_contains_loose_scalar(&temp, needle, found, module)
                }
            };
        }
        return match module.array_layout(&temp) {
            ArrayLayout::Assoc => emit_assoc_array_contains_scalar(&temp, needle, found, module),
            ArrayLayout::Value => emit_value_array_contains_scalar(&temp, needle, found, module),
            ArrayLayout::CompactInt => emit_array_contains_int(
                &Expr {
                    kind: ExprKind::Variable(temp),
                    span: array.span,
                },
                needle,
                strict,
                found,
                module,
            ),
        };
    }
    if !expression_is_arrayy(array, module) {
        return Err(CompileError::new(
            array.span,
            "wasm32-web in_array() currently supports indexed array values only",
        ));
    }
    if let ExprKind::Variable(var) = &array.kind {
        if module.local_kind(var) == Some(LocalKind::Array)
            && module.array_layout(var) == ArrayLayout::Assoc
        {
            if !strict {
                if let Some(index) = static_loose_array_constant_search_index(var, needle, module) {
                    module.body().line(&format!("i32.const {}", i32::from(index.is_some())));
                    module.body().line(&format!("local.set {}", found));
                    return Ok(());
                }
                if module.array_runtime_value_cell_kind(var).is_some() {
                    return emit_assoc_array_contains_loose_scalar(var, needle, found, module);
                }
                return Err(CompileError::new(
                    array.span,
                    "wasm32-web loose in_array() over associative arrays requires scalar value metadata",
                ));
            }
            return emit_assoc_array_contains_scalar(var, needle, found, module);
        }
        if module.local_kind(var) == Some(LocalKind::Array)
            && module.array_layout(var) == ArrayLayout::Value
        {
            if !strict {
                if let Some(index) = static_loose_array_constant_search_index(var, needle, module) {
                    module.body().line(&format!("i32.const {}", i32::from(index.is_some())));
                    module.body().line(&format!("local.set {}", found));
                    return Ok(());
                }
                return emit_value_array_contains_loose_scalar(var, needle, found, module);
            }
            return emit_value_array_contains_scalar(var, needle, found, module);
        }
        if module.local_kind(var) == Some(LocalKind::Array)
            && module.array_layout(var) == ArrayLayout::CompactInt
            && !strict
        {
            return emit_compact_int_array_contains_loose_scalar(var, needle, found, module);
        }
    }
    let needle_local = module.next_label("in_array_needle");
    let ptr = module.next_label("in_array_ptr");
    let len = module.next_label("in_array_len");
    let index = module.next_label("in_array_index");
    let done_label = module.next_label("in_array_done");
    let loop_label = module.next_label("in_array_loop");
    module.declare_i64_local(needle_local.trim_start_matches('$').to_string());
    for local in [&ptr, &len, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    require_int(needle, module)?;
    module.body().line(&format!("local.set {}", needle_local));
    emit_array_value_to_stack(array, module)?;
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.set {}", ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.get {}", needle_local));
    module.body().line("i64.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_compact_int_array_contains_loose_scalar(
    var: &str,
    needle: &Expr,
    found: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let needle_cell = module.next_label("compact_in_array_loose_needle");
    let item_cell = module.next_label("compact_in_array_loose_item");
    let index = module.next_label("compact_in_array_loose_index");
    let matched = module.next_label("compact_in_array_loose_matched");
    let done_label = module.next_label("compact_in_array_loose_done");
    let loop_label = module.next_label("compact_in_array_loose_loop");
    for local in [&needle_cell, &item_cell, &index, &matched] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 1");
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", needle_cell));
    emit_store_value_cell(&needle_cell, needle, module).map_err(|_| {
        CompileError::new(
            needle.span,
            "wasm32-web loose in_array() over compact integer arrays requires a supported scalar needle",
        )
    })?;
    module.body().line("i32.const 1");
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", item_cell));
    module.body().line(&format!("local.get {}", item_cell));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.store");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", item_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.store");
    emit_value_cell_ptrs_php_loose_equal_between(&item_cell, &needle_cell, &matched, module);
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}
