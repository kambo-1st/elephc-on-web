//! Purpose:
//! Emits wasm32-web direct-output helpers for `implode()` and boxed value cells.
//! Keeps output-only separator handling separate from string materialization.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::implode` through its re-export.
//! - `crate::codegen::wasm::expr` callers that output mixed value cells.
//!
//! Key details:
//! - Direct-output paths stream pieces to host writes instead of building a string.
//! - Array shape checks stay identical to the materializing `implode()` lowering.

use super::*;

enum OutputImplodeSeparator {
    Static(String),
    Local(String),
}

fn emit_output_separator(separator: &OutputImplodeSeparator, module: &mut WasmModule) {
    match separator {
        OutputImplodeSeparator::Static(separator) => {
            if separator.is_empty() {
                return;
            }
            let (ptr, len) = module.intern_string(separator);
            module.body().line(&format!("i32.const {}", ptr));
            module.body().line(&format!("i32.const {}", len));
            module.body().line("call $host_write");
        }
        OutputImplodeSeparator::Local(separator) => {
            module.body().line(&format!("local.get ${}_ptr", separator));
            module.body().line(&format!("local.get ${}_len", separator));
            module.body().line("call $host_write");
        }
    }
}

pub(super) fn emit_output_implode_assigned_string_array(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let (separator, array) = match args {
        [array] => (OutputImplodeSeparator::Static(String::new()), array),
        [separator, array] => {
            let separator = if let Some(separator) = static_or_tracked_string_value(separator, module) {
                OutputImplodeSeparator::Static(separator)
            } else if let Some(separator) =
                runtime_string_arg_or_materialize(separator, "output_implode_separator", module)?
            {
                OutputImplodeSeparator::Local(separator)
            } else {
                return Err(CompileError::new(separator.span, "wasm32-web implode() separator must be a string"));
            };
            (separator, array)
        }
        _ => {
            return Err(CompileError::new(
                call.span,
                "wasm32-web implode() expects one or two arguments",
            ));
        }
    };
    let temp;
    let name = match &array.kind {
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Array) => {
            name.as_str()
        }
        ExprKind::ArrayLiteral(_) => {
            temp = module
                .next_label("output_implode_direct_literal_array")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            temp.as_str()
        }
        ExprKind::ArrayLiteralAssoc(_) => {
            temp = module
                .next_label("output_implode_direct_assoc_array")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            temp.as_str()
        }
        ExprKind::FunctionCall { name, .. }
            if module.has_function(name)
                && module.function_return_kind(name) == Some(ValueKind::Array)
                && matches!(
                    module.function_array_return_layout(name),
                    ArrayLayout::Value | ArrayLayout::Assoc
                ) =>
        {
            temp = module
                .next_label("output_implode_return_array")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            temp.as_str()
        }
        ExprKind::FunctionCall { name, .. }
            if name.eq_ignore_ascii_case("explode")
                || name.eq_ignore_ascii_case("str_split")
                || name.eq_ignore_ascii_case("range")
                || name.eq_ignore_ascii_case("class_parents")
                || name.eq_ignore_ascii_case("class_implements")
                || name.eq_ignore_ascii_case("class_uses")
                || name.eq_ignore_ascii_case("class_attribute_names") =>
        {
            temp = module
                .next_label("output_implode_direct_array")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            temp.as_str()
        }
        ExprKind::MethodCall { object, method, .. }
            if method_call_array_return_metadata(object, method, module).is_some() =>
        {
            temp = module
                .next_label("output_implode_method_array")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            temp.as_str()
        }
        ExprKind::NullsafeMethodCall { object, method, .. }
            if object_expr_is_known_non_null(object, module)
                && method_call_array_return_metadata(object, method, module).is_some() =>
        {
            temp = module
                .next_label("output_implode_nullsafe_method_array")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            temp.as_str()
        }
        ExprKind::StaticMethodCall { receiver, method, .. }
            if static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            temp = module
                .next_label("output_implode_static_method_array")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            temp.as_str()
        }
        ExprKind::DynamicStaticMethodCall { receiver, method, .. }
            if dynamic_static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            temp = module
                .next_label("output_implode_dynamic_static_method_array")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            temp.as_str()
        }
        ExprKind::ArrayAccess { .. } if nested_array_metadata_for_access_expr(array, module).is_some() => {
            temp = materialize_nested_output_implode_source(array, module)?;
            temp.as_str()
        }
        _ => return Ok(false),
    };
    if module.array_layout(name) == ArrayLayout::CompactInt {
        emit_output_implode_compact_int_array(name, &separator, module);
        return Ok(true);
    }
    if module.array_layout(name) == ArrayLayout::Assoc {
        emit_output_implode_assoc_array(name, &separator, module, array.span)?;
        return Ok(true);
    }
    if module.array_layout(name) != ArrayLayout::Value {
        return Err(CompileError::new(
            array.span,
            "wasm32-web implode() over assigned arrays currently requires a value-cell array",
        ));
    }
    if module.array_length(name).is_none() || module.array_value_cell_kinds(name).is_none() {
        emit_output_implode_runtime_value_array(name, &separator, module);
        return Ok(true);
    }
    let Some(len) = module.array_length(name) else {
        return Err(CompileError::new(
            array.span,
            "wasm32-web implode() over assigned arrays currently requires a known array length",
        ));
    };
    let Some(kinds) = module.array_value_cell_kinds(name) else {
        return Err(CompileError::new(
            array.span,
            "wasm32-web implode() over assigned arrays currently requires known element types",
        ));
    };
    if kinds.len() != len
        || kinds
            .iter()
            .any(|kind| matches!(kind, ValueCellKind::Array))
    {
        return Err(CompileError::new(
            array.span,
            "wasm32-web implode() over assigned arrays currently supports scalar elements only",
        ));
    }
    for index in 0..len {
        if index > 0 {
            emit_output_separator(&separator, module);
        }
        emit_value_array_static_cell_addr(name, index, module);
        emit_output_value_cell_stack(module);
    }
    Ok(true)
}

fn materialize_nested_output_implode_source(
    source: &Expr,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let Some(metadata) = nested_array_metadata_for_access_expr(source, module) else {
        return Err(CompileError::new(
            source.span,
            "wasm32-web implode() requires nested array metadata",
        ));
    };
    let temp = module
        .next_label("output_implode_nested_source")
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
                "wasm32-web implode() expected a nested array value",
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

fn emit_output_implode_assoc_array(
    name: &str,
    separator: &OutputImplodeSeparator,
    module: &mut WasmModule,
    span: crate::span::Span,
) -> Result<(), CompileError> {
    let Some(kinds) = module.array_value_cell_kinds(name) else {
        emit_output_implode_mixed_assoc_array(name, separator, module);
        return Ok(());
    };
    let Some(len) = module.array_length(name) else {
        return Err(CompileError::new(
            span,
            "wasm32-web implode() over associative arrays requires a known array length",
        ));
    };
    if kinds.len() != len
        || kinds
            .iter()
            .any(|kind| matches!(kind, ValueCellKind::Array))
    {
        return Err(CompileError::new(
            span,
            "wasm32-web implode() over associative arrays currently supports scalar elements only",
        ));
    }
    for index in 0..len {
        if index > 0 {
            emit_output_separator(separator, module);
        }
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line("call $__rt_assoc_value_cell");
        emit_output_value_cell_stack(module);
    }
    Ok(())
}

fn emit_output_implode_mixed_assoc_array(
    name: &str,
    separator: &OutputImplodeSeparator,
    module: &mut WasmModule,
) {
    let index = module.next_label("output_implode_mixed_assoc_index");
    let cell = module.next_label("output_implode_mixed_assoc_cell");
    let tag = module.next_label("output_implode_mixed_assoc_tag");
    let loop_label = module.next_label("output_implode_mixed_assoc_loop");
    let done_label = module.next_label("output_implode_mixed_assoc_done");
    for local in [&index, &cell, &tag] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 0");
    module.body().line("i32.gt_u");
    module.body().open("if");
    emit_output_separator(separator, module);
    module.body().close("end");
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", tag));
    emit_output_implode_reject_unsupported_mixed_assoc_tag(&tag, module);
    module.body().line(&format!("local.get {}", cell));
    emit_output_value_cell_stack(module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_output_implode_reject_unsupported_mixed_assoc_tag(tag: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.eq");
    module.body().line("i32.or");
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
}

fn emit_output_implode_compact_int_array(
    name: &str,
    separator: &OutputImplodeSeparator,
    module: &mut WasmModule,
) {
    if let Some(len) = module.array_length(name) {
        for index in 0..len {
            if index > 0 {
                emit_output_separator(separator, module);
            }
            module.body().line(&format!("local.get ${}_ptr", name));
            module.body().line(&format!("i32.const {}", index * 8));
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("call $host_write_int");
        }
        return;
    }
    let index = module.next_label("output_implode_int_index");
    let done_label = module.next_label("output_implode_int_done");
    let loop_label = module.next_label("output_implode_int_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 0");
    module.body().line("i32.gt_u");
    module.body().open("if");
    emit_output_separator(separator, module);
    module.body().close("end");
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("call $host_write_int");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_output_implode_runtime_value_array(
    name: &str,
    separator: &OutputImplodeSeparator,
    module: &mut WasmModule,
) {
    let index = module.next_label("output_implode_runtime_index");
    let cell = module.next_label("output_implode_runtime_cell");
    let loop_label = module.next_label("output_implode_runtime_loop");
    let done_label = module.next_label("output_implode_runtime_done");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 0");
    module.body().line("i32.gt_u");
    module.body().open("if");
    emit_output_separator(separator, module);
    module.body().close("end");
    emit_value_cell_address_for_local(name, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    emit_output_value_cell_stack(module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_output_value_cell_stack(module: &mut WasmModule) {
    let (array_marker_ptr, _) = module.intern_string("Array");
    module.body().line(&format!("i32.const {}", array_marker_ptr));
    module.body().line("call $__rt_output_value_cell");
}
