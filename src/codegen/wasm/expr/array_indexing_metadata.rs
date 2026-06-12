//! Purpose:
//! Computes static nested array indexing metadata for wasm32-web array reads.
//! Keeps metadata queries separate from runtime traversal emitters.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` expression-kind, scalar, output, and nested assignment helpers.
//!
//! Key details:
//! - Static metadata preserves null-on-missing behavior and rejects unsupported layouts elsewhere.

use super::*;

pub(in crate::codegen::wasm) fn nested_array_access_requires_layout(array: &Expr) -> bool {
    matches!(array.kind, ExprKind::ArrayAccess { .. })
}

pub(in crate::codegen::wasm) fn nested_array_access_unsupported(expr: &Expr) -> CompileError {
    CompileError::new(
        expr.span,
        "wasm32-web nested array traversal requires nested array layout metadata",
    )
}

#[derive(Clone, Debug)]
pub(in crate::codegen::wasm) enum NestedArrayIndex {
    Offset(usize),
    Key(AssocKeyValue),
}

pub(in crate::codegen::wasm) fn nested_array_index_metadata(
    array: &Expr,
    index: &Expr,
    module: &WasmModule,
) -> Option<(NestedArrayMetadata, NestedArrayIndex)> {
    let metadata = nested_array_metadata_for_access_expr(array, module)?;
    let inner_index = match metadata.layout {
        ArrayLayout::Assoc => NestedArrayIndex::Key(static_assoc_access_key(index, module)?),
        ArrayLayout::CompactInt | ArrayLayout::Value => {
            let offset = static_or_const_or_i64_local_value(index, module)
                .and_then(|value| usize::try_from(value).ok())?;
            NestedArrayIndex::Offset(offset)
        }
    };
    Some((metadata, inner_index))
}

pub(in crate::codegen::wasm) fn nested_array_metadata_for_access_expr(
    expr: &Expr,
    module: &WasmModule,
) -> Option<NestedArrayMetadata> {
    let ExprKind::ArrayAccess { array: outer, index: outer_index } = &expr.kind else {
        return None;
    };
    match &outer.kind {
        ExprKind::Variable(name) => {
            if module.local_kind(name) != Some(LocalKind::Array) {
                return None;
            }
            let outer_index = match module.array_layout(name) {
                ArrayLayout::Value => {
                    usize::try_from(static_or_const_or_i64_local_value(outer_index, module)?).ok()?
                }
                ArrayLayout::Assoc => {
                    let key = static_assoc_access_key(outer_index, module)?;
                    if let Some(position) = module
                        .array_key_values(name)
                        .and_then(|keys| keys.iter().position(|candidate| *candidate == key))
                    {
                        position
                    } else if module.array_runtime_nested_value_metadata(name).is_some()
                        && matches!(
                            (module.array_runtime_key_kind(name), &key),
                            (Some(AssocKeyKind::Int), AssocKeyValue::Int(_))
                                | (Some(AssocKeyKind::Str), AssocKeyValue::Str(_))
                        )
                    {
                        return module.array_runtime_nested_value_metadata(name);
                    } else {
                        return None;
                    }
                }
                ArrayLayout::CompactInt => return None,
            };
            module
                .array_nested_value_metadata(name, outer_index)
                .or_else(|| module.array_runtime_nested_value_metadata(name))
        }
        ExprKind::ArrayAccess { .. } => {
            let metadata = nested_array_metadata_for_access_expr(outer, module)
                .or_else(|| dynamic_nested_array_value_metadata(outer, module))?;
            nested_array_metadata_child(&metadata, outer_index, module)
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            let property_info = module.object_static_property(receiver, property)?;
            if !module.object_member_is_accessible(&property_info.owner_class, &property_info.visibility) {
                return None;
            }
            let metadata = module
                .static_property_nested_value_metadata(&property_info)
                .or_else(|| {
                    property_info
                        .default
                        .as_ref()
                        .and_then(|default| nested_array_metadata_for_expr(default, module))
                })?;
            nested_array_metadata_child(&metadata, outer_index, module)
        }
        _ => None,
    }
}

fn nested_array_metadata_child(
    metadata: &NestedArrayMetadata,
    index: &Expr,
    module: &WasmModule,
) -> Option<NestedArrayMetadata> {
    let child_index = match metadata.layout {
        ArrayLayout::CompactInt | ArrayLayout::Value => {
            usize::try_from(static_or_const_or_i64_local_value(index, module)?).ok()?
        }
        ArrayLayout::Assoc => {
            let key = static_assoc_access_key(index, module)?;
            let Some(keys) = metadata.key_values.as_ref() else {
                return nested_array_single_unknown_assoc_child(metadata);
            };
            if let Some(position) = keys.iter().position(|candidate| *candidate == key) {
                position
            } else {
                return nested_array_single_unknown_assoc_child(metadata);
            }
        }
    };
    metadata
        .nested_values
        .as_ref()?
        .get(child_index)
        .cloned()
        .flatten()
}

fn nested_array_single_unknown_assoc_child(metadata: &NestedArrayMetadata) -> Option<NestedArrayMetadata> {
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

pub(in crate::codegen::wasm) fn nested_array_static_access_kind(
    array: &Expr,
    index: &Expr,
    module: &WasmModule,
) -> Option<ValueCellKind> {
    let (metadata, index) = nested_array_index_metadata(array, index, module)?;
    match metadata.layout {
        ArrayLayout::CompactInt => {
            let NestedArrayIndex::Offset(index) = index else {
                return None;
            };
            if index < metadata.len {
                Some(ValueCellKind::Int)
            } else {
                Some(ValueCellKind::Null)
            }
        }
        ArrayLayout::Value => {
            let NestedArrayIndex::Offset(index) = index else {
                return None;
            };
            metadata
                .value_kinds
                .as_ref()?
                .get(index)
                .copied()
                .or(Some(ValueCellKind::Null))
        }
        ArrayLayout::Assoc => {
            let NestedArrayIndex::Key(key) = index else {
                return None;
            };
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

pub(in crate::codegen::wasm) fn static_assoc_access_key(index: &Expr, module: &WasmModule) -> Option<AssocKeyValue> {
    if let Some(value) = static_or_const_int_value(index) {
        return Some(AssocKeyValue::Int(value));
    }
    static_string_value(index, module).map(AssocKeyValue::Str)
}
