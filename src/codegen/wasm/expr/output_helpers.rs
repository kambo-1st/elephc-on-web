//! Purpose:
//! Owns wasm32-web expression output helpers that are not tied to a PHP builtin.
//! Keeps constant, ternary, and match output lowering out of the main dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::emit_output_expr`
//!
//! Key details:
//! - Helpers emit directly to host output and preserve the dispatcher's stack contract.
//! - Match output currently supports integer subjects, matching the parent lowering.

use super::*;

pub(super) fn emit_output_class_name(
    expr: &Expr,
    receiver: &StaticReceiver,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(class_name) = module.class_name_for_receiver(receiver) else {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web ::class currently requires a named class receiver",
        ));
    };
    let (ptr, len) = module.intern_string(&class_name);
    module.body().line(&format!("i32.const {}", ptr));
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $host_write");
    Ok(())
}

pub(super) fn emit_output_scoped_constant(
    expr: &Expr,
    receiver: &StaticReceiver,
    name: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(value) = module.class_constant_value(receiver, name) else {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web class constant is not supported yet",
        ));
    };
    emit_output_constant_value(value, module);
    Ok(())
}

pub(super) fn emit_output_constant(
    expr: &Expr,
    name: &Name,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(value) = module.constant_value(name) else {
        if let Some(value) = const_int_value(name) {
            module.body().line(&format!("i64.const {}", value));
            module.body().line("call $host_write_int");
            return Ok(());
        }
        return Err(CompileError::new(
            expr.span,
            "wasm32-web constant is not supported yet",
        ));
    };
    emit_output_constant_value(value, module);
    Ok(())
}

pub(super) fn emit_output_constant_value(value: ConstantValue, module: &mut WasmModule) {
    match value {
        ConstantValue::Str(value) => {
            let (ptr, len) = module.intern_string(&value);
            module.body().line(&format!("i32.const {}", ptr));
            module.body().line(&format!("i32.const {}", len));
            module.body().line("call $host_write");
        }
        ConstantValue::Int(value) => {
            module.body().line(&format!("i64.const {}", value));
            module.body().line("call $host_write_int");
        }
        ConstantValue::Float(value) => {
            module.body().line(&format!("f64.const {}", value));
            module.body().line("call $host_write_float");
        }
        ConstantValue::Bool(true) => {
            module.body().line("i64.const 1");
            module.body().line("call $host_write_int");
        }
        ConstantValue::Bool(false) | ConstantValue::Null => {}
    }
}

pub(super) fn emit_output_ternary(
    condition: &Expr,
    then_expr: &Expr,
    else_expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_condition(condition, module)?;
    module.body().open("if");
    emit_output_expr(then_expr, module)?;
    module.body().line("else");
    emit_output_expr(else_expr, module)?;
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_output_short_ternary(
    value: &Expr,
    default: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_condition(value, module)?;
    module.body().open("if");
    emit_output_expr(value, module)?;
    module.body().line("else");
    emit_output_expr(default, module)?;
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_output_match(
    expr: &Expr,
    subject: &Expr,
    arms: &[(Vec<Expr>, Expr)],
    default: Option<&Expr>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if arms.is_empty() && default.is_none() {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web match expression requires at least one arm or default",
        ));
    }
    let subject_local = module.next_label("match_subject").trim_start_matches('$').to_string();
    module.declare_i64_local(subject_local.clone());
    require_int(subject, module)?;
    module.body().line(&format!("local.set ${}", subject_local));
    emit_output_match_chain(&subject_local, arms, default, module)
}

fn emit_output_match_chain(
    subject_local: &str,
    arms: &[(Vec<Expr>, Expr)],
    default: Option<&Expr>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some((conditions, value)) = arms.first() {
        emit_match_condition(subject_local, conditions, module)?;
        module.body().open("if");
        emit_output_expr(value, module)?;
        module.body().line("else");
        emit_output_match_chain(subject_local, &arms[1..], default, module)?;
        module.body().close("end");
    } else if let Some(default) = default {
        emit_output_expr(default, module)?;
    } else {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            "wasm32-web match without default can fail at runtime and is not supported yet",
        ));
    }
    Ok(())
}
