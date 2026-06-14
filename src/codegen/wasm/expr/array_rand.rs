//! Purpose:
//! Emits wasm32-web `array_rand()` lowering for indexed and associative arrays.
//!
//! Called from:
//! - `super::array_aggregates` through its public re-export.
//!
//! Key details:
//! - Scalar calls support count=1; array assignment supports partial/full key arrays.
//! - Associative string keys are returned as boxed mixed cells, while int keys stay scalar.

use super::*;

pub(super) fn emit_array_rand_call(
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    validate_array_rand_count_arg(expr, args, module)?;
    if array_rand_count_requests_array_result(args, module) {
        return emit_array_rand_array_value_to_stack(expr, args, module);
    }
    match &args[0].kind {
        ExprKind::ArrayLiteral(items) => {
            if !array_literal_values_are_side_effect_free(items) {
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web array_rand() over direct literals requires side-effect-free values",
                ));
            }
            emit_array_rand_known_len(args[0].span, items.len(), module)?;
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Array) => {
            return emit_array_rand_from_local(args[0].span, name, module);
        }
        ExprKind::StaticMethodCall { .. } if enum_cases_array_rand_needs_materialized_arg(&args[0], module) => {
            let temp = materialize_enum_cases_array_rand_source(&args[0], "array_rand_enum_cases_source", module)?;
            return emit_array_rand_from_local(args[0].span, &temp, module);
        }
        ExprKind::StaticMethodCall {
            receiver, method, ..
        } if static_method_call_array_return_metadata(receiver, method, module).is_some() => {
            let temp = materialize_static_method_array_rand_source(
                &args[0],
                "array_rand_static_method_source",
                module,
            )?;
            return emit_array_rand_from_local(args[0].span, &temp, module);
        }
        ExprKind::MethodCall { object, method, .. }
            if method_call_array_return_metadata(object, method, module).is_some() =>
        {
            let temp = materialize_static_method_array_rand_source(
                &args[0],
                "array_rand_method_source",
                module,
            )?;
            return emit_array_rand_from_local(args[0].span, &temp, module);
        }
        ExprKind::NullsafeMethodCall { object, method, .. }
            if object_expr_is_known_non_null(object, module)
                && method_call_array_return_metadata(object, method, module).is_some() =>
        {
            let temp = materialize_static_method_array_rand_source(
                &args[0],
                "array_rand_nullsafe_method_source",
                module,
            )?;
            return emit_array_rand_from_local(args[0].span, &temp, module);
        }
        ExprKind::FunctionCall { .. } | ExprKind::ExprCall { .. } => {
            let temp = module
                .next_label("array_rand_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, &args[0], module)?;
            if matches!(args[0].kind, ExprKind::ExprCall { .. })
                && module.array_layout(&temp) == ArrayLayout::Assoc
            {
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web array_rand() over callable-return associative expression arrays requires key metadata support",
                ));
            }
            return emit_array_rand_from_local(args[0].span, &temp, module);
        }
        ExprKind::ArrayAccess { .. } if nested_array_metadata_for_access_expr(&args[0], module).is_some() => {
            let temp = materialize_nested_array_rand_source(&args[0], "array_rand_nested_source", module)?;
            return emit_array_rand_from_local(args[0].span, &temp, module);
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            let temp = module
                .next_label("array_rand_assoc_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_assoc_array_items_assign(&temp, items, module)?;
            return emit_array_rand_from_local(args[0].span, &temp, module);
        }
        _ => {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_rand() currently requires an indexed array literal or local",
            ));
        }
    }
    Ok(ValueKind::Int)
}

fn emit_array_rand_array_value_to_stack(
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let temp = module
        .next_label("array_rand_value_result")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_array_rand_assign(&temp, expr, args, module)?;
    module.body().line(&format!("local.get ${}_ptr", temp));
    module.body().line(&format!("local.get ${}_len", temp));
    Ok(ValueKind::Array)
}

pub(super) fn emit_array_rand_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if args.len() != 2 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web array_rand() array assignment currently requires an explicit count",
        ));
    }
    match &args[0].kind {
        ExprKind::ArrayLiteral(items) => {
            if !array_literal_values_are_side_effect_free(items) {
                return Err(CompileError::new(
                    args[0].span,
                    "wasm32-web array_rand() over direct literals requires side-effect-free values",
                ));
            }
            emit_array_rand_full_index_keys_assign(name, args[0].span, items.len(), &args[1], module)
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            let temp = module
                .next_label("array_rand_assoc_array_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_assoc_array_items_assign(&temp, items, module)?;
            emit_array_rand_full_assoc_keys_from_local(name, args[0].span, &temp, &args[1], module)
        }
        ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
            if module.array_layout(source) == ArrayLayout::Assoc {
                emit_array_rand_full_assoc_keys_from_local(name, args[0].span, source, &args[1], module)
            } else {
                emit_array_rand_full_index_keys_from_local(name, args[0].span, source, &args[1], module)
            }
        }
        ExprKind::StaticMethodCall { .. } if enum_cases_array_rand_needs_materialized_arg(&args[0], module) => {
            let temp = materialize_enum_cases_array_rand_source(&args[0], "array_rand_array_enum_cases_source", module)?;
            emit_array_rand_full_index_keys_from_local(name, args[0].span, &temp, &args[1], module)
        }
        ExprKind::StaticMethodCall {
            receiver, method, ..
        } if static_method_call_array_return_metadata(receiver, method, module).is_some() => {
            let temp = materialize_static_method_array_rand_source(
                &args[0],
                "array_rand_array_static_method_source",
                module,
            )?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                emit_array_rand_full_assoc_keys_from_local(name, args[0].span, &temp, &args[1], module)
            } else {
                emit_array_rand_full_index_keys_from_local(name, args[0].span, &temp, &args[1], module)
            }
        }
        ExprKind::MethodCall { object, method, .. }
            if method_call_array_return_metadata(object, method, module).is_some() =>
        {
            let temp = materialize_static_method_array_rand_source(
                &args[0],
                "array_rand_array_method_source",
                module,
            )?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                emit_array_rand_full_assoc_keys_from_local(name, args[0].span, &temp, &args[1], module)
            } else {
                emit_array_rand_full_index_keys_from_local(name, args[0].span, &temp, &args[1], module)
            }
        }
        ExprKind::NullsafeMethodCall { object, method, .. }
            if object_expr_is_known_non_null(object, module)
                && method_call_array_return_metadata(object, method, module).is_some() =>
        {
            let temp = materialize_static_method_array_rand_source(
                &args[0],
                "array_rand_array_nullsafe_method_source",
                module,
            )?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                emit_array_rand_full_assoc_keys_from_local(name, args[0].span, &temp, &args[1], module)
            } else {
                emit_array_rand_full_index_keys_from_local(name, args[0].span, &temp, &args[1], module)
            }
        }
        ExprKind::FunctionCall { .. } | ExprKind::ExprCall { .. } => {
            let temp = module
                .next_label("array_rand_array_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, &args[0], module)?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                if matches!(args[0].kind, ExprKind::ExprCall { .. }) {
                    return Err(CompileError::new(
                        args[0].span,
                        "wasm32-web array_rand() over callable-return associative expression arrays requires key metadata support",
                    ));
                }
                emit_array_rand_full_assoc_keys_from_local(name, args[0].span, &temp, &args[1], module)
            } else {
                emit_array_rand_full_index_keys_from_local(name, args[0].span, &temp, &args[1], module)
            }
        }
        _ => Err(CompileError::new(
            args[0].span,
            "wasm32-web array_rand() array assignment currently supports known indexed or associative array sources",
        )),
    }
}

fn enum_cases_array_rand_needs_materialized_arg(arg: &Expr, module: &WasmModule) -> bool {
    let ExprKind::StaticMethodCall {
        receiver,
        method,
        args,
    } = &arg.kind
    else {
        return false;
    };
    method.eq_ignore_ascii_case("cases")
        && args.is_empty()
        && module
            .class_name_for_receiver(receiver)
            .and_then(|class_name| module.enum_case_names(&class_name))
            .is_some()
}

fn materialize_enum_cases_array_rand_source(
    source: &Expr,
    label: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let temp = module
        .next_label(label)
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_array_assign(&temp, source, module)?;
    Ok(temp)
}

fn materialize_static_method_array_rand_source(
    source: &Expr,
    label: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let temp = module
        .next_label(label)
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_array_assign(&temp, source, module)?;
    Ok(temp)
}

fn materialize_nested_array_rand_source(
    source: &Expr,
    label: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let Some(metadata) = nested_array_metadata_for_access_expr(source, module) else {
        return Err(CompileError::new(
            source.span,
            "wasm32-web array_rand() requires nested array metadata",
        ));
    };
    let temp = module.next_label(label).trim_start_matches('$').to_string();
    module.declare_array_local(temp.clone());
    match emit_expr(source, module)? {
        ValueKind::Array => {
            module.body().line(&format!("local.set ${}_len", temp));
            module.body().line(&format!("local.set ${}_ptr", temp));
        }
        _ => {
            return Err(CompileError::new(
                source.span,
                "wasm32-web array_rand() expected a nested array value",
            ));
        }
    }
    let key_values = metadata.key_values.clone();
    module.set_array_layout(&temp, metadata.layout);
    if metadata.layout != ArrayLayout::Assoc || key_values.is_some() {
        module.set_array_length(&temp, metadata.len);
    }
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

fn validate_array_rand_count_arg(
    expr: &Expr,
    args: &[Expr],
    module: &WasmModule,
) -> Result<(), CompileError> {
    if args.is_empty() || args.len() > 2 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_rand() expects one or two arguments",
        ));
    }
    let Some(count) = args.get(1) else {
        return Ok(());
    };
    if array_rand_full_count_value(count, module).is_none()
        && array_rand_dynamic_full_count_source(&args[0], count, module).is_none()
    {
        return Err(CompileError::new(
            count.span,
            "wasm32-web array_rand() currently requires a static or known count argument",
        ));
    }
    Ok(())
}

fn emit_array_rand_from_local(
    span: Span,
    name: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if module.array_layout(name) == ArrayLayout::Assoc {
        if module
            .array_key_kinds(name)
            .is_some_and(|kinds| kinds.iter().all(|kind| *kind == AssocKeyKind::Int))
        {
            emit_array_rand_assoc_int_key_to_stack(span, name, module)?;
            return Ok(ValueKind::Int);
        }
        emit_array_rand_assoc_key_to_stack(span, name, module)?;
        return Ok(ValueKind::Mixed);
    }
    if let Some(len) = module.array_length(name) {
        emit_array_rand_known_len(span, len, module)?;
    } else {
        emit_array_rand_runtime_len(&format!("${}_len", name), module);
    }
    Ok(ValueKind::Int)
}

fn emit_array_rand_full_index_keys_from_local(
    target: &str,
    span: Span,
    source: &str,
    count: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) == ArrayLayout::Assoc {
        return Err(CompileError::new(
            span,
            "wasm32-web array_rand() array assignment currently supports indexed array sources",
        ));
    }
    let Some(len) = module.array_length(source) else {
        if array_rand_count_matches_local(count, source) {
            return emit_array_rand_runtime_full_index_keys_assign(target, source, module);
        }
        return Err(CompileError::new(
            span,
            "wasm32-web array_rand() full-count array assignment currently requires a known source length",
        ));
    };
    emit_array_rand_full_index_keys_assign(target, span, len, count, module)
}

fn emit_array_rand_full_index_keys_assign(
    target: &str,
    span: Span,
    len: usize,
    count: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if len == 0 {
        return Err(CompileError::new(
            span,
            "wasm32-web array_rand() on an empty array is unsupported",
        ));
    }
    let Some(count_value) = array_rand_full_count_value(count, module) else {
        return Err(CompileError::new(
            count.span,
            "wasm32-web array_rand() full-count array assignment currently requires a static or known count",
        ));
    };
    if count_value < 1 || count_value > len as i64 {
        return Err(CompileError::new(
            count.span,
            "wasm32-web array_rand() array assignment count must be between 1 and the source length",
        ));
    }
    let count_len = usize::try_from(count_value).expect("validated positive array_rand count");
    if count_len < len {
        return emit_array_rand_partial_index_keys_assign(target, len, count_len, module);
    }
    module.set_array_length(target, count_len);
    module.set_array_layout(target, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(target, None);
    module.set_array_nested_value_metadata(target, None);
    module.body().line(&format!("i32.const {}", count_len));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", target));
    module.body().line(&format!("i32.const {}", count_len));
    module.body().line(&format!("local.set ${}_len", target));
    for index in 0..count_len {
        module.body().line(&format!("local.get ${}_ptr", target));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line(&format!("i64.const {}", index));
        module.body().line("i64.store");
    }
    Ok(())
}

fn emit_array_rand_runtime_full_index_keys_assign(
    target: &str,
    source: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let index = module.next_label("array_rand_full_runtime_index");
    let done_label = module.next_label("array_rand_full_runtime_done");
    let loop_label = module.next_label("array_rand_full_runtime_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.clear_array_length(target);
    module.set_array_layout(target, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(target, None);
    module.set_array_runtime_value_cell_kind(target, None);
    module.set_array_nested_value_metadata(target, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", target));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set ${}_len", target));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", target));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i64.extend_i32_u");
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_array_rand_partial_index_keys_assign(
    target: &str,
    len: usize,
    count: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let pool = module.next_label("array_rand_key_pool");
    let selected_offset = module.next_label("array_rand_selected_offset");
    let swap_value = module.next_label("array_rand_swap_value");
    let left_value = module.next_label("array_rand_left_value");
    let right_value = module.next_label("array_rand_right_value");
    module.declare_i32_local(pool.trim_start_matches('$').to_string());
    module.declare_i32_local(selected_offset.trim_start_matches('$').to_string());
    for local in [&swap_value, &left_value, &right_value] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }

    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set {}", pool));
    for index in 0..len {
        module.body().line(&format!("local.get {}", pool));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line(&format!("i64.const {}", index));
        module.body().line("i64.store");
    }
    for index in 0..count {
        emit_array_rand_pool_swap(index, len, &pool, &selected_offset, &swap_value, module);
    }
    emit_array_rand_sort_selected_keys(count, &pool, &left_value, &right_value, module);

    module.set_array_length(target, count);
    module.set_array_layout(target, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(target, None);
    module.set_array_nested_value_metadata(target, None);
    module.body().line(&format!("i32.const {}", count));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", target));
    module.body().line(&format!("i32.const {}", count));
    module.body().line(&format!("local.set ${}_len", target));
    for index in 0..count {
        module.body().line(&format!("local.get ${}_ptr", target));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line(&format!("local.get {}", pool));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line("i64.load");
        module.body().line("i64.store");
    }
    Ok(())
}

fn emit_array_rand_pool_swap(
    index: usize,
    len: usize,
    pool: &str,
    selected_offset: &str,
    swap_value: &str,
    module: &mut WasmModule,
) {
    module.body().line("call $host_random_u32");
    module.body().line(&format!("i32.const {}", len - index));
    module.body().line("i32.rem_u");
    module.body().line(&format!("i32.const {}", index));
    module.body().line("i32.add");
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line(&format!("local.set {}", selected_offset));
    module.body().line(&format!("local.get {}", pool));
    module.body().line(&format!("local.get {}", selected_offset));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", swap_value));
    module.body().line(&format!("local.get {}", pool));
    module.body().line(&format!("local.get {}", selected_offset));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", pool));
    module.body().line(&format!("i32.const {}", index * 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", pool));
    module.body().line(&format!("i32.const {}", index * 8));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", swap_value));
    module.body().line("i64.store");
}

fn emit_array_rand_sort_selected_keys(
    count: usize,
    pool: &str,
    left_value: &str,
    right_value: &str,
    module: &mut WasmModule,
) {
    for _ in 0..count {
        for index in 0..count.saturating_sub(1) {
            emit_array_rand_compare_swap_selected_key(index, pool, left_value, right_value, module);
        }
    }
}

fn emit_array_rand_compare_swap_selected_key(
    index: usize,
    pool: &str,
    left_value: &str,
    right_value: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", pool));
    module.body().line(&format!("i32.const {}", index * 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", left_value));
    module.body().line(&format!("local.get {}", pool));
    module.body().line(&format!("i32.const {}", (index + 1) * 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", right_value));
    module.body().line(&format!("local.get {}", left_value));
    module.body().line(&format!("local.get {}", right_value));
    module.body().line("i64.gt_s");
    module.body().open("if");
    module.body().line(&format!("local.get {}", pool));
    module.body().line(&format!("i32.const {}", index * 8));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", right_value));
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", pool));
    module.body().line(&format!("i32.const {}", (index + 1) * 8));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", left_value));
    module.body().line("i64.store");
    module.body().close("end");
}

fn emit_array_rand_full_assoc_keys_from_local(
    target: &str,
    span: Span,
    source: &str,
    count: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            span,
            "wasm32-web array_rand() full-count associative assignment currently requires a known source length",
        ));
    };
    if len == 0 {
        return Err(CompileError::new(
            span,
            "wasm32-web array_rand() on an empty array is unsupported",
        ));
    }
    let Some(count_value) = array_rand_full_count_value(count, module) else {
        return Err(CompileError::new(
            count.span,
            "wasm32-web array_rand() full-count associative assignment currently requires a static or known count",
        ));
    };
    if count_value < 1 || count_value > len as i64 {
        return Err(CompileError::new(
            count.span,
            "wasm32-web array_rand() associative array assignment count must be between 1 and the source length",
        ));
    }
    let Some(key_kinds) = module.array_key_kinds(source).map(|kinds| kinds.to_vec()) else {
        return Err(CompileError::new(
            span,
            "wasm32-web array_rand() full-count associative assignment requires key metadata",
        ));
    };
    let count_len = usize::try_from(count_value).expect("validated positive array_rand count");
    if count_len < len {
        return emit_array_rand_partial_assoc_keys_assign(target, source, &key_kinds, count_len, module);
    }
    if key_kinds.iter().all(|kind| *kind == AssocKeyKind::Int) {
        return emit_array_rand_full_assoc_int_keys_assign(target, source, &key_kinds, module);
    }
    emit_array_rand_full_assoc_value_keys_assign(target, source, &key_kinds, module)
}

fn emit_array_rand_partial_assoc_keys_assign(
    target: &str,
    source: &str,
    key_kinds: &[AssocKeyKind],
    count: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let len = key_kinds.len();
    let pool = module.next_label("array_rand_assoc_key_pool");
    let selected_offset = module.next_label("array_rand_assoc_selected_offset");
    let swap_value = module.next_label("array_rand_assoc_swap_value");
    let left_value = module.next_label("array_rand_assoc_left_value");
    let right_value = module.next_label("array_rand_assoc_right_value");
    let selected_index = module.next_label("array_rand_assoc_selected_index");
    for local in [&pool, &selected_offset, &selected_index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&swap_value, &left_value, &right_value] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }

    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set {}", pool));
    for index in 0..len {
        module.body().line(&format!("local.get {}", pool));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line(&format!("i64.const {}", index));
        module.body().line("i64.store");
    }
    for index in 0..count {
        emit_array_rand_pool_swap(index, len, &pool, &selected_offset, &swap_value, module);
    }
    emit_array_rand_sort_selected_keys(count, &pool, &left_value, &right_value, module);

    if key_kinds.iter().all(|kind| *kind == AssocKeyKind::Int) {
        module.set_array_length(target, count);
        module.set_array_layout(target, ArrayLayout::CompactInt);
        module.set_array_value_cell_kinds(target, None);
        module.set_array_runtime_value_cell_kind(target, None);
        module.set_array_nested_value_metadata(target, None);
        module.body().line(&format!("i32.const {}", count));
        module.body().line("call $__rt_alloc_indexed_slots");
        module.body().line(&format!("local.set ${}_ptr", target));
        module.body().line(&format!("i32.const {}", count));
        module.body().line(&format!("local.set ${}_len", target));
        for index in 0..count {
            emit_array_rand_selected_assoc_index(index, &pool, &selected_index, module);
            module.body().line(&format!("local.get ${}_ptr", target));
            module.body().line(&format!("i32.const {}", index * 8));
            module.body().line("i32.add");
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get {}", selected_index));
            module.body().line("call $__rt_assoc_entry");
            module.body().line("call $__rt_assoc_key_payload_i64");
            module.body().line("i64.store");
        }
        return Ok(());
    }
    module.set_array_length(target, count);
    module.set_array_layout(target, ArrayLayout::Value);
    module.set_array_key_kinds(target, Some(vec![AssocKeyKind::Int; count]));
    if key_kinds.iter().all(|kind| *kind == AssocKeyKind::Str) {
        module.set_array_value_cell_kinds(target, Some(vec![ValueCellKind::Str; count]));
        module.set_array_runtime_value_cell_kind(target, Some(ValueCellKind::Str));
    } else {
        module.set_array_value_cell_kinds(target, None);
        module.set_array_runtime_value_cell_kind(target, None);
    }
    module.set_array_nested_value_metadata(target, None);
    module.body().line(&format!("i32.const {}", count));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", target));
    module.body().line(&format!("i32.const {}", count));
    module.body().line(&format!("local.set ${}_len", target));
    for index in 0..count {
        emit_array_rand_selected_assoc_index(index, &pool, &selected_index, module);
        module.body().line(&format!("local.get ${}_ptr", target));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("local.get {}", selected_index));
        module.body().line("call $__rt_assoc_entry");
        emit_array_rand_assoc_key_entry_to_value_cell(module);
    }
    Ok(())
}

fn emit_array_rand_assoc_key_entry_to_value_cell(module: &mut WasmModule) {
    let target_cell = module.next_label("array_rand_assoc_target_cell");
    let entry = module.next_label("array_rand_assoc_selected_entry");
    for local in [&target_cell, &entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.set {}", target_cell));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line("call $__rt_value_store_string");
    module.body().close("else");
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_key_payload_i64");
    module.body().line("call $__rt_value_store_int");
    module.body().close("end");
}

fn emit_array_rand_selected_assoc_index(
    index: usize,
    pool: &str,
    selected_index: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", pool));
    module.body().line(&format!("i32.const {}", index * 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", selected_index));
}

fn emit_array_rand_full_assoc_int_keys_assign(
    target: &str,
    source: &str,
    key_kinds: &[AssocKeyKind],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let len = key_kinds.len();
    module.set_array_length(target, len);
    module.set_array_layout(target, ArrayLayout::CompactInt);
    module.set_array_value_cell_kinds(target, None);
    module.set_array_nested_value_metadata(target, None);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", target));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", target));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", target));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line("call $__rt_assoc_key_payload_i64");
        module.body().line("i64.store");
    }
    Ok(())
}

fn emit_array_rand_full_assoc_value_keys_assign(
    target: &str,
    source: &str,
    key_kinds: &[AssocKeyKind],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let len = key_kinds.len();
    module.set_array_length(target, len);
    module.set_array_layout(target, ArrayLayout::Value);
    module.set_array_value_cell_kinds(
        target,
        Some(
            key_kinds
                .iter()
                .map(|kind| match kind {
                    AssocKeyKind::Int => ValueCellKind::Int,
                    AssocKeyKind::Str => ValueCellKind::Str,
                })
                .collect(),
        ),
    );
    module.set_array_nested_value_metadata(target, None);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", target));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", target));
    for (index, key_kind) in key_kinds.iter().enumerate() {
        module.body().line(&format!("local.get ${}_ptr", target));
        module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        match key_kind {
            AssocKeyKind::Int => {
                module.body().line(&format!("local.get ${}_ptr", source));
                module.body().line(&format!("i32.const {}", index));
                module.body().line("call $__rt_assoc_entry");
                module.body().line("call $__rt_assoc_key_payload_i64");
                module.body().line("call $__rt_value_store_int");
            }
            AssocKeyKind::Str => {
                module.body().line(&format!("local.get ${}_ptr", source));
                module.body().line(&format!("i32.const {}", index));
                module.body().line("call $__rt_assoc_entry");
                module.body().line("i32.const 8");
                module.body().line("i32.add");
                module.body().line("i32.load");
                module.body().line(&format!("local.get ${}_ptr", source));
                module.body().line(&format!("i32.const {}", index));
                module.body().line("call $__rt_assoc_entry");
                module.body().line("i32.const 12");
                module.body().line("i32.add");
                module.body().line("i32.load");
                module.body().line("call $__rt_value_store_string");
            }
        }
    }
    Ok(())
}

fn emit_array_rand_assoc_int_key_to_stack(
    span: Span,
    name: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let index = module.next_label("array_rand_assoc_int_index");
    let entry = module.next_label("array_rand_assoc_int_entry");
    for local in [&index, &entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_array_rand_index_to_i32(span, name, &index, module)?;
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_key_payload_i64");
    Ok(())
}

fn array_rand_full_count_value(count: &Expr, module: &WasmModule) -> Option<i64> {
    static_or_const_or_i64_local_value(count, module).or_else(|| known_count_call_value(count, module))
}

fn array_rand_count_requests_array_result(args: &[Expr], module: &WasmModule) -> bool {
    let Some(count) = args.get(1) else {
        return false;
    };
    array_rand_full_count_value(count, module).is_some_and(|value| value != 1)
        || array_rand_dynamic_full_count_source(&args[0], count, module).is_some()
}

fn array_rand_dynamic_full_count_source<'a>(
    source: &'a Expr,
    count: &Expr,
    module: &WasmModule,
) -> Option<&'a str> {
    let ExprKind::Variable(source_name) = &source.kind else {
        return None;
    };
    if !array_rand_count_matches_local(count, source_name) {
        return None;
    }
    if module.local_kind(source_name) == Some(LocalKind::Array)
        && module.array_layout(source_name) != ArrayLayout::Assoc
        && module.array_length(source_name).is_none()
    {
        Some(source_name)
    } else {
        None
    }
}

fn array_rand_count_matches_local(count: &Expr, source: &str) -> bool {
    let ExprKind::FunctionCall { name, args } = &count.kind else {
        return false;
    };
    if !name.eq_ignore_ascii_case("count") || args.len() != 1 {
        return false;
    }
    matches!(&args[0].kind, ExprKind::Variable(name) if name == source)
}

fn known_count_call_value(expr: &Expr, module: &WasmModule) -> Option<i64> {
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("count") || args.len() != 1 {
        return None;
    }
    known_array_len_for_count(&args[0], module)
}

fn known_array_len_for_count(expr: &Expr, module: &WasmModule) -> Option<i64> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => Some(items.len() as i64),
        ExprKind::ArrayLiteralAssoc(items) => Some(items.len() as i64),
        ExprKind::Variable(name) => module.array_length(name).map(|len| len as i64),
        ExprKind::StaticMethodCall {
            receiver, method, ..
        } => static_method_call_array_return_metadata(receiver, method, module)
            .and_then(|metadata| metadata.len)
            .map(|len| len as i64)
            .or_else(|| static_enum_cases_len(expr, module).map(|len| len as i64)),
        ExprKind::MethodCall { object, method, .. }
        | ExprKind::NullsafeMethodCall { object, method, .. } => {
            method_call_array_return_metadata(object, method, module)
                .and_then(|metadata| metadata.len)
                .map(|len| len as i64)
        }
        _ => None,
    }
}

fn emit_array_rand_assoc_key_to_stack(
    span: Span,
    name: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let result = module
        .next_label("array_rand_assoc_key")
        .trim_start_matches('$')
        .to_string();
    let index = module.next_label("array_rand_assoc_index");
    let entry = module.next_label("array_rand_assoc_entry");
    let key_kind = module.next_label("array_rand_assoc_key_kind");
    for local in [&index, &entry, &key_kind] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i32_local(result.clone());
    emit_alloc_mixed_cell(&result, module);
    emit_array_rand_index_to_i32(span, name, &index, module)?;
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", key_kind));
    module.body().line(&format!("local.get {}", key_kind));
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get ${}", result));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line("call $__rt_value_store_string");
    module.body().close("else");
    module.body().line(&format!("local.get ${}", result));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_key_payload_i64");
    module.body().line("call $__rt_value_store_int");
    module.body().close("end");
    module.body().line(&format!("local.get ${}", result));
    Ok(())
}

fn emit_array_rand_index_to_i32(
    span: Span,
    name: &str,
    index: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(len) = module.array_length(name) {
        if len == 0 {
            return Err(CompileError::new(
                span,
                "wasm32-web array_rand() on an empty array is unsupported",
            ));
        }
        module.body().line("call $host_random_u32");
        module.body().line(&format!("i32.const {}", len));
        module.body().line("i32.rem_u");
        module.body().line(&format!("local.set {}", index));
    } else {
        module.body().line(&format!("local.get ${}_len", name));
        module.body().line("i32.eqz");
        module.body().open("if");
        module.body().line("unreachable");
        module.body().close("end");
        module.body().line("call $host_random_u32");
        module.body().line(&format!("local.get ${}_len", name));
        module.body().line("i32.rem_u");
        module.body().line(&format!("local.set {}", index));
    }
    Ok(())
}

fn array_literal_values_are_side_effect_free(items: &[Expr]) -> bool {
    items.iter().all(|item| {
        matches!(
            item.kind,
            ExprKind::IntLiteral(_)
                | ExprKind::FloatLiteral(_)
                | ExprKind::StringLiteral(_)
                | ExprKind::BoolLiteral(_)
                | ExprKind::Null
        )
    })
}

fn emit_array_rand_known_len(
    span: Span,
    len: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if len == 0 {
        return Err(CompileError::new(
            span,
            "wasm32-web array_rand() on an empty array is unsupported",
        ));
    }
    module.body().line("call $host_random_u32");
    module.body().line(&format!("i32.const {}", len));
    module.body().line("i32.rem_u");
    module.body().line("i64.extend_i32_u");
    Ok(())
}

fn emit_array_rand_runtime_len(len_local: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", len_local));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line("call $host_random_u32");
    module.body().line(&format!("local.get {}", len_local));
    module.body().line("i32.rem_u");
    module.body().line("i64.extend_i32_u");
}
