//! Purpose:
//! Lowers wasm32-web string-producing control-flow expressions.
//! Keeps ternary, short-ternary, and match string value helpers out of generic materialization.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::string_materialization`
//! - `crate::codegen::wasm::expr::value_flow`
//!
//! Key details:
//! - Helpers preserve stack order as pointer then length for string values.

use super::*;
use super::string_arg_materialization::materialize_runtime_string_expr;
use super::string_materialization::emit_string_value_to_stack;

pub(in crate::codegen::wasm) fn emit_string_ternary_value_to_stack(
    _expr: &Expr,
    condition: &Expr,
    then_expr: &Expr,
    else_expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_condition(condition, module)?;
    module.body().open("if (result i32 i32)");
    emit_string_value_to_stack(then_expr, module)?;
    module.body().line("else");
    emit_string_value_to_stack(else_expr, module)?;
    module.body().close("end");
    Ok(())
}

pub(in crate::codegen::wasm) fn emit_short_string_ternary_value_to_stack(
    value: &Expr,
    default: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let local = materialize_runtime_string_expr(value, "short_string_ternary", module)?;
    emit_string_local_truthiness(&local, module);
    module.body().open("if (result i32 i32)");
    module.body().line(&format!("local.get ${}_ptr", local));
    module.body().line(&format!("local.get ${}_len", local));
    module.body().line("else");
    emit_string_value_to_stack(default, module)?;
    module.body().close("end");
    Ok(())
}

pub(in crate::codegen::wasm) fn match_subject(expr: &Expr) -> &Expr {
    let ExprKind::Match { subject, .. } = &expr.kind else {
        unreachable!("match_subject called for a non-match expression");
    };
    subject
}

pub(in crate::codegen::wasm) fn match_arms(expr: &Expr) -> &[(Vec<Expr>, Expr)] {
    let ExprKind::Match { arms, .. } = &expr.kind else {
        unreachable!("match_arms called for a non-match expression");
    };
    arms
}

pub(in crate::codegen::wasm) fn match_default(expr: &Expr) -> Option<&Expr> {
    let ExprKind::Match { default, .. } = &expr.kind else {
        unreachable!("match_default called for a non-match expression");
    };
    default.as_deref()
}
