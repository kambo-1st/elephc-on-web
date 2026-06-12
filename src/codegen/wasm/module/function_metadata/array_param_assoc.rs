//! Purpose:
//! Collects associative-key metadata for wasm user-function array parameters.
//! Separates key-kind/key-value inference from value-cell parameter metadata.
//!
//! Called from:
//! - `crate::codegen::wasm::module::function_metadata`.
//!
//! Key details:
//! - Tracks local associative metadata and merges only consistent key metadata into parameter slots.

use super::*;

pub(in crate::codegen::wasm::module) fn collect_function_array_param_assoc_metadata(
    program: &Program,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    function_array_return_param_indices: &HashMap<String, usize>,
) -> (
    HashMap<String, Vec<Option<Vec<AssocKeyKind>>>>,
    HashMap<String, Vec<Option<Vec<AssocKeyValue>>>>,
) {
    let mut kind_states = HashMap::new();
    let mut value_states = HashMap::new();
    let mut local_key_kinds = HashMap::new();
    let mut local_key_values = HashMap::new();
    for stmt in program {
        collect_array_param_assoc_metadata_calls_in_stmt(
            stmt,
            &mut kind_states,
            &mut value_states,
            &mut local_key_kinds,
            &mut local_key_values,
            function_params,
            function_param_kinds,
            function_defaults,
            function_array_return_key_kinds,
            function_array_return_key_values,
            function_array_return_param_indices,
        );
    }
    (
        finalize_param_metadata(kind_states),
        finalize_param_metadata(value_states),
    )
}

fn collect_array_param_assoc_metadata_calls_in_stmt(
    stmt: &Stmt,
    key_kind_states: &mut HashMap<String, Vec<ParamMetadata<Vec<AssocKeyKind>>>>,
    key_value_states: &mut HashMap<String, Vec<ParamMetadata<Vec<AssocKeyValue>>>>,
    local_key_kinds: &mut HashMap<String, Vec<AssocKeyKind>>,
    local_key_values: &mut HashMap<String, Vec<AssocKeyValue>>,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    function_array_return_param_indices: &HashMap<String, usize>,
) {
    if let StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } = &stmt.kind {
        if let Some(kinds) = array_arg_static_key_kinds_with_locals(
            value,
            function_params,
            function_defaults,
            function_array_return_key_kinds,
            function_array_return_param_indices,
            local_key_kinds,
        ) {
            local_key_kinds.insert(name.clone(), kinds);
        } else {
            local_key_kinds.remove(name);
        }
        if let Some(values) = array_arg_static_key_values_with_locals(
            value,
            function_params,
            function_defaults,
            function_array_return_key_values,
            function_array_return_param_indices,
            local_key_values,
        ) {
            local_key_values.insert(name.clone(), values);
        } else {
            local_key_values.remove(name);
        }
    }
    collect_array_param_calls_in_stmt(stmt, &mut |name, args| {
        collect_array_param_assoc_metadata_call(
            name,
            args,
            key_kind_states,
            key_value_states,
            local_key_kinds,
            local_key_values,
            function_params,
            function_param_kinds,
            function_defaults,
            function_array_return_key_kinds,
            function_array_return_key_values,
            function_array_return_param_indices,
        );
    });
}

fn collect_array_param_assoc_metadata_call(
    name: &Name,
    args: &[Expr],
    key_kind_states: &mut HashMap<String, Vec<ParamMetadata<Vec<AssocKeyKind>>>>,
    key_value_states: &mut HashMap<String, Vec<ParamMetadata<Vec<AssocKeyValue>>>>,
    local_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    local_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_key_values: &HashMap<String, Vec<AssocKeyValue>>,
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
        let arg = arg_for_param(args, param_names, index).or(default);
        let key_kinds = arg.and_then(|arg| {
            array_arg_static_key_kinds_with_locals(
                arg,
                function_params,
                function_defaults,
                function_array_return_key_kinds,
                function_array_return_param_indices,
                local_key_kinds,
            )
        });
        let key_values = arg.and_then(|arg| {
            array_arg_static_key_values_with_locals(
                arg,
                function_params,
                function_defaults,
                function_array_return_key_values,
                function_array_return_param_indices,
                local_key_values,
            )
        });
        merge_param_metadata(key_kind_states, &key, index, param_kinds.len(), key_kinds);
        merge_param_metadata(key_value_states, &key, index, param_kinds.len(), key_values);
    }
}

fn array_arg_static_key_kinds_with_locals(
    arg: &Expr,
    function_params: &HashMap<String, Vec<String>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_param_indices: &HashMap<String, usize>,
    local_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<Vec<AssocKeyKind>> {
    match &arg.kind {
        ExprKind::Variable(name) => local_key_kinds.get(name).cloned(),
        ExprKind::ArrayLiteralAssoc(items) => static_assoc_key_kinds_for_items(items),
        ExprKind::FunctionCall { name, args } => function_array_return_key_kinds
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
                array_arg_static_key_kinds_with_locals(
                    arg,
                    function_params,
                    function_defaults,
                    function_array_return_key_kinds,
                    function_array_return_param_indices,
                    local_key_kinds,
                )
            }),
        _ => None,
    }
}

fn array_arg_static_key_values_with_locals(
    arg: &Expr,
    function_params: &HashMap<String, Vec<String>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    function_array_return_param_indices: &HashMap<String, usize>,
    local_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) -> Option<Vec<AssocKeyValue>> {
    match &arg.kind {
        ExprKind::Variable(name) => local_key_values.get(name).cloned(),
        ExprKind::ArrayLiteralAssoc(items) => static_assoc_key_values_for_items(items),
        ExprKind::FunctionCall { name, args } => function_array_return_key_values
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
                array_arg_static_key_values_with_locals(
                    arg,
                    function_params,
                    function_defaults,
                    function_array_return_key_values,
                    function_array_return_param_indices,
                    local_key_values,
                )
            }),
        _ => None,
    }
}
