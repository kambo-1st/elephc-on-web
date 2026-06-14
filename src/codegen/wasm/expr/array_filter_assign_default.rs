//! Purpose:
//! Lowers no-callback `array_filter` assignment paths for wasm32-web.
//! Keeps default truthiness filtering separate from callback and callback-mode filtering.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_filter_assign`
//!
//! Key details:
//! - Preserves assoc/value-cell metadata while filtering with PHP truthiness semantics.

use super::*;
use super::array_filter_default::{
    emit_array_filter_default_assoc_local_assign,
    emit_array_filter_default_compact_int_local_assign,
    emit_array_filter_default_runtime_assoc_local_assign,
    emit_array_filter_default_value_int_local_assign,
    emit_array_filter_default_value_object_local_assign,
    emit_array_filter_default_value_scalar_local_assign,
    emit_array_filter_default_value_string_local_assign,
};

pub(super) fn emit_array_filter_default_assign(
    name: &str,
    call: &Expr,
    source: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let source_expr_span = source.span;
    match &source.kind {
        ExprKind::ArrayLiteral(items) if !array_literal_needs_value_cells(items) => {
            emit_array_filter_default_literal_ints_assign(name, items, module)
        }
        ExprKind::ArrayLiteral(items) if array_map_items_are_ints(items, module) => {
            emit_array_filter_default_literal_ints_assign(name, items, module)
        }
        ExprKind::ArrayLiteral(items) if array_map_items_are_strings(items, module) => {
            emit_array_filter_default_literal_strings_assign(name, items, module)
        }
        ExprKind::ArrayLiteral(items) if array_filter_items_are_bools(items, module) => {
            emit_array_filter_default_literal_scalar_assign(name, items, ValueCellKind::Bool, module)
        }
        ExprKind::ArrayLiteral(items) if array_filter_items_are_floats(items, module) => {
            emit_array_filter_default_literal_scalar_assign(name, items, ValueCellKind::Float, module)
        }
        ExprKind::ArrayLiteral(items) if array_filter_items_are_nulls(items, module) => {
            emit_array_filter_default_literal_scalar_assign(name, items, ValueCellKind::Null, module)
        }
        ExprKind::ArrayLiteral(items) if array_filter_items_are_arrays(items, module) => {
            emit_array_filter_default_literal_scalar_assign(name, items, ValueCellKind::Array, module)
        }
        ExprKind::ArrayLiteralAssoc(items) if array_filter_assoc_items_are_supported(items, module) => {
            emit_array_filter_default_assoc_literal_assign(name, items, source_expr_span, module)
        }
        ExprKind::ArrayAccess { .. } if nested_array_metadata_for_access_expr(source, module).is_some() => {
            let temp = materialize_nested_array_filter_default_source(source, module)?;
            emit_array_filter_default_staged_assign(name, &temp, source_expr_span, module)
        }
        ExprKind::MethodCall { object, method, .. }
        | ExprKind::NullsafeMethodCall { object, method, .. }
            if method_call_array_return_metadata(object, method, module).is_some() =>
        {
            let temp = materialize_method_array_filter_default_source(source, object, method, module)?;
            emit_array_filter_default_staged_assign(name, &temp, source_expr_span, module)
        }
        ExprKind::StaticMethodCall { receiver, method, .. }
            if static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            let temp = materialize_static_method_array_filter_default_source(source, receiver, method, module)?;
            emit_array_filter_default_staged_assign(name, &temp, source_expr_span, module)
        }
        ExprKind::DynamicStaticMethodCall { receiver, method, .. }
            if dynamic_static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            let temp = materialize_dynamic_static_method_array_filter_default_source(
                source, receiver, method, module,
            )?;
            emit_array_filter_default_staged_assign(name, &temp, source_expr_span, module)
        }
        ExprKind::FunctionCall { .. } | ExprKind::ExprCall { .. }
            if expression_has_array_type(source, module) =>
        {
            emit_array_filter_default_array_expr_assign(name, source, source_expr_span, module)
        }
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } if method.eq_ignore_ascii_case("cases")
            && args.is_empty()
            && module
                .class_name_for_receiver(receiver)
                .and_then(|class_name| module.enum_case_names(&class_name))
                .is_some() =>
        {
            emit_array_filter_default_array_expr_assign(name, source, source_expr_span, module)
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::CompactInt =>
        {
            emit_array_filter_default_compact_int_local_assign(name, source, call.span, module)
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_ints(source, module) =>
        {
            emit_array_filter_default_value_int_local_assign(name, source, source_expr_span, module)
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_map_value_cells_are_strings(source, module) =>
        {
            emit_array_filter_default_value_string_local_assign(name, source, source_expr_span, module)
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_bools(source, module) =>
        {
            emit_array_filter_default_value_scalar_local_assign(
                name,
                source,
                source_expr_span,
                ValueCellKind::Bool,
                module,
            )
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_floats(source, module) =>
        {
            emit_array_filter_default_value_scalar_local_assign(
                name,
                source,
                source_expr_span,
                ValueCellKind::Float,
                module,
            )
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_nulls(source, module) =>
        {
            emit_array_filter_default_value_scalar_local_assign(
                name,
                source,
                source_expr_span,
                ValueCellKind::Null,
                module,
            )
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && module.array_object_classes(source).is_some() =>
        {
            emit_array_filter_default_value_object_local_assign(name, source, source_expr_span, module)
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Value
                && array_filter_value_cells_are_arrays(source, module) =>
        {
            emit_array_filter_default_value_scalar_local_assign(
                name,
                source,
                source_expr_span,
                ValueCellKind::Array,
                module,
            )
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_filter_assoc_value_cells_are_supported(source, module) =>
        {
            emit_array_filter_default_assoc_local_assign(name, source, source_expr_span, module)
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Array)
                && module.array_layout(source) == ArrayLayout::Assoc
                && array_filter_runtime_assoc_value_cells_are_supported(source, module) =>
        {
            emit_array_filter_default_runtime_assoc_local_assign(name, source, source_expr_span, module)
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Mixed)
                && module.mixed_value_cell_kind(source).is_none() =>
        {
            emit_array_filter_default_unknown_mixed_assign(name, source, module)
        }
        _ => Err(CompileError::new(
            source.span,
            "wasm32-web array_filter() without a callback currently supports homogeneous scalar or nested-array arrays",
        )),
    }
}

fn materialize_nested_array_filter_default_source(
    source: &Expr,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let Some(metadata) = nested_array_metadata_for_access_expr(source, module) else {
        return Err(CompileError::new(
            source.span,
            "wasm32-web array_filter() requires nested array metadata",
        ));
    };
    let temp = module
        .next_label("array_filter_default_nested_source")
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
                "wasm32-web array_filter() expected a nested array value",
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

fn materialize_method_array_filter_default_source(
    source: &Expr,
    object: &Expr,
    method: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let metadata = method_call_array_return_metadata(object, method, module).ok_or_else(|| {
        CompileError::new(
            source.span,
            "wasm32-web array_filter() requires method array return metadata",
        )
    })?;
    let temp = module
        .next_label("array_filter_default_method_source")
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
                "wasm32-web array_filter() expected an array-returning method value",
            ));
        }
    }
    module.set_array_layout(&temp, metadata.layout);
    if let Some(len) = metadata.len {
        module.set_array_length(&temp, len);
    }
    module.set_array_value_cell_kinds(&temp, metadata.value_kinds);
    module.set_array_value_constants(&temp, metadata.value_constants);
    module.set_array_runtime_value_cell_kind(&temp, metadata.runtime_value_kind);
    module.set_array_nested_value_metadata(&temp, metadata.nested_values);
    module.set_array_key_kinds(&temp, metadata.key_kinds);
    module.set_array_key_values(&temp, metadata.key_values);
    Ok(temp)
}

fn materialize_static_method_array_filter_default_source(
    source: &Expr,
    receiver: &StaticReceiver,
    method: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let metadata = static_method_call_array_return_metadata(receiver, method, module).ok_or_else(|| {
        CompileError::new(
            source.span,
            "wasm32-web array_filter() requires static method array return metadata",
        )
    })?;
    let temp = module
        .next_label("array_filter_default_static_method_source")
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
                "wasm32-web array_filter() expected an array-returning static method value",
            ));
        }
    }
    module.set_array_layout(&temp, metadata.layout);
    if let Some(len) = metadata.len {
        module.set_array_length(&temp, len);
    }
    module.set_array_value_cell_kinds(&temp, metadata.value_kinds);
    module.set_array_value_constants(&temp, metadata.value_constants);
    module.set_array_runtime_value_cell_kind(&temp, metadata.runtime_value_kind);
    module.set_array_nested_value_metadata(&temp, metadata.nested_values);
    module.set_array_key_kinds(&temp, metadata.key_kinds);
    module.set_array_key_values(&temp, metadata.key_values);
    Ok(temp)
}

fn materialize_dynamic_static_method_array_filter_default_source(
    source: &Expr,
    receiver: &StaticReceiver,
    method: &Expr,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let metadata = dynamic_static_method_call_array_return_metadata(receiver, method, module).ok_or_else(|| {
        CompileError::new(
            source.span,
            "wasm32-web array_filter() requires dynamic static method array return metadata",
        )
    })?;
    let temp = module
        .next_label("array_filter_default_dynamic_static_method_source")
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
                "wasm32-web array_filter() expected an array-returning dynamic static method value",
            ));
        }
    }
    module.set_array_layout(&temp, metadata.layout);
    if let Some(len) = metadata.len {
        module.set_array_length(&temp, len);
    }
    module.set_array_value_cell_kinds(&temp, metadata.value_kinds);
    module.set_array_value_constants(&temp, metadata.value_constants);
    module.set_array_runtime_value_cell_kind(&temp, metadata.runtime_value_kind);
    module.set_array_nested_value_metadata(&temp, metadata.nested_values);
    module.set_array_key_kinds(&temp, metadata.key_kinds);
    module.set_array_key_values(&temp, metadata.key_values);
    Ok(temp)
}

fn emit_array_filter_default_array_expr_assign(
    name: &str,
    source_expr: &Expr,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_filter_default_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_array_assign(&temp, source_expr, module)?;
    emit_array_filter_default_staged_assign(name, &temp, source_span, module)
}

fn emit_array_filter_default_staged_assign(
    name: &str,
    source: &str,
    source_span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) == ArrayLayout::CompactInt {
        return emit_array_filter_default_compact_int_local_assign(name, source, source_span, module);
    }
    if module.array_layout(source) == ArrayLayout::Value && array_map_value_cells_are_ints(source, module) {
        return emit_array_filter_default_value_int_local_assign(name, source, source_span, module);
    }
    if module.array_layout(source) == ArrayLayout::Value && array_map_value_cells_are_strings(source, module) {
        return emit_array_filter_default_value_string_local_assign(name, source, source_span, module);
    }
    if module.array_layout(source) == ArrayLayout::Value && array_filter_value_cells_are_bools(source, module) {
        return emit_array_filter_default_value_scalar_local_assign(
            name,
            source,
            source_span,
            ValueCellKind::Bool,
            module,
        );
    }
    if module.array_layout(source) == ArrayLayout::Value && array_filter_value_cells_are_floats(source, module) {
        return emit_array_filter_default_value_scalar_local_assign(
            name,
            source,
            source_span,
            ValueCellKind::Float,
            module,
        );
    }
    if module.array_layout(source) == ArrayLayout::Value && array_filter_value_cells_are_nulls(source, module) {
        return emit_array_filter_default_value_scalar_local_assign(
            name,
            source,
            source_span,
            ValueCellKind::Null,
            module,
        );
    }
    if module.array_layout(source) == ArrayLayout::Value && module.array_object_classes(source).is_some() {
        return emit_array_filter_default_value_object_local_assign(name, source, source_span, module);
    }
    if module.array_layout(source) == ArrayLayout::Value && array_filter_value_cells_are_arrays(source, module) {
        return emit_array_filter_default_value_scalar_local_assign(
            name,
            source,
            source_span,
            ValueCellKind::Array,
            module,
        );
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && array_filter_assoc_value_cells_are_supported(source, module)
    {
        return emit_array_filter_default_assoc_local_assign(name, source, source_span, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && module
            .array_runtime_value_cell_kind(source)
            .is_some_and(array_filter_value_cell_kind_is_supported)
    {
        return emit_array_filter_default_runtime_assoc_local_assign(name, source, source_span, module);
    }
    if module.array_layout(source) == ArrayLayout::Assoc
        && module.array_value_cell_kinds(source).is_none()
        && module.array_runtime_value_cell_kind(source).is_none()
    {
        return emit_array_filter_default_mixed_assoc_local_assign(name, source, module);
    }
    Err(CompileError::new(
        source_span,
        "wasm32-web array_filter() without a callback over direct array expressions requires homogeneous scalar or nested-array values",
    ))
}

fn emit_array_filter_default_unknown_mixed_assign(
    name: &str,
    source: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("array_filter_default_unknown_mixed_source")
        .trim_start_matches('$')
        .to_string();
    let heap_kind = module.next_label("array_filter_default_unknown_mixed_heap_kind");
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
    emit_array_filter_default_mixed_indexed_local_assign(name, &temp, module);
    module.body().close("else");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.set_array_layout(&temp, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(&temp, None);
    module.set_array_runtime_value_cell_kind(&temp, None);
    module.set_array_key_kinds(&temp, None);
    module.set_array_runtime_key_kind(&temp, None);
    module.set_array_php_normalized_runtime_keys(&temp, true);
    emit_array_filter_default_mixed_assoc_local_assign(name, &temp, module)?;
    module.body().close("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_array_filter_default_mixed_indexed_local_assign(
    name: &str,
    source: &str,
    module: &mut WasmModule,
) {
    let out_index = emit_array_filter_runtime_result_prelude(name, source, module);
    let index = module.next_label("array_filter_indexed_mixed_index");
    let value_cell = module.next_label("array_filter_indexed_mixed_value_cell");
    for local in [&index, &value_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.set_array_runtime_value_cell_kind(name, None);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    let loop_label = module.next_label("array_filter_indexed_mixed_loop");
    let done_label = module.next_label("array_filter_indexed_mixed_done");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module
        .body()
        .line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", value_cell));
    emit_value_cell_pointer_dynamic_truthiness(&value_cell, module);
    module.body().open("if");
    emit_array_filter_copy_value_entry_dynamic_key(name, source, &out_index, &index, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_array_filter_default_mixed_assoc_local_assign(
    name: &str,
    source: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let out_index = module.next_label("array_filter_assoc_mixed_out_index");
    let index = module.next_label("array_filter_assoc_mixed_index");
    let source_entry = module.next_label("array_filter_assoc_mixed_source_entry");
    let target_entry = module.next_label("array_filter_assoc_mixed_target_entry");
    let value_cell = module.next_label("array_filter_assoc_mixed_value_cell");
    for local in [&out_index, &index, &source_entry, &target_entry, &value_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.clear_array_length(name);
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    module.set_array_runtime_key_kind(name, module.array_runtime_key_kind(source));
    module.set_array_php_normalized_runtime_keys(
        name,
        module.array_has_php_normalized_runtime_keys(source),
    );
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    let loop_label = module.next_label("array_filter_assoc_mixed_loop");
    let done_label = module.next_label("array_filter_assoc_mixed_done");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", source_entry));
    emit_assoc_value_cell_address(&source_entry, &value_cell, module);
    emit_value_cell_pointer_dynamic_truthiness(&value_cell, module);
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    copy_assoc_entry(&target_entry, &source_entry, module);
    emit_array_filter_increment_len(name, &out_index, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}
