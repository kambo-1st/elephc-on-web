//! Purpose:
//! Infers foreach value-local metadata for wasm32-web `array_map()` results.
//! Keeps callback return-kind and null-callback identity handling out of the main collector.
//!
//! Called from:
//! - `super::foreach_value_local_kind()`.
//! - Sibling array transform metadata helpers that consume nested `array_map()` expressions.
//!
//! Key details:
//! - `array_map(null, ...)` preserves source value kinds for a single source array.
//! - Static callback metadata controls typed foreach value locals when the callback is known.

use super::*;

pub(super) fn array_map_foreach_value_local_kind(
    args: &[Expr],
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_return_kinds: &HashMap<String, ValueKind>,
    function_possible_static_string_returns: &HashMap<String, Vec<String>>,
    callable_targets: &HashMap<String, String>,
    string_static_values: &HashMap<String, String>,
) -> LocalKind {
    if matches!(args.first().map(|arg| &arg.kind), Some(ExprKind::Null)) {
        if args.len() > 2 {
            return LocalKind::Array;
        }
        return args
            .get(1)
            .and_then(|source| {
                value_kinds_for_foreach_source(
                    source,
                    array_value_kinds,
                    array_runtime_value_kinds,
                )
                .or_else(|| {
                    let ExprKind::FunctionCall { name, .. } = &source.kind else {
                        return None;
                    };
                    let key = function_key(name);
                    function_array_return_value_kinds
                        .get(&key)
                        .cloned()
                        .or_else(|| {
                            function_array_return_runtime_value_kinds
                                .get(&key)
                                .map(|kind| vec![*kind])
                        })
                })
            })
            .as_deref()
            .map(foreach_value_cell_local_kind)
            .unwrap_or(LocalKind::I64);
    }
    match array_map_callback_expr_return_kind(
        args.first().expect("array_map args are nonempty after null branch"),
        function_return_kinds,
        callable_targets,
        string_static_values,
        function_possible_static_string_returns,
    ) {
        Some(ValueKind::Str) => LocalKind::Str,
        Some(ValueKind::Bool) => LocalKind::I32,
        Some(ValueKind::Int) | _ => LocalKind::I64,
    }
}
