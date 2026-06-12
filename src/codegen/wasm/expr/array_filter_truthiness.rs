//! Purpose:
//! Emits wasm32-web truthiness helpers used by array_filter lowering.
//! Keeps PHP truthiness and strlen-truthiness mechanics separate from filter loops.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_map_filter` and filter helper modules.
//!
//! Key details:
//! - Implements PHP string "0" falsehood, scalar value-cell truthiness, and default assoc metadata filtering.

use super::*;
pub(super) fn emit_value_cell_pointer_truthiness(cell: &str, kind: ValueCellKind, module: &mut WasmModule) {
    match kind {
        ValueCellKind::Int | ValueCellKind::Bool => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
        }
        ValueCellKind::Str => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line("i32.const 1");
            module.body().line("i32.ne");
            module.body().open("if (result i32)");
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line("i32.const 0");
            module.body().line("i32.ne");
            module.body().line("else");
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line("i32.load8_u");
            module.body().line("i32.const 48");
            module.body().line("i32.ne");
            module.body().close("end");
        }
        ValueCellKind::Array => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line("i32.eqz");
            module.body().line("i32.eqz");
        }
        ValueCellKind::Float => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("f64.load");
            module.body().line("f64.const 0");
            module.body().line("f64.ne");
        }
        ValueCellKind::Null => {
            module.body().line("i32.const 0");
        }
    }
}

pub(super) fn emit_value_cell_pointer_strlen_truthiness(cell: &str, kind: ValueCellKind, module: &mut WasmModule) {
    match kind {
        ValueCellKind::Int | ValueCellKind::Float => {
            module.body().line("i32.const 1");
        }
        ValueCellKind::Str => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 12");
            module.body().line("i32.add");
            module.body().line("i32.load");
            module.body().line("i32.const 0");
            module.body().line("i32.ne");
        }
        ValueCellKind::Bool => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
        }
        ValueCellKind::Null => {
            module.body().line("i32.const 0");
        }
        _ => unreachable!("array_filter strlen truthiness only handles scalar cells"),
    }
}

pub(super) fn emit_value_cell_pointer_strlen_length(cell: &str, kind: ValueCellKind, module: &mut WasmModule) {
    match kind {
        ValueCellKind::Int => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.load");
            emit_i64_stack_strlen_length("array_map_assoc_int_strlen", module);
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
            emit_array_map_strlen_result(module);
        }
        ValueCellKind::Bool => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
            module.body().line("i64.extend_i32_u");
        }
        ValueCellKind::Float => {
            module.body().line(&format!("local.get {}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("f64.load");
            emit_f64_stack_strlen_length("array_map_assoc_float_strlen", module);
        }
        ValueCellKind::Null => {
            module.body().line("i64.const 0");
        }
        _ => unreachable!("array_map assoc strlen length only handles scalar cells"),
    }
}

pub(super) fn array_filter_default_assoc_literal_metadata(
    items: &[(Expr, Expr)],
    module: &WasmModule,
) -> Option<(Vec<AssocKeyValue>, Vec<ValueCellKind>)> {
    let normalized = normalize_assoc_items(items)?;
    let mut keys = Vec::new();
    let mut values = Vec::new();
    for (key, value) in normalized {
        if static_value_cell_truthiness_for_filter(&value)? {
            keys.push(assoc_key_value_for_expr(&key)?);
            values.push(value_cell_kind_for_expr(&value, module)?);
        }
    }
    Some((keys, values))
}

pub(super) fn static_value_cell_truthiness_for_filter(value: &Expr) -> Option<bool> {
    match &value.kind {
        ExprKind::Null => Some(false),
        ExprKind::BoolLiteral(value) => Some(*value),
        ExprKind::StringLiteral(value) => Some(!value.is_empty() && value != "0"),
        ExprKind::IntLiteral(value) => Some(*value != 0),
        ExprKind::Negate(inner) if matches!(inner.kind, ExprKind::IntLiteral(_)) => {
            static_value_cell_truthiness_for_filter(inner)
        }
        ExprKind::FloatLiteral(value) => Some(*value != 0.0),
        ExprKind::Negate(inner) if matches!(inner.kind, ExprKind::FloatLiteral(_)) => {
            static_value_cell_truthiness_for_filter(inner)
        }
        ExprKind::ArrayLiteral(items) => Some(!items.is_empty()),
        ExprKind::ArrayLiteralAssoc(items) => {
            normalize_assoc_items(items).map(|items| !items.is_empty())
        }
        _ => None,
    }
}

pub(super) fn emit_string_parts_truthiness(ptr: &str, len: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 1");
    module.body().line("i32.ne");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 0");
    module.body().line("i32.ne");
    module.body().line("else");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line("i32.load8_u");
    module.body().line("i32.const 48");
    module.body().line("i32.ne");
    module.body().close("end");
}

pub(super) fn emit_static_scalar_truthiness(
    item: &Expr,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match kind {
        ValueCellKind::Bool => match emit_expr(item, module)? {
            ValueKind::Bool => Ok(()),
            _ => Err(CompileError::new(
                item.span,
                "wasm32-web array_filter() expected a boolean value cell",
            )),
        },
        ValueCellKind::Float => {
            require_float(item, module)?;
            module.body().line("f64.const 0");
            module.body().line("f64.ne");
            Ok(())
        }
        ValueCellKind::Null => {
            module.body().line("i32.const 0");
            Ok(())
        }
        ValueCellKind::Array => {
            let len = match &item.kind {
                ExprKind::ArrayLiteral(items) => items.len(),
                ExprKind::ArrayLiteralAssoc(items) => items.len(),
                _ => {
                    return Err(CompileError::new(
                        item.span,
                        "wasm32-web array_filter() nested-array truthiness requires literal array cells",
                    ));
                }
            };
            module.body().line(&format!("i32.const {}", usize::from(len != 0)));
            Ok(())
        }
        _ => Err(CompileError::new(
            item.span,
            "wasm32-web array_filter() default truthiness requires bool, float, null, or array cells",
        )),
    }
}

pub(super) fn emit_static_scalar_strlen_truthiness(
    item: &Expr,
    kind: ValueCellKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match kind {
        ValueCellKind::Bool => match emit_expr(item, module)? {
            ValueKind::Bool => Ok(()),
            _ => Err(CompileError::new(
                item.span,
                "wasm32-web array_filter(strlen(...)) expected a boolean value cell",
            )),
        },
        ValueCellKind::Float => {
            require_float(item, module)?;
            module.body().line("drop");
            module.body().line("i32.const 1");
            Ok(())
        }
        ValueCellKind::Null => {
            module.body().line("i32.const 0");
            Ok(())
        }
        _ => Err(CompileError::new(
            item.span,
            "wasm32-web array_filter(strlen(...)) currently supports scalar string-coercible value cells",
        )),
    }
}

pub(super) fn emit_value_cell_truthiness(
    source: &str,
    index: usize,
    kind: ValueCellKind,
    module: &mut WasmModule,
) {
    match kind {
        ValueCellKind::Bool => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("call $__rt_value_payload_i64");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
        }
        ValueCellKind::Float => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE + 8));
            module.body().line("i32.add");
            module.body().line("f64.load");
            module.body().line("f64.const 0");
            module.body().line("f64.ne");
        }
        ValueCellKind::Null => {
            module.body().line("i32.const 0");
        }
        ValueCellKind::Array => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("i32.const 12");
            module.body().line("call $__rt_value_payload_i32");
            module.body().line("i32.eqz");
            module.body().line("i32.eqz");
        }
        _ => unreachable!("array_filter local truthiness only handles bool/float/null/array cells"),
    }
}

pub(super) fn emit_value_cell_truthiness_dynamic(
    source: &str,
    index: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) {
    match kind {
        ValueCellKind::Bool => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get {}", index));
            module.body().line("call $__rt_value_payload_i64");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
        }
        ValueCellKind::Float => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get {}", index));
            module.body().line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
            module.body().line("i32.mul");
            module.body().line("i32.add");
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("f64.load");
            module.body().line("f64.const 0");
            module.body().line("f64.ne");
        }
        ValueCellKind::Null => {
            module.body().line("i32.const 0");
        }
        ValueCellKind::Array => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get {}", index));
            module.body().line("i32.const 12");
            module.body().line("call $__rt_value_payload_i32");
            module.body().line("i32.eqz");
            module.body().line("i32.eqz");
        }
        _ => unreachable!("array_filter dynamic truthiness only handles bool/float/null/array cells"),
    }
}

pub(super) fn emit_value_cell_strlen_truthiness(
    source: &str,
    index: usize,
    kind: ValueCellKind,
    module: &mut WasmModule,
) {
    match kind {
        ValueCellKind::Bool => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("i32.const {}", index));
            module.body().line("call $__rt_value_payload_i64");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
        }
        ValueCellKind::Float => {
            module.body().line("i32.const 1");
        }
        ValueCellKind::Null => {
            module.body().line("i32.const 0");
        }
        _ => unreachable!("array_filter strlen truthiness only handles bool/float/null cells"),
    }
}

pub(super) fn emit_value_cell_strlen_truthiness_dynamic(
    source: &str,
    index: &str,
    kind: ValueCellKind,
    module: &mut WasmModule,
) {
    match kind {
        ValueCellKind::Bool => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get {}", index));
            module.body().line("call $__rt_value_payload_i64");
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
        }
        ValueCellKind::Float => {
            module.body().line("i32.const 1");
        }
        ValueCellKind::Null => {
            module.body().line("i32.const 0");
        }
        _ => unreachable!("array_filter dynamic strlen truthiness only handles bool/float/null cells"),
    }
}
