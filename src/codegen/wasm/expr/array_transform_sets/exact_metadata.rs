//! Purpose:
//! Computes exact array transform metadata for wasm32-web array set operations.
//! Keeps static flip, unique, and value-set key/value metadata separate from WAT loop emission.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets`.
//!
//! Key details:
//! - Metadata must preserve PHP key normalization, duplicate-key behavior, and scalar string representations.

use super::*;

pub(super) struct ExactUniqueMetadata {
    values: Vec<ConstantValue>,
    value_kinds: Vec<ValueCellKind>,
    key_kinds: Vec<AssocKeyKind>,
    key_values: Vec<AssocKeyValue>,
}

pub(super) fn exact_indexed_value_flip_metadata(
    source: &str,
    module: &WasmModule,
) -> Option<ExactUniqueMetadata> {
    let values = module.array_value_constants(source)?;
    let value_kinds = module.array_value_cell_kinds(source)?;
    if values.len() != value_kinds.len() {
        return None;
    }
    let mut out_values = Vec::new();
    let mut out_value_kinds = Vec::new();
    let mut out_key_kinds = Vec::new();
    let mut out_key_values = Vec::new();
    for (index, value) in values.iter().enumerate() {
        let Some(key) = flip_key_from_constant_value(value) else {
            continue;
        };
        let replacement = ConstantValue::Int(i64::try_from(index).ok()?);
        if let Some(existing) = out_key_values.iter().position(|existing| existing == &key) {
            out_values[existing] = replacement;
        } else {
            out_key_kinds.push(assoc_key_kind_for_value(&key));
            out_key_values.push(key);
            out_values.push(replacement);
            out_value_kinds.push(ValueCellKind::Int);
        }
    }
    Some(ExactUniqueMetadata {
        values: out_values,
        value_kinds: out_value_kinds,
        key_kinds: out_key_kinds,
        key_values: out_key_values,
    })
}

pub(super) fn exact_assoc_value_flip_metadata(source: &str, module: &WasmModule) -> Option<ExactUniqueMetadata> {
    let values = module.array_value_constants(source)?;
    let value_kinds = module.array_value_cell_kinds(source)?;
    let key_kinds = module.array_key_kinds(source)?;
    let key_values = module.array_key_values(source)?;
    if values.len() != value_kinds.len() || values.len() != key_kinds.len() || values.len() != key_values.len() {
        return None;
    }
    let mut out_values = Vec::new();
    let mut out_value_kinds = Vec::new();
    let mut out_key_kinds = Vec::new();
    let mut out_key_values = Vec::new();
    for (value, (source_key_kind, source_key_value)) in values
        .iter()
        .zip(key_kinds.iter().zip(key_values.iter()))
    {
        let Some(key) = flip_key_from_constant_value(value) else {
            continue;
        };
        let Some(replacement) = constant_value_from_assoc_key(*source_key_kind, source_key_value) else {
            return None;
        };
        let replacement_kind = value_cell_kind_from_assoc_key_kind(*source_key_kind);
        if let Some(existing) = out_key_values.iter().position(|existing| existing == &key) {
            out_values[existing] = replacement;
            out_value_kinds[existing] = replacement_kind;
        } else {
            out_key_kinds.push(assoc_key_kind_for_value(&key));
            out_key_values.push(key);
            out_values.push(replacement);
            out_value_kinds.push(replacement_kind);
        }
    }
    Some(ExactUniqueMetadata {
        values: out_values,
        value_kinds: out_value_kinds,
        key_kinds: out_key_kinds,
        key_values: out_key_values,
    })
}

pub(super) fn exact_unique_indexed_value_metadata(
    source: &str,
    sort_mode: ArrayUniqueSortMode,
    module: &WasmModule,
) -> Option<ExactUniqueMetadata> {
    let values = module.array_value_constants(source)?;
    let value_kinds = module.array_value_cell_kinds(source)?;
    if values.len() != value_kinds.len() {
        return None;
    }
    let mut seen: Vec<ConstantValue> = Vec::new();
    let mut out_values = Vec::new();
    let mut out_kinds = Vec::new();
    let mut key_values = Vec::new();
    for (index, (value, kind)) in values.iter().zip(value_kinds.iter()).enumerate() {
        if seen
            .iter()
            .any(|existing| exact_unique_values_equal(existing, value, sort_mode))
        {
            continue;
        }
        seen.push(value.clone());
        out_values.push(value.clone());
        out_kinds.push(*kind);
        key_values.push(AssocKeyValue::Int(i64::try_from(index).ok()?));
    }
    let key_kinds = vec![AssocKeyKind::Int; out_values.len()];
    Some(ExactUniqueMetadata {
        values: out_values,
        value_kinds: out_kinds,
        key_kinds,
        key_values,
    })
}

pub(super) fn exact_unique_assoc_value_metadata(
    source: &str,
    sort_mode: ArrayUniqueSortMode,
    module: &WasmModule,
) -> Option<ExactUniqueMetadata> {
    let values = module.array_value_constants(source)?;
    let value_kinds = module.array_value_cell_kinds(source)?;
    let key_kinds = module.array_key_kinds(source)?;
    let key_values = module.array_key_values(source)?;
    if values.len() != value_kinds.len() || values.len() != key_kinds.len() || values.len() != key_values.len() {
        return None;
    }
    let mut seen: Vec<ConstantValue> = Vec::new();
    let mut out_values = Vec::new();
    let mut out_value_kinds = Vec::new();
    let mut out_key_kinds = Vec::new();
    let mut out_key_values = Vec::new();
    for (((value, kind), key_kind), key_value) in values
        .iter()
        .zip(value_kinds.iter())
        .zip(key_kinds.iter())
        .zip(key_values.iter())
    {
        if seen
            .iter()
            .any(|existing| exact_unique_values_equal(existing, value, sort_mode))
        {
            continue;
        }
        seen.push(value.clone());
        out_values.push(value.clone());
        out_value_kinds.push(*kind);
        out_key_kinds.push(*key_kind);
        out_key_values.push(key_value.clone());
    }
    Some(ExactUniqueMetadata {
        values: out_values,
        value_kinds: out_value_kinds,
        key_kinds: out_key_kinds,
        key_values: out_key_values,
    })
}

fn exact_unique_values_equal(
    left: &ConstantValue,
    right: &ConstantValue,
    sort_mode: ArrayUniqueSortMode,
) -> bool {
    match sort_mode {
        ArrayUniqueSortMode::String => {
            constant_value_cell_compare_string(left) == constant_value_cell_compare_string(right)
        }
        ArrayUniqueSortMode::Numeric => exact_unique_numeric_value(left) == exact_unique_numeric_value(right),
        ArrayUniqueSortMode::Regular => compare_loose_static_scalars(left, &BinOp::Eq, right),
    }
}

fn exact_unique_numeric_value(value: &ConstantValue) -> f64 {
    match value {
        ConstantValue::Int(value) => *value as f64,
        ConstantValue::Float(value) => *value,
        ConstantValue::Bool(value) => i32::from(*value) as f64,
        ConstantValue::Null => 0.0,
        ConstantValue::Str(value) => php_leading_numeric_string_value(value)
            .map(|(value, _)| value)
            .unwrap_or(0.0),
    }
}

pub(super) fn exact_indexed_value_set_metadata(
    source: &str,
    function_name: &str,
    args: &[Expr],
    module: &WasmModule,
) -> Option<ExactUniqueMetadata> {
    let values = module.array_value_constants(source)?;
    let value_kinds = module.array_value_cell_kinds(source)?;
    if values.len() != value_kinds.len() {
        return None;
    }
    let compare_sets = exact_value_set_compare_strings(function_name, args, module)?;
    let keep_matching = function_name.eq_ignore_ascii_case("array_intersect");
    let mut out_values = Vec::new();
    let mut out_kinds = Vec::new();
    let mut key_values = Vec::new();
    for (index, (value, kind)) in values.iter().zip(value_kinds.iter()).enumerate() {
        let compare = constant_value_cell_compare_string(value);
        let matched = if keep_matching {
            compare_sets.iter().all(|set| set.iter().any(|candidate| candidate == &compare))
        } else {
            compare_sets.iter().any(|set| set.iter().any(|candidate| candidate == &compare))
        };
        if matched == keep_matching {
            out_values.push(value.clone());
            out_kinds.push(*kind);
            key_values.push(AssocKeyValue::Int(i64::try_from(index).ok()?));
        }
    }
    let key_kinds = vec![AssocKeyKind::Int; out_values.len()];
    Some(ExactUniqueMetadata {
        values: out_values,
        value_kinds: out_kinds,
        key_kinds,
        key_values,
    })
}

pub(super) fn exact_assoc_value_set_metadata(
    source: &str,
    function_name: &str,
    args: &[Expr],
    module: &WasmModule,
) -> Option<ExactUniqueMetadata> {
    let values = module.array_value_constants(source)?;
    let value_kinds = module.array_value_cell_kinds(source)?;
    let key_kinds = module.array_key_kinds(source)?;
    let key_values = module.array_key_values(source)?;
    if values.len() != value_kinds.len() || values.len() != key_kinds.len() || values.len() != key_values.len() {
        return None;
    }
    let compare_sets = exact_value_set_compare_strings(function_name, args, module)?;
    let keep_matching = function_name.eq_ignore_ascii_case("array_intersect");
    let mut out_values = Vec::new();
    let mut out_value_kinds = Vec::new();
    let mut out_key_kinds = Vec::new();
    let mut out_key_values = Vec::new();
    for (((value, kind), key_kind), key_value) in values
        .iter()
        .zip(value_kinds.iter())
        .zip(key_kinds.iter())
        .zip(key_values.iter())
    {
        let compare = constant_value_cell_compare_string(value);
        let matched = if keep_matching {
            compare_sets.iter().all(|set| set.iter().any(|candidate| candidate == &compare))
        } else {
            compare_sets.iter().any(|set| set.iter().any(|candidate| candidate == &compare))
        };
        if matched == keep_matching {
            out_values.push(value.clone());
            out_value_kinds.push(*kind);
            out_key_kinds.push(*key_kind);
            out_key_values.push(key_value.clone());
        }
    }
    Some(ExactUniqueMetadata {
        values: out_values,
        value_kinds: out_value_kinds,
        key_kinds: out_key_kinds,
        key_values: out_key_values,
    })
}

pub(super) fn exact_value_set_compare_strings(
    function_name: &str,
    args: &[Expr],
    module: &WasmModule,
) -> Option<Vec<Vec<String>>> {
    if args.len() < 2
        || !(function_name.eq_ignore_ascii_case("array_diff")
            || function_name.eq_ignore_ascii_case("array_intersect"))
    {
        return None;
    }
    args[1..]
        .iter()
        .map(|arg| exact_value_set_arg_compare_strings(arg, module))
        .collect()
}

pub(super) fn exact_value_set_arg_compare_strings(arg: &Expr, module: &WasmModule) -> Option<Vec<String>> {
    match &arg.kind {
        ExprKind::Variable(name) => module
            .array_value_constants(name)?
            .iter()
            .map(|value| Some(constant_value_cell_compare_string(value)))
            .collect(),
        ExprKind::ArrayLiteral(items) => items
            .iter()
            .map(|item| static_value_cell_compare_string(item, module))
            .collect(),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .map(|(_, value)| static_value_cell_compare_string(value, module))
            .collect(),
        _ => None,
    }
}

pub(super) fn apply_exact_unique_metadata(name: &str, metadata: ExactUniqueMetadata, module: &mut WasmModule) {
    let len = metadata.values.len();
    module.set_array_length(name, len);
    module.set_array_value_cell_kinds(name, Some(metadata.value_kinds));
    module.set_array_key_kinds(name, Some(metadata.key_kinds));
    module.set_array_key_values(name, Some(metadata.key_values));
    module.set_array_value_constants(name, Some(metadata.values));
}

pub(super) fn emit_exact_assoc_metadata_assign(
    name: &str,
    metadata: ExactUniqueMetadata,
    span: Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let mut items = Vec::new();
    for ((key_kind, key_value), value) in metadata
        .key_kinds
        .into_iter()
        .zip(metadata.key_values.into_iter())
        .zip(metadata.values.into_iter())
    {
        let key = expr_from_assoc_key(key_kind, key_value, span)?;
        let value = expr_from_constant_value(value, span);
        items.push((key, value));
    }
    emit_assoc_array_items_assign(name, &items, module)
}

fn expr_from_assoc_key(
    kind: AssocKeyKind,
    value: AssocKeyValue,
    span: Span,
) -> Result<Expr, CompileError> {
    match (kind, value) {
        (AssocKeyKind::Int, AssocKeyValue::Int(value)) => Ok(Expr::new(ExprKind::IntLiteral(value), span)),
        (AssocKeyKind::Str, AssocKeyValue::Str(value)) => Ok(Expr::new(ExprKind::StringLiteral(value), span)),
        _ => Err(CompileError::new(
            span,
            "wasm32-web exact associative array metadata has inconsistent key shape",
        )),
    }
}

fn expr_from_constant_value(value: ConstantValue, span: Span) -> Expr {
    match value {
        ConstantValue::Int(value) => Expr::new(ExprKind::IntLiteral(value), span),
        ConstantValue::Float(value) => Expr::new(ExprKind::FloatLiteral(value), span),
        ConstantValue::Bool(value) => Expr::new(ExprKind::BoolLiteral(value), span),
        ConstantValue::Str(value) => Expr::new(ExprKind::StringLiteral(value), span),
        ConstantValue::Null => Expr::new(ExprKind::Null, span),
    }
}

pub(super) fn flip_key_from_constant_value(value: &ConstantValue) -> Option<AssocKeyValue> {
    match value {
        ConstantValue::Int(value) => Some(AssocKeyValue::Int(*value)),
        ConstantValue::Str(value) => literal_php_array_int_key(value)
            .map(AssocKeyValue::Int)
            .or_else(|| Some(AssocKeyValue::Str(value.clone()))),
        ConstantValue::Float(_) | ConstantValue::Bool(_) | ConstantValue::Null => None,
    }
}

pub(super) fn value_cell_kind_from_assoc_key_kind(kind: AssocKeyKind) -> ValueCellKind {
    match kind {
        AssocKeyKind::Int => ValueCellKind::Int,
        AssocKeyKind::Str => ValueCellKind::Str,
    }
}

pub(super) fn constant_value_from_assoc_key(
    kind: AssocKeyKind,
    value: &AssocKeyValue,
) -> Option<ConstantValue> {
    match (kind, value) {
        (AssocKeyKind::Int, AssocKeyValue::Int(value)) => Some(ConstantValue::Int(*value)),
        (AssocKeyKind::Str, AssocKeyValue::Str(value)) => Some(ConstantValue::Str(value.clone())),
        _ => None,
    }
}

pub(super) fn constant_value_cell_compare_string(value: &ConstantValue) -> String {
    match value {
        ConstantValue::Int(value) => value.to_string(),
        ConstantValue::Float(value) => value.to_string(),
        ConstantValue::Bool(true) => "1".to_string(),
        ConstantValue::Bool(false) | ConstantValue::Null => String::new(),
        ConstantValue::Str(value) => value.clone(),
    }
}
