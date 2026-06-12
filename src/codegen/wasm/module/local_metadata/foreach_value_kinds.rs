//! Purpose:
//! Classifies known value-cell arrays into foreach local kinds.
//! Keeps repeated homogeneous-cell mapping out of the main local metadata collector.
//!
//! Called from:
//! - `super::foreach_value_local_kind()` and array transform metadata helpers.
//!
//! Key details:
//! - Empty or null-only value sets stay conservative as integer/mixed according to existing rules.
//! - Heterogeneous value-cell arrays produce `LocalKind::Mixed`.

use super::*;

pub(super) fn foreach_value_cell_local_kind(kinds: &[ValueCellKind]) -> LocalKind {
    homogeneous_foreach_value_local_kind(kinds)
}

pub(super) fn assoc_foreach_value_local_kind(kinds: &[ValueCellKind]) -> LocalKind {
    homogeneous_foreach_value_local_kind(kinds)
}

fn homogeneous_foreach_value_local_kind(kinds: &[ValueCellKind]) -> LocalKind {
    let Some(first) = kinds.first().copied() else {
        return LocalKind::I64;
    };
    if kinds.iter().all(|kind| *kind == first) {
        match first {
            ValueCellKind::Int => LocalKind::I64,
            ValueCellKind::Float => LocalKind::F64,
            ValueCellKind::Bool => LocalKind::I32,
            ValueCellKind::Str => LocalKind::Str,
            ValueCellKind::Array => LocalKind::Array,
            ValueCellKind::Null => LocalKind::Mixed,
        }
    } else {
        LocalKind::Mixed
    }
}
