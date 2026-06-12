//! Purpose:
//! Collects wasm array-return layout metadata for user functions.
//! Keeps return-layout traversal separate from parameter metadata collection.
//!
//! Called from:
//! - `crate::codegen::wasm::module::function_metadata`.
//!
//! Key details:
//! - Tracks local array/string/callable metadata while preserving consistent return layouts.

use super::*;

pub(in crate::codegen::wasm::module) fn collect_function_array_return_layouts(
    program: &Program,
    constants: &HashMap<String, ConstantValue>,
    function_return_kinds: &HashMap<String, ValueKind>,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
) -> HashMap<String, ArrayLayout> {
    let mut layouts = program
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl { name, params, body, .. } => {
                let array_params = params
                    .iter()
                    .filter_map(|(param_name, ty, _, _)| {
                        (local_kind_from_type(ty.as_ref()) == LocalKind::Array)
                            .then(|| param_name.as_str())
                    })
                    .collect::<HashSet<_>>();
                consistent_array_return_layout(body, &array_params, constants, function_return_kinds)
                    .map(|layout| (function_key(name), layout))
            }
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    for method in object_array_return_methods(object_classes) {
        let array_params = method
            .params
            .iter()
            .filter_map(|(param_name, ty)| {
                (local_kind_from_type(ty.as_ref()) == LocalKind::Array)
                    .then(|| param_name.as_str())
            })
            .collect::<HashSet<_>>();
        if let Some(layout) =
            consistent_array_return_layout(&method.body, &array_params, constants, function_return_kinds)
        {
            layouts.insert(function_key(&method.symbol), layout);
        }
    }
    layouts
}

fn consistent_array_return_layout(
    stmts: &[Stmt],
    array_params: &HashSet<&str>,
    constants: &HashMap<String, ConstantValue>,
    function_return_kinds: &HashMap<String, ValueKind>,
) -> Option<ArrayLayout> {
    let mut layout = None;
    let mut local_layouts = HashMap::new();
    let mut local_string_values = HashMap::new();
    let mut local_callable_targets = HashMap::new();
    for stmt in stmts {
        collect_array_return_layout(
            stmt,
            array_params,
            &mut layout,
            constants,
            function_return_kinds,
            &mut local_layouts,
            &mut local_string_values,
            &mut local_callable_targets,
        )?;
    }
    layout
}

fn collect_array_return_layout(
    stmt: &Stmt,
    array_params: &HashSet<&str>,
    layout: &mut Option<ArrayLayout>,
    constants: &HashMap<String, ConstantValue>,
    function_return_kinds: &HashMap<String, ValueKind>,
    local_layouts: &mut HashMap<String, ArrayLayout>,
    local_string_values: &mut HashMap<String, String>,
    local_callable_targets: &mut HashMap<String, String>,
) -> Option<()> {
    match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            if let Some(target) = callable_target_for_return_layout(value, local_callable_targets) {
                local_callable_targets.insert(name.clone(), target);
            } else {
                local_callable_targets.remove(name);
            }
            if let Some(text) = static_string_value_for_return_layout(value, local_string_values) {
                local_string_values.insert(name.clone(), text);
            } else {
                local_string_values.remove(name);
            }
            if let Some(next) = array_local_assignment_layout_for_return(
                value,
                array_params,
                constants,
                function_return_kinds,
                local_layouts,
                local_string_values,
                local_callable_targets,
            ) {
                local_layouts.insert(name.clone(), next);
            } else {
                local_layouts.remove(name);
            }
            Some(())
        }
        StmtKind::Return(Some(expr)) => {
            let next = array_return_expr_layout_for_return(
                expr,
                array_params,
                constants,
                function_return_kinds,
                local_layouts,
                local_string_values,
                local_callable_targets,
            )?;
            match layout {
                Some(existing) if *existing != next => None,
                Some(_) => Some(()),
                None => {
                    *layout = Some(next);
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
                collect_array_return_layout(
                    stmt,
                    array_params,
                    layout,
                    constants,
                    function_return_kinds,
                    &mut local_layouts.clone(),
                    &mut local_string_values.clone(),
                    &mut local_callable_targets.clone(),
                )?;
            }
            for (_, body) in elseif_clauses {
                for stmt in body {
                    collect_array_return_layout(
                        stmt,
                        array_params,
                        layout,
                        constants,
                        function_return_kinds,
                        &mut local_layouts.clone(),
                        &mut local_string_values.clone(),
                        &mut local_callable_targets.clone(),
                    )?;
                }
            }
            if let Some(body) = else_body {
                for stmt in body {
                    collect_array_return_layout(
                        stmt,
                        array_params,
                        layout,
                        constants,
                        function_return_kinds,
                        &mut local_layouts.clone(),
                        &mut local_string_values.clone(),
                        &mut local_callable_targets.clone(),
                    )?;
                }
            }
            Some(())
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for stmt in stmts {
                collect_array_return_layout(
                    stmt,
                    array_params,
                    layout,
                    constants,
                    function_return_kinds,
                    local_layouts,
                    local_string_values,
                    local_callable_targets,
                )?;
            }
            Some(())
        }
        _ => Some(()),
    }
}

fn array_return_expr_layout_for_return(
    expr: &Expr,
    array_params: &HashSet<&str>,
    constants: &HashMap<String, ConstantValue>,
    function_return_kinds: &HashMap<String, ValueKind>,
    local_layouts: &HashMap<String, ArrayLayout>,
    local_string_values: &HashMap<String, String>,
    local_callable_targets: &HashMap<String, String>,
) -> Option<ArrayLayout> {
    match &expr.kind {
        ExprKind::Variable(name) => local_layouts
            .get(name)
            .copied()
            .or_else(|| array_return_expr_layout(expr, array_params, constants)),
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_map") => {
            Some(array_map_return_layout_for_return(
                args,
                function_return_kinds,
                local_string_values,
                local_callable_targets,
            ))
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("array_filter") => {
            Some(ArrayLayout::Assoc)
        }
        _ => array_return_expr_layout(expr, array_params, constants),
    }
}

fn array_local_assignment_layout_for_return(
    expr: &Expr,
    array_params: &HashSet<&str>,
    constants: &HashMap<String, ConstantValue>,
    function_return_kinds: &HashMap<String, ValueKind>,
    local_layouts: &HashMap<String, ArrayLayout>,
    local_string_values: &HashMap<String, String>,
    local_callable_targets: &HashMap<String, String>,
) -> Option<ArrayLayout> {
    match &expr.kind {
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_map") => {
            Some(array_map_return_layout_for_return(
                args,
                function_return_kinds,
                local_string_values,
                local_callable_targets,
            ))
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("array_filter") => {
            Some(ArrayLayout::Assoc)
        }
        _ => array_return_expr_layout_for_return(
            expr,
            array_params,
            constants,
            function_return_kinds,
            local_layouts,
            local_string_values,
            local_callable_targets,
        ),
    }
}

fn array_map_return_layout_for_return(
    args: &[Expr],
    function_return_kinds: &HashMap<String, ValueKind>,
    local_string_values: &HashMap<String, String>,
    local_callable_targets: &HashMap<String, String>,
) -> ArrayLayout {
    args.first()
        .and_then(|callback| {
            static_callback_name_for_return_layout(
                callback,
                local_string_values,
                local_callable_targets,
            )
        })
        .and_then(|callback| array_map_callback_return_kind(&callback, function_return_kinds))
        .filter(|kind| *kind == ValueKind::Int)
        .map_or(ArrayLayout::Value, |_| ArrayLayout::CompactInt)
}

pub(in crate::codegen::wasm::module) fn static_callback_name_for_return_layout(
    expr: &Expr,
    local_string_values: &HashMap<String, String>,
    local_callable_targets: &HashMap<String, String>,
) -> Option<String> {
    match &expr.kind {
        ExprKind::Variable(name) => local_callable_targets
            .get(name)
            .cloned()
            .or_else(|| local_string_values.get(name).cloned()),
        _ => static_callback_name_for_locals(expr),
    }
}

pub(in crate::codegen::wasm::module) fn callable_target_for_return_layout(
    expr: &Expr,
    local_callable_targets: &HashMap<String, String>,
) -> Option<String> {
    match &expr.kind {
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => Some(name.to_string()),
        ExprKind::Variable(name) => local_callable_targets.get(name).cloned(),
        _ => None,
    }
}

pub(in crate::codegen::wasm::module) fn static_string_value_for_return_layout(
    expr: &Expr,
    local_string_values: &HashMap<String, String>,
) -> Option<String> {
    match &expr.kind {
        ExprKind::StringLiteral(text) => Some(text.clone()),
        ExprKind::Variable(name) => local_string_values.get(name).cloned(),
        _ => None,
    }
}

fn array_return_expr_layout(
    expr: &Expr,
    array_params: &HashSet<&str>,
    constants: &HashMap<String, ConstantValue>,
) -> Option<ArrayLayout> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) if array_literal_needs_value_cells_for_layout(items) => {
            Some(ArrayLayout::Value)
        }
        ExprKind::ArrayLiteralAssoc(_) => Some(ArrayLayout::Assoc),
        ExprKind::ArrayLiteral(_) => Some(ArrayLayout::CompactInt),
        ExprKind::Variable(name) if array_params.contains(name.as_str()) => Some(ArrayLayout::Value),
        ExprKind::Variable(_) => Some(ArrayLayout::Value),
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("array_pad") => {
            Some(ArrayLayout::Value)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_fill") => {
            array_fill_return_layout(args, constants)
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("array_map") => {
            Some(ArrayLayout::Value)
        }
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("array_filter")
                && matches!(
                    args.first().map(|arg| &arg.kind),
                    Some(ExprKind::FunctionCall { name, .. }) if name.eq_ignore_ascii_case("array_map")
                ) =>
        {
            Some(ArrayLayout::Assoc)
        }
        ExprKind::FunctionCall { name, .. }
            if name.eq_ignore_ascii_case("explode")
                || name.eq_ignore_ascii_case("str_split")
                || static_split_string_array_len(expr, constants).is_some() =>
        {
            Some(ArrayLayout::Value)
        }
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("array_merge")
                && array_merge_return_expr_uses_value_layout(args, array_params) =>
        {
            Some(ArrayLayout::Value)
        }
        _ => None,
    }
}

fn array_fill_return_layout(
    args: &[Expr],
    constants: &HashMap<String, ConstantValue>,
) -> Option<ArrayLayout> {
    if args.len() != 3 {
        return None;
    }
    match static_or_const_int_value_for_metadata(&args[0], constants) {
        Some(0) => {
            if array_fill_value_needs_value_layout(&args[2]) {
                Some(ArrayLayout::Value)
            } else {
                Some(ArrayLayout::CompactInt)
            }
        }
        Some(_) | None => Some(ArrayLayout::Assoc),
    }
}

fn array_fill_value_needs_value_layout(value: &Expr) -> bool {
    !matches!(
        value.kind,
        ExprKind::IntLiteral(_) | ExprKind::ConstRef(_)
    ) && !matches!(
        &value.kind,
        ExprKind::Negate(inner)
            if matches!(inner.kind, ExprKind::IntLiteral(_) | ExprKind::ConstRef(_))
    )
}

pub(in crate::codegen::wasm::module) fn static_array_fill_return_len(
    args: &[Expr],
    constants: &HashMap<String, ConstantValue>,
) -> Option<usize> {
    if args.len() != 3 {
        return None;
    }
    let count = static_or_const_int_value_for_metadata(&args[1], constants)?;
    usize::try_from(count).ok()
}

fn array_merge_return_expr_uses_value_layout(args: &[Expr], array_params: &HashSet<&str>) -> bool {
    args.iter().any(|arg| match &arg.kind {
        ExprKind::ArrayLiteral(items) => array_literal_needs_value_cells_for_layout(items),
        ExprKind::Variable(name) => array_params.contains(name.as_str()),
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("array_pad") => true,
        _ => false,
    }) && args.iter().all(|arg| match &arg.kind {
        ExprKind::ArrayLiteralAssoc(_) => false,
        ExprKind::Variable(name) => array_params.contains(name.as_str()),
        ExprKind::ArrayLiteral(_) => true,
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("array_pad") => true,
        _ => false,
    })
}

fn array_literal_needs_value_cells_for_layout(items: &[Expr]) -> bool {
    items.iter().any(|item| {
        !matches!(
            item.kind,
            ExprKind::IntLiteral(_) | ExprKind::ConstRef(_)
        ) && !matches!(
            &item.kind,
            ExprKind::Negate(inner)
                if matches!(inner.kind, ExprKind::IntLiteral(_) | ExprKind::ConstRef(_))
        )
    })
}
