//! Purpose:
//! Collects static array-length metadata for wasm user-function array parameters.
//! Keeps parameter length inference separate from value-kind and assoc-key metadata.
//!
//! Called from:
//! - `crate::codegen::wasm::module::function_metadata`.
//!
//! Key details:
//! - Tracks local array lengths and merges only consistent metadata into function parameter slots.

use super::*;

pub(in crate::codegen::wasm::module) fn collect_function_array_param_lengths(
    program: &Program,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_lengths: &HashMap<String, usize>,
    function_array_return_layouts: &HashMap<String, ArrayLayout>,
    function_array_return_param_indices: &HashMap<String, usize>,
) -> HashMap<String, Vec<Option<usize>>> {
    let mut states = HashMap::new();
    let mut local_array_lengths = HashMap::new();
    for stmt in program {
        collect_array_param_length_calls_in_stmt(
            stmt,
            &mut states,
            &mut local_array_lengths,
            function_params,
            function_param_kinds,
            function_defaults,
            function_array_return_lengths,
            function_array_return_layouts,
            function_array_return_param_indices,
        );
    }
    finalize_param_metadata(states)
}

fn collect_array_param_length_calls_in_stmt(
    stmt: &Stmt,
    states: &mut HashMap<String, Vec<ParamMetadata<usize>>>,
    local_array_lengths: &mut HashMap<String, usize>,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_lengths: &HashMap<String, usize>,
    function_array_return_layouts: &HashMap<String, ArrayLayout>,
    function_array_return_param_indices: &HashMap<String, usize>,
) {
    if let StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } = &stmt.kind {
        if let Some(len) = array_arg_static_length_with_locals(
            value,
            function_params,
            function_defaults,
            function_array_return_lengths,
            function_array_return_layouts,
            function_array_return_param_indices,
            local_array_lengths,
        ) {
            local_array_lengths.insert(name.clone(), len);
        } else {
            local_array_lengths.remove(name);
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
            if *kind != LocalKind::Array {
                continue;
            }
            let default = defaults
                .and_then(|defaults| defaults.get(index))
                .and_then(Option::as_ref);
            let value = arg_for_param(args, param_names, index).or(default).and_then(|arg| {
                array_arg_static_length_with_locals(
                    arg,
                    function_params,
                    function_defaults,
                    function_array_return_lengths,
                    function_array_return_layouts,
                    function_array_return_param_indices,
                    local_array_lengths,
                )
            });
            merge_param_metadata(states, &key, index, param_kinds.len(), value);
        }
    });
}

fn array_arg_static_length(
    arg: &Expr,
    function_params: &HashMap<String, Vec<String>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_lengths: &HashMap<String, usize>,
    function_array_return_layouts: &HashMap<String, ArrayLayout>,
    function_array_return_param_indices: &HashMap<String, usize>,
) -> Option<usize> {
    match &arg.kind {
        ExprKind::ArrayLiteral(items) => Some(items.len()),
        ExprKind::ArrayLiteralAssoc(items) => Some(normalized_static_assoc_items(items)?.len()),
        ExprKind::FunctionCall { name, args }
            if function_array_return_layouts.get(&function_key(name)) != Some(&ArrayLayout::Assoc) =>
        {
            let key = function_key(name);
            function_array_return_lengths.get(&key).copied().or_else(|| {
                let param_index = *function_array_return_param_indices.get(&key)?;
                let param_names = function_params.get(&key)?;
                let default = function_defaults
                    .get(&key)
                    .and_then(|defaults| defaults.get(param_index))
                    .and_then(Option::as_ref);
                let arg = arg_for_param(args, param_names, param_index).or(default)?;
                array_arg_static_length(
                    arg,
                    function_params,
                    function_defaults,
                    function_array_return_lengths,
                    function_array_return_layouts,
                    function_array_return_param_indices,
                )
            })
        }
        _ => None,
    }
}

fn array_arg_static_length_with_locals(
    arg: &Expr,
    function_params: &HashMap<String, Vec<String>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_lengths: &HashMap<String, usize>,
    function_array_return_layouts: &HashMap<String, ArrayLayout>,
    function_array_return_param_indices: &HashMap<String, usize>,
    local_array_lengths: &HashMap<String, usize>,
) -> Option<usize> {
    match &arg.kind {
        ExprKind::Variable(name) => local_array_lengths.get(name).copied(),
        _ => array_arg_static_length(
            arg,
            function_params,
            function_defaults,
            function_array_return_lengths,
            function_array_return_layouts,
            function_array_return_param_indices,
        ),
    }
}
