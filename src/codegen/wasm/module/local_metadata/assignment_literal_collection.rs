//! Purpose:
//! Collects wasm32-web metadata for direct array literal assignments.
//! Keeps literal array bookkeeping out of the broader assignment collector.
//!
//! Called from:
//! - `super::assignment_collection::collect_assignment_locals()`.
//!
//! Key details:
//! - Static indexed literals clear assoc key metadata.
//! - Static associative literals preserve exact key/value metadata when known.

use super::*;

pub(super) fn collect_literal_assignment_metadata(
    name: &String,
    value: &Expr,
    array_value_kinds: &mut HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &mut HashMap<String, ValueCellKind>,
    array_nested_values: &mut HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_key_kinds: &mut HashMap<String, Vec<AssocKeyKind>>,
    array_key_values: &mut HashMap<String, Vec<AssocKeyValue>>,
) {
    if let ExprKind::ArrayLiteral(items) = &value.kind {
        if let Some(kinds) = static_value_cell_kinds_for_items(items) {
            array_value_kinds.insert(name.clone(), kinds);
        }
        array_runtime_value_kinds.remove(name);
        if let Some(metadata) = static_nested_array_metadata_for_items(items) {
            array_nested_values.insert(name.clone(), metadata);
        } else {
            array_nested_values.remove(name);
        }
        array_key_kinds.remove(name);
        array_key_values.remove(name);
    }
    if let ExprKind::ArrayLiteralAssoc(items) = &value.kind {
        if let Some(kinds) = static_value_cell_kinds_for_assoc_items(items) {
            array_value_kinds.insert(name.clone(), kinds);
        }
        array_runtime_value_kinds.remove(name);
        if let Some(metadata) = static_nested_array_metadata_for_assoc_items(items) {
            array_nested_values.insert(name.clone(), metadata);
        } else {
            array_nested_values.remove(name);
        }
        if let Some(kinds) = static_assoc_key_kinds_for_items(items) {
            array_key_kinds.insert(name.clone(), kinds);
        }
        if let Some(values) = static_assoc_key_values_for_items(items) {
            array_key_values.insert(name.clone(), values);
        }
    }
}
