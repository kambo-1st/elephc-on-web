//! Purpose:
//! Lowers json_encode() expressions for the wasm32-web backend.
//! Owns array output and JSON value-cell materialization for runtime and metadata-backed arrays.
//!
//! Called from:
//! - `crate::codegen::wasm::expr`
//!
//! Key details:
//! - Keeps JSON-specific metadata validation isolated while reusing sibling string escaping helpers.

use super::*;
use super::json_encode_output_arrays::*;
use super::json_encode_runtime_arrays::*;
use super::json_memory_string::{
    emit_copy_literal_text_to_memory,
};
use super::json_output_metadata::{
    assoc_key_values_are_json_list, json_value_kinds_need_missing_nested_metadata,
};

pub(super) fn emit_output_json_encode_array_local(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let name = match &args[0].kind {
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Array) => name.clone(),
        ExprKind::FunctionCall { name, .. }
            if module.has_function(name) && module.function_return_kind(name) == Some(ValueKind::Array) =>
        {
            let temp = module.next_label("json_array_source");
            let temp_name = temp.trim_start_matches('$').to_string();
            module.declare_array_local(temp_name.clone());
            emit_array_assign(&temp_name, &args[0], module)?;
            temp_name
        }
        ExprKind::FunctionCall { name, .. } if json_encode_direct_array_call_is_supported(name) => {
            let temp = module.next_label("json_array_source");
            let temp_name = temp.trim_start_matches('$').to_string();
            module.declare_array_local(temp_name.clone());
            emit_array_assign(&temp_name, &args[0], module)?;
            temp_name
        }
        ExprKind::MethodCall { object, method, .. }
            if method_call_array_return_metadata(object, method, module).is_some() =>
        {
            materialize_json_method_array_source(&args[0], "json_method_array_source", module)?
        }
        ExprKind::NullsafeMethodCall { object, method, .. }
            if object_expr_is_known_non_null(object, module)
                && method_call_array_return_metadata(object, method, module).is_some() =>
        {
            materialize_json_method_array_source(&args[0], "json_nullsafe_method_array_source", module)?
        }
        ExprKind::StaticMethodCall { receiver, method, .. }
            if static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            materialize_json_method_array_source(&args[0], "json_static_method_array_source", module)?
        }
        ExprKind::DynamicStaticMethodCall { receiver, method, .. }
            if dynamic_static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            materialize_json_method_array_source(&args[0], "json_dynamic_static_method_array_source", module)?
        }
        ExprKind::ArrayAccess { .. } if nested_array_metadata_for_access_expr(&args[0], module).is_some() => {
            materialize_nested_json_source(&args[0], "json_nested_array_source", module)?
        }
        _ => return Ok(false),
    };
    let flags = args
        .get(1)
        .map(static_or_const_int_value)
        .unwrap_or(Some(0))
        .ok_or_else(|| {
            CompileError::new(
                call.span,
                "wasm32-web runtime json_encode() currently requires static flags",
            )
        })?;
            if let Some(depth) = args.get(2) {
                const_or_literal_int_arg(depth)?;
            }
    let Some(len) = module.array_length(&name) else {
        match module.array_layout(&name) {
            ArrayLayout::CompactInt => {
                emit_output_json_encode_runtime_compact_int_array(&name, flags, module);
                emit_json_last_error_none(module);
                return Ok(true);
            }
            ArrayLayout::Value => {
                if let Some(kind) = module.array_runtime_value_cell_kind(&name) {
                    emit_output_json_encode_runtime_value_array(call, &name, kind, flags, module)?;
                } else {
                    emit_output_json_encode_runtime_value_array_dynamic(&name, flags, module);
                };
                emit_json_last_error_none(module);
                return Ok(true);
            }
            ArrayLayout::Assoc => {
                if let Some(kind) = module.array_runtime_value_cell_kind(&name) {
                    emit_output_json_encode_runtime_assoc_array(call, &name, kind, flags, module)?;
                } else {
                    emit_output_json_encode_runtime_assoc_array_dynamic(&name, flags, module);
                };
                emit_json_last_error_none(module);
                return Ok(true);
            }
        }
    };
    match module.array_layout(&name) {
        ArrayLayout::CompactInt => emit_output_json_encode_compact_int_array(&name, len, flags, module),
        ArrayLayout::Value => emit_output_json_encode_value_array(call, &name, len, flags, module)?,
        ArrayLayout::Assoc => emit_output_json_encode_assoc_array(call, &name, len, flags, module)?,
    }
    emit_json_last_error_none(module);
    Ok(true)
}

fn json_encode_direct_array_call_is_supported(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "range"
            | "pathinfo"
            | "explode"
            | "str_split"
            | "array_values"
            | "array_reverse"
            | "array_keys"
            | "array_unique"
            | "array_flip"
            | "array_diff"
            | "array_intersect"
            | "array_merge"
            | "array_diff_key"
            | "array_intersect_key"
            | "array_slice"
            | "array_pad"
            | "array_chunk"
            | "array_map"
            | "array_filter"
            | "array_fill"
            | "array_fill_keys"
            | "array_combine"
            | "array_column"
            | "class_parents"
            | "class_implements"
            | "class_uses"
    )
}

fn materialize_json_method_array_source(
    source: &Expr,
    label: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let temp = module.next_label(label).trim_start_matches('$').to_string();
    module.declare_array_local(temp.clone());
    emit_array_assign(&temp, source, module)?;
    Ok(temp)
}

fn materialize_nested_json_source(
    source: &Expr,
    label: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let Some(metadata) = nested_array_metadata_for_access_expr(source, module) else {
        return Err(CompileError::new(
            source.span,
            "wasm32-web json_encode() requires nested array metadata",
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
                "wasm32-web json_encode() expected a nested array value",
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

pub(super) fn emit_json_encode_array_value_to_stack(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let Some(first_arg) = args.first() else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web json_encode() expects one to three arguments",
        ));
    };
    let name = match &first_arg.kind {
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Array) => name.clone(),
        ExprKind::FunctionCall { name, .. }
            if module.has_function(name) && module.function_return_kind(name) == Some(ValueKind::Array) =>
        {
            let temp = module.next_label("json_value_array_source");
            let temp_name = temp.trim_start_matches('$').to_string();
            module.declare_array_local(temp_name.clone());
            emit_array_assign(&temp_name, first_arg, module)?;
            temp_name
        }
        ExprKind::FunctionCall { name, .. } if json_encode_direct_array_call_is_supported(name) => {
            let temp = module.next_label("json_value_array_source");
            let temp_name = temp.trim_start_matches('$').to_string();
            module.declare_array_local(temp_name.clone());
            emit_array_assign(&temp_name, first_arg, module)?;
            temp_name
        }
        ExprKind::MethodCall { object, method, .. }
            if method_call_array_return_metadata(object, method, module).is_some() =>
        {
            materialize_json_method_array_source(first_arg, "json_value_method_array_source", module)?
        }
        ExprKind::NullsafeMethodCall { object, method, .. }
            if object_expr_is_known_non_null(object, module)
                && method_call_array_return_metadata(object, method, module).is_some() =>
        {
            materialize_json_method_array_source(first_arg, "json_value_nullsafe_method_array_source", module)?
        }
        ExprKind::StaticMethodCall { receiver, method, .. }
            if static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            materialize_json_method_array_source(first_arg, "json_value_static_method_array_source", module)?
        }
        ExprKind::DynamicStaticMethodCall { receiver, method, .. }
            if dynamic_static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            materialize_json_method_array_source(first_arg, "json_value_dynamic_static_method_array_source", module)?
        }
        ExprKind::ArrayAccess { .. } if nested_array_metadata_for_access_expr(first_arg, module).is_some() => {
            materialize_nested_json_source(first_arg, "json_value_nested_array_source", module)?
        }
        _ => return Ok(false),
    };
    let flags = args
        .get(1)
        .map(static_or_const_int_value)
        .unwrap_or(Some(0))
        .ok_or_else(|| {
            CompileError::new(
                call.span,
                "wasm32-web runtime json_encode() string values currently require static flags",
            )
        })?;
    if let Some(depth) = args.get(2) {
        const_or_literal_int_arg(depth)?;
    }
    if let Some(len) = module.array_length(&name) {
        if emit_json_encode_known_array_value_to_stack(call, &name, len, flags, module)? {
            emit_json_last_error_none(module);
            return Ok(true);
        }
    }
    match (module.array_layout(&name), module.array_length(&name)) {
        (ArrayLayout::CompactInt, _) => {
            emit_json_encode_runtime_compact_int_array_value_to_stack(&name, flags, module);
            emit_json_last_error_none(module);
            Ok(true)
        }
        (ArrayLayout::Value, None) => {
            if let Some(kind) = homogeneous_array_value_kind(&name, module) {
                emit_json_encode_runtime_value_array_value_to_stack(call, &name, kind, flags, module)?;
            } else {
                emit_json_encode_runtime_value_array_dynamic_value_to_stack(&name, flags, module);
            };
            emit_json_last_error_none(module);
            Ok(true)
        }
        (ArrayLayout::Value, Some(_)) => {
            if let Some(kind) = homogeneous_array_value_kind(&name, module) {
                emit_json_encode_runtime_value_array_value_to_stack(call, &name, kind, flags, module)?;
            } else {
                emit_json_encode_runtime_value_array_dynamic_value_to_stack(&name, flags, module);
            };
            emit_json_last_error_none(module);
            Ok(true)
        }
        (ArrayLayout::Assoc, _) => {
            let kind = module
                .array_runtime_value_cell_kind(&name)
                .or_else(|| module.array_value_cell_kinds(&name).and_then(homogeneous_assoc_value_set_kind));
            if let Some(kind) = kind {
                emit_json_encode_runtime_assoc_array_value_to_stack(call, &name, kind, flags, module)?;
            } else {
                emit_json_encode_runtime_assoc_array_dynamic_value_to_stack(&name, flags, module);
            };
            emit_json_last_error_none(module);
            Ok(true)
        }
    }
}

fn emit_json_encode_known_array_value_to_stack(
    call: &Expr,
    name: &str,
    len: usize,
    flags: i64,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let metadata = NestedArrayMetadata {
        layout: module.array_layout(name),
        len,
        value_kinds: module.array_value_cell_kinds(name).map(|kinds| kinds.to_vec()),
        key_values: module.array_key_values(name).map(|keys| keys.to_vec()),
        nested_values: module.array_nested_value_metadata_items(name).map(|items| items.to_vec()),
    };
    if metadata.layout == ArrayLayout::CompactInt {
        return Ok(false);
    }
    if !json_metadata_has_nested_array_values(&metadata) {
        return Ok(false);
    }
    validate_json_metadata_for_string_value(call.span, &metadata)?;
    let out_ptr = module.next_label("json_known_array_out_ptr");
    let out_len = module.next_label("json_known_array_out_len");
    for local in [&out_ptr, &out_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_len));
    emit_json_metadata_bound(
        &format!("${}_ptr", name),
        &metadata,
        flags,
        &out_len,
        module,
    )?;
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_len));
    emit_json_array_from_metadata_to_memory(
        &format!("${}_ptr", name),
        &metadata,
        flags,
        call.span,
        &out_ptr,
        &out_len,
        module,
    )?;
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_len));
    Ok(true)
}

fn json_metadata_has_nested_array_values(metadata: &NestedArrayMetadata) -> bool {
    metadata
        .value_kinds
        .as_ref()
        .is_some_and(|kinds| kinds.iter().any(|kind| *kind == ValueCellKind::Array))
        || metadata
            .nested_values
            .as_ref()
            .is_some_and(|items| items.iter().flatten().any(json_metadata_has_nested_array_values))
}

fn validate_json_metadata_for_string_value(
    span: Span,
    metadata: &NestedArrayMetadata,
) -> Result<(), CompileError> {
    match metadata.layout {
        ArrayLayout::CompactInt => Ok(()),
        ArrayLayout::Value | ArrayLayout::Assoc => {
            let Some(kinds) = metadata.value_kinds.as_ref() else {
                return Err(CompileError::new(
                    span,
                    "wasm32-web json_encode() string values require exact value metadata",
                ));
            };
            if kinds.len() != metadata.len
                || json_value_kinds_need_missing_nested_metadata(
                    kinds,
                    metadata.nested_values.as_deref(),
                )
            {
                return Err(CompileError::new(
                    span,
                    "wasm32-web json_encode() string values require known nested metadata",
                ));
            }
            if metadata.layout == ArrayLayout::Assoc
                && metadata
                    .key_values
                    .as_ref()
                    .is_none_or(|keys| keys.len() != metadata.len)
            {
                return Err(CompileError::new(
                    span,
                    "wasm32-web json_encode() associative string values require exact keys",
                ));
            }
            if let Some(nested_values) = metadata.nested_values.as_ref() {
                for nested in nested_values.iter().flatten() {
                    validate_json_metadata_for_string_value(span, nested)?;
                }
            }
            Ok(())
        }
    }
}

fn emit_json_metadata_bound(
    ptr: &str,
    metadata: &NestedArrayMetadata,
    flags: i64,
    out_len: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_json_add_const_bound(2, out_len, module);
    match metadata.layout {
        ArrayLayout::CompactInt => {
            for index in 0..metadata.len {
                emit_json_add_static_separator_and_key_bound(index, Some(index as i64), flags, out_len, module);
                emit_json_add_const_bound(64, out_len, module);
            }
            Ok(())
        }
        ArrayLayout::Value => {
            let kinds = metadata.value_kinds.as_ref().expect("validated value metadata");
            for (index, kind) in kinds.iter().enumerate() {
                emit_json_add_static_separator_and_key_bound(index, Some(index as i64), flags, out_len, module);
                let cell = module.next_label("json_bound_value_cell");
                module.declare_i32_local(cell.trim_start_matches('$').to_string());
                module.body().line(&format!("local.get {}", ptr));
                module
                    .body()
                    .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
                module.body().line("i32.add");
                module.body().line(&format!("local.set {}", cell));
                let nested = metadata
                    .nested_values
                    .as_ref()
                    .and_then(|items| items.get(index))
                    .and_then(Option::as_ref);
                emit_json_value_cell_bound(&cell, *kind, nested, flags, out_len, module)?;
            }
            Ok(())
        }
        ArrayLayout::Assoc => {
            let keys = metadata.key_values.as_ref().expect("validated key metadata");
            let kinds = metadata.value_kinds.as_ref().expect("validated value metadata");
            let as_list = flags & 16 == 0 && assoc_key_values_are_json_list(keys);
            for index in 0..metadata.len {
                emit_json_add_static_separator_bound(index, out_len, module);
                if !as_list {
                    emit_json_add_assoc_key_bound(&keys[index], flags, out_len, module);
                }
                let cell = module.next_label("json_bound_assoc_value_cell");
                module.declare_i32_local(cell.trim_start_matches('$').to_string());
                module.body().line(&format!("local.get {}", ptr));
                module
                    .body()
                    .line(&format!("i32.const {}", index * WASM_ASSOC_ENTRY_SIZE + 16));
                module.body().line("i32.add");
                module.body().line(&format!("local.set {}", cell));
                let nested = metadata
                    .nested_values
                    .as_ref()
                    .and_then(|items| items.get(index))
                    .and_then(Option::as_ref);
                emit_json_value_cell_bound(&cell, kinds[index], nested, flags, out_len, module)?;
            }
            Ok(())
        }
    }
}

fn emit_json_value_cell_bound(
    cell: &str,
    kind: ValueCellKind,
    nested: Option<&NestedArrayMetadata>,
    flags: i64,
    out_len: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match kind {
        ValueCellKind::Str => {
            module.body().line(&format!("local.get {}", out_len));
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line("i32.const 6");
            module.body().line("i32.mul");
            module.body().line("i32.const 32");
            module.body().line("i32.add");
            module.body().line("i32.add");
            module.body().line(&format!("local.set {}", out_len));
            Ok(())
        }
        ValueCellKind::Array => {
            let nested = nested.expect("validated nested metadata");
            let ptr = module.next_label("json_bound_nested_ptr");
            module.declare_i32_local(ptr.trim_start_matches('$').to_string());
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.set {}", ptr));
            emit_json_metadata_bound(&ptr, nested, flags, out_len, module)
        }
        ValueCellKind::Int | ValueCellKind::Float | ValueCellKind::Bool | ValueCellKind::Null => {
            emit_json_add_const_bound(64, out_len, module);
            Ok(())
        }
    }
}

fn emit_json_add_static_separator_and_key_bound(
    index: usize,
    key: Option<i64>,
    flags: i64,
    out_len: &str,
    module: &mut WasmModule,
) {
    emit_json_add_static_separator_bound(index, out_len, module);
    if flags & 16 != 0 {
        let key_len = json_quote(&key.unwrap_or(index as i64).to_string(), flags).len() + 1;
        emit_json_add_const_bound(key_len, out_len, module);
    }
}

fn emit_json_add_static_separator_bound(index: usize, out_len: &str, module: &mut WasmModule) {
    if index > 0 {
        emit_json_add_const_bound(1, out_len, module);
    }
}

fn emit_json_add_assoc_key_bound(
    key: &AssocKeyValue,
    flags: i64,
    out_len: &str,
    module: &mut WasmModule,
) {
    let text = match key {
        AssocKeyValue::Int(value) => json_quote(&value.to_string(), flags),
        AssocKeyValue::Str(value) => json_quote(value, flags),
    };
    emit_json_add_const_bound(text.len() + 1, out_len, module);
}

fn emit_json_add_const_bound(amount: usize, out_len: &str, module: &mut WasmModule) {
    if amount == 0 {
        return;
    }
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("i32.const {}", amount));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_len));
}

fn emit_json_array_from_metadata_to_memory(
    ptr: &str,
    metadata: &NestedArrayMetadata,
    flags: i64,
    span: Span,
    out_ptr: &str,
    out_len: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match metadata.layout {
        ArrayLayout::CompactInt => {
            emit_json_compact_int_from_metadata_to_memory(ptr, metadata.len, flags, out_ptr, out_len, module);
            Ok(())
        }
        ArrayLayout::Value => {
            emit_json_value_array_from_metadata_to_memory(ptr, metadata, flags, span, out_ptr, out_len, module)
        }
        ArrayLayout::Assoc => {
            emit_json_assoc_array_from_metadata_to_memory(ptr, metadata, flags, span, out_ptr, out_len, module)
        }
    }
}

fn emit_json_compact_int_from_metadata_to_memory(
    ptr: &str,
    len: usize,
    flags: i64,
    out_ptr: &str,
    out_len: &str,
    module: &mut WasmModule,
) {
    emit_copy_literal_text_to_memory(if flags & 16 != 0 { "{" } else { "[" }, out_ptr, out_len, module);
    for index in 0..len {
        emit_json_static_separator_and_key_to_memory(index, Some(index as i64), flags, out_ptr, out_len, module);
        let part = module.next_label("json_known_int_part");
        module.declare_i32_local(format!("{}_ptr", part.trim_start_matches('$')));
        module.declare_i32_local(format!("{}_len", part.trim_start_matches('$')));
        module.body().line(&format!("local.get {}", ptr));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line("i64.load");
        emit_i64_stack_string_cast_value_to_stack("json_known_int", module);
        module.body().line(&format!("local.set ${}_len", part.trim_start_matches('$')));
        module.body().line(&format!("local.set ${}_ptr", part.trim_start_matches('$')));
        emit_copy_string_to_memory(part.trim_start_matches('$'), out_ptr, out_len, module);
    }
    emit_copy_literal_text_to_memory(if flags & 16 != 0 { "}" } else { "]" }, out_ptr, out_len, module);
}

fn emit_json_value_array_from_metadata_to_memory(
    ptr: &str,
    metadata: &NestedArrayMetadata,
    flags: i64,
    span: Span,
    out_ptr: &str,
    out_len: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let kinds = metadata.value_kinds.as_ref().ok_or_else(|| {
        CompileError::new(
            span,
            "wasm32-web json_encode() nested string values require exact value metadata",
        )
    })?;
    emit_copy_literal_text_to_memory(if flags & 16 != 0 { "{" } else { "[" }, out_ptr, out_len, module);
    for (index, kind) in kinds.iter().enumerate() {
        emit_json_static_separator_and_key_to_memory(index, Some(index as i64), flags, out_ptr, out_len, module);
        let cell = module.next_label("json_known_value_cell");
        module.declare_i32_local(cell.trim_start_matches('$').to_string());
        module.body().line(&format!("local.get {}", ptr));
        module
            .body()
            .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", cell));
        let nested = metadata
            .nested_values
            .as_ref()
            .and_then(|items| items.get(index))
            .and_then(Option::as_ref);
        emit_json_value_cell_with_nested_to_memory(&cell, *kind, nested, flags, span, out_ptr, out_len, module)?;
    }
    emit_copy_literal_text_to_memory(if flags & 16 != 0 { "}" } else { "]" }, out_ptr, out_len, module);
    Ok(())
}

fn emit_json_assoc_array_from_metadata_to_memory(
    ptr: &str,
    metadata: &NestedArrayMetadata,
    flags: i64,
    span: Span,
    out_ptr: &str,
    out_len: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let keys = metadata.key_values.as_ref().ok_or_else(|| {
        CompileError::new(
            span,
            "wasm32-web json_encode() nested associative string values require exact keys",
        )
    })?;
    let kinds = metadata.value_kinds.as_ref().ok_or_else(|| {
        CompileError::new(
            span,
            "wasm32-web json_encode() nested associative string values require exact values",
        )
    })?;
    let as_list = flags & 16 == 0 && assoc_key_values_are_json_list(keys);
    emit_copy_literal_text_to_memory(if as_list { "[" } else { "{" }, out_ptr, out_len, module);
    for index in 0..metadata.len {
        emit_json_static_separator_to_memory(index, out_ptr, out_len, module);
        if !as_list {
            emit_json_assoc_key_to_memory(&keys[index], flags, out_ptr, out_len, module);
        }
        let cell = module.next_label("json_known_assoc_cell");
        module.declare_i32_local(cell.trim_start_matches('$').to_string());
        module.body().line(&format!("local.get {}", ptr));
        module
            .body()
            .line(&format!("i32.const {}", index * WASM_ASSOC_ENTRY_SIZE + 16));
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", cell));
        let nested = metadata
            .nested_values
            .as_ref()
            .and_then(|items| items.get(index))
            .and_then(Option::as_ref);
        emit_json_value_cell_with_nested_to_memory(&cell, kinds[index], nested, flags, span, out_ptr, out_len, module)?;
    }
    emit_copy_literal_text_to_memory(if as_list { "]" } else { "}" }, out_ptr, out_len, module);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_json_value_cell_with_nested_to_memory(
    cell: &str,
    kind: ValueCellKind,
    nested: Option<&NestedArrayMetadata>,
    flags: i64,
    span: Span,
    out_ptr: &str,
    out_len: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if kind != ValueCellKind::Array {
        emit_json_value_cell_to_memory(cell, kind, out_ptr, out_len, flags, module);
        return Ok(());
    }
    let Some(nested) = nested else {
        return Err(CompileError::new(
            span,
            "wasm32-web json_encode() array string values require nested metadata",
        ));
    };
    let ptr = module.next_label("json_known_nested_ptr");
    module.declare_i32_local(ptr.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", ptr));
    emit_json_array_from_metadata_to_memory(&ptr, nested, flags, span, out_ptr, out_len, module)
}

fn emit_json_static_separator_and_key_to_memory(
    index: usize,
    key: Option<i64>,
    flags: i64,
    out_ptr: &str,
    out_len: &str,
    module: &mut WasmModule,
) {
    emit_json_static_separator_to_memory(index, out_ptr, out_len, module);
    if flags & 16 != 0 {
        let key = json_quote(&key.unwrap_or(index as i64).to_string(), flags);
        emit_copy_literal_text_to_memory(&key, out_ptr, out_len, module);
        emit_copy_literal_text_to_memory(":", out_ptr, out_len, module);
    }
}

fn emit_json_static_separator_to_memory(
    index: usize,
    out_ptr: &str,
    out_len: &str,
    module: &mut WasmModule,
) {
    if index > 0 {
        emit_copy_literal_text_to_memory(",", out_ptr, out_len, module);
    }
}

fn emit_json_assoc_key_to_memory(
    key: &AssocKeyValue,
    flags: i64,
    out_ptr: &str,
    out_len: &str,
    module: &mut WasmModule,
) {
    let key = match key {
        AssocKeyValue::Int(value) => json_quote(&value.to_string(), flags),
        AssocKeyValue::Str(value) => json_quote(value, flags),
    };
    emit_copy_literal_text_to_memory(&key, out_ptr, out_len, module);
    emit_copy_literal_text_to_memory(":", out_ptr, out_len, module);
}
