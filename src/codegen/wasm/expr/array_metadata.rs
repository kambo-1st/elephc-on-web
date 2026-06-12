//! Purpose:
//! Infers wasm32-web array value-cell, key, constant, and nested-array metadata.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` and its array lowering submodules.
//!
//! Key details:
//! - Metadata must describe the boxed runtime value-cell layout without emitting
//!   WAT, so lowering code can choose runtime helpers without duplicating shape logic.

use super::*;

pub(in crate::codegen::wasm) fn value_cell_kinds_for_items(
    items: &[Expr],
    module: &WasmModule,
) -> Option<Vec<ValueCellKind>> {
    items
        .iter()
        .map(|value| value_cell_kind_for_expr(value, module))
        .collect()
}

pub(in crate::codegen::wasm) fn value_cell_kinds_for_assoc_items(
    items: &[(Expr, Expr)],
    module: &WasmModule,
) -> Option<Vec<ValueCellKind>> {
    items
        .iter()
        .map(|(_, value)| value_cell_kind_for_expr(value, module))
        .collect()
}

pub(in crate::codegen::wasm) fn value_cell_constants_for_items(
    items: &[Expr],
    module: &WasmModule,
) -> Option<Vec<ConstantValue>> {
    items
        .iter()
        .map(|value| static_scalar_value(value, module))
        .collect()
}

pub(in crate::codegen::wasm) fn value_cell_constants_for_assoc_items(
    items: &[(Expr, Expr)],
    module: &WasmModule,
) -> Option<Vec<ConstantValue>> {
    items
        .iter()
        .map(|(_, value)| static_scalar_value(value, module))
        .collect()
}

pub(in crate::codegen::wasm) fn array_value_constants_after_pop_shift(
    name: &str,
    len: usize,
    pop: bool,
    module: &WasmModule,
) -> Option<Vec<ConstantValue>> {
    module.array_value_constants(name).map(|values| {
        if pop {
            values[..len - 1].to_vec()
        } else {
            values[1..].to_vec()
        }
    })
}

pub(in crate::codegen::wasm) fn key_kinds_for_assoc_items(
    items: &[(Expr, Expr)],
) -> Option<Vec<AssocKeyKind>> {
    items
        .iter()
        .map(|(key, _)| key_kind_for_assoc_key(key))
        .collect()
}

fn key_kind_for_assoc_key(key: &Expr) -> Option<AssocKeyKind> {
    match &key.kind {
        ExprKind::IntLiteral(_) => Some(AssocKeyKind::Int),
        ExprKind::Negate(inner) if matches!(inner.kind, ExprKind::IntLiteral(_)) => {
            Some(AssocKeyKind::Int)
        }
        ExprKind::StringLiteral(value) => {
            if literal_php_array_int_key(value).is_some() {
                Some(AssocKeyKind::Int)
            } else {
                Some(AssocKeyKind::Str)
            }
        }
        _ => None,
    }
}

pub(in crate::codegen::wasm) fn value_cell_kind_for_expr(
    value: &Expr,
    module: &WasmModule,
) -> Option<ValueCellKind> {
    if let Some(value) = static_scalar_value(value, module) {
        return Some(match value {
            ConstantValue::Int(_) => ValueCellKind::Int,
            ConstantValue::Float(_) => ValueCellKind::Float,
            ConstantValue::Bool(_) => ValueCellKind::Bool,
            ConstantValue::Str(_) => ValueCellKind::Str,
            ConstantValue::Null => ValueCellKind::Null,
        });
    }
    match &value.kind {
        ExprKind::Null => Some(ValueCellKind::Null),
        ExprKind::NullsafePropertyAccess { object, .. }
        | ExprKind::NullsafeDynamicPropertyAccess { object, .. }
        | ExprKind::NullsafeMethodCall { object, .. }
            if matches!(object.kind, ExprKind::Null) =>
        {
            Some(ValueCellKind::Null)
        }
        ExprKind::FloatLiteral(_) => Some(ValueCellKind::Float),
        ExprKind::Negate(inner) if matches!(inner.kind, ExprKind::FloatLiteral(_)) => {
            Some(ValueCellKind::Float)
        }
        ExprKind::Negate(inner)
            if matches!(inner.kind, ExprKind::IntLiteral(_) | ExprKind::ConstRef(_)) =>
        {
            Some(ValueCellKind::Int)
        }
        ExprKind::StringLiteral(_) => Some(ValueCellKind::Str),
        ExprKind::BoolLiteral(_) => Some(ValueCellKind::Bool),
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => Some(ValueCellKind::Array),
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Str) => {
            Some(ValueCellKind::Str)
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::F64) => {
            Some(ValueCellKind::Float)
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::I32) => {
            Some(ValueCellKind::Bool)
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::I64) => {
            Some(ValueCellKind::Int)
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Array) => {
            Some(ValueCellKind::Array)
        }
        ExprKind::ArrayAccess { array, index } => {
            if let Some(kind) = nested_array_static_access_kind(array, index, module) {
                return Some(kind);
            }
            if let Some(kind) = direct_array_chunk_first_scalar_kind(array, index, module) {
                return Some(kind);
            }
            if let Some(kind) = assoc_static_access_kind(array, index, module) {
                return Some(kind);
            }
            let ExprKind::Variable(name) = &array.kind else {
                return None;
            };
            if module.local_kind(name) != Some(LocalKind::Array)
                || module.array_layout(name) != ArrayLayout::Value
            {
                return None;
            }
            let index = usize::try_from(static_or_const_or_i64_local_value(index, module)?).ok()?;
            module.array_value_cell_kind(name, index)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("call_user_func") => {
            let (target, call_args) = call_user_func_target(args, module)?;
            value_cell_kind_for_value_kind(callable_return_kind(&target, call_args, module)?)
        }
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("call_user_func_array") =>
        {
            value_cell_kind_for_value_kind(call_user_func_array_return_kind(args, module)?)
        }
        ExprKind::FunctionCall { name, .. } => {
            value_cell_kind_for_value_kind(module.function_return_kind(name)?)
        }
        ExprKind::ClosureCall { var, args } => {
            let target = callable_variable_target(module, var)?;
            value_cell_kind_for_value_kind(callable_return_kind(&target, args, module)?)
        }
        ExprKind::IntLiteral(_) | ExprKind::ConstRef(_) | ExprKind::Negate(_) => {
            Some(ValueCellKind::Int)
        }
        _ => None,
    }
}

fn value_cell_kind_for_value_kind(kind: ValueKind) -> Option<ValueCellKind> {
    match kind {
        ValueKind::Int => Some(ValueCellKind::Int),
        ValueKind::Float => Some(ValueCellKind::Float),
        ValueKind::Bool => Some(ValueCellKind::Bool),
        ValueKind::Str => Some(ValueCellKind::Str),
        ValueKind::Array => Some(ValueCellKind::Array),
        ValueKind::Null => Some(ValueCellKind::Null),
        ValueKind::Object => None,
        ValueKind::Mixed => None,
        ValueKind::Never => None,
    }
}

pub(in crate::codegen::wasm) fn nested_array_metadata_for_items(
    items: &[Expr],
    module: &WasmModule,
) -> Option<Vec<Option<NestedArrayMetadata>>> {
    let metadata = items
        .iter()
        .map(|item| nested_array_metadata_for_expr(item, module))
        .collect::<Vec<_>>();
    metadata.iter().any(Option::is_some).then_some(metadata)
}

pub(in crate::codegen::wasm) fn nested_array_metadata_for_assoc_items(
    items: &[(Expr, Expr)],
    module: &WasmModule,
) -> Option<Vec<Option<NestedArrayMetadata>>> {
    let metadata = items
        .iter()
        .map(|(_, item)| nested_array_metadata_for_expr(item, module))
        .collect::<Vec<_>>();
    metadata.iter().any(Option::is_some).then_some(metadata)
}

pub(in crate::codegen::wasm) fn array_filter_default_nested_metadata_for_items(
    items: &[Expr],
    module: &WasmModule,
) -> Option<Vec<Option<NestedArrayMetadata>>> {
    let metadata = items
        .iter()
        .filter_map(|item| {
            static_value_cell_truthiness_for_filter(item)
                .filter(|truthy| *truthy)
                .map(|_| nested_array_metadata_for_expr(item, module))
        })
        .collect::<Vec<_>>();
    metadata.iter().any(Option::is_some).then_some(metadata)
}

pub(in crate::codegen::wasm) fn array_filter_default_int_key_values_for_items(
    items: &[Expr],
) -> Option<Vec<AssocKeyValue>> {
    let mut keys = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if static_value_cell_truthiness_for_filter(item)? {
            keys.push(AssocKeyValue::Int(index as i64));
        }
    }
    Some(keys)
}

pub(in crate::codegen::wasm) fn array_filter_default_nested_metadata_for_assoc_items(
    items: &[(Expr, Expr)],
    module: &WasmModule,
) -> Option<Vec<Option<NestedArrayMetadata>>> {
    let normalized = normalize_assoc_items(items)?;
    let metadata = normalized
        .iter()
        .filter_map(|(_, item)| {
            static_value_cell_truthiness_for_filter(item)
                .filter(|truthy| *truthy)
                .map(|_| nested_array_metadata_for_expr(item, module))
        })
        .collect::<Vec<_>>();
    metadata.iter().any(Option::is_some).then_some(metadata)
}

pub(in crate::codegen::wasm) fn array_filter_default_nested_metadata_for_local(
    source: &str,
    module: &WasmModule,
) -> Option<Vec<Option<NestedArrayMetadata>>> {
    let metadata = module
        .array_nested_value_metadata_items(source)?
        .iter()
        .filter_map(|metadata| {
            metadata
                .as_ref()
                .filter(|metadata| metadata.len != 0)
                .map(|metadata| Some(metadata.clone()))
        })
        .collect::<Vec<_>>();
    metadata.iter().any(Option::is_some).then_some(metadata)
}

pub(in crate::codegen::wasm) fn array_filter_default_int_key_values_for_local(
    source: &str,
    module: &WasmModule,
) -> Option<Vec<AssocKeyValue>> {
    let mut keys = Vec::new();
    for (index, metadata) in module.array_nested_value_metadata_items(source)?.iter().enumerate() {
        if metadata.as_ref().is_some_and(|metadata| metadata.len != 0) {
            keys.push(AssocKeyValue::Int(index as i64));
        }
    }
    Some(keys)
}

pub(in crate::codegen::wasm) fn array_filter_default_assoc_nested_metadata_for_local(
    source: &str,
    module: &WasmModule,
) -> Option<(Vec<AssocKeyValue>, Vec<Option<NestedArrayMetadata>>)> {
    let source_keys = module.array_key_values(source)?;
    let mut keys = Vec::new();
    let mut nested_values = Vec::new();
    for (index, metadata) in module.array_nested_value_metadata_items(source)?.iter().enumerate() {
        let Some(metadata) = metadata.as_ref().filter(|metadata| metadata.len != 0) else {
            continue;
        };
        keys.push(source_keys.get(index)?.clone());
        nested_values.push(Some(metadata.clone()));
    }
    nested_values
        .iter()
        .any(Option::is_some)
        .then_some((keys, nested_values))
}

pub(super) fn array_filter_callback_assoc_nested_metadata_for_local(
    source: &str,
    shape: ArrayFilterCallbackShape,
    module: &WasmModule,
) -> Option<(Vec<AssocKeyValue>, Vec<Option<NestedArrayMetadata>>)> {
    if shape != ArrayFilterCallbackShape::Array {
        return None;
    }
    let value_kinds = module.array_value_cell_kinds(source)?;
    if !value_kinds.iter().all(|kind| *kind == ValueCellKind::Array) {
        return None;
    }
    let keys = module.array_key_values(source)?.to_vec();
    let nested_values = module.array_nested_value_metadata_items(source)?.to_vec();
    nested_values
        .iter()
        .any(Option::is_some)
        .then_some((keys, nested_values))
}

pub(in crate::codegen::wasm) fn nested_array_metadata_for_expr(
    value: &Expr,
    module: &WasmModule,
) -> Option<NestedArrayMetadata> {
    match &value.kind {
        ExprKind::ArrayLiteral(items) => {
            let value_kinds = value_cell_kinds_for_items(items, module);
            Some(NestedArrayMetadata {
                layout: if value_kinds.is_some() {
                    ArrayLayout::Value
                } else {
                    ArrayLayout::CompactInt
                },
                len: items.len(),
                value_kinds,
                key_values: None,
                nested_values: nested_array_metadata_for_items(items, module),
            })
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            let normalized = normalize_assoc_items(items);
            let items = normalized.as_deref().unwrap_or(items);
            Some(NestedArrayMetadata {
                layout: ArrayLayout::Assoc,
                len: items.len(),
                value_kinds: value_cell_kinds_for_assoc_items(items, module),
                key_values: key_values_for_assoc_items(items),
                nested_values: nested_array_metadata_for_assoc_items(items, module),
            })
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Array) => {
            Some(NestedArrayMetadata {
                layout: module.array_layout(name),
                len: module.array_length(name)?,
                value_kinds: module.array_value_cell_kinds(name).map(|kinds| kinds.to_vec()),
                key_values: module.array_key_values(name).map(|values| values.to_vec()),
                nested_values: module
                    .array_nested_value_metadata_items(name)
                    .map(|values| values.to_vec()),
            })
        }
        _ => None,
    }
}
