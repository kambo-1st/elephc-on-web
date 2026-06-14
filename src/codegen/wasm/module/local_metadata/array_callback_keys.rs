//! Purpose:
//! Infers key metadata for wasm32-web callback array builtins.
//! Keeps `array_map()` and `array_filter()` key-shape rules out of local metadata orchestration.
//!
//! Called from:
//! - `super::foreach_key_local_kind()` and array transform metadata collection.
//!
//! Key details:
//! - `array_map(null, ...)` with multiple sources reindexes rows numerically.
//! - Key-preserving callback transforms reuse known associative builder and function-return metadata.

use super::*;

pub(super) fn array_map_foreach_key_kinds(
    args: &[Expr],
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    string_static_values: Option<&HashMap<String, String>>,
) -> Option<Vec<AssocKeyKind>> {
    if matches!(args.first().map(|arg| &arg.kind), Some(ExprKind::Null)) && args.len() > 2 {
        return array_map_null_index_key_kinds(
            args,
            array_value_kinds,
            function_array_return_value_kinds,
        );
    }
    let source = args.get(1)?;
    match &source.kind {
        ExprKind::ArrayLiteralAssoc(items) => static_assoc_key_kinds_for_items(items),
        ExprKind::Variable(name) => array_key_kinds.get(name).cloned(),
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
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "class_parents" | "class_implements" | "class_uses"
            ) =>
        {
            Some(vec![AssocKeyKind::Str])
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
        } => string_static_values
            .and_then(|values| dynamic_static_method_call_return_key(class_name.as_str(), method, values))
            .and_then(|key| function_array_return_key_kinds.get(&key).cloned()),
        _ => None,
    }
}

fn array_map_null_index_key_kinds(
    args: &[Expr],
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<Vec<AssocKeyKind>> {
    let len = args
        .iter()
        .skip(1)
        .filter_map(|source| match &source.kind {
            ExprKind::ArrayLiteral(items) => Some(items.len()),
            ExprKind::ArrayLiteralAssoc(items) => Some(items.len()),
            ExprKind::Variable(name) => array_value_kinds.get(name).map(Vec::len),
            ExprKind::FunctionCall { name, .. } => function_array_return_value_kinds
                .get(&function_key(name))
                .map(Vec::len),
            _ => None,
        })
        .max()?;
    Some(vec![AssocKeyKind::Int; len])
}

pub(in crate::codegen::wasm::module) fn array_filter_foreach_key_kinds(
    args: &[Expr],
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    string_static_values: Option<&HashMap<String, String>>,
) -> Option<Vec<AssocKeyKind>> {
    let source = args.first()?;
    match &source.kind {
        ExprKind::ArrayLiteralAssoc(items) => static_assoc_key_kinds_for_items(items),
        ExprKind::Variable(name) => array_key_kinds.get(name).cloned(),
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
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "class_parents" | "class_implements" | "class_uses"
            ) =>
        {
            Some(vec![AssocKeyKind::Str])
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
        } => string_static_values
            .and_then(|values| dynamic_static_method_call_return_key(class_name.as_str(), method, values))
            .and_then(|key| function_array_return_key_kinds.get(&key).cloned()),
        _ => None,
    }
}
