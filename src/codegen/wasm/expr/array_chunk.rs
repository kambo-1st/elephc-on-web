//! Purpose:
//! Lowers wasm32-web array_chunk helpers for static, dynamic, indexed, value-cell, and associative arrays.
//! Keeps chunk sizing, preserve-key metadata, and nested chunk materialization out of expr.rs.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` array assignment lowering.
//!
//! Key details:
//! - Helpers preserve PHP reindexing, preserve_keys behavior, and runtime-length chunk metadata.

use super::*;

pub(super) fn emit_indexed_array_chunk_assign(
    name: &str,
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let preserve_keys = array_chunk_preserve_keys_arg(args, expr.span, module)?;
    if args.len() < 2 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_chunk() expects two or three arguments",
        ));
    }
    let Some(chunk_size) = static_or_const_or_i64_local_value(&args[1], module) else {
        if let ExprKind::Variable(source) = &args[0].kind {
            if module.local_kind(source) == Some(LocalKind::Mixed)
                && module.mixed_value_cell_kind(source).is_none()
            {
                return emit_unknown_mixed_array_chunk_assign(name, expr, args, source, module);
            }
        }
        return emit_dynamic_array_chunk_assign(name, expr, args, module);
    };
    let Ok(chunk_size) = usize::try_from(chunk_size) else {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web array_chunk() requires a static positive integer chunk size",
        ));
    };
    if chunk_size == 0 {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web array_chunk() requires a static positive integer chunk size",
        ));
    }
    match &args[0].kind {
        ExprKind::ArrayLiteral(items) => {
            let chunks = if preserve_keys {
                static_array_chunk_preserve_key_items(expr, items, chunk_size)
            } else {
                static_array_chunk_items(expr, items, chunk_size)
            };
            emit_value_array_items_assign(name, &chunks, module)
        }
        ExprKind::ArrayLiteralAssoc(items) if preserve_keys => {
            let chunks = static_assoc_array_chunk_items(expr, items, chunk_size);
            emit_value_array_items_assign(name, &chunks, module)
        }
        ExprKind::ArrayLiteralAssoc(_) => Err(array_unsupported(&args[0])),
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Mixed)
                && module.mixed_value_cell_kind(source).is_none() =>
        {
            emit_unknown_mixed_array_chunk_assign(name, expr, args, source, module)
        }
        ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
            let Some(len) = module.array_length(source) else {
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web array_chunk() requires a known indexed array length",
                ));
            };
            let source_value_kinds = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec());
            let source_nested_metadata = module
                .array_nested_value_metadata_items(source)
                .map(|metadata| metadata.to_vec());
            let source_key_values = module.array_key_values(source).map(|keys| keys.to_vec());
            if preserve_keys {
                validate_array_chunk_preserve_key_known_source(
                    args[0].span,
                    module.array_layout(source),
                    source_value_kinds.as_deref(),
                    source_nested_metadata.as_deref(),
                    source_key_values.as_deref(),
                    module.array_object_classes(source),
                )?;
            }
            let source_ptr = preserve_array_ptr(source, "array_chunk", module);
            emit_known_array_chunk_assign(
                name,
                &source_ptr,
                module.array_layout(source),
                len,
                chunk_size,
                source_value_kinds,
                source_nested_metadata,
                source_key_values,
                preserve_keys,
                module,
            )
        }
        ExprKind::FunctionCall {
            name: function_name,
            args: call_args,
        } if module.has_function(function_name)
            && module.function_return_kind(function_name) == Some(ValueKind::Array) =>
        {
            let layout = module.function_array_return_layout(function_name);
            emit_user_function_args(&args[0], function_name, call_args, module)?;
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(function_name)));
            let source_ptr = module.next_label("array_chunk_return_ptr");
            let source_len = module.next_label("array_chunk_return_len");
            for local in [&source_ptr, &source_len] {
                module.declare_i32_local(local.trim_start_matches('$').to_string());
            }
            module.body().line(&format!("local.set {}", source_len));
            module.body().line(&format!("local.set {}", source_ptr));
            let Some(len) = module.function_array_return_length(function_name) else {
                if layout != ArrayLayout::Value {
                    return Err(CompileError::new(
                        args[0].span,
                        "wasm32-web array_chunk() requires a known function return length",
                    ));
                }
                let outer_len = module.next_label("array_chunk_outer_len");
                module.declare_i32_local(outer_len.trim_start_matches('$').to_string());
                emit_runtime_array_chunk_outer_len(&source_len, chunk_size, &outer_len, module);
                module.clear_array_length(name);
                module.set_array_layout(name, ArrayLayout::Value);
                module.set_array_value_cell_kinds(name, None);
                module.set_array_nested_value_metadata(name, None);
                module.body().line(&format!("local.get {}", source_ptr));
                module.body().line(&format!("local.get {}", source_len));
                module.body().line(&format!("i32.const {}", chunk_size));
                module.body().line(&format!("local.get {}", outer_len));
                module.body().line("call $__rt_array_chunk_value_cells");
                module.body().line(&format!("local.set ${}_ptr", name));
                module.body().line(&format!("local.get {}", outer_len));
                module.body().line(&format!("local.set ${}_len", name));
                return Ok(());
            };
            if preserve_keys && layout == ArrayLayout::Assoc && module.function_array_return_key_values(function_name).is_none() {
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web array_chunk(..., true) requires known associative return keys",
                ));
            }
            let source_value_kinds = module
                .function_array_return_value_kinds(function_name)
                .map(|kinds| kinds.to_vec());
            let source_nested_metadata = module
                .function_array_return_nested_values(function_name)
                .map(|metadata| metadata.to_vec());
            let source_key_values = module
                .function_array_return_key_values(function_name)
                .map(|keys| keys.to_vec());
            if preserve_keys {
                validate_array_chunk_preserve_key_known_source(
                    args[0].span,
                    module.function_array_return_layout(function_name),
                    source_value_kinds.as_deref(),
                    source_nested_metadata.as_deref(),
                    source_key_values.as_deref(),
                    None,
                )?;
            }
            emit_known_array_chunk_assign(
                name,
                &source_ptr,
                module.function_array_return_layout(function_name),
                len,
                chunk_size,
                source_value_kinds,
                source_nested_metadata,
                source_key_values,
                preserve_keys,
                module,
            )
        }
        ExprKind::FunctionCall { .. } if expression_has_array_type(&args[0], module) => {
            let temp = materialize_array_map_multi_source(&args[0], "array_chunk_source", module)?;
            let Some(len) = module.array_length(&temp) else {
                if module.array_layout(&temp) != ArrayLayout::Value {
                    return Err(CompileError::new(
                        args[0].span,
                        "wasm32-web array_chunk() requires a known indexed array length",
                    ));
                }
                return emit_dynamic_array_chunk_assign(
                    name,
                    expr,
                    &[
                        Expr::new(ExprKind::Variable(temp), args[0].span),
                        args[1].clone(),
                    ],
                    module,
                );
            };
            let source_value_kinds = module.array_value_cell_kinds(&temp).map(|kinds| kinds.to_vec());
            let source_nested_metadata = module
                .array_nested_value_metadata_items(&temp)
                .map(|metadata| metadata.to_vec());
            let source_key_values = module.array_key_values(&temp).map(|keys| keys.to_vec());
            if preserve_keys {
                validate_array_chunk_preserve_key_known_source(
                    args[0].span,
                    module.array_layout(&temp),
                    source_value_kinds.as_deref(),
                    source_nested_metadata.as_deref(),
                    source_key_values.as_deref(),
                    module.array_object_classes(&temp),
                )?;
            }
            let source_ptr = preserve_array_ptr(&temp, "array_chunk", module);
            emit_known_array_chunk_assign(
                name,
                &source_ptr,
                module.array_layout(&temp),
                len,
                chunk_size,
                source_value_kinds,
                source_nested_metadata,
                source_key_values,
                preserve_keys,
                module,
            )
        }
        ExprKind::MethodCall { object, method, .. }
        | ExprKind::NullsafeMethodCall { object, method, .. }
            if method_call_array_return_metadata(object, method, module).is_some() =>
        {
            let temp = materialize_array_map_multi_source(&args[0], "array_chunk_method_source", module)?;
            let Some(len) = module.array_length(&temp) else {
                if module.array_layout(&temp) != ArrayLayout::Value {
                    return Err(CompileError::new(
                        args[0].span,
                        "wasm32-web array_chunk() requires a known indexed array length",
                    ));
                }
                return emit_dynamic_array_chunk_assign(
                    name,
                    expr,
                    &[
                        Expr::new(ExprKind::Variable(temp), args[0].span),
                        args[1].clone(),
                    ],
                    module,
                );
            };
            let source_value_kinds = module.array_value_cell_kinds(&temp).map(|kinds| kinds.to_vec());
            let source_nested_metadata = module
                .array_nested_value_metadata_items(&temp)
                .map(|metadata| metadata.to_vec());
            let source_key_values = module.array_key_values(&temp).map(|keys| keys.to_vec());
            if preserve_keys {
                validate_array_chunk_preserve_key_known_source(
                    args[0].span,
                    module.array_layout(&temp),
                    source_value_kinds.as_deref(),
                    source_nested_metadata.as_deref(),
                    source_key_values.as_deref(),
                    module.array_object_classes(&temp),
                )?;
            }
            let source_ptr = preserve_array_ptr(&temp, "array_chunk", module);
            emit_known_array_chunk_assign(
                name,
                &source_ptr,
                module.array_layout(&temp),
                len,
                chunk_size,
                source_value_kinds,
                source_nested_metadata,
                source_key_values,
                preserve_keys,
                module,
            )
        }
        ExprKind::StaticMethodCall { .. } if expression_has_array_type(&args[0], module) => {
            let temp = materialize_array_map_multi_source(&args[0], "array_chunk_enum_cases", module)?;
            let Some(len) = module.array_length(&temp) else {
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web array_chunk(Enum::cases()) requires enum case metadata",
                ));
            };
            let source_value_kinds = module.array_value_cell_kinds(&temp).map(|kinds| kinds.to_vec());
            let source_nested_metadata = module
                .array_nested_value_metadata_items(&temp)
                .map(|metadata| metadata.to_vec());
            let source_key_values = module.array_key_values(&temp).map(|keys| keys.to_vec());
            if preserve_keys {
                validate_array_chunk_preserve_key_known_source(
                    args[0].span,
                    module.array_layout(&temp),
                    source_value_kinds.as_deref(),
                    source_nested_metadata.as_deref(),
                    source_key_values.as_deref(),
                    module.array_object_classes(&temp),
                )?;
            }
            let source_ptr = preserve_array_ptr(&temp, "array_chunk", module);
            emit_known_array_chunk_assign(
                name,
                &source_ptr,
                module.array_layout(&temp),
                len,
                chunk_size,
                source_value_kinds,
                source_nested_metadata,
                source_key_values,
                preserve_keys,
                module,
            )
        }
        _ => Err(CompileError::new(
            args[0].span,
            "wasm32-web array_chunk() currently supports indexed array values only",
        )),
    }
}

fn emit_unknown_mixed_array_chunk_assign(
    name: &str,
    expr: &Expr,
    args: &[Expr],
    source: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("unknown_mixed_array_chunk_source")
        .trim_start_matches('$')
        .to_string();
    let heap_kind = module.next_label("unknown_mixed_array_chunk_heap_kind");
    let mut chunk_args = args.to_vec();
    chunk_args[0] = Expr::new(ExprKind::Variable(temp.clone()), args[0].span);
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
    emit_dynamic_array_chunk_assign(name, expr, &chunk_args, module)?;
    module.body().close("else");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.set_array_layout(&temp, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(&temp, None);
    module.set_array_key_kinds(&temp, None);
    module.set_array_php_normalized_runtime_keys(&temp, true);
    if array_chunk_preserve_keys_arg(args, expr.span, module)? {
        emit_dynamic_assoc_array_chunk_preserve_keys_assign(name, &temp, &chunk_args, module)?;
    } else {
        emit_dynamic_array_chunk_assign(name, expr, &chunk_args, module)?;
    }
    module.body().close("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn array_chunk_preserve_keys_arg(
    args: &[Expr],
    span: Span,
    module: &WasmModule,
) -> Result<bool, CompileError> {
    if args.len() > 3 {
        return Err(CompileError::new(
            span,
            "wasm32-web array_chunk() expects two or three arguments",
        ));
    }
    let Some(preserve_keys) = args.get(2) else {
        return Ok(false);
    };
    let Some(value) = static_or_const_or_i32_bool_value(preserve_keys, module) else {
        return Err(CompileError::new(
            preserve_keys.span,
            "wasm32-web array_chunk() currently requires a static boolean preserve_keys argument",
        ));
    };
    Ok(value)
}

pub(super) fn validate_array_chunk_preserve_key_known_source(
    span: Span,
    source_layout: ArrayLayout,
    source_value_kinds: Option<&[ValueCellKind]>,
    source_nested_metadata: Option<&[Option<NestedArrayMetadata>]>,
    source_key_values: Option<&[AssocKeyValue]>,
    source_object_classes: Option<&[Option<String>]>,
) -> Result<(), CompileError> {
    match source_layout {
        ArrayLayout::CompactInt => Ok(()),
        ArrayLayout::Value if source_value_kinds.is_some_and(|kinds| {
            kinds.iter().all(|kind| *kind != ValueCellKind::Array)
        }) => Ok(()),
        ArrayLayout::Value
            if known_nested_array_chunk_source(source_value_kinds, source_nested_metadata) =>
        {
            Ok(())
        }
        ArrayLayout::Value if source_object_classes.is_some() => Ok(()),
        ArrayLayout::Assoc
            if source_key_values.is_some()
                && source_value_kinds.is_some_and(|kinds| {
                    kinds.iter().all(|kind| *kind != ValueCellKind::Array)
                }) =>
        {
            Ok(())
        }
        _ => Err(CompileError::new(
            span,
            "wasm32-web array_chunk(..., true) currently supports known scalar sources only",
        )),
    }
}

pub(super) fn known_nested_array_chunk_source(
    source_value_kinds: Option<&[ValueCellKind]>,
    source_nested_metadata: Option<&[Option<NestedArrayMetadata>]>,
) -> bool {
    let Some(kinds) = source_value_kinds else {
        return false;
    };
    let Some(metadata) = source_nested_metadata else {
        return false;
    };
    kinds.len() == metadata.len()
        && kinds
            .iter()
            .zip(metadata.iter())
            .all(|(kind, metadata)| *kind != ValueCellKind::Array || metadata.is_some())
}

pub(super) fn emit_dynamic_array_chunk_assign(
    name: &str,
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let source = match &args[0].kind {
        ExprKind::Variable(source) => source.clone(),
        ExprKind::FunctionCall { .. } | ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)
            if expression_has_array_type(&args[0], module) =>
        {
            let temp = module
                .next_label("array_chunk_return_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, &args[0], module)?;
            temp
        }
        _ => {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web dynamic array_chunk() currently requires an assigned value-array source",
            ));
        }
    };
    if module.local_kind(&source) != Some(LocalKind::Array) {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web dynamic array_chunk() currently requires an assigned value-array source",
        ));
    }
    let preserve_keys = array_chunk_preserve_keys_arg(args, expr.span, module)?;
    if preserve_keys && module.array_layout(&source) == ArrayLayout::Assoc {
        return emit_dynamic_assoc_array_chunk_preserve_keys_assign(name, &source, args, module);
    }
    let promoted_compact_source = module.array_layout(&source) == ArrayLayout::CompactInt;
    let source = if promoted_compact_source {
        let value_source = module
            .next_label("array_chunk_value_source")
            .trim_start_matches('$')
            .to_string();
        module.declare_array_local(value_source.clone());
        emit_compact_array_to_value_array_assign(&value_source, &source, module)?;
        value_source
    } else if module.array_layout(&source) == ArrayLayout::Assoc {
        let value_source = module
            .next_label("array_chunk_assoc_values_source")
            .trim_start_matches('$')
            .to_string();
        module.declare_array_local(value_source.clone());
        emit_assoc_array_values_assign(&value_source, &source, module)?;
        value_source
    } else {
        source
    };
    if module.array_layout(&source) != ArrayLayout::Value {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web dynamic array_chunk() currently requires an assigned value-array source",
        ));
    }
    let source_ptr = preserve_array_ptr(&source, "array_chunk", module);
    let source_len = module.next_label("array_chunk_source_len");
    let chunk_size = module.next_label("array_chunk_size");
    let outer_len = module.next_label("array_chunk_outer_len");
    for local in [&source_len, &chunk_size, &outer_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", source_len));
    require_int(&args[1], module)?;
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", chunk_size));
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.const 0");
    module.body().line("i32.le_s");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    emit_runtime_array_chunk_outer_len_from_local(&source_len, &chunk_size, &outer_len, module);
    let first_chunk_metadata = dynamic_array_chunk_metadata(&source, preserve_keys, module);
    let runtime_chunk_metadata = if first_chunk_metadata.is_none() {
        dynamic_runtime_array_chunk_metadata(&source, preserve_keys, module).or_else(|| {
            promoted_compact_source.then_some(NestedArrayMetadata {
                layout: if preserve_keys {
                    ArrayLayout::Assoc
                } else {
                    ArrayLayout::Value
                },
                len: usize::MAX,
                value_kinds: Some(vec![ValueCellKind::Int]),
                key_values: None,
                nested_values: None,
            })
        })
    } else {
        None
    };
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Array));
    module.set_array_nested_value_metadata(name, first_chunk_metadata);
    module.set_array_runtime_nested_value_metadata(name, runtime_chunk_metadata);
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line(&format!("local.get {}", outer_len));
    if preserve_keys {
        module
            .body()
            .line("call $__rt_array_chunk_value_cells_preserve_int_keys");
    } else {
        module.body().line("call $__rt_array_chunk_value_cells");
    }
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", outer_len));
    module.body().line(&format!("local.set ${}_len", name));
    let _ = expr;
    Ok(())
}

fn emit_dynamic_assoc_array_chunk_preserve_keys_assign(
    name: &str,
    source: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let source_ptr = preserve_array_ptr(source, "array_chunk", module);
    let source_len = module.next_label("array_chunk_assoc_source_len");
    let chunk_size = module.next_label("array_chunk_assoc_size");
    let outer_len = module.next_label("array_chunk_assoc_outer_len");
    for local in [&source_len, &chunk_size, &outer_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", source_len));
    require_int(&args[1], module)?;
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", chunk_size));
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.const 0");
    module.body().line("i32.le_s");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    emit_runtime_array_chunk_outer_len_from_local(&source_len, &chunk_size, &outer_len, module);
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Array));
    let first_chunk_metadata = dynamic_array_chunk_metadata(source, true, module);
    let runtime_chunk_metadata = if first_chunk_metadata.is_none() {
        dynamic_runtime_array_chunk_metadata(source, true, module)
    } else {
        None
    };
    module.set_array_nested_value_metadata(name, first_chunk_metadata);
    module.set_array_runtime_nested_value_metadata(
        name,
        runtime_chunk_metadata,
    );
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line(&format!("local.get {}", outer_len));
    module
        .body()
        .line("call $__rt_array_chunk_assoc_entries_preserve_keys");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", outer_len));
    module.body().line(&format!("local.set ${}_len", name));
    Ok(())
}

pub(super) fn dynamic_runtime_array_chunk_metadata(
    source: &str,
    preserve_keys: bool,
    module: &WasmModule,
) -> Option<NestedArrayMetadata> {
    if let Some(kind) = module.array_runtime_value_cell_kind(source) {
        return Some(NestedArrayMetadata {
            layout: if preserve_keys {
                ArrayLayout::Assoc
            } else {
                ArrayLayout::Value
            },
            len: usize::MAX,
            value_kinds: Some(vec![kind]),
            key_values: None,
            nested_values: None,
        });
    }
    if preserve_keys && module.array_value_cell_kinds(source).is_some() {
        return Some(NestedArrayMetadata {
            layout: ArrayLayout::Assoc,
            len: usize::MAX,
            value_kinds: None,
            key_values: None,
            nested_values: None,
        });
    }
    if preserve_keys && module.array_layout(source) == ArrayLayout::Assoc {
        return Some(NestedArrayMetadata {
            layout: ArrayLayout::Assoc,
            len: usize::MAX,
            value_kinds: None,
            key_values: None,
            nested_values: None,
        });
    }
    None
}

pub(super) fn dynamic_array_chunk_metadata(
    source: &str,
    preserve_keys: bool,
    module: &WasmModule,
) -> Option<Vec<Option<NestedArrayMetadata>>> {
    let source_len = module.array_length(source)?;
    if source_len == 0 {
        return None;
    }
    let kinds = module.array_value_cell_kinds(source)?;
    let homogeneous_value_kinds = kinds
        .first()
        .copied()
        .filter(|first| kinds.iter().all(|kind| *kind == *first))
        .map(|kind| vec![kind; source_len]);
    Some(
        (0..source_len)
            .map(|index| {
                Some(NestedArrayMetadata {
                    layout: if preserve_keys {
                        ArrayLayout::Assoc
                    } else {
                        ArrayLayout::Value
                    },
                    len: source_len,
                    value_kinds: homogeneous_value_kinds.clone().or_else(|| {
                        if index == 0 {
                            Some(kinds.to_vec())
                        } else {
                            None
                        }
                    }),
                    key_values: None,
                    nested_values: None,
                })
            })
            .collect(),
    )
}

pub(super) fn emit_runtime_array_chunk_outer_len(
    source_len: &str,
    chunk_size: usize,
    outer_len: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.const 0");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", outer_len));
    module.body().line("else");
    module.body().line(&format!("local.get {}", source_len));
    module.body().line(&format!("i32.const {}", chunk_size));
    module.body().line("i32.add");
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("i32.const {}", chunk_size));
    module.body().line("i32.div_u");
    module.body().line(&format!("local.set {}", outer_len));
    module.body().close("end");
}

pub(super) fn emit_runtime_array_chunk_outer_len_from_local(
    source_len: &str,
    chunk_size: &str,
    outer_len: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.const 0");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", outer_len));
    module.body().line("else");
    module.body().line(&format!("local.get {}", source_len));
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.add");
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.div_u");
    module.body().line(&format!("local.set {}", outer_len));
    module.body().close("end");
}
