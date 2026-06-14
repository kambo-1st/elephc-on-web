//! Purpose:
//! Collects wasm32-web metadata produced by assignment statements.
//! Keeps array, callable, string, and runtime-value assignment bookkeeping out of the statement dispatcher.
//!
//! Called from:
//! - `super::collect_stmt_locals()` for `StmtKind::Assign`.
//!
//! Key details:
//! - This module updates the same metadata maps as the original collector branch.
//! - Unsupported or unknown array shapes are widened by removing stale narrow metadata.

use super::*;
use super::assignment_literal_collection::collect_literal_assignment_metadata;

pub(super) fn collect_assignment_locals(
    name: &String,
    value: &Expr,
    locals: &mut HashMap<String, LocalKind>,
    array_value_kinds: &mut HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &mut HashMap<String, ValueCellKind>,
    array_nested_values: &mut HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_runtime_nested_values: &mut HashMap<String, NestedArrayMetadata>,
    array_key_kinds: &mut HashMap<String, Vec<AssocKeyKind>>,
    array_key_values: &mut HashMap<String, Vec<AssocKeyValue>>,
    php_normalized_key_arrays: &mut HashSet<String>,
    callable_targets: &mut HashMap<String, String>,
    string_static_values: &mut HashMap<String, String>,
    function_return_kinds: &HashMap<String, ValueKind>,
    function_callable_return_targets: &HashMap<String, String>,
    function_possible_callable_return_targets: &HashMap<String, Vec<String>>,
    function_static_string_returns: &HashMap<String, String>,
    function_possible_static_string_returns: &HashMap<String, Vec<String>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_layouts: &HashMap<String, ArrayLayout>,
    function_array_return_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    function_array_return_param_indices: &HashMap<String, usize>,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
    constants: &HashMap<String, ConstantValue>,
    class_constants: &HashMap<String, ConstantValue>,
    array_constants: &HashMap<String, ConstantArrayValue>,
) {
    if is_uninitialized_null_coalesce_assignment(name, value, locals) {
        return;
    }
    let kind = if locals.get(name) == Some(&LocalKind::Mixed)
        || mixed_return_method_for_assignment(value, function_return_kinds)
    {
        LocalKind::Mixed
    } else if matches!(
        &value.kind,
        ExprKind::ConstRef(const_name) if array_constants.contains_key(const_name.as_str())
    ) {
        LocalKind::Array
    } else if let Some(kind) = array_constant_offset_local_kind(
        value,
        locals,
        function_return_kinds,
        constants,
        class_constants,
        array_constants,
    ) {
        kind
    } else {
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
            .unwrap_or_else(|| {
                infer_assignment_fallback_local_kind(
                    value,
                    locals,
                    array_key_values,
                    callable_targets,
                    string_static_values,
                    function_callable_return_targets,
                    function_possible_callable_return_targets,
                    function_possible_static_string_returns,
                    function_return_kinds,
                    object_classes,
                    constants,
                    class_constants,
                )
            })
    };
    locals.insert(
        name.clone(),
        kind,
    );
    if collect_array_constant_assignment_metadata(
        name,
        value,
        array_constants,
        array_value_kinds,
        array_runtime_value_kinds,
        array_nested_values,
        array_key_kinds,
        array_key_values,
    ) {
        array_runtime_nested_values.remove(name);
        php_normalized_key_arrays.remove(name);
    }
    array_runtime_nested_values.remove(name);
    if let Some(target) =
        callable_target_for_locals(
            value,
            callable_targets,
            string_static_values,
            function_return_kinds,
            object_classes,
            constants,
            class_constants,
        )
    {
        callable_targets.insert(name.clone(), target);
    } else {
        callable_targets.remove(name);
    }
    if let Some(value) = static_string_for_assignment_locals(
        value,
        string_static_values,
        function_static_string_returns,
        constants,
    ) {
        string_static_values.insert(name.clone(), value);
    } else {
        string_static_values.remove(name);
    }
    collect_literal_assignment_metadata(
        name,
        value,
        array_value_kinds,
        array_runtime_value_kinds,
        array_nested_values,
        array_key_kinds,
        array_key_values,
    );
    if let ExprKind::Variable(source) = &value.kind {
        if locals.get(source) == Some(&LocalKind::Array) {
            if let Some(kinds) = array_value_kinds.get(source).cloned() {
                array_value_kinds.insert(name.clone(), kinds);
                array_runtime_value_kinds.remove(name);
            } else {
                array_value_kinds.remove(name);
                if let Some(kind) = array_runtime_value_kinds.get(source).copied() {
                    array_runtime_value_kinds.insert(name.clone(), kind);
                } else {
                    array_runtime_value_kinds.remove(name);
                }
            }
            if let Some(metadata) = array_nested_values.get(source).cloned() {
                array_nested_values.insert(name.clone(), metadata);
            } else {
                array_nested_values.remove(name);
            }
            if let Some(metadata) = array_runtime_nested_values.get(source).cloned() {
                array_runtime_nested_values.insert(name.clone(), metadata);
            } else {
                array_runtime_nested_values.remove(name);
            }
            if let Some(kinds) = array_key_kinds.get(source).cloned() {
                array_key_kinds.insert(name.clone(), kinds);
            } else {
                array_key_kinds.remove(name);
            }
            if let Some(values) = array_key_values.get(source).cloned() {
                array_key_values.insert(name.clone(), values);
            } else {
                array_key_values.remove(name);
            }
        }
    }
    if let Some(key) = array_return_metadata_key_for_assignment(value, function_return_kinds) {
        copy_array_return_assignment_metadata(
            name,
            &key,
            array_value_kinds,
            array_runtime_value_kinds,
            array_nested_values,
            array_key_kinds,
            array_key_values,
            function_array_return_value_kinds,
            function_array_return_runtime_value_kinds,
            function_array_return_nested_values,
            function_array_return_key_kinds,
            function_array_return_key_values,
        );
    }
    if let ExprKind::ExprCall { callee, .. } = &value.kind {
        if let Some(key) = callable_expr_array_return_metadata_key_for_assignment(
            callee,
            function_return_kinds,
            function_callable_return_targets,
            function_possible_callable_return_targets,
            function_array_return_value_kinds,
            function_array_return_runtime_value_kinds,
            function_array_return_nested_values,
            function_array_return_key_kinds,
            function_array_return_key_values,
        ) {
            copy_array_return_assignment_metadata(
                name,
                &key,
                array_value_kinds,
                array_runtime_value_kinds,
                array_nested_values,
                array_key_kinds,
                array_key_values,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_array_return_nested_values,
                function_array_return_key_kinds,
                function_array_return_key_values,
            );
        }
    }
    copy_forwarded_array_param_assignment_metadata(
        name,
        value,
        array_value_kinds,
        array_runtime_value_kinds,
        array_nested_values,
        array_key_kinds,
        array_key_values,
        function_array_return_value_kinds,
        function_array_return_nested_values,
        function_array_return_key_kinds,
        function_array_return_key_values,
        function_array_return_param_indices,
    );
    if let Some(kind) =
        array_runtime_value_kind_for_assignment(
            value,
            array_value_kinds,
            array_runtime_value_kinds,
            array_key_kinds,
        )
    {
        array_value_kinds.remove(name);
        array_runtime_value_kinds.insert(name.clone(), kind);
        if kind == ValueCellKind::Array {
            if let Some(metadata) = runtime_nested_value_for_assignment(value, array_runtime_nested_values) {
                array_runtime_nested_values.insert(name.clone(), metadata);
            } else {
                array_runtime_nested_values.remove(name);
            }
        } else {
            array_runtime_nested_values.remove(name);
        }
    }
    if array_keys_mixed_value_kinds_for_assignment(
        value,
        array_key_kinds,
        array_value_kinds,
        array_runtime_value_kinds,
        php_normalized_key_arrays,
        function_array_return_value_kinds,
        function_array_return_key_kinds,
    ) {
        array_value_kinds.remove(name);
        array_runtime_value_kinds.remove(name);
        array_key_kinds.insert(name.clone(), vec![AssocKeyKind::Int]);
        array_key_values.remove(name);
    }
    if let Some(kind) =
        array_map_runtime_value_kind_for_assignment(
            value,
            function_return_kinds,
            callable_targets,
            string_static_values,
            function_possible_static_string_returns,
        )
    {
        array_value_kinds.remove(name);
        array_runtime_value_kinds.insert(name.clone(), kind);
    }
    if let Some(kind) = array_filter_runtime_value_kind_for_assignment(
        value,
        array_runtime_value_kinds,
        function_array_return_runtime_value_kinds,
        function_return_kinds,
        callable_targets,
        string_static_values,
        function_possible_static_string_returns,
    ) {
        array_value_kinds.remove(name);
        array_runtime_value_kinds.insert(name.clone(), kind);
        if kind == ValueCellKind::Array {
            if let Some(metadata) =
                array_filter_runtime_nested_value_for_assignment(value, array_runtime_nested_values)
            {
                array_runtime_nested_values.insert(name.clone(), metadata);
            } else {
                array_runtime_nested_values.remove(name);
            }
        } else {
            array_runtime_nested_values.remove(name);
        }
    }
    if let ExprKind::FunctionCall { name: function_name, args } = &value.kind {
        if function_name.eq_ignore_ascii_case("array_map")
            && matches!(args.first().map(|arg| &arg.kind), Some(ExprKind::Null))
            && args.len() == 2
        {
            array_transform_metadata::copy_array_map_null_metadata(
                name,
                &args[1],
                array_value_kinds,
                array_runtime_value_kinds,
                array_nested_values,
                array_key_kinds,
                array_key_values,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_array_return_nested_values,
                function_array_return_key_kinds,
                function_array_return_key_values,
            );
        } else if function_name.eq_ignore_ascii_case("array_map")
            && matches!(args.first().map(|arg| &arg.kind), Some(ExprKind::Null))
            && args.len() > 2
        {
            if let Some((kinds, nested)) =
                array_transform_metadata::array_map_null_multi_metadata_for_assignment(
                args,
                array_value_kinds,
                function_array_return_value_kinds,
            ) {
                let key_kinds = vec![AssocKeyKind::Int; kinds.len()];
                array_value_kinds.insert(name.clone(), kinds);
                array_runtime_value_kinds.remove(name);
                array_nested_values.insert(name.clone(), nested);
                array_key_kinds.insert(name.clone(), key_kinds);
                array_key_values.remove(name);
            } else {
                array_value_kinds.remove(name);
                array_runtime_value_kinds.insert(name.clone(), ValueCellKind::Array);
                array_key_kinds.remove(name);
                array_key_values.remove(name);
            }
        }
        if function_name.eq_ignore_ascii_case("array_filter") {
            if let Some(keys) =
                array_filter_foreach_key_kinds(
                    args,
                    array_key_kinds,
                    array_value_kinds,
                    array_runtime_value_kinds,
                    function_array_return_key_kinds,
                )
            {
                array_key_kinds.insert(name.clone(), keys);
                array_key_values.remove(name);
            }
            if let Some((keys, values)) = array_filter_method_return_metadata_for_assignment(
                args,
                function_return_kinds,
                function_array_return_key_kinds,
                function_array_return_value_kinds,
            ) {
                array_key_kinds.insert(name.clone(), keys);
                array_key_values.remove(name);
                array_value_kinds.insert(name.clone(), values);
                array_runtime_value_kinds.remove(name);
            }
        }
        if function_name.eq_ignore_ascii_case("array_unique") {
            if let Some((keys, values)) = key_preserving_method_return_metadata_for_assignment(
                args,
                function_return_kinds,
                function_array_return_key_kinds,
                function_array_return_value_kinds,
            ) {
                array_key_kinds.insert(name.clone(), keys);
                array_key_values.remove(name);
                array_value_kinds.insert(name.clone(), values);
                array_runtime_value_kinds.remove(name);
            }
        }
        if matches!(
            function_name.to_ascii_lowercase().as_str(),
            "array_diff" | "array_intersect" | "array_diff_key" | "array_intersect_key"
        ) {
            if let Some((keys, values)) = key_preserving_method_return_metadata_for_assignment(
                args,
                function_return_kinds,
                function_array_return_key_kinds,
                function_array_return_value_kinds,
            ) {
                array_key_kinds.insert(name.clone(), keys);
                array_key_values.remove(name);
                array_value_kinds.insert(name.clone(), values);
                array_runtime_value_kinds.remove(name);
            }
        }
        if function_name.eq_ignore_ascii_case("array_flip") {
            if let Some((keys, values)) = array_flip_method_return_metadata_for_assignment(
                args,
                function_return_kinds,
                function_array_return_key_kinds,
                function_array_return_value_kinds,
            ) {
                array_key_kinds.insert(name.clone(), keys);
                array_key_values.remove(name);
                array_value_kinds.insert(name.clone(), values);
                array_runtime_value_kinds.remove(name);
            }
        }
        if function_name.eq_ignore_ascii_case("array_map") {
            if let Some(keys) =
                array_map_foreach_key_kinds(
                    args,
                    array_key_kinds,
                    array_value_kinds,
                    array_runtime_value_kinds,
                    function_array_return_value_kinds,
                    function_array_return_key_kinds,
                )
            {
                array_key_kinds.insert(name.clone(), keys);
                array_key_values.remove(name);
            }
        }
        if function_name.eq_ignore_ascii_case("array_pad") {
            if let Some(keys) = array_pad_key_kinds_for_assignment(
                args,
                array_key_kinds,
                function_return_kinds,
                function_array_return_key_kinds,
            ) {
                array_key_kinds.insert(name.clone(), keys);
                array_key_values.remove(name);
            }
        }
        if function_name.eq_ignore_ascii_case("array_rand") && args.len() == 2 {
            if let Some(key_kinds) = array_rand_full_count_key_value_kinds(
                &args[0],
                &args[1],
                array_key_kinds,
                function_return_kinds,
                function_array_return_value_kinds,
                function_array_return_key_kinds,
                string_static_values,
            )
            {
                array_key_kinds.insert(name.clone(), vec![AssocKeyKind::Int; key_kinds.len()]);
                array_key_values.remove(name);
                if key_kinds.iter().all(|kind| *kind == AssocKeyKind::Int) {
                    array_value_kinds.remove(name);
                } else {
                    array_value_kinds.insert(
                        name.clone(),
                        key_kinds
                            .iter()
                            .map(|kind| match kind {
                                AssocKeyKind::Int => ValueCellKind::Int,
                                AssocKeyKind::Str => ValueCellKind::Str,
                            })
                            .collect(),
                    );
                }
                array_runtime_value_kinds.remove(name);
            } else if let Some(len) = array_rand_full_count_indexed_len(
                &args[0],
                &args[1],
                function_return_kinds,
                function_array_return_value_kinds,
                array_key_kinds,
                string_static_values,
            ) {
                array_key_kinds.insert(name.clone(), vec![AssocKeyKind::Int; len]);
                array_key_values.remove(name);
                array_value_kinds.remove(name);
                array_runtime_value_kinds.remove(name);
            } else if array_rand_dynamic_indexed_full_count(&args[0], &args[1], locals) {
                array_key_kinds.insert(name.clone(), vec![AssocKeyKind::Int]);
                array_key_values.remove(name);
                array_value_kinds.remove(name);
                array_runtime_value_kinds.remove(name);
            }
        }
    }
    if let Some(metadata) =
        array_access_metadata::array_access_metadata_for_locals(
            value,
            array_nested_values,
            array_key_values,
        )
    {
        if let Some(kinds) = metadata.value_kinds.clone() {
            array_value_kinds.insert(name.clone(), kinds);
        } else {
            array_value_kinds.remove(name);
        }
        if let Some(nested_values) = metadata.nested_values.clone() {
            array_nested_values.insert(name.clone(), nested_values);
        } else {
            array_nested_values.remove(name);
        }
        if let Some(key_values) = metadata.key_values.clone() {
            array_key_kinds.insert(
                name.clone(),
                key_values.iter().map(assoc_key_kind_for_value).collect(),
            );
            array_key_values.insert(name.clone(), key_values);
        } else {
            array_key_kinds.remove(name);
            array_key_values.remove(name);
        }
    }
    if let Some(kinds) =
        array_transform_metadata::set_op_value_kinds_for_assignment(value, array_value_kinds)
    {
        array_value_kinds.insert(name.clone(), kinds);
        array_runtime_value_kinds.remove(name);
    }
    if let Some(kinds) = array_transform_metadata::array_merge_value_kinds_for_assignment(
        value,
        array_value_kinds,
        array_runtime_value_kinds,
        string_static_values,
        function_array_return_value_kinds,
        function_array_return_runtime_value_kinds,
    )
    {
        array_value_kinds.insert(name.clone(), kinds);
        array_runtime_value_kinds.remove(name);
    }
    if !array_key_values.contains_key(name) {
        if let Some(kinds) = array_transform_metadata::array_merge_key_kinds_for_assignment(
            value,
            array_key_kinds,
            array_value_kinds,
            array_runtime_value_kinds,
            php_normalized_key_arrays,
            string_static_values,
            function_array_return_key_kinds,
        ) {
            array_key_kinds.insert(name.clone(), kinds);
            array_key_values.remove(name);
        }
    }
    if let Some(kinds) =
        array_transform_metadata::key_set_value_kinds_for_assignment(value, array_value_kinds)
    {
        array_value_kinds.insert(name.clone(), kinds);
        array_runtime_value_kinds.remove(name);
    }
    if let Some((keys, values)) = array_unique_static_literal_metadata_for_assignment(value) {
        array_value_kinds.insert(name.clone(), values);
        array_runtime_value_kinds.remove(name);
        array_nested_values.remove(name);
        array_key_kinds.insert(name.clone(), keys);
        array_key_values.remove(name);
    }
    if let Some((keys, values)) =
        array_unique_source_metadata_for_assignment(
            value,
            array_value_kinds,
            array_key_kinds,
            array_constants,
        )
    {
        array_value_kinds.insert(name.clone(), values);
        array_runtime_value_kinds.remove(name);
        array_nested_values.remove(name);
        array_key_kinds.insert(name.clone(), keys);
        array_key_values.remove(name);
    }
    if let Some(nested_values) =
        array_metadata::array_merge_nested_values_for_assignment(value, array_nested_values)
    {
        array_nested_values.insert(name.clone(), nested_values);
    }
    if let Some(nested_values) = array_metadata::array_chunk_nested_values_for_assignment(
        value,
        array_nested_values,
        array_value_kinds,
        array_key_values,
        function_array_return_value_kinds,
        function_array_return_key_values,
    ) {
        let len = nested_values.len();
        array_value_kinds.insert(name.clone(), vec![ValueCellKind::Array; len]);
        array_runtime_value_kinds.remove(name);
        array_nested_values.insert(name.clone(), nested_values);
        array_key_kinds.insert(name.clone(), vec![AssocKeyKind::Int; len]);
        array_key_values.remove(name);
        array_runtime_nested_values.remove(name);
    } else if let Some(metadata) = array_metadata::array_chunk_runtime_nested_value_for_assignment(
        value,
        array_value_kinds,
        array_runtime_value_kinds,
        function_array_return_value_kinds,
        function_array_return_runtime_value_kinds,
    ) {
        array_value_kinds.remove(name);
        array_runtime_value_kinds.insert(name.clone(), ValueCellKind::Array);
        array_nested_values.remove(name);
        array_runtime_nested_values.insert(name.clone(), metadata);
        array_key_kinds.insert(name.clone(), vec![AssocKeyKind::Int]);
        array_key_values.remove(name);
    }
    if let Some(kind) =
        array_transform_metadata::set_op_runtime_value_kind_for_assignment(
            value,
            array_runtime_value_kinds,
        )
    {
        array_value_kinds.remove(name);
        array_runtime_value_kinds.insert(name.clone(), kind);
        array_nested_values.remove(name);
        array_key_kinds.remove(name);
        array_key_values.remove(name);
    }
    if let Some(kinds) =
        array_filter_metadata::default_value_kinds_for_assignment(value, array_value_kinds)
    {
        let filter_keys = array_filter_metadata::default_key_values_for_assignment(
            value,
            array_nested_values,
            array_key_values,
        );
        array_value_kinds.insert(name.clone(), kinds);
        array_runtime_value_kinds.remove(name);
        if let Some(nested_values) =
            array_filter_metadata::default_nested_values_for_assignment(value, array_nested_values)
        {
            array_nested_values.insert(name.clone(), nested_values);
        } else {
            array_nested_values.remove(name);
        }
        if let Some(keys) = filter_keys {
            array_key_kinds.insert(
                name.clone(),
                keys.iter().map(assoc_key_kind_for_value).collect(),
            );
            array_key_values.insert(name.clone(), keys);
        } else {
            array_key_kinds.remove(name);
            array_key_values.remove(name);
        }
    }
    if let Some(kinds) =
        array_filter_metadata::callback_value_kinds_for_assignment(value, array_value_kinds)
    {
        let keys = vec![AssocKeyKind::Int; kinds.len()];
        array_value_kinds.insert(name.clone(), kinds);
        array_runtime_value_kinds.remove(name);
        array_nested_values.remove(name);
        array_key_kinds.insert(name.clone(), keys);
        array_key_values.remove(name);
    }
    if let Some((keys, values)) =
        array_filter_metadata::default_assoc_metadata_for_assignment(value)
    {
        array_value_kinds.insert(name.clone(), values);
        array_runtime_value_kinds.remove(name);
        if let Some(nested_values) =
            array_filter_metadata::default_nested_values_for_assignment(value, array_nested_values)
        {
            array_nested_values.insert(name.clone(), nested_values);
        } else {
            array_nested_values.remove(name);
        }
        array_key_kinds.insert(
            name.clone(),
            keys.iter().map(assoc_key_kind_for_value).collect(),
        );
        array_key_values.insert(name.clone(), keys);
    }
    if let Some((keys, values)) =
        array_filter_metadata::callback_assoc_metadata_for_assignment(
            value,
            array_value_kinds,
            array_key_kinds,
        )
    {
        array_value_kinds.insert(name.clone(), values);
        array_runtime_value_kinds.remove(name);
        array_nested_values.remove(name);
        array_key_kinds.insert(name.clone(), keys);
        array_key_values.remove(name);
    }
    if let Some((keys, values, nested_values)) =
        array_filter_metadata::callback_exact_nested_metadata_for_assignment(
            value,
            array_value_kinds,
            array_nested_values,
            array_key_values,
            function_array_return_value_kinds,
            function_array_return_nested_values,
            function_array_return_key_values,
            callable_targets,
            string_static_values,
        )
    {
        array_value_kinds.insert(name.clone(), values);
        array_runtime_value_kinds.remove(name);
        array_nested_values.insert(name.clone(), nested_values);
        array_key_kinds.insert(
            name.clone(),
            keys.iter().map(assoc_key_kind_for_value).collect(),
        );
        array_key_values.insert(name.clone(), keys);
    }
    if array_filter_metadata::use_key_mixed_keys_for_assignment(value, array_key_kinds) {
        array_value_kinds.remove(name);
        array_runtime_value_kinds.remove(name);
        array_nested_values.remove(name);
        array_key_values.remove(name);
    }
    if let Some(keys) =
        assoc_builder_key_kinds_for_assignment(value, array_value_kinds, array_runtime_value_kinds)
    {
        array_key_kinds.insert(name.clone(), keys);
        if let Some(values) = assoc_builder_key_values_for_assignment(value) {
            array_key_values.insert(name.clone(), values);
        } else {
            array_key_values.remove(name);
        }
    }
    if array_filter_source_has_unknown_nested_assoc_keys(
        value,
        array_nested_values,
        array_runtime_nested_values,
    ) || key_preserving_reverse_source_has_unknown_nested_assoc_keys(
        value,
        array_nested_values,
        array_runtime_nested_values,
    ) || key_preserving_reverse_source_needs_runtime_keys(
        value,
    ) || key_preserving_slice_source_has_unknown_nested_assoc_keys(
        value,
        array_nested_values,
        array_runtime_nested_values,
    ) || array_unique_source_has_unknown_nested_assoc_keys(
        value,
        array_nested_values,
        array_runtime_nested_values,
    ) || array_merge_source_has_unknown_nested_assoc_keys(
        value,
        array_nested_values,
        array_runtime_nested_values,
    ) || array_pad_source_has_unknown_nested_assoc_keys(
        value,
        array_nested_values,
        array_runtime_nested_values,
    ) || array_value_set_source_has_unknown_nested_assoc_keys(
        value,
        array_nested_values,
        array_runtime_nested_values,
    ) || assoc_match_result_has_runtime_keys(
        value,
        array_key_kinds,
    ) || array_return_assignment_has_runtime_assoc_keys(
        value,
        function_return_kinds,
        function_array_return_layouts,
        function_array_return_key_kinds,
    ) || expr_has_php_normalized_runtime_keys(
        value,
        php_normalized_key_arrays,
        array_value_kinds,
        array_runtime_value_kinds,
    ) || array_filter_unknown_mixed_numeric_result(value, locals) {
        if array_filter_unknown_mixed_numeric_result(value, locals) {
            array_value_kinds.remove(name);
            array_runtime_value_kinds.remove(name);
            array_nested_values.remove(name);
            array_key_kinds.remove(name);
            array_key_values.remove(name);
        }
        php_normalized_key_arrays.insert(name.clone());
    } else {
        php_normalized_key_arrays.remove(name);
    }
    if array_unique_source_has_unknown_values(value, array_value_kinds, array_runtime_value_kinds)
        || array_unique_source_has_unknown_nested_assoc_keys(
            value,
            array_nested_values,
            array_runtime_nested_values,
        )
    {
        array_value_kinds.remove(name);
        array_runtime_value_kinds.remove(name);
        array_nested_values.remove(name);
        array_key_kinds.remove(name);
        array_key_values.remove(name);
        php_normalized_key_arrays.insert(name.clone());
    }
}

fn assoc_match_result_has_runtime_keys(
    value: &Expr,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> bool {
    let ExprKind::Match { arms, default, .. } = &value.kind else {
        return false;
    };
    let values = arms.iter().map(|(_, value)| value).chain(default.as_deref());
    let mut saw_assoc = false;
    for value in values {
        match &value.kind {
            ExprKind::ArrayLiteralAssoc(_) => {
                saw_assoc = true;
            }
            ExprKind::Variable(name) if array_key_kinds.contains_key(name) => {
                saw_assoc = true;
            }
            _ => return false,
        }
    }
    saw_assoc
}

fn array_return_assignment_has_runtime_assoc_keys(
    value: &Expr,
    function_return_kinds: &HashMap<String, ValueKind>,
    function_array_return_layouts: &HashMap<String, ArrayLayout>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> bool {
    let Some(key) = array_return_metadata_key_for_assignment(value, function_return_kinds) else {
        return false;
    };
    function_array_return_layouts.get(&key) == Some(&ArrayLayout::Assoc)
        && !function_array_return_key_kinds.contains_key(&key)
}

fn array_constant_offset_local_kind(
    value: &Expr,
    locals: &HashMap<String, LocalKind>,
    function_return_kinds: &HashMap<String, ValueKind>,
    constants: &HashMap<String, ConstantValue>,
    class_constants: &HashMap<String, ConstantValue>,
    array_constants: &HashMap<String, ConstantArrayValue>,
) -> Option<LocalKind> {
    let ExprKind::ArrayAccess { array, index } = &value.kind else {
        return None;
    };
    let ExprKind::ConstRef(const_name) = &array.kind else {
        return None;
    };
    match array_constants.get(const_name.as_str())? {
        ConstantArrayValue::Indexed(items) => {
            let index = usize::try_from(static_or_const_int_value_for_locals(index)?).ok()?;
            let item = items.get(index)?;
            return Some(array_constant_offset_item_local_kind(
                item,
                locals,
                function_return_kinds,
                constants,
                class_constants,
            ));
        }
        ConstantArrayValue::Assoc(items) => {
            let key = static_assoc_key_value_for_expr(index)?;
            let normalized = normalized_static_assoc_items(items).unwrap_or_else(|| items.clone());
            let (_, item) = normalized.iter().rev().find(|(candidate, _)| {
                static_assoc_key_value_for_expr(candidate)
                    .is_some_and(|candidate| candidate == key)
            })?;
            return Some(array_constant_offset_item_local_kind(
                item,
                locals,
                function_return_kinds,
                constants,
                class_constants,
            ));
        }
    }
}

fn array_constant_offset_item_local_kind(
    item: &Expr,
    locals: &HashMap<String, LocalKind>,
    function_return_kinds: &HashMap<String, ValueKind>,
    constants: &HashMap<String, ConstantValue>,
    class_constants: &HashMap<String, ConstantValue>,
) -> LocalKind {
    static_value_cell_kind_for_expr(item)
        .map(local_kind_for_value_cell)
        .unwrap_or_else(|| {
            infer_local_kind(
                item,
                locals,
                function_return_kinds,
                constants,
                class_constants,
            )
        })
}

fn collect_array_constant_assignment_metadata(
    name: &str,
    value: &Expr,
    array_constants: &HashMap<String, ConstantArrayValue>,
    array_value_kinds: &mut HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &mut HashMap<String, ValueCellKind>,
    array_nested_values: &mut HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_key_kinds: &mut HashMap<String, Vec<AssocKeyKind>>,
    array_key_values: &mut HashMap<String, Vec<AssocKeyValue>>,
) -> bool {
    let ExprKind::ConstRef(const_name) = &value.kind else {
        return false;
    };
    match array_constants.get(const_name.as_str()) {
        Some(ConstantArrayValue::Indexed(items)) => {
            array_runtime_value_kinds.remove(name);
            if let Some(metadata) = static_nested_array_metadata_for_items(items) {
                array_nested_values.insert(name.to_string(), metadata);
            } else {
                array_nested_values.remove(name);
            }
            array_key_kinds.remove(name);
            array_key_values.remove(name);
            let Some(kinds) = static_value_cell_kinds_for_items(items) else {
                array_value_kinds.remove(name);
                return true;
            };
            if kinds.iter().all(|kind| *kind == ValueCellKind::Int) {
                array_value_kinds.remove(name);
            } else {
                array_value_kinds.insert(name.to_string(), kinds);
            }
            true
        }
        Some(ConstantArrayValue::Assoc(items)) => {
            array_runtime_value_kinds.remove(name);
            if let Some(metadata) = static_nested_array_metadata_for_assoc_items(items) {
                array_nested_values.insert(name.to_string(), metadata);
            } else {
                array_nested_values.remove(name);
            }
            if let Some((keys, values)) = array_constant_key_value_kinds(const_name, array_constants)
            {
                array_key_kinds.insert(name.to_string(), keys);
                array_value_kinds.insert(name.to_string(), values);
            } else {
                array_key_kinds.remove(name);
                array_value_kinds.remove(name);
            }
            array_key_values.insert(
                name.to_string(),
                static_assoc_key_values_for_items(items).unwrap_or_default(),
            );
            true
        }
        None => false,
    }
}

fn array_return_metadata_key_for_assignment(
    value: &Expr,
    function_return_kinds: &HashMap<String, ValueKind>,
) -> Option<String> {
    match &value.kind {
        ExprKind::FunctionCall { name, .. } => {
            let key = function_key(name);
            function_return_kinds
                .get(&key)
                .is_some_and(|kind| *kind == ValueKind::Array)
                .then_some(key)
        }
        ExprKind::MethodCall { object, method, .. } => {
            if let ExprKind::NewObject { class_name, .. } = &object.kind {
                let key = method_call_return_key(class_name, method);
                return function_return_kinds
                    .get(&key)
                    .is_some_and(|kind| *kind == ValueKind::Array)
                    .then_some(key);
            }
            unique_unknown_receiver_array_method_key(method, function_return_kinds)
        }
        ExprKind::NullsafeMethodCall { object, method, .. } => {
            if let ExprKind::NewObject { class_name, .. } = &object.kind {
                let key = method_call_return_key(class_name, method);
                return function_return_kinds
                    .get(&key)
                    .is_some_and(|kind| *kind == ValueKind::Array)
                    .then_some(key);
            }
            None
        }
        _ => None,
    }
}

fn callable_expr_array_return_metadata_key_for_assignment(
    callee: &Expr,
    function_return_kinds: &HashMap<String, ValueKind>,
    function_callable_return_targets: &HashMap<String, String>,
    function_possible_callable_return_targets: &HashMap<String, Vec<String>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) -> Option<String> {
    let descriptor_key = callable_return_descriptor_key_for_assignment(callee, function_return_kinds)?;
    let targets = function_possible_callable_return_targets
        .get(&descriptor_key)
        .cloned()
        .or_else(|| {
            function_callable_return_targets
                .get(&descriptor_key)
                .map(|target| vec![target.clone()])
        })?;
    let mut selected = None;
    for target in targets {
        let key = function_key(&target);
        if function_return_kinds.get(&key) != Some(&ValueKind::Array) {
            return None;
        }
        if let Some(existing) = selected.as_deref() {
            if !array_return_metadata_matches(
                existing,
                &key,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_array_return_nested_values,
                function_array_return_key_kinds,
                function_array_return_key_values,
            ) {
                return None;
            }
        } else {
            selected = Some(key);
        }
    }
    selected
}

fn callable_return_descriptor_key_for_assignment(
    callee: &Expr,
    function_return_kinds: &HashMap<String, ValueKind>,
) -> Option<String> {
    match &callee.kind {
        ExprKind::FunctionCall { name, .. } => Some(function_key(name)),
        ExprKind::StaticMethodCall {
            receiver: StaticReceiver::Named(class_name),
            method,
            ..
        } => Some(static_method_call_return_key(class_name.as_str(), method)),
        ExprKind::MethodCall { object, method, .. } => {
            if let ExprKind::NewObject { class_name, .. } = &object.kind {
                Some(method_call_return_key(class_name.as_str(), method))
            } else {
                unique_unknown_receiver_array_method_key(method, function_return_kinds)
            }
        }
        ExprKind::NullsafeMethodCall { object, method, .. } => {
            if let ExprKind::NewObject { class_name, .. } = &object.kind {
                Some(method_call_return_key(class_name.as_str(), method))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn array_return_metadata_matches(
    left: &str,
    right: &str,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) -> bool {
    function_array_return_value_kinds.get(left) == function_array_return_value_kinds.get(right)
        && function_array_return_runtime_value_kinds.get(left)
            == function_array_return_runtime_value_kinds.get(right)
        && function_array_return_nested_values.get(left) == function_array_return_nested_values.get(right)
        && function_array_return_key_kinds.get(left) == function_array_return_key_kinds.get(right)
        && function_array_return_key_values.get(left) == function_array_return_key_values.get(right)
}

fn mixed_return_method_for_assignment(
    value: &Expr,
    function_return_kinds: &HashMap<String, ValueKind>,
) -> bool {
    let ExprKind::MethodCall { object, method, .. } = &value.kind else {
        return false;
    };
    if let ExprKind::NewObject { class_name, .. } = &object.kind {
        return function_return_kinds
            .get(&method_call_return_key(class_name, method))
            .is_some_and(|kind| *kind == ValueKind::Mixed);
    }
    unknown_receiver_method_has_only_kind(method, ValueKind::Mixed, function_return_kinds)
}

fn array_pad_key_kinds_for_assignment(
    args: &[Expr],
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_return_kinds: &HashMap<String, ValueKind>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<Vec<AssocKeyKind>> {
    let source = args.first()?;
    let target_len = static_or_const_int_value_for_locals(args.get(1)?)?;
    let source_kinds = array_pad_source_key_kinds(
        source,
        array_key_kinds,
        function_return_kinds,
        function_array_return_key_kinds,
    )?;
    let target_abs = usize::try_from(target_len.abs()).ok()?;
    let pad_count = target_abs.saturating_sub(source_kinds.len());
    let mut keys = Vec::with_capacity(source_kinds.len() + pad_count);
    if target_len < 0 {
        keys.extend(std::iter::repeat(AssocKeyKind::Int).take(pad_count));
    }
    keys.extend(source_kinds);
    if target_len >= 0 {
        keys.extend(std::iter::repeat(AssocKeyKind::Int).take(pad_count));
    }
    Some(keys)
}

fn array_pad_source_key_kinds(
    source: &Expr,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_return_kinds: &HashMap<String, ValueKind>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<Vec<AssocKeyKind>> {
    match &source.kind {
        ExprKind::ArrayLiteral(items) => Some(vec![AssocKeyKind::Int; items.len()]),
        ExprKind::ArrayLiteralAssoc(items) => static_assoc_key_kinds_for_items(items),
        ExprKind::Variable(name) => array_key_kinds.get(name).cloned(),
        ExprKind::FunctionCall { name, .. } => function_array_return_key_kinds
            .get(&function_key(name))
            .cloned(),
        ExprKind::MethodCall { method, .. } | ExprKind::NullsafeMethodCall { method, .. } => {
            if let Some(key) = array_return_metadata_key_for_assignment(source, function_return_kinds)
            {
                return function_array_return_key_kinds.get(&key).cloned();
            }
            consistent_unknown_receiver_array_method_key_kinds(
                method,
                function_return_kinds,
                function_array_return_key_kinds,
            )
        }
        _ => None,
    }
}

fn consistent_unknown_receiver_array_method_key_kinds(
    method: &str,
    function_return_kinds: &HashMap<String, ValueKind>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<Vec<AssocKeyKind>> {
    let suffix = format!("->{}", function_key(method));
    let mut merged = None;
    for (candidate, kind) in function_return_kinds {
        if *kind != ValueKind::Array || !candidate.ends_with(&suffix) {
            continue;
        }
        let kinds = function_array_return_key_kinds.get(candidate)?;
        if merged.as_ref().is_some_and(|existing| existing != kinds) {
            return None;
        }
        merged = Some(kinds.clone());
    }
    merged
}

fn array_filter_method_return_metadata_for_assignment(
    args: &[Expr],
    function_return_kinds: &HashMap<String, ValueKind>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<(Vec<AssocKeyKind>, Vec<ValueCellKind>)> {
    if args.len() != 1 {
        return None;
    }
    let source = args.first()?;
    let method = match &source.kind {
        ExprKind::MethodCall { method, .. } | ExprKind::NullsafeMethodCall { method, .. } => {
            method
        }
        _ => return None,
    };
    let keys = array_pad_source_key_kinds(
        source,
        &HashMap::new(),
        function_return_kinds,
        function_array_return_key_kinds,
    )?;
    let values = if let Some(key) = array_return_metadata_key_for_assignment(source, function_return_kinds) {
        function_array_return_value_kinds.get(&key).cloned()
    } else {
        consistent_unknown_receiver_array_method_value_kinds(
            method,
            function_return_kinds,
            function_array_return_value_kinds,
        )
    }?;
    Some((keys, values))
}

fn key_preserving_method_return_metadata_for_assignment(
    args: &[Expr],
    function_return_kinds: &HashMap<String, ValueKind>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<(Vec<AssocKeyKind>, Vec<ValueCellKind>)> {
    let source = args.first()?;
    let method = match &source.kind {
        ExprKind::MethodCall { method, .. } | ExprKind::NullsafeMethodCall { method, .. } => {
            method
        }
        _ => return None,
    };
    let keys = array_pad_source_key_kinds(
        source,
        &HashMap::new(),
        function_return_kinds,
        function_array_return_key_kinds,
    )?;
    let values = if let Some(key) = array_return_metadata_key_for_assignment(source, function_return_kinds) {
        function_array_return_value_kinds.get(&key).cloned()
    } else {
        consistent_unknown_receiver_array_method_value_kinds(
            method,
            function_return_kinds,
            function_array_return_value_kinds,
        )
    }?;
    Some((keys, values))
}

fn array_flip_method_return_metadata_for_assignment(
    args: &[Expr],
    function_return_kinds: &HashMap<String, ValueKind>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<(Vec<AssocKeyKind>, Vec<ValueCellKind>)> {
    let source = args.first()?;
    let method = match &source.kind {
        ExprKind::MethodCall { method, .. } | ExprKind::NullsafeMethodCall { method, .. } => {
            method
        }
        _ => return None,
    };
    let source_keys = array_pad_source_key_kinds(
        source,
        &HashMap::new(),
        function_return_kinds,
        function_array_return_key_kinds,
    )?;
    let source_values = if let Some(key) = array_return_metadata_key_for_assignment(source, function_return_kinds) {
        function_array_return_value_kinds.get(&key).cloned()
    } else {
        consistent_unknown_receiver_array_method_value_kinds(
            method,
            function_return_kinds,
            function_array_return_value_kinds,
        )
    }?;
    let mut flipped_keys = Vec::new();
    let mut flipped_values = Vec::new();
    for (source_key, source_value) in source_keys.into_iter().zip(source_values) {
        let Some(flipped_key) = assoc_key_kind_from_value_cell_kind(source_value) else {
            continue;
        };
        flipped_keys.push(flipped_key);
        flipped_values.push(value_cell_kind_from_assoc_key_kind(source_key));
    }
    Some((flipped_keys, flipped_values))
}

fn assoc_key_kind_from_value_cell_kind(kind: ValueCellKind) -> Option<AssocKeyKind> {
    match kind {
        ValueCellKind::Int => Some(AssocKeyKind::Int),
        ValueCellKind::Str => Some(AssocKeyKind::Str),
        _ => None,
    }
}

fn value_cell_kind_from_assoc_key_kind(kind: AssocKeyKind) -> ValueCellKind {
    match kind {
        AssocKeyKind::Int => ValueCellKind::Int,
        AssocKeyKind::Str => ValueCellKind::Str,
    }
}

fn consistent_unknown_receiver_array_method_value_kinds(
    method: &str,
    function_return_kinds: &HashMap<String, ValueKind>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<Vec<ValueCellKind>> {
    let suffix = format!("->{}", function_key(method));
    let mut merged = None;
    for (candidate, kind) in function_return_kinds {
        if *kind != ValueKind::Array || !candidate.ends_with(&suffix) {
            continue;
        }
        let kinds = function_array_return_value_kinds.get(candidate)?;
        if merged.as_ref().is_some_and(|existing| existing != kinds) {
            return None;
        }
        merged = Some(kinds.clone());
    }
    merged
}

fn unique_unknown_receiver_array_method_key(
    method: &str,
    function_return_kinds: &HashMap<String, ValueKind>,
) -> Option<String> {
    let suffix = format!("->{}", function_key(method));
    let mut key = None;
    for (candidate, kind) in function_return_kinds {
        if *kind != ValueKind::Array || !candidate.ends_with(&suffix) {
            continue;
        }
        if key.as_ref().is_some_and(|existing| existing != candidate) {
            return None;
        }
        key = Some(candidate.clone());
    }
    key
}

fn unknown_receiver_method_has_only_kind(
    method: &str,
    expected: ValueKind,
    function_return_kinds: &HashMap<String, ValueKind>,
) -> bool {
    let suffix = format!("->{}", function_key(method));
    let mut found = false;
    for (candidate, kind) in function_return_kinds {
        if !candidate.ends_with(&suffix) {
            continue;
        }
        if *kind != expected {
            return false;
        }
        found = true;
    }
    found
}

fn copy_array_return_assignment_metadata(
    name: &str,
    key: &str,
    array_value_kinds: &mut HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &mut HashMap<String, ValueCellKind>,
    array_nested_values: &mut HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_key_kinds: &mut HashMap<String, Vec<AssocKeyKind>>,
    array_key_values: &mut HashMap<String, Vec<AssocKeyValue>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_runtime_value_kinds: &HashMap<String, ValueCellKind>,
    function_array_return_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) {
    if let Some(kinds) = function_array_return_value_kinds.get(key) {
        array_value_kinds.insert(name.to_string(), kinds.clone());
        array_runtime_value_kinds.remove(name);
    } else if let Some(kind) = function_array_return_runtime_value_kinds.get(key) {
        array_value_kinds.remove(name);
        array_runtime_value_kinds.insert(name.to_string(), *kind);
    } else {
        array_value_kinds.remove(name);
        array_runtime_value_kinds.remove(name);
    }
    if let Some(metadata) = function_array_return_nested_values.get(key) {
        array_nested_values.insert(name.to_string(), metadata.clone());
    } else {
        array_nested_values.remove(name);
    }
    if let Some(kinds) = function_array_return_key_kinds.get(key) {
        array_key_kinds.insert(name.to_string(), kinds.clone());
    } else {
        array_key_kinds.remove(name);
    }
    if let Some(values) = function_array_return_key_values.get(key) {
        array_key_values.insert(name.to_string(), values.clone());
    } else {
        array_key_values.remove(name);
    }
}

fn copy_forwarded_array_param_assignment_metadata(
    name: &str,
    value: &Expr,
    array_value_kinds: &mut HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &mut HashMap<String, ValueCellKind>,
    array_nested_values: &mut HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_key_kinds: &mut HashMap<String, Vec<AssocKeyKind>>,
    array_key_values: &mut HashMap<String, Vec<AssocKeyValue>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    function_array_return_param_indices: &HashMap<String, usize>,
) {
    let ExprKind::FunctionCall { name: function_name, args } = &value.kind else {
        return;
    };
    let key = function_key(function_name);
    let Some(param_index) = function_array_return_param_indices.get(&key).copied() else {
        return;
    };
    let Some(arg) = args.get(param_index) else {
        return;
    };
    if let Some(kinds) = forwarded_array_arg_value_kinds(arg, array_value_kinds, function_array_return_value_kinds) {
        array_value_kinds.insert(name.to_string(), kinds);
        array_runtime_value_kinds.remove(name);
    }
    if let Some(metadata) =
        forwarded_array_arg_nested_values(arg, array_nested_values, function_array_return_nested_values)
    {
        array_nested_values.insert(name.to_string(), metadata);
    }
    if let Some(kinds) = forwarded_array_arg_key_kinds(arg, array_key_kinds, function_array_return_key_kinds) {
        array_key_kinds.insert(name.to_string(), kinds);
    }
    if let Some(values) =
        forwarded_array_arg_key_values(arg, array_key_values, function_array_return_key_values)
    {
        array_key_values.insert(name.to_string(), values);
    }
}

fn forwarded_array_arg_value_kinds(
    arg: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
) -> Option<Vec<ValueCellKind>> {
    match &arg.kind {
        ExprKind::ArrayLiteral(items) => static_value_cell_kinds_for_items(items),
        ExprKind::ArrayLiteralAssoc(items) => static_value_cell_kinds_for_assoc_items(items),
        ExprKind::Variable(name) => array_value_kinds.get(name).cloned(),
        ExprKind::FunctionCall { name, .. } => function_array_return_value_kinds
            .get(&function_key(name))
            .cloned(),
        _ => None,
    }
}

fn forwarded_array_arg_nested_values(
    arg: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    function_array_return_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
) -> Option<Vec<Option<NestedArrayMetadata>>> {
    match &arg.kind {
        ExprKind::ArrayLiteral(items) => static_nested_array_metadata_for_items(items),
        ExprKind::ArrayLiteralAssoc(items) => static_nested_array_metadata_for_assoc_items(items),
        ExprKind::Variable(name) => array_nested_values.get(name).cloned(),
        ExprKind::FunctionCall { name, .. } => function_array_return_nested_values
            .get(&function_key(name))
            .cloned(),
        _ => None,
    }
}

fn forwarded_array_arg_key_kinds(
    arg: &Expr,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
) -> Option<Vec<AssocKeyKind>> {
    match &arg.kind {
        ExprKind::ArrayLiteral(items) => Some(vec![AssocKeyKind::Int; items.len()]),
        ExprKind::ArrayLiteralAssoc(items) => static_assoc_key_kinds_for_items(items),
        ExprKind::Variable(name) => array_key_kinds.get(name).cloned(),
        ExprKind::FunctionCall { name, .. } => function_array_return_key_kinds
            .get(&function_key(name))
            .cloned(),
        _ => None,
    }
}

fn forwarded_array_arg_key_values(
    arg: &Expr,
    array_key_values: &HashMap<String, Vec<AssocKeyValue>>,
    function_array_return_key_values: &HashMap<String, Vec<AssocKeyValue>>,
) -> Option<Vec<AssocKeyValue>> {
    match &arg.kind {
        ExprKind::ArrayLiteralAssoc(items) => static_assoc_key_values_for_items(items),
        ExprKind::Variable(name) => array_key_values.get(name).cloned(),
        ExprKind::FunctionCall { name, .. } => function_array_return_key_values
            .get(&function_key(name))
            .cloned(),
        _ => None,
    }
}

fn array_filter_unknown_mixed_numeric_result(
    value: &Expr,
    locals: &HashMap<String, LocalKind>,
) -> bool {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return false;
    };
    if !name.eq_ignore_ascii_case("array_filter") || args.len() != 2 {
        return false;
    }
    let Some(callback) = static_callback_name_for_assignment_locals(
        &args[1],
        &HashMap::new(),
        &HashMap::new(),
    ) else {
        return false;
    };
    if !callback.eq_ignore_ascii_case("is_numeric") {
        return false;
    }
    matches!(
        &args[0].kind,
        ExprKind::Variable(source) if locals.get(source) == Some(&LocalKind::Mixed)
    )
}

fn array_unique_source_has_unknown_values(
    value: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_runtime_value_kinds: &HashMap<String, ValueCellKind>,
) -> bool {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return false;
    };
    if !name.eq_ignore_ascii_case("array_unique") {
        return false;
    }
    match args.first().map(|arg| &arg.kind) {
        Some(ExprKind::Variable(source)) => {
            !array_value_kinds.contains_key(source) && !array_runtime_value_kinds.contains_key(source)
        }
        Some(ExprKind::FunctionCall { .. }) => true,
        _ => false,
    }
}

fn array_unique_static_literal_metadata_for_assignment(
    value: &Expr,
) -> Option<(Vec<AssocKeyKind>, Vec<ValueCellKind>)> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_unique")
        || !args.get(1).is_some_and(array_unique_static_literal_metadata_arg)
    {
        return None;
    }
    match args.first().map(|arg| &arg.kind)? {
        ExprKind::ArrayLiteral(items) => Some((
            vec![AssocKeyKind::Int; items.len()],
            static_value_cell_kinds_for_items(items)?,
        )),
        ExprKind::ArrayLiteralAssoc(items) => Some((
            static_assoc_key_kinds_for_items(items)?,
            static_value_cell_kinds_for_assoc_items(items)?,
        )),
        _ => None,
    }
}

fn array_unique_static_literal_metadata_arg(arg: &Expr) -> bool {
    match &arg.kind {
        ExprKind::IntLiteral(value) => matches!(*value, 0 | 1 | 2 | 3 | 5),
        ExprKind::ConstRef(name) => matches!(
            name.as_str(),
            "SORT_REGULAR" | "SORT_NUMERIC" | "SORT_STRING" | "SORT_LOCALE_STRING"
        ),
        _ => false,
    }
}

fn array_unique_source_metadata_for_assignment(
    value: &Expr,
    array_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    array_constants: &HashMap<String, ConstantArrayValue>,
) -> Option<(Vec<AssocKeyKind>, Vec<ValueCellKind>)> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_unique") {
        return None;
    }
    let (keys, values) = match &args.first()?.kind {
        ExprKind::Variable(source) => {
            let values = array_value_kinds.get(source)?.clone();
            let keys = array_key_kinds
                .get(source)
                .cloned()
                .unwrap_or_else(|| vec![AssocKeyKind::Int; values.len()]);
            (keys, values)
        }
        ExprKind::ConstRef(source) => array_constant_key_value_kinds(source, array_constants)?,
        _ => return None,
    };
    Some((keys, values))
}

fn array_constant_key_value_kinds(
    source: &str,
    array_constants: &HashMap<String, ConstantArrayValue>,
) -> Option<(Vec<AssocKeyKind>, Vec<ValueCellKind>)> {
    match array_constants.get(source)? {
        ConstantArrayValue::Indexed(items) => Some((
            vec![AssocKeyKind::Int; items.len()],
            static_value_cell_kinds_for_items(items)?,
        )),
        ConstantArrayValue::Assoc(items) => Some((
            static_assoc_key_kinds_for_items(items)?,
            static_value_cell_kinds_for_assoc_items(items)?,
        )),
    }
}

fn array_filter_source_has_unknown_nested_assoc_keys(
    value: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_runtime_nested_values: &HashMap<String, NestedArrayMetadata>,
) -> bool {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return false;
    };
    if !name.eq_ignore_ascii_case("array_filter") {
        return false;
    }
    args.first().is_some_and(|source| {
        array_access_has_unknown_nested_assoc_keys(
            source,
            array_nested_values,
            array_runtime_nested_values,
        )
    })
}

fn array_unique_source_has_unknown_nested_assoc_keys(
    value: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_runtime_nested_values: &HashMap<String, NestedArrayMetadata>,
) -> bool {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return false;
    };
    if !name.eq_ignore_ascii_case("array_unique") {
        return false;
    }
    args.first().is_some_and(|source| {
        array_access_has_unknown_nested_assoc_keys(
            source,
            array_nested_values,
            array_runtime_nested_values,
        )
    })
}

fn array_merge_source_has_unknown_nested_assoc_keys(
    value: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_runtime_nested_values: &HashMap<String, NestedArrayMetadata>,
) -> bool {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return false;
    };
    if !name.eq_ignore_ascii_case("array_merge") {
        return false;
    }
    args.iter().any(|source| {
        array_access_has_unknown_nested_assoc_keys(
            source,
            array_nested_values,
            array_runtime_nested_values,
        )
    })
}

fn array_pad_source_has_unknown_nested_assoc_keys(
    value: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_runtime_nested_values: &HashMap<String, NestedArrayMetadata>,
) -> bool {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return false;
    };
    if !name.eq_ignore_ascii_case("array_pad") {
        return false;
    }
    args.first().is_some_and(|source| {
        array_access_has_unknown_nested_assoc_keys(
            source,
            array_nested_values,
            array_runtime_nested_values,
        )
    })
}

fn array_value_set_source_has_unknown_nested_assoc_keys(
    value: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_runtime_nested_values: &HashMap<String, NestedArrayMetadata>,
) -> bool {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return false;
    };
    if !matches!(name.to_ascii_lowercase().as_str(), "array_diff" | "array_intersect") {
        return false;
    }
    args.first().is_some_and(|source| {
        array_access_has_unknown_nested_assoc_keys(
            source,
            array_nested_values,
            array_runtime_nested_values,
        )
    })
}

fn key_preserving_reverse_source_has_unknown_nested_assoc_keys(
    value: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_runtime_nested_values: &HashMap<String, NestedArrayMetadata>,
) -> bool {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return false;
    };
    if !name.eq_ignore_ascii_case("array_reverse") || !array_reverse_preserves_keys_for_locals(args) {
        return false;
    }
    args.first().is_some_and(|source| {
        array_access_has_unknown_nested_assoc_keys(
            source,
            array_nested_values,
            array_runtime_nested_values,
        )
    })
}

fn key_preserving_reverse_source_needs_runtime_keys(value: &Expr) -> bool {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return false;
    };
    if !name.eq_ignore_ascii_case("array_reverse") || !array_reverse_preserves_keys_for_locals(args) {
        return false;
    }
    args.first().is_some_and(|source| {
        matches!(
            source.kind,
            ExprKind::MethodCall { .. }
                | ExprKind::NullsafeMethodCall { .. }
                | ExprKind::PropertyAccess { .. }
                | ExprKind::NullsafePropertyAccess { .. }
                | ExprKind::DynamicPropertyAccess { .. }
                | ExprKind::NullsafeDynamicPropertyAccess { .. }
        )
    })
}

fn key_preserving_slice_source_has_unknown_nested_assoc_keys(
    value: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_runtime_nested_values: &HashMap<String, NestedArrayMetadata>,
) -> bool {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return false;
    };
    if !name.eq_ignore_ascii_case("array_slice") || !array_slice_preserves_keys_for_locals(args) {
        return false;
    }
    args.first().is_some_and(|source| {
        array_access_has_unknown_nested_assoc_keys(
            source,
            array_nested_values,
            array_runtime_nested_values,
        )
    })
}

fn array_reverse_preserves_keys_for_locals(args: &[Expr]) -> bool {
    matches!(
        args.get(1).map(|arg| &arg.kind),
        Some(ExprKind::BoolLiteral(true))
    )
}

fn array_slice_preserves_keys_for_locals(args: &[Expr]) -> bool {
    matches!(
        args.get(3).map(|arg| &arg.kind),
        Some(ExprKind::BoolLiteral(true))
    )
}

fn array_access_has_unknown_nested_assoc_keys(
    source: &Expr,
    array_nested_values: &HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_runtime_nested_values: &HashMap<String, NestedArrayMetadata>,
) -> bool {
    let ExprKind::ArrayAccess { array, index } = &source.kind else {
        return false;
    };
    let ExprKind::Variable(name) = &array.kind else {
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

fn array_filter_runtime_nested_value_for_assignment(
    value: &Expr,
    array_runtime_nested_values: &HashMap<String, NestedArrayMetadata>,
) -> Option<NestedArrayMetadata> {
    let ExprKind::FunctionCall { name, args } = &value.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_filter") || args.is_empty() {
        return None;
    }
    let ExprKind::Variable(source) = &args[0].kind else {
        return None;
    };
    array_runtime_nested_values.get(source).cloned()
}

fn runtime_nested_value_for_assignment(
    value: &Expr,
    array_runtime_nested_values: &HashMap<String, NestedArrayMetadata>,
) -> Option<NestedArrayMetadata> {
    match &value.kind {
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_fill_keys") => {
            args.get(1).and_then(static_nested_array_metadata_for_expr)
        }
        ExprKind::Variable(source) => array_runtime_nested_values.get(source).cloned(),
        _ => None,
    }
}

fn array_rand_full_count_key_value_kinds(
    source: &Expr,
    count: &Expr,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_return_kinds: &HashMap<String, ValueKind>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    string_static_values: &HashMap<String, String>,
) -> Option<Vec<AssocKeyKind>> {
    let key_kinds = match &source.kind {
        ExprKind::ArrayLiteralAssoc(items) => static_assoc_key_kinds_for_items(items),
        ExprKind::Variable(name) => array_key_kinds.get(name).cloned(),
        ExprKind::StaticMethodCall {
            receiver: StaticReceiver::Named(class_name),
            method,
            ..
        } => array_key_kinds
            .get(&static_method_call_return_key(class_name.as_str(), method))
            .cloned(),
        ExprKind::DynamicStaticMethodCall {
            receiver: StaticReceiver::Named(class_name),
            method,
            ..
        } => dynamic_static_method_return_key(class_name.as_str(), method, string_static_values)
            .and_then(|key| array_key_kinds.get(&key).cloned()),
        ExprKind::MethodCall { method, .. } | ExprKind::NullsafeMethodCall { method, .. } => {
            consistent_unknown_receiver_array_method_key_kinds(
                method,
                function_return_kinds,
                function_array_return_key_kinds,
            )
        }
        _ => None,
    }?;
    let count_value = array_rand_full_count_metadata_value(
        source,
        count,
        array_key_kinds,
        function_array_return_value_kinds,
        function_array_return_key_kinds,
        string_static_values,
    )?;
    (count_value == key_kinds.len()).then_some(key_kinds)
}

fn array_rand_full_count_indexed_len(
    source: &Expr,
    count: &Expr,
    function_return_kinds: &HashMap<String, ValueKind>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    string_static_values: &HashMap<String, String>,
) -> Option<usize> {
    let len = match &source.kind {
        ExprKind::StaticMethodCall {
            receiver: StaticReceiver::Named(class_name),
            method,
            ..
        } => {
            let key = static_method_call_return_key(class_name.as_str(), method);
            if array_key_kinds.contains_key(&key) {
                return None;
            }
            function_array_return_value_kinds.get(&key).map(Vec::len)?
        }
        ExprKind::DynamicStaticMethodCall {
            receiver: StaticReceiver::Named(class_name),
            method,
            ..
        } => {
            let key = dynamic_static_method_return_key(class_name.as_str(), method, string_static_values)?;
            if array_key_kinds.contains_key(&key) {
                return None;
            }
            function_array_return_value_kinds.get(&key).map(Vec::len)?
        }
        ExprKind::MethodCall { method, .. } | ExprKind::NullsafeMethodCall { method, .. } => {
            if consistent_unknown_receiver_array_method_key_kinds(
                method,
                function_return_kinds,
                &HashMap::new(),
            )
            .is_some()
            {
                return None;
            }
            consistent_unknown_receiver_array_method_value_kinds(
                method,
                function_return_kinds,
                function_array_return_value_kinds,
            )
            .map(|kinds| kinds.len())?
        }
        _ => return None,
    };
    let count_value = array_rand_full_count_metadata_value(
        source,
        count,
        array_key_kinds,
        function_array_return_value_kinds,
        &HashMap::new(),
        string_static_values,
    )?;
    (count_value == len).then_some(len)
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

fn array_rand_full_count_metadata_value(
    source: &Expr,
    count: &Expr,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    string_static_values: &HashMap<String, String>,
) -> Option<usize> {
    static_count_value(count).or_else(|| {
        known_count_call_value(
            source,
            count,
            array_key_kinds,
            function_array_return_value_kinds,
            function_array_return_key_kinds,
            string_static_values,
        )
    })
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
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    string_static_values: &HashMap<String, String>,
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
        function_array_return_key_kinds,
        string_static_values,
    )
        .or_else(|| {
            known_array_len_for_metadata(
                source,
                array_key_kinds,
                function_array_return_value_kinds,
                function_array_return_key_kinds,
                string_static_values,
            )
        })
}

fn known_array_len_for_metadata(
    expr: &Expr,
    array_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_value_kinds: &HashMap<String, Vec<ValueCellKind>>,
    function_array_return_key_kinds: &HashMap<String, Vec<AssocKeyKind>>,
    string_static_values: &HashMap<String, String>,
) -> Option<usize> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => Some(items.len()),
        ExprKind::ArrayLiteralAssoc(items) => Some(items.len()),
        ExprKind::Variable(name) => array_key_kinds.get(name).map(Vec::len),
        ExprKind::StaticMethodCall {
            receiver: StaticReceiver::Named(class_name),
            method,
            ..
        } => {
            let key = static_method_call_return_key(class_name.as_str(), method);
            array_key_kinds
                .get(&key)
                .map(Vec::len)
                .or_else(|| function_array_return_value_kinds.get(&key).map(Vec::len))
        }
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
                .or_else(|| method_return_value_kinds(method, function_array_return_value_kinds).map(|kinds| kinds.len()))
        }
        _ => None,
    }
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
