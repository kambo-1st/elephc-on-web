//! Purpose:
//! Computes wasm32-web array_splice() metadata after key/value reindexing.
//! Keeps pure layout updates separate from splice emitter loops and heap copies.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_splice`.
//! - array transform helpers that reuse splice-style key reindexing.
//!
//! Key details:
//! - Preserves PHP key reindexing rules and value-cell kind/constant metadata for downstream reads.

use super::*;

pub(super) fn assoc_splice_reindexed_key_kinds(kinds: &[AssocKeyKind]) -> Vec<AssocKeyKind> {
    kinds
        .iter()
        .map(|kind| {
            if *kind == AssocKeyKind::Int {
                AssocKeyKind::Int
            } else {
                AssocKeyKind::Str
            }
        })
        .collect()
}

pub(super) fn assoc_splice_remaining_key_kinds(
    kinds: &[AssocKeyKind],
    start: usize,
    take: usize,
    replacement_len: usize,
) -> Vec<AssocKeyKind> {
    kinds
        .iter()
        .enumerate()
        .filter_map(|(index, kind)| {
            if (start..start + take).contains(&index) {
                None
            } else if *kind == AssocKeyKind::Int {
                Some(AssocKeyKind::Int)
            } else {
                Some(AssocKeyKind::Str)
            }
        })
        .take(start)
        .chain((0..replacement_len).map(|_| AssocKeyKind::Int))
        .chain(kinds.iter().enumerate().filter_map(|(index, kind)| {
            if index < start + take {
                None
            } else if *kind == AssocKeyKind::Int {
                Some(AssocKeyKind::Int)
            } else {
                Some(AssocKeyKind::Str)
            }
        }))
        .collect()
}

pub(super) fn assoc_splice_remaining_key_values(
    key_kinds: &[AssocKeyKind],
    key_values: Option<&[AssocKeyValue]>,
    start: usize,
    take: usize,
    replacement_items: Option<&[Expr]>,
) -> Option<Vec<AssocKeyValue>> {
    let key_values = key_values?;
    let mut out = Vec::with_capacity(key_kinds.len() - take + replacement_items.map_or(0, <[Expr]>::len));
    let mut next_int_key = 0i64;
    for index in 0..start {
        push_reindexed_assoc_key_value(&mut out, key_kinds[index], &key_values[index], &mut next_int_key);
    }
    if let Some(items) = replacement_items {
        for _ in items {
            out.push(AssocKeyValue::Int(next_int_key));
            next_int_key += 1;
        }
    }
    for index in start + take..key_kinds.len() {
        push_reindexed_assoc_key_value(&mut out, key_kinds[index], &key_values[index], &mut next_int_key);
    }
    Some(out)
}

pub(super) fn push_reindexed_assoc_key_value(
    out: &mut Vec<AssocKeyValue>,
    kind: AssocKeyKind,
    value: &AssocKeyValue,
    next_int_key: &mut i64,
) {
    match kind {
        AssocKeyKind::Int => {
            out.push(AssocKeyValue::Int(*next_int_key));
            *next_int_key += 1;
        }
        AssocKeyKind::Str => out.push(value.clone()),
    }
}

pub(super) fn assoc_splice_remaining_value_kinds(
    kinds: Option<&[ValueCellKind]>,
    start: usize,
    take: usize,
    replacement_items: Option<&[Expr]>,
    module: &WasmModule,
) -> Option<Vec<ValueCellKind>> {
    let kinds = kinds?;
    let mut out = Vec::with_capacity(kinds.len() - take + replacement_items.map_or(0, <[Expr]>::len));
    out.extend_from_slice(&kinds[..start]);
    if let Some(items) = replacement_items {
        for item in items {
            out.push(value_cell_kind_for_expr(item, module)?);
        }
    }
    out.extend_from_slice(&kinds[start + take..]);
    Some(out)
}

pub(super) fn value_splice_remaining_value_kinds(
    kinds: Option<&[ValueCellKind]>,
    start: usize,
    take: usize,
    replacement_items: Option<&[Expr]>,
    module: &WasmModule,
) -> Option<Vec<ValueCellKind>> {
    let kinds = kinds?;
    let mut out = Vec::with_capacity(kinds.len() - take + replacement_items.map_or(0, <[Expr]>::len));
    out.extend_from_slice(&kinds[..start]);
    if let Some(items) = replacement_items {
        for item in items {
            out.push(value_cell_kind_for_expr(item, module)?);
        }
    }
    out.extend_from_slice(&kinds[start + take..]);
    Some(out)
}

pub(super) fn splice_remaining_value_constants(
    values: Option<&[ConstantValue]>,
    start: usize,
    take: usize,
    replacement_items: Option<&[Expr]>,
    module: &WasmModule,
) -> Option<Vec<ConstantValue>> {
    let values = values?;
    let mut out = Vec::with_capacity(values.len() - take + replacement_items.map_or(0, <[Expr]>::len));
    out.extend_from_slice(&values[..start]);
    if let Some(items) = replacement_items {
        out.extend(value_cell_constants_for_items(items, module)?);
    }
    out.extend_from_slice(&values[start + take..]);
    Some(out)
}

pub(super) fn value_splice_local_replacement_value_kinds(
    source_kinds: Option<&[ValueCellKind]>,
    replacement_kinds: Option<&[ValueCellKind]>,
    start: usize,
    take: usize,
) -> Option<Vec<ValueCellKind>> {
    let source_kinds = source_kinds?;
    let replacement_kinds = replacement_kinds?;
    let mut out = Vec::with_capacity(source_kinds.len() - take + replacement_kinds.len());
    out.extend_from_slice(&source_kinds[..start]);
    out.extend_from_slice(replacement_kinds);
    out.extend_from_slice(&source_kinds[start + take..]);
    Some(out)
}

pub(super) fn value_splice_local_replacement_value_constants(
    source_values: Option<&[ConstantValue]>,
    replacement_values: Option<&[ConstantValue]>,
    start: usize,
    take: usize,
) -> Option<Vec<ConstantValue>> {
    let source_values = source_values?;
    let replacement_values = replacement_values?;
    let mut out = Vec::with_capacity(source_values.len() - take + replacement_values.len());
    out.extend_from_slice(&source_values[..start]);
    out.extend_from_slice(replacement_values);
    out.extend_from_slice(&source_values[start + take..]);
    Some(out)
}
