//! Purpose:
//! Lowers wasm32-web array membership expressions such as `in_array()` and
//! `array_key_exists()`.
//!
//! Called from:
//! - `super::emit_expr()` for PHP array membership builtins.
//!
//! Key details:
//! - Associative-key paths preserve runtime string/int/mixed-key comparison semantics.
//! - Unsupported dynamic shapes stay as `CompileError`s instead of silent fallbacks.

use super::*;
use super::array_contains_assoc::{
    emit_assoc_array_contains_loose_scalar, emit_assoc_array_contains_scalar,
};
use super::array_contains_values::{
    emit_value_array_contains_loose_scalar, emit_value_array_contains_scalar,
    emit_value_array_contains_object_identity,
};

pub(super) fn emit_in_array_call(
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if !(2..=3).contains(&args.len()) {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web in_array() expects two or three arguments",
        ));
    }
    let strict = if let Some(strict) = args.get(2) {
        if static_or_const_or_i32_bool_value(strict, module).is_none() {
            return Err(CompileError::new(
                strict.span,
                "wasm32-web in_array() requires a static boolean strict flag",
            ));
        }
        static_or_const_or_i32_bool_value(strict, module).expect("checked by guard")
    } else {
        false
    };
    let found = module.next_label("in_array_found");
    module.declare_i32_local(found.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    if let ExprKind::Variable(name) = &args[1].kind {
        if module.local_kind(name) == Some(LocalKind::Mixed) && module.mixed_value_cell_kind(name).is_none() {
            emit_unknown_mixed_array_in_array_call(&args[0], name, strict, &found, module)?;
            module.body().line(&format!("local.get {}", found));
            return Ok(ValueKind::Bool);
        }
    }
    if matches!(args[1].kind, ExprKind::ArrayAccess { .. })
        && nested_array_metadata_for_access_expr(&args[1], module).is_some()
    {
        let temp = materialize_nested_array_membership_source(&args[1], module)?;
        let temp_expr = Expr::new(ExprKind::Variable(temp), args[1].span);
        emit_array_contains_int(&temp_expr, &args[0], strict, &found, module)?;
        module.body().line(&format!("local.get {}", found));
        return Ok(ValueKind::Bool);
    }
    if let ExprKind::StaticMethodCall {
        receiver,
        method,
        args: method_args,
    } = &args[1].kind
    {
        if method.eq_ignore_ascii_case("cases")
            && method_args.is_empty()
            && module
                .class_name_for_receiver(receiver)
                .and_then(|class_name| module.enum_case_names(&class_name))
                .is_some()
        {
            if !strict {
                return Err(CompileError::new(
                    args[1].span,
                    "wasm32-web loose in_array() over enum cases requires object comparison runtime support",
                ));
            }
            let temp = module
                .next_label("enum_cases_in_array_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, &args[1], module)?;
            emit_value_array_contains_object_identity(&temp, &args[0], &found, module)?;
            module.body().line(&format!("local.get {}", found));
            return Ok(ValueKind::Bool);
        }
    }
    emit_array_contains_int(&args[1], &args[0], strict, &found, module)?;
    module.body().line(&format!("local.get {}", found));
    Ok(ValueKind::Bool)
}

pub(super) fn emit_array_key_exists_call(
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if args.len() != 2 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_key_exists() expects exactly two arguments",
        ));
    }
    if let ExprKind::Variable(name) = &args[1].kind {
        if module.local_kind(name) == Some(LocalKind::Mixed) && module.mixed_value_cell_kind(name).is_none() {
            return emit_unknown_mixed_array_key_exists_call(expr, &args[0], name, module);
        }
    }
    if let ExprKind::Variable(name) = &args[1].kind {
        if module.local_kind(name) == Some(LocalKind::Array)
            && module.array_layout(name) == ArrayLayout::Assoc
        {
            return emit_assoc_array_key_exists_call(expr, &args[0], name, module);
        }
    }
    if matches!(args[1].kind, ExprKind::ArrayAccess { .. })
        && nested_array_metadata_for_access_expr(&args[1], module).is_some()
    {
        let temp = materialize_nested_array_membership_source(&args[1], module)?;
        if module.array_layout(&temp) == ArrayLayout::Assoc {
            return emit_assoc_array_key_exists_call(expr, &args[0], &temp, module);
        }
        let temp_expr = Expr::new(ExprKind::Variable(temp), args[1].span);
        return emit_indexed_array_key_exists_call(&args[0], &temp_expr, module);
    }
    if let ExprKind::ArrayLiteralAssoc(items) = &args[1].kind {
        let temp = module
            .next_label("assoc_array_key_exists_source")
            .trim_start_matches('$')
            .to_string();
        module.declare_array_local(temp.clone());
        emit_assoc_array_items_assign(&temp, items, module)?;
        return emit_assoc_array_key_exists_call(expr, &args[0], &temp, module);
    }
    if let ExprKind::StaticMethodCall {
        receiver,
        method,
        args: method_args,
    } = &args[1].kind
    {
        if method.eq_ignore_ascii_case("cases")
            && method_args.is_empty()
            && module
                .class_name_for_receiver(receiver)
                .and_then(|class_name| module.enum_case_names(&class_name))
                .is_some()
        {
            let temp = module
                .next_label("enum_cases_key_exists_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, &args[1], module)?;
            let temp_expr = Expr::new(ExprKind::Variable(temp), args[1].span);
            return emit_indexed_array_key_exists_call(&args[0], &temp_expr, module);
        }
    }
    if let ExprKind::FunctionCall { name: function_name, .. } = &args[1].kind {
        if module.has_function(function_name)
            && module.function_return_kind(function_name) == Some(ValueKind::Array)
            && module.function_array_return_layout(function_name) == ArrayLayout::Assoc
        {
            let temp = module
                .next_label("assoc_key_exists_return_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, &args[1], module)?;
            return emit_assoc_array_key_exists_call(expr, &args[0], &temp, module);
        }
    }
    if expression_has_array_type(&args[1], module) && !expression_is_arrayy(&args[1], module) {
        let temp = module
            .next_label("assoc_key_exists_direct_source")
            .trim_start_matches('$')
            .to_string();
        module.declare_array_local(temp.clone());
        emit_array_assign(&temp, &args[1], module)?;
        if module.array_layout(&temp) == ArrayLayout::Assoc {
            return emit_assoc_array_key_exists_call(expr, &args[0], &temp, module);
        }
        let temp_expr = Expr::new(ExprKind::Variable(temp), args[1].span);
        return emit_indexed_array_key_exists_call(&args[0], &temp_expr, module);
    }
    if !expression_is_arrayy(&args[1], module) {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web array_key_exists() currently supports indexed array values only",
        ));
    }
    emit_indexed_array_key_exists_call(&args[0], &args[1], module)
}

fn materialize_nested_array_membership_source(
    source: &Expr,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let Some(metadata) = nested_array_metadata_for_access_expr(source, module) else {
        return Err(CompileError::new(
            source.span,
            "wasm32-web array membership requires nested array metadata",
        ));
    };
    let temp = module
        .next_label("array_membership_nested_source")
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
                "wasm32-web array membership expected a nested array value",
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

fn emit_indexed_array_key_exists_call(
    key_expr: &Expr,
    array_expr: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let key = module.next_label("array_key");
    let ptr = module.next_label("array_key_ptr");
    let len = module.next_label("array_key_len");
    module.declare_i64_local(key.trim_start_matches('$').to_string());
    for local in [&ptr, &len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_array_value_to_stack(array_expr, module)?;
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.set {}", ptr));
    if expression_is_stringy(key_expr, module) {
        return emit_indexed_array_string_key_exists(key_expr, &key, &len, module);
    }
    require_int(key_expr, module)?;
    module.body().line(&format!("local.set {}", key));
    emit_indexed_array_int_key_exists(&key, &len, module);
    Ok(ValueKind::Bool)
}

fn emit_indexed_array_string_key_exists(
    key_expr: &Expr,
    key: &str,
    len: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let key_ptr = module.next_label("array_key_string_ptr");
    let key_len = module.next_label("array_key_string_len");
    module.declare_i32_local(key_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(key_len.trim_start_matches('$').to_string());
    emit_string_value_to_stack(key_expr, module)?;
    module.body().line(&format!("local.set {}", key_len));
    module.body().line(&format!("local.set {}", key_ptr));
    module.body().line(&format!("local.get {}", key_ptr));
    module.body().line(&format!("local.get {}", key_len));
    module.body().line("call $__rt_php_array_key_is_int");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", key_ptr));
    module.body().line(&format!("local.get {}", key_len));
    module.body().line("call $__rt_php_array_key_to_int");
    module.body().line(&format!("local.set {}", key));
    emit_indexed_array_int_key_exists(key, len, module);
    module.body().close("else");
    module.body().line("i32.const 0");
    module.body().close("end");
    Ok(ValueKind::Bool)
}

fn emit_indexed_array_int_key_exists(key: &str, len: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", key));
    module.body().line("i64.const 0");
    module.body().line("i64.ge_s");
    module.body().line(&format!("local.get {}", key));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i64.extend_i32_u");
    module.body().line("i64.lt_s");
    module.body().line("i32.and");
}

fn emit_load_unknown_mixed_array_header(
    source: &str,
    ptr: &str,
    len: &str,
    heap_kind: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_ptr");
    module.body().line(&format!("local.set {}", ptr));
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_len_i32");
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_heap_kind");
    module.body().line(&format!("local.set {}", heap_kind));
}

pub(in crate::codegen::wasm::expr) fn emit_unknown_mixed_array_in_array_call(
    needle: &Expr,
    source: &str,
    strict: bool,
    found: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("unknown_mixed_in_array_source")
        .trim_start_matches('$')
        .to_string();
    let heap_kind = module.next_label("unknown_mixed_in_array_heap_kind");
    module.declare_array_local(temp.clone());
    module.declare_i32_local(heap_kind.trim_start_matches('$').to_string());
    emit_load_unknown_mixed_array_header(
        source,
        &format!("${}_ptr", temp),
        &format!("${}_len", temp),
        &heap_kind,
        module,
    );
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_INDEXED_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.set_array_layout(&temp, ArrayLayout::Value);
    module.set_array_value_cell_kinds(&temp, None);
    if strict {
        emit_value_array_contains_scalar(&temp, needle, found, module)?;
    } else {
        emit_value_array_contains_loose_scalar(&temp, needle, found, module)?;
    }
    module.body().close("else");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.set_array_layout(&temp, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(&temp, None);
    module.set_array_key_kinds(&temp, None);
    if strict {
        emit_assoc_array_contains_scalar(&temp, needle, found, module)?;
    } else {
        emit_assoc_array_contains_loose_scalar(&temp, needle, found, module)?;
    }
    module.body().close("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_unknown_mixed_array_key_exists_call(
    expr: &Expr,
    key: &Expr,
    source: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let ptr = module.next_label("unknown_mixed_key_exists_ptr");
    let len = module.next_label("unknown_mixed_key_exists_len");
    let heap_kind = module.next_label("unknown_mixed_key_exists_heap_kind");
    for local in [&ptr, &len, &heap_kind] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_load_unknown_mixed_array_header(source, &ptr, &len, &heap_kind, module);
    if expression_is_stringy(key, module) {
        let key_ptr = module.next_label("unknown_mixed_key_exists_key_ptr");
        let key_len = module.next_label("unknown_mixed_key_exists_key_len");
        let int_key = module.next_label("unknown_mixed_key_exists_int_key");
        module.declare_i32_local(key_ptr.trim_start_matches('$').to_string());
        module.declare_i32_local(key_len.trim_start_matches('$').to_string());
        module.declare_i64_local(int_key.trim_start_matches('$').to_string());
        emit_string_value_to_stack(key, module)?;
        module.body().line(&format!("local.set {}", key_len));
        module.body().line(&format!("local.set {}", key_ptr));
        emit_unknown_mixed_array_string_key_exists(&ptr, &len, &heap_kind, &key_ptr, &key_len, &int_key, module);
        let _ = expr;
        return Ok(ValueKind::Bool);
    }
    if expression_is_inty(key, module) {
        let int_key = module.next_label("unknown_mixed_key_exists_int_key");
        module.declare_i64_local(int_key.trim_start_matches('$').to_string());
        require_int(key, module)?;
        module.body().line(&format!("local.set {}", int_key));
        emit_unknown_mixed_array_int_key_exists(&ptr, &len, &heap_kind, &int_key, module);
        let _ = expr;
        return Ok(ValueKind::Bool);
    }
    Err(CompileError::new(
        key.span,
        "wasm32-web array_key_exists() over unknown mixed arrays requires an integer or string key",
    ))
}

fn emit_unknown_mixed_array_int_key_exists(
    ptr: &str,
    len: &str,
    heap_kind: &str,
    key: &str,
    module: &mut WasmModule,
) {
    let found = module.next_label("unknown_mixed_key_exists_found");
    module.declare_i32_local(found.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_INDEXED_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_unknown_mixed_indexed_int_key_exists(len, key, &found, module);
    module.body().close("else");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    emit_unknown_mixed_assoc_int_key_exists(ptr, len, key, &found, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
}

fn emit_unknown_mixed_array_string_key_exists(
    ptr: &str,
    len: &str,
    heap_kind: &str,
    key_ptr: &str,
    key_len: &str,
    int_key: &str,
    module: &mut WasmModule,
) {
    let found = module.next_label("unknown_mixed_key_exists_found");
    module.declare_i32_local(found.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_INDEXED_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", key_ptr));
    module.body().line(&format!("local.get {}", key_len));
    module.body().line("call $__rt_php_array_key_is_int");
    module.body().open("if");
    module.body().line(&format!("local.get {}", key_ptr));
    module.body().line(&format!("local.get {}", key_len));
    module.body().line("call $__rt_php_array_key_to_int");
    module.body().line(&format!("local.set {}", int_key));
    emit_unknown_mixed_indexed_int_key_exists(len, int_key, &found, module);
    module.body().close("else");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().close("end");
    module.body().close("else");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    emit_unknown_mixed_assoc_string_key_exists(ptr, len, key_ptr, key_len, &found, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
}

fn emit_unknown_mixed_indexed_int_key_exists(
    len: &str,
    key: &str,
    found: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", key));
    module.body().line("i64.const 0");
    module.body().line("i64.ge_s");
    module.body().line(&format!("local.get {}", key));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i64.extend_i32_u");
    module.body().line("i64.lt_s");
    module.body().line("i32.and");
    module.body().line(&format!("local.set {}", found));
}

fn emit_unknown_mixed_assoc_int_key_exists(
    ptr: &str,
    len: &str,
    key: &str,
    found: &str,
    module: &mut WasmModule,
) {
    let index = module.next_label("unknown_mixed_assoc_key_exists_index");
    let entry = module.next_label("unknown_mixed_assoc_key_exists_entry");
    let done_label = module.next_label("unknown_mixed_assoc_key_exists_done");
    let loop_label = module.next_label("unknown_mixed_assoc_key_exists_loop");
    for local in [&index, &entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get {}", key));
    module.body().line("call $__rt_assoc_key_eq_int");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_unknown_mixed_assoc_string_key_exists(
    ptr: &str,
    len: &str,
    key_ptr: &str,
    key_len: &str,
    found: &str,
    module: &mut WasmModule,
) {
    let index = module.next_label("unknown_mixed_assoc_key_exists_index");
    let entry = module.next_label("unknown_mixed_assoc_key_exists_entry");
    let done_label = module.next_label("unknown_mixed_assoc_key_exists_done");
    let loop_label = module.next_label("unknown_mixed_assoc_key_exists_loop");
    for local in [&index, &entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get {}", key_ptr));
    module.body().line(&format!("local.get {}", key_len));
    module.body().line("call $__rt_assoc_key_eq_php_string");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_assoc_array_key_exists_call(
    expr: &Expr,
    key: &Expr,
    name: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if let Some(key_value) = static_or_const_int_value(key) {
        return emit_assoc_array_int_key_exists(expr, name, key_value, module);
    }
    if let Some(key_value) = static_string_value(key, module) {
        return emit_assoc_array_string_key_exists(expr, name, &key_value, module);
    }
    if let ExprKind::Variable(key_name) = &key.kind {
        if module.local_kind(key_name) == Some(LocalKind::Mixed) {
            return emit_assoc_array_mixed_key_exists(expr, name, key_name, module);
        }
        if module.local_kind(key_name) == Some(LocalKind::Str) {
            return emit_assoc_array_dynamic_key_exists(
                expr,
                name,
                &AssocAssignKey::StringVar(key_name.clone()),
                module,
            );
        }
        if module.local_kind(key_name) == Some(LocalKind::I64) {
            return emit_assoc_array_dynamic_key_exists(
                expr,
                name,
                &AssocAssignKey::IntVar(key_name.clone()),
                module,
            );
        }
    }
    if expression_is_stringy(key, module) {
        let ptr = module.next_label("assoc_key_exists_key_ptr");
        let len = module.next_label("assoc_key_exists_key_len");
        module.declare_i32_local(ptr.trim_start_matches('$').to_string());
        module.declare_i32_local(len.trim_start_matches('$').to_string());
        emit_string_value_to_stack(key, module)?;
        module.body().line(&format!("local.set {}", len));
        module.body().line(&format!("local.set {}", ptr));
        return emit_assoc_array_dynamic_key_exists(
            expr,
            name,
            &AssocAssignKey::StringDynamic { ptr, len },
            module,
        );
    }
    let int_key = module.next_label("assoc_key_exists_int_key");
    module.declare_i64_local(int_key.trim_start_matches('$').to_string());
    require_int(key, module)?;
    module.body().line(&format!("local.set {}", int_key));
    emit_assoc_array_dynamic_key_exists(
        expr,
        name,
        &AssocAssignKey::IntDynamic(int_key),
        module,
    )
}

fn emit_assoc_array_mixed_key_exists(
    expr: &Expr,
    name: &str,
    key_name: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let entry = module.next_label("assoc_array_mixed_key_entry");
    let entry_base = module.next_label("assoc_array_mixed_key_entry_base");
    let key_tag = module.next_label("assoc_array_mixed_key_tag");
    let matched = module.next_label("assoc_array_mixed_key_matched");
    let found = module.next_label("assoc_array_mixed_key_found");
    let done_label = module.next_label("assoc_array_mixed_key_done");
    let loop_label = module.next_label("assoc_array_mixed_key_loop");
    for local in [&entry, &entry_base, &key_tag, &matched, &found] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", entry));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", entry));
    module
        .body()
        .line(&format!("i32.const {}", WASM_ASSOC_ENTRY_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", entry_base));
    module.body().line(&format!("local.get ${}", key_name));
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", key_tag));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("local.get {}", key_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", entry_base));
    module.body().line(&format!("local.get ${}", key_name));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.get ${}", key_name));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line("call $__rt_assoc_key_eq_php_string");
    module.body().line(&format!("local.set {}", matched));
    module.body().close("else");
    module.body().line(&format!("local.get {}", key_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", entry_base));
    module.body().line(&format!("local.get ${}", key_name));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("call $__rt_assoc_key_eq_int");
    module.body().line(&format!("local.set {}", matched));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
    let _ = expr;
    Ok(ValueKind::Bool)
}

fn emit_assoc_array_dynamic_key_exists(
    expr: &Expr,
    name: &str,
    key: &AssocAssignKey,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let entry = module.next_label("assoc_array_entry");
    let entry_base = module.next_label("assoc_array_entry_base");
    let matched = module.next_label("assoc_array_key_matched");
    let found = module.next_label("assoc_array_key_found");
    let done_label = module.next_label("assoc_array_key_done");
    let loop_label = module.next_label("assoc_array_key_loop");
    for local in [&entry, &entry_base, &matched, &found] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", entry));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", entry));
    module
        .body()
        .line(&format!("i32.const {}", WASM_ASSOC_ENTRY_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", entry_base));
    emit_assoc_entry_matches_assign_key(&entry_base, key, &matched, module);
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
    let _ = expr;
    Ok(ValueKind::Bool)
}

fn emit_assoc_array_int_key_exists(
    expr: &Expr,
    name: &str,
    key_value: i64,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let key = module.next_label("assoc_array_key");
    let entry = module.next_label("assoc_array_entry");
    let found = module.next_label("assoc_array_key_found");
    let done_label = module.next_label("assoc_array_key_done");
    let loop_label = module.next_label("assoc_array_key_loop");
    module.declare_i64_local(key.trim_start_matches('$').to_string());
    for local in [&entry, &found] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("i64.const {}", key_value));
    module.body().line(&format!("local.set {}", key));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", entry));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", entry));
    module
        .body()
        .line(&format!("i32.const {}", WASM_ASSOC_ENTRY_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", entry));
    module
        .body()
        .line(&format!("i32.const {}", WASM_ASSOC_ENTRY_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.get {}", key));
    module.body().line("i64.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
    let _ = expr;
    Ok(ValueKind::Bool)
}

fn emit_assoc_array_string_key_exists(
    expr: &Expr,
    name: &str,
    key_value: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let (key_ptr, key_len) = module.intern_string(key_value);
    let entry = module.next_label("assoc_array_entry");
    let entry_base = module.next_label("assoc_array_entry_base");
    let offset = module.next_label("assoc_array_key_offset");
    let matched = module.next_label("assoc_array_key_matched");
    let found = module.next_label("assoc_array_key_found");
    let done_label = module.next_label("assoc_array_key_done");
    let loop_label = module.next_label("assoc_array_key_loop");
    let compare_done_label = module.next_label("assoc_array_key_compare_done");
    let compare_loop_label = module.next_label("assoc_array_key_compare_loop");
    for local in [&entry, &entry_base, &offset, &matched, &found] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", entry));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", entry));
    module
        .body()
        .line(&format!("i32.const {}", WASM_ASSOC_ENTRY_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", entry_base));
    module.body().line(&format!("local.get {}", entry_base));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", entry_base));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", key_len));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", matched));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", offset));
    module.body().open(&format!("block {}", compare_done_label));
    module.body().open(&format!("loop {}", compare_loop_label));
    module.body().line(&format!("local.get {}", offset));
    module.body().line(&format!("i32.const {}", key_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", compare_done_label));
    module.body().line(&format!("local.get {}", entry_base));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.get {}", offset));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("i32.const {}", key_ptr));
    module.body().line(&format!("local.get {}", offset));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("br {}", compare_done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", offset));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", offset));
    module.body().line(&format!("br {}", compare_loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
    let _ = expr;
    Ok(ValueKind::Bool)
}
