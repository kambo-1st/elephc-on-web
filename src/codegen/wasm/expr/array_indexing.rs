//! Purpose:
//! Lowers wasm32-web nested array reads, dynamic nested associative traversal,
//! and direct `array_chunk()` scalar-read shortcuts.
//!
//! Called from:
//! - `super::emit_expr()`, `super::emit_output_expr()`, and array assignment helpers.
//!
//! Key details:
//! - Nested reads depend on `NestedArrayMetadata` and explicit runtime scans for
//!   dynamic associative keys.
//! - Unsupported nested layouts remain compile errors instead of degrading to
//!   integer-only reads.

use super::*;
use super::array_indexing_metadata::*;
use super::array_indexing_value_cells::*;
pub(super) use super::array_indexing_nested::*;

pub(super) fn indexed_array_expr_value_cell_kinds(expr: &Expr, module: &WasmModule) -> Option<Vec<ValueCellKind>> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => value_cell_kinds_for_items(items, module).or_else(|| {
            items
                .iter()
                .all(|item| matches!(item.kind, ExprKind::IntLiteral(_)))
                .then(|| vec![ValueCellKind::Int; items.len()])
        }),
        ExprKind::Variable(name)
            if module.local_kind(name) == Some(LocalKind::Array)
                && module.array_layout(name) == ArrayLayout::Value =>
        {
            module.array_value_cell_kinds(name).map(|kinds| kinds.to_vec())
        }
        ExprKind::Variable(name)
            if module.local_kind(name) == Some(LocalKind::Array)
                && module.array_layout(name) == ArrayLayout::CompactInt =>
        {
            module.array_length(name).map(|len| vec![ValueCellKind::Int; len])
        }
        ExprKind::FunctionCall { name, .. }
            if module.has_function(name)
                && module.function_return_kind(name) == Some(ValueKind::Array)
                && module.function_array_return_layout(name) == ArrayLayout::Value =>
        {
            module.function_array_return_value_kinds(name).map(|kinds| kinds.to_vec())
        }
        ExprKind::FunctionCall { name, .. }
            if module.has_function(name)
                && module.function_return_kind(name) == Some(ValueKind::Array)
                && module.function_array_return_layout(name) == ArrayLayout::CompactInt =>
        {
            module
                .function_array_return_length(name)
                .map(|len| vec![ValueCellKind::Int; len])
        }
        _ => None,
    }
}

pub(super) fn emit_value_cell_result_from_cell(
    cell: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    match kind {
        ValueCellKind::Int => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.load");
            Ok(ValueKind::Int)
        }
        ValueCellKind::Float => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("f64.load");
            Ok(ValueKind::Float)
        }
        ValueCellKind::Bool => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
            Ok(ValueKind::Bool)
        }
        ValueCellKind::Str => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            Ok(ValueKind::Str)
        }
        ValueCellKind::Array => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            Ok(ValueKind::Array)
        }
        ValueCellKind::Null => {
            module.body().line("i32.const 0");
            Ok(ValueKind::Null)
        }
    }
}

pub(super) fn emit_array_index_expr(
    expr: &Expr,
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if let Some(kind) = static_property_array_access_kind(array, index, module) {
        let Some(cell) = materialize_mixed_value_cell(expr, module)? else {
            return Err(array_unsupported(array));
        };
        return emit_value_cell_result_from_cell(&format!("${}", cell), kind, module);
    }
    if nested_array_access_requires_layout(array) {
        return Err(nested_array_access_unsupported(expr));
    }
    if let ExprKind::ConstRef(name) = &array.kind {
        let Some(array_constant) = module.array_constant_value(name) else {
            return Err(array_unsupported(array));
        };
        if let ConstantArrayValue::Assoc(items) = array_constant {
            let Some(key) = assoc_key_value_for_const_index(index, module) else {
                return Err(CompileError::new(
                    index.span,
                    "wasm32-web array constant access requires a static integer or string key",
                ));
            };
            let normalized = normalize_assoc_items(&items).unwrap_or(items);
            let Some((_, item)) = normalized.iter().rev().find(|(candidate, _)| {
                assoc_key_value_for_expr(candidate).is_some_and(|candidate| candidate == key)
            }) else {
                return Ok(ValueKind::Null);
            };
            return emit_array_constant_item_expr(expr, item, module);
        }
        let Some(index_value) = static_or_const_int_value(index) else {
            return Err(CompileError::new(
                index.span,
                "wasm32-web scalar array access requires a static integer index",
            ));
        };
        let Ok(index_value) = usize::try_from(index_value) else {
            return Ok(ValueKind::Null);
        };
        let ConstantArrayValue::Indexed(items) = array_constant else {
            unreachable!("associative array constants were handled above");
        };
        let Some(item) = items.get(index_value) else {
            return Ok(ValueKind::Null);
        };
        return emit_array_constant_item_expr(expr, item, module);
    }
    if let ExprKind::Variable(name) = &array.kind {
        if module.local_kind(name) == Some(LocalKind::Array)
            && module.array_layout(name) == ArrayLayout::Assoc
        {
            if let Some(key_value) = static_or_const_int_value(index) {
                return emit_assoc_array_index_int_expr(name, key_value, module);
            }
            if let Some(key_value) = static_string_value(index, module) {
                return emit_assoc_array_index_string_expr(name, &key_value, module);
            }
        }
    }
    if expression_has_array_type(array, module) {
        let temp = module
            .next_label("direct_array_access")
            .trim_start_matches('$')
            .to_string();
        module.declare_array_local(temp.clone());
        emit_array_assign(&temp, array, module)?;
        if module.array_layout(&temp) == ArrayLayout::Assoc {
            if let Some(key_value) = static_or_const_int_value(index) {
                return emit_assoc_array_index_int_expr(&temp, key_value, module);
            }
            if let Some(key_value) = static_string_value(index, module) {
                return emit_assoc_array_index_string_expr(&temp, &key_value, module);
            }
        }
        let Some(index_value) = static_or_const_int_value(index) else {
            return Err(CompileError::new(
                index.span,
                "wasm32-web scalar array access requires a static integer index",
            ));
        };
        let Ok(index_value) = usize::try_from(index_value) else {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web scalar array access does not support missing indexes yet",
            ));
        };
        if module.array_layout(&temp) == ArrayLayout::Value {
            return emit_value_array_index_expr(expr, &temp, index_value, module);
        }
        let Some(len) = module.array_length(&temp) else {
            return Err(CompileError::new(
                array.span,
                "wasm32-web scalar array access requires a known indexed array length",
            ));
        };
        if index_value >= len {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web scalar array access does not support missing indexes yet",
            ));
        }
        module.body().line(&format!("local.get ${}_ptr", temp));
        module.body().line(&format!("i32.const {}", index_value * 8));
        module.body().line("i32.add");
        module.body().line("i64.load");
        return Ok(ValueKind::Int);
    }
    let Some(index_value) = static_or_const_int_value(index) else {
        return Err(CompileError::new(
            index.span,
            "wasm32-web scalar array access requires a static integer index",
        ));
    };
    let Ok(index_value) = usize::try_from(index_value) else {
        return match &array.kind {
            ExprKind::ArrayLiteral(_) => Ok(ValueKind::Null),
            _ => Err(CompileError::new(
                expr.span,
                "wasm32-web scalar array access does not support missing indexes yet",
            )),
        };
    };
    match &array.kind {
        ExprKind::ArrayLiteral(items) => {
            let Some(item) = items.get(index_value) else {
                return Ok(ValueKind::Null);
            };
            require_int(item, module)?;
            Ok(ValueKind::Int)
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Array) => {
            if module.array_layout(name) == ArrayLayout::Value {
                return emit_value_array_index_expr(expr, name, index_value, module);
            }
            let Some(len) = module.array_length(name) else {
                return Err(CompileError::new(
                    array.span,
                    "wasm32-web scalar array access requires a known indexed array length",
                ));
            };
            if index_value >= len {
                return Err(CompileError::new(
                    expr.span,
                    "wasm32-web scalar array access does not support missing indexes yet",
                ));
            }
            module.body().line(&format!("local.get ${}_ptr", name));
            module.body().line(&format!("i32.const {}", index_value * 8));
            module.body().line("i32.add");
            module.body().line("i64.load");
            Ok(ValueKind::Int)
        }
        _ => Err(array_unsupported(array)),
    }
}

fn emit_array_constant_item_expr(
    expr: &Expr,
    item: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if matches!(
        item.kind,
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)
    ) {
        return emit_array_constant_child_expr(expr, item, module);
    }
    if let Some(value) = static_string_value(item, module) {
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(ValueKind::Str);
    }
    emit_expr(item, module)
}

fn emit_array_constant_child_expr(
    expr: &Expr,
    item: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let temp = module
        .next_label("array_constant_child")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_array_assign(&temp, item, module)?;
    module.body().line(&format!("local.get ${}_ptr", temp));
    module.body().line(&format!("local.get ${}_len", temp));
    match item.kind {
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => Ok(ValueKind::Array),
        _ => Err(array_unsupported(expr)),
    }
}

fn assoc_key_value_for_const_index(index: &Expr, module: &WasmModule) -> Option<AssocKeyValue> {
    if let Some(value) = static_or_const_int_value(index) {
        return Some(AssocKeyValue::Int(value));
    }
    let value = static_string_value(index, module)?;
    literal_php_array_int_key(&value)
        .map(AssocKeyValue::Int)
        .or_else(|| Some(AssocKeyValue::Str(value)))
}

pub(super) use super::array_indexing_assoc::*;

pub(super) fn emit_value_array_index_expr(
    expr: &Expr,
    name: &str,
    index: usize,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let Some(len) = module.array_length(name) else {
        let has_static_nested_metadata = module.array_nested_value_metadata(name, index).is_some();
        if has_static_nested_metadata || module.array_runtime_nested_value_metadata(name).is_some() {
            if index != 0 || !has_static_nested_metadata {
                module.body().line(&format!("i32.const {}", index));
                module.body().line(&format!("local.get ${}_len", name));
                module.body().line("i32.ge_u");
                module.body().open("if");
                module.body().line("unreachable");
                module.body().close("end");
            }
            emit_value_array_static_payload_addr(name, index, module);
            module.body().line("i32.load");
            emit_value_array_static_payload_addr(name, index, module);
            module.body().line("i32.const 4");
            module.body().line("i32.add");
            module.body().line("i32.load");
            return Ok(ValueKind::Array);
        }
        return Err(CompileError::new(
            expr.span,
            "wasm32-web scalar value-array access requires a known indexed array length",
        ));
    };
    if index >= len {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web scalar value-array access does not support missing indexes yet",
        ));
    }
    if module.array_object_class(name, index).is_some() {
        emit_value_array_static_payload_addr(name, index, module);
        module.body().line("i32.load");
        return Ok(ValueKind::Object);
    }
    let Some(kind) = module.array_value_cell_kind(name, index) else {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web scalar value-array access requires a known element type",
        ));
    };
    match kind {
        ValueCellKind::Int => {
            emit_value_array_static_payload_addr(name, index, module);
            module.body().line("i64.load");
            Ok(ValueKind::Int)
        }
        ValueCellKind::Float => {
            emit_value_array_static_payload_addr(name, index, module);
            module.body().line("f64.load");
            Ok(ValueKind::Float)
        }
        ValueCellKind::Bool => {
            emit_value_array_static_payload_addr(name, index, module);
            module.body().line("i64.load");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
            Ok(ValueKind::Bool)
        }
        ValueCellKind::Str => {
            emit_value_array_static_payload_addr(name, index, module);
            module.body().line("i32.load");
            emit_value_array_static_payload_addr(name, index, module);
            module.body().line("i32.const 4");
            module.body().line("i32.add");
            module.body().line("i32.load");
            Ok(ValueKind::Str)
        }
        ValueCellKind::Array => {
            emit_value_array_static_payload_addr(name, index, module);
            module.body().line("i32.load");
            emit_value_array_static_payload_addr(name, index, module);
            module.body().line("i32.const 4");
            module.body().line("i32.add");
            module.body().line("i32.load");
            Ok(ValueKind::Array)
        }
        ValueCellKind::Null => {
            module.body().line("i32.const 0");
            Ok(ValueKind::Null)
        }
    }
}
