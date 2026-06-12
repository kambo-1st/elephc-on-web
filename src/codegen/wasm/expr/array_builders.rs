//! Purpose:
//! Lowers wasm32-web array construction helpers for fill, fill_keys, combine, and column.
//! Keeps static/dynamic builder metadata and runtime string-key loops out of expr.rs.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` array assignment lowering.
//!
//! Key details:
//! - Helpers preserve PHP key normalization, value-cell layout, and dynamic builder metadata.

mod fill_dynamic;

use super::*;
use fill_dynamic::{
    emit_dynamic_assoc_array_fill_assign, emit_dynamic_indexed_array_fill_assign,
    emit_dynamic_value_array_fill_assign,
};

pub(super) fn emit_indexed_array_fill_assign(
    name: &str,
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if args.len() != 3 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_fill() expects exactly three arguments",
        ));
    }
    let Some(start) = static_or_const_or_i64_local_value(&args[0], module) else {
        if expression_is_inty(&args[0], module) {
            return emit_dynamic_assoc_array_fill_assign(name, &args[0], &args[1], &args[2], module);
        }
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_fill() requires an integer start index",
        ));
    };
    let Some(count) = static_or_const_or_i64_local_value(&args[1], module) else {
        if start != 0 {
            return emit_dynamic_assoc_array_fill_assign(name, &args[0], &args[1], &args[2], module);
        }
        if start == 0
            && matches!(
                args[2].kind,
                ExprKind::IntLiteral(_) | ExprKind::ConstRef(_) | ExprKind::Negate(_)
            )
        {
            return emit_dynamic_indexed_array_fill_assign(name, &args[1], &args[2], module);
        }
        if start == 0 && value_cell_kind_for_expr(&args[2], module).is_some() {
            return emit_dynamic_value_array_fill_assign(name, &args[1], &args[2], module);
        }
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web array_fill() requires a static integer count",
        ));
    };
    let Ok(count) = usize::try_from(count) else {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web array_fill() count must not be negative",
        ));
    };
    if start != 0 {
        let items = (0..count)
            .map(|index| {
                (
                    Expr {
                        kind: ExprKind::IntLiteral(start + index as i64),
                        span: args[0].span,
                    },
                    args[2].clone(),
                )
            })
            .collect::<Vec<_>>();
        return emit_assoc_array_items_assign(name, &items, module);
    }
    if value_cell_kind_for_expr(&args[2], module).is_some()
        && !matches!(
            args[2].kind,
            ExprKind::IntLiteral(_) | ExprKind::ConstRef(_) | ExprKind::Negate(_)
        )
    {
        let items = (0..count).map(|_| args[2].clone()).collect::<Vec<_>>();
        return emit_value_array_items_assign(name, &items, module);
    }
    emit_array_alloc_prelude(name, count, module);
    for index in 0..count {
        emit_array_store_expr(name, index, &args[2], module)?;
    }
    Ok(())
}

pub(super) fn emit_array_fill_keys_assign(
    name: &str,
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if args.len() != 2 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_fill_keys() expects exactly two arguments",
        ));
    }
    if !matches!(args[0].kind, ExprKind::ArrayLiteral(_))
        && expression_has_array_type(&args[0], module)
    {
        let source = materialize_array_map_multi_source(&args[0], "array_fill_keys_source", module)?;
        if module.array_layout(&source) == ArrayLayout::Value
            && array_map_value_cells_are_strings(&source, module)
        {
            return emit_runtime_string_array_fill_keys_assign(name, &source, &args[1], module);
        }
        if module.array_layout(&source) == ArrayLayout::CompactInt {
            return emit_runtime_int_array_fill_keys_assign(name, &source, &args[1], false, module);
        }
        if module.array_layout(&source) == ArrayLayout::Value
            && array_map_value_cells_are_ints(&source, module)
        {
            return emit_runtime_int_array_fill_keys_assign(name, &source, &args[1], true, module);
        }
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_fill_keys() runtime keys currently require string or integer value cells",
        ));
    }
    let ExprKind::ArrayLiteral(keys) = &args[0].kind else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_fill_keys() currently requires a static indexed key array",
        ));
    };
    let mut items = Vec::with_capacity(keys.len());
    for key in keys {
        if static_int_value(key).is_none() && static_string_value(key, module).is_none() {
            return Err(CompileError::new(
                key.span,
                "wasm32-web array_fill_keys() keys must be static integers or strings",
            ));
        }
        items.push((key.clone(), args[1].clone()));
    }
    emit_assoc_array_items_assign(name, &items, module)
}

pub(super) fn emit_runtime_string_array_fill_keys_assign(
    name: &str,
    source: &str,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let len = module.next_label("array_fill_keys_len");
    let index = module.next_label("array_fill_keys_index");
    let scan = module.next_label("array_fill_keys_scan");
    let key_ptr = module.next_label("array_fill_keys_key_ptr");
    let key_len = module.next_label("array_fill_keys_key_len");
    let entry = module.next_label("array_fill_keys_entry");
    let scan_entry = module.next_label("array_fill_keys_scan_entry");
    let matched = module.next_label("array_fill_keys_matched");
    let cell = module.next_label("array_fill_keys_cell");
    let fill_cell = module.next_label("array_fill_keys_fill_cell");
    for local in [
        &len,
        &index,
        &scan,
        &key_ptr,
        &key_len,
        &entry,
        &scan_entry,
        &matched,
        &cell,
        &fill_cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_release_current_assoc_array(name, module);
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Str));
    module.set_array_php_normalized_runtime_keys(name, true);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, value_cell_kind_for_expr(value, module));
    module.set_array_runtime_nested_value_metadata(name, nested_array_metadata_for_expr(value, module));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.get {}", len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 1");
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", fill_cell));
    emit_store_value_cell(&fill_cell, value, module)?;
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    let loop_label = module.next_label("array_fill_keys_loop");
    let done_label = module.next_label("array_fill_keys_done");
    let scan_loop = module.next_label("array_fill_keys_scan_loop");
    let scan_done = module.next_label("array_fill_keys_scan_done");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", key_ptr));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", key_len));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", scan));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().open(&format!("block {}", scan_done));
    module.body().open(&format!("loop {}", scan_loop));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", scan_done));
    emit_assoc_entry_address(&format!("${}_ptr", name), &scan, &scan_entry, module);
    emit_assoc_entry_matches_string_parts(&scan_entry, &key_ptr, &key_len, &matched, module);
    module.body().line(&format!("local.get {}", matched));
    module.body().line(&format!("br_if {}", scan_done));
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().line(&format!("br {}", scan_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line(&format!("local.get {}", scan_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_copy_value_cell_from_addr_to_addr(&cell, &fill_cell, module);
    module.body().close("else");
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get {}", key_ptr));
    module.body().line(&format!("local.get {}", key_len));
    module.body().line("call $__rt_assoc_store_php_string_key");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_copy_value_cell_from_addr_to_addr(&cell, &fill_cell, module);
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set ${}_len", name));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", fill_cell));
    module.body().line("call $__rt_value_release");
    Ok(())
}

pub(super) fn emit_runtime_int_array_fill_keys_assign(
    name: &str,
    source: &str,
    value: &Expr,
    keys_are_value_cells: bool,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let len = module.next_label("array_fill_keys_int_len");
    let index = module.next_label("array_fill_keys_int_index");
    let scan = module.next_label("array_fill_keys_int_scan");
    let key = module.next_label("array_fill_keys_int_key");
    let entry = module.next_label("array_fill_keys_int_entry");
    let scan_entry = module.next_label("array_fill_keys_int_scan_entry");
    let matched = module.next_label("array_fill_keys_int_matched");
    let cell = module.next_label("array_fill_keys_int_cell");
    let fill_cell = module.next_label("array_fill_keys_int_fill_cell");
    for local in [
        &len,
        &index,
        &scan,
        &entry,
        &scan_entry,
        &matched,
        &cell,
        &fill_cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(key.trim_start_matches('$').to_string());
    emit_release_current_assoc_array(name, module);
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    module.set_array_php_normalized_runtime_keys(name, false);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, value_cell_kind_for_expr(value, module));
    module.set_array_runtime_nested_value_metadata(name, nested_array_metadata_for_expr(value, module));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.get {}", len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 1");
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", fill_cell));
    emit_store_value_cell(&fill_cell, value, module)?;
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    let loop_label = module.next_label("array_fill_keys_int_loop");
    let done_label = module.next_label("array_fill_keys_int_done");
    let scan_loop = module.next_label("array_fill_keys_int_scan_loop");
    let scan_done = module.next_label("array_fill_keys_int_scan_done");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    if keys_are_value_cells {
        module.body().line("call $__rt_value_payload_i64");
    } else {
        module.body().line("i32.const 8");
        module.body().line("i32.mul");
        module.body().line("i32.add");
        module.body().line("i64.load");
    }
    module.body().line(&format!("local.set {}", key));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", scan));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().open(&format!("block {}", scan_done));
    module.body().open(&format!("loop {}", scan_loop));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", scan_done));
    emit_assoc_entry_address(&format!("${}_ptr", name), &scan, &scan_entry, module);
    module.body().line(&format!("local.get {}", scan_entry));
    module.body().line(&format!("local.get {}", key));
    module.body().line("call $__rt_assoc_key_eq_int");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("local.get {}", matched));
    module.body().line(&format!("br_if {}", scan_done));
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().line(&format!("br {}", scan_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line(&format!("local.get {}", scan_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_copy_value_cell_from_addr_to_addr(&cell, &fill_cell, module);
    module.body().close("else");
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get {}", key));
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_copy_value_cell_from_addr_to_addr(&cell, &fill_cell, module);
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set ${}_len", name));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", fill_cell));
    module.body().line("call $__rt_value_release");
    Ok(())
}

pub(super) fn emit_array_combine_assign(
    name: &str,
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if args.len() != 2 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_combine() expects exactly two arguments",
        ));
    }
    if (!matches!(args[0].kind, ExprKind::ArrayLiteral(_))
        || !matches!(args[1].kind, ExprKind::ArrayLiteral(_)))
        && expression_has_array_type(&args[0], module)
        && expression_has_array_type(&args[1], module)
    {
        let keys = materialize_array_map_multi_source(&args[0], "array_combine_keys", module)?;
        let values = materialize_array_map_multi_source(&args[1], "array_combine_values", module)?;
        let value_kind = homogeneous_array_value_kind(&values, module);
        if value_kind.is_none() && !array_combine_values_support_mixed_cells(&values, module) {
            return Err(CompileError::new(
                args[1].span,
                "wasm32-web array_combine() runtime values currently require scalar value cells",
            ));
        }
        if let (Some(key_len), Some(value_len)) =
            (module.array_length(&keys), module.array_length(&values))
        {
            if key_len != value_len {
                return Err(CompileError::new(
                    expr.span,
                    "wasm32-web array_combine() key and value arrays must have the same length",
                ));
            }
        }
        if module.array_layout(&keys) == ArrayLayout::Value && array_map_value_cells_are_strings(&keys, module)
        {
            return emit_runtime_string_array_combine_assign(name, &keys, &values, value_kind, module);
        }
        if module.array_layout(&keys) == ArrayLayout::CompactInt {
            return emit_runtime_int_array_combine_assign(name, &keys, &values, value_kind, false, module);
        }
        if module.array_layout(&keys) == ArrayLayout::Value && array_map_value_cells_are_ints(&keys, module)
        {
            return emit_runtime_int_array_combine_assign(name, &keys, &values, value_kind, true, module);
        }
        if module.array_layout(&keys) == ArrayLayout::Value && array_map_value_cells_are_ints_or_strings(&keys, module)
        {
            return emit_runtime_mixed_key_array_combine_assign(name, &keys, &values, value_kind, module);
        }
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_combine() runtime keys currently require string or integer value cells",
        ));
    }
    let ExprKind::ArrayLiteral(keys) = &args[0].kind else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_combine() currently requires a static indexed key array",
        ));
    };
    let ExprKind::ArrayLiteral(values) = &args[1].kind else {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web array_combine() currently requires a static indexed value array",
        ));
    };
    if keys.len() != values.len() {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_combine() key and value arrays must have the same length",
        ));
    }
    let mut items = Vec::with_capacity(keys.len());
    for (key, value) in keys.iter().zip(values.iter()) {
        if static_int_value(key).is_none() && static_string_value(key, module).is_none() {
            return Err(CompileError::new(
                key.span,
                "wasm32-web array_combine() keys must be static integers or strings",
            ));
        }
        items.push((key.clone(), value.clone()));
    }
    emit_assoc_array_items_assign(name, &items, module)
}

fn array_combine_values_support_mixed_cells(source: &str, module: &WasmModule) -> bool {
    module.array_value_cell_kinds(source).is_some_and(|kinds| {
        kinds.iter().all(|kind| {
            matches!(
                kind,
                ValueCellKind::Int
                    | ValueCellKind::Float
                    | ValueCellKind::Bool
                    | ValueCellKind::Str
                    | ValueCellKind::Null
            )
        })
    })
}

fn array_map_value_cells_are_ints_or_strings(source: &str, module: &WasmModule) -> bool {
    mixed_array_key_kinds(source, module).is_some()
}

fn mixed_array_key_kinds(source: &str, module: &WasmModule) -> Option<Vec<ValueCellKind>> {
    if let Some(kinds) = module.array_value_cell_kinds(source) {
        if kinds
            .iter()
            .all(|kind| matches!(kind, ValueCellKind::Int | ValueCellKind::Str))
        {
            return Some(kinds.to_vec());
        }
    }
    let constants = module.array_value_constants(source)?;
    constants
        .iter()
        .map(|value| match value {
            ConstantValue::Int(_) => Some(ValueCellKind::Int),
            ConstantValue::Str(_) => Some(ValueCellKind::Str),
            _ => None,
        })
        .collect()
}

fn mixed_array_assoc_key_values(source: &str, module: &WasmModule) -> Option<Vec<AssocKeyValue>> {
    let constants = module.array_value_constants(source)?;
    let mut keys = Vec::with_capacity(constants.len());
    for value in constants {
        let key = match value {
            ConstantValue::Int(value) => AssocKeyValue::Int(*value),
            ConstantValue::Str(value) => literal_php_array_int_key(value)
                .map(AssocKeyValue::Int)
                .unwrap_or_else(|| AssocKeyValue::Str(value.clone())),
            _ => return None,
        };
        if keys.iter().any(|existing| existing == &key) {
            return None;
        }
        keys.push(key);
    }
    Some(keys)
}

pub(super) fn homogeneous_array_value_kind(source: &str, module: &WasmModule) -> Option<ValueCellKind> {
    if module.array_layout(source) == ArrayLayout::CompactInt {
        return Some(ValueCellKind::Int);
    }
    if module.array_layout(source) == ArrayLayout::Assoc {
        if let Some(kind) = module.array_runtime_value_cell_kind(source) {
            return Some(kind);
        }
        let kinds = module.array_value_cell_kinds(source)?;
        let first = kinds.first().copied()?;
        return kinds.iter().all(|kind| *kind == first).then_some(first);
    }
    if module.array_layout(source) != ArrayLayout::Value {
        return None;
    }
    if let Some(kind) = module.array_runtime_value_cell_kind(source) {
        return Some(kind);
    }
    let kinds = module.array_value_cell_kinds(source)?;
    let first = kinds.first().copied()?;
    kinds.iter().all(|kind| *kind == first).then_some(first)
}

pub(super) fn emit_runtime_string_array_combine_assign(
    name: &str,
    keys: &str,
    values: &str,
    value_kind: Option<ValueCellKind>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let index = module.next_label("array_combine_index");
    let scan = module.next_label("array_combine_scan");
    let key_ptr = module.next_label("array_combine_key_ptr");
    let key_len = module.next_label("array_combine_key_len");
    let entry = module.next_label("array_combine_entry");
    let scan_entry = module.next_label("array_combine_scan_entry");
    let matched = module.next_label("array_combine_matched");
    let cell = module.next_label("array_combine_cell");
    for local in [
        &index,
        &scan,
        &key_ptr,
        &key_len,
        &entry,
        &scan_entry,
        &matched,
        &cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_release_current_assoc_array(name, module);
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Str));
    module.set_array_php_normalized_runtime_keys(name, true);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, value_kind);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", keys));
    module.body().line(&format!("local.get ${}_len", values));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get ${}_len", keys));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    let loop_label = module.next_label("array_combine_loop");
    let done_label = module.next_label("array_combine_done");
    let scan_loop = module.next_label("array_combine_scan_loop");
    let scan_done = module.next_label("array_combine_scan_done");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", keys));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", keys));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", key_ptr));
    module.body().line(&format!("local.get ${}_ptr", keys));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", key_len));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", scan));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().open(&format!("block {}", scan_done));
    module.body().open(&format!("loop {}", scan_loop));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", scan_done));
    emit_assoc_entry_address(&format!("${}_ptr", name), &scan, &scan_entry, module);
    emit_assoc_entry_matches_string_parts(&scan_entry, &key_ptr, &key_len, &matched, module);
    module.body().line(&format!("local.get {}", matched));
    module.body().line(&format!("br_if {}", scan_done));
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().line(&format!("br {}", scan_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line(&format!("local.get {}", scan_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_copy_array_combine_value_to_cell(&cell, values, &index, module);
    module.body().close("else");
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get {}", key_ptr));
    module.body().line(&format!("local.get {}", key_len));
    module.body().line("call $__rt_assoc_store_php_string_key");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_copy_array_combine_value_to_cell(&cell, values, &index, module);
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set ${}_len", name));
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

pub(super) fn emit_runtime_int_array_combine_assign(
    name: &str,
    keys: &str,
    values: &str,
    value_kind: Option<ValueCellKind>,
    keys_are_value_cells: bool,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let index = module.next_label("array_combine_int_index");
    let scan = module.next_label("array_combine_int_scan");
    let key = module.next_label("array_combine_int_key");
    let entry = module.next_label("array_combine_int_entry");
    let scan_entry = module.next_label("array_combine_int_scan_entry");
    let matched = module.next_label("array_combine_int_matched");
    let cell = module.next_label("array_combine_int_cell");
    for local in [&index, &scan, &entry, &scan_entry, &matched, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(key.trim_start_matches('$').to_string());
    emit_release_current_assoc_array(name, module);
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    module.set_array_php_normalized_runtime_keys(name, false);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, value_kind);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", keys));
    module.body().line(&format!("local.get ${}_len", values));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get ${}_len", keys));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    let loop_label = module.next_label("array_combine_int_loop");
    let done_label = module.next_label("array_combine_int_done");
    let scan_loop = module.next_label("array_combine_int_scan_loop");
    let scan_done = module.next_label("array_combine_int_scan_done");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", keys));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", keys));
    module.body().line(&format!("local.get {}", index));
    if keys_are_value_cells {
        module.body().line("call $__rt_value_payload_i64");
    } else {
        module.body().line("i32.const 8");
        module.body().line("i32.mul");
        module.body().line("i32.add");
        module.body().line("i64.load");
    }
    module.body().line(&format!("local.set {}", key));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", scan));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().open(&format!("block {}", scan_done));
    module.body().open(&format!("loop {}", scan_loop));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", scan_done));
    emit_assoc_entry_address(&format!("${}_ptr", name), &scan, &scan_entry, module);
    module.body().line(&format!("local.get {}", scan_entry));
    module.body().line(&format!("local.get {}", key));
    module.body().line("call $__rt_assoc_key_eq_int");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("local.get {}", matched));
    module.body().line(&format!("br_if {}", scan_done));
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().line(&format!("br {}", scan_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line(&format!("local.get {}", scan_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_copy_array_combine_value_to_cell(&cell, values, &index, module);
    module.body().close("else");
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get {}", key));
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_copy_array_combine_value_to_cell(&cell, values, &index, module);
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set ${}_len", name));
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

pub(super) fn emit_runtime_mixed_key_array_combine_assign(
    name: &str,
    keys: &str,
    values: &str,
    value_kind: Option<ValueCellKind>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let index = module.next_label("array_combine_mixed_key_index");
    let scan = module.next_label("array_combine_mixed_key_scan");
    let key_tag = module.next_label("array_combine_mixed_key_tag");
    let key = module.next_label("array_combine_mixed_key_int");
    let key_ptr = module.next_label("array_combine_mixed_key_ptr");
    let key_len = module.next_label("array_combine_mixed_key_len");
    let entry = module.next_label("array_combine_mixed_key_entry");
    let scan_entry = module.next_label("array_combine_mixed_key_scan_entry");
    let matched = module.next_label("array_combine_mixed_key_matched");
    let cell = module.next_label("array_combine_mixed_key_cell");
    for local in [
        &index,
        &scan,
        &key_tag,
        &key_ptr,
        &key_len,
        &entry,
        &scan_entry,
        &matched,
        &cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(key.trim_start_matches('$').to_string());
    emit_release_current_assoc_array(name, module);
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Assoc);
    let exact_key_values = mixed_array_assoc_key_values(keys, module);
    module.set_array_key_kinds(
        name,
        exact_key_values.as_ref().map(|keys| {
            keys.iter()
                .map(|key| match key {
                    AssocKeyValue::Int(_) => AssocKeyKind::Int,
                    AssocKeyValue::Str(_) => AssocKeyKind::Str,
                })
                .collect()
        }),
    );
    module.set_array_key_values(name, exact_key_values);
    module.set_array_runtime_key_kind(name, None);
    module.set_array_php_normalized_runtime_keys(name, true);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, value_kind);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", keys));
    module.body().line(&format!("local.get ${}_len", values));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get ${}_len", keys));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}_len", name));
    if let Some(kinds) = mixed_array_key_kinds(keys, module) {
        for (offset, kind) in kinds.iter().enumerate() {
            module.body().line(&format!("i32.const {}", offset));
            module.body().line(&format!("local.set {}", index));
            match kind {
                ValueCellKind::Int => emit_runtime_mixed_key_array_combine_int_iteration(
                    name,
                    keys,
                    values,
                    &index,
                    &scan,
                    &key,
                    &entry,
                    &scan_entry,
                    &matched,
                    &cell,
                    module,
                ),
                ValueCellKind::Str => emit_runtime_mixed_key_array_combine_string_iteration(
                    name,
                    keys,
                    values,
                    &index,
                    &scan,
                    &key_ptr,
                    &key_len,
                    &entry,
                    &scan_entry,
                    &matched,
                    &cell,
                    module,
                ),
                _ => module.body().line("unreachable"),
            }
        }
        return Ok(());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    let loop_label = module.next_label("array_combine_mixed_key_loop");
    let done_label = module.next_label("array_combine_mixed_key_done");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", keys));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", keys));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_tag");
    module.body().line(&format!("local.set {}", key_tag));
    module.body().line(&format!("local.get {}", key_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_runtime_mixed_key_array_combine_int_iteration(
        name,
        keys,
        values,
        &index,
        &scan,
        &key,
        &entry,
        &scan_entry,
        &matched,
        &cell,
        module,
    );
    module.body().close("else");
    module.body().line(&format!("local.get {}", key_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    emit_runtime_mixed_key_array_combine_string_iteration(
        name,
        keys,
        values,
        &index,
        &scan,
        &key_ptr,
        &key_len,
        &entry,
        &scan_entry,
        &matched,
        &cell,
        module,
    );
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

fn emit_runtime_mixed_key_array_combine_int_iteration(
    name: &str,
    keys: &str,
    values: &str,
    index: &str,
    scan: &str,
    key: &str,
    entry: &str,
    scan_entry: &str,
    matched: &str,
    cell: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", keys));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_i64");
    module.body().line(&format!("local.set {}", key));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", scan));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    let scan_loop = module.next_label("array_combine_mixed_int_scan_loop");
    let scan_done = module.next_label("array_combine_mixed_int_scan_done");
    module.body().open(&format!("block {}", scan_done));
    module.body().open(&format!("loop {}", scan_loop));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", scan_done));
    emit_assoc_entry_address(&format!("${}_ptr", name), scan, scan_entry, module);
    module.body().line(&format!("local.get {}", scan_entry));
    module.body().line(&format!("local.get {}", key));
    module.body().line("call $__rt_assoc_key_eq_int");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("local.get {}", matched));
    module.body().line(&format!("br_if {}", scan_done));
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().line(&format!("br {}", scan_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line(&format!("local.get {}", scan_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_copy_array_combine_value_to_cell(cell, values, index, module);
    module.body().close("else");
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get {}", key));
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_initialize_value_cell(cell, module);
    emit_copy_array_combine_value_to_cell(cell, values, index, module);
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set ${}_len", name));
    module.body().close("end");
}

fn emit_runtime_mixed_key_array_combine_string_iteration(
    name: &str,
    keys: &str,
    values: &str,
    index: &str,
    scan: &str,
    key_ptr: &str,
    key_len: &str,
    entry: &str,
    scan_entry: &str,
    matched: &str,
    cell: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", keys));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", key_ptr));
    module.body().line(&format!("local.get ${}_ptr", keys));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", key_len));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", scan));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    let scan_loop = module.next_label("array_combine_mixed_string_scan_loop");
    let scan_done = module.next_label("array_combine_mixed_string_scan_done");
    module.body().open(&format!("block {}", scan_done));
    module.body().open(&format!("loop {}", scan_loop));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", scan_done));
    emit_assoc_entry_address(&format!("${}_ptr", name), scan, scan_entry, module);
    emit_assoc_entry_matches_string_parts(scan_entry, key_ptr, key_len, matched, module);
    module.body().line(&format!("local.get {}", matched));
    module.body().line(&format!("br_if {}", scan_done));
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().line(&format!("br {}", scan_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line(&format!("local.get {}", scan_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_copy_array_combine_value_to_cell(cell, values, index, module);
    module.body().close("else");
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get {}", key_ptr));
    module.body().line(&format!("local.get {}", key_len));
    module.body().line("call $__rt_assoc_store_php_string_key");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_initialize_value_cell(cell, module);
    emit_copy_array_combine_value_to_cell(cell, values, index, module);
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set ${}_len", name));
    module.body().close("end");
}

fn emit_initialize_value_cell(cell: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 0");
    module.body().line("i32.store");
}

pub(super) fn emit_copy_array_combine_value_to_cell(
    target_cell: &str,
    values: &str,
    index: &str,
    module: &mut WasmModule,
) {
    if module.array_layout(values) == ArrayLayout::CompactInt {
        module.body().line(&format!("local.get {}", target_cell));
        module.body().line(&format!("local.get ${}_ptr", values));
        module.body().line(&format!("local.get {}", index));
        module.body().line("i32.const 8");
        module.body().line("i32.mul");
        module.body().line("i32.add");
        module.body().line("i64.load");
        module.body().line("call $__rt_value_store_int");
        return;
    }
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get ${}_ptr", values));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_cell");
    module.body().line("call $__rt_value_copy");
}

pub(super) fn emit_array_column_assign(
    name: &str,
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if args.len() != 2 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_column() expects exactly two arguments",
        ));
    }
    let ExprKind::ArrayLiteral(rows) = &args[0].kind else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_column() currently requires a static indexed row array",
        ));
    };
    let column = static_assoc_key_value(&args[1], module)?;
    let mut values = Vec::new();
    for row in rows {
        let ExprKind::ArrayLiteralAssoc(items) = &row.kind else {
            return Err(CompileError::new(
                row.span,
                "wasm32-web array_column() rows must be static associative arrays",
            ));
        };
        if let Some(value) = array_column_static_row_value(items, &column, module)? {
            values.push(value);
        }
    }
    let array = Expr::new(ExprKind::ArrayLiteral(values), expr.span);
    emit_array_assign(name, &array, module)
}

pub(super) fn array_column_static_row_value(
    items: &[(Expr, Expr)],
    column: &StaticAssocKey,
    module: &WasmModule,
) -> Result<Option<Expr>, CompileError> {
    let mut found = None;
    for (key, value) in items {
        if &static_assoc_key_value(key, module)? == column {
            found = Some(value.clone());
        }
    }
    Ok(found)
}

pub(super) enum ArrayCopySource {
    Static(Vec<Expr>),
    Runtime { ptr: String, len: usize },
}
