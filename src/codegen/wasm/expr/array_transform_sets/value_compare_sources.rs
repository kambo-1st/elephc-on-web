//! Purpose:
//! Plans static and runtime compare sources for wasm32-web array set operations.
//! Keeps mask validation and source classification separate from comparison emitters.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets::value_string`.
//! - Sibling static metadata and indexed-int transform helpers through the parent re-export.
//!
//! Key details:
//! - Source classification decides which PHP string-representation comparison path codegen emits later.

use super::*;

pub(in crate::codegen::wasm::expr) fn static_int_compare_sets(
    function_name: &str,
    args: &[Expr],
) -> Result<Vec<Vec<i64>>, CompileError> {
    let mut compare_sets = Vec::new();
    for arg in &args[1..] {
        let ExprKind::ArrayLiteral(items) = &arg.kind else {
            return Err(CompileError::new(
                arg.span,
                &format!("wasm32-web {function_name}() currently requires direct indexed integer literal masks"),
            ));
        };
        let mut values = Vec::new();
        for item in items {
            let Some(value) = static_int_value(item) else {
                return Err(CompileError::new(
                    item.span,
                    &format!("wasm32-web {function_name}() currently requires static integer literal mask values"),
                ));
            };
            values.push(value);
        }
        compare_sets.push(values);
    }
    Ok(compare_sets)
}

pub(in crate::codegen::wasm::expr) enum ValueStringCompareSource {
    Static(Vec<String>),
    RuntimeCompactInt { ptr: String, len: String },
    Runtime { ptr: String, len: String },
    RuntimeScalar { ptr: String, len: String },
    RuntimeAny { ptr: String, len: String },
}

pub(in crate::codegen::wasm::expr) fn prepare_value_string_compare_sources(
    function_name: &str,
    args: &[Expr],
    source_all_strings: bool,
    module: &mut WasmModule,
) -> Result<Vec<ValueStringCompareSource>, CompileError> {
    let mut compare_sets = Vec::new();
    for arg in &args[1..] {
        match &arg.kind {
            ExprKind::ArrayLiteral(items) => {
                let mut values = Vec::new();
                for item in items {
                    let Some(value) = static_value_cell_compare_string(item, module) else {
                        return Err(CompileError::new(
                            item.span,
                            &format!("wasm32-web {function_name}() currently requires static scalar literal mask values"),
                        ));
                    };
                    values.push(value);
                }
                compare_sets.push(ValueStringCompareSource::Static(values));
            }
            ExprKind::Variable(source)
                if module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::Value =>
            {
                let Some(kinds) = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec()) else {
                    return Err(CompileError::new(
                        arg.span,
                        &format!("wasm32-web {function_name}() assigned value-cell masks require known element types"),
                    ));
                };
                if kinds.iter().any(|kind| matches!(kind, ValueCellKind::Array)) {
                    return Err(CompileError::new(
                        arg.span,
                        &format!("wasm32-web {function_name}() assigned value-cell masks currently reject array values"),
                    ));
                }
                let ptr = preserve_array_ptr(source, function_name, module);
                let len = module.next_label("value_array_set_compare_len");
                module.declare_i32_local(len.trim_start_matches('$').to_string());
                module.body().line(&format!("local.get ${}_len", source));
                module.body().line(&format!("local.set {}", len));
                let mask_all_strings = kinds.iter().all(|kind| matches!(kind, ValueCellKind::Str));
                let mask_all_scalars = kinds.iter().all(|kind| !matches!(kind, ValueCellKind::Str));
                if source_all_strings && mask_all_strings {
                    compare_sets.push(ValueStringCompareSource::Runtime { ptr, len });
                } else if !source_all_strings && mask_all_scalars {
                    compare_sets.push(ValueStringCompareSource::RuntimeScalar { ptr, len });
                } else {
                    compare_sets.push(ValueStringCompareSource::RuntimeAny { ptr, len });
                }
            }
            ExprKind::Variable(source)
                if module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::CompactInt =>
            {
                let ptr = preserve_array_ptr(source, function_name, module);
                let len = module.next_label("value_array_set_compact_compare_len");
                module.declare_i32_local(len.trim_start_matches('$').to_string());
                module.body().line(&format!("local.get ${}_len", source));
                module.body().line(&format!("local.set {}", len));
                compare_sets.push(ValueStringCompareSource::RuntimeCompactInt { ptr, len });
            }
            ExprKind::FunctionCall { .. } if expression_has_array_type(arg, module) => {
                let source = materialize_array_map_multi_source(arg, "value_array_set_compare", module)?;
                match module.array_layout(&source) {
                    ArrayLayout::Value => {
                        let ptr = preserve_array_ptr(&source, function_name, module);
                        let len = module.next_label("value_array_set_compare_len");
                        module.declare_i32_local(len.trim_start_matches('$').to_string());
                        module.body().line(&format!("local.get ${}_len", source));
                        module.body().line(&format!("local.set {}", len));
                        let mask_all_strings = module
                            .array_value_cell_kinds(&source)
                            .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Str)))
                            || module.array_runtime_value_cell_kind(&source) == Some(ValueCellKind::Str);
                        let mask_all_scalars = module
                            .array_value_cell_kinds(&source)
                            .is_some_and(|kinds| kinds.iter().all(|kind| !matches!(kind, ValueCellKind::Str)))
                            || module
                                .array_runtime_value_cell_kind(&source)
                                .is_some_and(|kind| !matches!(kind, ValueCellKind::Str));
                        if source_all_strings && mask_all_strings {
                            compare_sets.push(ValueStringCompareSource::Runtime { ptr, len });
                        } else if !source_all_strings && mask_all_scalars {
                            compare_sets.push(ValueStringCompareSource::RuntimeScalar { ptr, len });
                        } else {
                            compare_sets.push(ValueStringCompareSource::RuntimeAny { ptr, len });
                        }
                    }
                    ArrayLayout::CompactInt => {
                        let ptr = preserve_array_ptr(&source, function_name, module);
                        let len = module.next_label("value_array_set_compact_compare_len");
                        module.declare_i32_local(len.trim_start_matches('$').to_string());
                        module.body().line(&format!("local.get ${}_len", source));
                        module.body().line(&format!("local.set {}", len));
                        compare_sets.push(ValueStringCompareSource::RuntimeCompactInt { ptr, len });
                    }
                    ArrayLayout::Assoc => {
                        return Err(CompileError::new(
                            arg.span,
                            &format!("wasm32-web {function_name}() value-array masks currently require indexed values"),
                        ));
                    }
                }
            }
            _ => {
                return Err(CompileError::new(
                    arg.span,
                    &format!(
                        "wasm32-web {function_name}() currently requires direct indexed string literal masks or assigned string value-cell masks"
                    ),
                ));
            }
        }
    }
    Ok(compare_sets)
}

pub(in crate::codegen::wasm::expr) fn static_value_cell_compare_string(expr: &Expr, module: &WasmModule) -> Option<String> {
    match &expr.kind {
        ExprKind::Null => Some(String::new()),
        ExprKind::BoolLiteral(value) => Some(if *value { "1".to_string() } else { String::new() }),
        _ if static_string_value(expr, module).is_some() => static_string_value(expr, module),
        _ => static_int_value(expr).map(|value| value.to_string()),
    }
}
