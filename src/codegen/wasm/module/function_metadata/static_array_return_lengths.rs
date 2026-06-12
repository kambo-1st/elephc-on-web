//! Purpose:
//! Collects static array-length metadata for wasm user-function returns.
//! Keeps return length discovery separate from broader function metadata traversal.
//!
//! Called from:
//! - `crate::codegen::wasm::module::function_metadata`.
//!
//! Key details:
//! - Only records functions whose return branches all expose one consistent static array length.

use super::*;

pub(in crate::codegen::wasm::module) fn collect_function_array_return_lengths(
    program: &Program,
    constants: &HashMap<String, ConstantValue>,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
) -> HashMap<String, usize> {
    let mut lengths = program
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl { name, body, .. } => {
                consistent_static_array_return_length(body, constants)
                    .map(|len| (function_key(name), len))
            }
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    for method in object_array_return_methods(object_classes) {
        if let Some(len) = consistent_static_array_return_length(&method.body, constants) {
            lengths.insert(function_key(&method.symbol), len);
        }
    }
    lengths
}

fn consistent_static_array_return_length(
    stmts: &[Stmt],
    constants: &HashMap<String, ConstantValue>,
) -> Option<usize> {
    let mut length = None;
    for stmt in stmts {
        collect_static_array_return_length(stmt, &mut length, constants)?;
    }
    length
}

fn collect_static_array_return_length(
    stmt: &Stmt,
    length: &mut Option<usize>,
    constants: &HashMap<String, ConstantValue>,
) -> Option<()> {
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => {
            let len = match &expr.kind {
                ExprKind::ArrayLiteral(items) => items.len(),
                ExprKind::ArrayLiteralAssoc(items) => items.len(),
                ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_fill") => {
                    static_array_fill_return_len(args, constants)?
                }
                ExprKind::FunctionCall { .. } => static_split_string_array_len(expr, constants)?,
                _ => return None,
            };
            match length {
                Some(existing) if *existing != len => None,
                Some(_) => Some(()),
                None => {
                    *length = Some(len);
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
                collect_static_array_return_length(stmt, length, constants)?;
            }
            for (_, body) in elseif_clauses {
                for stmt in body {
                    collect_static_array_return_length(stmt, length, constants)?;
                }
            }
            if let Some(body) = else_body {
                for stmt in body {
                    collect_static_array_return_length(stmt, length, constants)?;
                }
            }
            Some(())
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for stmt in stmts {
                collect_static_array_return_length(stmt, length, constants)?;
            }
            Some(())
        }
        _ => Some(()),
    }
}
