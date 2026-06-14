//! Purpose:
//! Infers foreach key local kinds for `array_merge` in wasm32-web local metadata.
//! Keeps merge-specific associative key aggregation out of the main collector.
//!
//! Called from:
//! - `super::foreach_key_local_kind()` and `array_transform_metadata` helpers.
//!
//! Key details:
//! - PHP-normalized runtime keys stay mixed because integer/string identity may change at runtime.
//! - Unknown merge sources stay conservative as indexed integer-key arrays.

use super::*;

pub(in crate::codegen::wasm::module) fn array_merge_foreach_key_local_kind(
    args: &[Expr],
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    php_normalized_key_arrays: &HashSet<String>,
    string_static_values: &HashMap<String, String>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> LocalKind {
    let mut saw_key = false;
    let mut all_string = true;
    let mut all_int = true;
    for arg in args {
        if expr_has_marked_php_normalized_runtime_keys(arg, php_normalized_key_arrays) {
            return LocalKind::Mixed;
        }
        let Some(kinds) = assoc_key_kinds_for_foreach_source(
            arg,
            array_key_kinds,
            array_value_kinds,
            array_runtime_value_kinds,
            string_static_values,
            function_array_return_key_kinds,
        ) else {
            return LocalKind::I64;
        };
        saw_key = true;
        all_string &= kinds.iter().all(|kind| *kind == AssocKeyKind::Str);
        all_int &= kinds.iter().all(|kind| *kind == AssocKeyKind::Int);
    }
    if saw_key && all_string {
        LocalKind::Str
    } else if saw_key && !all_int {
        LocalKind::Mixed
    } else {
        LocalKind::I64
    }
}

fn assoc_key_kinds_for_foreach_source(
    source: &Expr,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    string_static_values: &HashMap<String, String>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<Vec<AssocKeyKind>> {
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
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_merge") => {
            let mut kinds = Vec::new();
            for arg in args {
                kinds.extend(assoc_key_kinds_for_foreach_source(
                    arg,
                    array_key_kinds,
                    array_value_kinds,
                    array_runtime_value_kinds,
                    string_static_values,
                    function_array_return_key_kinds,
                )?);
            }
            Some(kinds)
        }
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("array_map")
                && matches!(args.first().map(|arg| &arg.kind), Some(ExprKind::Null)) =>
        {
            args.get(1).and_then(|source| {
                assoc_key_kinds_for_foreach_source(
                    source,
                    array_key_kinds,
                    array_value_kinds,
                    array_runtime_value_kinds,
                    string_static_values,
                    function_array_return_key_kinds,
                )
            })
        }
        ExprKind::DynamicStaticMethodCall {
            receiver: StaticReceiver::Named(class_name),
            method,
            ..
        } => dynamic_static_method_call_return_key(class_name.as_str(), method, string_static_values)
            .and_then(|key| function_array_return_key_kinds.get(&key).cloned()),
        _ => None,
    }
}
