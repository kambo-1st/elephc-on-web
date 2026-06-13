//! Purpose:
//! Lowers wasm32-web statement control flow such as branches, loops, switch,
//! returns, and break/continue jumps.
//!
//! Called from:
//! - `crate::codegen::wasm::stmt`
//!
//! Key details:
//! - Uses the parent statement dispatcher for nested statement bodies.
//! - Emits explicit CompileError diagnostics for unsupported wasm return shapes.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, Stmt};

use super::emit_stmt;
use super::super::expr::{
    dynamic_numeric_operand_may_materialize, emit_condition, emit_known_mixed_numeric_operand,
    emit_expr, emit_mixed_value_to_stack, emit_return_array_value_to_stack, emit_string_value_to_stack,
    evaluated_static_callback_function_name, require_float, require_int,
};
use super::super::module::{ValueKind, WasmModule};

pub(super) fn emit_switch(
    subject: &Expr,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let subject_local = module.next_label("switch_subject").trim_start_matches('$').to_string();
    let matched_local = module.next_label("switch_matched").trim_start_matches('$').to_string();
    module.declare_i64_local(subject_local.clone());
    module.declare_i32_local(matched_local.clone());
    require_int(subject, module)?;
    module.body().line(&format!("local.set ${}", subject_local));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}", matched_local));

    let break_label = module.next_label("switch_break");
    module.body().open(&format!("block {}", break_label));
    module.push_switch(break_label.clone());

    for (conditions, body) in cases {
        let case_end_label = module.next_label("switch_next");
        module.body().open(&format!("block {}", case_end_label));
        emit_case_condition(
            &subject_local,
            &matched_local,
            conditions,
            &case_end_label,
            module,
        )?;
        for stmt in body {
            emit_stmt(stmt, module)?;
        }
        module.body().close("end");
    }

    if let Some(default) = default {
        for stmt in default {
            emit_stmt(stmt, module)?;
        }
    }

    module.pop_switch();
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_return(
    value: Option<&Expr>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(value) = value {
        match module.current_return_kind() {
            ValueKind::Int => emit_numeric_return_value(value, false, module)?,
            ValueKind::Float => emit_numeric_return_value(value, true, module)?,
            ValueKind::Bool => emit_condition(value, module)?,
            ValueKind::Str => emit_string_value_to_stack(value, module)?,
            ValueKind::Array => emit_return_array_value_to_stack(value, module)?,
            ValueKind::Object => {
                let kind = emit_expr(value, module)?;
                if !matches!(kind, ValueKind::Object | ValueKind::Null) {
                    return Err(CompileError::new(
                        value.span,
                        "wasm32-web object return expected an object value",
                    ));
                }
            }
            ValueKind::Callable => {
                let Some(_) = evaluated_static_callback_function_name(value, module)? else {
                    return Err(CompileError::new(
                        value.span,
                        "wasm32-web callable returns require a statically known callable target",
                    ));
                };
                module.body().line("i32.const 0");
            }
            ValueKind::Mixed => emit_mixed_value_to_stack(value, module)?,
            ValueKind::Never => {
                return Err(CompileError::new(
                    value.span,
                    "wasm32-web never-returning functions must not return a value",
                ));
            }
            ValueKind::Null => {
                return Err(CompileError::new(
                    value.span,
                    "wasm32-web user functions do not support this return type yet",
                ));
            }
        }
    } else {
        match module.current_return_kind() {
            ValueKind::Int => module.body().line("i64.const 0"),
            ValueKind::Float => module.body().line("f64.const 0"),
            ValueKind::Bool => module.body().line("i32.const 0"),
            ValueKind::Mixed => {
                return Err(CompileError::new(
                    crate::span::Span::dummy(),
                    "wasm32-web mixed-returning functions must return an explicit value",
                ));
            }
            ValueKind::Str => {
                module.body().line("i32.const 0");
                module.body().line("i32.const 0");
            }
            ValueKind::Array => {
                module.body().line("i32.const 0");
                module.body().line("i32.const 0");
            }
            ValueKind::Object => {
                return Err(CompileError::new(
                    crate::span::Span::dummy(),
                    "wasm32-web object returns are not supported yet",
                ));
            }
            ValueKind::Callable => module.body().line("i32.const 0"),
            ValueKind::Null => {}
            ValueKind::Never => module.body().line("unreachable"),
        }
    }
    module.body().line("return");
    Ok(())
}

fn emit_numeric_return_value(
    value: &Expr,
    use_float: bool,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if dynamic_numeric_operand_may_materialize(value, module)? {
        emit_known_mixed_numeric_operand(value, use_float, module)?;
        return Ok(());
    }
    if use_float {
        require_float(value, module)
    } else {
        require_int(value, module)
    }
}

pub(super) fn emit_if(
    condition: &Expr,
    then_body: &[Stmt],
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: Option<&[Stmt]>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_condition(condition, module)?;
    module.body().open("if");
    for stmt in then_body {
        emit_stmt(stmt, module)?;
    }
    emit_else_chain(elseif_clauses, else_body, module)?;
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_while(
    condition: &Expr,
    body: &[Stmt],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let break_label = module.next_label("break");
    let continue_label = module.next_label("continue");
    module.body().open(&format!("block {}", break_label));
    module.body().open(&format!("loop {}", continue_label));
    module.push_loop(break_label.clone(), continue_label.clone());
    emit_condition(condition, module)?;
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", break_label));
    for stmt in body {
        emit_stmt(stmt, module)?;
    }
    module.body().line(&format!("br {}", continue_label));
    module.pop_loop();
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_do_while(
    body: &[Stmt],
    condition: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let break_label = module.next_label("break");
    let loop_label = module.next_label("loop");
    let continue_label = module.next_label("continue");
    module.body().open(&format!("block {}", break_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().open(&format!("block {}", continue_label));
    module.push_loop(break_label.clone(), continue_label.clone());
    for stmt in body {
        emit_stmt(stmt, module)?;
    }
    module.pop_loop();
    module.body().close("end");
    emit_condition(condition, module)?;
    module.body().line(&format!("br_if {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_for(
    init: Option<&Stmt>,
    condition: Option<&Expr>,
    update: Option<&Stmt>,
    body: &[Stmt],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(init) = init {
        emit_stmt(init, module)?;
    }
    let break_label = module.next_label("break");
    let loop_label = module.next_label("loop");
    let continue_label = module.next_label("continue");
    module.body().open(&format!("block {}", break_label));
    module.body().open(&format!("loop {}", loop_label));
    if let Some(condition) = condition {
        emit_condition(condition, module)?;
        module.body().line("i32.eqz");
        module.body().line(&format!("br_if {}", break_label));
    }
    module.body().open(&format!("block {}", continue_label));
    module.push_loop(break_label.clone(), continue_label.clone());
    for stmt in body {
        emit_stmt(stmt, module)?;
    }
    module.pop_loop();
    module.body().close("end");
    if let Some(update) = update {
        emit_stmt(update, module)?;
    }
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_break(
    levels: usize,
    module: &mut WasmModule,
    span: crate::span::Span,
) -> Result<(), CompileError> {
    let label = module.break_label(levels).map(str::to_string).ok_or_else(|| {
        CompileError::new(span, "wasm32-web break target is outside the active break stack")
    })?;
    module.body().line(&format!("br {}", label));
    Ok(())
}

pub(super) fn emit_continue(
    levels: usize,
    module: &mut WasmModule,
    span: crate::span::Span,
) -> Result<(), CompileError> {
    let label = module.continue_label(levels).map(str::to_string).ok_or_else(|| {
        CompileError::new(span, "wasm32-web continue target is outside the active loop stack")
    })?;
    module.body().line(&format!("br {}", label));
    Ok(())
}

fn emit_case_condition(
    subject_local: &str,
    matched_local: &str,
    conditions: &[Expr],
    case_end_label: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if conditions.is_empty() {
        return Ok(());
    }
    module.body().line(&format!("local.get ${}", matched_local));
    module.body().line("i32.eqz");
    module.body().open("if");
    for condition in conditions {
        module.body().line(&format!("local.get ${}", subject_local));
        require_int(condition, module)?;
        module.body().line("i64.eq");
    }
    for _ in 1..conditions.len() {
        module.body().line("i32.or");
    }
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", case_end_label));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set ${}", matched_local));
    module.body().close("end");
    Ok(())
}

fn emit_else_chain(
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: Option<&[Stmt]>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some((condition, body)) = elseif_clauses.first() {
        module.body().line("else");
        emit_condition(condition, module)?;
        module.body().open("if");
        for stmt in body {
            emit_stmt(stmt, module)?;
        }
        emit_else_chain(&elseif_clauses[1..], else_body, module)?;
        module.body().close("end");
    } else if let Some(body) = else_body {
        module.body().line("else");
        for stmt in body {
            emit_stmt(stmt, module)?;
        }
    }
    Ok(())
}
