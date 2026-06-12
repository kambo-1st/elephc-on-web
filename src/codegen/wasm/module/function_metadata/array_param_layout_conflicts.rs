//! Purpose:
//! Detects wasm user-function array parameters called with conflicting layouts.
//! Keeps static-layout safety checks separate from length/value/key metadata.
//!
//! Called from:
//! - `crate::codegen::wasm::module::function_metadata`.
//!
//! Key details:
//! - wasm currently emits one function body per PHP function, so an array param
//!   cannot be both indexed/value and associative without runtime layout dispatch.

use super::*;

pub(in crate::codegen::wasm::module) fn collect_function_array_param_layout_conflicts(
    program: &Program,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_layouts: &HashMap<String, ArrayLayout>,
    function_array_return_param_indices: &HashMap<String, usize>,
) -> Vec<(String, usize)> {
    let mut states = HashMap::new();
    let mut local_layouts = HashMap::new();
    for stmt in program {
        collect_array_param_layout_calls_in_stmt(
            stmt,
            &mut states,
            &mut local_layouts,
            function_params,
            function_param_kinds,
            function_defaults,
            function_array_return_layouts,
            function_array_return_param_indices,
        );
    }
    states
        .into_iter()
        .flat_map(|(function, params)| {
            params
                .into_iter()
                .enumerate()
                .filter_map(move |(index, state)| match state {
                    ParamMetadata::Unknown => Some((function.clone(), index)),
                    ParamMetadata::Unseen | ParamMetadata::Known(_) => None,
                })
        })
        .collect()
}

fn collect_array_param_layout_calls_in_stmt(
    stmt: &Stmt,
    states: &mut HashMap<String, Vec<ParamMetadata<ArrayLayout>>>,
    local_layouts: &mut HashMap<String, ArrayLayout>,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_layouts: &HashMap<String, ArrayLayout>,
    function_array_return_param_indices: &HashMap<String, usize>,
) {
    match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            collect_array_param_layout_calls_in_expr(
                value,
                states,
                local_layouts,
                function_params,
                function_param_kinds,
                function_defaults,
                function_array_return_layouts,
                function_array_return_param_indices,
            );
            if let Some(layout) = array_arg_static_layout_with_locals(
                value,
                function_params,
                function_defaults,
                function_array_return_layouts,
                function_array_return_param_indices,
                local_layouts,
            ) {
                local_layouts.insert(name.clone(), layout);
            } else {
                local_layouts.remove(name);
            }
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_array_param_layout_calls_in_expr(
                condition,
                states,
                local_layouts,
                function_params,
                function_param_kinds,
                function_defaults,
                function_array_return_layouts,
                function_array_return_param_indices,
            );
            collect_array_param_layout_calls_in_branch(
                then_body,
                states,
                local_layouts,
                function_params,
                function_param_kinds,
                function_defaults,
                function_array_return_layouts,
                function_array_return_param_indices,
            );
            for (condition, body) in elseif_clauses {
                collect_array_param_layout_calls_in_expr(
                    condition,
                    states,
                    local_layouts,
                    function_params,
                    function_param_kinds,
                    function_defaults,
                    function_array_return_layouts,
                    function_array_return_param_indices,
                );
                collect_array_param_layout_calls_in_branch(
                    body,
                    states,
                    local_layouts,
                    function_params,
                    function_param_kinds,
                    function_defaults,
                    function_array_return_layouts,
                    function_array_return_param_indices,
                );
            }
            if let Some(body) = else_body {
                collect_array_param_layout_calls_in_branch(
                    body,
                    states,
                    local_layouts,
                    function_params,
                    function_param_kinds,
                    function_defaults,
                    function_array_return_layouts,
                    function_array_return_param_indices,
                );
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            collect_array_param_layout_calls_in_expr(
                condition,
                states,
                local_layouts,
                function_params,
                function_param_kinds,
                function_defaults,
                function_array_return_layouts,
                function_array_return_param_indices,
            );
            collect_array_param_layout_calls_in_branch(
                body,
                states,
                local_layouts,
                function_params,
                function_param_kinds,
                function_defaults,
                function_array_return_layouts,
                function_array_return_param_indices,
            );
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_array_param_layout_calls_in_stmt(
                    init,
                    states,
                    local_layouts,
                    function_params,
                    function_param_kinds,
                    function_defaults,
                    function_array_return_layouts,
                    function_array_return_param_indices,
                );
            }
            if let Some(condition) = condition {
                collect_array_param_layout_calls_in_expr(
                    condition,
                    states,
                    local_layouts,
                    function_params,
                    function_param_kinds,
                    function_defaults,
                    function_array_return_layouts,
                    function_array_return_param_indices,
                );
            }
            collect_array_param_layout_calls_in_branch(
                body,
                states,
                local_layouts,
                function_params,
                function_param_kinds,
                function_defaults,
                function_array_return_layouts,
                function_array_return_param_indices,
            );
            if let Some(update) = update {
                collect_array_param_layout_calls_in_branch_stmt(
                    update,
                    states,
                    local_layouts,
                    function_params,
                    function_param_kinds,
                    function_defaults,
                    function_array_return_layouts,
                    function_array_return_param_indices,
                );
            }
        }
        StmtKind::Foreach { array, body, .. } => {
            collect_array_param_layout_calls_in_expr(
                array,
                states,
                local_layouts,
                function_params,
                function_param_kinds,
                function_defaults,
                function_array_return_layouts,
                function_array_return_param_indices,
            );
            collect_array_param_layout_calls_in_branch(
                body,
                states,
                local_layouts,
                function_params,
                function_param_kinds,
                function_defaults,
                function_array_return_layouts,
                function_array_return_param_indices,
            );
        }
        StmtKind::Switch { subject, cases, default } => {
            collect_array_param_layout_calls_in_expr(
                subject,
                states,
                local_layouts,
                function_params,
                function_param_kinds,
                function_defaults,
                function_array_return_layouts,
                function_array_return_param_indices,
            );
            for (conditions, body) in cases {
                for condition in conditions {
                    collect_array_param_layout_calls_in_expr(
                        condition,
                        states,
                        local_layouts,
                        function_params,
                        function_param_kinds,
                        function_defaults,
                        function_array_return_layouts,
                        function_array_return_param_indices,
                    );
                }
                collect_array_param_layout_calls_in_branch(
                    body,
                    states,
                    local_layouts,
                    function_params,
                    function_param_kinds,
                    function_defaults,
                    function_array_return_layouts,
                    function_array_return_param_indices,
                );
            }
            if let Some(default) = default {
                collect_array_param_layout_calls_in_branch(
                    default,
                    states,
                    local_layouts,
                    function_params,
                    function_param_kinds,
                    function_defaults,
                    function_array_return_layouts,
                    function_array_return_param_indices,
                );
            }
        }
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => {
            collect_array_param_layout_calls_in_stmts(
                body,
                states,
                local_layouts,
                function_params,
                function_param_kinds,
                function_defaults,
                function_array_return_layouts,
                function_array_return_param_indices,
            );
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            collect_array_param_layout_calls_in_branch(
                try_body,
                states,
                local_layouts,
                function_params,
                function_param_kinds,
                function_defaults,
                function_array_return_layouts,
                function_array_return_param_indices,
            );
            for catch in catches {
                collect_array_param_layout_calls_in_branch(
                    &catch.body,
                    states,
                    local_layouts,
                    function_params,
                    function_param_kinds,
                    function_defaults,
                    function_array_return_layouts,
                    function_array_return_param_indices,
                );
            }
            if let Some(finally_body) = finally_body {
                collect_array_param_layout_calls_in_branch(
                    finally_body,
                    states,
                    local_layouts,
                    function_params,
                    function_param_kinds,
                    function_defaults,
                    function_array_return_layouts,
                    function_array_return_param_indices,
                );
            }
        }
        _ => collect_array_param_calls_in_stmt(stmt, &mut |name, args| {
            merge_array_param_layout_call(
                states,
                local_layouts,
                name,
                args,
                function_params,
                function_param_kinds,
                function_defaults,
                function_array_return_layouts,
                function_array_return_param_indices,
            );
        }),
    }
}

fn collect_array_param_layout_calls_in_stmts(
    stmts: &[Stmt],
    states: &mut HashMap<String, Vec<ParamMetadata<ArrayLayout>>>,
    local_layouts: &mut HashMap<String, ArrayLayout>,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_layouts: &HashMap<String, ArrayLayout>,
    function_array_return_param_indices: &HashMap<String, usize>,
) {
    for stmt in stmts {
        collect_array_param_layout_calls_in_stmt(
            stmt,
            states,
            local_layouts,
            function_params,
            function_param_kinds,
            function_defaults,
            function_array_return_layouts,
            function_array_return_param_indices,
        );
    }
}

fn collect_array_param_layout_calls_in_branch_stmt(
    stmt: &Stmt,
    states: &mut HashMap<String, Vec<ParamMetadata<ArrayLayout>>>,
    parent_local_layouts: &HashMap<String, ArrayLayout>,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_layouts: &HashMap<String, ArrayLayout>,
    function_array_return_param_indices: &HashMap<String, usize>,
) {
    let mut branch_local_layouts = parent_local_layouts.clone();
    collect_array_param_layout_calls_in_stmt(
        stmt,
        states,
        &mut branch_local_layouts,
        function_params,
        function_param_kinds,
        function_defaults,
        function_array_return_layouts,
        function_array_return_param_indices,
    );
}

fn collect_array_param_layout_calls_in_branch(
    stmts: &[Stmt],
    states: &mut HashMap<String, Vec<ParamMetadata<ArrayLayout>>>,
    parent_local_layouts: &HashMap<String, ArrayLayout>,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_layouts: &HashMap<String, ArrayLayout>,
    function_array_return_param_indices: &HashMap<String, usize>,
) {
    let mut branch_local_layouts = parent_local_layouts.clone();
    collect_array_param_layout_calls_in_stmts(
        stmts,
        states,
        &mut branch_local_layouts,
        function_params,
        function_param_kinds,
        function_defaults,
        function_array_return_layouts,
        function_array_return_param_indices,
    );
}

fn collect_array_param_layout_calls_in_expr(
    expr: &Expr,
    states: &mut HashMap<String, Vec<ParamMetadata<ArrayLayout>>>,
    local_layouts: &HashMap<String, ArrayLayout>,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_layouts: &HashMap<String, ArrayLayout>,
    function_array_return_param_indices: &HashMap<String, usize>,
) {
    collect_array_param_calls_in_expr(expr, &mut |name, args| {
        merge_array_param_layout_call(
            states,
            local_layouts,
            name,
            args,
            function_params,
            function_param_kinds,
            function_defaults,
            function_array_return_layouts,
            function_array_return_param_indices,
        );
    });
}

fn merge_array_param_layout_call(
    states: &mut HashMap<String, Vec<ParamMetadata<ArrayLayout>>>,
    local_layouts: &HashMap<String, ArrayLayout>,
    name: &Name,
    args: &[Expr],
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_layouts: &HashMap<String, ArrayLayout>,
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
            array_arg_static_layout_with_locals(
                arg,
                function_params,
                function_defaults,
                function_array_return_layouts,
                function_array_return_param_indices,
                local_layouts,
            )
        });
        merge_known_layout_metadata(states, &key, index, param_kinds.len(), value);
    }
}

fn merge_known_layout_metadata(
    states: &mut HashMap<String, Vec<ParamMetadata<ArrayLayout>>>,
    function_name: &str,
    param_index: usize,
    param_count: usize,
    value: Option<ArrayLayout>,
) {
    let Some(value) = value else {
        return;
    };
    let slots = states
        .entry(function_name.to_string())
        .or_insert_with(|| vec![ParamMetadata::Unseen; param_count]);
    let Some(slot) = slots.get_mut(param_index) else {
        return;
    };
    match slot {
        slot @ ParamMetadata::Unseen => *slot = ParamMetadata::Known(value),
        ParamMetadata::Known(existing) if *existing == value => {}
        slot => *slot = ParamMetadata::Unknown,
    }
}

fn array_arg_static_layout_with_locals(
    arg: &Expr,
    function_params: &HashMap<String, Vec<String>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_layouts: &HashMap<String, ArrayLayout>,
    function_array_return_param_indices: &HashMap<String, usize>,
    local_layouts: &HashMap<String, ArrayLayout>,
) -> Option<ArrayLayout> {
    match &arg.kind {
        ExprKind::Variable(name) => local_layouts.get(name).copied(),
        ExprKind::ArrayLiteral(_) => Some(ArrayLayout::Value),
        ExprKind::ArrayLiteralAssoc(_) => Some(ArrayLayout::Assoc),
        ExprKind::FunctionCall { name, args } => {
            let key = function_key(name);
            function_array_return_layouts.get(&key).copied().or_else(|| {
                let param_index = *function_array_return_param_indices.get(&key)?;
                let param_names = function_params.get(&key)?;
                let default = function_defaults
                    .get(&key)
                    .and_then(|defaults| defaults.get(param_index))
                    .and_then(Option::as_ref);
                let arg = arg_for_param(args, param_names, param_index).or(default)?;
                array_arg_static_layout_with_locals(
                    arg,
                    function_params,
                    function_defaults,
                    function_array_return_layouts,
                    function_array_return_param_indices,
                    local_layouts,
                )
            })
        }
        _ => None,
    }
}
