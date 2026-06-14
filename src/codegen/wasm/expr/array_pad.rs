//! Purpose:
//! Lowers wasm32-web array_pad helpers for compact, value-cell, associative, and dynamic arrays.
//! Keeps pad sizing, metadata propagation, and runtime source/value loops out of expr.rs.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` array assignment lowering.
//!
//! Key details:
//! - Helpers preserve PHP left/right padding behavior and promote to value-cell arrays when needed.

use super::*;
use super::array_transform_sets::{emit_assoc_entry_address, emit_assoc_value_cell_address};

pub(super) fn emit_indexed_array_pad_assign(
    name: &str,
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if args.len() != 3 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_pad() expects exactly three arguments",
        ));
    }
    let Some(target_len) = static_or_const_or_i64_local_value(&args[1], module) else {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web array_pad() requires a static integer length",
        ));
    };
    let target_abs_i32 = i32::try_from(target_len.unsigned_abs()).map_err(|_| {
        CompileError::new(args[1].span, "wasm32-web array_pad() length is out of range")
    })?;
    let target_abs = target_len.unsigned_abs() as usize;
    match &args[0].kind {
        ExprKind::ArrayLiteral(items) => {
            let padded = static_array_pad_items(items, target_len, &args[2]);
            if array_literal_needs_value_cells(&padded) {
                emit_value_array_items_assign(name, &padded, module)
            } else {
                module.set_array_layout(name, ArrayLayout::CompactInt);
                emit_static_array_items_assign(name, &padded, module)
            }
        }
        ExprKind::ArrayLiteralAssoc(_) => Err(array_unsupported(&args[0])),
        ExprKind::ArrayAccess { .. } if nested_array_metadata_for_access_expr(&args[0], module).is_some() => {
            let temp = materialize_nested_array_pad_source(&args[0], module)?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                return emit_assoc_array_pad_assign(
                    name,
                    &temp,
                    target_len,
                    target_abs_i32,
                    &args[2],
                    module,
                );
            }
            if module.array_layout(&temp) == ArrayLayout::Value || expr_needs_value_cell(&args[2]) {
                if module.array_length(&temp).is_none() {
                    return emit_dynamic_value_array_pad_assign(
                        name,
                        &Expr::new(ExprKind::Variable(temp.clone()), args[0].span),
                        module.array_layout(&temp),
                        target_len,
                        target_abs_i32,
                        &args[2],
                        module,
                    );
                }
                return emit_known_value_array_pad_assign(
                    name,
                    &Expr::new(ExprKind::Variable(temp.clone()), args[0].span),
                    target_len,
                    &args[2],
                    module,
                );
            }
            emit_dynamic_indexed_array_pad_assign(
                name,
                &Expr::new(ExprKind::Variable(temp.clone()), args[0].span),
                target_len,
                target_abs_i32,
                &args[2],
                module,
            )
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Mixed)
                && module.mixed_value_cell_kind(source).is_none() =>
        {
            emit_unknown_mixed_array_pad_assign(
                name,
                source,
                target_len,
                target_abs_i32,
                &args[2],
                module,
            )
        }
        ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
            if module.array_layout(source) == ArrayLayout::Assoc {
                return emit_assoc_array_pad_assign(
                    name,
                    source,
                    target_len,
                    target_abs_i32,
                    &args[2],
                    module,
                );
            }
            if module.array_layout(source) == ArrayLayout::Value || expr_needs_value_cell(&args[2]) {
                if module.array_length(source).is_none() {
                    return emit_dynamic_value_array_pad_assign(
                        name,
                        &args[0],
                        module.array_layout(source),
                        target_len,
                        target_abs_i32,
                        &args[2],
                        module,
                    );
                }
                return emit_known_value_array_pad_assign(name, &args[0], target_len, &args[2], module);
            }
            let Some(len) = module.array_length(source) else {
                return emit_dynamic_indexed_array_pad_assign(
                    name,
                    &args[0],
                    target_len,
                    target_abs_i32,
                    &args[2],
                    module,
                );
            };
            let out_len = target_abs.max(len);
            let pad_count = out_len - len;
            let source_ptr = preserve_array_ptr(source, "array_pad", module);
            emit_array_alloc_prelude(name, out_len, module);
            if target_len < 0 {
                for index in 0..pad_count {
                    emit_array_store_expr(name, index, &args[2], module)?;
                }
                for source_index in 0..len {
                    emit_array_store_load(
                        name,
                        pad_count + source_index,
                        &source_ptr,
                        source_index,
                        module,
                    );
                }
            } else {
                for source_index in 0..len {
                    emit_array_store_load(name, source_index, &source_ptr, source_index, module);
                }
                for index in len..out_len {
                    emit_array_store_expr(name, index, &args[2], module)?;
                }
            }
            Ok(())
        }
        ExprKind::FunctionCall { name: function_name, .. }
            if module.has_function(function_name)
            && module.function_return_kind(function_name) == Some(ValueKind::Array)
            && (module.function_array_return_layout(function_name) == ArrayLayout::Value
                || expr_needs_value_cell(&args[2])) =>
        {
            if module.function_array_return_layout(function_name) == ArrayLayout::Assoc
                && !function_array_return_has_only_int_keys(function_name, module)
            {
                return Err(array_unsupported(&args[0]));
            }
            if module.function_array_return_length(function_name).is_none() {
                return emit_dynamic_value_array_pad_assign(
                    name,
                    &args[0],
                    module.function_array_return_layout(function_name),
                    target_len,
                    target_abs_i32,
                    &args[2],
                    module,
                );
            }
            emit_known_value_array_pad_assign(name, &args[0], target_len, &args[2], module)
        }
        ExprKind::MethodCall { object, method, .. }
        | ExprKind::NullsafeMethodCall { object, method, .. }
            if method_call_array_return_metadata(object, method, module).is_some() =>
        {
            let temp = materialize_method_array_pad_source(&args[0], object, method, module)?;
            let source_layout = module.array_layout(&temp);
            if source_layout == ArrayLayout::Assoc {
                return emit_assoc_array_pad_assign(
                    name,
                    &temp,
                    target_len,
                    target_abs_i32,
                    &args[2],
                    module,
                );
            }
            if source_layout == ArrayLayout::Value || expr_needs_value_cell(&args[2]) {
                if module.array_length(&temp).is_none() {
                    return emit_dynamic_value_array_pad_assign(
                        name,
                        &Expr::new(ExprKind::Variable(temp), args[0].span),
                        source_layout,
                        target_len,
                        target_abs_i32,
                        &args[2],
                        module,
                    );
                }
                return emit_known_value_array_pad_assign(
                    name,
                    &Expr::new(ExprKind::Variable(temp), args[0].span),
                    target_len,
                    &args[2],
                    module,
                );
            }
            emit_dynamic_indexed_array_pad_assign(
                name,
                &Expr::new(ExprKind::Variable(temp), args[0].span),
                target_len,
                target_abs_i32,
                &args[2],
                module,
            )
        }
        ExprKind::FunctionCall { .. } | ExprKind::ExprCall { .. }
            if expression_has_array_type(&args[0], module) =>
        {
            let temp = materialize_array_map_multi_source(&args[0], "array_pad_source", module)?;
            let source_layout = module.array_layout(&temp);
            if source_layout == ArrayLayout::Assoc && !assoc_array_has_only_int_keys(&temp, module) {
                return Err(array_unsupported(&args[0]));
            }
            if source_layout == ArrayLayout::Value || expr_needs_value_cell(&args[2]) {
                if module.array_length(&temp).is_none() {
                    return emit_dynamic_value_array_pad_assign(
                        name,
                        &Expr::new(ExprKind::Variable(temp), args[0].span),
                        source_layout,
                        target_len,
                        target_abs_i32,
                        &args[2],
                        module,
                    );
                }
                return emit_known_value_array_pad_assign(
                    name,
                    &Expr::new(ExprKind::Variable(temp), args[0].span),
                    target_len,
                    &args[2],
                    module,
                );
            }
            emit_dynamic_indexed_array_pad_assign(
                name,
                &Expr::new(ExprKind::Variable(temp), args[0].span),
                target_len,
                target_abs_i32,
                &args[2],
                module,
            )
        }
        ExprKind::StaticMethodCall { receiver, method, .. }
            if static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            let temp = materialize_static_method_array_pad_source(&args[0], receiver, method, module)?;
            let source_layout = module.array_layout(&temp);
            if source_layout == ArrayLayout::Assoc {
                return emit_assoc_array_pad_assign(
                    name,
                    &temp,
                    target_len,
                    target_abs_i32,
                    &args[2],
                    module,
                );
            }
            if source_layout == ArrayLayout::Value || expr_needs_value_cell(&args[2]) {
                if module.array_length(&temp).is_none() {
                    return emit_dynamic_value_array_pad_assign(
                        name,
                        &Expr::new(ExprKind::Variable(temp), args[0].span),
                        source_layout,
                        target_len,
                        target_abs_i32,
                        &args[2],
                        module,
                    );
                }
                return emit_known_value_array_pad_assign(
                    name,
                    &Expr::new(ExprKind::Variable(temp), args[0].span),
                    target_len,
                    &args[2],
                    module,
                );
            }
            emit_dynamic_indexed_array_pad_assign(
                name,
                &Expr::new(ExprKind::Variable(temp), args[0].span),
                target_len,
                target_abs_i32,
                &args[2],
                module,
            )
        }
        ExprKind::StaticMethodCall { .. } if expression_has_array_type(&args[0], module) => {
            let temp = materialize_array_map_multi_source(&args[0], "array_pad_enum_cases", module)?;
            let source_layout = module.array_layout(&temp);
            if source_layout == ArrayLayout::Assoc && !assoc_array_has_only_int_keys(&temp, module) {
                return Err(array_unsupported(&args[0]));
            }
            if source_layout == ArrayLayout::Value || expr_needs_value_cell(&args[2]) {
                if module.array_length(&temp).is_none() {
                    return emit_dynamic_value_array_pad_assign(
                        name,
                        &Expr::new(ExprKind::Variable(temp), args[0].span),
                        source_layout,
                        target_len,
                        target_abs_i32,
                        &args[2],
                        module,
                    );
                }
                return emit_known_value_array_pad_assign(
                    name,
                    &Expr::new(ExprKind::Variable(temp), args[0].span),
                    target_len,
                    &args[2],
                    module,
                );
            }
            emit_dynamic_indexed_array_pad_assign(
                name,
                &Expr::new(ExprKind::Variable(temp), args[0].span),
                target_len,
                target_abs_i32,
                &args[2],
                module,
            )
        }
        _ if expression_is_arrayy(&args[0], module) => emit_dynamic_indexed_array_pad_assign(
            name,
            &args[0],
            target_len,
            target_abs_i32,
            &args[2],
            module,
        ),
        _ => Err(CompileError::new(
            args[0].span,
            "wasm32-web array_pad() currently supports indexed array values only",
        )),
    }
}

fn materialize_method_array_pad_source(
    source: &Expr,
    object: &Expr,
    method: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let metadata = method_call_array_return_metadata(object, method, module).ok_or_else(|| {
        CompileError::new(
            source.span,
            "wasm32-web array_pad() requires method array return metadata",
        )
    })?;
    let temp = module
        .next_label("array_pad_method_source")
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
                "wasm32-web array_pad() expected an array-returning method value",
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

fn materialize_static_method_array_pad_source(
    source: &Expr,
    receiver: &StaticReceiver,
    method: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let metadata = static_method_call_array_return_metadata(receiver, method, module).ok_or_else(|| {
        CompileError::new(
            source.span,
            "wasm32-web array_pad() requires static method array return metadata",
        )
    })?;
    let temp = module
        .next_label("array_pad_static_method_source")
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
                "wasm32-web array_pad() expected an array-returning static method value",
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

fn materialize_nested_array_pad_source(
    source: &Expr,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let Some(metadata) = nested_array_metadata_for_access_expr(source, module) else {
        return Err(CompileError::new(
            source.span,
            "wasm32-web array_pad() requires nested array metadata",
        ));
    };
    let temp = module
        .next_label("array_pad_nested_source")
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
                "wasm32-web array_pad() expected a nested array value",
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

fn emit_unknown_mixed_array_pad_assign(
    name: &str,
    source: &str,
    target_len: i64,
    target_abs: i32,
    pad_value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("unknown_mixed_array_pad_source")
        .trim_start_matches('$')
        .to_string();
    let heap_kind = module.next_label("unknown_mixed_array_pad_heap_kind");
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
    emit_unknown_mixed_indexed_array_pad_assoc_assign(
        name,
        &temp,
        target_len,
        target_abs,
        pad_value,
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
    emit_assoc_array_pad_assign(name, &temp, target_len, target_abs, pad_value, module)?;
    module.body().close("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_unknown_mixed_indexed_array_pad_assoc_assign(
    name: &str,
    source: &str,
    target_len: i64,
    target_abs: i32,
    pad_value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let target_abs = usize::try_from(target_abs).map_err(|_| {
        CompileError::new(pad_value.span, "wasm32-web array_pad() length is out of range")
    })?;
    let source_len = module.next_label("unknown_mixed_indexed_pad_source_len");
    let out_len = module.next_label("unknown_mixed_indexed_pad_out_len");
    let pad_count = module.next_label("unknown_mixed_indexed_pad_count");
    let index = module.next_label("unknown_mixed_indexed_pad_index");
    let source_index = module.next_label("unknown_mixed_indexed_pad_source_index");
    let target_entry = module.next_label("unknown_mixed_indexed_pad_target_entry");
    let target_cell = module.next_label("unknown_mixed_indexed_pad_target_cell");
    let source_cell = module.next_label("unknown_mixed_indexed_pad_source_cell");
    for local in [
        &source_len,
        &out_len,
        &pad_count,
        &index,
        &source_index,
        &target_entry,
        &target_cell,
        &source_cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_key_kinds(name, None);
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", source_len));
    module.body().line(&format!("i32.const {}", target_abs));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("else");
    module.body().line(&format!("i32.const {}", target_abs));
    module.body().close("end");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", pad_count));
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.set ${}_len", name));
    if target_len < 0 {
        emit_unknown_mixed_indexed_array_pad_values_assoc(
            name,
            &index,
            "i32.const 0",
            &pad_count,
            &target_entry,
            &target_cell,
            pad_value,
            module,
        )?;
        emit_unknown_mixed_indexed_array_pad_source_assoc(
            name,
            source,
            &index,
            &source_index,
            &source_len,
            Some(&pad_count),
            &target_entry,
            &target_cell,
            &source_cell,
            module,
        );
    } else {
        emit_unknown_mixed_indexed_array_pad_source_assoc(
            name,
            source,
            &index,
            &source_index,
            &source_len,
            None,
            &target_entry,
            &target_cell,
            &source_cell,
            module,
        );
        emit_unknown_mixed_indexed_array_pad_values_assoc(
            name,
            &index,
            &format!("local.get {}", source_len),
            &out_len,
            &target_entry,
            &target_cell,
            pad_value,
            module,
        )?;
    }
    Ok(())
}

fn emit_unknown_mixed_indexed_array_pad_source_assoc(
    name: &str,
    source: &str,
    index: &str,
    source_index: &str,
    source_len: &str,
    target_offset: Option<&str>,
    target_entry: &str,
    target_cell: &str,
    source_cell: &str,
    module: &mut WasmModule,
) {
    let done_label = module.next_label("unknown_mixed_indexed_pad_source_done");
    let loop_label = module.next_label("unknown_mixed_indexed_pad_source_loop");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", source_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    if let Some(target_offset) = target_offset {
        module.body().line(&format!("local.get {}", target_offset));
        module.body().line(&format!("local.get {}", source_index));
        module.body().line("i32.add");
    } else {
        module.body().line(&format!("local.get {}", source_index));
    }
    module.body().line(&format!("local.set {}", index));
    emit_unknown_mixed_indexed_array_pad_entry_key(name, index, target_entry, module);
    emit_assoc_value_cell_address(target_entry, target_cell, module);
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", source_cell));
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get {}", source_cell));
    module.body().line("call $__rt_value_copy");
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_unknown_mixed_indexed_array_pad_values_assoc(
    name: &str,
    index: &str,
    start: &str,
    end: &str,
    target_entry: &str,
    target_cell: &str,
    pad_value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let done_label = module.next_label("unknown_mixed_indexed_pad_values_done");
    let loop_label = module.next_label("unknown_mixed_indexed_pad_values_loop");
    module.body().line(start);
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_unknown_mixed_indexed_array_pad_entry_key(name, index, target_entry, module);
    emit_assoc_value_cell_address(target_entry, target_cell, module);
    emit_store_value_cell(target_cell, pad_value, module)?;
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_unknown_mixed_indexed_array_pad_entry_key(
    name: &str,
    index: &str,
    target_entry: &str,
    module: &mut WasmModule,
) {
    emit_assoc_entry_address(&format!("${}_ptr", name), index, target_entry, module);
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.store");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i64.extend_i32_u");
    module.body().line("i64.store");
}

pub(super) fn emit_known_value_array_pad_assign(
    name: &str,
    source: &Expr,
    target_len: i64,
    pad_value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let (
        len,
        source_ptr,
        source_layout,
        source_value_kinds,
        source_value_constants,
        source_nested_values,
        source_object_classes,
    ) = match &source.kind {
        ExprKind::Variable(source_name) if module.local_kind(source_name) == Some(LocalKind::Array) => {
            let Some(len) = module.array_length(source_name) else {
                return Err(CompileError::new(
                    source.span,
                    "wasm32-web value-array array_pad() requires a known indexed array length",
                ));
            };
            if module.array_layout(source_name) == ArrayLayout::Assoc
                && !assoc_array_has_only_int_keys(source_name, module)
            {
                return Err(array_unsupported(source));
            }
            (
                len,
                preserve_array_ptr(source_name, "array_pad", module),
                module.array_layout(source_name),
                if module.array_layout(source_name) == ArrayLayout::Value
                    || module.array_layout(source_name) == ArrayLayout::Assoc
                {
                    module.array_value_cell_kinds(source_name).map(|kinds| kinds.to_vec())
                } else {
                    Some(vec![ValueCellKind::Int; len])
                },
                module.array_value_constants(source_name).map(|values| values.to_vec()),
                module
                    .array_nested_value_metadata_items(source_name)
                    .map(|items| items.to_vec()),
                module.array_object_classes(source_name).map(|classes| classes.to_vec()),
            )
        }
        ExprKind::FunctionCall {
            name: function_name,
            args,
        } if module.has_function(function_name)
            && module.function_return_kind(function_name) == Some(ValueKind::Array) =>
        {
            let Some(len) = module.function_array_return_length(function_name) else {
                return Err(CompileError::new(
                    source.span,
                    "wasm32-web value-array array_pad() requires known function return lengths",
                ));
            };
            if module.function_array_return_layout(function_name) == ArrayLayout::Assoc
                && !function_array_return_has_only_int_keys(function_name, module)
            {
                return Err(array_unsupported(source));
            }
            emit_user_function_args(source, function_name, args, module)?;
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(function_name)));
            let source_ptr = module.next_label("array_pad_return_ptr");
            let source_len = module.next_label("array_pad_return_len");
            for local in [&source_ptr, &source_len] {
                module.declare_i32_local(local.trim_start_matches('$').to_string());
            }
            module.body().line(&format!("local.set {}", source_len));
            module.body().line(&format!("local.set {}", source_ptr));
            (
                len,
                source_ptr,
                module.function_array_return_layout(function_name),
                if module.function_array_return_layout(function_name) == ArrayLayout::Value
                    || module.function_array_return_layout(function_name) == ArrayLayout::Assoc
                {
                    module
                        .function_array_return_value_kinds(function_name)
                        .map(|kinds| kinds.to_vec())
                } else {
                    Some(vec![ValueCellKind::Int; len])
                },
                None,
                module
                    .function_array_return_nested_values(function_name)
                    .map(|items| items.to_vec()),
                None,
            )
        }
        _ => return Err(array_unsupported(source)),
    };
    let target_abs = target_len.unsigned_abs() as usize;
    let out_len = target_abs.max(len);
    let pad_count = out_len - len;
    module.set_array_length(name, out_len);
    module.set_array_layout(name, ArrayLayout::Value);
    let pad_value_kind = value_cell_kind_for_expr(pad_value, module);
    let padded_value_kinds = match (source_value_kinds, pad_value_kind) {
        (Some(source_kinds), Some(pad_kind)) => {
            let mut kinds = Vec::with_capacity(out_len);
            if target_len < 0 {
                kinds.extend(std::iter::repeat(pad_kind).take(pad_count));
                kinds.extend(source_kinds);
            } else {
                kinds.extend(source_kinds);
                kinds.extend(std::iter::repeat(pad_kind).take(pad_count));
            }
            Some(kinds)
        }
        (Some(source_kinds), None) if pad_count == 0 => Some(source_kinds),
        _ => None,
    };
    let pad_nested_value = nested_array_metadata_for_expr(pad_value, module);
    let pad_constant_value = static_scalar_value(pad_value, module);
    let padded_value_constants = source_value_constants.and_then(|source_values| {
        let mut values = Vec::with_capacity(out_len);
        if target_len < 0 {
            values.extend(std::iter::repeat(pad_constant_value.clone()?).take(pad_count));
            values.extend(source_values);
        } else {
            values.extend(source_values);
            values.extend(std::iter::repeat(pad_constant_value?).take(pad_count));
        }
        Some(values)
    });
    let padded_nested_values = source_nested_values.map(|source_values| {
        let mut values = Vec::with_capacity(out_len);
        if target_len < 0 {
            values.extend(std::iter::repeat(pad_nested_value.clone()).take(pad_count));
            values.extend(source_values);
        } else {
            values.extend(source_values);
            values.extend(std::iter::repeat(pad_nested_value).take(pad_count));
        }
        values
    });
    let pad_object_class = object_class_name_for_expr(pad_value, module);
    let padded_object_classes = match (source_object_classes, pad_object_class) {
        (Some(source_classes), pad_class) => {
            let mut classes = Vec::with_capacity(out_len);
            if target_len < 0 {
                classes.extend(std::iter::repeat(pad_class.clone()).take(pad_count));
                classes.extend(source_classes);
            } else {
                classes.extend(source_classes);
                classes.extend(std::iter::repeat(pad_class).take(pad_count));
            }
            Some(classes)
        }
        (None, Some(pad_class)) => {
            let mut classes = Vec::with_capacity(out_len);
            if target_len < 0 {
                classes.extend(std::iter::repeat(Some(pad_class)).take(pad_count));
                classes.extend(std::iter::repeat(None).take(len));
            } else {
                classes.extend(std::iter::repeat(None).take(len));
                classes.extend(std::iter::repeat(Some(pad_class)).take(pad_count));
            }
            Some(classes)
        }
        (None, None) => None,
    };
    module.set_array_value_cell_kinds(name, padded_value_kinds);
    module.set_array_value_constants(name, padded_value_constants);
    module.set_array_nested_value_metadata(name, padded_nested_values);
    module.set_array_object_classes(name, padded_object_classes);
    module.body().line(&format!("i32.const {}", out_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", out_len));
    module.body().line(&format!("local.set ${}_len", name));
    if target_len < 0 {
        for index in 0..pad_count {
            emit_value_array_store_expr(name, index, pad_value, module)?;
        }
        for source_index in 0..len {
            emit_value_pad_source_cell(
                name,
                pad_count + source_index,
                source_layout,
                &source_ptr,
                source_index,
                module,
            );
        }
    } else {
        for source_index in 0..len {
            emit_value_pad_source_cell(name, source_index, source_layout, &source_ptr, source_index, module);
        }
        for index in len..out_len {
            emit_value_array_store_expr(name, index, pad_value, module)?;
        }
    }
    Ok(())
}

pub(super) use super::array_pad_assoc::*;

pub(super) use super::array_pad_dynamic::*;

pub(super) fn static_array_pad_items(items: &[Expr], target_len: i64, value: &Expr) -> Vec<Expr> {
    let out_len = (target_len.unsigned_abs() as usize).max(items.len());
    let pad_count = out_len - items.len();
    let mut out = Vec::with_capacity(out_len);
    if target_len < 0 {
        out.extend((0..pad_count).map(|_| value.clone()));
        out.extend(items.iter().cloned());
    } else {
        out.extend(items.iter().cloned());
        out.extend((0..pad_count).map(|_| value.clone()));
    }
    out
}
