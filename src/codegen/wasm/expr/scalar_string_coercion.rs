//! Purpose:
//! Emits mixed/value-cell string coercion helpers for wasm32-web scalar operations.
//! Keeps dynamic string casting, strlen(), and ord() branching out of scalar operator dispatch.
//!
//! Called from:
//! - `super::scalar_ops` and string builtin lowering helpers.
//!
//! Key details:
//! - Preserves boxed Mixed/value-cell tags and rejects unsupported object/callable/resource coercions.

use super::*;

pub(in crate::codegen::wasm) fn known_mixed_string_coercion_kind(
    expr: &Expr,
    module: &WasmModule,
) -> Result<Option<ValueCellKind>, CompileError> {
    known_mixed_value_cell_kind(expr, module)
}

pub(in crate::codegen::wasm) fn unsupported_string_coercion_expr(expr: &Expr) -> Option<CompileError> {
    match &expr.kind {
        ExprKind::NewScopedObject { .. } => Some(CompileError::new(
            expr.span,
            "wasm32-web scoped object string coercion requires object runtime support",
        )),
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_) => {
            Some(CompileError::new(
                expr.span,
                "wasm32-web callable string coercion requires callable runtime support",
            ))
        }
        ExprKind::FunctionCall { name, .. } if is_resource_returning_builtin(name) => {
            Some(CompileError::new(
                expr.span,
                "wasm32-web resource string coercion requires resource runtime support",
            ))
        }
        _ => None,
    }
}

pub(in crate::codegen::wasm) fn unsupported_string_coercion_in_string_context(expr: &Expr) -> Option<CompileError> {
    if let Some(err) = unsupported_string_coercion_expr(expr) {
        return Some(err);
    }
    match &expr.kind {
        ExprKind::Cast {
            target: CastType::String,
            expr,
        } => unsupported_string_coercion_expr(expr),
        ExprKind::BinaryOp {
            op: BinOp::Concat,
            left,
            right,
        } => unsupported_string_coercion_in_string_context(left)
            .or_else(|| unsupported_string_coercion_in_string_context(right)),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => unsupported_string_coercion_in_string_context(then_expr)
            .or_else(|| unsupported_string_coercion_in_string_context(else_expr)),
        ExprKind::ShortTernary { value, default } => unsupported_string_coercion_in_string_context(value)
            .or_else(|| unsupported_string_coercion_in_string_context(default)),
        ExprKind::Match { arms, default, .. } => arms
            .iter()
            .find_map(|(_, value)| unsupported_string_coercion_in_string_context(value))
            .or_else(|| {
                default
                    .as_deref()
                    .and_then(unsupported_string_coercion_in_string_context)
            }),
        _ => None,
    }
}

pub(in crate::codegen::wasm) fn dynamic_scalar_mixed_string_coercion_supported(expr: &Expr, module: &WasmModule) -> bool {
    let ExprKind::ArrayAccess { array, index } = &expr.kind else {
        return false;
    };
    let ExprKind::Variable(name) = &array.kind else {
        return false;
    };
    if module.local_kind(name) != Some(LocalKind::Array) {
        return false;
    }
    let dynamic_index = match module.array_layout(name) {
        ArrayLayout::Value => static_or_const_or_i64_local_value(index, module).is_none(),
        ArrayLayout::Assoc => assoc_static_access_kind(array, index, module).is_none(),
        ArrayLayout::CompactInt => false,
    };
    if !dynamic_index {
        return false;
    }
    if let Some(kinds) = module.array_value_cell_kinds(name) {
        return kinds.iter().all(|kind| !matches!(kind, ValueCellKind::Array));
    }
    matches!(
        known_mixed_value_cell_kind(expr, module),
        Ok(Some(
            ValueCellKind::Int
                | ValueCellKind::Float
                | ValueCellKind::Bool
                | ValueCellKind::Str
                | ValueCellKind::Null
        ))
    )
}

pub(in crate::codegen::wasm) fn variable_index_scalar_mixed_string_coercion_candidate(expr: &Expr, module: &WasmModule) -> bool {
    let ExprKind::ArrayAccess { index, .. } = &expr.kind else {
        return false;
    };
    if !matches!(index.kind, ExprKind::Variable(_)) {
        return false;
    }
    matches!(
        known_mixed_value_cell_kind(expr, module),
        Ok(Some(
            ValueCellKind::Int
                | ValueCellKind::Float
                | ValueCellKind::Bool
                | ValueCellKind::Str
                | ValueCellKind::Null
        ))
    )
}

pub(in crate::codegen::wasm) fn unknown_mixed_string_coercion_candidate(expr: &Expr, module: &WasmModule) -> bool {
    match &expr.kind {
        ExprKind::Variable(name) => {
            module.local_kind(name) == Some(LocalKind::Mixed) && module.mixed_value_cell_kind(name).is_none()
        }
        ExprKind::FunctionCall { name, .. } => {
            if name.eq_ignore_ascii_case("array_search") {
                return true;
            }
            module.has_function(name)
                && module.function_return_kind(name) == Some(ValueKind::Mixed)
                && module.function_mixed_return_kind(name).is_none()
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            static_property_value_kind(receiver, property, module) == Some(ValueKind::Mixed)
        }
        ExprKind::NullsafeMethodCall { object, .. } => {
            !object_expr_is_known_non_null(object, module)
        }
        ExprKind::ArrayAccess { array, .. }
            if matches!(
                &array.kind,
                ExprKind::StaticPropertyAccess { receiver, property }
                    if static_property_value_kind(receiver, property, module) == Some(ValueKind::Mixed)
            ) =>
        {
            true
        }
        ExprKind::ArrayAccess { array, index } if nested_array_access_requires_layout(array) => {
            nested_array_index_metadata(array, index, module).is_some_and(|(metadata, _)| {
                metadata.layout == ArrayLayout::Value && metadata.value_kinds.is_none()
            })
        }
        _ => false,
    }
}

pub(in crate::codegen::wasm) fn materialize_dynamic_string_coercion_value_cell(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<Option<String>, CompileError> {
    let ExprKind::ArrayAccess { array, index } = &expr.kind else {
        return Ok(None);
    };
    let ExprKind::Variable(source) = &array.kind else {
        return Ok(None);
    };
    if module.local_kind(source) != Some(LocalKind::Array) || module.array_layout(source) != ArrayLayout::Value {
        return Ok(None);
    }
    if !matches!(index.kind, ExprKind::Variable(_)) {
        return Ok(None);
    }

    let local = module
        .next_label("mixed_dynamic_string_value")
        .trim_start_matches('$')
        .to_string();
    let index_local = module.next_label("mixed_dynamic_string_index");
    module.declare_i32_local(local.clone());
    module.declare_i32_local(index_local.trim_start_matches('$').to_string());
    emit_alloc_mixed_cell(&local, module);
    require_int(index, module)?;
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", index_local));
    module.body().line(&format!("local.get ${}", local));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.get {}", index_local));
    module.body().line("call $__rt_value_copy_index_or_null");
    Ok(Some(local))
}

pub(in crate::codegen::wasm) fn emit_known_mixed_string_cast_value_to_stack(
    cell: &str,
    kind: ValueCellKind,
    _span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match kind {
        ValueCellKind::Str => {
            emit_mixed_i32_payload(cell, 8, module);
            emit_mixed_i32_payload(cell, 12, module);
            Ok(())
        }
        ValueCellKind::Int => {
            emit_mixed_i64_payload(cell, module);
            emit_i64_stack_string_cast_value_to_stack("mixed_string_cast_int", module);
            Ok(())
        }
        ValueCellKind::Float => {
            emit_mixed_f64_payload(cell, module);
            emit_f64_stack_string_cast_value_to_stack("mixed_string_cast_float", module);
            Ok(())
        }
        ValueCellKind::Bool => {
            emit_mixed_i64_payload(cell, module);
            module.body().line("i64.eqz");
            module.body().line("i32.eqz");
            module.body().open("if (result i32 i32)");
            let (ptr, len) = module.intern_string("1");
            module.body().line(&format!("i32.const {}", ptr));
            module.body().line(&format!("i32.const {}", len));
            module.body().line("else");
            module.body().line("i32.const 0");
            module.body().line("i32.const 0");
            module.body().close("end");
            Ok(())
        }
        ValueCellKind::Null => {
            module.body().line("i32.const 0");
            module.body().line("i32.const 0");
            Ok(())
        }
        ValueCellKind::Array => {
            let (ptr, len) = module.intern_string("Array");
            module.body().line(&format!("i32.const {}", ptr));
            module.body().line(&format!("i32.const {}", len));
            Ok(())
        }
    }
}

pub(in crate::codegen::wasm) fn emit_dynamic_mixed_string_cast_value_to_stack(cell: &str, module: &mut WasmModule) {
    let tag = module.next_label("dynamic_mixed_string_tag");
    let out_ptr = module.next_label("dynamic_mixed_string_ptr");
    let out_len = module.next_label("dynamic_mixed_string_len");
    module.declare_i32_local(tag.trim_start_matches('$').to_string());
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(out_len.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("call $__rt_mixed_tag");
    module.body().line(&format!("local.set {}", tag));
    emit_dynamic_mixed_string_branch_string(cell, &tag, &out_ptr, &out_len, module);
    emit_dynamic_mixed_string_branch_int(cell, &tag, &out_ptr, &out_len, module);
    emit_dynamic_mixed_string_branch_float(cell, &tag, &out_ptr, &out_len, module);
    emit_dynamic_mixed_string_branch_bool(cell, &tag, &out_ptr, &out_len, module);
    emit_dynamic_mixed_string_branch_array(&tag, &out_ptr, &out_len, module);
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_len));
}

pub(in crate::codegen::wasm) fn emit_dynamic_mixed_strlen_length(cell: &str, module: &mut WasmModule) {
    let tag = module.next_label("dynamic_mixed_strlen_tag");
    let len = module.next_label("dynamic_mixed_strlen_len");
    module.declare_i32_local(tag.trim_start_matches('$').to_string());
    module.declare_i64_local(len.trim_start_matches('$').to_string());
    module.body().line("i64.const 0");
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("call $__rt_mixed_tag");
    module.body().line(&format!("local.set {}", tag));
    emit_dynamic_mixed_strlen_branch_string(cell, &tag, &len, module);
    emit_dynamic_mixed_strlen_branch_int(cell, &tag, &len, module);
    emit_dynamic_mixed_strlen_branch_float(cell, &tag, &len, module);
    emit_dynamic_mixed_strlen_branch_bool(cell, &tag, &len, module);
    emit_dynamic_mixed_strlen_branch_array(&tag, module);
    module.body().line(&format!("local.get {}", len));
}

pub(in crate::codegen::wasm) fn emit_dynamic_mixed_strlen_branch_string(
    cell: &str,
    tag: &str,
    len: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_mixed_i32_payload(cell, 12, module);
    module.body().line("i64.extend_i32_u");
    module.body().line(&format!("local.set {}", len));
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn emit_dynamic_mixed_strlen_branch_int(
    cell: &str,
    tag: &str,
    len: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_mixed_i64_payload(cell, module);
    emit_i64_stack_strlen_length("dynamic_mixed_strlen_int", module);
    module.body().line(&format!("local.set {}", len));
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn emit_dynamic_mixed_strlen_branch_float(
    cell: &str,
    tag: &str,
    len: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_mixed_f64_payload(cell, module);
    emit_f64_stack_strlen_length("dynamic_mixed_strlen_float", module);
    module.body().line(&format!("local.set {}", len));
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn emit_dynamic_mixed_strlen_branch_bool(
    cell: &str,
    tag: &str,
    len: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_mixed_i64_payload(cell, module);
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().line("i64.extend_i32_u");
    module.body().line(&format!("local.set {}", len));
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn emit_dynamic_mixed_strlen_branch_array(tag: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn emit_value_cell_pointer_ord_value(cell: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line("i32.eqz");
    module.body().open("if (result i64)");
    module.body().line("i64.const 0");
    module.body().line("else");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line("i32.load8_u");
    module.body().line("i64.extend_i32_u");
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn emit_dynamic_mixed_ord_value(cell: &str, module: &mut WasmModule) {
    let tag = module.next_label("dynamic_mixed_ord_tag");
    let result = module.next_label("dynamic_mixed_ord_result");
    module.declare_i32_local(tag.trim_start_matches('$').to_string());
    module.declare_i64_local(result.trim_start_matches('$').to_string());
    module.body().line("i64.const 0");
    module.body().line(&format!("local.set {}", result));
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("call $__rt_mixed_tag");
    module.body().line(&format!("local.set {}", tag));
    emit_dynamic_mixed_ord_branch_string(cell, &tag, &result, module);
    emit_dynamic_mixed_ord_branch_int(cell, &tag, &result, module);
    emit_dynamic_mixed_ord_branch_float(cell, &tag, &result, module);
    emit_dynamic_mixed_ord_branch_bool(cell, &tag, &result, module);
    emit_dynamic_mixed_ord_branch_array(&tag, module);
    module.body().line(&format!("local.get {}", result));
}

pub(in crate::codegen::wasm) fn emit_dynamic_mixed_ord_branch_string(
    cell: &str,
    tag: &str,
    result: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_value_cell_pointer_ord_value(&format!("${cell}"), module);
    module.body().line(&format!("local.set {}", result));
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn emit_dynamic_mixed_ord_branch_int(
    cell: &str,
    tag: &str,
    result: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_mixed_i64_payload(cell, module);
    emit_i64_stack_string_cast_value_to_stack("dynamic_mixed_ord_int", module);
    emit_stack_string_ord_value("dynamic_mixed_ord_int", module);
    module.body().line(&format!("local.set {}", result));
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn emit_dynamic_mixed_ord_branch_float(
    cell: &str,
    tag: &str,
    result: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_mixed_f64_payload(cell, module);
    emit_f64_stack_string_cast_value_to_stack("dynamic_mixed_ord_float", module);
    emit_stack_string_ord_value("dynamic_mixed_ord_float", module);
    module.body().line(&format!("local.set {}", result));
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn emit_dynamic_mixed_ord_branch_bool(
    cell: &str,
    tag: &str,
    result: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_mixed_i64_payload(cell, module);
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().open("if (result i64)");
    module.body().line("i64.const 49");
    module.body().line("else");
    module.body().line("i64.const 0");
    module.body().close("end");
    module.body().line(&format!("local.set {}", result));
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn emit_dynamic_mixed_ord_branch_array(tag: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn emit_dynamic_mixed_string_branch_string(
    cell: &str,
    tag: &str,
    out_ptr: &str,
    out_len: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_mixed_i32_payload(cell, 8, module);
    module.body().line(&format!("local.set {}", out_ptr));
    emit_mixed_i32_payload(cell, 12, module);
    module.body().line(&format!("local.set {}", out_len));
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn emit_dynamic_mixed_string_branch_int(
    cell: &str,
    tag: &str,
    out_ptr: &str,
    out_len: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_mixed_i64_payload(cell, module);
    emit_i64_stack_string_cast_value_to_stack("dynamic_mixed_string_int", module);
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn emit_dynamic_mixed_string_branch_float(
    cell: &str,
    tag: &str,
    out_ptr: &str,
    out_len: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_mixed_f64_payload(cell, module);
    emit_f64_stack_string_cast_value_to_stack("dynamic_mixed_string_float", module);
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn emit_dynamic_mixed_string_branch_bool(
    cell: &str,
    tag: &str,
    out_ptr: &str,
    out_len: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_mixed_i64_payload(cell, module);
    module.body().line("i64.eqz");
    module.body().line("i32.eqz");
    module.body().open("if");
    let (ptr, len) = module.intern_string("1");
    module.body().line(&format!("i32.const {}", ptr));
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set {}", out_len));
    module.body().close("end");
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn emit_dynamic_mixed_string_branch_array(
    tag: &str,
    out_ptr: &str,
    out_len: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    let (ptr, len) = module.intern_string("Array");
    module.body().line(&format!("i32.const {}", ptr));
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set {}", out_len));
    module.body().close("end");
}
