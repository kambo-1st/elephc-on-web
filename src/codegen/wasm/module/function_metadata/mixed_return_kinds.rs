//! Purpose:
//! Collects wasm mixed-return value-cell metadata for user functions.
//! Keeps mixed return branch traversal separate from other array/function metadata collectors.
//!
//! Called from:
//! - `crate::codegen::wasm::module::function_metadata`.
//!
//! Key details:
//! - Tracks local value kinds and array access metadata so `mixed` returns keep their runtime cell kind.

use super::*;

pub(in crate::codegen::wasm::module) fn collect_function_mixed_return_kinds(
    program: &Program,
) -> HashMap<String, ValueCellKind> {
    program
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl {
                name,
                params,
                return_type,
                body,
                ..
            } if value_kind_from_return_type(return_type.as_ref()) == ValueKind::Mixed => {
                consistent_mixed_return_value_kind(params, body).map(|kind| (function_key(name), kind))
            }
            _ => None,
        })
        .collect()
}

fn consistent_mixed_return_value_kind(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    stmts: &[Stmt],
) -> Option<ValueCellKind> {
    let mut kind = None;
    let mut local_value_kinds = mixed_return_param_value_kinds(params);
    let mut array_value_kinds = HashMap::new();
    let mut array_nested_values = HashMap::new();
    let mut array_key_values = HashMap::new();
    for stmt in stmts {
        collect_mixed_return_value_kind(
            stmt,
            &mut kind,
            &mut local_value_kinds,
            &mut array_value_kinds,
            &mut array_nested_values,
            &mut array_key_values,
        )?;
    }
    kind
}

fn mixed_return_param_value_kinds(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
) -> HashMap<String, ValueCellKind> {
    params
        .iter()
        .filter_map(|(name, ty, _, _)| {
            value_cell_kind_from_type(ty.as_ref()).map(|kind| (name.clone(), kind))
        })
        .collect()
}

fn collect_mixed_return_value_kind(
    stmt: &Stmt,
    kind: &mut Option<ValueCellKind>,
    local_value_kinds: &mut HashMap<String, ValueCellKind>,
    array_value_kinds: &mut HashMap<String, Vec<ValueCellKind>>,
    array_nested_values: &mut HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_key_values: &mut HashMap<String, Vec<AssocKeyValue>>,
) -> Option<()> {
    match &stmt.kind {
        StmtKind::Assign { name, value } => {
            if let Some(value_kind) = mixed_return_expr_value_kind(
                value,
                local_value_kinds,
                array_value_kinds,
                array_nested_values,
                array_key_values,
            ) {
                local_value_kinds.insert(name.clone(), value_kind);
            } else {
                local_value_kinds.remove(name);
            }
            collect_array_assignment_metadata_for_mixed_return(
                name,
                value,
                array_value_kinds,
                array_nested_values,
                array_key_values,
            );
            Some(())
        }
        StmtKind::Return(Some(expr)) => {
            let next = mixed_return_expr_value_kind(
                expr,
                local_value_kinds,
                array_value_kinds,
                array_nested_values,
                array_key_values,
            )?;
            match kind {
                Some(existing) if *existing != next => None,
                Some(_) => Some(()),
                None => {
                    *kind = Some(next);
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
                collect_mixed_return_value_kind(
                    stmt,
                    kind,
                    local_value_kinds,
                    array_value_kinds,
                    array_nested_values,
                    array_key_values,
                )?;
            }
            for (_, body) in elseif_clauses {
                for stmt in body {
                    collect_mixed_return_value_kind(
                        stmt,
                        kind,
                        local_value_kinds,
                        array_value_kinds,
                        array_nested_values,
                        array_key_values,
                    )?;
                }
            }
            if let Some(body) = else_body {
                for stmt in body {
                    collect_mixed_return_value_kind(
                        stmt,
                        kind,
                        local_value_kinds,
                        array_value_kinds,
                        array_nested_values,
                        array_key_values,
                    )?;
                }
            }
            Some(())
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for stmt in stmts {
                collect_mixed_return_value_kind(
                    stmt,
                    kind,
                    local_value_kinds,
                    array_value_kinds,
                    array_nested_values,
                    array_key_values,
                )?;
            }
            Some(())
        }
        _ => Some(()),
    }
}

fn collect_array_assignment_metadata_for_mixed_return(
    name: &str,
    value: &Expr,
    array_value_kinds: &mut HashMap<String, Vec<ValueCellKind>>,
    array_nested_values: &mut HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_key_values: &mut HashMap<String, Vec<AssocKeyValue>>,
) {
    match &value.kind {
        ExprKind::ArrayLiteral(items) => {
            if let Some(kinds) = static_value_cell_kinds_for_items(items) {
                array_value_kinds.insert(name.to_string(), kinds);
            } else {
                array_value_kinds.remove(name);
            }
            if let Some(metadata) = static_nested_array_metadata_for_items(items) {
                array_nested_values.insert(name.to_string(), metadata);
            } else {
                array_nested_values.remove(name);
            }
            array_key_values.remove(name);
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            if let Some(kinds) = static_value_cell_kinds_for_assoc_items(items) {
                array_value_kinds.insert(name.to_string(), kinds);
            } else {
                array_value_kinds.remove(name);
            }
            if let Some(metadata) = static_nested_array_metadata_for_assoc_items(items) {
                array_nested_values.insert(name.to_string(), metadata);
            } else {
                array_nested_values.remove(name);
            }
            if let Some(values) = static_assoc_key_values_for_items(items) {
                array_key_values.insert(name.to_string(), values);
            } else {
                array_key_values.remove(name);
            }
        }
        ExprKind::ArrayAccess { .. } => {
            if let Some(metadata) = array_access_metadata::array_access_metadata_for_locals(
                value,
                array_nested_values,
                array_key_values,
            ) {
                if let Some(kinds) = metadata.value_kinds {
                    array_value_kinds.insert(name.to_string(), kinds);
                } else {
                    array_value_kinds.remove(name);
                }
                if let Some(nested_values) = metadata.nested_values {
                    array_nested_values.insert(name.to_string(), nested_values);
                } else {
                    array_nested_values.remove(name);
                }
                if let Some(key_values) = metadata.key_values {
                    array_key_values.insert(name.to_string(), key_values);
                } else {
                    array_key_values.remove(name);
                }
            }
        }
        _ => {}
    }
}

fn mixed_return_expr_value_kind(
    expr: &Expr,
    local_value_kinds: &HashMap<String, ValueCellKind>,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) -> Option<ValueCellKind> {
    if let Some(kind) = static_value_cell_kind_for_expr(expr) {
        return Some(kind);
    }
    if let ExprKind::Variable(name) = &expr.kind {
        return local_value_kinds.get(name).copied();
    }
    array_access_metadata::array_access_value_cell_kind_for_locals(
        expr,
        array_value_kinds,
        array_nested_values,
        array_key_values,
    )
}
