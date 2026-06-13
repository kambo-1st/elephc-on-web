//! Purpose:
//! Collects statically known callable targets for wasm user-function callable parameters.
//! Keeps monomorphic callable-parameter support separate from runtime callable descriptors.
//!
//! Called from:
//! - `crate::codegen::wasm::module::function_metadata`
//!
//! Key details:
//! - Only parameters whose observed call sites all pass the same direct target are seeded.
//! - Conflicting or dynamic targets remain unknown so codegen rejects them instead of guessing.

use super::*;

pub(in crate::codegen::wasm::module) fn collect_function_callable_param_targets(
    program: &Program,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
) -> HashMap<String, Vec<Option<String>>> {
    let mut states = HashMap::new();
    let mut local_callable_targets = HashMap::new();
    for stmt in program {
        collect_callable_param_targets_in_stmt(
            stmt,
            &mut states,
            &mut local_callable_targets,
            function_params,
            function_param_kinds,
            function_defaults,
        );
    }
    finalize_param_metadata(states)
}

fn collect_callable_param_targets_in_stmt(
    stmt: &Stmt,
    states: &mut HashMap<String, Vec<ParamMetadata<String>>>,
    local_callable_targets: &mut HashMap<String, String>,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
) {
    if let StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } = &stmt.kind {
        if let Some(target) = callable_target_for_param_metadata(value, local_callable_targets) {
            local_callable_targets.insert(name.clone(), target);
        } else {
            local_callable_targets.remove(name);
        }
    }
    collect_array_param_calls_in_stmt(stmt, &mut |name, args| {
        let key = function_key(name);
        let Some(param_kinds) = function_param_kinds.get(&key) else {
            return;
        };
        let Some(param_names) = function_params.get(&key) else {
            return;
        };
        let defaults = function_defaults.get(&key);
        for (index, kind) in param_kinds.iter().enumerate() {
            if *kind != LocalKind::Callable {
                continue;
            }
            let default = defaults
                .and_then(|defaults| defaults.get(index))
                .and_then(Option::as_ref);
            let value = arg_for_param(args, param_names, index)
                .or(default)
                .and_then(|arg| callable_target_for_param_metadata(arg, local_callable_targets));
            merge_param_metadata(states, &key, index, param_kinds.len(), value);
        }
    });
}

fn callable_target_for_param_metadata(
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
            let then_target = callable_target_for_param_metadata(then_expr, local_callable_targets)?;
            let else_target = callable_target_for_param_metadata(else_expr, local_callable_targets)?;
            then_target
                .eq_ignore_ascii_case(&else_target)
                .then_some(then_target)
        }
        _ => None,
    }
}
