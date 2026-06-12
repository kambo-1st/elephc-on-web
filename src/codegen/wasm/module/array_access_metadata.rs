//! Purpose:
//! Computes WASM-only compile-time metadata for array access expressions.
//! Keeps nested indexed/associative access inference out of the main module state file.
//!
//! Called from:
//! - `super::infer_assignment_local_kind()` and return/local metadata probes.
//!
//! Key details:
//! - These helpers never emit code; they only infer known nested shapes and value-cell kinds.
//! - Dynamic or unsupported shapes return `None`/`false` so later codegen can use runtime paths or reject.

use std::collections::{HashMap, HashSet};

use crate::parser::ast::{Expr, ExprKind};

use super::{
    function_key, static_assoc_key_value_for_expr, static_or_const_int_value_for_locals,
    static_value_cell_kind_for_expr, ArrayLayout, AssocKeyKind, AssocKeyValue, LocalKind,
    NestedArrayMetadata, ValueCellKind,
};

pub(super) fn assoc_mixed_key_access_needs_mixed_cell_for_locals(
    array: &Expr,
    index: &Expr,
    locals: &HashMap<String, LocalKind>,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    php_normalized_key_arrays: &HashSet<String>,
) -> bool {
    let ExprKind::Variable(source) = &array.kind else {
        return false;
    };
    let ExprKind::Variable(key) = &index.kind else {
        return false;
    };
    locals.get(source) == Some(&LocalKind::Array)
        && locals.get(key) == Some(&LocalKind::Mixed)
        && (array_key_kinds.contains_key(source)
            || array_key_values.contains_key(source)
            || php_normalized_key_arrays.contains(source))
}

pub(super) fn dynamic_outer_nested_assoc_access_value_kind_for_locals(
    array: &Expr,
    index: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
) -> Option<ValueCellKind> {
    let ExprKind::ArrayAccess {
        array: outer_array,
        index: outer_index,
    } = &array.kind
    else {
        return None;
    };
    let ExprKind::Variable(outer_name) = &outer_array.kind else {
        return None;
    };
    if static_assoc_key_value_for_expr(outer_index).is_some()
        || static_or_const_int_value_for_locals(outer_index).is_some()
    {
        return None;
    }
    let metadata = common_nested_assoc_metadata_for_locals(outer_name, array_nested_values)?;
    if metadata.layout != ArrayLayout::Assoc {
        return None;
    }
    if static_assoc_key_value_for_expr(index).is_some() {
        return metadata_value_kind_child_for_locals(&metadata, index);
    }
    homogeneous_nested_assoc_value_kind_for_locals(&metadata)
}

pub(super) fn dynamic_outer_nested_assoc_access_needs_mixed_cell_for_locals(
    array: &Expr,
    index: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
) -> bool {
    if static_assoc_key_value_for_expr(index).is_some()
        || static_or_const_int_value_for_locals(index).is_some()
    {
        return false;
    }
    let ExprKind::ArrayAccess {
        array: outer_array,
        index: outer_index,
    } = &array.kind
    else {
        return false;
    };
    let ExprKind::Variable(outer_name) = &outer_array.kind else {
        return false;
    };
    if static_assoc_key_value_for_expr(outer_index).is_some()
        || static_or_const_int_value_for_locals(outer_index).is_some()
    {
        return false;
    }
    let Some(metadata) = common_nested_assoc_metadata_for_locals(outer_name, array_nested_values) else {
        return false;
    };
    metadata.layout == ArrayLayout::Assoc
        && homogeneous_nested_assoc_value_kind_for_locals(&metadata).is_none()
}

fn common_nested_assoc_metadata_for_locals(
    name: &str,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
) -> Option<NestedArrayMetadata> {
    let mut metadata = array_nested_values
        .get(name)?
        .iter()
        .cloned()
        .collect::<Option<Vec<_>>>()?
        .into_iter();
    let first = metadata.next()?;
    metadata.all(|candidate| candidate == first).then_some(first)
}

pub(in crate::codegen::wasm::module) fn homogeneous_nested_assoc_value_kind_for_locals(
    metadata: &NestedArrayMetadata,
) -> Option<ValueCellKind> {
    if metadata.layout != ArrayLayout::Assoc {
        return None;
    }
    let values = metadata.value_kinds.as_ref()?;
    let first = values.first().copied()?;
    values.iter().all(|kind| *kind == first).then_some(first)
}

pub(super) fn dynamic_nested_assoc_access_for_locals(
    array: &Expr,
    index: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) -> bool {
    if heterogeneous_dynamic_middle_assoc_leaf_needs_mixed_cell_for_locals(
        array,
        array_nested_values,
        array_key_values,
    ) {
        return true;
    }
    let Some(metadata) = array_access_metadata_for_locals(array, array_nested_values, array_key_values)
        .or_else(|| {
            dynamic_nested_array_value_metadata_for_locals(
                array,
                array_nested_values,
                array_key_values,
            )
        })
    else {
        return false;
    };
    if metadata.layout != ArrayLayout::Assoc {
        return false;
    }
    let static_key = static_assoc_key_value_for_expr(index);
    static_key.is_none()
        || (metadata.key_values.is_none()
            && homogeneous_nested_assoc_value_kind_for_locals(&metadata).is_none())
}

fn heterogeneous_dynamic_middle_assoc_leaf_needs_mixed_cell_for_locals(
    array: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) -> bool {
    let ExprKind::ArrayAccess {
        array: parent,
        index: middle_index,
    } = &array.kind
    else {
        return false;
    };
    if static_assoc_key_value_for_expr(middle_index).is_some()
        || static_or_const_int_value_for_locals(middle_index).is_some()
    {
        return false;
    }
    let Some(parent_metadata) =
        array_access_metadata_for_locals(parent, array_nested_values, array_key_values)
    else {
        return false;
    };
    parent_metadata.layout == ArrayLayout::Assoc
        && homogeneous_nested_assoc_value_kind_for_locals(&parent_metadata)
            == Some(ValueCellKind::Array)
        && parent_metadata
            .nested_values
            .as_ref()
            .is_some_and(|children| {
                children
                    .iter()
                    .any(|child| child.as_ref().is_some_and(|child| child.layout == ArrayLayout::Assoc))
            })
}

pub(super) fn dynamic_parent_nested_assoc_access_value_kind_for_locals(
    array: &Expr,
    index: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) -> Option<ValueCellKind> {
    let metadata = dynamic_nested_array_value_metadata_for_locals(
        array,
        array_nested_values,
        array_key_values,
    )?;
    if metadata.layout != ArrayLayout::Assoc {
        return None;
    }
    if static_assoc_key_value_for_expr(index).is_some() {
        return metadata_value_kind_child_for_locals(&metadata, index);
    }
    homogeneous_nested_assoc_value_kind_for_locals(&metadata)
}

pub(super) fn dynamic_nested_array_value_metadata_for_locals(
    value: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) -> Option<NestedArrayMetadata> {
    let ExprKind::ArrayAccess { array, index } = &value.kind else {
        return None;
    };
    if static_assoc_key_value_for_expr(index).is_some()
        || static_or_const_int_value_for_locals(index).is_some()
    {
        return None;
    }
    let parent_metadata = array_access_metadata_for_locals(array, array_nested_values, array_key_values)
        .or_else(|| dynamic_outer_nested_assoc_metadata_for_locals(array, array_nested_values))?;
    if parent_metadata.layout != ArrayLayout::Assoc
        || homogeneous_nested_assoc_value_kind_for_locals(&parent_metadata) != Some(ValueCellKind::Array)
    {
        return None;
    }
    common_nested_child_metadata_for_locals(&parent_metadata)
}

fn dynamic_outer_nested_assoc_metadata_for_locals(
    value: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
) -> Option<NestedArrayMetadata> {
    let ExprKind::ArrayAccess {
        array: outer_array,
        index: outer_index,
    } = &value.kind
    else {
        return None;
    };
    let ExprKind::Variable(outer_name) = &outer_array.kind else {
        return None;
    };
    if static_assoc_key_value_for_expr(outer_index).is_some()
        || static_or_const_int_value_for_locals(outer_index).is_some()
    {
        return None;
    }
    common_nested_assoc_metadata_for_locals(outer_name, array_nested_values)
}

fn common_nested_child_metadata_for_locals(
    metadata: &NestedArrayMetadata,
) -> Option<NestedArrayMetadata> {
    let mut nested = metadata
        .nested_values
        .as_ref()?
        .iter()
        .cloned()
        .collect::<Option<Vec<_>>>()?
        .into_iter();
    let first = nested.next()?;
    nested.all(|candidate| candidate == first).then_some(first)
}

pub(super) fn direct_array_chunk_first_scalar_kind_for_locals(
    array: &Expr,
    index: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_layouts: &HashMap<String, ArrayLayout>,
) -> Option<ValueCellKind> {
    if static_or_const_int_value_for_locals(index) != Some(0) {
        return None;
    }
    let ExprKind::ArrayAccess { array: chunks, index: outer_index } = &array.kind else {
        return None;
    };
    if static_or_const_int_value_for_locals(outer_index) != Some(0) {
        return None;
    }
    let ExprKind::FunctionCall { name, args } = &chunks.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_chunk") || args.len() != 2 {
        return None;
    }
    match &args[0].kind {
        ExprKind::ArrayLiteral(items) => items.first().and_then(static_value_cell_kind_for_expr),
        ExprKind::Variable(source) => array_value_kinds
            .get(source)
            .and_then(|kinds| kinds.first())
            .copied(),
        ExprKind::FunctionCall { name, .. } => function_array_return_value_kinds
            .get(&function_key(name))
            .and_then(|kinds| kinds.first())
            .copied()
            .or_else(|| {
                (function_array_return_layouts.get(&function_key(name)) == Some(&ArrayLayout::CompactInt))
                    .then_some(ValueCellKind::Int)
            }),
        _ => None,
    }
}

pub(super) fn array_access_metadata_for_locals(
    value: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) -> Option<NestedArrayMetadata> {
    let ExprKind::ArrayAccess { array, index } = &value.kind else {
        return None;
    };
    match &array.kind {
        ExprKind::Variable(name) => {
            let item_index = array_access_item_index_for_locals(name, index, array_key_values)?;
            array_nested_values.get(name)?.get(item_index)?.clone()
        }
        ExprKind::ArrayAccess { .. } => {
            let metadata = array_access_metadata_for_locals(array, array_nested_values, array_key_values)?;
            metadata_child_for_locals(&metadata, index)
        }
        _ => None,
    }
}

pub(super) fn array_access_value_cell_kind_for_locals(
    value: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) -> Option<ValueCellKind> {
    let ExprKind::ArrayAccess { array, index } = &value.kind else {
        return None;
    };
    if array_access_metadata_for_locals(value, array_nested_values, array_key_values).is_some() {
        return Some(ValueCellKind::Array);
    }
    match &array.kind {
        ExprKind::Variable(name) => {
            if let Some(key_values) = array_key_values.get(name) {
                let key = static_assoc_key_value_for_expr(index)?;
                let Some(entry_index) = key_values.iter().position(|candidate| *candidate == key) else {
                    return Some(ValueCellKind::Null);
                };
                return array_value_kinds.get(name)?.get(entry_index).copied();
            }
            let index = usize::try_from(static_or_const_int_value_for_locals(index)?).ok()?;
            array_value_kinds.get(name)?.get(index).copied()
        }
        ExprKind::ArrayAccess { .. } => {
            let metadata = array_access_metadata_for_locals(array, array_nested_values, array_key_values)?;
            metadata_value_kind_child_for_locals(&metadata, index)
        }
        _ => None,
    }
}

fn array_access_item_index_for_locals(
    name: &str,
    index: &Expr,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) -> Option<usize> {
    if let Some(offset) = static_or_const_int_value_for_locals(index)
        .and_then(|value| usize::try_from(value).ok())
    {
        return Some(offset);
    }
    let key = static_assoc_key_value_for_expr(index)?;
    array_key_values
        .get(name)?
        .iter()
        .position(|candidate| *candidate == key)
}

fn metadata_child_for_locals(
    metadata: &NestedArrayMetadata,
    index: &Expr,
) -> Option<NestedArrayMetadata> {
    let item_index = match metadata.layout {
        ArrayLayout::CompactInt | ArrayLayout::Value => {
            usize::try_from(static_or_const_int_value_for_locals(index)?).ok()?
        }
        ArrayLayout::Assoc => {
            let key = static_assoc_key_value_for_expr(index)?;
            if let Some(position) = metadata
                .key_values
                .as_ref()?
                .iter()
                .position(|candidate| *candidate == key)
            {
                position
            } else {
                return single_unknown_assoc_child_for_locals(metadata);
            }
        }
    };
    metadata.nested_values.as_ref()?.get(item_index)?.clone()
}

fn single_unknown_assoc_child_for_locals(
    metadata: &NestedArrayMetadata,
) -> Option<NestedArrayMetadata> {
    let known_key_count = metadata.key_values.as_ref().map_or(0, Vec::len);
    if known_key_count >= metadata.len {
        return None;
    }
    let candidates = metadata
        .nested_values
        .as_ref()?
        .iter()
        .skip(known_key_count)
        .filter_map(|value| {
            value
                .as_ref()
                .filter(|metadata| metadata.layout == ArrayLayout::Assoc)
        })
        .collect::<Vec<_>>();
    let [metadata] = candidates.as_slice() else {
        return None;
    };
    Some((*metadata).clone())
}

fn metadata_value_kind_child_for_locals(
    metadata: &NestedArrayMetadata,
    index: &Expr,
) -> Option<ValueCellKind> {
    if metadata_child_for_locals(metadata, index).is_some() {
        return Some(ValueCellKind::Array);
    }
    let item_index = match metadata.layout {
        ArrayLayout::CompactInt | ArrayLayout::Value => {
            usize::try_from(static_or_const_int_value_for_locals(index)?).ok()?
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
            entry_index
        }
    };
    metadata.value_kinds.as_ref()?.get(item_index).copied()
}
