//! Purpose:
//! Provides shared wasm32-web helpers for value-cell array sorting and mutation.
//!
//! Called from:
//! - Sibling modules under `crate::codegen::wasm::expr`.
//!
//! Key details:
//! - These helpers operate on the wasm heap cell layout directly and preserve the
//!   boxed value-cell contract used by array mutators and sort emitters.
//! - Unsupported value-cell mutator paths still produce `CompileError` diagnostics.

use super::*;
use super::array_indexing_nested::emit_assoc_entry_matches_assoc_key_value;

pub(in crate::codegen::wasm) use super::array_value_cell_assoc_reads::emit_copy_assoc_access_to_value_cell_by_mixed_key;
pub(super) use super::array_value_cell_assoc_reads::emit_copy_assoc_access_to_value_cell;
pub(super) use super::array_value_cell_sort::*;

pub(super) fn reject_value_array_mutator(
    span: crate::span::Span,
    array: &str,
    function_name: &str,
    module: &WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(array) == ArrayLayout::Value {
        return Err(CompileError::new(
            span,
            &format!("wasm32-web {function_name}() does not support value-cell arrays yet"),
        ));
    }
    Ok(())
}

pub(super) fn emit_ensure_unique_array_payload(name: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line("call $__rt_ensure_unique");
    module.body().line(&format!("local.set ${}_ptr", name));
}

pub(in crate::codegen::wasm) fn array_literal_needs_value_cells(items: &[Expr]) -> bool {
    items.iter().any(expr_needs_value_cell)
}

pub(in crate::codegen::wasm) fn expr_needs_value_cell(expr: &Expr) -> bool {
    !matches!(
        expr.kind,
        ExprKind::IntLiteral(_) | ExprKind::ConstRef(_)
    ) && !matches!(
        &expr.kind,
        ExprKind::Negate(inner)
            if matches!(inner.kind, ExprKind::IntLiteral(_) | ExprKind::ConstRef(_))
    )
}

pub(in crate::codegen::wasm) fn emit_value_array_items_assign(
    name: &str,
    items: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_release_current_value_array(name, module);
    module.set_array_length(name, items.len());
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, value_cell_kinds_for_items(items, module));
    module.set_array_value_constants(name, value_cell_constants_for_items(items, module));
    module.set_array_nested_value_metadata(name, nested_array_metadata_for_items(items, module));
    module.set_array_object_classes(name, object_classes_for_items(items, module));
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", items.len()));
    module.body().line(&format!("local.set ${}_len", name));
    for (index, item) in items.iter().enumerate() {
        emit_value_array_store_expr(name, index, item, module)?;
    }
    Ok(())
}

fn object_classes_for_items(items: &[Expr], module: &WasmModule) -> Option<Vec<Option<String>>> {
    let classes = items
        .iter()
        .map(|item| object_class_name_for_expr(item, module))
        .collect::<Vec<_>>();
    classes.iter().any(Option::is_some).then_some(classes)
}

pub(in crate::codegen::wasm) fn emit_release_current_value_array(
    name: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("call $__rt_value_array_release");
}

pub(super) fn unsupported_runtime_value_cell_expr(value: &Expr) -> Option<CompileError> {
    match &value.kind {
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_) | ExprKind::ClosureCall { .. } => {
            Some(CompileError::new(
                value.span,
                "wasm32-web callable value cells require callable ownership runtime support",
            ))
        }
        ExprKind::FunctionCall { name, .. } if is_resource_returning_builtin(name) => {
            Some(CompileError::new(
                value.span,
                "wasm32-web resource value cells require resource ownership runtime support",
            ))
        }
        _ => None,
    }
}

pub(super) fn is_resource_returning_builtin(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "fopen" | "tmpfile" | "popen" | "opendir" | "fsockopen" | "pfsockopen"
            | "stream_socket_client" | "stream_socket_server"
    )
}

pub(in crate::codegen::wasm) fn emit_value_array_store_expr(
    name: &str,
    index: usize,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let cell = module.next_label("value_array_cell");
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
    emit_store_value_cell(&cell, value, module)
}

pub(in crate::codegen::wasm) fn emit_store_value_cell(
    cell: &str,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(err) = unsupported_runtime_value_cell_expr(value) {
        return Err(err);
    }
    match &value.kind {
        ExprKind::ArrayAccess { array, index } => {
            if expression_is_stringy(array, module) {
                module.body().line(&format!("local.get {}", cell));
                emit_string_index_to_stack(value, array, index, module)?;
                module.body().line("call $__rt_value_store_string");
                return Ok(());
            }
            if missing_direct_array_literal_index(array, index) {
                module.body().line(&format!("local.get {}", cell));
                module.body().line("call $__rt_value_store_null");
                return Ok(());
            }
            if expression_has_array_type(array, module)
                && !matches!(&array.kind, ExprKind::Variable(_))
            {
                let temp = module
                    .next_label("value_cell_array_access_source")
                    .trim_start_matches('$')
                    .to_string();
                module.declare_array_local(temp.clone());
                emit_array_assign(&temp, array, module)?;
                let temp_access = Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(Expr::new(ExprKind::Variable(temp), array.span)),
                        index: Box::new(*index.clone()),
                    },
                    value.span,
                );
                return emit_store_value_cell(cell, &temp_access, module);
            }
            if let ExprKind::Variable(source) = &array.kind {
                if module.local_kind(source) == Some(LocalKind::Mixed) {
                    emit_copy_unknown_mixed_array_access_to_value_cell(cell, source, index, value, module)?;
                    return Ok(());
                }
            }
            if nested_array_access_requires_layout(array) {
                emit_store_nested_array_access_to_value_cell(cell, value, array, index, module)?;
                return Ok(());
            }
            if let ExprKind::Variable(source) = &array.kind {
                if module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::Value
                {
                    emit_copy_array_access_to_value_cell(cell, source, index, value, module)?;
                    return Ok(());
                }
                if module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::Assoc
                {
                    emit_copy_assoc_access_to_value_cell(cell, source, index, value, module)?;
                    return Ok(());
                }
            }
            Err(array_unsupported(value))
        }
        ExprKind::Null => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
            module.body().line("i32.store");
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.const 0");
            module.body().line("i64.store");
            Ok(())
        }
        ExprKind::PropertyAccess { .. }
        | ExprKind::DynamicPropertyAccess { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::DynamicMethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::DynamicStaticMethodCall { .. }
        | ExprKind::ScopedConstantAccess { .. } => {
            let emitted = emit_expr(value, module)?;
            emit_store_emitted_value_kind(cell, emitted, "value_cell_property", module)
        }
        ExprKind::NullsafePropertyAccess { object, .. }
        | ExprKind::NullsafeDynamicPropertyAccess { object, .. }
        | ExprKind::NullsafeMethodCall { object, .. }
        | ExprKind::NullsafeDynamicMethodCall { object, .. }
            if matches!(object.kind, ExprKind::Null) =>
        {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("call $__rt_value_store_null");
            Ok(())
        }
        ExprKind::NullsafePropertyAccess { .. } | ExprKind::NullsafeDynamicPropertyAccess { .. } => {
            let emitted = emit_expr(value, module)?;
            emit_store_emitted_value_kind(cell, emitted, "value_cell_nullsafe_property", module)
        }
        ExprKind::NullsafeMethodCall { .. } => {
            let emitted = emit_expr(value, module)?;
            emit_store_emitted_value_kind(cell, emitted, "value_cell_nullsafe_method", module)
        }
        ExprKind::NullsafeDynamicMethodCall { .. } => {
            let emitted = emit_expr(value, module)?;
            emit_store_emitted_value_kind(cell, emitted, "value_cell_nullsafe_dynamic_method", module)
        }
        ExprKind::StringLiteral(value) => {
            let (ptr, len) = module.intern_string(value);
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("i32.const {}", ptr));
            module.body().line(&format!("i32.const {}", len));
            module.body().line("call $__rt_value_store_string");
            Ok(())
        }
        ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Str) => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get ${}_len", source));
            module.body().line("call $__rt_value_store_string");
            Ok(())
        }
        ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Mixed) => {
            emit_copy_value_cell_from_addr_to_addr(
                cell,
                &format!("${}", source),
                module,
            );
            Ok(())
        }
        ExprKind::FunctionCall { name, .. }
            if module.has_function(name)
                && module.function_return_kind(name) == Some(ValueKind::Mixed) =>
        {
            let temp = module
                .next_label("value_cell_mixed_return")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(temp.clone());
            emit_mixed_value_to_stack(value, module)?;
            module.body().line(&format!("local.set ${}", temp));
            emit_copy_value_cell_from_addr_to_addr(cell, &format!("${}", temp), module);
            emit_release_value_cell(&format!("${}", temp), module);
            Ok(())
        }
        ExprKind::BoolLiteral(value) => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("i32.const {}", i32::from(*value)));
            module.body().line("call $__rt_value_store_bool");
            Ok(())
        }
        ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::I32) => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("local.get ${}", source));
            module.body().line("call $__rt_value_store_bool");
            Ok(())
        }
        ExprKind::FloatLiteral(_) => {
            module.body().line(&format!("local.get {}", cell));
            require_float(value, module)?;
            module.body().line("call $__rt_value_store_float");
            Ok(())
        }
        ExprKind::Negate(inner) if expression_is_floaty(inner, module) => {
            module.body().line(&format!("local.get {}", cell));
            require_float(value, module)?;
            module.body().line("call $__rt_value_store_float");
            Ok(())
        }
        ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::F64) => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("local.get ${}", source));
            module.body().line("call $__rt_value_store_float");
            Ok(())
        }
        ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get ${}_len", source));
            module.body().line("call $__rt_value_store_array");
            Ok(())
        }
        ExprKind::NewObject { .. } | ExprKind::NewScopedObject { .. } => {
            module.body().line(&format!("local.get {}", cell));
            if emit_expr(value, module)? != ValueKind::Object {
                return Err(CompileError::new(
                    value.span,
                    "wasm32-web object value cells expected an object expression",
                ));
            }
            module.body().line("call $__rt_value_store_object");
            Ok(())
        }
        ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Object) => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("local.get ${}", source));
            module.body().line("call $__rt_value_store_object");
            Ok(())
        }
        ExprKind::FunctionCall { name, .. }
            if module.has_function(name)
                && module.function_return_kind(name) == Some(ValueKind::Array) =>
        {
            let temp = module
                .next_label("value_cell_array_return")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, value, module)?;
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("local.get ${}_ptr", temp));
            module.body().line(&format!("local.get ${}_len", temp));
            module.body().line("call $__rt_value_store_array");
            Ok(())
        }
        ExprKind::ArrayLiteral(items) => {
            let temp = module.next_label("value_cell_array").trim_start_matches('$').to_string();
            module.declare_array_local(temp.clone());
            emit_value_array_items_assign(&temp, items, module)?;
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("local.get ${}_ptr", temp));
            module.body().line(&format!("local.get ${}_len", temp));
            module.body().line("call $__rt_value_store_array");
            Ok(())
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            let temp = module
                .next_label("value_cell_assoc_array")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_assoc_array_items_assign(&temp, items, module)?;
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("local.get ${}_ptr", temp));
            module.body().line(&format!("local.get ${}_len", temp));
            module.body().line("call $__rt_value_store_array");
            Ok(())
        }
        _ => {
            if let Some(value) = static_scalar_value(value, module) {
                emit_store_static_scalar_value_cell(cell, value, module);
                return Ok(());
            }
            if value_cell_kind_for_expr(value, module)
                .is_some_and(|kind| kind != ValueCellKind::Array)
            {
                let emitted = emit_expr(value, module)?;
                return emit_store_emitted_value_kind(cell, emitted, "value_cell_scalar", module);
            }
            module.body().line(&format!("local.get {}", cell));
            require_int(value, module)?;
            module.body().line("call $__rt_value_store_int");
            Ok(())
        }
    }
}

fn emit_store_static_scalar_value_cell(
    cell: &str,
    value: ConstantValue,
    module: &mut WasmModule,
) {
    match value {
        ConstantValue::Int(value) => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("i64.const {}", value));
            module.body().line("call $__rt_value_store_int");
        }
        ConstantValue::Float(value) => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("f64.const {}", value));
            module.body().line("call $__rt_value_store_float");
        }
        ConstantValue::Bool(value) => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("i32.const {}", i32::from(value)));
            module.body().line("call $__rt_value_store_bool");
        }
        ConstantValue::Str(value) => {
            let (ptr, len) = module.intern_string(&value);
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("i32.const {}", ptr));
            module.body().line(&format!("i32.const {}", len));
            module.body().line("call $__rt_value_store_string");
        }
        ConstantValue::Null => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("call $__rt_value_store_null");
        }
    }
}

fn emit_copy_unknown_mixed_array_access_to_value_cell(
    cell: &str,
    source: &str,
    index: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if expression_is_inty(index, module) {
        emit_copy_unknown_mixed_array_int_access_to_value_cell(cell, source, index, module)?;
    } else if expression_is_stringy(index, module) {
        emit_copy_unknown_mixed_array_string_access_to_value_cell(cell, source, index, module)?;
    } else {
        return Err(CompileError::new(
            index.span,
            "wasm32-web unknown mixed array reads currently require an integer or string key",
        ));
    }
    let _ = value;
    Ok(())
}

fn emit_copy_unknown_mixed_array_int_access_to_value_cell(
    cell: &str,
    source: &str,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let key = module.next_label("unknown_mixed_array_read_key");
    let ptr = module.next_label("unknown_mixed_array_read_ptr");
    let len = module.next_label("unknown_mixed_array_read_len");
    let heap_kind = module.next_label("unknown_mixed_array_read_heap_kind");
    for local in [&key, &ptr, &len, &heap_kind] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    require_int(index, module)?;
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", key));
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
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", len));
    module.body().line(&format!("local.get {}", key));
    module.body().line("call $__rt_value_copy_index_or_null");
    module.body().close("else");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_scan_unknown_mixed_assoc_int_access_to_value_cell(cell, &ptr, &len, &key, module);
    module.body().close("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_copy_unknown_mixed_array_string_access_to_value_cell(
    cell: &str,
    source: &str,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let key_ptr = module.next_label("unknown_mixed_array_read_key_ptr");
    let key_len = module.next_label("unknown_mixed_array_read_key_len");
    let ptr = module.next_label("unknown_mixed_array_read_ptr");
    let len = module.next_label("unknown_mixed_array_read_len");
    let heap_kind = module.next_label("unknown_mixed_array_read_heap_kind");
    for local in [&key_ptr, &key_len, &ptr, &len, &heap_kind] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_string_value_to_stack(index, module)?;
    module.body().line(&format!("local.set {}", key_len));
    module.body().line(&format!("local.set {}", key_ptr));
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
    module.body().line(&format!("local.get {}", key_ptr));
    module.body().line(&format!("local.get {}", key_len));
    module.body().line("call $__rt_php_array_key_is_int");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", len));
    module.body().line(&format!("local.get {}", key_ptr));
    module.body().line(&format!("local.get {}", key_len));
    module.body().line("call $__rt_php_array_key_to_int");
    module.body().line("i32.wrap_i64");
    module.body().line("call $__rt_value_copy_index_or_null");
    module.body().close("else");
    emit_store_null_value_cell(cell, module);
    module.body().close("end");
    module.body().close("else");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_scan_unknown_mixed_assoc_string_access_to_value_cell(cell, &ptr, &len, &key_ptr, &key_len, module);
    module.body().close("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_scan_unknown_mixed_assoc_int_access_to_value_cell(
    cell: &str,
    ptr: &str,
    len: &str,
    key: &str,
    module: &mut WasmModule,
) {
    let index = module.next_label("unknown_mixed_assoc_read_index");
    let entry = module.next_label("unknown_mixed_assoc_read_entry");
    let source_cell = module.next_label("unknown_mixed_assoc_read_source_cell");
    let done_label = module.next_label("unknown_mixed_assoc_read_done");
    let loop_label = module.next_label("unknown_mixed_assoc_read_loop");
    for local in [&index, &entry, &source_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_store_null_value_cell(cell, module);
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
    module.body().line("i64.extend_i32_s");
    module.body().line("call $__rt_assoc_key_eq_int");
    module.body().open("if");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", source_cell));
    emit_copy_value_cell_from_addr_to_addr(cell, &source_cell, module);
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

fn emit_scan_unknown_mixed_assoc_string_access_to_value_cell(
    cell: &str,
    ptr: &str,
    len: &str,
    key_ptr: &str,
    key_len: &str,
    module: &mut WasmModule,
) {
    let index = module.next_label("unknown_mixed_assoc_string_read_index");
    let entry = module.next_label("unknown_mixed_assoc_string_read_entry");
    let source_cell = module.next_label("unknown_mixed_assoc_string_read_source_cell");
    let done_label = module.next_label("unknown_mixed_assoc_string_read_done");
    let loop_label = module.next_label("unknown_mixed_assoc_string_read_loop");
    for local in [&index, &entry, &source_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_store_null_value_cell(cell, module);
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
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", source_cell));
    emit_copy_value_cell_from_addr_to_addr(cell, &source_cell, module);
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

fn missing_direct_array_literal_index(array: &Expr, index: &Expr) -> bool {
    let ExprKind::ArrayLiteral(items) = &array.kind else {
        return false;
    };
    let Some(index) = static_or_const_int_value(index) else {
        return false;
    };
    let Ok(index) = usize::try_from(index) else {
        return true;
    };
    index >= items.len()
}

fn emit_store_nested_array_access_to_value_cell(
    cell: &str,
    value: &Expr,
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(kind) = nested_array_static_access_kind(array, index, module) else {
        if emit_copy_dynamic_nested_assoc_access_to_value_cell(cell, array, index, module)? {
            return Ok(());
        }
        let emitted = match emit_nested_array_index_expr(value, array, index, module) {
            Ok(emitted) => emitted,
            Err(err)
                if err.message
                    == "wasm32-web nested array traversal requires nested array layout metadata" =>
            {
                if emit_store_materialized_nested_parent_to_value_cell(
                    cell,
                    value,
                    array,
                    index,
                    module,
                )? {
                    return Ok(());
                }
                return Err(err);
            }
            Err(err) => return Err(err),
        };
        emit_store_emitted_value_kind(cell, emitted, "nested_mixed_dynamic", module)?;
        return Ok(());
    };
    match kind {
        ValueCellKind::Null => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
            module.body().line("i32.store");
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.const 0");
            module.body().line("i64.store");
        }
        ValueCellKind::Int => {
            let temp = module.next_label("nested_mixed_int");
            module.declare_i64_local(temp.trim_start_matches('$').to_string());
            emit_nested_array_index_expr(value, array, index, module)?;
            module.body().line(&format!("local.set {}", temp));
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("local.get {}", temp));
            module.body().line("call $__rt_value_store_int");
        }
        ValueCellKind::Float => {
            let temp = module.next_label("nested_mixed_float");
            module.declare_f64_local(temp.trim_start_matches('$').to_string());
            emit_nested_array_index_expr(value, array, index, module)?;
            module.body().line(&format!("local.set {}", temp));
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("local.get {}", temp));
            module.body().line("call $__rt_value_store_float");
        }
        ValueCellKind::Bool => {
            let temp = module.next_label("nested_mixed_bool");
            module.declare_i32_local(temp.trim_start_matches('$').to_string());
            emit_nested_array_index_expr(value, array, index, module)?;
            module.body().line(&format!("local.set {}", temp));
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("local.get {}", temp));
            module.body().line("call $__rt_value_store_bool");
        }
        ValueCellKind::Str => {
            let ptr = module.next_label("nested_mixed_str_ptr");
            let len = module.next_label("nested_mixed_str_len");
            module.declare_i32_local(ptr.trim_start_matches('$').to_string());
            module.declare_i32_local(len.trim_start_matches('$').to_string());
            emit_nested_array_index_expr(value, array, index, module)?;
            module.body().line(&format!("local.set {}", len));
            module.body().line(&format!("local.set {}", ptr));
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("local.get {}", ptr));
            module.body().line(&format!("local.get {}", len));
            module.body().line("call $__rt_value_store_string");
        }
        ValueCellKind::Array => {
            let ptr = module.next_label("nested_mixed_array_ptr");
            let len = module.next_label("nested_mixed_array_len");
            module.declare_i32_local(ptr.trim_start_matches('$').to_string());
            module.declare_i32_local(len.trim_start_matches('$').to_string());
            emit_nested_array_index_expr(value, array, index, module)?;
            module.body().line(&format!("local.set {}", len));
            module.body().line(&format!("local.set {}", ptr));
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("local.get {}", ptr));
            module.body().line(&format!("local.get {}", len));
            module.body().line("call $__rt_value_store_array");
        }
    }
    Ok(())
}

fn emit_store_materialized_nested_parent_to_value_cell(
    cell: &str,
    value: &Expr,
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let ExprKind::ArrayAccess {
        array: parent,
        index: outer_index,
    } = &array.kind
    else {
        return Ok(false);
    };
    if !expression_has_array_type(parent, module) || matches!(&parent.kind, ExprKind::Variable(_)) {
        return Ok(false);
    }
    let temp = module
        .next_label("value_cell_nested_array_access_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_array_assign(&temp, parent, module)?;
    let temp_outer_access = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(Expr::new(ExprKind::Variable(temp), parent.span)),
            index: Box::new(*outer_index.clone()),
        },
        array.span,
    );
    let temp_nested_access = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(temp_outer_access),
            index: Box::new(index.clone()),
        },
        value.span,
    );
    emit_store_value_cell(cell, &temp_nested_access, module)?;
    Ok(true)
}

fn emit_copy_dynamic_nested_assoc_access_to_value_cell(
    target_cell: &str,
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if emit_copy_dynamic_outer_nested_assoc_access_to_value_cell(target_cell, array, index, module)? {
        return Ok(true);
    }
    if emit_copy_heterogeneous_dynamic_middle_assoc_leaf_to_value_cell(
        target_cell,
        array,
        index,
        module,
    )? {
        return Ok(true);
    }
    let Some(metadata) = nested_array_metadata_for_access_expr(array, module)
        .or_else(|| dynamic_nested_array_value_metadata(array, module))
    else {
        return Ok(false);
    };
    if metadata.layout != ArrayLayout::Assoc {
        return Ok(false);
    }
    let ptr = module.next_label("mixed_dynamic_nested_assoc_ptr");
    let len = module.next_label("mixed_dynamic_nested_assoc_len");
    module.declare_i32_local(ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(len.trim_start_matches('$').to_string());
    match emit_expr(array, module)? {
        ValueKind::Array => {
            module.body().line(&format!("local.set {}", len));
            module.body().line(&format!("local.set {}", ptr));
        }
        _ => return Ok(false),
    }
    if let Some(key) = static_assoc_access_key(index, module) {
        emit_copy_dynamic_nested_assoc_scan_to_value_cell(
            target_cell,
            &ptr,
            &len,
            module,
            |entry, matched, module| {
                emit_assoc_entry_matches_assoc_key_value(entry, &key, matched, module);
            },
        );
        return Ok(true);
    }
    if let Some(key) = runtime_string_arg_or_materialize(index, "mixed_dynamic_nested_assoc_key", module)? {
        emit_copy_dynamic_nested_assoc_scan_to_value_cell(
            target_cell,
            &ptr,
            &len,
            module,
            |entry, matched, module| {
                emit_assoc_entry_matches_string_parts(
                    entry,
                    &format!("${}_ptr", key),
                    &format!("${}_len", key),
                    matched,
                    module,
                );
            },
        );
        return Ok(true);
    }
    if expression_is_inty(index, module) {
        let key = module.next_label("mixed_dynamic_nested_assoc_int_key");
        module.declare_i64_local(key.trim_start_matches('$').to_string());
        require_int(index, module)?;
        module.body().line(&format!("local.set {}", key));
        emit_copy_dynamic_nested_assoc_scan_to_value_cell(
            target_cell,
            &ptr,
            &len,
            module,
            |entry, matched, module| {
                module.body().line(&format!("local.get {}", entry));
                module.body().line(&format!("local.get {}", key));
                module.body().line("call $__rt_assoc_key_eq_int");
                module.body().line(&format!("local.set {}", matched));
            },
        );
        return Ok(true);
    }
    if let Some(key_name) = mixed_key_local_name(index, module) {
        emit_copy_dynamic_nested_assoc_scan_to_value_cell(
            target_cell,
            &ptr,
            &len,
            module,
            |entry, matched, module| {
                emit_assoc_entry_matches_mixed_key_cell(entry, key_name, matched, module);
            },
        );
        return Ok(true);
    }
    Ok(false)
}

fn emit_copy_heterogeneous_dynamic_middle_assoc_leaf_to_value_cell(
    target_cell: &str,
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let ExprKind::ArrayAccess {
        array: parent,
        index: middle_index,
    } = &array.kind
    else {
        return Ok(false);
    };
    if static_assoc_access_key(middle_index, module).is_some() {
        return Ok(false);
    }
    let Some(parent_metadata) = nested_array_metadata_for_access_expr(parent, module) else {
        return Ok(false);
    };
    if !nested_assoc_children_include_assoc_array(&parent_metadata) {
        return Ok(false);
    }
    let ptr = module.next_label("mixed_heterogeneous_middle_assoc_ptr");
    let len = module.next_label("mixed_heterogeneous_middle_assoc_len");
    module.declare_i32_local(ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(len.trim_start_matches('$').to_string());
    match emit_expr(array, module)? {
        ValueKind::Array => {
            module.body().line(&format!("local.set {}", len));
            module.body().line(&format!("local.set {}", ptr));
        }
        _ => return Ok(false),
    }
    let guard_assoc_heap = nested_assoc_children_include_non_assoc_array(&parent_metadata);
    if let Some(key) = static_assoc_access_key(index, module) {
        emit_copy_dynamic_middle_nested_assoc_scan_to_value_cell(
            target_cell,
            &ptr,
            &len,
            guard_assoc_heap,
            module,
            |entry, matched, module| {
                emit_assoc_entry_matches_assoc_key_value(entry, &key, matched, module);
            },
        );
        return Ok(true);
    }
    if let Some(key) = runtime_string_arg_or_materialize(
        index,
        "mixed_heterogeneous_middle_assoc_key",
        module,
    )? {
        emit_copy_dynamic_middle_nested_assoc_scan_to_value_cell(
            target_cell,
            &ptr,
            &len,
            guard_assoc_heap,
            module,
            |entry, matched, module| {
                emit_assoc_entry_matches_string_parts(
                    entry,
                    &format!("${}_ptr", key),
                    &format!("${}_len", key),
                    matched,
                    module,
                );
            },
        );
        return Ok(true);
    }
    if expression_is_inty(index, module) {
        let key = module.next_label("mixed_heterogeneous_middle_assoc_int_key");
        module.declare_i64_local(key.trim_start_matches('$').to_string());
        require_int(index, module)?;
        module.body().line(&format!("local.set {}", key));
        emit_copy_dynamic_middle_nested_assoc_scan_to_value_cell(
            target_cell,
            &ptr,
            &len,
            guard_assoc_heap,
            module,
            |entry, matched, module| {
                module.body().line(&format!("local.get {}", entry));
                module.body().line(&format!("local.get {}", key));
                module.body().line("call $__rt_assoc_key_eq_int");
                module.body().line(&format!("local.set {}", matched));
            },
        );
        return Ok(true);
    }
    if let Some(key_name) = mixed_key_local_name(index, module) {
        emit_copy_dynamic_middle_nested_assoc_scan_to_value_cell(
            target_cell,
            &ptr,
            &len,
            guard_assoc_heap,
            module,
            |entry, matched, module| {
                emit_assoc_entry_matches_mixed_key_cell(entry, key_name, matched, module);
            },
        );
        return Ok(true);
    }
    Ok(false)
}

fn nested_assoc_children_include_assoc_array(metadata: &NestedArrayMetadata) -> bool {
    metadata.layout == ArrayLayout::Assoc
        && homogeneous_nested_assoc_value_kind(metadata) == Some(ValueCellKind::Array)
        && metadata
            .nested_values
            .as_ref()
            .is_some_and(|children| {
                children
                    .iter()
                    .any(|child| child.as_ref().is_some_and(|child| child.layout == ArrayLayout::Assoc))
            })
}

fn nested_assoc_children_include_non_assoc_array(metadata: &NestedArrayMetadata) -> bool {
    metadata
        .nested_values
        .as_ref()
        .is_some_and(|children| {
            children
                .iter()
                .any(|child| child.as_ref().is_some_and(|child| child.layout != ArrayLayout::Assoc))
        })
}

fn emit_copy_dynamic_outer_nested_assoc_access_to_value_cell(
    target_cell: &str,
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let Some((outer_name, outer_index, metadata)) = dynamic_outer_nested_assoc_metadata(array, module)
    else {
        return Ok(false);
    };
    if metadata.layout != ArrayLayout::Assoc
        || homogeneous_nested_assoc_value_kind(&metadata).is_some()
    {
        return Ok(false);
    }
    let Some((ptr, len)) =
        emit_dynamic_outer_assoc_nested_array_parts(&outer_name, outer_index, module)?
    else {
        return Ok(false);
    };
    if let Some(key) = static_assoc_access_key(index, module) {
        emit_copy_dynamic_nested_assoc_scan_to_value_cell(
            target_cell,
            &ptr,
            &len,
            module,
            |entry, matched, module| {
                emit_assoc_entry_matches_assoc_key_value(entry, &key, matched, module);
            },
        );
        return Ok(true);
    }
    if let Some(key) = runtime_string_arg_or_materialize(
        index,
        "mixed_dynamic_outer_inner_nested_assoc_key",
        module,
    )? {
        emit_copy_dynamic_nested_assoc_scan_to_value_cell(
            target_cell,
            &ptr,
            &len,
            module,
            |entry, matched, module| {
                emit_assoc_entry_matches_string_parts(
                    entry,
                    &format!("${}_ptr", key),
                    &format!("${}_len", key),
                    matched,
                    module,
                );
            },
        );
        return Ok(true);
    }
    if expression_is_inty(index, module) {
        let key = module.next_label("mixed_dynamic_outer_inner_nested_assoc_int_key");
        module.declare_i64_local(key.trim_start_matches('$').to_string());
        require_int(index, module)?;
        module.body().line(&format!("local.set {}", key));
        emit_copy_dynamic_nested_assoc_scan_to_value_cell(
            target_cell,
            &ptr,
            &len,
            module,
            |entry, matched, module| {
                module.body().line(&format!("local.get {}", entry));
                module.body().line(&format!("local.get {}", key));
                module.body().line("call $__rt_assoc_key_eq_int");
                module.body().line(&format!("local.set {}", matched));
            },
        );
        return Ok(true);
    }
    if let Some(key_name) = mixed_key_local_name(index, module) {
        emit_copy_dynamic_nested_assoc_scan_to_value_cell(
            target_cell,
            &ptr,
            &len,
            module,
            |entry, matched, module| {
                emit_assoc_entry_matches_mixed_key_cell(entry, key_name, matched, module);
            },
        );
        return Ok(true);
    }
    Ok(false)
}

pub(in crate::codegen::wasm::expr) fn mixed_key_local_name<'a>(
    index: &'a Expr,
    module: &WasmModule,
) -> Option<&'a str> {
    let ExprKind::Variable(name) = &index.kind else {
        return None;
    };
    (module.local_kind(name) == Some(LocalKind::Mixed)).then_some(name.as_str())
}

fn emit_assoc_entry_matches_mixed_key_cell(
    entry: &str,
    key_name: &str,
    matched: &str,
    module: &mut WasmModule,
) {
    let key_tag = module.next_label("mixed_dynamic_nested_assoc_key_tag");
    module.declare_i32_local(key_tag.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}", key_name));
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", key_tag));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("local.get {}", key_tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", entry));
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
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get ${}", key_name));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("call $__rt_assoc_key_eq_int");
    module.body().line(&format!("local.set {}", matched));
    module.body().close("end");
    module.body().close("end");
}

fn emit_copy_dynamic_nested_assoc_scan_to_value_cell(
    target_cell: &str,
    ptr: &str,
    len: &str,
    module: &mut WasmModule,
    emit_match: impl Fn(&str, &str, &mut WasmModule),
) {
    let index = module.next_label("mixed_dynamic_nested_assoc_index");
    let entry = module.next_label("mixed_dynamic_nested_assoc_entry");
    let matched = module.next_label("mixed_dynamic_nested_assoc_matched");
    let source_cell = module.next_label("mixed_dynamic_nested_assoc_source_cell");
    let found = module.next_label("mixed_dynamic_nested_assoc_found");
    let done_label = module.next_label("mixed_dynamic_nested_assoc_done");
    let loop_label = module.next_label("mixed_dynamic_nested_assoc_loop");
    for local in [&index, &entry, &matched, &source_cell, &found] {
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
    emit_match(&entry, &matched, module);
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", source_cell));
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get {}", source_cell));
    module.body().line("call $__rt_value_copy");
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
    module.body().line(&format!("local.get {}", found));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line("call $__rt_value_store_null");
    module.body().close("end");
}

fn emit_copy_dynamic_nested_assoc_scan_to_value_cell_if_assoc_heap(
    target_cell: &str,
    ptr: &str,
    len: &str,
    module: &mut WasmModule,
    emit_match: impl Fn(&str, &str, &mut WasmModule),
) {
    module.body().line(&format!("local.get {}", ptr));
    module.body().line("call $__rt_heap_kind");
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_copy_dynamic_nested_assoc_scan_to_value_cell(target_cell, ptr, len, module, emit_match);
    module.body().close("else");
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line("call $__rt_value_store_null");
    module.body().close("end");
}

fn emit_copy_dynamic_middle_nested_assoc_scan_to_value_cell(
    target_cell: &str,
    ptr: &str,
    len: &str,
    guard_assoc_heap: bool,
    module: &mut WasmModule,
    emit_match: impl Fn(&str, &str, &mut WasmModule),
) {
    if guard_assoc_heap {
        emit_copy_dynamic_nested_assoc_scan_to_value_cell_if_assoc_heap(
            target_cell,
            ptr,
            len,
            module,
            emit_match,
        );
    } else {
        emit_copy_dynamic_nested_assoc_scan_to_value_cell(target_cell, ptr, len, module, emit_match);
    }
}

pub(in crate::codegen::wasm) fn emit_store_emitted_value_kind(
    cell: &str,
    kind: ValueKind,
    label_prefix: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match kind {
        ValueKind::Null => {
            module.body().line("drop");
            module.body().line(&format!("local.get {}", cell));
            module.body().line("call $__rt_value_store_null");
        }
        ValueKind::Int => {
            let temp = module.next_label(&format!("{}_int", label_prefix));
            module.declare_i64_local(temp.trim_start_matches('$').to_string());
            module.body().line(&format!("local.set {}", temp));
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("local.get {}", temp));
            module.body().line("call $__rt_value_store_int");
        }
        ValueKind::Float => {
            let temp = module.next_label(&format!("{}_float", label_prefix));
            module.declare_f64_local(temp.trim_start_matches('$').to_string());
            module.body().line(&format!("local.set {}", temp));
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("local.get {}", temp));
            module.body().line("call $__rt_value_store_float");
        }
        ValueKind::Bool => {
            let temp = module.next_label(&format!("{}_bool", label_prefix));
            module.declare_i32_local(temp.trim_start_matches('$').to_string());
            module.body().line(&format!("local.set {}", temp));
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("local.get {}", temp));
            module.body().line("call $__rt_value_store_bool");
        }
        ValueKind::Str => {
            let ptr = module.next_label(&format!("{}_str_ptr", label_prefix));
            let len = module.next_label(&format!("{}_str_len", label_prefix));
            module.declare_i32_local(ptr.trim_start_matches('$').to_string());
            module.declare_i32_local(len.trim_start_matches('$').to_string());
            module.body().line(&format!("local.set {}", len));
            module.body().line(&format!("local.set {}", ptr));
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("local.get {}", ptr));
            module.body().line(&format!("local.get {}", len));
            module.body().line("call $__rt_value_store_string");
        }
        ValueKind::Array => {
            let ptr = module.next_label(&format!("{}_array_ptr", label_prefix));
            let len = module.next_label(&format!("{}_array_len", label_prefix));
            module.declare_i32_local(ptr.trim_start_matches('$').to_string());
            module.declare_i32_local(len.trim_start_matches('$').to_string());
            module.body().line(&format!("local.set {}", len));
            module.body().line(&format!("local.set {}", ptr));
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("local.get {}", ptr));
            module.body().line(&format!("local.get {}", len));
            module.body().line("call $__rt_value_store_array");
        }
        ValueKind::Object => {
            let ptr = module.next_label(&format!("{}_object_ptr", label_prefix));
            module.declare_i32_local(ptr.trim_start_matches('$').to_string());
            module.body().line(&format!("local.set {}", ptr));
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("local.get {}", ptr));
            module.body().line("call $__rt_value_store_object");
        }
        ValueKind::Callable => {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                "wasm32-web callable value cells require callable runtime support",
            ));
        }
        ValueKind::Mixed => {
            let source_cell = module.next_label(&format!("{}_mixed_cell", label_prefix));
            module.declare_i32_local(source_cell.trim_start_matches('$').to_string());
            module.body().line(&format!("local.set {}", source_cell));
            module.body().line(&format!("local.get {}", cell));
            module.body().line(&format!("local.get {}", source_cell));
            module.body().line("call $__rt_value_copy");
        }
        ValueKind::Never => {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                "wasm32-web never-returning expressions cannot be stored in value cells",
            ));
        }
    }
    Ok(())
}

fn emit_copy_array_access_to_value_cell(
    cell: &str,
    source: &str,
    index: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(index) = static_or_const_int_value(index) {
        if let Some(len) = module.array_length(source) {
            let Ok(index) = usize::try_from(index) else {
                return Err(CompileError::new(
                    value.span,
                    "wasm32-web value-cell array access does not support missing indexes yet",
                ));
            };
            if index >= len {
                return Err(CompileError::new(
                    value.span,
                    "wasm32-web value-cell array access does not support missing indexes yet",
                ));
            }
            let source_cell = module.next_label("value_cell_source");
            module.declare_i32_local(source_cell.trim_start_matches('$').to_string());
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("call $__rt_value_cell");
            module.body().line(&format!("local.set {}", source_cell));
            emit_copy_value_cell_from_addr_to_addr(cell, &source_cell, module);
            return Ok(());
        }
        module.body().line(&format!("local.get {}", cell));
        module.body().line(&format!("local.get ${}_ptr", source));
        module.body().line(&format!("local.get ${}_len", source));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_copy_index_or_null");
        return Ok(());
    }

    let index_local = module.next_label("value_cell_access_index");
    module.declare_i32_local(index_local.trim_start_matches('$').to_string());
    require_int(index, module)?;
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", index_local));
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.get {}", index_local));
    module.body().line("call $__rt_value_copy_index_or_null");
    Ok(())
}
