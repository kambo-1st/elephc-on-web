//! Purpose:
//! Collects statically known callable targets returned by wasm user functions.
//! Keeps monomorphic callable-return support separate from runtime descriptors.
//!
//! Called from:
//! - `crate::codegen::wasm::module::function_metadata`
//!
//! Key details:
//! - Only callable-returning functions whose return paths resolve to one direct target are tracked.
//! - Dynamic, closure, instance-captured, or conflicting returns stay unknown and are rejected by codegen.

use super::*;

pub(in crate::codegen::wasm::module) fn collect_function_callable_return_targets(
    program: &Program,
) -> HashMap<String, String> {
    let mut targets = HashMap::new();
    for stmt in program {
        let StmtKind::FunctionDecl {
            name,
            return_type,
            body,
            ..
        } = &stmt.kind else {
            continue;
        };
        if value_kind_from_return_type(return_type.as_ref()) != ValueKind::Callable {
            continue;
        }
        if let Some(target) = consistent_callable_return_target(body, &HashMap::new()) {
            targets.insert(function_key(name), target);
        }
    }
    targets
}

fn consistent_callable_return_target(
    body: &[Stmt],
    local_callable_targets: &HashMap<String, String>,
) -> Option<String> {
    let mut target = None;
    for stmt in body {
        let stmt_target = callable_return_target_from_stmt(stmt, local_callable_targets)?;
        if target
            .as_ref()
            .is_some_and(|existing: &String| !existing.eq_ignore_ascii_case(&stmt_target))
        {
            return None;
        }
        target = Some(stmt_target);
    }
    target
}

fn callable_return_target_from_stmt(
    stmt: &Stmt,
    local_callable_targets: &HashMap<String, String>,
) -> Option<String> {
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => callable_return_target_from_expr(expr, local_callable_targets),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            let mut target = consistent_callable_return_target(then_body, local_callable_targets)?;
            for (_, body) in elseif_clauses {
                let branch_target = consistent_callable_return_target(body, local_callable_targets)?;
                if !target.eq_ignore_ascii_case(&branch_target) {
                    return None;
                }
                target = branch_target;
            }
            let else_target = consistent_callable_return_target(else_body.as_deref()?, local_callable_targets)?;
            target.eq_ignore_ascii_case(&else_target).then_some(target)
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            consistent_callable_return_target(stmts, local_callable_targets)
        }
        _ => None,
    }
}

fn callable_return_target_from_expr(
    expr: &Expr,
    local_callable_targets: &HashMap<String, String>,
) -> Option<String> {
    match &expr.kind {
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => Some(name.to_string()),
        ExprKind::Variable(name) => local_callable_targets.get(name).cloned(),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_target = callable_return_target_from_expr(then_expr, local_callable_targets)?;
            let else_target = callable_return_target_from_expr(else_expr, local_callable_targets)?;
            then_target
                .eq_ignore_ascii_case(&else_target)
                .then_some(then_target)
        }
        _ => None,
    }
}
