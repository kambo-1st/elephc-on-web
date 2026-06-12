//! Purpose:
//! Collects wasm32-web metadata changes from direct array mutation statements.
//! Keeps `ArrayAssign` and `ArrayPush` bookkeeping out of the statement dispatcher.
//!
//! Called from:
//! - `super::collect_stmt_locals()` for `StmtKind::ArrayAssign` and `StmtKind::ArrayPush`.
//!
//! Key details:
//! - Narrow static metadata is retained only when the mutated slot remains statically known.
//! - Unknown mutation shapes clear stale array/key metadata instead of preserving unsafe facts.

use super::*;

pub(super) fn collect_array_assign_locals(
    array: &str,
    index: &Expr,
    value: &Expr,
    array_value_kinds: &mut HashMap<String, Vec<ValueCellKind>>,
    array_nested_values: &mut HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_key_kinds: &mut HashMap<String, Vec<AssocKeyKind>>,
    array_key_values: &mut HashMap<String, Vec<AssocKeyValue>>,
) {
    let Some(index) = static_or_const_int_value_for_locals(index) else {
        clear_array_metadata(
            array,
            array_value_kinds,
            array_nested_values,
            array_key_kinds,
            array_key_values,
        );
        return;
    };
    let Ok(index) = usize::try_from(index) else {
        clear_array_metadata(
            array,
            array_value_kinds,
            array_nested_values,
            array_key_kinds,
            array_key_values,
        );
        return;
    };
    let Some(kind) = static_value_cell_kind_for_expr(value) else {
        clear_array_metadata(
            array,
            array_value_kinds,
            array_nested_values,
            array_key_kinds,
            array_key_values,
        );
        return;
    };
    if let Some(kinds) = array_value_kinds.get_mut(array) {
        if let Some(slot) = kinds.get_mut(index) {
            *slot = kind;
        } else {
            clear_array_metadata(
                array,
                array_value_kinds,
                array_nested_values,
                array_key_kinds,
                array_key_values,
            );
        }
    }
}

pub(super) fn collect_array_push_locals(
    array: &str,
    value: &Expr,
    array_value_kinds: &mut HashMap<String, Vec<ValueCellKind>>,
    array_nested_values: &mut HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_key_kinds: &mut HashMap<String, Vec<AssocKeyKind>>,
    array_key_values: &mut HashMap<String, Vec<AssocKeyValue>>,
) {
    let Some(kind) = static_value_cell_kind_for_expr(value) else {
        clear_array_metadata(
            array,
            array_value_kinds,
            array_nested_values,
            array_key_kinds,
            array_key_values,
        );
        return;
    };
    if let Some(kinds) = array_value_kinds.get_mut(array) {
        kinds.push(kind);
    }
    array_nested_values.remove(array);
    array_key_kinds.remove(array);
    array_key_values.remove(array);
}

fn clear_array_metadata(
    array: &str,
    array_value_kinds: &mut HashMap<String, Vec<ValueCellKind>>,
    array_nested_values: &mut HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_key_kinds: &mut HashMap<String, Vec<AssocKeyKind>>,
    array_key_values: &mut HashMap<String, Vec<AssocKeyValue>>,
) {
    array_value_kinds.remove(array);
    array_nested_values.remove(array);
    array_key_kinds.remove(array);
    array_key_values.remove(array);
}
