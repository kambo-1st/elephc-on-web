//! Purpose:
//! Infers assignment RHS local kinds for wasm32-web metadata collection.
//! Keeps array-access and array-mutator special cases out of the main statement collector.
//!
//! Called from:
//! - `super::collect_stmt_locals()` when assigning an expression to a local.
//!
//! Key details:
//! - Dynamic or missing array accesses are widened to `Mixed` when PHP can produce null/false.
//! - Metadata-only inference must stay conservative so unsupported runtime shapes are not miscompiled.

use super::*;
use crate::codegen::wasm::module::array_access_metadata::homogeneous_nested_assoc_value_kind_for_locals;

pub(super) fn infer_assignment_local_kind(
    value: &Expr,
    locals: &HashMap<String, LocalKind>,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_runtime_nested_values: &HashMap<String, NestedArrayMetadata>,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    php_normalized_key_arrays: &HashSet<String>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    string_static_values: &HashMap<String, String>,
    function_array_return_layouts: &HashMap<String, ArrayLayout>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_return_kinds: &HashMap<String, ValueKind>,
    function_callable_return_targets: &HashMap<String, String>,
    function_possible_callable_return_targets: &HashMap<String, Vec<String>>,
) -> Option<LocalKind> {
    match &value.kind {
        ExprKind::Null
        | ExprKind::NullsafePropertyAccess { .. }
        | ExprKind::NullsafeDynamicPropertyAccess { .. }
        | ExprKind::NullsafeMethodCall { .. }
        | ExprKind::NullsafeDynamicMethodCall { .. } => Some(LocalKind::Mixed),
        ExprKind::Assignment {
            value,
            result_target: Some(result_target),
            ..
        } => infer_assignment_local_kind(
            result_target,
            locals,
            array_value_kinds,
            array_nested_values,
            array_runtime_nested_values,
            array_key_kinds,
            array_key_values,
            php_normalized_key_arrays,
            function_array_return_value_kinds,
            array_runtime_value_kinds,
            string_static_values,
            function_array_return_layouts,
            function_array_return_key_kinds,
            function_return_kinds,
            function_callable_return_targets,
            function_possible_callable_return_targets,
        )
        .or_else(|| {
            infer_assignment_local_kind(
                value,
                locals,
                array_value_kinds,
                array_nested_values,
                array_runtime_nested_values,
                array_key_kinds,
                array_key_values,
                php_normalized_key_arrays,
                function_array_return_value_kinds,
                array_runtime_value_kinds,
                string_static_values,
                function_array_return_layouts,
                function_array_return_key_kinds,
                function_return_kinds,
                function_callable_return_targets,
                function_possible_callable_return_targets,
            )
        })
        .or_else(|| {
            Some(infer_local_kind(
                value,
                locals,
                function_return_kinds,
                &HashMap::new(),
                &HashMap::new(),
            ))
        }),
        ExprKind::PropertyAccess { object, .. } | ExprKind::DynamicPropertyAccess { object, .. }
            if matches!(&object.kind, ExprKind::Variable(name) if locals.get(name) == Some(&LocalKind::Mixed)) =>
        {
            Some(LocalKind::Mixed)
        }
        ExprKind::ArrayAccess { array, index } => {
            infer_array_access_assignment_kind(
                value,
                array,
                index,
                locals,
                array_value_kinds,
                array_nested_values,
                array_runtime_nested_values,
                array_key_kinds,
                array_key_values,
                php_normalized_key_arrays,
                function_array_return_value_kinds,
                function_array_return_layouts,
            )
        }
        ExprKind::FunctionCall { name, args }
            if matches!(name.to_ascii_lowercase().as_str(), "array_pop" | "array_shift") =>
        {
            let [Expr {
                kind: ExprKind::Variable(array),
                ..
            }] = args.as_slice()
            else {
                return None;
            };
            array_value_kinds.get(array).and_then(|kinds| {
                if kinds.is_empty()
                    || kinds.iter().any(|kind| !matches!(kind, ValueCellKind::Int))
                {
                    Some(LocalKind::Mixed)
                } else {
                    None
                }
            })
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_rand") => {
            let source = match args.as_slice() {
                [source] => source,
                [source, _count] => source,
                _ => return None,
            };
            if args
                .get(1)
                .and_then(|count| {
                    array_rand_known_full_count_value(
                        source,
                        count,
                        array_key_kinds,
                        function_array_return_value_kinds,
                        string_static_values,
                        function_array_return_key_kinds,
                    )
                })
                .is_some_and(|value| value > 1)
            {
                return Some(LocalKind::Array);
            }
            if args
                .get(1)
                .is_some_and(|count| array_rand_dynamic_indexed_full_count(source, count, locals))
            {
                return Some(LocalKind::Array);
            }
            array_rand_metadata::local_kind_for_source(
                source,
                array_key_kinds,
                array_key_values,
                array_nested_values,
                php_normalized_key_arrays,
                array_value_kinds,
                array_runtime_value_kinds,
                string_static_values,
                function_array_return_layouts,
                function_array_return_key_kinds,
                function_return_kinds,
                function_callable_return_targets,
                function_possible_callable_return_targets,
            )
        }
        _ => None,
    }
}

fn array_rand_known_full_count_value(
    source: &Expr,
    count: &Expr,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    string_static_values: &HashMap<String, String>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<usize> {
    static_positive_count(count).or_else(|| {
        known_count_call_value(
            source,
            count,
            array_key_kinds,
            function_array_return_value_kinds,
            string_static_values,
            function_array_return_key_kinds,
        )
    })
}

fn array_rand_dynamic_indexed_full_count(
    source: &Expr,
    count: &Expr,
    locals: &HashMap<String, LocalKind>,
) -> bool {
    let ExprKind::Variable(source_name) = &source.kind else {
        return false;
    };
    if locals.get(source_name) != Some(&LocalKind::Array) {
        return false;
    }
    let ExprKind::FunctionCall { name, args } = &count.kind else {
        return false;
    };
    name.eq_ignore_ascii_case("count")
        && matches!(args.as_slice(), [Expr { kind: ExprKind::Variable(counted), .. }] if counted == source_name)
}

fn static_positive_count(count: &Expr) -> Option<usize> {
    let ExprKind::IntLiteral(value) = count.kind else {
        return None;
    };
    usize::try_from(value).ok()
}

fn known_count_call_value(
    source: &Expr,
    count: &Expr,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    string_static_values: &HashMap<String, String>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<usize> {
    let ExprKind::FunctionCall { name, args } = &count.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("count") || args.len() != 1 {
        return None;
    }
    known_array_len_for_metadata(
        &args[0],
        array_key_kinds,
        function_array_return_value_kinds,
        string_static_values,
        function_array_return_key_kinds,
    )
        .or_else(|| {
            known_array_len_for_metadata(
                source,
                array_key_kinds,
                function_array_return_value_kinds,
                string_static_values,
                function_array_return_key_kinds,
            )
        })
}

fn known_array_len_for_metadata(
    expr: &Expr,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    string_static_values: &HashMap<String, String>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<usize> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => Some(items.len()),
        ExprKind::ArrayLiteralAssoc(items) => Some(items.len()),
        ExprKind::Variable(name) => array_key_kinds.get(name).map(Vec::len),
        ExprKind::StaticMethodCall {
            receiver: StaticReceiver::Named(class_name),
            method,
            ..
        } => known_static_method_return_len(
            class_name.as_str(),
            method,
            array_key_kinds,
            function_array_return_value_kinds,
        ),
        ExprKind::DynamicStaticMethodCall {
            receiver: StaticReceiver::Named(class_name),
            method,
            ..
        } => {
            let key = dynamic_static_method_return_key(class_name.as_str(), method, string_static_values)?;
            array_key_kinds
                .get(&key)
                .map(Vec::len)
                .or_else(|| function_array_return_value_kinds.get(&key).map(Vec::len))
        }
        ExprKind::MethodCall { method, .. } | ExprKind::NullsafeMethodCall { method, .. } => {
            method_return_key_kinds(method, function_array_return_key_kinds)
                .map(|kinds| kinds.len())
                .or_else(|| {
                    method_return_value_kinds(method, function_array_return_value_kinds)
                        .map(|kinds| kinds.len())
                })
        }
        _ => None,
    }
}

fn known_static_method_return_len(
    class_name: &str,
    method: &str,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<usize> {
    let key = static_method_call_return_key(class_name, method);
    array_key_kinds
        .get(&key)
        .map(Vec::len)
        .or_else(|| function_array_return_value_kinds.get(&key).map(Vec::len))
}

fn dynamic_static_method_return_key(
    class_name: &str,
    method: &Expr,
    string_static_values: &HashMap<String, String>,
) -> Option<String> {
    let method = match &method.kind {
        ExprKind::StringLiteral(method) => method.as_str(),
        ExprKind::Variable(name) => string_static_values.get(name)?.as_str(),
        _ => return None,
    };
    Some(static_method_call_return_key(class_name, method))
}

fn method_return_key_kinds(
    method: &str,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<Vec<AssocKeyKind>> {
    let suffix = format!("->{}", function_key(method));
    let mut matching = function_array_return_key_kinds
        .iter()
        .filter_map(|(key, kinds)| key.ends_with(&suffix).then(|| kinds.clone()));
    let first = matching.next()?;
    if matching.all(|kinds| kinds == first) {
        Some(first)
    } else {
        None
    }
}

fn method_return_value_kinds(
    method: &str,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<Vec<ValueCellKind>> {
    let suffix = format!("->{}", function_key(method));
    let mut matching = function_array_return_value_kinds
        .iter()
        .filter_map(|(key, kinds)| key.ends_with(&suffix).then(|| kinds.clone()));
    let first = matching.next()?;
    if matching.all(|kinds| kinds == first) {
        Some(first)
    } else {
        None
    }
}

fn infer_array_access_assignment_kind(
    value: &Expr,
    array: &Expr,
    index: &Expr,
    locals: &HashMap<String, LocalKind>,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_runtime_nested_values: &HashMap<String, NestedArrayMetadata>,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    php_normalized_key_arrays: &HashSet<String>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_layouts: &HashMap<String, ArrayLayout>,
) -> Option<LocalKind> {
    if array_access_metadata::assoc_mixed_key_access_needs_mixed_cell_for_locals(
        array,
        index,
        locals,
        array_key_kinds,
        array_key_values,
        php_normalized_key_arrays,
    ) {
        return Some(LocalKind::Mixed);
    }
    if let Some(kind) = array_access_metadata::direct_array_chunk_first_scalar_kind_for_locals(
        array,
        index,
        array_value_kinds,
        function_array_return_value_kinds,
        function_array_return_layouts,
    ) {
        return Some(local_kind_for_value_cell(kind));
    }
    if array_access_metadata::array_access_metadata_for_locals(
        value,
        array_nested_values,
        array_key_values,
    )
    .is_some()
    {
        return Some(LocalKind::Array);
    }
    if array_access_metadata::dynamic_nested_array_value_metadata_for_locals(
        value,
        array_nested_values,
        array_key_values,
    )
    .is_some()
    {
        return Some(LocalKind::Array);
    }
    if let Some(kind) = pathinfo_direct_array_access_value_kind(array, index) {
        return Some(local_kind_for_value_cell(kind));
    }
    if let Some(kind) = nested_array_access_value_kind(array, index, array_nested_values, array_key_values) {
        return Some(local_kind_for_value_cell(kind));
    }
    if let Some(kind) = array_access_metadata::dynamic_parent_nested_assoc_access_value_kind_for_locals(
        array,
        index,
        array_nested_values,
        array_key_values,
    ) {
        return Some(local_kind_for_value_cell(kind));
    }
    if let Some(kind) = array_access_metadata::dynamic_outer_nested_assoc_access_value_kind_for_locals(
        array,
        index,
        array_nested_values,
    ) {
        return Some(local_kind_for_value_cell(kind));
    }
    if array_access_metadata::dynamic_outer_nested_assoc_access_needs_mixed_cell_for_locals(
        array,
        index,
        array_nested_values,
    ) {
        return Some(LocalKind::Mixed);
    }
    if runtime_nested_assoc_access_needs_mixed_cell_for_locals(
        array,
        index,
        array_runtime_nested_values,
    ) {
        return Some(LocalKind::Mixed);
    }
    if array_access_metadata::dynamic_nested_assoc_access_for_locals(
        array,
        index,
        array_nested_values,
        array_key_values,
    ) {
        return Some(LocalKind::Mixed);
    }
    if let Some(kind) = assoc_array_access_value_kind(array, index, array_value_kinds, array_key_values) {
        return Some(local_kind_for_value_cell(kind));
    }
    if let ExprKind::ArrayLiteral(items) = &array.kind {
        let index = static_or_const_int_value_for_locals(index)?;
        let Ok(index) = usize::try_from(index) else {
            return Some(LocalKind::Mixed);
        };
        if let Some(kind) = static_value_cell_kinds_for_items(items)
            .and_then(|items| items.get(index).copied())
        {
            return Some(local_kind_for_value_cell(kind));
        }
        if index >= items.len() {
            return Some(LocalKind::Mixed);
        }
    }
    if let ExprKind::FunctionCall { name, .. } = &array.kind {
        let index = static_or_const_int_value_for_locals(index)?;
        let index = usize::try_from(index).ok()?;
        return function_array_return_value_kinds
            .get(&function_key(name))
            .and_then(|items| items.get(index).copied())
            .map(local_kind_for_value_cell);
    }
    let ExprKind::Variable(name) = &array.kind else {
        return None;
    };
    let index = static_or_const_int_value_for_locals(index)?;
    let index = usize::try_from(index).ok()?;
    array_value_kinds
        .get(name)
        .and_then(|items| items.get(index).copied())
        .map(local_kind_for_value_cell)
}

fn runtime_nested_assoc_access_needs_mixed_cell_for_locals(
    array: &Expr,
    index: &Expr,
    array_runtime_nested_values: &HashMap<String, NestedArrayMetadata>,
) -> bool {
    let ExprKind::ArrayAccess {
        array: outer_array,
        index: outer_index,
    } = &array.kind
    else {
        return false;
    };
    if static_or_const_int_value_for_locals(outer_index).is_none() {
        return false;
    }
    let ExprKind::Variable(outer_name) = &outer_array.kind else {
        return false;
    };
    let Some(metadata) = array_runtime_nested_values.get(outer_name) else {
        return false;
    };
    metadata.layout == ArrayLayout::Assoc
        && (static_assoc_key_value_for_expr(index).is_none()
            || metadata.key_values.is_none())
        && homogeneous_nested_assoc_value_kind_for_locals(metadata).is_none()
}
