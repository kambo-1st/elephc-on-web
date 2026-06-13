//! Purpose:
//! Collects statically known callable targets returned by wasm user functions.
//! Keeps monomorphic callable-return support separate from runtime descriptors.
//!
//! Called from:
//! - `crate::codegen::wasm::module::function_metadata`
//!
//! Key details:
//! - Only callable-returning functions whose return paths resolve to one direct target are tracked.
//! - Finite string-built returns are tracked when every part is statically known.
//! - Dynamic, closure, instance-captured, or open-ended returns stay unknown and are rejected by codegen.

use super::*;

pub(in crate::codegen::wasm::module) fn collect_function_callable_return_targets(
    program: &Program,
    constants: &HashMap<String, ConstantValue>,
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
        if let Some(target) = callable_return_target_from_body(body, constants, &mut HashMap::new()) {
            targets.insert(function_key(name), target);
        }
    }
    targets
}

pub(in crate::codegen::wasm::module) fn collect_function_possible_callable_return_targets(
    program: &Program,
    constants: &HashMap<String, ConstantValue>,
) -> HashMap<String, Vec<String>> {
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
        let mut values = Vec::new();
        if collect_possible_callable_return_targets(
            body,
            constants,
            &mut HashMap::new(),
            &mut values,
        )
        .is_some()
            && values.len() > 1
        {
            targets.insert(function_key(name), values);
        }
    }
    targets
}

fn callable_return_target_from_body(
    body: &[Stmt],
    constants: &HashMap<String, ConstantValue>,
    local_callable_targets: &mut HashMap<String, String>,
) -> Option<String> {
    for stmt in body {
        if let Some(target) = callable_return_target_from_stmt(stmt, constants, local_callable_targets) {
            return Some(target);
        }
    }
    None
}

fn callable_return_target_from_stmt(
    stmt: &Stmt,
    constants: &HashMap<String, ConstantValue>,
    local_callable_targets: &mut HashMap<String, String>,
) -> Option<String> {
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => {
            callable_return_target_from_expr(expr, constants, local_callable_targets)
        }
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            if let Some(target) =
                callable_return_target_from_expr(value, constants, local_callable_targets)
            {
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
            let mut target = callable_return_target_from_body(then_body, constants, &mut then_targets)?;
            for (_, body) in elseif_clauses {
                let mut branch_targets = local_callable_targets.clone();
                let branch_target =
                    callable_return_target_from_body(body, constants, &mut branch_targets)?;
                if !target.eq_ignore_ascii_case(&branch_target) {
                    return None;
                }
                target = branch_target;
            }
            let mut else_targets = local_callable_targets.clone();
            let else_target =
                callable_return_target_from_body(else_body.as_deref()?, constants, &mut else_targets)?;
            target.eq_ignore_ascii_case(&else_target).then_some(target)
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            callable_return_target_from_body(stmts, constants, local_callable_targets)
        }
        _ => None,
    }
}

fn collect_possible_callable_return_targets(
    body: &[Stmt],
    constants: &HashMap<String, ConstantValue>,
    local_callable_targets: &mut HashMap<String, Vec<String>>,
    values: &mut Vec<String>,
) -> Option<()> {
    for stmt in body {
        collect_possible_callable_return_targets_from_stmt(
            stmt,
            constants,
            local_callable_targets,
            values,
        )?;
    }
    Some(())
}

fn collect_possible_callable_return_targets_from_stmt(
    stmt: &Stmt,
    constants: &HashMap<String, ConstantValue>,
    local_callable_targets: &mut HashMap<String, Vec<String>>,
    values: &mut Vec<String>,
) -> Option<()> {
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => collect_possible_callable_return_targets_from_expr(
            expr,
            constants,
            local_callable_targets,
            values,
        ),
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            let mut assigned = Vec::new();
            if collect_possible_callable_return_targets_from_expr(
                value,
                constants,
                local_callable_targets,
                &mut assigned,
            )
            .is_some()
            {
                local_callable_targets.insert(name.clone(), assigned);
            } else {
                local_callable_targets.remove(name);
            }
            Some(())
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            let mut then_targets = local_callable_targets.clone();
            collect_possible_callable_return_targets(
                then_body,
                constants,
                &mut then_targets,
                values,
            )?;
            for (_, body) in elseif_clauses {
                let mut branch_targets = local_callable_targets.clone();
                collect_possible_callable_return_targets(
                    body,
                    constants,
                    &mut branch_targets,
                    values,
                )?;
            }
            let mut else_targets = local_callable_targets.clone();
            collect_possible_callable_return_targets(
                else_body.as_deref()?,
                constants,
                &mut else_targets,
                values,
            )
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            collect_possible_callable_return_targets(stmts, constants, local_callable_targets, values)
        }
        _ => Some(()),
    }
}

fn collect_possible_callable_return_targets_from_expr(
    expr: &Expr,
    constants: &HashMap<String, ConstantValue>,
    local_callable_targets: &HashMap<String, Vec<String>>,
    values: &mut Vec<String>,
) -> Option<()> {
    for value in possible_callable_return_targets_from_expr(expr, constants, local_callable_targets)? {
        push_unique_callable_return_target(values, value);
    }
    Some(())
}

fn possible_callable_return_targets_from_expr(
    expr: &Expr,
    constants: &HashMap<String, ConstantValue>,
    local_callable_targets: &HashMap<String, Vec<String>>,
) -> Option<Vec<String>> {
    match &expr.kind {
        ExprKind::Variable(name) => Some(local_callable_targets.get(name)?.clone()),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let mut values = possible_callable_return_targets_from_expr(
                then_expr,
                constants,
                local_callable_targets,
            )?;
            for value in possible_callable_return_targets_from_expr(
                else_expr,
                constants,
                local_callable_targets,
            )? {
                push_unique_callable_return_target(&mut values, value);
            }
            Some(values)
        }
        ExprKind::BinaryOp {
            left,
            op: BinOp::Concat,
            right,
        } => {
            let left_values =
                possible_callable_return_targets_from_expr(left, constants, local_callable_targets)?;
            let right_values =
                possible_callable_return_targets_from_expr(right, constants, local_callable_targets)?;
            let mut values = Vec::new();
            for left_value in &left_values {
                for right_value in &right_values {
                    push_unique_callable_return_target(
                        &mut values,
                        format!("{}{}", left_value, right_value),
                    );
                }
            }
            Some(values)
        }
        _ => {
            let value = callable_return_target_from_expr(expr, constants, &HashMap::new())?;
            Some(vec![value])
        }
    }
}

fn push_unique_callable_return_target(values: &mut Vec<String>, value: String) {
    if !values
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&value))
    {
        values.push(value);
    }
}

fn callable_return_target_from_expr(
    expr: &Expr,
    constants: &HashMap<String, ConstantValue>,
    local_callable_targets: &HashMap<String, String>,
) -> Option<String> {
    match &expr.kind {
        ExprKind::StringLiteral(name) => Some(name.clone()),
        ExprKind::ConstRef(name) => match constants.get(name.as_str())? {
            ConstantValue::Str(value) => Some(value.clone()),
            _ => None,
        },
        ExprKind::BinaryOp {
            left,
            op: BinOp::Concat,
            right,
        } => Some(format!(
            "{}{}",
            callable_return_target_from_expr(left, constants, local_callable_targets)?,
            callable_return_target_from_expr(right, constants, local_callable_targets)?
        )),
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
            let then_target =
                callable_return_target_from_expr(then_expr, constants, local_callable_targets)?;
            let else_target =
                callable_return_target_from_expr(else_expr, constants, local_callable_targets)?;
            then_target
                .eq_ignore_ascii_case(&else_target)
                .then_some(then_target)
        }
        _ => None,
    }
}
