//! Purpose:
//! Lowers wasm32-web foreach statements over compact, value-cell, and associative
//! array layouts.
//!
//! Called from:
//! - `crate::codegen::wasm::stmt`
//!
//! Key details:
//! - Reuses the parent statement dispatcher for foreach bodies.
//! - Preserves runtime value-cell and associative key metadata while binding locals.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, Stmt};

mod assoc;
mod compact;
mod iterator;
mod value;

use super::emit_stmt;
use assoc::{emit_assoc_array_foreach, emit_assoc_array_foreach_by_ref};
use compact::{
    emit_compact_array_foreach, emit_compact_array_foreach_by_ref,
    emit_compact_foreach_int_value_assign,
    emit_indexed_foreach_key_assign,
};
use value::{emit_value_array_foreach, emit_value_array_foreach_by_ref};
use super::super::expr::{
    array_literal_needs_value_cells, emit_assoc_array_items_assign, emit_array_assign,
    callable_expr_return_kind, emit_array_value_to_stack, emit_expr, emit_value_array_items_assign,
    emit_value_cell_address_for_local, method_call_array_return_metadata,
    nested_array_metadata_for_access_expr, normalize_assoc_items, object_class_name_for_expr,
    static_method_call_array_return_metadata, dynamic_static_method_call_array_return_metadata,
};
use super::super::module::{
    ArrayLayout, AssocKeyKind, AssocKeyValue, ConstantArrayValue, LocalKind,
    NestedArrayMetadata, ValueKind, ValueCellKind, WasmModule,
};

const WASM_HEAP_KIND_INDEXED_ARRAY: i32 = 2;
const WASM_HEAP_KIND_ASSOC_ARRAY: i32 = 3;

pub(super) fn emit_foreach(
    array: &Expr,
    key_var: Option<&str>,
    value_var: &str,
    value_by_ref: bool,
    body: &[Stmt],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if value_by_ref {
        if let ExprKind::Variable(name) = &array.kind {
            if emit_unknown_mixed_array_foreach_by_ref(name, key_var, value_var, body, array.span, module)? {
                return Ok(());
            }
            if module.array_layout(name) == ArrayLayout::CompactInt {
                return emit_compact_array_foreach_by_ref(name, key_var, value_var, body, module);
            }
            if module.array_layout(name) == ArrayLayout::Value {
                return emit_value_array_foreach_by_ref(name, key_var, value_var, body, module);
            }
            if module.array_layout(name) == ArrayLayout::Assoc {
                return emit_assoc_array_foreach_by_ref(name, key_var, value_var, body, module);
            }
        }
        return Err(CompileError::new(
            array.span,
            "wasm32-web foreach by reference is not supported yet",
        ));
    }
    if let ExprKind::ConstRef(name) = &array.kind {
        match module.array_constant_value(name) {
            Some(ConstantArrayValue::Indexed(items)) => {
                let rewritten = Expr::new(ExprKind::ArrayLiteral(items), array.span);
                return emit_foreach(&rewritten, key_var, value_var, false, body, module);
            }
            Some(ConstantArrayValue::Assoc(items)) => {
                let rewritten = Expr::new(
                    ExprKind::ArrayLiteralAssoc(normalize_assoc_items(&items).unwrap_or(items)),
                    array.span,
                );
                return emit_foreach(&rewritten, key_var, value_var, false, body, module);
            }
            None => {}
        }
    }
    if let ExprKind::ArrayLiteralAssoc(items) = &array.kind {
        let temp = module
            .next_label("foreach_assoc_array")
            .trim_start_matches('$')
            .to_string();
        module.declare_array_local(temp.clone());
        emit_assoc_array_items_assign(&temp, items, module)?;
        return emit_assoc_array_foreach(&temp, key_var, value_var, body, module);
    }
    if let Some(class_name) = object_class_name_for_expr(array, module) {
        if module.object_class_implements_interface_or_parent(&class_name, "Iterator") {
            return iterator::emit_iterator_object_foreach(
                array,
                &class_name,
                key_var,
                value_var,
                body,
                module,
            );
        }
        if module.object_class_implements_interface_or_parent(&class_name, "IteratorAggregate") {
            return iterator::emit_iterator_aggregate_object_foreach(
                array,
                &class_name,
                key_var,
                value_var,
                body,
                module,
            );
        }
    }
    if iterator::expr_declared_as_iterator(array, module) {
        return iterator::emit_dynamic_iterator_object_foreach(
            array,
            key_var,
            value_var,
            body,
            module,
        );
    }
    if let ExprKind::StaticMethodCall {
        receiver,
        method,
        args,
    } = &array.kind
    {
        if method.eq_ignore_ascii_case("cases")
            && args.is_empty()
            && module
                .class_name_for_receiver(receiver)
                .and_then(|class_name| module.enum_case_names(&class_name))
                .is_some()
        {
            let temp = module
                .next_label("foreach_enum_cases")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            return emit_value_array_foreach(&temp, key_var, value_var, body, module);
        }
        if static_method_call_array_return_metadata(receiver, method, module).is_some() {
            let temp = module
                .next_label("foreach_static_method_array_return")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                return emit_assoc_array_foreach(&temp, key_var, value_var, body, module);
            }
            if module.array_layout(&temp) == ArrayLayout::Value {
                return emit_value_array_foreach(&temp, key_var, value_var, body, module);
            }
            return emit_compact_array_foreach(&temp, key_var, value_var, body, module);
        }
    }
    if let ExprKind::DynamicStaticMethodCall {
        receiver,
        method,
        ..
    } = &array.kind
    {
        if dynamic_static_method_call_array_return_metadata(receiver, method, module).is_some() {
            let temp = module
                .next_label("foreach_dynamic_static_method_array_return")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                return emit_assoc_array_foreach(&temp, key_var, value_var, body, module);
            }
            if module.array_layout(&temp) == ArrayLayout::Value {
                return emit_value_array_foreach(&temp, key_var, value_var, body, module);
            }
            return emit_compact_array_foreach(&temp, key_var, value_var, body, module);
        }
    }
    if let ExprKind::FunctionCall { name, .. } = &array.kind {
        if module.has_function(name)
            && module.function_return_kind(name) == Some(ValueKind::Array)
            && module.function_array_return_layout(name) == ArrayLayout::Assoc
        {
            let temp = module
                .next_label("foreach_assoc_return")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            return emit_assoc_array_foreach(&temp, key_var, value_var, body, module);
        }
        if module.has_function(name)
            && module.function_return_kind(name) == Some(ValueKind::Array)
            && module.function_array_return_layout(name) == ArrayLayout::Value
        {
            let temp = module
                .next_label("foreach_value_return")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            return emit_value_array_foreach(&temp, key_var, value_var, body, module);
        }
        if is_materializable_foreach_array_call(name) {
            let temp = module
                .next_label("foreach_array_call")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            if module.array_layout(&temp) == ArrayLayout::Assoc
                && module.array_key_kinds(&temp).is_none()
                && array_filter_uses_index_keys(array, module)
            {
                module.set_array_key_kinds(&temp, Some(vec![AssocKeyKind::Int]));
            }
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                return emit_assoc_array_foreach(&temp, key_var, value_var, body, module);
            }
            if module.array_layout(&temp) == ArrayLayout::Value {
                return emit_value_array_foreach(&temp, key_var, value_var, body, module);
            }
            return emit_compact_array_foreach(&temp, key_var, value_var, body, module);
        }
    }
    if let ExprKind::ExprCall { callee, args } = &array.kind {
        if callable_expr_return_kind(module, callee, args) == Some(ValueKind::Array) {
            let temp = module
                .next_label("foreach_callable_expr_array_return")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                return emit_assoc_array_foreach(&temp, key_var, value_var, body, module);
            }
            if module.array_layout(&temp) == ArrayLayout::Value {
                return emit_value_array_foreach(&temp, key_var, value_var, body, module);
            }
            return emit_compact_array_foreach(&temp, key_var, value_var, body, module);
        }
    }
    if let ExprKind::MethodCall { object, method, .. }
    | ExprKind::NullsafeMethodCall { object, method, .. } = &array.kind
    {
        if method_call_array_return_metadata(object, method, module).is_some() {
            let temp = module
                .next_label("foreach_method_array_return")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                return emit_assoc_array_foreach(&temp, key_var, value_var, body, module);
            }
            if module.array_layout(&temp) == ArrayLayout::Value {
                return emit_value_array_foreach(&temp, key_var, value_var, body, module);
            }
            return emit_compact_array_foreach(&temp, key_var, value_var, body, module);
        }
    }
    if let ExprKind::ArrayLiteral(items) = &array.kind {
        if array_literal_needs_value_cells(items) {
            let temp = module
                .next_label("foreach_value_array")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_value_array_items_assign(&temp, items, module)?;
            return emit_value_array_foreach(&temp, key_var, value_var, body, module);
        }
    }
    if emit_static_nested_array_access_foreach(array, key_var, value_var, body, module)? {
        return Ok(());
    }
    if emit_nested_array_value_foreach(array, key_var, value_var, body, module)? {
        return Ok(());
    }
    if let ExprKind::Variable(name) = &array.kind {
        if emit_unknown_mixed_array_foreach(name, key_var, value_var, body, array.span, module)? {
            return Ok(());
        }
        if module.array_layout(name) == ArrayLayout::Assoc {
            return emit_assoc_array_foreach(name, key_var, value_var, body, module);
        }
        if module.array_layout(name) == ArrayLayout::Value {
            return emit_value_array_foreach(name, key_var, value_var, body, module);
        }
    }
    let ptr = module.next_label("foreach_ptr");
    let len = module.next_label("foreach_len");
    let index = module.next_label("foreach_index");
    let break_label = module.next_label("foreach_break");
    let loop_label = module.next_label("foreach_loop");
    let continue_label = module.next_label("foreach_continue");
    for local in [&ptr, &len, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_array_value_to_stack(array, module)?;
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.set {}", ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", break_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", break_label));
    if let Some(key_var) = key_var {
        emit_indexed_foreach_key_assign(key_var, &index, array.span, module)?;
    }
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    emit_compact_foreach_int_value_assign(value_var, array.span, module)?;
    module.body().open(&format!("block {}", continue_label));
    module.push_loop(break_label.clone(), continue_label.clone());
    for stmt in body {
        emit_stmt(stmt, module)?;
    }
    module.pop_loop();
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

fn emit_unknown_mixed_array_foreach(
    source: &str,
    key_var: Option<&str>,
    value_var: &str,
    body: &[Stmt],
    span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if module.local_kind(source) != Some(LocalKind::Mixed) {
        return Ok(false);
    }
    if matches!(
        module.mixed_value_cell_kind(source),
        Some(kind) if kind != ValueCellKind::Array
    ) {
        return Ok(false);
    }
    if module.local_kind(value_var) != Some(LocalKind::Mixed) {
        return Err(CompileError::new(
            span,
            "wasm32-web unknown mixed array foreach requires a mixed value local",
        ));
    }
    if let Some(key_var) = key_var {
        if module.local_kind(key_var) != Some(LocalKind::Mixed) {
            return Err(CompileError::new(
                span,
                "wasm32-web unknown mixed array foreach requires a mixed key local",
            ));
        }
    }
    let temp = module
        .next_label("foreach_unknown_mixed_array")
        .trim_start_matches('$')
        .to_string();
    let heap_kind = module.next_label("foreach_unknown_mixed_heap_kind");
    module.declare_array_local(temp.clone());
    module.declare_i32_local(heap_kind.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_heap_kind");
    module.body().line(&format!("local.set {}", heap_kind));
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_ptr");
    module.body().line(&format!("local.set ${}_ptr", temp));
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_len_i32");
    module.body().line(&format!("local.set ${}_len", temp));
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_INDEXED_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.set_array_layout(&temp, ArrayLayout::Value);
    module.set_array_value_cell_kinds(&temp, None);
    emit_value_array_foreach(&temp, key_var, value_var, body, module)?;
    module.body().close("else");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.set_array_layout(&temp, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(&temp, None);
    module.set_array_key_kinds(&temp, None);
    emit_assoc_array_foreach(&temp, key_var, value_var, body, module)?;
    module.body().close("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    Ok(true)
}

fn emit_unknown_mixed_array_foreach_by_ref(
    source: &str,
    key_var: Option<&str>,
    value_var: &str,
    body: &[Stmt],
    span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if module.local_kind(source) != Some(LocalKind::Mixed) {
        return Ok(false);
    }
    if matches!(
        module.mixed_value_cell_kind(source),
        Some(kind) if kind != ValueCellKind::Array
    ) {
        return Ok(false);
    }
    if module.local_kind(value_var) != Some(LocalKind::Mixed) {
        return Err(CompileError::new(
            span,
            "wasm32-web unknown mixed array foreach by reference requires a mixed value local",
        ));
    }
    if let Some(key_var) = key_var {
        if module.local_kind(key_var) != Some(LocalKind::Mixed) {
            return Err(CompileError::new(
                span,
                "wasm32-web unknown mixed array foreach by reference requires a mixed key local",
            ));
        }
    }
    let temp = module
        .next_label("foreach_unknown_mixed_array_ref")
        .trim_start_matches('$')
        .to_string();
    let heap_kind = module.next_label("foreach_unknown_mixed_ref_heap_kind");
    let alias_cell = module.next_label("foreach_unknown_mixed_ref_cell");
    module.declare_array_local(temp.clone());
    module.declare_i32_local(heap_kind.trim_start_matches('$').to_string());
    module.declare_i32_local(alias_cell.trim_start_matches('$').to_string());
    module.set_mixed_ref_alias(value_var, &alias_cell);
    module.set_mixed_ref_alias_refresh(value_var, true);
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_heap_kind");
    module.body().line(&format!("local.set {}", heap_kind));
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_ptr");
    module.body().line(&format!("local.set ${}_ptr", temp));
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_len_i32");
    module.body().line(&format!("local.set ${}_len", temp));
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_INDEXED_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.set_array_layout(&temp, ArrayLayout::Value);
    module.set_array_value_cell_kinds(&temp, None);
    emit_value_array_foreach_by_ref(&temp, key_var, value_var, body, module)?;
    module.body().close("else");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.set_array_layout(&temp, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(&temp, None);
    module.set_array_key_kinds(&temp, None);
    emit_assoc_array_foreach_by_ref(&temp, key_var, value_var, body, module)?;
    module.body().close("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    module.set_mixed_ref_alias_refresh(value_var, false);
    Ok(true)
}

fn is_materializable_foreach_array_call(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "array_filter"
            | "array_map"
            | "array_values"
            | "array_keys"
            | "array_reverse"
            | "array_unique"
            | "array_flip"
            | "array_slice"
            | "array_merge"
            | "array_diff"
            | "array_intersect"
            | "array_diff_key"
            | "array_intersect_key"
            | "array_pad"
            | "array_fill"
            | "array_fill_keys"
            | "array_combine"
            | "array_column"
            | "array_chunk"
            | "array_rand"
            | "range"
            | "explode"
            | "str_split"
            | "pathinfo"
    )
}

fn emit_static_nested_array_access_foreach(
    array: &Expr,
    key_var: Option<&str>,
    value_var: &str,
    body: &[Stmt],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let ExprKind::ArrayAccess { array: outer, index } = &array.kind else {
        return Ok(false);
    };
    let ExprKind::Variable(source) = &outer.kind else {
        return Ok(false);
    };
    if module.local_kind(source) != Some(LocalKind::Array) || module.array_layout(source) != ArrayLayout::Value {
        return Ok(false);
    }
    let Some(index_value) = static_nonnegative_int(index) else {
        return Ok(false);
    };
    let Some(metadata) = module.array_nested_value_metadata(source, index_value) else {
        return Ok(false);
    };
    let temp = module
        .next_label("foreach_nested_array_access")
        .trim_start_matches('$')
        .to_string();
    let index_local = module.next_label("foreach_nested_index");
    let cell = module.next_label("foreach_nested_cell");
    module.declare_array_local(temp.clone());
    module.declare_i32_local(index_local.trim_start_matches('$').to_string());
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.body().line(&format!("i32.const {}", index_value));
    module.body().line(&format!("local.set {}", index_local));
    emit_value_cell_address_for_local(source, &index_local, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set ${}_ptr", temp));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set ${}_len", temp));
    apply_nested_array_metadata_to_local(&temp, metadata, module);
    match module.array_layout(&temp) {
        ArrayLayout::Assoc => emit_assoc_array_foreach(&temp, key_var, value_var, body, module)?,
        ArrayLayout::Value => emit_value_array_foreach(&temp, key_var, value_var, body, module)?,
        ArrayLayout::CompactInt => emit_compact_array_foreach(&temp, key_var, value_var, body, module)?,
    }
    Ok(true)
}

fn emit_nested_array_value_foreach(
    array: &Expr,
    key_var: Option<&str>,
    value_var: &str,
    body: &[Stmt],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let Some(metadata) = nested_array_metadata_for_access_expr(array, module) else {
        return Ok(false);
    };
    let temp = module
        .next_label("foreach_nested_array_value")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    match emit_expr(array, module)? {
        ValueKind::Array => {
            module.body().line(&format!("local.set ${}_len", temp));
            module.body().line(&format!("local.set ${}_ptr", temp));
        }
        _ => return Ok(false),
    }
    apply_nested_array_metadata_to_local(&temp, metadata, module);
    match module.array_layout(&temp) {
        ArrayLayout::Assoc => emit_assoc_array_foreach(&temp, key_var, value_var, body, module)?,
        ArrayLayout::Value => emit_value_array_foreach(&temp, key_var, value_var, body, module)?,
        ArrayLayout::CompactInt => emit_compact_array_foreach(&temp, key_var, value_var, body, module)?,
    }
    Ok(true)
}

fn static_nonnegative_int(expr: &Expr) -> Option<usize> {
    let ExprKind::IntLiteral(value) = expr.kind else {
        return None;
    };
    usize::try_from(value).ok()
}

fn apply_nested_array_metadata_to_local(
    name: &str,
    metadata: NestedArrayMetadata,
    module: &mut WasmModule,
) {
    let key_values = metadata.key_values.clone();
    module.set_array_layout(name, metadata.layout);
    module.set_array_length(name, metadata.len);
    module.set_array_value_cell_kinds(name, metadata.value_kinds);
    module.set_array_key_values(name, key_values.clone());
    module.set_array_key_kinds(
        name,
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
        name,
        metadata.layout == ArrayLayout::Assoc && module.array_key_kinds(name).is_none(),
    );
    module.set_array_nested_value_metadata(name, metadata.nested_values);
}

fn array_filter_uses_index_keys(array: &Expr, module: &WasmModule) -> bool {
    let ExprKind::FunctionCall { name, args } = &array.kind else {
        return false;
    };
    if matches!(name.to_ascii_lowercase().as_str(), "array_values" | "array_keys") {
        return true;
    }
    if !matches!(name.to_ascii_lowercase().as_str(), "array_filter" | "array_map") {
        return false;
    }
    let source_index = if name.eq_ignore_ascii_case("array_map") { 1 } else { 0 };
    let Some(source) = args.get(source_index) else {
        return false;
    };
    match &source.kind {
        ExprKind::ArrayLiteral(_) => true,
        ExprKind::ArrayLiteralAssoc(_) => false,
        ExprKind::Variable(name)
            if module.local_kind(name) == Some(LocalKind::Mixed)
                && module.mixed_value_cell_kind(name).is_none() =>
        {
            false
        }
        ExprKind::Variable(name) => module.array_layout(name) != ArrayLayout::Assoc,
        ExprKind::FunctionCall { name, .. } if module.has_function(name) => {
            module.function_array_return_layout(name) != ArrayLayout::Assoc
        }
        ExprKind::FunctionCall { name, .. }
            if matches!(name.to_ascii_lowercase().as_str(), "array_filter" | "array_map") =>
        {
            array_filter_uses_index_keys(source, module)
        }
        ExprKind::FunctionCall { name, .. }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "array_values"
                    | "array_keys"
                    | "array_fill"
                    | "array_pad"
                    | "array_chunk"
                    | "range"
                    | "explode"
                    | "str_split"
            ) =>
        {
            true
        }
        _ => false,
    }
}
