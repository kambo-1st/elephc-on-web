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
        if let Some(target) = callable_return_target_from_body(body, &mut HashMap::new()) {
            targets.insert(function_key(name), target);
        }
    }
    targets
}

fn callable_return_target_from_body(
    body: &[Stmt],
    local_callable_targets: &mut HashMap<String, String>,
) -> Option<String> {
    for stmt in body {
        if let Some(target) = callable_return_target_from_stmt(stmt, local_callable_targets) {
            return Some(target);
        }
    }
    None
}

fn callable_return_target_from_stmt(
    stmt: &Stmt,
    local_callable_targets: &mut HashMap<String, String>,
) -> Option<String> {
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => callable_return_target_from_expr(expr, local_callable_targets),
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            if let Some(target) = callable_return_target_from_expr(value, local_callable_targets) {
                local_callable_targets.insert(name.clone(), target);
            } else {
                local_callable_targets.remove(name);
            }
            None
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            let mut then_targets = local_callable_targets.clone();
            let mut target = callable_return_target_from_body(then_body, &mut then_targets)?;
            for (_, body) in elseif_clauses {
                let mut branch_targets = local_callable_targets.clone();
                let branch_target = callable_return_target_from_body(body, &mut branch_targets)?;
                if !target.eq_ignore_ascii_case(&branch_target) {
                    return None;
                }
                target = branch_target;
            }
            let mut else_targets = local_callable_targets.clone();
            let else_target =
                callable_return_target_from_body(else_body.as_deref()?, &mut else_targets)?;
            target.eq_ignore_ascii_case(&else_target).then_some(target)
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            callable_return_target_from_body(stmts, local_callable_targets)
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
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod {
            receiver: StaticReceiver::Named(class_name),
            method,
        }) => Some(format!(
            "__wasm_static_method_{}_{}",
            function_key(class_name.as_str()),
            function_key(method)
        )),
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
