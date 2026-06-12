//! Purpose:
//! Infers foreach key metadata for `array_flip` results in wasm32-web local collection.
//! Keeps flip-specific value-to-key mapping out of the main metadata collector.
//!
//! Called from:
//! - `super::foreach_key_local_kind()` and transform key metadata helpers.
//!
//! Key details:
//! - PHP only turns string and integer values into array keys for these tracked shapes.
//! - Unsupported value-cell kinds are ignored consistently with the previous in-file helper.
//! - Flipped values are inferred from the original key kinds when those keys are statically known.

use super::*;

pub(super) fn array_flip_foreach_key_local_kind(
    args: &[Expr],
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> LocalKind {
    array_flip_foreach_key_kinds(args, array_value_kinds)
        .map(|kinds| {
            if kinds.iter().all(|kind| *kind == AssocKeyKind::Str) {
                LocalKind::Str
            } else if kinds.iter().all(|kind| *kind == AssocKeyKind::Int) {
                LocalKind::I64
            } else {
                LocalKind::Mixed
            }
        })
        .unwrap_or(LocalKind::I64)
}

pub(super) fn array_flip_foreach_key_kinds(
    args: &[Expr],
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<Vec<AssocKeyKind>> {
    let Some(source) = args.first() else {
        return None;
    };
    let kinds = match &source.kind {
        ExprKind::ArrayLiteral(items) => static_value_cell_kinds_for_items(items),
        ExprKind::ArrayLiteralAssoc(items) => static_value_cell_kinds_for_assoc_items(items),
        ExprKind::Variable(name) => array_value_kinds.get(name).cloned(),
        _ => None,
    };
    kinds.map(|kinds| {
        kinds
            .into_iter()
            .filter_map(|kind| match kind {
                ValueCellKind::Str => Some(AssocKeyKind::Str),
                ValueCellKind::Int => Some(AssocKeyKind::Int),
                _ => None,
            })
            .collect()
    })
}

pub(super) fn array_flip_value_kinds(
    args: &[Expr],
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<Vec<ValueCellKind>> {
    let source = args.first()?;
    let key_kinds = match &source.kind {
        ExprKind::ArrayLiteral(_) => None,
        ExprKind::ArrayLiteralAssoc(items) => static_assoc_key_kinds_for_items(items),
        ExprKind::Variable(name) => array_key_kinds.get(name).cloned(),
        _ => None,
    }?;
    Some(
        key_kinds
            .into_iter()
            .map(|kind| match kind {
                AssocKeyKind::Str => ValueCellKind::Str,
                AssocKeyKind::Int => ValueCellKind::Int,
            })
            .collect(),
    )
}
