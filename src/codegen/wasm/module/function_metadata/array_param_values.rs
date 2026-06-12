//! Purpose:
//! Collects value-cell kind metadata for wasm user-function array parameters.
//! Keeps array-value inference separate from return and associative-key metadata.
//!
//! Called from:
//! - `crate::codegen::wasm::module::function_metadata`.
//!
//! Key details:
//! - Tracks local array value kinds across control-flow and array_chunk foreach paths.

use super::*;

pub(in crate::codegen::wasm::module) fn collect_function_array_param_value_kinds(
    program: &Program,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_param_indices: &HashMap<String, usize>,
) -> HashMap<String, Vec<Option<Vec<ValueCellKind>>>> {
    let mut states = HashMap::new();
    let max_passes = program.len().saturating_add(function_params.len()).saturating_add(2);
    for _ in 0..max_passes {
        let before = states.clone();
        let mut local_array_value_kinds = HashMap::new();
        let mut local_chunk_value_kinds = HashMap::new();
        for stmt in program {
            collect_array_param_value_kind_calls_in_stmt(
                stmt,
                &mut states,
                &mut local_array_value_kinds,
                &mut local_chunk_value_kinds,
                function_params,
                function_param_kinds,
                function_defaults,
                function_array_return_value_kinds,
                function_array_return_param_indices,
            );
        }
        if states == before {
            break;
        }
    }
    finalize_param_metadata(states)
}

fn collect_array_param_value_kind_calls_in_stmt(
    stmt: &Stmt,
    states: &mut HashMap<String, Vec<ParamMetadata<Vec<ValueCellKind>>>>,
    local_array_value_kinds: &mut HashMap<String, Vec<ValueCellKind>>,
    local_chunk_value_kinds: &mut HashMap<String, Vec<ValueCellKind>>,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_param_indices: &HashMap<String, usize>,
) {
    let collect_expr = |expr: &Expr,
                        states: &mut HashMap<String, Vec<ParamMetadata<Vec<ValueCellKind>>>>,
                        local_array_value_kinds: &HashMap<String, Vec<ValueCellKind>>| {
        collect_array_param_calls_in_expr(expr, &mut |name, args| {
            collect_array_param_value_kind_call(
                name,
                args,
                states,
                local_array_value_kinds,
                function_params,
                function_param_kinds,
                function_defaults,
                function_array_return_value_kinds,
                function_array_return_param_indices,
            );
        });
    };
    match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            collect_expr(value, states, local_array_value_kinds);
            if let Some(kinds) = array_chunk_inner_value_kinds_for_param_metadata(
                value,
                function_params,
                function_defaults,
                function_array_return_value_kinds,
                function_array_return_param_indices,
                local_array_value_kinds,
            ) {
                local_array_value_kinds.insert(name.clone(), vec![ValueCellKind::Array]);
                local_chunk_value_kinds.insert(name.clone(), kinds);
            } else if let Some(kinds) = array_arg_static_value_kinds_with_locals(
                value,
                function_params,
                function_defaults,
                function_array_return_value_kinds,
                function_array_return_param_indices,
                local_array_value_kinds,
            ) {
                local_array_value_kinds.insert(name.clone(), kinds);
            }
            return;
        }
        StmtKind::Foreach { array, value_var, body, .. } => {
            collect_expr(array, states, local_array_value_kinds);
            let chunk_kinds = match &array.kind {
                ExprKind::FunctionCall { .. } => array_chunk_inner_value_kinds_for_param_metadata(
                    array,
                    function_params,
                    function_defaults,
                    function_array_return_value_kinds,
                    function_array_return_param_indices,
                    local_array_value_kinds,
                ),
                ExprKind::Variable(name) => local_chunk_value_kinds.get(name).cloned(),
                _ => None,
            };
            let mut body_array_value_kinds = local_array_value_kinds.clone();
            let mut body_chunk_value_kinds = local_chunk_value_kinds.clone();
            if let Some(kinds) = chunk_kinds {
                body_array_value_kinds.insert(value_var.clone(), kinds);
            }
            for stmt in body {
                collect_array_param_value_kind_calls_in_stmt(
                    stmt,
                    states,
                    &mut body_array_value_kinds,
                    &mut body_chunk_value_kinds,
                    function_params,
                    function_param_kinds,
                    function_defaults,
                    function_array_return_value_kinds,
                    function_array_return_param_indices,
                );
            }
            return;
        }
        StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
            collect_expr(condition, states, local_array_value_kinds);
            let mut scoped_values = local_array_value_kinds.clone();
            let mut scoped_chunks = local_chunk_value_kinds.clone();
            for stmt in then_body {
                collect_array_param_value_kind_calls_in_stmt(
                    stmt,
                    states,
                    &mut scoped_values,
                    &mut scoped_chunks,
                    function_params,
                    function_param_kinds,
                    function_defaults,
                    function_array_return_value_kinds,
                    function_array_return_param_indices,
                );
            }
            for (condition, body) in elseif_clauses {
                collect_expr(condition, states, local_array_value_kinds);
                let mut scoped_values = local_array_value_kinds.clone();
                let mut scoped_chunks = local_chunk_value_kinds.clone();
                for stmt in body {
                    collect_array_param_value_kind_calls_in_stmt(
                        stmt,
                        states,
                        &mut scoped_values,
                        &mut scoped_chunks,
                        function_params,
                        function_param_kinds,
                        function_defaults,
                        function_array_return_value_kinds,
                        function_array_return_param_indices,
                    );
                }
            }
            if let Some(body) = else_body {
                let mut scoped_values = local_array_value_kinds.clone();
                let mut scoped_chunks = local_chunk_value_kinds.clone();
                for stmt in body {
                    collect_array_param_value_kind_calls_in_stmt(
                        stmt,
                        states,
                        &mut scoped_values,
                        &mut scoped_chunks,
                        function_params,
                        function_param_kinds,
                        function_defaults,
                        function_array_return_value_kinds,
                        function_array_return_param_indices,
                    );
                }
            }
            return;
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            collect_expr(condition, states, local_array_value_kinds);
            let mut scoped_values = local_array_value_kinds.clone();
            let mut scoped_chunks = local_chunk_value_kinds.clone();
            for stmt in body {
                collect_array_param_value_kind_calls_in_stmt(
                    stmt,
                    states,
                    &mut scoped_values,
                    &mut scoped_chunks,
                    function_params,
                    function_param_kinds,
                    function_defaults,
                    function_array_return_value_kinds,
                    function_array_return_param_indices,
                );
            }
            return;
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            let mut scoped_values = local_array_value_kinds.clone();
            let mut scoped_chunks = local_chunk_value_kinds.clone();
            if let Some(init) = init {
                collect_array_param_value_kind_calls_in_stmt(
                    init,
                    states,
                    &mut scoped_values,
                    &mut scoped_chunks,
                    function_params,
                    function_param_kinds,
                    function_defaults,
                    function_array_return_value_kinds,
                    function_array_return_param_indices,
                );
            }
            if let Some(condition) = condition {
                collect_expr(condition, states, &scoped_values);
            }
            for stmt in body {
                collect_array_param_value_kind_calls_in_stmt(
                    stmt,
                    states,
                    &mut scoped_values,
                    &mut scoped_chunks,
                    function_params,
                    function_param_kinds,
                    function_defaults,
                    function_array_return_value_kinds,
                    function_array_return_param_indices,
                );
            }
            if let Some(update) = update {
                collect_array_param_value_kind_calls_in_stmt(
                    update,
                    states,
                    &mut scoped_values,
                    &mut scoped_chunks,
                    function_params,
                    function_param_kinds,
                    function_defaults,
                    function_array_return_value_kinds,
                    function_array_return_param_indices,
                );
            }
            return;
        }
        StmtKind::Switch { subject, cases, default } => {
            collect_expr(subject, states, local_array_value_kinds);
            for (conditions, body) in cases {
                for condition in conditions {
                    collect_expr(condition, states, local_array_value_kinds);
                }
                let mut scoped_values = local_array_value_kinds.clone();
                let mut scoped_chunks = local_chunk_value_kinds.clone();
                for stmt in body {
                    collect_array_param_value_kind_calls_in_stmt(
                        stmt,
                        states,
                        &mut scoped_values,
                        &mut scoped_chunks,
                        function_params,
                        function_param_kinds,
                        function_defaults,
                        function_array_return_value_kinds,
                        function_array_return_param_indices,
                    );
                }
            }
            if let Some(body) = default {
                let mut scoped_values = local_array_value_kinds.clone();
                let mut scoped_chunks = local_chunk_value_kinds.clone();
                for stmt in body {
                    collect_array_param_value_kind_calls_in_stmt(
                        stmt,
                        states,
                        &mut scoped_values,
                        &mut scoped_chunks,
                        function_params,
                        function_param_kinds,
                        function_defaults,
                        function_array_return_value_kinds,
                        function_array_return_param_indices,
                    );
                }
            }
            return;
        }
        StmtKind::FunctionDecl { name, params, body, .. } => {
            if !function_array_params_ready_for_value_kind_collection(
                name,
                states,
                function_param_kinds,
            ) {
                return;
            }
            let mut scoped_values = local_array_value_kinds.clone();
            let mut scoped_chunks = local_chunk_value_kinds.clone();
            seed_function_array_param_value_kinds(
                name,
                params,
                states,
                function_param_kinds,
                &mut scoped_values,
            );
            for stmt in body {
                collect_array_param_value_kind_calls_in_stmt(
                    stmt,
                    states,
                    &mut scoped_values,
                    &mut scoped_chunks,
                    function_params,
                    function_param_kinds,
                    function_defaults,
                    function_array_return_value_kinds,
                    function_array_return_param_indices,
                );
            }
            return;
        }
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => {
            let mut scoped_values = local_array_value_kinds.clone();
            let mut scoped_chunks = local_chunk_value_kinds.clone();
            for stmt in body {
                collect_array_param_value_kind_calls_in_stmt(
                    stmt,
                    states,
                    &mut scoped_values,
                    &mut scoped_chunks,
                    function_params,
                    function_param_kinds,
                    function_defaults,
                    function_array_return_value_kinds,
                    function_array_return_param_indices,
                );
            }
            return;
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let mut scoped_values = local_array_value_kinds.clone();
            let mut scoped_chunks = local_chunk_value_kinds.clone();
            for stmt in try_body {
                collect_array_param_value_kind_calls_in_stmt(
                    stmt,
                    states,
                    &mut scoped_values,
                    &mut scoped_chunks,
                    function_params,
                    function_param_kinds,
                    function_defaults,
                    function_array_return_value_kinds,
                    function_array_return_param_indices,
                );
            }
            for catch in catches {
                let mut scoped_values = local_array_value_kinds.clone();
                let mut scoped_chunks = local_chunk_value_kinds.clone();
                for stmt in &catch.body {
                    collect_array_param_value_kind_calls_in_stmt(
                        stmt,
                        states,
                        &mut scoped_values,
                        &mut scoped_chunks,
                        function_params,
                        function_param_kinds,
                        function_defaults,
                        function_array_return_value_kinds,
                        function_array_return_param_indices,
                    );
                }
            }
            if let Some(body) = finally_body {
                let mut scoped_values = local_array_value_kinds.clone();
                let mut scoped_chunks = local_chunk_value_kinds.clone();
                for stmt in body {
                    collect_array_param_value_kind_calls_in_stmt(
                        stmt,
                        states,
                        &mut scoped_values,
                        &mut scoped_chunks,
                        function_params,
                        function_param_kinds,
                        function_defaults,
                        function_array_return_value_kinds,
                        function_array_return_param_indices,
                    );
                }
            }
            return;
        }
        _ => {}
    }
    collect_array_param_calls_in_stmt(stmt, &mut |name, args| {
        collect_array_param_value_kind_call(
            name,
            args,
            states,
            local_array_value_kinds,
            function_params,
            function_param_kinds,
            function_defaults,
            function_array_return_value_kinds,
            function_array_return_param_indices,
        );
    });
}

fn collect_array_param_value_kind_call(
    name: &Name,
    args: &[Expr],
    states: &mut HashMap<String, Vec<ParamMetadata<Vec<ValueCellKind>>>>,
    local_array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_param_indices: &HashMap<String, usize>,
) {
    let key = function_key(name);
    let Some(param_kinds) = function_param_kinds.get(&key) else {
        return;
    };
    let Some(param_names) = function_params.get(&key) else {
        return;
    };
    let defaults = function_defaults.get(&key);
    for (index, kind) in param_kinds.iter().enumerate() {
        if *kind != LocalKind::Array {
            continue;
        }
        let default = defaults
            .and_then(|defaults| defaults.get(index))
            .and_then(Option::as_ref);
        let value = arg_for_param(args, param_names, index).or(default).and_then(|arg| {
            array_arg_static_value_kinds_with_locals(
                arg,
                function_params,
                function_defaults,
                function_array_return_value_kinds,
                function_array_return_param_indices,
                local_array_value_kinds,
            )
        });
        merge_param_metadata(states, &key, index, param_kinds.len(), value);
    }
}

fn function_array_params_ready_for_value_kind_collection(
    name: &str,
    states: &HashMap<String, Vec<ParamMetadata<Vec<ValueCellKind>>>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
) -> bool {
    let key = function_key(name);
    let Some(param_kinds) = function_param_kinds.get(&key) else {
        return true;
    };
    let Some(values) = states.get(&key) else {
        return !param_kinds.iter().any(|kind| *kind == LocalKind::Array);
    };
    param_kinds.iter().enumerate().all(|(index, kind)| {
        *kind != LocalKind::Array
            || matches!(
                values.get(index),
                Some(ParamMetadata::Known(_) | ParamMetadata::Unknown)
            )
    })
}

fn seed_function_array_param_value_kinds(
    name: &str,
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    states: &HashMap<String, Vec<ParamMetadata<Vec<ValueCellKind>>>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    local_array_value_kinds: &mut HashMap<String, Vec<ValueCellKind>>,
) {
    let key = function_key(name);
    let Some(param_kinds) = function_param_kinds.get(&key) else {
        return;
    };
    let Some(values) = states.get(&key) else {
        return;
    };
    for (index, (param_name, _, _, _)) in params.iter().enumerate() {
        if param_kinds.get(index) != Some(&LocalKind::Array) {
            continue;
        }
        if let Some(ParamMetadata::Known(kinds)) = values.get(index) {
            local_array_value_kinds.insert(param_name.clone(), kinds.clone());
        }
    }
}

fn array_arg_static_value_kinds(
    arg: &Expr,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<Vec<ValueCellKind>> {
    match &arg.kind {
        ExprKind::ArrayLiteral(items) => static_value_cell_kinds_for_items(items),
        ExprKind::ArrayLiteralAssoc(items) => static_value_cell_kinds_for_assoc_items(items),
        ExprKind::FunctionCall { name, .. } => {
            function_array_return_value_kinds.get(&function_key(name)).cloned()
        }
        _ => None,
    }
}

fn array_arg_static_value_kinds_with_locals(
    arg: &Expr,
    function_params: &HashMap<String, Vec<String>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_param_indices: &HashMap<String, usize>,
    local_array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<Vec<ValueCellKind>> {
    match &arg.kind {
        ExprKind::Variable(name) => local_array_value_kinds.get(name).cloned(),
        ExprKind::FunctionCall { name, args } => function_array_return_value_kinds
            .get(&function_key(name))
            .cloned()
            .or_else(|| {
                let key = function_key(name);
                let param_index = *function_array_return_param_indices.get(&key)?;
                let param_names = function_params.get(&key)?;
                let default = function_defaults
                    .get(&key)
                    .and_then(|defaults| defaults.get(param_index))
                    .and_then(Option::as_ref);
                let arg = arg_for_param(args, param_names, param_index).or(default)?;
                array_arg_static_value_kinds_with_locals(
                    arg,
                    function_params,
                    function_defaults,
                    function_array_return_value_kinds,
                    function_array_return_param_indices,
                    local_array_value_kinds,
                )
            }),
        _ => array_arg_static_value_kinds(arg, function_array_return_value_kinds),
    }
}

fn array_chunk_inner_value_kinds_for_param_metadata(
    arg: &Expr,
    function_params: &HashMap<String, Vec<String>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_param_indices: &HashMap<String, usize>,
    local_array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<Vec<ValueCellKind>> {
    let ExprKind::FunctionCall { name, args } = &arg.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_chunk") || args.len() != 2 {
        return None;
    }
    array_arg_static_value_kinds_with_locals(
        &args[0],
        function_params,
        function_defaults,
        function_array_return_value_kinds,
        function_array_return_param_indices,
        local_array_value_kinds,
    )
}
