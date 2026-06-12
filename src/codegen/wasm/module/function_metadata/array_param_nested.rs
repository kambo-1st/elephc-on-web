//! Purpose:
//! Collects runtime nested-array metadata for wasm user-function array parameters.
//! This lets array parameters retain row-shape knowledge for dynamic chunked arrays.
//!
//! Called from:
//! - `crate::codegen::wasm::module::function_metadata`.
//!
//! Key details:
//! - Tracks only consistent callsite metadata and stays compile-time only.
//! - Runtime-size `array_chunk()` rows use runtime nested metadata rather than exact row lengths.

use super::*;

pub(in crate::codegen::wasm::module) fn collect_function_array_param_runtime_nested_values(
    program: &Program,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_param_indices: &HashMap<String, usize>,
) -> HashMap<String, Vec<Option<NestedArrayMetadata>>> {
    let mut states = HashMap::new();
    let mut local_value_kinds = HashMap::new();
    let mut local_runtime_value_kinds = HashMap::new();
    let mut local_runtime_nested_values = HashMap::new();
    for stmt in program {
        collect_array_param_runtime_nested_calls_in_stmt(
            stmt,
            &mut states,
            &mut local_value_kinds,
            &mut local_runtime_value_kinds,
            &mut local_runtime_nested_values,
            function_params,
            function_param_kinds,
            function_defaults,
            function_array_return_value_kinds,
            function_array_return_runtime_value_kinds,
            function_array_return_param_indices,
        );
    }
    finalize_param_metadata(states)
}

fn collect_array_param_runtime_nested_calls_in_stmt(
    stmt: &Stmt,
    states: &mut HashMap<String, Vec<ParamMetadata<NestedArrayMetadata>>>,
    local_value_kinds: &mut HashMap<String, Vec<ValueCellKind>>,
    local_runtime_value_kinds: &mut HashMap<String, ValueCellKind>,
    local_runtime_nested_values: &mut HashMap<String, NestedArrayMetadata>,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_param_indices: &HashMap<String, usize>,
) {
    if let StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } = &stmt.kind {
        if let Some(kinds) = array_arg_value_kinds_with_locals(
            value,
            function_params,
            function_defaults,
            function_array_return_value_kinds,
            function_array_return_param_indices,
            local_value_kinds,
        ) {
            local_value_kinds.insert(name.clone(), kinds);
            local_runtime_value_kinds.remove(name);
        } else if let Some(kind) = array_arg_runtime_value_kind_with_locals(
            value,
            function_array_return_runtime_value_kinds,
            local_runtime_value_kinds,
        ) {
            local_value_kinds.remove(name);
            local_runtime_value_kinds.insert(name.clone(), kind);
        } else {
            local_value_kinds.remove(name);
            local_runtime_value_kinds.remove(name);
        }
        if let Some(metadata) = array_arg_runtime_nested_value_with_locals(
            value,
            function_params,
            function_defaults,
            function_array_return_value_kinds,
            function_array_return_runtime_value_kinds,
            function_array_return_param_indices,
            local_value_kinds,
            local_runtime_value_kinds,
            local_runtime_nested_values,
        ) {
            local_runtime_nested_values.insert(name.clone(), metadata);
        } else {
            local_runtime_nested_values.remove(name);
        }
    }
    collect_array_param_calls_in_stmt(stmt, &mut |name, args| {
        collect_array_param_runtime_nested_call(
            name,
            args,
            states,
            local_value_kinds,
            local_runtime_value_kinds,
            local_runtime_nested_values,
            function_params,
            function_param_kinds,
            function_defaults,
            function_array_return_value_kinds,
            function_array_return_runtime_value_kinds,
            function_array_return_param_indices,
        );
    });
}

fn collect_array_param_runtime_nested_call(
    name: &Name,
    args: &[Expr],
    states: &mut HashMap<String, Vec<ParamMetadata<NestedArrayMetadata>>>,
    local_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    local_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    local_runtime_nested_values: &HashMap<String, NestedArrayMetadata>,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
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
            array_arg_runtime_nested_value_with_locals(
                arg,
                function_params,
                function_defaults,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_array_return_param_indices,
                local_value_kinds,
                local_runtime_value_kinds,
                local_runtime_nested_values,
            )
        });
        merge_param_metadata(states, &key, index, param_kinds.len(), value);
    }
}

fn array_arg_runtime_nested_value_with_locals(
    arg: &Expr,
    function_params: &HashMap<String, Vec<String>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_param_indices: &HashMap<String, usize>,
    local_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    local_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    local_runtime_nested_values: &HashMap<String, NestedArrayMetadata>,
) -> Option<NestedArrayMetadata> {
    match &arg.kind {
        ExprKind::Variable(name) => local_runtime_nested_values.get(name).cloned(),
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_chunk") => {
            array_chunk_runtime_nested_value_for_param(
                args,
                function_params,
                function_defaults,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_array_return_param_indices,
                local_value_kinds,
                local_runtime_value_kinds,
            )
        }
        _ => None,
    }
}

fn array_chunk_runtime_nested_value_for_param(
    args: &[Expr],
    function_params: &HashMap<String, Vec<String>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_param_indices: &HashMap<String, usize>,
    local_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    local_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> Option<NestedArrayMetadata> {
    if args.len() < 2 || args.len() > 3 || literal_positive_usize(args.get(1)?).is_some() {
        return None;
    }
    let source = args.first()?;
    let preserve_keys = literal_true(args.get(2));
    if preserve_keys
        && array_arg_value_kinds_with_locals(
            source,
            function_params,
            function_defaults,
            function_array_return_value_kinds,
            function_array_return_param_indices,
            local_value_kinds,
        )
        .is_some()
    {
        return Some(NestedArrayMetadata {
            layout: ArrayLayout::Assoc,
            len: usize::MAX,
            value_kinds: None,
            key_values: None,
            nested_values: None,
        });
    }
    let kind = array_arg_runtime_value_kind_with_locals(
        source,
        function_array_return_runtime_value_kinds,
        local_runtime_value_kinds,
    )
    .or_else(|| {
        array_arg_value_kinds_with_locals(
            source,
            function_params,
            function_defaults,
            function_array_return_value_kinds,
            function_array_return_param_indices,
            local_value_kinds,
        )
        .and_then(homogeneous_value_kind)
    })?;
    Some(NestedArrayMetadata {
        layout: if preserve_keys {
            ArrayLayout::Assoc
        } else {
            ArrayLayout::Value
        },
        len: usize::MAX,
        value_kinds: Some(vec![kind]),
        key_values: None,
        nested_values: None,
    })
}

fn array_arg_value_kinds_with_locals(
    arg: &Expr,
    function_params: &HashMap<String, Vec<String>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_param_indices: &HashMap<String, usize>,
    local_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<Vec<ValueCellKind>> {
    match &arg.kind {
        ExprKind::Variable(name) => local_value_kinds.get(name).cloned(),
        ExprKind::ArrayLiteral(items) => static_value_cell_kinds_for_items(items),
        ExprKind::ArrayLiteralAssoc(items) => static_value_cell_kinds_for_assoc_items(items),
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_combine") => {
            array_arg_value_kinds_with_locals(
                args.get(1)?,
                function_params,
                function_defaults,
                function_array_return_value_kinds,
                function_array_return_param_indices,
                local_value_kinds,
            )
        }
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
                array_arg_value_kinds_with_locals(
                    arg,
                    function_params,
                    function_defaults,
                    function_array_return_value_kinds,
                    function_array_return_param_indices,
                    local_value_kinds,
                )
            }),
        _ => None,
    }
}

fn array_arg_runtime_value_kind_with_locals(
    arg: &Expr,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    local_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> Option<ValueCellKind> {
    match &arg.kind {
        ExprKind::Variable(name) => local_runtime_value_kinds.get(name).copied(),
        ExprKind::FunctionCall { name, .. } => function_array_return_runtime_value_kinds
            .get(&function_key(name))
            .copied(),
        _ => None,
    }
}

fn homogeneous_value_kind(kinds: Vec<ValueCellKind>) -> Option<ValueCellKind> {
    let mut kinds = kinds.into_iter();
    let first = kinds.next()?;
    kinds.all(|kind| kind == first).then_some(first)
}

fn literal_positive_usize(expr: &Expr) -> Option<usize> {
    match &expr.kind {
        ExprKind::IntLiteral(value) if *value > 0 => usize::try_from(*value).ok(),
        _ => None,
    }
}

fn literal_true(expr: Option<&Expr>) -> bool {
    matches!(expr.map(|expr| &expr.kind), Some(ExprKind::BoolLiteral(true)))
}
