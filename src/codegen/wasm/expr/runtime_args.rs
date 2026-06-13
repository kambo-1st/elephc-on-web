//! Purpose:
//! Classifies runtime scalar arguments for wasm32-web string and formatting builtins.
//! Keeps shared bool and padding option normalization out of the expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr`
//! - focused wasm expression helper modules such as `sprintf`, `number_format`, and `str_pad`
//!
//! Key details:
//! - Only recognizes already-materialized wasm locals with the expected runtime kind.
//! - PHP-visible validation errors stay at the caller site that owns the builtin semantics.

use super::*;

pub(super) fn runtime_string_variable_arg<'a>(expr: &'a Expr, module: &WasmModule) -> Option<&'a str> {
    match &expr.kind {
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Str) => {
            Some(name.as_str())
        }
        _ => None,
    }
}


pub(super) fn runtime_int_variable_arg<'a>(expr: &'a Expr, module: &WasmModule) -> Option<&'a str> {
    match &expr.kind {
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::I64) => {
            Some(name.as_str())
        }
        _ => None,
    }
}

pub(super) fn runtime_float_variable_arg<'a>(expr: &'a Expr, module: &WasmModule) -> Option<&'a str> {
    match &expr.kind {
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::F64) => {
            Some(name.as_str())
        }
        _ => None,
    }
}

pub(super) fn runtime_bool_variable_arg<'a>(expr: &'a Expr, module: &WasmModule) -> Option<&'a str> {
    match &expr.kind {
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::I32) => {
            Some(name.as_str())
        }
        _ => None,
    }
}

#[derive(Clone)]
pub(super) enum RuntimePadType<'a> {
    Static(i64),
    Variable(Cow<'a, str>),
}

#[derive(Clone, Copy)]
pub(super) enum RuntimeBoolArg<'a> {
    Static(bool),
    Variable(&'a str),
}

pub(super) fn runtime_bool_arg<'a>(
    expr: &'a Expr,
    module: &WasmModule,
) -> Result<RuntimeBoolArg<'a>, CompileError> {
    if let Some(var) = runtime_bool_variable_arg(expr, module) {
        return Ok(RuntimeBoolArg::Variable(var));
    }
    literal_bool_arg(expr).map(RuntimeBoolArg::Static)
}

pub(super) fn runtime_str_pad_type<'a>(
    _call: &Expr,
    arg: Option<&'a Expr>,
    module: &mut WasmModule,
) -> Result<RuntimePadType<'a>, CompileError> {
    let Some(arg) = arg else {
        return Ok(RuntimePadType::Static(1));
    };
    if let Some(pad_type) = static_or_const_int_value(arg) {
        return Ok(RuntimePadType::Static(pad_type));
    }
    if let Some(var) = runtime_int_variable_arg(arg, module) {
        return Ok(RuntimePadType::Variable(Cow::Borrowed(var)));
    }
    let local = module.next_label("str_pad_type").trim_start_matches('$').to_string();
    module.declare_i64_local(local.clone());
    require_int(arg, module)?;
    module.body().line(&format!("local.set ${}", local));
    Ok(RuntimePadType::Variable(Cow::Owned(local)))
}
