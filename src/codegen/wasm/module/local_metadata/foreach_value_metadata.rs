//! Purpose:
//! Infers foreach value local kinds for wasm32-web array metadata.
//! Keeps source-specific array iteration rules out of the main statement collector.
//!
//! Called from:
//! - `super::collect_stmt_locals()` when lowering `foreach` value locals.
//!
//! Key details:
//! - Static and runtime array metadata are mapped conservatively to local storage kinds.
//! - Unknown or heterogeneous foreach values stay boxed as `Mixed`/`I64` instead of narrowing.

use super::*;

pub(super) fn foreach_value_local_kind(
    array: &Expr,
    locals: &HashMap<String, LocalKind>,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    php_normalized_key_arrays: &HashSet<String>,
    callable_targets: &HashMap<String, String>,
    string_static_values: &HashMap<String, String>,
    function_return_kinds: &HashMap<String, ValueKind>,
    function_possible_static_string_returns: &HashMap<String, Vec<String>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    array_constants: &HashMap<String, ConstantArrayValue>,
) -> LocalKind {
    match &array.kind {
        ExprKind::ArrayLiteral(items) => {
            if homogeneous_new_object_items(items).is_some() {
                return LocalKind::Object;
            }
            let Some(kinds) = static_value_cell_kinds_for_items(items) else {
                return LocalKind::I64;
            };
            foreach_value_cell_local_kind(&kinds)
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            let Some(kinds) = static_value_cell_kinds_for_assoc_items(items) else {
                return LocalKind::I64;
            };
            assoc_foreach_value_local_kind(&kinds)
        }
        ExprKind::ConstRef(name) => match array_constants.get(name.as_str()) {
            Some(ConstantArrayValue::Indexed(items)) => {
                let Some(kinds) = static_value_cell_kinds_for_items(items) else {
                    return LocalKind::I64;
                };
                foreach_value_cell_local_kind(&kinds)
            }
            Some(ConstantArrayValue::Assoc(items)) => {
                let Some(kinds) = static_value_cell_kinds_for_assoc_items(items) else {
                    return LocalKind::I64;
                };
                assoc_foreach_value_local_kind(&kinds)
            }
            None => LocalKind::I64,
        },
        ExprKind::Variable(name)
            if locals.get(name) == Some(&LocalKind::Array)
                && array_key_kinds.contains_key(name)
                && array_value_kinds.get(name).is_some() =>
        {
            assoc_foreach_value_local_kind(
                array_value_kinds
                    .get(name)
                    .expect("checked by guard for associative foreach metadata"),
            )
        }
        ExprKind::Variable(name)
            if locals.get(name) == Some(&LocalKind::Array)
                && array_runtime_value_kinds.get(name).is_some() =>
        {
            local_kind_for_value_cell(
                *array_runtime_value_kinds
                    .get(name)
                    .expect("checked by guard for runtime value foreach metadata"),
            )
        }
        ExprKind::Variable(name)
            if locals.get(name) == Some(&LocalKind::Array)
                && array_value_kinds.get(name).is_some() =>
        {
            foreach_value_cell_local_kind(
                array_value_kinds
                    .get(name)
                    .expect("checked by guard for value foreach metadata"),
            )
        }
        ExprKind::Variable(name)
            if locals.get(name) == Some(&LocalKind::Array)
                && php_normalized_key_arrays.contains(name) =>
        {
            LocalKind::Mixed
        }
        ExprKind::FunctionCall { name, .. }
            if function_array_return_key_kinds.contains_key(&function_key(name))
                && function_array_return_value_kinds
                    .get(&function_key(name))
                    .is_some() =>
        {
            assoc_foreach_value_local_kind(
                function_array_return_value_kinds
                    .get(&function_key(name))
                    .expect("checked by guard for associative return foreach metadata"),
            )
        }
        ExprKind::FunctionCall { name, .. }
            if function_array_return_value_kinds
                .get(&function_key(name))
                .is_some() =>
        {
            foreach_value_cell_local_kind(
                function_array_return_value_kinds
                    .get(&function_key(name))
                    .expect("checked by guard for value return foreach metadata"),
            )
        }
        ExprKind::FunctionCall { name, .. }
            if function_array_return_runtime_value_kinds
                .get(&function_key(name))
                .is_some() =>
        {
            local_kind_for_value_cell(
                *function_array_return_runtime_value_kinds
                    .get(&function_key(name))
                    .expect("checked by guard for runtime value return foreach metadata"),
            )
        }
        ExprKind::StaticMethodCall {
            receiver: StaticReceiver::Named(class_name),
            method,
            ..
        } if function_array_return_key_kinds
            .contains_key(&static_method_call_return_key(class_name.as_str(), method))
            && function_array_return_value_kinds
                .get(&static_method_call_return_key(class_name.as_str(), method))
                .is_some() =>
        {
            assoc_foreach_value_local_kind(
                function_array_return_value_kinds
                    .get(&static_method_call_return_key(class_name.as_str(), method))
                    .expect("checked by guard for associative static method return foreach metadata"),
            )
        }
        ExprKind::StaticMethodCall {
            receiver: StaticReceiver::Named(class_name),
            method,
            ..
        } if function_array_return_value_kinds
            .get(&static_method_call_return_key(class_name.as_str(), method))
            .is_some() =>
        {
            foreach_value_cell_local_kind(
                function_array_return_value_kinds
                    .get(&static_method_call_return_key(class_name.as_str(), method))
                    .expect("checked by guard for value static method return foreach metadata"),
            )
        }
        ExprKind::StaticMethodCall {
            receiver: StaticReceiver::Named(class_name),
            method,
            ..
        } if function_array_return_runtime_value_kinds
            .get(&static_method_call_return_key(class_name.as_str(), method))
            .is_some() =>
        {
            local_kind_for_value_cell(
                *function_array_return_runtime_value_kinds
                    .get(&static_method_call_return_key(class_name.as_str(), method))
                    .expect("checked by guard for runtime value static method return foreach metadata"),
            )
        }
        ExprKind::DynamicStaticMethodCall {
            receiver: StaticReceiver::Named(class_name),
            method,
            ..
        } => {
            if let Some(key) =
                dynamic_static_method_call_return_key(class_name.as_str(), method, string_static_values)
            {
                if function_array_return_key_kinds.contains_key(&key)
                    && function_array_return_value_kinds.get(&key).is_some()
                {
                    return assoc_foreach_value_local_kind(
                        function_array_return_value_kinds
                            .get(&key)
                            .expect("checked by guard for associative dynamic static method return foreach metadata"),
                    );
                }
                if let Some(kinds) = function_array_return_value_kinds.get(&key) {
                    return foreach_value_cell_local_kind(kinds);
                }
                if let Some(kind) = function_array_return_runtime_value_kinds.get(&key) {
                    return local_kind_for_value_cell(*kind);
                }
            }
            LocalKind::I64
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_filter") => {
            if let Some(kinds) = array_filter_array_map_null_value_kinds_for_foreach(
                args,
                array_value_kinds,
                function_array_return_value_kinds,
            ) {
                return foreach_value_cell_local_kind(&kinds);
            }
            if let Some(kinds) =
                array_filter_metadata::default_value_kinds_for_assignment(array, array_value_kinds)
            {
                return foreach_value_cell_local_kind(&kinds);
            }
            if let Some(kinds) =
                array_filter_metadata::callback_value_kinds_for_assignment(array, array_value_kinds)
            {
                return foreach_value_cell_local_kind(&kinds);
            }
            if let Some((_, kinds)) =
                array_filter_metadata::default_assoc_metadata_for_assignment(array)
            {
                return assoc_foreach_value_local_kind(&kinds);
            }
            if let Some((_, kinds)) =
                array_filter_metadata::callback_assoc_metadata_for_assignment(
                    array,
                    array_value_kinds,
                    array_key_kinds,
                )
            {
                return assoc_foreach_value_local_kind(&kinds);
            }
            if let Some(kinds) = array_filter_metadata::function_return_value_kinds(
                array,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
            ) {
                return foreach_value_cell_local_kind(&kinds);
            }
            if let Some(kind) = array_filter_array_map_runtime_value_kind_for_foreach(
                args,
                function_return_kinds,
                callable_targets,
                string_static_values,
                function_possible_static_string_returns,
            ) {
                return local_kind_for_value_cell(kind);
            }
            if let Some(kind) = array_filter_runtime_value_kind_for_assignment(
                array,
                array_runtime_value_kinds,
                function_array_return_runtime_value_kinds,
                function_return_kinds,
                callable_targets,
                string_static_values,
                function_possible_static_string_returns,
            ) {
                return local_kind_for_value_cell(kind);
            }
            LocalKind::I64
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_map") => {
            array_map_foreach_value_local_kind(
                args,
                array_value_kinds,
                array_runtime_value_kinds,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_return_kinds,
                function_possible_static_string_returns,
                callable_targets,
                string_static_values,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_values") => {
            array_values_foreach_value_local_kind(
                args,
                array_value_kinds,
                array_runtime_value_kinds,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_return_kinds,
                function_possible_static_string_returns,
                callable_targets,
                string_static_values,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_keys") => {
            array_keys_foreach_value_local_kind(
                args,
                array_key_kinds,
                array_value_kinds,
                array_runtime_value_kinds,
                php_normalized_key_arrays,
                function_array_return_value_kinds,
                function_array_return_key_kinds,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_reverse") => {
            array_values_foreach_value_local_kind(
                args,
                array_value_kinds,
                array_runtime_value_kinds,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_return_kinds,
                function_possible_static_string_returns,
                callable_targets,
                string_static_values,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_unique") => {
            array_values_foreach_value_local_kind(
                args,
                array_value_kinds,
                array_runtime_value_kinds,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_return_kinds,
                function_possible_static_string_returns,
                callable_targets,
                string_static_values,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_slice") => {
            if args.first().is_some_and(array_slice_method_source_needs_mixed_value) {
                return LocalKind::Mixed;
            }
            if let Some(kind) = dynamic_static_method_source_value_local_kind(
                args.first(),
                string_static_values,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_array_return_key_kinds,
            ) {
                return kind;
            }
            array_values_foreach_value_local_kind(
                args,
                array_value_kinds,
                array_runtime_value_kinds,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_return_kinds,
                function_possible_static_string_returns,
                callable_targets,
                string_static_values,
            )
        }
        ExprKind::FunctionCall { name, args }
            if matches!(name.to_ascii_lowercase().as_str(), "array_diff" | "array_intersect") =>
        {
            array_values_foreach_value_local_kind(
                args,
                array_value_kinds,
                array_runtime_value_kinds,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_return_kinds,
                function_possible_static_string_returns,
                callable_targets,
                string_static_values,
            )
        }
        ExprKind::FunctionCall { name, args }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "array_diff_key" | "array_intersect_key"
            ) =>
        {
            array_values_foreach_value_local_kind(
                args,
                array_value_kinds,
                array_runtime_value_kinds,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_return_kinds,
                function_possible_static_string_returns,
                callable_targets,
                string_static_values,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_merge") => {
            array_merge_foreach_value_local_kind(
                args,
                array_value_kinds,
                array_runtime_value_kinds,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                string_static_values,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_pad") => {
            array_pad_foreach_value_local_kind(args, array_value_kinds, array_runtime_value_kinds)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_fill") => {
            array_fill_foreach_value_local_kind(args)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_combine") => {
            args.get(1)
                .and_then(|value| {
                    runtime_value_kind_for_array_arg(value, array_value_kinds, array_runtime_value_kinds)
                })
                .map(local_kind_for_value_cell)
                .unwrap_or(LocalKind::I64)
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("array_chunk") => {
            LocalKind::Array
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_rand") => {
            array_rand_full_count_foreach_value_local_kind(
                args,
                array_key_kinds,
                function_array_return_key_kinds,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("range") => {
            range_foreach_value_local_kind(args, locals)
        }
        ExprKind::StaticMethodCall { method, args, .. }
            if method.eq_ignore_ascii_case("cases") && args.is_empty() =>
        {
            LocalKind::Object
        }
        ExprKind::MethodCall { .. } | ExprKind::NullsafeMethodCall { .. } => LocalKind::Mixed,
        ExprKind::FunctionCall { name, .. }
            if matches!(name.to_ascii_lowercase().as_str(), "explode" | "str_split") =>
        {
            LocalKind::Str
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("pathinfo") => {
            LocalKind::Str
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("class_parents") => {
            LocalKind::Str
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("class_implements") => {
            LocalKind::Str
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_flip") => {
            array_keys_foreach_value_local_kind(
                args,
                array_key_kinds,
                array_value_kinds,
                array_runtime_value_kinds,
                php_normalized_key_arrays,
                function_array_return_value_kinds,
                function_array_return_key_kinds,
            )
        }
        ExprKind::Variable(name)
            if locals.get(name) == Some(&LocalKind::Array)
                && array_value_kinds
                    .get(name)
                    .is_some_and(|kinds| kinds.iter().any(|kind| !matches!(kind, ValueCellKind::Int))) =>
        {
            foreach_value_cell_local_kind(
                array_value_kinds
                    .get(name)
                    .expect("checked by guard for value foreach metadata"),
            )
        }
        ExprKind::Variable(name)
            if locals.get(name) == Some(&LocalKind::Array)
                && array_key_kinds.contains_key(name)
                && !array_value_kinds.contains_key(name) =>
        {
            LocalKind::Mixed
        }
        _ => LocalKind::I64,
    }
}

fn dynamic_static_method_source_value_local_kind(
    source: Option<&Expr>,
    string_static_values: &HashMap<String, String>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<LocalKind> {
    let ExprKind::DynamicStaticMethodCall {
        receiver: StaticReceiver::Named(class_name),
        method,
        ..
    } = &source?.kind else {
        return None;
    };
    let key = dynamic_static_method_call_return_key(class_name.as_str(), method, string_static_values)?;
    if function_array_return_key_kinds.contains_key(&key) {
        if let Some(kinds) = function_array_return_value_kinds.get(&key) {
            return Some(assoc_foreach_value_local_kind(kinds));
        }
    }
    if let Some(kinds) = function_array_return_value_kinds.get(&key) {
        return Some(foreach_value_cell_local_kind(kinds));
    }
    function_array_return_runtime_value_kinds
        .get(&key)
        .map(|kind| local_kind_for_value_cell(*kind))
}

fn homogeneous_new_object_items(items: &[Expr]) -> Option<&str> {
    let first = match &items.first()?.kind {
        ExprKind::NewObject { class_name, .. } => class_name.as_str(),
        _ => return None,
    };
    items
        .iter()
        .all(|item| {
            matches!(
                &item.kind,
                ExprKind::NewObject { class_name, .. } if class_name.as_str() == first
            )
        })
        .then_some(first)
}

fn array_rand_full_count_foreach_value_local_kind(
    args: &[Expr],
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> LocalKind {
    if args.len() != 2 {
        return LocalKind::I64;
    }
    let count = &args[1];
    let Some(source) = args.first() else {
        return LocalKind::I64;
    };
    let key_kinds = match &source.kind {
        ExprKind::ArrayLiteralAssoc(items) => static_assoc_key_kinds_for_items(items),
        ExprKind::Variable(name) => array_key_kinds.get(name).cloned(),
        ExprKind::MethodCall { method, .. } | ExprKind::NullsafeMethodCall { method, .. } => {
            method_return_key_kinds(method, function_array_return_key_kinds)
        }
        _ => None,
    };
    let Some(key_kinds) = key_kinds else {
        return LocalKind::I64;
    };
    let Some(count_value) =
        array_rand_full_count_metadata_value(source, count, array_key_kinds, function_array_return_key_kinds)
    else {
        return LocalKind::I64;
    };
    if count_value <= 1 {
        return LocalKind::I64;
    }
    if count_value != key_kinds.len()
        && key_kinds
            .first()
            .is_none_or(|first| key_kinds.iter().any(|kind| kind != first))
    {
        return LocalKind::Mixed;
    }
    let value_kinds = key_kinds
        .iter()
        .map(|kind| match kind {
            AssocKeyKind::Int => ValueCellKind::Int,
            AssocKeyKind::Str => ValueCellKind::Str,
        })
        .collect::<Vec<_>>();
    foreach_value_cell_local_kind(&value_kinds)
}

fn array_rand_full_count_metadata_value(
    source: &Expr,
    count: &Expr,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<usize> {
    static_count_value(count)
        .or_else(|| known_count_call_value(source, count, array_key_kinds, function_array_return_key_kinds))
}

fn static_count_value(count: &Expr) -> Option<usize> {
    let ExprKind::IntLiteral(value) = count.kind else {
        return None;
    };
    usize::try_from(value).ok()
}

fn known_count_call_value(
    source: &Expr,
    count: &Expr,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<usize> {
    let ExprKind::FunctionCall { name, args } = &count.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("count") || args.len() != 1 {
        return None;
    }
    known_array_len_for_metadata(&args[0], array_key_kinds, function_array_return_key_kinds)
        .or_else(|| known_array_len_for_metadata(source, array_key_kinds, function_array_return_key_kinds))
}

fn known_array_len_for_metadata(
    expr: &Expr,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<usize> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => Some(items.len()),
        ExprKind::ArrayLiteralAssoc(items) => Some(items.len()),
        ExprKind::Variable(name) => array_key_kinds.get(name).map(Vec::len),
        ExprKind::MethodCall { method, .. } | ExprKind::NullsafeMethodCall { method, .. } => {
            method_return_key_kinds(method, function_array_return_key_kinds).map(|kinds| kinds.len())
        }
        _ => None,
    }
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

fn array_slice_method_source_needs_mixed_value(source: &Expr) -> bool {
    matches!(
        source.kind,
        ExprKind::MethodCall { .. } | ExprKind::NullsafeMethodCall { .. }
    )
}
