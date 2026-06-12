//! Purpose:
//! Validates simple array-variable argument shapes for wasm32-web array helpers.
//! Keeps builtin argument shape checks out of the main expression dispatcher.
//!
//! Called from:
//! - array mutator lowering under `crate::codegen::wasm::expr`
//! - mixed pop/shift assignment lowering in `crate::codegen::wasm::expr`
//!
//! Key details:
//! - These helpers only validate assigned array variables; builtin-specific
//!   semantic checks remain with the mutator/search emitters.

use super::*;

pub(crate) fn single_array_variable_arg(
    expr: &Expr,
    args: &[Expr],
    function_name: &str,
    module: &WasmModule,
) -> Result<String, CompileError> {
    if args.len() != 1 {
        return Err(CompileError::new(
            expr.span,
            &format!("wasm32-web {function_name}() expects exactly one argument"),
        ));
    }
    let ExprKind::Variable(name) = &args[0].kind else {
        return Err(CompileError::new(
            args[0].span,
            &format!("wasm32-web {function_name}() requires an assigned indexed array variable"),
        ));
    };
    if module.local_kind(name) != Some(LocalKind::Array) {
        return Err(CompileError::new(
            args[0].span,
            &format!("wasm32-web {function_name}() first argument must be an indexed array"),
        ));
    }
    Ok(name.clone())
}
