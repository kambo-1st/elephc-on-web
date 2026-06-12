//! Purpose:
//! Lowers mixed/value-cell scalar comparisons for wasm32-web expressions.
//! Keeps dynamic value-cell comparison dispatch separate from the main scalar operator file.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::scalar_ops` through the wasm expression module.
//!
//! Key details:
//! - Handles PHP loose numeric/bool comparison choices for mixed cells and dynamic array reads.
//! - Unsupported string/array comparison combinations return CompileError instead of guessing semantics.

use super::*;

pub(in crate::codegen::wasm) fn emit_known_mixed_numeric_comparison(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if emit_dynamic_scalar_value_cell_comparison(left, op, right, true, module)? {
        return Ok(true);
    }
    if emit_dynamic_scalar_value_cell_comparison(right, op, left, false, module)? {
        return Ok(true);
    }
    let left_kind = known_mixed_comparison_kind(left, module)?;
    let right_kind = known_mixed_comparison_kind(right, module)?;
    if left_kind.is_none() && right_kind.is_none() {
        return Ok(false);
    }
    if left_kind == Some(ValueKind::Bool)
        || right_kind == Some(ValueKind::Bool)
        || expression_is_booly(left, module)
        || expression_is_booly(right, module)
    {
        emit_condition(left, module)?;
        emit_condition(right, module)?;
        let instr = match op {
            BinOp::Lt => "i32.lt_s",
            BinOp::Gt => "i32.gt_s",
            BinOp::LtEq => "i32.le_s",
            BinOp::GtEq => "i32.ge_s",
            BinOp::Eq => "i32.eq",
            BinOp::NotEq => "i32.ne",
            _ => unreachable!(),
        };
        module.body().line(instr);
        return Ok(true);
    }
    let use_float = left_kind == Some(ValueKind::Float)
        || right_kind == Some(ValueKind::Float)
        || expression_is_floaty(left, module)
        || expression_is_floaty(right, module);
    emit_known_mixed_numeric_operand(left, use_float, module)?;
    emit_known_mixed_numeric_operand(right, use_float, module)?;
    let instr = match (op, use_float) {
        (BinOp::Lt, true) => "f64.lt",
        (BinOp::Gt, true) => "f64.gt",
        (BinOp::LtEq, true) => "f64.le",
        (BinOp::GtEq, true) => "f64.ge",
        (BinOp::Eq, true) => "f64.eq",
        (BinOp::NotEq, true) => "f64.ne",
        (BinOp::Lt, false) => "i64.lt_s",
        (BinOp::Gt, false) => "i64.gt_s",
        (BinOp::LtEq, false) => "i64.le_s",
        (BinOp::GtEq, false) => "i64.ge_s",
        (BinOp::Eq, false) => "i64.eq",
        (BinOp::NotEq, false) => "i64.ne",
        _ => unreachable!(),
    };
    module.body().line(instr);
    Ok(true)
}

pub(in crate::codegen::wasm) fn emit_dynamic_scalar_value_cell_comparison(
    dynamic: &Expr,
    op: &BinOp,
    other: &Expr,
    dynamic_is_left: bool,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let Some(kinds) = dynamic_scalar_value_cell_comparison_kinds(dynamic, module) else {
        return Ok(false);
    };
    if !dynamic_scalar_comparison_other_is_supported(other, module) {
        return Ok(false);
    }
    let Some(cell) = materialize_mixed_value_cell(dynamic, module)? else {
        return Ok(false);
    };
    let tag = module.next_label("dynamic_scalar_compare_tag");
    module.declare_i32_local(tag.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("call $__rt_mixed_tag");
    module.body().line(&format!("local.set {}", tag));
    let other_is_bool = expression_is_booly(other, module)
        || known_mixed_value_cell_kind(other, module)? == Some(ValueCellKind::Bool);
    if other_is_bool {
        emit_dynamic_scalar_bool_compare(&cell, op, other, dynamic_is_left, module)?;
        return Ok(true);
    }
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().open("if (result i32)");
    emit_dynamic_scalar_bool_compare_operands(&cell, other, dynamic_is_left, module)?;
    module.body().line(i32_compare_instruction(op));
    module.body().line("else");
    let use_float = kinds
        .iter()
        .any(|kind| matches!(kind, ValueCellKind::Float | ValueCellKind::Str))
        || expression_is_floaty(other, module)
        || known_mixed_value_cell_kind(other, module)? == Some(ValueCellKind::Float);
    emit_dynamic_scalar_numeric_compare_operands(
        &cell,
        other,
        dynamic_is_left,
        use_float,
        module,
    )?;
    module.body().line(numeric_compare_instruction(op, use_float));
    module.body().close("end");
    Ok(true)
}

pub(in crate::codegen::wasm) fn dynamic_scalar_value_cell_comparison_kinds(
    expr: &Expr,
    module: &WasmModule,
) -> Option<Vec<ValueCellKind>> {
    let ExprKind::ArrayAccess { array, index } = &expr.kind else {
        return None;
    };
    let ExprKind::Variable(name) = &array.kind else {
        return None;
    };
    if module.local_kind(name) != Some(LocalKind::Array) {
        return None;
    }
    let dynamic_index = match module.array_layout(name) {
        ArrayLayout::Value => static_or_const_or_i64_local_value(index, module).is_none(),
        ArrayLayout::Assoc => assoc_static_access_kind(array, index, module).is_none(),
        ArrayLayout::CompactInt => false,
    };
    if !dynamic_index {
        return None;
    }
    let kinds = module.array_value_cell_kinds(name)?;
    kinds
        .iter()
        .all(|kind| {
            matches!(
                kind,
                ValueCellKind::Int
                    | ValueCellKind::Float
                    | ValueCellKind::Bool
                    | ValueCellKind::Null
                    | ValueCellKind::Str
            )
        })
        .then(|| kinds.to_vec())
}

pub(in crate::codegen::wasm) fn dynamic_scalar_comparison_other_is_supported(expr: &Expr, module: &WasmModule) -> bool {
    matches!(
        expr.kind,
        ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) | ExprKind::BoolLiteral(_) | ExprKind::Null
    ) || expression_is_inty(expr, module)
        || expression_is_floaty(expr, module)
        || expression_is_booly(expr, module)
        || matches!(
            known_mixed_value_cell_kind(expr, module),
            Ok(Some(
                ValueCellKind::Int | ValueCellKind::Float | ValueCellKind::Bool | ValueCellKind::Null
            ))
        )
}

pub(in crate::codegen::wasm) fn emit_dynamic_scalar_bool_compare(
    cell: &str,
    op: &BinOp,
    other: &Expr,
    dynamic_is_left: bool,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_dynamic_scalar_bool_compare_operands(cell, other, dynamic_is_left, module)?;
    module.body().line(i32_compare_instruction(op));
    Ok(())
}

pub(in crate::codegen::wasm) fn emit_dynamic_scalar_bool_compare_operands(
    cell: &str,
    other: &Expr,
    dynamic_is_left: bool,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if dynamic_is_left {
        emit_mixed_local_truthiness(cell, module);
        emit_condition(other, module)?;
    } else {
        emit_condition(other, module)?;
        emit_mixed_local_truthiness(cell, module);
    }
    Ok(())
}

pub(in crate::codegen::wasm) fn emit_dynamic_scalar_numeric_compare_operands(
    cell: &str,
    other: &Expr,
    dynamic_is_left: bool,
    use_float: bool,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if dynamic_is_left {
        emit_dynamic_value_cell_numeric_operand(cell, use_float, module);
        emit_known_mixed_numeric_operand(other, use_float, module)?;
    } else {
        emit_known_mixed_numeric_operand(other, use_float, module)?;
        emit_dynamic_value_cell_numeric_operand(cell, use_float, module);
    }
    Ok(())
}

pub(in crate::codegen::wasm) fn i32_compare_instruction(op: &BinOp) -> &'static str {
    match op {
        BinOp::Lt => "i32.lt_s",
        BinOp::Gt => "i32.gt_s",
        BinOp::LtEq => "i32.le_s",
        BinOp::GtEq => "i32.ge_s",
        BinOp::Eq => "i32.eq",
        BinOp::NotEq => "i32.ne",
        _ => unreachable!("loose comparison helper only handles comparison operators"),
    }
}

pub(in crate::codegen::wasm) fn numeric_compare_instruction(op: &BinOp, use_float: bool) -> &'static str {
    match (op, use_float) {
        (BinOp::Lt, true) => "f64.lt",
        (BinOp::Gt, true) => "f64.gt",
        (BinOp::LtEq, true) => "f64.le",
        (BinOp::GtEq, true) => "f64.ge",
        (BinOp::Eq, true) => "f64.eq",
        (BinOp::NotEq, true) => "f64.ne",
        (BinOp::Lt, false) => "i64.lt_s",
        (BinOp::Gt, false) => "i64.gt_s",
        (BinOp::LtEq, false) => "i64.le_s",
        (BinOp::GtEq, false) => "i64.ge_s",
        (BinOp::Eq, false) => "i64.eq",
        (BinOp::NotEq, false) => "i64.ne",
        _ => unreachable!("loose comparison helper only handles comparison operators"),
    }
}

pub(in crate::codegen::wasm) fn known_mixed_comparison_kind(
    expr: &Expr,
    module: &WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    if let Some(kind) = dynamic_numeric_value_cell_comparison_kind(expr, module) {
        return Ok(Some(kind));
    }
    match known_mixed_value_cell_kind(expr, module)? {
        Some(ValueCellKind::Float) => Ok(Some(ValueKind::Float)),
        Some(ValueCellKind::Int) => Ok(Some(ValueKind::Int)),
        Some(ValueCellKind::Bool) => Ok(Some(ValueKind::Bool)),
        Some(ValueCellKind::Null) => Ok(Some(ValueKind::Null)),
        Some(ValueCellKind::Str) | Some(ValueCellKind::Array) => Err(CompileError::new(
            expr.span,
            "wasm32-web mixed numeric comparison does not support string or array values yet",
        )),
        None => Ok(None),
    }
}

pub(in crate::codegen::wasm) fn dynamic_numeric_value_cell_comparison_kind(expr: &Expr, module: &WasmModule) -> Option<ValueKind> {
    dynamic_numeric_value_cell_kind_with(expr, module, false)
}
