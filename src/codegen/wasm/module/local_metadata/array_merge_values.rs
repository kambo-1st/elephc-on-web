//! Purpose:
//! Infers foreach value-local metadata for wasm32-web `array_merge()` expressions.
//! Keeps merge value aggregation separate from the local metadata dispatcher.
//!
//! Called from:
//! - `super::foreach_value_local_kind()` when direct merge results are iterated.
//!
//! Key details:
//! - Merges known value-cell metadata from each source in argument order.
//! - Handles `array_map(null, ...)` rows by preserving row value metadata where known.

use super::*;

pub(super) fn array_merge_foreach_value_local_kind(
    args: &[Expr],
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    string_static_values: &HashMap<String, String>,
) -> LocalKind {
    let mut kinds = Vec::new();
    for arg in args {
        let Some(mut source_kinds) = value_kinds_for_foreach_source(
            arg,
            array_value_kinds,
            array_runtime_value_kinds,
        )
        .or_else(|| {
            if let ExprKind::FunctionCall { name, args } = &arg.kind {
                if name.eq_ignore_ascii_case("array_map")
                    && matches!(args.first().map(|arg| &arg.kind), Some(ExprKind::Null))
                {
                    if args.len() > 2 {
                        return array_map_null_row_kinds_for_foreach(
                            args,
                            array_value_kinds,
                            function_array_return_value_kinds,
                        );
                    }
                    return array_return_value_kinds_for_foreach_source(
                        args.get(1)?,
                        function_array_return_value_kinds,
                        function_array_return_runtime_value_kinds,
                    );
                }
            }
            array_return_value_kinds_for_foreach_source(
                arg,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
            )
            .or_else(|| {
                dynamic_static_method_return_value_kinds(
                    arg,
                    string_static_values,
                    function_array_return_value_kinds,
                    function_array_return_runtime_value_kinds,
                )
            })
        }) else {
            return LocalKind::I64;
        };
        kinds.append(&mut source_kinds);
    }
    if kinds.is_empty() {
        LocalKind::I64
    } else {
        foreach_value_cell_local_kind(&kinds)
    }
}

fn dynamic_static_method_return_value_kinds(
    source: &Expr,
    string_static_values: &HashMap<String, String>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> Option<Vec<ValueCellKind>> {
    let ExprKind::DynamicStaticMethodCall {
        receiver: StaticReceiver::Named(class_name),
        method,
        ..
    } = &source.kind else {
        return None;
    };
    let key = dynamic_static_method_call_return_key(class_name.as_str(), method, string_static_values)?;
    function_array_return_value_kinds
        .get(&key)
        .cloned()
        .or_else(|| function_array_return_runtime_value_kinds.get(&key).map(|kind| vec![*kind]))
}
