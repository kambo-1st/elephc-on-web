//! Purpose:
//! Computes wasm32-web associative array_merge() metadata for keys and value cells.
//! Keeps PHP key normalization metadata separate from merge emission loops.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_merge`
//!
//! Key details:
//! - Metadata tracks duplicate string-key replacement, integer-key reindexing, and nested array values.

use super::*;
use super::array_merge_key_set::{static_assoc_key_value, StaticAssocKey};

pub(super) struct AssocArrayMergeMetadata {
    pub(super) keys: Vec<AssocKeyValue>,
    pub(super) value_kinds: Vec<ValueCellKind>,
    pub(super) nested_values: Vec<Option<NestedArrayMetadata>>,
}

pub(super) fn assoc_array_merge_metadata(
    args: &[Expr],
    module: &WasmModule,
) -> Result<Option<AssocArrayMergeMetadata>, CompileError> {
    let mut keys = Vec::new();
    let mut values = Vec::new();
    let mut nested_values = Vec::new();
    let mut next_int_key = 0i64;
    for arg in args {
        match &arg.kind {
            ExprKind::Variable(source)
                if module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::Assoc =>
            {
                let Some(source_keys) = module.array_key_values(source) else {
                    return Ok(None);
                };
                let Some(source_kinds) = module.array_key_kinds(source) else {
                    return Ok(None);
                };
                let Some(source_values) = module.array_value_cell_kinds(source) else {
                    return Ok(None);
                };
                let source_nested_values = module.array_nested_value_metadata_items(source);
                for (index, ((key, key_kind), value_kind)) in source_keys
                    .iter()
                    .zip(source_kinds.iter())
                    .zip(source_values.iter())
                    .enumerate()
                {
                    assoc_array_merge_metadata_push(
                        match key_kind {
                            AssocKeyKind::Int => AssocKeyValue::Int(next_int_key),
                            AssocKeyKind::Str => key.clone(),
                        },
                        *value_kind,
                        source_nested_values
                            .and_then(|metadata| metadata.get(index).cloned())
                            .flatten(),
                        &mut keys,
                        &mut values,
                        &mut nested_values,
                    );
                    if matches!(key_kind, AssocKeyKind::Int) {
                        next_int_key += 1;
                    }
                }
            }
            ExprKind::Variable(source)
                if module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::CompactInt =>
            {
                let Some(len) = module.array_length(source) else {
                    return Ok(None);
                };
                for _ in 0..len {
                    assoc_array_merge_metadata_push(
                        AssocKeyValue::Int(next_int_key),
                        ValueCellKind::Int,
                        None,
                        &mut keys,
                        &mut values,
                        &mut nested_values,
                    );
                    next_int_key += 1;
                }
            }
            ExprKind::Variable(source)
                if module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::Value =>
            {
                let Some(source_values) = module.array_value_cell_kinds(source) else {
                    return Ok(None);
                };
                let source_nested_values = module.array_nested_value_metadata_items(source);
                for (index, value_kind) in source_values.iter().enumerate() {
                    assoc_array_merge_metadata_push(
                        AssocKeyValue::Int(next_int_key),
                        *value_kind,
                        source_nested_values
                            .and_then(|metadata| metadata.get(index).cloned())
                            .flatten(),
                        &mut keys,
                        &mut values,
                        &mut nested_values,
                    );
                    next_int_key += 1;
                }
            }
            ExprKind::FunctionCall { name, .. }
                if module.has_function(name)
                    && module.function_return_kind(name) == Some(ValueKind::Array)
                    && module.function_array_return_layout(name) == ArrayLayout::Assoc =>
            {
                let Some(source_keys) = module.function_array_return_key_values(name) else {
                    return Ok(None);
                };
                let Some(source_kinds) = module.function_array_return_key_kinds(name) else {
                    return Ok(None);
                };
                let Some(source_values) = module.function_array_return_value_kinds(name) else {
                    return Ok(None);
                };
                let source_nested_values = module.function_array_return_nested_values(name);
                for (index, ((key, key_kind), value_kind)) in source_keys
                    .iter()
                    .zip(source_kinds.iter())
                    .zip(source_values.iter())
                    .enumerate()
                {
                    assoc_array_merge_metadata_push(
                        match key_kind {
                            AssocKeyKind::Int => AssocKeyValue::Int(next_int_key),
                            AssocKeyKind::Str => key.clone(),
                        },
                        *value_kind,
                        source_nested_values
                            .and_then(|metadata| metadata.get(index).cloned())
                            .flatten(),
                        &mut keys,
                        &mut values,
                        &mut nested_values,
                    );
                    if matches!(key_kind, AssocKeyKind::Int) {
                        next_int_key += 1;
                    }
                }
            }
            ExprKind::ArrayLiteralAssoc(items) => {
                for (key, value) in items {
                    let key = match static_assoc_key_value(key, module)? {
                        StaticAssocKey::Int(_) => {
                            let key = AssocKeyValue::Int(next_int_key);
                            next_int_key += 1;
                            key
                        }
                        StaticAssocKey::Str(key) => AssocKeyValue::Str(key),
                    };
                    let Some(value_kind) = value_cell_kind_for_expr(value, module) else {
                        return Ok(None);
                    };
                    assoc_array_merge_metadata_push(
                        key,
                        value_kind,
                        nested_array_metadata_for_expr(value, module),
                        &mut keys,
                        &mut values,
                        &mut nested_values,
                    );
                }
            }
            ExprKind::ArrayLiteral(items) => {
                for value in items {
                    let Some(value_kind) = value_cell_kind_for_expr(value, module) else {
                        return Ok(None);
                    };
                    assoc_array_merge_metadata_push(
                        AssocKeyValue::Int(next_int_key),
                        value_kind,
                        nested_array_metadata_for_expr(value, module),
                        &mut keys,
                        &mut values,
                        &mut nested_values,
                    );
                    next_int_key += 1;
                }
            }
            _ => return Ok(None),
        }
    }
    Ok(Some(AssocArrayMergeMetadata { keys, value_kinds: values, nested_values }))
}

pub(super) fn assoc_array_merge_metadata_push(
    key: AssocKeyValue,
    value_kind: ValueCellKind,
    nested_value: Option<NestedArrayMetadata>,
    keys: &mut Vec<AssocKeyValue>,
    values: &mut Vec<ValueCellKind>,
    nested_values: &mut Vec<Option<NestedArrayMetadata>>,
) {
    if matches!(key, AssocKeyValue::Str(_)) {
        if let Some(index) = keys.iter().position(|existing| *existing == key) {
            values[index] = value_kind;
            nested_values[index] = nested_value;
            return;
        }
    }
    keys.push(key);
    values.push(value_kind);
    nested_values.push(nested_value);
}

pub(super) fn assoc_array_merge_runtime_key_kind(args: &[Expr], module: &WasmModule) -> Option<AssocKeyKind> {
    let mut kind = None;
    for arg in args {
        let source_kind = match &arg.kind {
            ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
                match module.array_layout(source) {
                    ArrayLayout::Assoc => module.array_runtime_key_kind(source).or_else(|| {
                        let kinds = module.array_key_kinds(source)?;
                        let first = *kinds.first()?;
                        kinds.iter().all(|kind| *kind == first).then_some(first)
                    })?,
                    ArrayLayout::Value | ArrayLayout::CompactInt => AssocKeyKind::Int,
                }
            }
            ExprKind::ArrayLiteralAssoc(items) => {
                let kinds = items
                    .iter()
                    .map(|(key, _)| {
                        static_assoc_key_value(key, module).ok().map(|key| match key {
                            StaticAssocKey::Int(_) => AssocKeyKind::Int,
                            StaticAssocKey::Str(_) => AssocKeyKind::Str,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?;
                let first = *kinds.first()?;
                kinds.iter().all(|kind| *kind == first).then_some(first)?
            }
            ExprKind::ArrayLiteral(_) => AssocKeyKind::Int,
            _ => return None,
        };
        if let Some(existing) = kind {
            if existing != source_kind {
                return None;
            }
        } else {
            kind = Some(source_kind);
        }
    }
    kind
}

pub(super) fn assoc_array_merge_has_php_normalized_runtime_keys(args: &[Expr], module: &WasmModule) -> bool {
    args.iter().any(|arg| match &arg.kind {
        ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
            module.array_has_php_normalized_runtime_keys(source)
        }
        ExprKind::FunctionCall { name, .. }
            if module.has_function(name)
                && module.function_return_kind(name) == Some(ValueKind::Array) =>
        {
            false
        }
        _ => false,
    })
}

pub(super) fn assoc_array_merge_runtime_value_kind(args: &[Expr], module: &WasmModule) -> Option<ValueCellKind> {
    let mut kind = None;
    for arg in args {
        let source_kind = match &arg.kind {
            ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
                homogeneous_array_value_kind(source, module)?
            }
            ExprKind::ArrayLiteralAssoc(items) => {
                let kinds = value_cell_kinds_for_assoc_items(items, module)?;
                let first = *kinds.first()?;
                kinds.iter().all(|kind| *kind == first).then_some(first)?
            }
            ExprKind::ArrayLiteral(items) => {
                let kinds = value_cell_kinds_for_items(items, module)?;
                let first = *kinds.first()?;
                kinds.iter().all(|kind| *kind == first).then_some(first)?
            }
            _ => return None,
        };
        if let Some(existing) = kind {
            if existing != source_kind {
                return None;
            }
        } else {
            kind = Some(source_kind);
        }
    }
    kind
}

pub(super) fn assoc_key_kind_for_value(value: &AssocKeyValue) -> AssocKeyKind {
    match value {
        AssocKeyValue::Int(_) => AssocKeyKind::Int,
        AssocKeyValue::Str(_) => AssocKeyKind::Str,
    }
}
