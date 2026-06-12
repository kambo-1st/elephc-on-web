//! Purpose:
//! Collects wasm function return metadata when an array return forwards an array parameter.
//! Keeps parameter-forwarding detection out of the broader function metadata collector.
//!
//! Called from:
//! - `crate::codegen::wasm::module::function_metadata`.
//!
//! Key details:
//! - Only records a function when every discovered return forwards the same array parameter.

use super::*;

pub(in crate::codegen::wasm::module) fn collect_function_array_return_param_indices(
    program: &Program,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
) -> HashMap<String, usize> {
    program
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl { name, params, body, .. } => {
                let param_kinds = function_param_kinds.get(&function_key(name))?;
                consistent_array_return_param_index(params, param_kinds, body)
                    .map(|index| (function_key(name), index))
            }
            _ => None,
        })
        .collect()
}

fn consistent_array_return_param_index(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    param_kinds: &[LocalKind],
    stmts: &[Stmt],
) -> Option<usize> {
    let mut index = None;
    for stmt in stmts {
        collect_array_return_param_index(stmt, params, param_kinds, &mut index)?;
    }
    index
}

fn collect_array_return_param_index(
    stmt: &Stmt,
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    param_kinds: &[LocalKind],
    index: &mut Option<usize>,
) -> Option<()> {
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => {
            let ExprKind::Variable(name) = &expr.kind else {
                return None;
            };
            let next = params
                .iter()
                .position(|(param_name, _, _, _)| param_name == name)
                .filter(|param_index| param_kinds.get(*param_index) == Some(&LocalKind::Array))?;
            match index {
                Some(existing) if *existing != next => None,
                Some(_) => Some(()),
                None => {
                    *index = Some(next);
                    Some(())
                }
            }
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            for stmt in then_body {
                collect_array_return_param_index(stmt, params, param_kinds, index)?;
            }
            for (_, body) in elseif_clauses {
                for stmt in body {
                    collect_array_return_param_index(stmt, params, param_kinds, index)?;
                }
            }
            if let Some(body) = else_body {
                for stmt in body {
                    collect_array_return_param_index(stmt, params, param_kinds, index)?;
                }
            }
            Some(())
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for stmt in stmts {
                collect_array_return_param_index(stmt, params, param_kinds, index)?;
            }
            Some(())
        }
        _ => Some(()),
    }
}
