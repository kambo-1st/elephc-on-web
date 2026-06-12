//! Purpose:
//! Owns wasm32-web static array transform validation and simple static item materialization.
//! Keeps argument checks and static metadata helpers separate from runtime transform loops.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets`.
//! - array helpers that materialize static indexed items.
//!
//! Key details:
//! - Preserves PHP-visible validation for array_reverse(), array_unique(), and static key/value transforms.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ArrayUniqueSortMode {
    String,
    Regular,
    Numeric,
}

pub(super) fn validate_array_reverse_preserve_keys_arg(
    args: &[Expr],
    span: Span,
    module: &WasmModule,
) -> Result<(), CompileError> {
    if args.len() < 1 || args.len() > 2 {
        return Err(CompileError::new(
            span,
            "wasm32-web array_reverse() expects one or two arguments",
        ));
    }
    let _ = array_reverse_preserve_keys_arg(args, module)?;
    Ok(())
}

pub(super) fn array_reverse_preserve_keys_arg(
    args: &[Expr],
    module: &WasmModule,
) -> Result<bool, CompileError> {
    let Some(preserve_keys) = args.get(1) else {
        return Ok(false);
    };
    let Some(value) = static_or_const_or_i32_bool_value(preserve_keys, module) else {
        return Err(CompileError::new(
            preserve_keys.span,
            "wasm32-web array_reverse() currently requires a static boolean preserve_keys argument",
        ));
    };
    Ok(value)
}

pub(super) fn validate_array_unique_sort_flag(
    args: &[Expr],
    span: Span,
    module: &WasmModule,
) -> Result<(), CompileError> {
    let _ = array_unique_sort_mode(args, span, module)?;
    Ok(())
}

pub(super) fn array_unique_sort_mode(
    args: &[Expr],
    span: Span,
    module: &WasmModule,
) -> Result<ArrayUniqueSortMode, CompileError> {
    if args.len() < 1 || args.len() > 2 {
        return Err(CompileError::new(
            span,
            "wasm32-web array_unique() expects one or two arguments",
        ));
    }
    let Some(sort_flag) = args.get(1) else {
        return Ok(ArrayUniqueSortMode::String);
    };
    let Some(value) = static_or_const_or_i64_local_value(sort_flag, module) else {
        return Err(CompileError::new(
            sort_flag.span,
            "wasm32-web array_unique() currently requires a static sort flag",
        ));
    };
    match value {
        0 | 3 => Ok(ArrayUniqueSortMode::Regular),
        1 => Ok(ArrayUniqueSortMode::Numeric),
        2 => Ok(ArrayUniqueSortMode::String),
        _ => Err(CompileError::new(
            sort_flag.span,
            "wasm32-web array_unique() currently supports SORT_STRING and static-literal SORT_REGULAR/SORT_NUMERIC modes plus PHP flag 3",
        )),
    }
}

pub(super) fn transformed_static_array_items(
    expr: &Expr,
    function_name: &str,
    items: &[Expr],
) -> Result<Vec<Expr>, CompileError> {
    match function_name.to_ascii_lowercase().as_str() {
        "array_values" => Ok(items.to_vec()),
        "array_reverse" => Ok(items.iter().cloned().rev().collect()),
        "array_keys" => Ok((0..items.len())
            .map(|index| Expr::new(ExprKind::IntLiteral(index as i64), expr.span))
            .collect()),
        _ => Err(array_unsupported(expr)),
    }
}

pub(super) fn assoc_key_constants_for_source(source: &str, module: &WasmModule) -> Option<Vec<ConstantValue>> {
    let kinds = module.array_key_kinds(source)?;
    let values = module.array_key_values(source)?;
    kinds
        .iter()
        .zip(values.iter())
        .map(|(kind, value)| match (kind, value) {
            (AssocKeyKind::Int, AssocKeyValue::Int(value)) => Some(ConstantValue::Int(*value)),
            (AssocKeyKind::Str, AssocKeyValue::Str(value)) => Some(ConstantValue::Str(value.clone())),
            _ => None,
        })
        .collect()
}

pub(super) fn emit_static_array_items_assign(
    name: &str,
    items: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.set_array_length(name, items.len());
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line(&format!("local.set ${}_len", name));
    for (index, item) in items.iter().enumerate() {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        require_int(item, module)?;
        module.body().line("i64.store");
    }
    Ok(())
}
