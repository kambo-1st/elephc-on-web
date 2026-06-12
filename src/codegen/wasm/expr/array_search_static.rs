//! Purpose:
//! Provides static loose array_search()/in_array() result helpers for wasm32-web lowering.
//! Keeps compile-time scalar comparison and associative-key result lookup separate from runtime loops.
//!
//! Called from:
//! - `super::array_search`, `super::array_contains`, and other wasm expression helpers.
//!
//! Key details:
//! - Uses PHP loose scalar comparison semantics and returns None when metadata is insufficient.

use super::*;

pub(super) fn static_loose_value_array_search_index(
    items: &[Expr],
    needle: &Expr,
    module: &WasmModule,
) -> Option<Option<usize>> {
    let needle = static_scalar_value(needle, module)?;
    let values = items
        .iter()
        .map(|item| static_scalar_value(item, module))
        .collect::<Option<Vec<_>>>()?;
    Some(
        values
            .iter()
            .position(|value| compare_static_scalars(&needle, &BinOp::Eq, value)),
    )
}

pub(super) fn static_loose_array_constant_search_index(
    name: &str,
    needle: &Expr,
    module: &WasmModule,
) -> Option<Option<usize>> {
    let needle = static_scalar_value(needle, module)?;
    Some(
        module
            .array_value_constants(name)?
            .iter()
            .position(|value| compare_static_scalars(&needle, &BinOp::Eq, value)),
    )
}

pub(super) fn static_loose_assoc_int_key_search_result(
    name: &str,
    needle: &Expr,
    module: &WasmModule,
) -> Option<Option<i64>> {
    let key_kinds = module.array_key_kinds(name)?;
    if !key_kinds.iter().all(|kind| *kind == AssocKeyKind::Int) {
        return None;
    }
    let key_values = module.array_key_values(name)?;
    if !key_values
        .iter()
        .all(|key| matches!(key, AssocKeyValue::Int(value) if *value >= 0))
    {
        return None;
    }
    let index = static_loose_array_constant_search_index(name, needle, module)?;
    Some(index.map(|index| match &key_values[index] {
        AssocKeyValue::Int(value) => *value,
        AssocKeyValue::Str(_) => unreachable!("integer key kinds checked above"),
    }))
}

pub(super) fn static_loose_assoc_string_key_search_result(
    name: &str,
    needle: &Expr,
    module: &WasmModule,
) -> Option<Option<String>> {
    let key_kinds = module.array_key_kinds(name)?;
    if !key_kinds.iter().all(|kind| *kind == AssocKeyKind::Str) {
        return None;
    }
    let key_values = module.array_key_values(name)?;
    let index = static_loose_array_constant_search_index(name, needle, module)?;
    Some(index.map(|index| match &key_values[index] {
        AssocKeyValue::Str(value) => value.clone(),
        AssocKeyValue::Int(_) => unreachable!("string key kinds checked above"),
    }))
}

pub(super) fn assoc_array_value_exprs(items: &[(Expr, Expr)]) -> Vec<Expr> {
    items.iter().map(|(_, value)| value.clone()).collect()
}
