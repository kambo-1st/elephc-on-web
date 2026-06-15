//! Purpose:
//! Lowers wasm32-web indexed, value-cell, and associative array merge helpers.
//! Keeps merge result construction and PHP key normalization out of expr.rs.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` array assignment lowering.
//!
//! Key details:
//! - Helpers preserve PHP key normalization, runtime value-cell ownership, and metadata.

use super::*;
use super::array_merge_assoc_emitters::*;
use super::array_merge_key_set::{
    static_assoc_key_value, StaticAssocKey,
};
use super::array_merge_metadata::*;
use super::array_merge_value::*;

pub(super) fn emit_indexed_array_merge_assign(
    name: &str,
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if args
        .iter()
        .any(|arg| enum_cases_array_merge_needs_materialized_arg(arg, module))
    {
        let args = materialize_enum_cases_array_merge_args(args, module)?;
        return emit_indexed_array_merge_assign(name, expr, &args, module);
    }
    if args
        .iter()
        .any(|arg| nested_array_merge_needs_materialized_arg(arg, module))
    {
        let args = materialize_nested_array_merge_args(args, module)?;
        return emit_indexed_array_merge_assign(name, expr, &args, module);
    }
    if args
        .iter()
        .any(|arg| unknown_mixed_array_merge_needs_materialized_arg(arg, module))
    {
        let args = materialize_unknown_mixed_array_merge_args(args, module)?;
        return emit_indexed_array_merge_assign(name, expr, &args, module);
    }
    if args
        .iter()
        .any(|arg| assoc_array_merge_needs_materialized_arg(arg, module))
    {
        let args = materialize_assoc_array_merge_args(args, module)?;
        return emit_indexed_array_merge_assign(name, expr, &args, module);
    }
    if args
        .iter()
        .any(|arg| method_array_merge_needs_materialized_arg(arg, module))
    {
        let args = materialize_method_array_merge_args(args, module)?;
        return emit_indexed_array_merge_assign(name, expr, &args, module);
    }
    if args
        .iter()
        .any(|arg| static_method_array_merge_needs_materialized_arg(arg, module))
    {
        let args = materialize_static_method_array_merge_args(args, module)?;
        return emit_indexed_array_merge_assign(name, expr, &args, module);
    }
    if args
        .iter()
        .any(|arg| dynamic_static_method_array_merge_needs_materialized_arg(arg, module))
    {
        let args = materialize_dynamic_static_method_array_merge_args(args, module)?;
        return emit_indexed_array_merge_assign(name, expr, &args, module);
    }
    if args
        .iter()
        .any(|arg| matches_assoc_array_expr(arg, module) || matches!(arg.kind, ExprKind::ArrayLiteralAssoc(_)))
    {
        if args
            .iter()
            .all(|arg| matches!(arg.kind, ExprKind::ArrayLiteralAssoc(_)))
        {
            let items = static_assoc_array_merge_items(expr, args, module)?;
            return emit_assoc_array_items_assign(name, &items, module);
        }
        return emit_assoc_array_merge_assign(name, expr, args, module);
    }
    if indexed_array_merge_needs_dynamic_value_layout(args, module) {
        return emit_dynamic_value_array_merge_assign(name, expr, args, module);
    }
    if indexed_array_merge_needs_value_layout(args, module) {
        return emit_value_array_merge_assign(name, expr, args, module);
    }
    if args.iter().any(|arg| indexed_array_merge_needs_dynamic(arg, module)) {
        return emit_dynamic_indexed_array_merge_assign(name, expr, args, module);
    }
    let mut sources = Vec::new();
    let mut total_len = 0usize;
    for arg in args {
        match &arg.kind {
            ExprKind::ArrayLiteral(items) => {
                total_len += items.len();
                sources.push(ArrayCopySource::Static(items.clone()));
            }
            ExprKind::ArrayLiteralAssoc(_) => return Err(array_unsupported(arg)),
            ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
                let Some(len) = module.array_length(source) else {
                    return Err(CompileError::new(
                        arg.span,
                        "wasm32-web array_merge() requires known indexed array lengths",
                    ));
                };
                total_len += len;
                let source_ptr = preserve_array_ptr(source, "array_merge", module);
                sources.push(ArrayCopySource::Runtime { ptr: source_ptr, len });
            }
            _ => {
                return Err(CompileError::new(
                    arg.span,
                    "wasm32-web array_merge() currently supports indexed array values only",
                ));
            }
        }
    }
    emit_array_alloc_prelude(name, total_len, module);
    let mut out_index = 0usize;
    for source in sources {
        match source {
            ArrayCopySource::Static(items) => {
                for item in items {
                    emit_array_store_expr(name, out_index, &item, module)?;
                    out_index += 1;
                }
            }
            ArrayCopySource::Runtime { ptr, len } => {
                for source_index in 0..len {
                    emit_array_store_load(name, out_index, &ptr, source_index, module);
                    out_index += 1;
                }
            }
        }
    }
    let _ = expr;
    Ok(())
}

fn method_array_merge_needs_materialized_arg(arg: &Expr, module: &WasmModule) -> bool {
    match &arg.kind {
        ExprKind::MethodCall { object, method, .. }
        | ExprKind::NullsafeMethodCall { object, method, .. } => {
            method_call_array_return_metadata(object, method, module).is_some()
        }
        _ => false,
    }
}

fn materialize_method_array_merge_args(
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<Vec<Expr>, CompileError> {
    let mut materialized = Vec::with_capacity(args.len());
    for arg in args {
        match &arg.kind {
            ExprKind::MethodCall { object, method, .. }
            | ExprKind::NullsafeMethodCall { object, method, .. }
                if method_call_array_return_metadata(object, method, module).is_some() =>
            {
                let metadata = method_call_array_return_metadata(object, method, module)
                    .expect("guarded method array return metadata");
                let temp = module
                    .next_label("array_merge_method_source")
                    .trim_start_matches('$')
                    .to_string();
                module.declare_array_local(temp.clone());
                match emit_expr(arg, module)? {
                    ValueKind::Array => {
                        module.body().line(&format!("local.set ${}_len", temp));
                        module.body().line(&format!("local.set ${}_ptr", temp));
                    }
                    _ => {
                        return Err(CompileError::new(
                            arg.span,
                            "wasm32-web array_merge() expected an array-returning method value",
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
                materialized.push(Expr::new(ExprKind::Variable(temp), arg.span));
            }
            _ => materialized.push(arg.clone()),
        }
    }
    Ok(materialized)
}

fn static_method_array_merge_needs_materialized_arg(arg: &Expr, module: &WasmModule) -> bool {
    match &arg.kind {
        ExprKind::StaticMethodCall { receiver, method, .. } => {
            static_method_call_array_return_metadata(receiver, method, module).is_some()
        }
        _ => false,
    }
}

fn materialize_static_method_array_merge_args(
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<Vec<Expr>, CompileError> {
    let mut materialized = Vec::with_capacity(args.len());
    for arg in args {
        match &arg.kind {
            ExprKind::StaticMethodCall { receiver, method, .. }
                if static_method_call_array_return_metadata(receiver, method, module).is_some() =>
            {
                let metadata = static_method_call_array_return_metadata(receiver, method, module)
                    .expect("guarded static method array return metadata");
                let temp = module
                    .next_label("array_merge_static_method_source")
                    .trim_start_matches('$')
                    .to_string();
                module.declare_array_local(temp.clone());
                match emit_expr(arg, module)? {
                    ValueKind::Array => {
                        module.body().line(&format!("local.set ${}_len", temp));
                        module.body().line(&format!("local.set ${}_ptr", temp));
                    }
                    _ => {
                        return Err(CompileError::new(
                            arg.span,
                            "wasm32-web array_merge() expected an array-returning static method value",
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
                materialized.push(Expr::new(ExprKind::Variable(temp), arg.span));
            }
            _ => materialized.push(arg.clone()),
        }
    }
    Ok(materialized)
}

fn dynamic_static_method_array_merge_needs_materialized_arg(
    arg: &Expr,
    module: &WasmModule,
) -> bool {
    match &arg.kind {
        ExprKind::DynamicStaticMethodCall { receiver, method, .. } => {
            dynamic_static_method_call_array_return_metadata(receiver, method, module).is_some()
        }
        _ => false,
    }
}

fn materialize_dynamic_static_method_array_merge_args(
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<Vec<Expr>, CompileError> {
    let mut materialized = Vec::with_capacity(args.len());
    for arg in args {
        match &arg.kind {
            ExprKind::DynamicStaticMethodCall { receiver, method, .. }
                if dynamic_static_method_call_array_return_metadata(receiver, method, module)
                    .is_some() =>
            {
                let metadata = dynamic_static_method_call_array_return_metadata(
                    receiver,
                    method,
                    module,
                )
                .expect("guarded dynamic static method array return metadata");
                let temp = module
                    .next_label("array_merge_dynamic_static_method_source")
                    .trim_start_matches('$')
                    .to_string();
                module.declare_array_local(temp.clone());
                match emit_expr(arg, module)? {
                    ValueKind::Array => {
                        module.body().line(&format!("local.set ${}_len", temp));
                        module.body().line(&format!("local.set ${}_ptr", temp));
                    }
                    _ => {
                        return Err(CompileError::new(
                            arg.span,
                            "wasm32-web array_merge() expected an array-returning dynamic static method value",
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
                materialized.push(Expr::new(ExprKind::Variable(temp), arg.span));
            }
            _ => materialized.push(arg.clone()),
        }
    }
    Ok(materialized)
}

fn enum_cases_array_merge_needs_materialized_arg(arg: &Expr, module: &WasmModule) -> bool {
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

fn materialize_enum_cases_array_merge_args(
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<Vec<Expr>, CompileError> {
    args.iter()
        .map(|arg| {
            if !enum_cases_array_merge_needs_materialized_arg(arg, module) {
                return Ok(arg.clone());
            }
            let temp = module
                .next_label("array_merge_enum_cases")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, arg, module)?;
            Ok(Expr::new(ExprKind::Variable(temp), arg.span))
        })
        .collect()
}

fn nested_array_merge_needs_materialized_arg(arg: &Expr, module: &WasmModule) -> bool {
    matches!(&arg.kind, ExprKind::ArrayAccess { .. })
        && nested_array_metadata_for_access_expr(arg, module).is_some()
}

fn materialize_nested_array_merge_args(
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<Vec<Expr>, CompileError> {
    args.iter()
        .map(|arg| {
            if !nested_array_merge_needs_materialized_arg(arg, module) {
                return Ok(arg.clone());
            }
            let temp = materialize_nested_array_merge_source(arg, module)?;
            Ok(Expr::new(ExprKind::Variable(temp), arg.span))
        })
        .collect()
}

fn materialize_nested_array_merge_source(
    source: &Expr,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let Some(metadata) = nested_array_metadata_for_access_expr(source, module) else {
        return Err(CompileError::new(
            source.span,
            "wasm32-web array_merge() requires nested array metadata",
        ));
    };
    let temp = module
        .next_label("array_merge_nested_source")
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
                "wasm32-web array_merge() expected a nested array value",
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

fn unknown_mixed_array_merge_needs_materialized_arg(arg: &Expr, module: &WasmModule) -> bool {
    matches!(
        &arg.kind,
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Mixed)
                && module.mixed_value_cell_kind(source).is_none()
    )
}

fn materialize_unknown_mixed_array_merge_args(
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<Vec<Expr>, CompileError> {
    args.iter()
        .map(|arg| {
            let ExprKind::Variable(source) = &arg.kind else {
                return Ok(arg.clone());
            };
            if !unknown_mixed_array_merge_needs_materialized_arg(arg, module) {
                return Ok(arg.clone());
            }
            let temp = module
                .next_label("unknown_mixed_array_merge_arg")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_unknown_mixed_array_merge_arg_assign(&temp, source, module);
            Ok(Expr::new(ExprKind::Variable(temp), arg.span))
        })
        .collect()
}

fn emit_unknown_mixed_array_merge_arg_assign(
    name: &str,
    source: &str,
    module: &mut WasmModule,
) {
    let ptr = module.next_label("unknown_mixed_array_merge_ptr");
    let len = module.next_label("unknown_mixed_array_merge_len");
    let heap_kind = module.next_label("unknown_mixed_array_merge_heap_kind");
    for local in [&ptr, &len, &heap_kind] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.clear_array_length(name);
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    module.set_array_runtime_key_kind(name, None);
    module.set_array_php_normalized_runtime_keys(name, true);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.set_array_runtime_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_ptr");
    module.body().line(&format!("local.set {}", ptr));
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_len_i32");
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_heap_kind");
    module.body().line(&format!("local.set {}", heap_kind));
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_INDEXED_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_unknown_mixed_indexed_array_merge_arg_assign(name, &ptr, &len, module);
    module.body().close("else");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", len));
    module.body().line("call $__rt_assoc_array_copy_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().close("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
}

fn emit_unknown_mixed_indexed_array_merge_arg_assign(
    name: &str,
    ptr: &str,
    len: &str,
    module: &mut WasmModule,
) {
    let index = module.next_label("unknown_mixed_array_merge_index");
    let entry = module.next_label("unknown_mixed_array_merge_entry");
    let source_cell = module.next_label("unknown_mixed_array_merge_source_cell");
    let done_label = module.next_label("unknown_mixed_array_merge_done");
    let loop_label = module.next_label("unknown_mixed_array_merge_loop");
    for local in [&index, &entry, &source_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get {}", len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.store");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i64.extend_i32_u");
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", source_cell));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 16");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", source_cell));
    module.body().line("call $__rt_value_copy");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn assoc_array_merge_needs_materialized_arg(arg: &Expr, module: &WasmModule) -> bool {
    match &arg.kind {
        ExprKind::FunctionCall { name, .. }
            if module.has_function(name)
                && module.function_return_kind(name) == Some(ValueKind::Array) =>
        {
            true
        }
        ExprKind::FunctionCall { name, .. } => {
            name.eq_ignore_ascii_case("array_filter")
                || name.eq_ignore_ascii_case("array_map")
                || name.eq_ignore_ascii_case("array_reverse")
                || name.eq_ignore_ascii_case("array_unique")
                || name.eq_ignore_ascii_case("array_flip")
                || name.eq_ignore_ascii_case("array_values")
                || name.eq_ignore_ascii_case("array_keys")
                || name.eq_ignore_ascii_case("array_slice")
                || name.eq_ignore_ascii_case("array_pad")
                || name.eq_ignore_ascii_case("range")
                || name.eq_ignore_ascii_case("explode")
                || name.eq_ignore_ascii_case("str_split")
                || name.eq_ignore_ascii_case("array_fill")
                || name.eq_ignore_ascii_case("pathinfo")
                || name.eq_ignore_ascii_case("array_chunk")
                || name.eq_ignore_ascii_case("array_diff")
                || name.eq_ignore_ascii_case("array_intersect")
                || name.eq_ignore_ascii_case("array_diff_key")
                || name.eq_ignore_ascii_case("array_intersect_key")
                || name.eq_ignore_ascii_case("array_merge")
                || name.eq_ignore_ascii_case("array_splice")
                || name.eq_ignore_ascii_case("array_fill_keys")
                || name.eq_ignore_ascii_case("array_combine")
                || name.eq_ignore_ascii_case("array_column")
                || name.eq_ignore_ascii_case("class_parents")
                || name.eq_ignore_ascii_case("class_implements")
                || name.eq_ignore_ascii_case("class_uses")
                || name.eq_ignore_ascii_case("class_attribute_names")
        }
        _ => false,
    }
}

pub(super) fn materialize_assoc_array_merge_args(
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<Vec<Expr>, CompileError> {
    args.iter()
        .map(|arg| {
            if !assoc_array_merge_needs_materialized_arg(arg, module) {
                return Ok(arg.clone());
            }
            let temp = module
                .next_label("assoc_merge_arg")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, arg, module)?;
            Ok(Expr::new(ExprKind::Variable(temp), arg.span))
        })
        .collect()
}

pub(super) fn static_assoc_array_merge_items(
    expr: &Expr,
    args: &[Expr],
    module: &WasmModule,
) -> Result<Vec<(Expr, Expr)>, CompileError> {
    let mut out: Vec<(StaticAssocKey, Expr, Expr)> = Vec::new();
    let mut next_int_key = 0i64;
    for arg in args {
        let ExprKind::ArrayLiteralAssoc(items) = &arg.kind else {
            return Err(CompileError::new(
                arg.span,
                "wasm32-web associative array_merge() currently requires associative literals only",
            ));
        };
        for (key, value) in items {
            let key_value = static_assoc_key_value(key, module)?;
            let output_key = match key_value {
                StaticAssocKey::Int(_) => {
                    let key = Expr::new(ExprKind::IntLiteral(next_int_key), key.span);
                    next_int_key += 1;
                    out.push((StaticAssocKey::Int(next_int_key - 1), key, value.clone()));
                    continue;
                }
                StaticAssocKey::Str(value) => StaticAssocKey::Str(value),
            };
            if let Some((_, _, existing_value)) = out
                .iter_mut()
                .find(|(existing_key, _, _)| existing_key == &output_key)
            {
                *existing_value = value.clone();
            } else {
                out.push((output_key, key.clone(), value.clone()));
            }
        }
    }
    let _ = expr;
    Ok(out
        .into_iter()
        .map(|(_, key, value)| (key, value))
        .collect())
}

pub(super) fn matches_assoc_array_expr(expr: &Expr, module: &WasmModule) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => {
            module.local_kind(name) == Some(LocalKind::Array)
                && module.array_layout(name) == ArrayLayout::Assoc
        }
        ExprKind::FunctionCall { name, .. } => {
            module.has_function(name)
                && module.function_return_kind(name) == Some(ValueKind::Array)
                && module.function_array_return_layout(name) == ArrayLayout::Assoc
        }
        _ => false,
    }
}

pub(super) fn emit_assoc_array_merge_assign(
    name: &str,
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let max_len = validate_assoc_array_merge_sources(expr, args, module)?;
    let metadata = assoc_array_merge_metadata(args, module)?;
    module.set_array_layout(name, ArrayLayout::Assoc);
    if let Some(metadata) = metadata.as_ref() {
        module.set_array_length(name, metadata.keys.len());
    } else if let Some(max_len) = max_len {
        module.set_array_length(name, max_len);
    } else {
        module.clear_array_length(name);
    }
    module.set_array_key_kinds(
        name,
        metadata
            .as_ref()
            .map(|metadata| metadata.keys.iter().map(assoc_key_kind_for_value).collect()),
    );
    module.set_array_key_values(name, metadata.as_ref().map(|metadata| metadata.keys.clone()));
    module.set_array_value_cell_kinds(name, metadata.as_ref().map(|metadata| metadata.value_kinds.clone()));
    module.set_array_nested_value_metadata(name, metadata.as_ref().map(|metadata| metadata.nested_values.clone()));
    if metadata.is_none() {
        module.set_array_runtime_key_kind(name, assoc_array_merge_runtime_key_kind(args, module));
        module.set_array_php_normalized_runtime_keys(
            name,
            assoc_array_merge_has_php_normalized_runtime_keys(args, module),
        );
        let runtime_value_kind = assoc_array_merge_runtime_value_kind(args, module);
        module.set_array_runtime_value_cell_kind(name, runtime_value_kind);
        if runtime_value_kind == Some(ValueCellKind::Array) {
            module.set_array_runtime_nested_value_metadata(
                name,
                assoc_array_merge_runtime_nested_value_metadata(args, module),
            );
        }
    }
    let out_index = module.next_label("assoc_merge_out_index");
    let scan = module.next_label("assoc_merge_scan");
    let target_entry = module.next_label("assoc_merge_target_entry");
    let scan_entry = module.next_label("assoc_merge_scan_entry");
    let matched = module.next_label("assoc_merge_matched");
    let found = module.next_label("assoc_merge_found");
    let source_ptr = module.next_label("assoc_merge_source_ptr");
    let source_len = module.next_label("assoc_merge_source_len");
    let source_index = module.next_label("assoc_merge_source_index");
    let source_entry = module.next_label("assoc_merge_source_entry");
    let next_int_key = module.next_label("assoc_merge_next_int_key");
    let value_cell = module.next_label("assoc_merge_value_cell");
    let total_len = module.next_label("assoc_merge_total_len");
    let compact_value = module.next_label("assoc_merge_compact_value");
    for local in [
        &out_index,
        &scan,
        &target_entry,
        &scan_entry,
        &matched,
        &found,
        &source_ptr,
        &source_len,
        &source_index,
        &source_entry,
        &value_cell,
        &total_len,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(next_int_key.trim_start_matches('$').to_string());
    module.declare_i64_local(compact_value.trim_start_matches('$').to_string());
    if let Some(max_len) = max_len {
        module.body().line(&format!("i32.const {}", max_len));
    } else {
        emit_assoc_array_merge_runtime_capacity(args, &total_len, module)?;
        module.body().line(&format!("local.get {}", total_len));
    }
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line("i64.const 0");
    module.body().line(&format!("local.set {}", next_int_key));
    for arg in args {
        match &arg.kind {
            ExprKind::Variable(source) => {
                match module.array_layout(source) {
                    ArrayLayout::Assoc => emit_assoc_array_merge_local_source(
                        name,
                        source,
                        &out_index,
                        &source_ptr,
                        &source_len,
                        &source_index,
                        &source_entry,
                        &scan,
                        &scan_entry,
                        &matched,
                        &found,
                        &target_entry,
                        &next_int_key,
                        module,
                    ),
                    ArrayLayout::Value => emit_assoc_array_merge_value_source(
                        name,
                        source,
                        &out_index,
                        &source_ptr,
                        &source_len,
                        &source_index,
                        &source_entry,
                        &target_entry,
                        &next_int_key,
                        &value_cell,
                        module,
                    ),
                    ArrayLayout::CompactInt => emit_assoc_array_merge_compact_int_source(
                        name,
                        source,
                        &out_index,
                        &source_ptr,
                        &source_len,
                        &source_index,
                        &target_entry,
                        &next_int_key,
                        &compact_value,
                        module,
                    ),
                }
            }
            ExprKind::FunctionCall { name: function_name, args: call_args }
                if module.has_function(function_name)
                    && module.function_return_kind(function_name) == Some(ValueKind::Array)
                    && module.function_array_return_layout(function_name) == ArrayLayout::Assoc =>
            {
                let temp = materialize_function_array_return_local(
                    arg,
                    function_name,
                    call_args,
                    "assoc_merge_return",
                    module,
                )?;
                emit_assoc_array_merge_local_source(
                    name,
                    &temp,
                    &out_index,
                    &source_ptr,
                    &source_len,
                    &source_index,
                    &source_entry,
                    &scan,
                    &scan_entry,
                    &matched,
                    &found,
                    &target_entry,
                    &next_int_key,
                    module,
                );
            }
            ExprKind::ArrayLiteralAssoc(items) => {
                for (key, value) in items {
                    match static_assoc_key_value(key, module)? {
                        StaticAssocKey::Int(_) => emit_assoc_array_merge_literal_int_item(
                            name,
                            value,
                            &out_index,
                            &target_entry,
                            &next_int_key,
                            &value_cell,
                            module,
                        )?,
                        StaticAssocKey::Str(key_value) => emit_assoc_array_merge_literal_string_item(
                            name,
                            &key_value,
                            value,
                            &out_index,
                            &scan,
                            &target_entry,
                            &scan_entry,
                            &matched,
                            &found,
                            &value_cell,
                            module,
                        )?,
                    }
                }
            }
            ExprKind::ArrayLiteral(items) => {
                for value in items {
                    emit_assoc_array_merge_literal_int_item(
                        name,
                        value,
                        &out_index,
                        &target_entry,
                        &next_int_key,
                        &value_cell,
                        module,
                    )?;
                }
            }
            _ => return Err(array_unsupported(arg)),
        }
    }
    module.body().line(&format!("local.get {}", out_index));
    module.body().line(&format!("local.set ${}_len", name));
    Ok(())
}

fn assoc_array_merge_runtime_nested_value_metadata(
    args: &[Expr],
    module: &WasmModule,
) -> Option<NestedArrayMetadata> {
    let mut metadata = None;
    for arg in args {
        let ExprKind::Variable(source) = &arg.kind else {
            return None;
        };
        if module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Array) {
            return None;
        }
        let source_metadata = module.array_runtime_nested_value_metadata(source)?;
        if metadata
            .as_ref()
            .is_some_and(|metadata| metadata != &source_metadata)
        {
            return None;
        }
        metadata = Some(source_metadata);
    }
    metadata
}

pub(super) fn emit_assoc_array_merge_runtime_capacity(
    args: &[Expr],
    total_len: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", total_len));
    for arg in args {
        match &arg.kind {
            ExprKind::Variable(source)
                if module.local_kind(source) == Some(LocalKind::Array)
                    && matches!(
                        module.array_layout(source),
                        ArrayLayout::Assoc | ArrayLayout::Value | ArrayLayout::CompactInt
                    ) =>
            {
                module.body().line(&format!("local.get {}", total_len));
                module.body().line(&format!("local.get ${}_len", source));
                module.body().line("i32.add");
                module.body().line(&format!("local.set {}", total_len));
            }
            ExprKind::FunctionCall { name, .. }
                if module.has_function(name)
                    && module.function_return_kind(name) == Some(ValueKind::Array)
                    && module.function_array_return_layout(name) == ArrayLayout::Assoc =>
            {
                let Some(len) = module.function_array_return_length(name) else {
                    return Err(CompileError::new(
                        arg.span,
                        "wasm32-web dynamic associative array_merge() requires assigned local sources",
                    ));
                };
                module.body().line(&format!("local.get {}", total_len));
                module.body().line(&format!("i32.const {}", len));
                module.body().line("i32.add");
                module.body().line(&format!("local.set {}", total_len));
            }
            ExprKind::ArrayLiteralAssoc(items) => {
                module.body().line(&format!("local.get {}", total_len));
                module.body().line(&format!("i32.const {}", items.len()));
                module.body().line("i32.add");
                module.body().line(&format!("local.set {}", total_len));
            }
            ExprKind::ArrayLiteral(items) => {
                module.body().line(&format!("local.get {}", total_len));
                module.body().line(&format!("i32.const {}", items.len()));
                module.body().line("i32.add");
                module.body().line(&format!("local.set {}", total_len));
            }
            _ => return Err(array_unsupported(arg)),
        }
    }
    Ok(())
}

pub(super) fn validate_assoc_array_merge_sources(
    expr: &Expr,
    args: &[Expr],
    module: &WasmModule,
) -> Result<Option<usize>, CompileError> {
    let mut max_len = 0usize;
    let mut known_len = true;
    for arg in args {
        match &arg.kind {
            ExprKind::Variable(source)
                if module.local_kind(source) == Some(LocalKind::Array)
                    && matches!(
                        module.array_layout(source),
                        ArrayLayout::Assoc | ArrayLayout::Value | ArrayLayout::CompactInt
                    ) =>
            {
                if let Some(len) = module.array_length(source) {
                    max_len += len;
                } else {
                    known_len = false;
                }
            }
            ExprKind::FunctionCall { name, .. }
                if module.has_function(name)
                    && module.function_return_kind(name) == Some(ValueKind::Array)
                    && module.function_array_return_layout(name) == ArrayLayout::Assoc =>
            {
                if module.function_array_return_key_kinds(name).is_none() {
                    return Err(CompileError::new(
                        arg.span,
                        "wasm32-web associative array_merge() requires statically-known key kinds",
                    ));
                }
                if let Some(len) = module.function_array_return_length(name) {
                    max_len += len;
                } else {
                    return Err(CompileError::new(
                        arg.span,
                        "wasm32-web associative array_merge() requires known associative source lengths",
                    ));
                }
            }
            ExprKind::ArrayLiteralAssoc(items) => {
                for (key, _) in items {
                    static_assoc_key_value(key, module)?;
                }
                max_len += items.len();
            }
            ExprKind::ArrayLiteral(items) => {
                for value in items {
                    if value_cell_kind_for_expr(value, module).is_none() {
                        return Err(array_unsupported(value));
                    }
                }
                max_len += items.len();
            }
            _ => {
                return Err(CompileError::new(
                    arg.span,
                    "wasm32-web associative array_merge() currently supports assigned array locals and associative literals only",
                ));
            }
        }
    }
    let _ = expr;
    Ok(known_len.then_some(max_len))
}
