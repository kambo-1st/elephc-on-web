//! Purpose:
//! Infers wasm32-web local kinds and collects locals from PHP statements.
//! Keeps local layout metadata separate from module state and WAT emission.
//!
//! Called from:
//! - `crate::codegen::wasm::module::WasmModule::new`.
//! - Function and expression metadata helpers that need local kind classification.
//!
//! Key details:
//! - Local kind inference is conservative; unknown or unsupported aggregate shapes stay broad.

use super::*;

mod assoc_builder_keys;
mod array_access_values;
mod array_column_values;
mod array_callback_keys;
mod array_map_values;
mod dynamic_calls;
mod array_flip_keys;
mod array_merge_keys;
mod array_merge_values;
mod array_mutation_collection;
mod array_values_metadata;
mod assignment_collection;
mod assignment_literal_collection;
mod assignment_metadata;
mod callback_values;
mod direct_foreach_values;
mod expression_inference;
mod foreach_key_metadata;
mod foreach_value_metadata;
mod foreach_value_kinds;
mod foreach_value_sources;
mod key_preserving_transform_keys;
mod local_kind_inference;
mod pathinfo_metadata;
mod runtime_value_metadata;
mod stmt_body_collection;
mod type_kinds;

pub(super) use self::assoc_builder_keys::{
    assoc_key_kind_for_value, direct_assoc_builder_key_kinds_for_foreach,
};
use self::dynamic_calls::{call_user_func_array_local_kind, call_user_func_local_kind};
use self::assoc_builder_keys::{
    assoc_builder_key_kinds_for_assignment, assoc_builder_key_values_for_assignment,
    direct_assoc_builder_key_kinds, direct_assoc_builder_php_normalized_key_kinds_for_foreach,
    expr_has_marked_php_normalized_runtime_keys, expr_has_php_normalized_runtime_keys,
};
use self::array_access_values::{assoc_array_access_value_kind, nested_array_access_value_kind};
use self::array_column_values::array_column_value_kinds;
use self::array_flip_keys::{
    array_flip_foreach_key_kinds, array_flip_foreach_key_local_kind, array_flip_value_kinds,
};
use self::array_map_values::array_map_foreach_value_local_kind;
pub(super) use self::array_merge_keys::array_merge_foreach_key_local_kind;
use self::array_merge_values::array_merge_foreach_value_local_kind;
use self::array_mutation_collection::{collect_array_assign_locals, collect_array_push_locals};
use self::array_values_metadata::array_values_foreach_value_local_kind;
use self::assignment_collection::collect_assignment_locals;
use self::assignment_metadata::infer_assignment_local_kind;
pub(in crate::codegen::wasm::module) use self::array_callback_keys::array_filter_foreach_key_kinds;
use self::array_callback_keys::array_map_foreach_key_kinds;
pub(super) use self::callback_values::{
    array_map_callback_expr_return_kind, array_map_callback_return_kind,
    static_callback_name_for_assignment_locals, static_callback_name_for_locals,
};
use self::callback_values::{callable_target_for_locals, static_string_for_assignment_locals};
use self::direct_foreach_values::{
    array_fill_foreach_value_local_kind, array_keys_foreach_value_local_kind,
    array_pad_foreach_value_local_kind, range_foreach_value_local_kind,
};
use self::expression_inference::{
    infer_assignment_fallback_local_kind, infer_branch_local_kind, infer_many_local_kind,
};
use self::foreach_key_metadata::{
    dynamic_static_method_call_return_key, foreach_key_local_kind,
};
use self::foreach_value_metadata::foreach_value_local_kind;
use self::foreach_value_kinds::{assoc_foreach_value_local_kind, foreach_value_cell_local_kind};
pub(super) use self::foreach_value_sources::value_kinds_for_foreach_source;
use self::foreach_value_sources::{
    array_filter_array_map_null_value_kinds_for_foreach, array_map_null_row_kinds_for_foreach,
    array_return_value_kinds_for_foreach_source, runtime_value_kind_for_array_arg,
    runtime_value_kind_for_first_arg,
};
use self::key_preserving_transform_keys::key_preserving_transform_foreach_key_kinds;
pub(super) use self::local_kind_inference::{
    infer_local_kind, unknown_receiver_method_local_kind,
};
use self::pathinfo_metadata::{pathinfo_call_returns_array, pathinfo_direct_array_access_value_kind};
use self::runtime_value_metadata::{
    array_filter_array_map_runtime_value_kind_for_foreach,
    array_filter_runtime_value_kind_for_assignment, array_keys_mixed_value_kinds_for_assignment,
    array_map_runtime_value_kind_for_assignment, array_runtime_value_kind_for_assignment,
};
use self::stmt_body_collection::{
    collect_for_locals, collect_if_locals, collect_stmt_body_locals, collect_switch_locals,
};
pub(super) use self::type_kinds::{
    local_kind_for_constant, local_kind_for_value, local_kind_for_value_cell, local_kind_from_type,
    value_cell_kind_from_type, value_kind_from_return_type,
};
pub(in crate::codegen::wasm) use self::type_kinds::value_kind_for_local;

pub(super) fn collect_stmt_locals(
    stmt: &Stmt,
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
    trait_use_names: &HashMap<String, Vec<String>>,
    constants: &HashMap<String, ConstantValue>,
    class_constants: &HashMap<String, ConstantValue>,
    array_constants: &HashMap<String, ConstantArrayValue>,
) {
    match &stmt.kind {
        StmtKind::Assign { name, value } => {
            collect_expr_assignment_prelude_locals(
                value,
                locals,
                array_value_kinds,
                array_runtime_value_kinds,
                array_nested_values,
                array_runtime_nested_values,
                array_key_kinds,
                array_key_values,
                php_normalized_key_arrays,
                callable_targets,
                string_static_values,
                function_return_kinds,
                function_callable_return_targets,
                function_possible_callable_return_targets,
                function_static_string_returns,
                function_possible_static_string_returns,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_array_return_layouts,
                function_array_return_nested_values,
                function_array_return_key_kinds,
                function_array_return_key_values,
                function_array_return_param_indices,
                object_classes,
                trait_use_names,
                constants,
                class_constants,
                array_constants,
            );
            collect_assignment_locals(
                name,
                value,
                locals,
                array_value_kinds,
                array_runtime_value_kinds,
                array_nested_values,
                array_runtime_nested_values,
                array_key_kinds,
                array_key_values,
                php_normalized_key_arrays,
                callable_targets,
                string_static_values,
                function_return_kinds,
                function_callable_return_targets,
                function_possible_callable_return_targets,
                function_static_string_returns,
                function_possible_static_string_returns,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_array_return_layouts,
                function_array_return_nested_values,
                function_array_return_key_kinds,
                function_array_return_key_values,
                function_array_return_param_indices,
                object_classes,
                trait_use_names,
                constants,
                class_constants,
                array_constants,
            );
        }
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        } => {
            collect_expr_assignment_prelude_locals(
                value,
                locals,
                array_value_kinds,
                array_runtime_value_kinds,
                array_nested_values,
                array_runtime_nested_values,
                array_key_kinds,
                array_key_values,
                php_normalized_key_arrays,
                callable_targets,
                string_static_values,
                function_return_kinds,
                function_callable_return_targets,
                function_possible_callable_return_targets,
                function_static_string_returns,
                function_possible_static_string_returns,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_array_return_layouts,
                function_array_return_nested_values,
                function_array_return_key_kinds,
                function_array_return_key_values,
                function_array_return_param_indices,
                object_classes,
                trait_use_names,
                constants,
                class_constants,
                array_constants,
            );
            if is_uninitialized_null_coalesce_assignment(name, value, locals) {
                return;
            }
            locals.insert(name.clone(), local_kind_from_type(Some(type_expr)));
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => collect_if_locals(
            then_body,
            elseif_clauses,
            else_body.as_deref(),
            locals,
            array_value_kinds,
            array_runtime_value_kinds,
            array_nested_values,
            array_runtime_nested_values,
            array_key_kinds,
            array_key_values,
            php_normalized_key_arrays,
            callable_targets,
            string_static_values,
            function_return_kinds,
            function_callable_return_targets,
            function_possible_callable_return_targets,
            function_static_string_returns,
            function_possible_static_string_returns,
            function_array_return_value_kinds,
            function_array_return_runtime_value_kinds,
            function_array_return_layouts,
            function_array_return_nested_values,
            function_array_return_key_kinds,
            function_array_return_key_values,
            function_array_return_param_indices,
            object_classes,
            trait_use_names,
            constants,
            class_constants,
            array_constants,
        ),
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. } => collect_stmt_body_locals(
            body,
            locals,
            array_value_kinds,
            array_runtime_value_kinds,
            array_nested_values,
            array_runtime_nested_values,
            array_key_kinds,
            array_key_values,
            php_normalized_key_arrays,
            callable_targets,
            string_static_values,
            function_return_kinds,
            function_callable_return_targets,
            function_possible_callable_return_targets,
            function_static_string_returns,
            function_possible_static_string_returns,
            function_array_return_value_kinds,
            function_array_return_runtime_value_kinds,
            function_array_return_layouts,
            function_array_return_nested_values,
            function_array_return_key_kinds,
            function_array_return_key_values,
            function_array_return_param_indices,
            object_classes,
            trait_use_names,
            constants,
            class_constants,
            array_constants,
        ),
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
            ..
        } => {
            let unknown_mixed_source = foreach_source_is_unknown_mixed(array, locals);
            if let Some(key_var) = key_var {
                let key_kind = if unknown_mixed_source {
                    LocalKind::Mixed
                } else {
                    foreach_key_local_kind(
                        array,
                        array_nested_values,
                        array_runtime_nested_values,
                        array_key_kinds,
                        array_value_kinds,
                        array_runtime_value_kinds,
                        php_normalized_key_arrays,
                        string_static_values,
                        function_array_return_value_kinds,
                        function_array_return_key_kinds,
                        array_constants,
                    )
                };
                let key_kind = match locals.get(key_var).copied() {
                    Some(existing) if existing != key_kind => LocalKind::Mixed,
                    _ => key_kind,
                };
                locals.insert(key_var.clone(), key_kind);
            }
            let value_kind = if unknown_mixed_source {
                LocalKind::Mixed
            } else {
                foreach_value_local_kind(
                    array,
                    locals,
                    array_value_kinds,
                    array_runtime_value_kinds,
                    array_key_kinds,
                    php_normalized_key_arrays,
                    callable_targets,
                    string_static_values,
                    function_return_kinds,
                    function_possible_static_string_returns,
                    function_array_return_value_kinds,
                    function_array_return_runtime_value_kinds,
                    function_array_return_key_kinds,
                    array_constants,
                )
            };
            let value_kind = match locals.get(value_var).copied() {
                Some(existing) if existing != value_kind => LocalKind::Mixed,
                _ => value_kind,
            };
            locals.insert(value_var.clone(), value_kind);
            if value_kind == LocalKind::Array {
                if let Some(metadata) = array_metadata::foreach_array_value_metadata_for_locals(
                    array,
                    array_nested_values,
                    array_runtime_nested_values,
                    array_value_kinds,
                    array_runtime_value_kinds,
                    array_key_values,
                    function_array_return_value_kinds,
                    function_array_return_runtime_value_kinds,
                    function_array_return_key_values,
                ) {
                    let value_kinds = metadata.value_kinds.clone().or_else(|| {
                        metadata.nested_values.as_ref().and_then(|nested_values| {
                            nested_values
                                .iter()
                                .all(Option::is_some)
                                .then(|| vec![ValueCellKind::Array; nested_values.len()])
                        })
                    });
                    if let Some(value_kinds) = value_kinds {
                        array_value_kinds.insert(value_var.clone(), value_kinds);
                    } else {
                        array_value_kinds.remove(value_var);
                    }
                    if let Some(nested_values) = metadata.nested_values.clone() {
                        array_nested_values.insert(value_var.clone(), nested_values);
                    } else {
                        array_nested_values.remove(value_var);
                    }
                    if let Some(key_values) = metadata.key_values.clone() {
                        array_key_kinds.insert(
                            value_var.clone(),
                            key_values
                                .iter()
                                .map(|key| match key {
                                    AssocKeyValue::Int(_) => AssocKeyKind::Int,
                                    AssocKeyValue::Str(_) => AssocKeyKind::Str,
                                })
                                .collect(),
                        );
                        array_key_values.insert(value_var.clone(), key_values);
                        php_normalized_key_arrays.remove(value_var);
                    } else {
                        array_key_kinds.remove(value_var);
                        array_key_values.remove(value_var);
                        if metadata.layout == ArrayLayout::Assoc {
                            php_normalized_key_arrays.insert(value_var.clone());
                        } else {
                            php_normalized_key_arrays.remove(value_var);
                        }
                    }
                }
            }
            collect_stmt_body_locals(
                body,
                locals,
                array_value_kinds,
                array_runtime_value_kinds,
                array_nested_values,
                array_runtime_nested_values,
                array_key_kinds,
                array_key_values,
                php_normalized_key_arrays,
                callable_targets,
                string_static_values,
                function_return_kinds,
                function_callable_return_targets,
                function_possible_callable_return_targets,
                function_static_string_returns,
                function_possible_static_string_returns,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_array_return_layouts,
                function_array_return_nested_values,
                function_array_return_key_kinds,
                function_array_return_key_values,
                function_array_return_param_indices,
                object_classes,
                trait_use_names,
                constants,
                class_constants,
                array_constants,
            );
        }
        StmtKind::ListUnpack { vars, value } => {
            collect_expr_assignment_prelude_locals(
                value,
                locals,
                array_value_kinds,
                array_runtime_value_kinds,
                array_nested_values,
                array_runtime_nested_values,
                array_key_kinds,
                array_key_values,
                php_normalized_key_arrays,
                callable_targets,
                string_static_values,
                function_return_kinds,
                function_callable_return_targets,
                function_possible_callable_return_targets,
                function_static_string_returns,
                function_possible_static_string_returns,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_array_return_layouts,
                function_array_return_nested_values,
                function_array_return_key_kinds,
                function_array_return_key_values,
                function_array_return_param_indices,
                object_classes,
                trait_use_names,
                constants,
                class_constants,
                array_constants,
            );
            for (index, var) in vars.iter().enumerate() {
                let access = Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(value.clone()),
                        index: Box::new(Expr::int_lit(index as i64)),
                    },
                    value.span,
                );
                collect_assignment_locals(
                    var,
                    &access,
                    locals,
                    array_value_kinds,
                    array_runtime_value_kinds,
                    array_nested_values,
                    array_runtime_nested_values,
                    array_key_kinds,
                    array_key_values,
                    php_normalized_key_arrays,
                    callable_targets,
                    string_static_values,
                    function_return_kinds,
                    function_callable_return_targets,
                    function_possible_callable_return_targets,
                    function_static_string_returns,
                    function_possible_static_string_returns,
                    function_array_return_value_kinds,
                    function_array_return_runtime_value_kinds,
                    function_array_return_layouts,
                    function_array_return_nested_values,
                    function_array_return_key_kinds,
                    function_array_return_key_values,
                    function_array_return_param_indices,
                    object_classes,
                    trait_use_names,
                    constants,
                    class_constants,
                    array_constants,
                );
            }
        }
        StmtKind::For {
            init, update, body, ..
        } => collect_for_locals(
            init.as_deref(),
            body,
            update.as_deref(),
            locals,
            array_value_kinds,
            array_runtime_value_kinds,
            array_nested_values,
            array_runtime_nested_values,
            array_key_kinds,
            array_key_values,
            php_normalized_key_arrays,
            callable_targets,
            string_static_values,
            function_return_kinds,
            function_callable_return_targets,
            function_possible_callable_return_targets,
            function_static_string_returns,
            function_possible_static_string_returns,
            function_array_return_value_kinds,
            function_array_return_runtime_value_kinds,
            function_array_return_layouts,
            function_array_return_nested_values,
            function_array_return_key_kinds,
            function_array_return_key_values,
            function_array_return_param_indices,
            object_classes,
            trait_use_names,
            constants,
            class_constants,
            array_constants,
        ),
        StmtKind::Switch { cases, default, .. } => collect_switch_locals(
            cases,
            default.as_deref(),
            locals,
            array_value_kinds,
            array_runtime_value_kinds,
            array_nested_values,
            array_runtime_nested_values,
            array_key_kinds,
            array_key_values,
            php_normalized_key_arrays,
            callable_targets,
            string_static_values,
            function_return_kinds,
            function_callable_return_targets,
            function_possible_callable_return_targets,
            function_static_string_returns,
            function_possible_static_string_returns,
            function_array_return_value_kinds,
            function_array_return_runtime_value_kinds,
            function_array_return_layouts,
            function_array_return_nested_values,
            function_array_return_key_kinds,
            function_array_return_key_values,
            function_array_return_param_indices,
            object_classes,
            trait_use_names,
            constants,
            class_constants,
            array_constants,
        ),
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => {
            collect_expr_assignment_prelude_locals(
                index,
                locals,
                array_value_kinds,
                array_runtime_value_kinds,
                array_nested_values,
                array_runtime_nested_values,
                array_key_kinds,
                array_key_values,
                php_normalized_key_arrays,
                callable_targets,
                string_static_values,
                function_return_kinds,
                function_callable_return_targets,
                function_possible_callable_return_targets,
                function_static_string_returns,
                function_possible_static_string_returns,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_array_return_layouts,
                function_array_return_nested_values,
                function_array_return_key_kinds,
                function_array_return_key_values,
                function_array_return_param_indices,
                object_classes,
                trait_use_names,
                constants,
                class_constants,
                array_constants,
            );
            collect_expr_assignment_prelude_locals(
                value,
                locals,
                array_value_kinds,
                array_runtime_value_kinds,
                array_nested_values,
                array_runtime_nested_values,
                array_key_kinds,
                array_key_values,
                php_normalized_key_arrays,
                callable_targets,
                string_static_values,
                function_return_kinds,
                function_callable_return_targets,
                function_possible_callable_return_targets,
                function_static_string_returns,
                function_possible_static_string_returns,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_array_return_layouts,
                function_array_return_nested_values,
                function_array_return_key_kinds,
                function_array_return_key_values,
                function_array_return_param_indices,
                object_classes,
                trait_use_names,
                constants,
                class_constants,
                array_constants,
            );
            collect_array_assign_locals(
                array,
                index,
                value,
                array_value_kinds,
                array_nested_values,
                array_key_kinds,
                array_key_values,
            );
        }
        StmtKind::ArrayPush { array, value } => {
            collect_expr_assignment_prelude_locals(
                value,
                locals,
                array_value_kinds,
                array_runtime_value_kinds,
                array_nested_values,
                array_runtime_nested_values,
                array_key_kinds,
                array_key_values,
                php_normalized_key_arrays,
                callable_targets,
                string_static_values,
                function_return_kinds,
                function_callable_return_targets,
                function_possible_callable_return_targets,
                function_static_string_returns,
                function_possible_static_string_returns,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_array_return_layouts,
                function_array_return_nested_values,
                function_array_return_key_kinds,
                function_array_return_key_values,
                function_array_return_param_indices,
                object_classes,
                trait_use_names,
                constants,
                class_constants,
                array_constants,
            );
            collect_array_push_locals(
                array,
                value,
                array_value_kinds,
                array_nested_values,
                array_key_kinds,
                array_key_values,
            );
        }
        StmtKind::Echo(expr) | StmtKind::ExprStmt(expr) | StmtKind::Return(Some(expr)) => collect_expr_assignment_prelude_locals(
            expr,
            locals,
            array_value_kinds,
            array_runtime_value_kinds,
            array_nested_values,
            array_runtime_nested_values,
            array_key_kinds,
            array_key_values,
            php_normalized_key_arrays,
            callable_targets,
            string_static_values,
            function_return_kinds,
            function_callable_return_targets,
            function_possible_callable_return_targets,
            function_static_string_returns,
            function_possible_static_string_returns,
            function_array_return_value_kinds,
            function_array_return_runtime_value_kinds,
            function_array_return_layouts,
            function_array_return_nested_values,
            function_array_return_key_kinds,
            function_array_return_key_values,
            function_array_return_param_indices,
            object_classes,
            trait_use_names,
            constants,
            class_constants,
            array_constants,
        ),
        _ => {}
    }
}

fn collect_expr_assignment_prelude_locals(
    expr: &Expr,
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
    trait_use_names: &HashMap<String, Vec<String>>,
    constants: &HashMap<String, ConstantValue>,
    class_constants: &HashMap<String, ConstantValue>,
    array_constants: &HashMap<String, ConstantArrayValue>,
) {
    macro_rules! collect_child {
        ($child:expr) => {
            collect_expr_assignment_prelude_locals(
                $child,
                locals,
                array_value_kinds,
                array_runtime_value_kinds,
                array_nested_values,
                array_runtime_nested_values,
                array_key_kinds,
                array_key_values,
                php_normalized_key_arrays,
                callable_targets,
                string_static_values,
                function_return_kinds,
                function_callable_return_targets,
                function_possible_callable_return_targets,
                function_static_string_returns,
                function_possible_static_string_returns,
                function_array_return_value_kinds,
                function_array_return_runtime_value_kinds,
                function_array_return_layouts,
                function_array_return_nested_values,
                function_array_return_key_kinds,
                function_array_return_key_values,
                function_array_return_param_indices,
                object_classes,
                trait_use_names,
                constants,
                class_constants,
                array_constants,
            )
        };
    }
    match &expr.kind {
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            for stmt in prelude {
                collect_stmt_locals(
                    stmt,
                    locals,
                    array_value_kinds,
                    array_runtime_value_kinds,
                    array_nested_values,
                    array_runtime_nested_values,
                    array_key_kinds,
                    array_key_values,
                    php_normalized_key_arrays,
                    callable_targets,
                    string_static_values,
                    function_return_kinds,
                    function_callable_return_targets,
                    function_possible_callable_return_targets,
                    function_static_string_returns,
                    function_possible_static_string_returns,
                    function_array_return_value_kinds,
                    function_array_return_runtime_value_kinds,
                    function_array_return_layouts,
                    function_array_return_nested_values,
                    function_array_return_key_kinds,
                    function_array_return_key_values,
                    function_array_return_param_indices,
                    object_classes,
                    trait_use_names,
                    constants,
                    class_constants,
                    array_constants,
                );
            }
            collect_child!(target);
            collect_child!(value);
            if let Some(result_target) = result_target.as_deref() {
                collect_child!(result_target);
            }
        }
        ExprKind::BinaryOp { left, right, .. }
        | ExprKind::NullCoalesce {
            value: left,
            default: right,
        }
        | ExprKind::Pipe {
            value: left,
            callable: right,
        }
        | ExprKind::ArrayAccess {
            array: left,
            index: right,
        } => {
            collect_child!(left);
            collect_child!(right);
        }
        ExprKind::InstanceOf { value, .. }
        | ExprKind::Negate(value)
        | ExprKind::Not(value)
        | ExprKind::BitNot(value)
        | ExprKind::Throw(value)
        | ExprKind::ErrorSuppress(value)
        | ExprKind::Print(value)
        | ExprKind::Cast { expr: value, .. }
        | ExprKind::Spread(value)
        | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::BufferNew { len: value, .. }
        | ExprKind::YieldFrom(value) => collect_child!(value),
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewScopedObject { args, .. }
        | ExprKind::NewDynamic { args, .. }
        | ExprKind::NewDynamicObject { args, .. } => {
            for arg in args {
                collect_child!(arg);
            }
        }
        ExprKind::DynamicStaticMethodCall { method, args, .. } => {
            collect_child!(method);
            for arg in args {
                collect_child!(arg);
            }
        }
        ExprKind::ExprCall { callee, args } => {
            collect_child!(callee);
            for arg in args {
                collect_child!(arg);
            }
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_child!(item);
            }
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                collect_child!(key);
                collect_child!(value);
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_child!(subject);
            for (conditions, result) in arms {
                for condition in conditions {
                    collect_child!(condition);
                }
                collect_child!(result);
            }
            if let Some(default) = default.as_deref() {
                collect_child!(default);
            }
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_child!(condition);
            collect_child!(then_expr);
            collect_child!(else_expr);
        }
        ExprKind::ShortTernary { value, default } => {
            collect_child!(value);
            collect_child!(default);
        }
        ExprKind::NamedArg { value, .. } => collect_child!(value),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            collect_child!(object);
            collect_child!(property);
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => collect_child!(object),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            collect_child!(object);
            for arg in args {
                collect_child!(arg);
            }
        }
        ExprKind::DynamicMethodCall {
            object,
            method,
            args,
        }
        | ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            collect_child!(object);
            collect_child!(method);
            for arg in args {
                collect_child!(arg);
            }
        }
        ExprKind::Closure {
            params,
            body,
            is_arrow,
            ..
        } => {
            for (_, _, default, _) in params {
                if let Some(default) = default {
                    collect_child!(default);
                }
            }
            if *is_arrow {
                collect_stmt_body_locals(
                    body,
                    locals,
                    array_value_kinds,
                    array_runtime_value_kinds,
                    array_nested_values,
                    array_runtime_nested_values,
                    array_key_kinds,
                    array_key_values,
                    php_normalized_key_arrays,
                    callable_targets,
                    string_static_values,
                    function_return_kinds,
                    function_callable_return_targets,
                    function_possible_callable_return_targets,
                    function_static_string_returns,
                    function_possible_static_string_returns,
                    function_array_return_value_kinds,
                    function_array_return_runtime_value_kinds,
                    function_array_return_layouts,
                    function_array_return_nested_values,
                    function_array_return_key_kinds,
                    function_array_return_key_values,
                    function_array_return_param_indices,
                    object_classes,
                    trait_use_names,
                    constants,
                    class_constants,
                    array_constants,
                );
            }
        }
        ExprKind::Yield { key, value } => {
            if let Some(key) = key.as_deref() {
                collect_child!(key);
            }
            if let Some(value) = value.as_deref() {
                collect_child!(value);
            }
        }
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::This
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => {}
    }
}

fn is_uninitialized_null_coalesce_assignment(
    name: &str,
    value: &Expr,
    locals: &HashMap<String, LocalKind>,
) -> bool {
    if locals.contains_key(name) {
        return false;
    }
    matches!(
        &value.kind,
        ExprKind::NullCoalesce { value, .. }
            if matches!(&value.kind, ExprKind::Variable(source) if source == name)
    )
}

fn foreach_source_is_unknown_mixed(
    array: &Expr,
    locals: &HashMap<String, LocalKind>,
) -> bool {
    match &array.kind {
        ExprKind::Variable(name) => locals.get(name) == Some(&LocalKind::Mixed),
        ExprKind::FunctionCall { name, args }
            if matches!(name.to_ascii_lowercase().as_str(), "array_filter") =>
        {
            args.first()
                .is_some_and(|source| foreach_source_is_unknown_mixed(source, locals))
        }
        _ => false,
    }
}
