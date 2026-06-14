//! Purpose:
//! Infers key local kinds for wasm32-web `foreach ($array as $key => ...)`.
//! Keeps foreach key metadata separate from broader local collection.
//!
//! Called from:
//! - `super::collect_stmt_locals()` when a foreach binds an explicit key.
//!
//! Key details:
//! - PHP-normalized runtime keys are mixed because integer and string keys can coexist.
//! - Key-preserving transforms reuse source metadata instead of duplicating array semantics.

use super::*;

pub(super) fn foreach_key_local_kind(
    array: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_runtime_nested_values: &HashMap<String, NestedArrayMetadata>,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    php_normalized_key_arrays: &HashSet<String>,
    string_static_values: &HashMap<String, String>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    array_constants: &HashMap<String, ConstantArrayValue>,
) -> LocalKind {
    if expr_has_marked_php_normalized_runtime_keys(array, php_normalized_key_arrays) {
        return LocalKind::Mixed;
    }
    if array_access_has_unknown_assoc_keys(array, array_nested_values, array_runtime_nested_values)
    {
        return LocalKind::Mixed;
    }
    let key_kinds = match &array.kind {
        ExprKind::ArrayLiteralAssoc(items) => static_assoc_key_kinds_for_items(items),
        ExprKind::ConstRef(name) => array_constant_key_kinds(name, array_constants),
        ExprKind::Variable(name) => array_key_kinds.get(name).cloned(),
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_filter") => {
            array_filter_foreach_key_kinds(
                args,
                array_key_kinds,
                array_value_kinds,
                array_runtime_value_kinds,
                function_array_return_key_kinds,
                Some(string_static_values),
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_map") => {
            array_map_foreach_key_kinds(
                args,
                array_key_kinds,
                array_value_kinds,
                array_runtime_value_kinds,
                function_array_return_value_kinds,
                function_array_return_key_kinds,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_reverse") => {
            key_preserving_transform_foreach_key_kinds(
                args,
                array_key_kinds,
                array_value_kinds,
                array_runtime_value_kinds,
                string_static_values,
                function_array_return_value_kinds,
                function_array_return_key_kinds,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_unique") => {
            key_preserving_transform_foreach_key_kinds(
                args,
                array_key_kinds,
                array_value_kinds,
                array_runtime_value_kinds,
                string_static_values,
                function_array_return_value_kinds,
                function_array_return_key_kinds,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_slice") => {
            if args.first().is_some_and(array_method_source_needs_mixed_key) {
                return LocalKind::Mixed;
            }
            if let Some(key_kinds) =
                dynamic_static_method_source_key_kinds(args.first(), string_static_values, function_array_return_key_kinds)
            {
                return key_local_kind_for_key_kinds(&key_kinds);
            }
            key_preserving_transform_foreach_key_kinds(
                args,
                array_key_kinds,
                array_value_kinds,
                array_runtime_value_kinds,
                string_static_values,
                function_array_return_value_kinds,
                function_array_return_key_kinds,
            )
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("pathinfo") => {
            Some(vec![AssocKeyKind::Str])
        }
        ExprKind::FunctionCall { name, args }
            if matches!(name.to_ascii_lowercase().as_str(), "array_diff" | "array_intersect") =>
        {
            key_preserving_transform_foreach_key_kinds(
                args,
                array_key_kinds,
                array_value_kinds,
                array_runtime_value_kinds,
                string_static_values,
                function_array_return_value_kinds,
                function_array_return_key_kinds,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_flip") => {
            array_flip_foreach_key_kinds(args, array_value_kinds)
        }
        ExprKind::FunctionCall { name, args }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "array_diff_key" | "array_intersect_key"
            ) =>
        {
            key_preserving_transform_foreach_key_kinds(
                args,
                array_key_kinds,
                array_value_kinds,
                array_runtime_value_kinds,
                string_static_values,
                function_array_return_value_kinds,
                function_array_return_key_kinds,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_flip") => {
            return array_flip_foreach_key_local_kind(args, array_value_kinds);
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_merge") => {
            return array_merge_foreach_key_local_kind(
                args,
                array_key_kinds,
                array_value_kinds,
                array_runtime_value_kinds,
                php_normalized_key_arrays,
                string_static_values,
                function_array_return_key_kinds,
            );
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_pad") => {
            if args.first().is_some_and(array_method_source_needs_mixed_key) {
                return LocalKind::Mixed;
            }
            array_pad_foreach_key_kinds(
                args,
                array_key_kinds,
                array_value_kinds,
                array_runtime_value_kinds,
                string_static_values,
                function_array_return_value_kinds,
                function_array_return_key_kinds,
            )
        }
        ExprKind::FunctionCall { name, args }
            if matches!(name.to_ascii_lowercase().as_str(), "array_fill_keys" | "array_combine") =>
        {
            direct_assoc_builder_key_kinds_for_foreach(
                args,
                array_value_kinds,
                array_runtime_value_kinds,
            )
        }
        ExprKind::FunctionCall { name, .. }
            if matches!(name.to_ascii_lowercase().as_str(), "array_values" | "array_keys") =>
        {
            None
        }
        ExprKind::FunctionCall { name, .. } => function_array_return_key_kinds
            .get(&function_key(name))
            .cloned(),
        ExprKind::StaticMethodCall {
            receiver: StaticReceiver::Named(class_name),
            method,
            ..
        } => function_array_return_key_kinds
            .get(&static_method_call_return_key(class_name.as_str(), method))
            .cloned(),
        ExprKind::DynamicStaticMethodCall {
            receiver: StaticReceiver::Named(class_name),
            method,
            ..
        } => dynamic_static_method_call_return_key(class_name.as_str(), method, string_static_values)
            .and_then(|key| function_array_return_key_kinds.get(&key).cloned()),
        ExprKind::MethodCall { .. } | ExprKind::NullsafeMethodCall { .. } => {
            return LocalKind::Mixed;
        }
        _ => None,
    };
    let Some(key_kinds) = key_kinds else {
        return LocalKind::I64;
    };
    if key_kinds.is_empty() || key_kinds.iter().all(|kind| *kind == AssocKeyKind::Int) {
        LocalKind::I64
    } else if key_kinds.iter().all(|kind| *kind == AssocKeyKind::Str) {
        LocalKind::Str
    } else {
        LocalKind::Mixed
    }
}

fn dynamic_static_method_source_key_kinds(
    source: Option<&Expr>,
    string_static_values: &HashMap<String, String>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<Vec<AssocKeyKind>> {
    let ExprKind::DynamicStaticMethodCall {
        receiver: StaticReceiver::Named(class_name),
        method,
        ..
    } = &source?.kind else {
        return None;
    };
    dynamic_static_method_call_return_key(class_name.as_str(), method, string_static_values)
        .and_then(|key| function_array_return_key_kinds.get(&key).cloned())
}

fn key_local_kind_for_key_kinds(key_kinds: &[AssocKeyKind]) -> LocalKind {
    if key_kinds.is_empty() || key_kinds.iter().all(|kind| *kind == AssocKeyKind::Int) {
        LocalKind::I64
    } else if key_kinds.iter().all(|kind| *kind == AssocKeyKind::Str) {
        LocalKind::Str
    } else {
        LocalKind::Mixed
    }
}

pub(super) fn dynamic_static_method_call_return_key(
    class_name: &str,
    method: &Expr,
    string_static_values: &HashMap<String, String>,
) -> Option<String> {
    let method = match &method.kind {
        ExprKind::StringLiteral(method) => method.clone(),
        ExprKind::Variable(name) => string_static_values.get(name)?.clone(),
        _ => return None,
    };
    Some(static_method_call_return_key(class_name, &method))
}

fn array_pad_foreach_key_kinds(
    args: &[Expr],
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    string_static_values: &HashMap<String, String>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<Vec<AssocKeyKind>> {
    let source = args.first()?;
    let target_len = static_or_const_int_value_for_locals(args.get(1)?)?;
    let mut key_kinds = key_preserving_transform_foreach_key_kinds(
        std::slice::from_ref(source),
        array_key_kinds,
        array_value_kinds,
        array_runtime_value_kinds,
        string_static_values,
        function_array_return_value_kinds,
        function_array_return_key_kinds,
    )?;
    let target_abs = usize::try_from(target_len.abs()).ok()?;
    let pad_count = target_abs.saturating_sub(key_kinds.len());
    if target_len < 0 {
        let mut padded = Vec::with_capacity(key_kinds.len() + pad_count);
        padded.extend(std::iter::repeat(AssocKeyKind::Int).take(pad_count));
        padded.extend(key_kinds);
        key_kinds = padded;
    } else {
        key_kinds.extend(std::iter::repeat(AssocKeyKind::Int).take(pad_count));
    }
    Some(key_kinds)
}

fn array_constant_key_kinds(
    name: &str,
    array_constants: &HashMap<String, ConstantArrayValue>,
) -> Option<Vec<AssocKeyKind>> {
    match array_constants.get(name)? {
        ConstantArrayValue::Indexed(items) => Some(vec![AssocKeyKind::Int; items.len()]),
        ConstantArrayValue::Assoc(items) => static_assoc_key_kinds_for_items(items),
    }
}

fn array_method_source_needs_mixed_key(source: &Expr) -> bool {
    matches!(
        source.kind,
        ExprKind::MethodCall { .. }
            | ExprKind::NullsafeMethodCall { .. }
            | ExprKind::PropertyAccess { .. }
            | ExprKind::NullsafePropertyAccess { .. }
            | ExprKind::DynamicPropertyAccess { .. }
            | ExprKind::NullsafeDynamicPropertyAccess { .. }
            | ExprKind::DynamicStaticMethodCall { .. }
    )
}

fn array_access_has_unknown_assoc_keys(
    array: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_runtime_nested_values: &HashMap<String, NestedArrayMetadata>,
) -> bool {
    let ExprKind::ArrayAccess { array: outer, index } = &array.kind else {
        return false;
    };
    let ExprKind::Variable(name) = &outer.kind else {
        return false;
    };
    let metadata = static_or_const_int_value_for_locals(index)
        .and_then(|offset| usize::try_from(offset).ok())
        .and_then(|offset| {
            array_nested_values
                .get(name)
                .and_then(|values| values.get(offset))
                .cloned()
                .flatten()
        })
        .or_else(|| array_runtime_nested_values.get(name).cloned());
    metadata.is_some_and(|metadata| {
        metadata.layout == ArrayLayout::Assoc && metadata.key_values.is_none()
    })
}
