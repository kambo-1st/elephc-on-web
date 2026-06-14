//! Purpose:
//! Lowers wasm32-web array_slice helpers for indexed, value-cell, and associative arrays.
//! Keeps slice bounds, preserve-key handling, and sliced metadata out of expr.rs.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` array assignment lowering.
//!
//! Key details:
//! - Helpers preserve PHP reindexing behavior, static metadata, and runtime bounds handling.

use super::*;

pub(super) fn emit_indexed_array_slice_assign(
    name: &str,
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    validate_array_slice_reindex_flag(args, expr.span, module)?;
    let preserve_keys = array_slice_preserve_keys_arg(args, module)?;
    if args.len() < 2 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_slice() expects two to four arguments",
        ));
    }
    let Some(offset) = static_or_const_or_i64_local_value(&args[1], module) else {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web array_slice() requires a static integer offset",
        ));
    };
    let length = array_slice_length_arg(args, module)?;
    match &args[0].kind {
        ExprKind::ArrayLiteral(items) => {
            let (start, take) = static_array_slice_bounds(items.len(), offset, length);
            let items = &items[start..start + take];
            if preserve_keys {
                let items = sliced_indexed_preserve_key_items(expr, items, start);
                return emit_assoc_array_items_assign(name, &items, module);
            }
            if array_literal_needs_value_cells(items) {
                emit_value_array_items_assign(name, items, module)
            } else {
                module.set_array_layout(name, ArrayLayout::CompactInt);
                emit_static_array_items_assign(name, items, module)
            }
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            let (start, take) = static_array_slice_bounds(items.len(), offset, length);
            let items = &items[start..start + take];
            let items = if preserve_keys {
                sliced_assoc_preserve_key_items(args[0].span, items)?
            } else {
                sliced_assoc_default_items(args[0].span, items)?
            };
            emit_assoc_array_items_assign(name, &items, module)
        }
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args: case_args,
        } if method.eq_ignore_ascii_case("cases")
            && case_args.is_empty()
            && module
                .class_name_for_receiver(receiver)
                .and_then(|class_name| module.enum_case_names(&class_name))
                .is_some() =>
        {
            let temp = module
                .next_label("array_slice_enum_cases")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, &args[0], module)?;
            if preserve_keys {
                let Some(len) = module.array_length(&temp) else {
                    return Err(CompileError::new(
                        args[0].span,
                        "wasm32-web array_slice(..., true) requires enum case metadata",
                    ));
                };
                let (start, take) = static_array_slice_bounds(len, offset, length);
                return emit_known_indexed_array_slice_preserve_keys_assign(
                    name,
                    &temp,
                    ArrayLayout::Value,
                    start,
                    take,
                    module,
                );
            }
            emit_dynamic_value_array_slice_assign(
                name,
                &Expr::new(ExprKind::Variable(temp), args[0].span),
                offset,
                length,
                module,
            )
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Mixed)
                && module.mixed_value_cell_kind(source).is_none() =>
        {
            emit_unknown_mixed_array_slice_assign(
                name,
                source,
                args[0].span,
                offset,
                length,
                preserve_keys,
                module,
            )
        }
        ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
            if module.array_layout(source) == ArrayLayout::Assoc {
                return emit_assoc_array_slice_assign(
                    name,
                    source,
                    args[0].span,
                    offset,
                    length,
                    preserve_keys,
                    module,
                );
            }
            if module.array_layout(source) == ArrayLayout::Value {
                if preserve_keys {
                    let Some(len) = module.array_length(source) else {
                        return Err(CompileError::new(
                            args[0].span,
                            "wasm32-web array_slice(..., true) requires a known indexed array length",
                        ));
                    };
                    let (start, take) = static_array_slice_bounds(len, offset, length);
                    return emit_known_indexed_array_slice_preserve_keys_assign(
                        name,
                        source,
                        ArrayLayout::Value,
                        start,
                        take,
                        module,
                    );
                }
                return emit_dynamic_value_array_slice_assign(
                    name,
                    &args[0],
                    offset,
                    length,
                    module,
                );
            }
            let Some(len) = module.array_length(source) else {
                return emit_dynamic_indexed_array_slice_assign(
                    name,
                    &args[0],
                    offset,
                    length,
                    module,
                );
            };
            let (start, take) = static_array_slice_bounds(len, offset, length);
            if preserve_keys {
                return emit_known_indexed_array_slice_preserve_keys_assign(
                    name,
                    source,
                    ArrayLayout::CompactInt,
                    start,
                    take,
                    module,
                );
            }
            let source_ptr = preserve_array_ptr(source, "array_slice", module);
            emit_array_alloc_prelude(name, take, module);
            for index in 0..take {
                emit_array_store_load(name, index, &source_ptr, start + index, module);
            }
            Ok(())
        }
        ExprKind::ArrayAccess { .. } if nested_array_metadata_for_access_expr(&args[0], module).is_some() => {
            let temp = materialize_nested_array_slice_source(&args[0], module)?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                emit_assoc_array_slice_assign(
                    name,
                    &temp,
                    args[0].span,
                    offset,
                    length,
                    preserve_keys,
                    module,
                )
            } else if module.array_layout(&temp) == ArrayLayout::Value {
                if preserve_keys {
                    let Some(len) = module.array_length(&temp) else {
                        return Err(CompileError::new(
                            args[0].span,
                            "wasm32-web array_slice(..., true) requires a known indexed array length",
                        ));
                    };
                    let (start, take) = static_array_slice_bounds(len, offset, length);
                    return emit_known_indexed_array_slice_preserve_keys_assign(
                        name,
                        &temp,
                        ArrayLayout::Value,
                        start,
                        take,
                        module,
                    );
                }
                emit_dynamic_value_array_slice_assign(
                    name,
                    &Expr::new(ExprKind::Variable(temp), args[0].span),
                    offset,
                    length,
                    module,
                )
            } else {
                if preserve_keys {
                    let Some(len) = module.array_length(&temp) else {
                        return Err(CompileError::new(
                            args[0].span,
                            "wasm32-web array_slice(..., true) requires a known indexed array length",
                        ));
                    };
                    let (start, take) = static_array_slice_bounds(len, offset, length);
                    return emit_known_indexed_array_slice_preserve_keys_assign(
                        name,
                        &temp,
                        ArrayLayout::CompactInt,
                        start,
                        take,
                        module,
                    );
                }
                emit_dynamic_indexed_array_slice_assign(
                    name,
                    &Expr::new(ExprKind::Variable(temp), args[0].span),
                    offset,
                    length,
                    module,
                )
            }
        }
        ExprKind::FunctionCall { name: function_name, .. }
            if module.has_function(function_name)
                && module.function_return_kind(function_name) == Some(ValueKind::Array)
                && module.function_array_return_layout(function_name) == ArrayLayout::Value =>
        {
            if preserve_keys {
                let temp = materialize_array_map_multi_source(&args[0], "array_slice_source", module)?;
                let Some(len) = module.array_length(&temp) else {
                    return Err(CompileError::new(
                        args[0].span,
                        "wasm32-web array_slice(..., true) requires a known indexed array length",
                    ));
                };
                let (start, take) = static_array_slice_bounds(len, offset, length);
                return emit_known_indexed_array_slice_preserve_keys_assign(
                    name,
                    &temp,
                    ArrayLayout::Value,
                    start,
                    take,
                    module,
                );
            }
            emit_dynamic_value_array_slice_assign(name, &args[0], offset, length, module)
        }
        ExprKind::MethodCall { object, method, .. }
        | ExprKind::NullsafeMethodCall { object, method, .. }
            if method_call_array_return_metadata(object, method, module).is_some() =>
        {
            let temp = materialize_method_array_slice_source(&args[0], object, method, module)?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                emit_assoc_array_slice_assign(
                    name,
                    &temp,
                    args[0].span,
                    offset,
                    length,
                    preserve_keys,
                    module,
                )
            } else if module.array_layout(&temp) == ArrayLayout::Value {
                if preserve_keys {
                    let Some(len) = module.array_length(&temp) else {
                        return Err(CompileError::new(
                            args[0].span,
                            "wasm32-web array_slice(..., true) requires a known indexed array length",
                        ));
                    };
                    let (start, take) = static_array_slice_bounds(len, offset, length);
                    return emit_known_indexed_array_slice_preserve_keys_assign(
                        name,
                        &temp,
                        ArrayLayout::Value,
                        start,
                        take,
                        module,
                    );
                }
                emit_dynamic_value_array_slice_assign(
                    name,
                    &Expr::new(ExprKind::Variable(temp), args[0].span),
                    offset,
                    length,
                    module,
                )
            } else {
                if preserve_keys {
                    let Some(len) = module.array_length(&temp) else {
                        return Err(CompileError::new(
                            args[0].span,
                            "wasm32-web array_slice(..., true) requires a known indexed array length",
                        ));
                    };
                    let (start, take) = static_array_slice_bounds(len, offset, length);
                    return emit_known_indexed_array_slice_preserve_keys_assign(
                        name,
                        &temp,
                        ArrayLayout::CompactInt,
                        start,
                        take,
                        module,
                    );
                }
                emit_dynamic_indexed_array_slice_assign(
                    name,
                    &Expr::new(ExprKind::Variable(temp), args[0].span),
                    offset,
                    length,
                    module,
                )
            }
        }
        ExprKind::StaticMethodCall { receiver, method, .. }
            if static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            let temp = materialize_static_method_array_slice_source(&args[0], receiver, method, module)?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                emit_assoc_array_slice_assign(
                    name,
                    &temp,
                    args[0].span,
                    offset,
                    length,
                    preserve_keys,
                    module,
                )
            } else if module.array_layout(&temp) == ArrayLayout::Value {
                if preserve_keys {
                    let Some(len) = module.array_length(&temp) else {
                        return Err(CompileError::new(
                            args[0].span,
                            "wasm32-web array_slice(..., true) requires a known indexed array length",
                        ));
                    };
                    let (start, take) = static_array_slice_bounds(len, offset, length);
                    return emit_known_indexed_array_slice_preserve_keys_assign(
                        name,
                        &temp,
                        ArrayLayout::Value,
                        start,
                        take,
                        module,
                    );
                }
                emit_dynamic_value_array_slice_assign(
                    name,
                    &Expr::new(ExprKind::Variable(temp), args[0].span),
                    offset,
                    length,
                    module,
                )
            } else {
                if preserve_keys {
                    let Some(len) = module.array_length(&temp) else {
                        return Err(CompileError::new(
                            args[0].span,
                            "wasm32-web array_slice(..., true) requires a known indexed array length",
                        ));
                    };
                    let (start, take) = static_array_slice_bounds(len, offset, length);
                    return emit_known_indexed_array_slice_preserve_keys_assign(
                        name,
                        &temp,
                        ArrayLayout::CompactInt,
                        start,
                        take,
                        module,
                    );
                }
                emit_dynamic_indexed_array_slice_assign(
                    name,
                    &Expr::new(ExprKind::Variable(temp), args[0].span),
                    offset,
                    length,
                    module,
                )
            }
        }
        ExprKind::DynamicStaticMethodCall { receiver, method, .. }
            if dynamic_static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            let temp =
                materialize_dynamic_static_method_array_slice_source(&args[0], receiver, method, module)?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                emit_assoc_array_slice_assign(
                    name,
                    &temp,
                    args[0].span,
                    offset,
                    length,
                    preserve_keys,
                    module,
                )
            } else if module.array_layout(&temp) == ArrayLayout::Value {
                if preserve_keys {
                    let Some(len) = module.array_length(&temp) else {
                        return Err(CompileError::new(
                            args[0].span,
                            "wasm32-web array_slice(..., true) requires a known indexed array length",
                        ));
                    };
                    let (start, take) = static_array_slice_bounds(len, offset, length);
                    return emit_known_indexed_array_slice_preserve_keys_assign(
                        name,
                        &temp,
                        ArrayLayout::Value,
                        start,
                        take,
                        module,
                    );
                }
                emit_dynamic_value_array_slice_assign(
                    name,
                    &Expr::new(ExprKind::Variable(temp), args[0].span),
                    offset,
                    length,
                    module,
                )
            } else {
                if preserve_keys {
                    let Some(len) = module.array_length(&temp) else {
                        return Err(CompileError::new(
                            args[0].span,
                            "wasm32-web array_slice(..., true) requires a known indexed array length",
                        ));
                    };
                    let (start, take) = static_array_slice_bounds(len, offset, length);
                    return emit_known_indexed_array_slice_preserve_keys_assign(
                        name,
                        &temp,
                        ArrayLayout::CompactInt,
                        start,
                        take,
                        module,
                    );
                }
                emit_dynamic_indexed_array_slice_assign(
                    name,
                    &Expr::new(ExprKind::Variable(temp), args[0].span),
                    offset,
                    length,
                    module,
                )
            }
        }
        ExprKind::FunctionCall { .. } | ExprKind::ExprCall { .. }
            if expression_has_array_type(&args[0], module) =>
        {
            let temp = materialize_array_map_multi_source(&args[0], "array_slice_source", module)?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                emit_assoc_array_slice_assign(name, &temp, args[0].span, offset, length, preserve_keys, module)
            } else if module.array_layout(&temp) == ArrayLayout::Value {
                if preserve_keys {
                    let Some(len) = module.array_length(&temp) else {
                        return Err(CompileError::new(
                            args[0].span,
                            "wasm32-web array_slice(..., true) requires a known indexed array length",
                        ));
                    };
                    let (start, take) = static_array_slice_bounds(len, offset, length);
                    return emit_known_indexed_array_slice_preserve_keys_assign(
                        name,
                        &temp,
                        ArrayLayout::Value,
                        start,
                        take,
                        module,
                    );
                }
                emit_dynamic_value_array_slice_assign(
                    name,
                    &Expr::new(ExprKind::Variable(temp), args[0].span),
                    offset,
                    length,
                    module,
                )
            } else {
                if preserve_keys {
                    let Some(len) = module.array_length(&temp) else {
                        return Err(CompileError::new(
                            args[0].span,
                            "wasm32-web array_slice(..., true) requires a known indexed array length",
                        ));
                    };
                    let (start, take) = static_array_slice_bounds(len, offset, length);
                    return emit_known_indexed_array_slice_preserve_keys_assign(
                        name,
                        &temp,
                        ArrayLayout::CompactInt,
                        start,
                        take,
                        module,
                    );
                }
                emit_dynamic_indexed_array_slice_assign(
                    name,
                    &Expr::new(ExprKind::Variable(temp), args[0].span),
                    offset,
                    length,
                    module,
                )
            }
        }
        _ if expression_is_arrayy(&args[0], module) => {
            if preserve_keys {
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web array_slice(..., true) requires a known indexed array length",
                ));
            }
            emit_dynamic_indexed_array_slice_assign(name, &args[0], offset, length, module)
        }
        _ => Err(CompileError::new(
            args[0].span,
            "wasm32-web array_slice() currently supports indexed array values only",
        )),
    }
}

fn materialize_method_array_slice_source(
    source: &Expr,
    object: &Expr,
    method: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let metadata = method_call_array_return_metadata(object, method, module).ok_or_else(|| {
        CompileError::new(
            source.span,
            "wasm32-web array_slice() requires method array return metadata",
        )
    })?;
    let temp = module
        .next_label("array_slice_method_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    match emit_expr(source, module)? {
        ValueKind::Array => {
            module.body().line(&format!("local.set ${}_len", temp));
            module.body().line(&format!("local.set ${}_ptr", temp));
        }
        _ => {
            return Err(CompileError::new(
                source.span,
                "wasm32-web array_slice() expected an array-returning method value",
            ));
        }
    }
    module.set_array_layout(&temp, metadata.layout);
    if let Some(len) = metadata.len {
        module.set_array_length(&temp, len);
    }
    module.set_array_value_cell_kinds(&temp, metadata.value_kinds);
    module.set_array_runtime_value_cell_kind(&temp, metadata.runtime_value_kind);
    module.set_array_nested_value_metadata(&temp, metadata.nested_values);
    module.set_array_key_kinds(&temp, metadata.key_kinds);
    module.set_array_key_values(&temp, metadata.key_values);
    Ok(temp)
}

fn materialize_static_method_array_slice_source(
    source: &Expr,
    receiver: &StaticReceiver,
    method: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let metadata = static_method_call_array_return_metadata(receiver, method, module).ok_or_else(|| {
        CompileError::new(
            source.span,
            "wasm32-web array_slice() requires static method array return metadata",
        )
    })?;
    let temp = module
        .next_label("array_slice_static_method_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    match emit_expr(source, module)? {
        ValueKind::Array => {
            module.body().line(&format!("local.set ${}_len", temp));
            module.body().line(&format!("local.set ${}_ptr", temp));
        }
        _ => {
            return Err(CompileError::new(
                source.span,
                "wasm32-web array_slice() expected an array-returning static method value",
            ));
        }
    }
    module.set_array_layout(&temp, metadata.layout);
    if let Some(len) = metadata.len {
        module.set_array_length(&temp, len);
    }
    module.set_array_value_cell_kinds(&temp, metadata.value_kinds);
    module.set_array_runtime_value_cell_kind(&temp, metadata.runtime_value_kind);
    module.set_array_nested_value_metadata(&temp, metadata.nested_values);
    module.set_array_key_kinds(&temp, metadata.key_kinds);
    module.set_array_key_values(&temp, metadata.key_values);
    Ok(temp)
}

fn materialize_dynamic_static_method_array_slice_source(
    source: &Expr,
    receiver: &StaticReceiver,
    method: &Expr,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let metadata = dynamic_static_method_call_array_return_metadata(receiver, method, module).ok_or_else(|| {
        CompileError::new(
            source.span,
            "wasm32-web array_slice() requires dynamic static method array return metadata",
        )
    })?;
    let temp = module
        .next_label("array_slice_dynamic_static_method_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    match emit_expr(source, module)? {
        ValueKind::Array => {
            module.body().line(&format!("local.set ${}_len", temp));
            module.body().line(&format!("local.set ${}_ptr", temp));
        }
        _ => {
            return Err(CompileError::new(
                source.span,
                "wasm32-web array_slice() expected an array-returning dynamic static method value",
            ));
        }
    }
    module.set_array_layout(&temp, metadata.layout);
    if let Some(len) = metadata.len {
        module.set_array_length(&temp, len);
    }
    module.set_array_value_cell_kinds(&temp, metadata.value_kinds);
    module.set_array_runtime_value_cell_kind(&temp, metadata.runtime_value_kind);
    module.set_array_nested_value_metadata(&temp, metadata.nested_values);
    module.set_array_key_kinds(&temp, metadata.key_kinds);
    module.set_array_key_values(&temp, metadata.key_values);
    Ok(temp)
}

fn materialize_nested_array_slice_source(
    source: &Expr,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let Some(metadata) = nested_array_metadata_for_access_expr(source, module) else {
        return Err(CompileError::new(
            source.span,
            "wasm32-web array_slice() requires nested array metadata",
        ));
    };
    let temp = module
        .next_label("array_slice_nested_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    match emit_expr(source, module)? {
        ValueKind::Array => {
            module.body().line(&format!("local.set ${}_len", temp));
            module.body().line(&format!("local.set ${}_ptr", temp));
        }
        _ => {
            return Err(CompileError::new(
                source.span,
                "wasm32-web array_slice() expected a nested array value",
            ));
        }
    }
    let key_values = metadata.key_values.clone();
    module.set_array_layout(&temp, metadata.layout);
    module.set_array_length(&temp, metadata.len);
    module.set_array_value_cell_kinds(&temp, metadata.value_kinds);
    module.set_array_key_values(&temp, key_values.clone());
    module.set_array_key_kinds(
        &temp,
        key_values.map(|keys| {
            keys.into_iter()
                .map(|key| match key {
                    AssocKeyValue::Int(_) => AssocKeyKind::Int,
                    AssocKeyValue::Str(_) => AssocKeyKind::Str,
                })
                .collect()
        }),
    );
    module.set_array_php_normalized_runtime_keys(
        &temp,
        metadata.layout == ArrayLayout::Assoc && module.array_key_kinds(&temp).is_none(),
    );
    module.set_array_nested_value_metadata(&temp, metadata.nested_values);
    Ok(temp)
}

fn emit_unknown_mixed_array_slice_assign(
    name: &str,
    source: &str,
    span: Span,
    offset: i64,
    length: Option<i64>,
    preserve_keys: bool,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("unknown_mixed_array_slice_source")
        .trim_start_matches('$')
        .to_string();
    let heap_kind = module.next_label("unknown_mixed_array_slice_heap_kind");
    module.declare_array_local(temp.clone());
    module.declare_i32_local(heap_kind.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_ptr");
    module.body().line(&format!("local.set ${}_ptr", temp));
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_len_i32");
    module.body().line(&format!("local.set ${}_len", temp));
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_heap_kind");
    module.body().line(&format!("local.set {}", heap_kind));
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_INDEXED_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.set_array_layout(&temp, ArrayLayout::Value);
    module.set_array_value_cell_kinds(&temp, None);
    module.set_array_runtime_value_cell_kind(&temp, None);
    emit_unknown_mixed_indexed_array_slice_assoc_assign(
        name,
        &temp,
        span,
        offset,
        length,
        preserve_keys,
        module,
    )?;
    module.body().close("else");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.set_array_layout(&temp, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(&temp, None);
    module.set_array_key_kinds(&temp, None);
    module.set_array_php_normalized_runtime_keys(&temp, true);
    emit_dynamic_assoc_array_slice_assign(
        name,
        &temp,
        span,
        offset,
        length,
        preserve_keys,
        module,
    )?;
    module.body().close("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_unknown_mixed_indexed_array_slice_assoc_assign(
    name: &str,
    source: &str,
    span: Span,
    offset: i64,
    length: Option<i64>,
    preserve_keys: bool,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let offset = i32::try_from(offset).map_err(|_| {
        CompileError::new(span, "wasm32-web array_slice() offset is out of range")
    })?;
    let length = length
        .map(|length| {
            i32::try_from(length)
                .map_err(|_| CompileError::new(span, "wasm32-web array_slice() length is out of range"))
        })
        .transpose()?;
    let source_len = module.next_label("unknown_mixed_indexed_slice_source_len");
    let start = module.next_label("unknown_mixed_indexed_slice_start");
    let end = module.next_label("unknown_mixed_indexed_slice_end");
    let take = module.next_label("unknown_mixed_indexed_slice_take");
    let index = module.next_label("unknown_mixed_indexed_slice_index");
    let source_index = module.next_label("unknown_mixed_indexed_slice_source_index");
    let target_entry = module.next_label("unknown_mixed_indexed_slice_target_entry");
    let target_cell = module.next_label("unknown_mixed_indexed_slice_target_cell");
    let source_cell = module.next_label("unknown_mixed_indexed_slice_source_cell");
    let done_label = module.next_label("unknown_mixed_indexed_slice_done");
    let loop_label = module.next_label("unknown_mixed_indexed_slice_loop");
    for local in [
        &source_len,
        &start,
        &end,
        &take,
        &index,
        &source_index,
        &target_entry,
        &target_cell,
        &source_cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_key_kinds(name, None);
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", source_len));
    emit_dynamic_slice_bounds(offset, length, &source_len, &start, &end, &take, module);
    module.body().line(&format!("local.get {}", take));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", take));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", take));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module
        .body()
        .line(&format!("i32.const {}", WASM_ASSOC_ENTRY_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.store");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    if preserve_keys {
        module.body().line(&format!("local.get {}", source_index));
    } else {
        module.body().line(&format!("local.get {}", index));
    }
    module.body().line("i64.extend_i32_u");
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("i32.const 16");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", target_cell));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", source_cell));
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get {}", source_cell));
    module.body().line("call $__rt_value_copy");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn validate_array_slice_reindex_flag(
    args: &[Expr],
    span: Span,
    module: &WasmModule,
) -> Result<(), CompileError> {
    if args.len() > 4 {
        return Err(CompileError::new(
            span,
            "wasm32-web array_slice() expects two to four arguments",
        ));
    }
    let Some(_preserve_keys) = args.get(3) else {
        return Ok(());
    };
    let _ = array_slice_preserve_keys_arg(args, module)?;
    Ok(())
}

pub(super) fn array_slice_preserve_keys_arg(
    args: &[Expr],
    module: &WasmModule,
) -> Result<bool, CompileError> {
    let Some(preserve_keys) = args.get(3) else {
        return Ok(false);
    };
    let Some(value) = static_or_const_or_i32_bool_value(preserve_keys, module) else {
        return Err(CompileError::new(
            preserve_keys.span,
            "wasm32-web array_slice() currently requires a static boolean preserve_keys argument",
        ));
    };
    Ok(value)
}

fn array_slice_length_arg(args: &[Expr], module: &WasmModule) -> Result<Option<i64>, CompileError> {
    let Some(length_arg) = args.get(2) else {
        return Ok(None);
    };
    if matches!(length_arg.kind, ExprKind::Null) {
        return Ok(None);
    }
    let Some(length) = static_or_const_or_i64_local_value(length_arg, module) else {
        return Err(CompileError::new(
            length_arg.span,
            "wasm32-web array_slice() requires a static integer or null length",
        ));
    };
    Ok(Some(length))
}

pub(super) fn emit_assoc_array_slice_assign(
    name: &str,
    source: &str,
    span: crate::span::Span,
    offset: i64,
    length: Option<i64>,
    preserve_keys: bool,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_key_kinds(source).is_none() || module.array_length(source).is_none() {
        return emit_dynamic_assoc_array_slice_assign(name, source, span, offset, length, preserve_keys, module);
    }
    let Some(key_kinds) = module.array_key_kinds(source).map(|kinds| kinds.to_vec()) else {
        return Err(CompileError::new(
            span,
            "wasm32-web array_slice() on associative arrays requires statically-known keys",
        ));
    };
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            span,
            "wasm32-web array_slice() on associative arrays requires a known length",
        ));
    };
    let (start, take) = static_array_slice_bounds(len, offset, length);
    let sliced_key_kinds = reindexed_assoc_key_kinds(&key_kinds[start..start + take]);
    let sliced_key_values = module
        .array_key_values(source)
        .map(|values| {
            if preserve_keys {
                values[start..start + take].to_vec()
            } else {
                reindexed_assoc_key_values(&key_kinds[start..start + take], &values[start..start + take])
            }
        });
    let sliced_value_kinds = module
        .array_value_cell_kinds(source)
        .map(|kinds| kinds[start..start + take].to_vec());
    let sliced_value_constants = module
        .array_value_constants(source)
        .map(|values| values[start..start + take].to_vec());
    let sliced_nested_values = module
        .array_nested_value_metadata_items(source)
        .map(|metadata| metadata[start..start + take].to_vec());
    let source_ptr = preserve_array_ptr(source, "assoc_array_slice", module);
    module.set_array_length(name, take);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_key_kinds(name, Some(sliced_key_kinds));
    module.set_array_key_values(name, sliced_key_values);
    module.set_array_value_cell_kinds(name, sliced_value_kinds);
    module.set_array_value_constants(name, sliced_value_constants);
    module.set_array_nested_value_metadata(name, sliced_nested_values);
    module.body().line(&format!("i32.const {}", take));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", take));
    module.body().line(&format!("local.set ${}_len", name));
    let mut next_int_key = 0i64;
    for index in 0..take {
        if preserve_keys {
            for entry_offset in [0, 8] {
                module.body().line(&format!("local.get ${}_ptr", name));
                module.body().line(&format!(
                    "i32.const {}",
                    index * WASM_ASSOC_ENTRY_SIZE + entry_offset
                ));
                module.body().line("i32.add");
                module.body().line(&format!("local.get {}", source_ptr));
                module.body().line(&format!(
                    "i32.const {}",
                    (start + index) * WASM_ASSOC_ENTRY_SIZE + entry_offset
                ));
                module.body().line("i32.add");
                module.body().line("i64.load");
                module.body().line("i64.store");
            }
        } else {
            match key_kinds[start + index] {
                AssocKeyKind::Int => {
                    module.body().line(&format!("local.get ${}_ptr", name));
                    module
                        .body()
                        .line(&format!("i32.const {}", index * WASM_ASSOC_ENTRY_SIZE));
                    module.body().line("i32.add");
                    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
                    module.body().line("i32.store");
                    module.body().line(&format!("local.get ${}_ptr", name));
                    module
                        .body()
                        .line(&format!("i32.const {}", index * WASM_ASSOC_ENTRY_SIZE + 8));
                    module.body().line("i32.add");
                    module.body().line(&format!("i64.const {}", next_int_key));
                    module.body().line("i64.store");
                    next_int_key += 1;
                }
                AssocKeyKind::Str => {
                    for entry_offset in [0, 8] {
                        module.body().line(&format!("local.get ${}_ptr", name));
                        module.body().line(&format!(
                            "i32.const {}",
                            index * WASM_ASSOC_ENTRY_SIZE + entry_offset
                        ));
                        module.body().line("i32.add");
                        module.body().line(&format!("local.get {}", source_ptr));
                        module.body().line(&format!(
                            "i32.const {}",
                            (start + index) * WASM_ASSOC_ENTRY_SIZE + entry_offset
                        ));
                        module.body().line("i32.add");
                        module.body().line("i64.load");
                        module.body().line("i64.store");
                    }
                }
            }
        }
        for entry_offset in [16, 24] {
            module.body().line(&format!("local.get ${}_ptr", name));
            module.body().line(&format!(
                "i32.const {}",
                index * WASM_ASSOC_ENTRY_SIZE + entry_offset
            ));
            module.body().line("i32.add");
            module.body().line(&format!("local.get {}", source_ptr));
            module.body().line(&format!(
                "i32.const {}",
                (start + index) * WASM_ASSOC_ENTRY_SIZE + entry_offset
            ));
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("i64.store");
        }
    }
    Ok(())
}

pub(super) use super::array_slice_dynamic::*;

pub(super) use super::array_slice_preserve::*;
