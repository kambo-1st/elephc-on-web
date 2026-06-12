//! Purpose:
//! Infers value-cell kinds for known associative and nested array accesses.
//! Keeps access-specific metadata lookup out of the main local collector.
//!
//! Called from:
//! - `super::infer_assignment_local_kind()`.
//!
//! Key details:
//! - Static keys and indexes are resolved against compile-time array metadata only.
//! - Missing nested associative keys produce null metadata to match PHP reads.

use super::*;

pub(super) fn assoc_array_access_value_kind(
    array: &Expr,
    index: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) -> Option<ValueCellKind> {
    let ExprKind::Variable(name) = &array.kind else {
        return None;
    };
    let key = static_assoc_key_value_for_expr(index)?;
    let entry_index = array_key_values
        .get(name)?
        .iter()
        .position(|candidate| *candidate == key)?;
    array_value_kinds.get(name)?.get(entry_index).copied()
}

pub(super) fn nested_array_access_value_kind(
    array: &Expr,
    index: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) -> Option<ValueCellKind> {
    let ExprKind::ArrayAccess { array: outer, index: outer_index } = &array.kind else {
        return None;
    };
    let ExprKind::Variable(name) = &outer.kind else {
        return None;
    };
    let outer_index = if let Some(offset) = static_or_const_int_value_for_locals(outer_index)
        .and_then(|value| usize::try_from(value).ok())
    {
        offset
    } else {
        let key = static_assoc_key_value_for_expr(outer_index)?;
        array_key_values
            .get(name)?
            .iter()
            .position(|candidate| *candidate == key)?
    };
    let metadata = array_nested_values.get(name)?.get(outer_index)?.as_ref()?;
    match metadata.layout {
        ArrayLayout::CompactInt | ArrayLayout::Value => {
            let index = static_or_const_int_value_for_locals(index)?;
            let index = usize::try_from(index).ok()?;
            metadata.value_kinds.as_ref()?.get(index).copied()
        }
        ArrayLayout::Assoc => {
            let key = static_assoc_key_value_for_expr(index)?;
            let Some(entry_index) = metadata
                .key_values
                .as_ref()?
                .iter()
                .position(|candidate| *candidate == key)
            else {
                return Some(ValueCellKind::Null);
            };
            metadata.value_kinds.as_ref()?.get(entry_index).copied()
        }
    }
}
