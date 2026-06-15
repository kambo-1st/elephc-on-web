//! Purpose:
//! Lowers top-level PHP array assignments for the wasm32-web backend.
//! Keeps assignment dispatch and array-access copy materialization out of the expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::emit_array_assign()`
//!
//! Key details:
//! - Array assignments must preserve layout metadata so later reads, foreach, and COW paths stay type-aware.

use super::*;
use crate::types::AttrArgValue;

pub(crate) fn emit_array_assign(
    name: &str,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let items = match &value.kind {
        ExprKind::ConstRef(const_name) => match module.array_constant_value(const_name) {
            Some(ConstantArrayValue::Indexed(items)) => items,
            Some(ConstantArrayValue::Assoc(items)) => {
                return emit_assoc_array_items_assign(
                    name,
                    &normalize_assoc_items(&items).unwrap_or(items),
                    module,
                );
            }
            None => return Err(array_unsupported(value)),
        },
        ExprKind::ArrayLiteral(items) if array_literal_needs_value_cells(items) => {
            assign_callable_array_metadata(name, value, module)?;
            return emit_value_array_items_assign(name, items, module);
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            assign_callable_array_metadata(name, value, module)?;
            return emit_assoc_array_items_assign(name, items, module);
        }
        ExprKind::ArrayAccess { .. } => {
            return emit_array_access_assign(name, value, module);
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } if match_result_is_assoc_array(arms, default.as_deref(), module) => {
            emit_assoc_array_match_to_stack(value, subject, arms, default.as_deref(), module)?;
            module.body().line(&format!("local.set ${}_len", name));
            module.body().line(&format!("local.set ${}_ptr", name));
            module.set_array_layout(name, ArrayLayout::Assoc);
            module.clear_array_length(name);
            module.set_array_value_cell_kinds(name, None);
            module.set_array_value_constants(name, None);
            module.set_array_runtime_value_cell_kind(name, None);
            module.set_array_nested_value_metadata(name, None);
            module.set_array_key_kinds(name, None);
            module.set_array_key_values(name, None);
            module.set_array_php_normalized_runtime_keys(name, true);
            return Ok(());
        }
        ExprKind::Match { .. } => {
            match emit_expr(value, module)? {
                ValueKind::Array => {
                    module.body().line(&format!("local.set ${}_len", name));
                    module.body().line(&format!("local.set ${}_ptr", name));
                    module.set_array_layout(name, ArrayLayout::CompactInt);
                    module.clear_array_length(name);
                    module.set_array_value_cell_kinds(name, None);
                    module.set_array_value_constants(name, None);
                    return Ok(());
                }
                _ => {
                    return Err(CompileError::new(
                        value.span,
                        "wasm32-web array assignment expected an array match result",
                    ));
                }
            }
        }
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } if method.eq_ignore_ascii_case("cases") && module
            .class_name_for_receiver(receiver)
            .and_then(|class_name| module.enum_case_names(&class_name))
            .is_some() =>
        {
            return emit_enum_cases_array_assign(name, value, receiver, args, module);
        }
        ExprKind::ArrayLiteral(items) => {
            assign_callable_array_metadata(name, value, module)?;
            items.clone()
        }
        ExprKind::FunctionCall { name: function_name, args } if function_name.eq_ignore_ascii_case("range") => {
            if let Some(items) = static_range_items_if_possible(value, args, module)? {
                items
            } else if range_args_are_stringy(args, module) {
                return emit_runtime_string_range_assign(name, value, args, module);
            } else {
                return emit_runtime_range_assign(name, value, args, module);
            }
        }
        ExprKind::FunctionCall { name: function_name, args } if function_name.eq_ignore_ascii_case("pathinfo") => {
            if args.first().and_then(|arg| static_pathinfo_path_value(arg, module)).is_some() {
                let items = static_pathinfo_assoc_items(value, args, module)?;
                return emit_assoc_array_items_assign(name, &items, module);
            }
            return emit_runtime_pathinfo_array_assign(name, value, args, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if function_name.eq_ignore_ascii_case("class_parents") =>
        {
            return emit_class_parents_array_assign(name, value, args, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if function_name.eq_ignore_ascii_case("class_implements") =>
        {
            return emit_class_implements_array_assign(name, value, args, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if function_name.eq_ignore_ascii_case("class_uses") =>
        {
            return emit_class_uses_array_assign(name, value, args, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if function_name.eq_ignore_ascii_case("class_attribute_names") =>
        {
            return emit_class_attribute_names_array_assign(name, value, args, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if function_name.eq_ignore_ascii_case("class_attribute_args") =>
        {
            return emit_class_attribute_args_array_assign(name, value, args, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if function_name.eq_ignore_ascii_case("explode") =>
        {
            if args.get(1).is_some_and(|arg| static_string_value(arg, module).is_none())
                || args
                    .get(2)
                    .is_some_and(|arg| static_or_module_const_int_value(arg, module).is_none())
            {
                return emit_runtime_explode_assign(name, value, args, module);
            }
            let span = value.span;
            let items = eval_static_explode(value, args, module)?
                .into_iter()
                .map(|item| Expr::new(ExprKind::StringLiteral(item), span))
                .collect::<Vec<_>>();
            return emit_value_array_items_assign(name, &items, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if function_name.eq_ignore_ascii_case("str_split") =>
        {
            if static_string_value(&args[0], module).is_none()
                || args
                    .get(1)
                    .is_some_and(|arg| static_or_module_const_int_value(arg, module).is_none())
            {
                return emit_runtime_str_split_assign(name, value, args, module);
            }
            let span = value.span;
            let items = eval_static_str_split(value, args, module)?
                .into_iter()
                .map(|item| Expr::new(ExprKind::StringLiteral(item), span))
                .collect::<Vec<_>>();
            return emit_value_array_items_assign(name, &items, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if matches!(
                function_name.to_ascii_lowercase().as_str(),
                "array_values" | "array_reverse" | "array_keys" | "array_unique" | "array_flip"
                    | "array_diff" | "array_intersect"
            ) =>
        {
            return emit_indexed_array_transform_assign(name, value, function_name, args, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if function_name.eq_ignore_ascii_case("array_merge") =>
        {
            return emit_indexed_array_merge_assign(name, value, args, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if function_name.eq_ignore_ascii_case("array_diff_key")
                || function_name.eq_ignore_ascii_case("array_intersect_key") =>
        {
            return emit_assoc_array_key_set_assign(name, value, function_name, args, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if function_name.eq_ignore_ascii_case("array_slice") =>
        {
            return emit_indexed_array_slice_assign(name, value, args, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if function_name.eq_ignore_ascii_case("array_splice") =>
        {
            return emit_indexed_array_splice_assign(name, value, args, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if function_name.eq_ignore_ascii_case("array_pad") =>
        {
            return emit_indexed_array_pad_assign(name, value, args, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if function_name.eq_ignore_ascii_case("array_chunk") =>
        {
            return emit_indexed_array_chunk_assign(name, value, args, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if function_name.eq_ignore_ascii_case("array_rand") =>
        {
            return emit_array_rand_assign(name, value, args, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if function_name.eq_ignore_ascii_case("array_map") =>
        {
            return emit_array_map_assign(name, value, args, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if function_name.eq_ignore_ascii_case("array_filter") =>
        {
            return emit_array_filter_assign(name, value, args, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if function_name.eq_ignore_ascii_case("array_fill") =>
        {
            return emit_indexed_array_fill_assign(name, value, args, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if function_name.eq_ignore_ascii_case("array_fill_keys") =>
        {
            return emit_array_fill_keys_assign(name, value, args, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if function_name.eq_ignore_ascii_case("array_combine") =>
        {
            return emit_array_combine_assign(name, value, args, module);
        }
        ExprKind::FunctionCall { name: function_name, args }
            if function_name.eq_ignore_ascii_case("array_column") =>
        {
            return emit_array_column_assign(name, value, args, module);
        }
        ExprKind::FunctionCall { name: function_name, .. }
            if is_callback_array_builtin(function_name) =>
        {
            return Err(CompileError::new(
                value.span,
                "wasm32-web callback array builtins require callable runtime support",
            ));
        }
        ExprKind::ExprCall { callee, .. }
            if callable_expr_array_return_metadata(callee, module).is_some() =>
        {
            let metadata = callable_expr_array_return_metadata(callee, module)
                .expect("guarded callable expression array return metadata");
            match emit_expr(value, module)? {
                ValueKind::Array => {
                    module.body().line(&format!("local.set ${}_len", name));
                    module.body().line(&format!("local.set ${}_ptr", name));
                    stamp_array_return_metadata(name, metadata, module);
                    return Ok(());
                }
                _ => unreachable!("array-returning callable expression metadata must emit an array value"),
            }
        }
        ExprKind::FunctionCall { name: function_name, args }
            if module.has_function(function_name)
                && module.function_return_kind(function_name) == Some(ValueKind::Array) =>
        {
            emit_user_function_args(value, function_name, args, module)?;
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(function_name)));
            module.body().line(&format!("local.set ${}_len", name));
            module.body().line(&format!("local.set ${}_ptr", name));
            if apply_forwarded_array_param_metadata(name, function_name, args, module) {
                return Ok(());
            }
            module.set_array_layout(
                name,
                module.function_array_return_layout(function_name),
            );
            if let Some(len) = module.function_array_return_length(function_name) {
                module.set_array_length(name, len);
            }
            if module.function_array_return_layout(function_name) == ArrayLayout::Value {
                module.set_array_value_cell_kinds(
                    name,
                    module
                        .function_array_return_value_kinds(function_name)
                        .map(|kinds| kinds.to_vec()),
                );
                module.set_array_value_constants(
                    name,
                    module
                        .function_array_return_value_constants(function_name)
                        .map(|values| values.to_vec()),
                );
                module.set_array_runtime_value_cell_kind(
                    name,
                    module.function_array_return_runtime_value_kind(function_name),
                );
                module.set_array_nested_value_metadata(
                    name,
                    module
                        .function_array_return_nested_values(function_name)
                        .map(|metadata| metadata.to_vec()),
                );
            } else if module.function_array_return_layout(function_name) == ArrayLayout::Assoc {
                module.set_array_value_cell_kinds(
                    name,
                    module
                        .function_array_return_value_kinds(function_name)
                        .map(|kinds| kinds.to_vec()),
                );
                module.set_array_value_constants(
                    name,
                    module
                        .function_array_return_value_constants(function_name)
                        .map(|values| values.to_vec()),
                );
                module.set_array_runtime_value_cell_kind(
                    name,
                    module.function_array_return_runtime_value_kind(function_name),
                );
                module.set_array_nested_value_metadata(
                    name,
                    module
                        .function_array_return_nested_values(function_name)
                        .map(|metadata| metadata.to_vec()),
                );
                module.set_array_key_kinds(
                    name,
                    module
                        .function_array_return_key_kinds(function_name)
                        .map(|kinds| kinds.to_vec()),
                );
                module.set_array_key_values(
                    name,
                    module
                        .function_array_return_key_values(function_name)
                        .map(|values| values.to_vec()),
                );
                module.set_array_php_normalized_runtime_keys(
                    name,
                    module.function_array_return_key_kinds(function_name).is_none(),
                );
            } else {
                module.set_array_value_cell_kinds(name, None);
                module.set_array_nested_value_metadata(name, None);
            }
            return Ok(());
        }
        ExprKind::MethodCall { object, method, .. }
            if method_call_array_return_metadata(object, method, module).is_some() =>
        {
            let metadata = method_call_array_return_metadata(object, method, module)
                .expect("guarded method array return metadata");
            match emit_expr(value, module)? {
                ValueKind::Array => {
                    module.body().line(&format!("local.set ${}_len", name));
                    module.body().line(&format!("local.set ${}_ptr", name));
                    module.set_array_layout(name, metadata.layout);
                    if let Some(len) = metadata.len {
                        module.set_array_length(name, len);
                    }
                    module.set_array_value_cell_kinds(name, metadata.value_kinds);
                    module.set_array_value_constants(name, metadata.value_constants);
                    module.set_array_runtime_value_cell_kind(name, metadata.runtime_value_kind);
                    module.set_array_nested_value_metadata(name, metadata.nested_values);
                    module.set_array_key_kinds(name, metadata.key_kinds);
                    module.set_array_key_values(name, metadata.key_values);
                    return Ok(());
                }
                _ => unreachable!("array-returning method metadata must emit an array value"),
            }
        }
        ExprKind::NullsafeMethodCall { object, method, .. }
            if object_expr_is_known_non_null(object, module)
                && method_call_array_return_metadata(object, method, module).is_some() =>
        {
            let metadata = method_call_array_return_metadata(object, method, module)
                .expect("guarded method array return metadata");
            match emit_expr(value, module)? {
                ValueKind::Array => {
                    module.body().line(&format!("local.set ${}_len", name));
                    module.body().line(&format!("local.set ${}_ptr", name));
                    module.set_array_layout(name, metadata.layout);
                    if let Some(len) = metadata.len {
                        module.set_array_length(name, len);
                    }
                    module.set_array_value_cell_kinds(name, metadata.value_kinds);
                    module.set_array_value_constants(name, metadata.value_constants);
                    module.set_array_runtime_value_cell_kind(name, metadata.runtime_value_kind);
                    module.set_array_nested_value_metadata(name, metadata.nested_values);
                    module.set_array_key_kinds(name, metadata.key_kinds);
                    module.set_array_key_values(name, metadata.key_values);
                    return Ok(());
                }
                _ => unreachable!("array-returning method metadata must emit an array value"),
            }
        }
        ExprKind::StaticMethodCall { receiver, method, .. }
            if static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            let metadata = static_method_call_array_return_metadata(receiver, method, module)
                .expect("guarded static method array return metadata");
            match emit_expr(value, module)? {
                ValueKind::Array => {
                    module.body().line(&format!("local.set ${}_len", name));
                    module.body().line(&format!("local.set ${}_ptr", name));
                    module.set_array_layout(name, metadata.layout);
                    if let Some(len) = metadata.len {
                        module.set_array_length(name, len);
                    }
                    module.set_array_value_cell_kinds(name, metadata.value_kinds);
                    module.set_array_value_constants(name, metadata.value_constants);
                    module.set_array_runtime_value_cell_kind(name, metadata.runtime_value_kind);
                    module.set_array_nested_value_metadata(name, metadata.nested_values);
                    module.set_array_key_kinds(name, metadata.key_kinds);
                    module.set_array_key_values(name, metadata.key_values);
                    return Ok(());
                }
                _ => unreachable!("array-returning static method metadata must emit an array value"),
            }
        }
        ExprKind::DynamicStaticMethodCall { receiver, method, .. }
            if dynamic_static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            let metadata = dynamic_static_method_call_array_return_metadata(receiver, method, module)
                .expect("guarded dynamic static method array return metadata");
            match emit_expr(value, module)? {
                ValueKind::Array => {
                    module.body().line(&format!("local.set ${}_len", name));
                    module.body().line(&format!("local.set ${}_ptr", name));
                    module.set_array_layout(name, metadata.layout);
                    if let Some(len) = metadata.len {
                        module.set_array_length(name, len);
                    }
                    module.set_array_value_cell_kinds(name, metadata.value_kinds);
                    module.set_array_value_constants(name, metadata.value_constants);
                    module.set_array_runtime_value_cell_kind(name, metadata.runtime_value_kind);
                    module.set_array_nested_value_metadata(name, metadata.nested_values);
                    module.set_array_key_kinds(name, metadata.key_kinds);
                    module.set_array_key_values(name, metadata.key_values);
                    return Ok(());
                }
                _ => unreachable!("array-returning dynamic static method metadata must emit an array value"),
            }
        }
        ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
            return emit_indexed_array_copy_assign(name, source, module);
        }
        _ => return Err(array_unsupported(value)),
    };
    if array_literal_needs_value_cells(&items) {
        return emit_value_array_items_assign(name, &items, module);
    }
    module.set_array_layout(name, ArrayLayout::CompactInt);
    emit_static_array_items_assign(name, &items, module)
}

pub(in crate::codegen::wasm) fn callable_expr_array_return_metadata(
    callee: &Expr,
    module: &WasmModule,
) -> Option<MethodArrayReturnMetadata> {
    let mut metadata: Option<MethodArrayReturnMetadata> = None;
    for target in callable_return_expr_targets(callee, module)? {
        if module.function_return_kind(&target) != Some(ValueKind::Array) {
            return None;
        }
        let candidate = MethodArrayReturnMetadata {
            layout: module.function_array_return_layout(&target),
            len: module.function_array_return_length(&target),
            value_kinds: module
                .function_array_return_value_kinds(&target)
                .map(|kinds| kinds.to_vec()),
            value_constants: module
                .function_array_return_value_constants(&target)
                .map(|values| values.to_vec()),
            runtime_value_kind: module.function_array_return_runtime_value_kind(&target),
            nested_values: module
                .function_array_return_nested_values(&target)
                .map(|values| values.to_vec()),
            key_kinds: module
                .function_array_return_key_kinds(&target)
                .map(|kinds| kinds.to_vec()),
            key_values: module
                .function_array_return_key_values(&target)
                .map(|values| values.to_vec()),
        };
        if let Some(existing) = metadata.as_mut() {
            if existing.layout != candidate.layout
                || existing.len != candidate.len
                || existing.value_kinds != candidate.value_kinds
                || existing.runtime_value_kind != candidate.runtime_value_kind
                || existing.nested_values != candidate.nested_values
                || existing.key_kinds != candidate.key_kinds
                || existing.key_values != candidate.key_values
            {
                return None;
            }
            if existing.value_constants != candidate.value_constants {
                existing.value_constants = None;
            }
        } else {
            metadata = Some(candidate);
        }
    }
    metadata
}

fn stamp_array_return_metadata(
    name: &str,
    metadata: MethodArrayReturnMetadata,
    module: &mut WasmModule,
) {
    module.set_array_layout(name, metadata.layout);
    if let Some(len) = metadata.len {
        module.set_array_length(name, len);
    }
    module.set_array_value_cell_kinds(name, metadata.value_kinds);
    module.set_array_value_constants(name, metadata.value_constants);
    module.set_array_runtime_value_cell_kind(name, metadata.runtime_value_kind);
    module.set_array_nested_value_metadata(name, metadata.nested_values);
    module.set_array_key_kinds(name, metadata.key_kinds);
    module.set_array_key_values(name, metadata.key_values);
}

fn apply_forwarded_array_param_metadata(
    name: &str,
    function_name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> bool {
    let Some(param_index) = module.function_array_return_param_index(function_name) else {
        return false;
    };
    let Some(arg) = args.get(param_index) else {
        return false;
    };
    match &arg.kind {
        ExprKind::ArrayLiteral(items) => {
            let layout = ArrayLayout::Value;
            module.set_array_layout(name, layout);
            module.set_array_length(name, items.len());
            module.set_array_value_cell_kinds(name, static_array_value_kinds(items));
            true
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            module.set_array_layout(name, ArrayLayout::Assoc);
            module.set_array_length(name, items.len());
            module.set_array_value_cell_kinds(name, static_assoc_array_value_kinds(items));
            module.set_array_key_kinds(name, static_assoc_array_key_kinds(items));
            module.set_array_key_values(name, static_assoc_array_key_values(items));
            true
        }
        ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
            module.set_array_layout(name, module.array_layout(source));
            if let Some(len) = module.array_length(source) {
                module.set_array_length(name, len);
            }
            module.set_array_value_cell_kinds(
                name,
                module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec()),
            );
            module.set_array_runtime_value_cell_kind(name, module.array_runtime_value_cell_kind(source));
            module.set_array_nested_value_metadata(
                name,
                module.array_nested_value_metadata_items(source).map(|items| items.to_vec()),
            );
            module.set_array_key_kinds(
                name,
                module.array_key_kinds(source).map(|kinds| kinds.to_vec()),
            );
            module.set_array_key_values(
                name,
                module.array_key_values(source).map(|values| values.to_vec()),
            );
            true
        }
        ExprKind::FunctionCall { name: forwarded_name, .. }
            if module.has_function(forwarded_name)
                && module.function_return_kind(forwarded_name) == Some(ValueKind::Array) =>
        {
            module.set_array_layout(name, module.function_array_return_layout(forwarded_name));
            if let Some(len) = module.function_array_return_length(forwarded_name) {
                module.set_array_length(name, len);
            }
            module.set_array_value_cell_kinds(
                name,
                module
                    .function_array_return_value_kinds(forwarded_name)
                    .map(|kinds| kinds.to_vec()),
            );
            module.set_array_runtime_value_cell_kind(
                name,
                module.function_array_return_runtime_value_kind(forwarded_name),
            );
            module.set_array_nested_value_metadata(
                name,
                module
                    .function_array_return_nested_values(forwarded_name)
                    .map(|metadata| metadata.to_vec()),
            );
            module.set_array_key_kinds(
                name,
                module
                    .function_array_return_key_kinds(forwarded_name)
                    .map(|kinds| kinds.to_vec()),
            );
            module.set_array_key_values(
                name,
                module
                    .function_array_return_key_values(forwarded_name)
                    .map(|values| values.to_vec()),
            );
            true
        }
        _ => false,
    }
}

fn static_array_value_kinds(items: &[Expr]) -> Option<Vec<ValueCellKind>> {
    items.iter().map(static_value_kind_for_array_item).collect()
}

fn static_assoc_array_value_kinds(items: &[(Expr, Expr)]) -> Option<Vec<ValueCellKind>> {
    items
        .iter()
        .map(|(_, value)| static_value_kind_for_array_item(value))
        .collect()
}

fn static_value_kind_for_array_item(item: &Expr) -> Option<ValueCellKind> {
    match &item.kind {
        ExprKind::IntLiteral(_) => Some(ValueCellKind::Int),
        ExprKind::FloatLiteral(_) => Some(ValueCellKind::Float),
        ExprKind::BoolLiteral(_) => Some(ValueCellKind::Bool),
        ExprKind::StringLiteral(_) => Some(ValueCellKind::Str),
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => Some(ValueCellKind::Array),
        ExprKind::Null => Some(ValueCellKind::Null),
        _ => None,
    }
}

fn static_assoc_array_key_kinds(items: &[(Expr, Expr)]) -> Option<Vec<AssocKeyKind>> {
    items
        .iter()
        .map(|(key, _)| match key.kind {
            ExprKind::IntLiteral(_) => Some(AssocKeyKind::Int),
            ExprKind::StringLiteral(_) => Some(AssocKeyKind::Str),
            _ => None,
        })
        .collect()
}

fn static_assoc_array_key_values(items: &[(Expr, Expr)]) -> Option<Vec<AssocKeyValue>> {
    items
        .iter()
        .map(|(key, _)| match &key.kind {
            ExprKind::IntLiteral(value) => Some(AssocKeyValue::Int(*value)),
            ExprKind::StringLiteral(value) => Some(AssocKeyValue::Str(value.clone())),
            _ => None,
        })
        .collect()
}

fn emit_class_parents_array_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let ([target] | [target, _]) = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web class_parents() expects one or two arguments",
        ));
    };
    let class_name = if let Some(class_name) = evaluated_static_or_tracked_string_value(target, module)? {
        class_name
    } else if let Some(class_name) = object_class_name_for_expr(target, module) {
        if emit_expr(target, module)? != ValueKind::Object {
            return Err(CompileError::new(
                target.span,
                "wasm32-web class_parents() expected an object or static class-string argument",
            ));
        }
        module.body().line("drop");
        class_name
    } else if let Some(cell) = materialize_mixed_value_cell(target, module)? {
        return emit_mixed_object_relation_array_assign(
            name,
            &cell,
            target.span,
            "class_parents",
            class_parent_assoc_items,
            module,
        );
    } else {
        return Err(CompileError::new(
            target.span,
            "wasm32-web class_parents() currently requires a known object or static class-string argument",
        ));
    };
    let Some(items) = class_parent_assoc_items(&class_name, target.span, module) else {
        return Err(CompileError::new(
            target.span,
            "wasm32-web class_parents() currently requires a declared class target",
        ));
    };
    emit_assoc_array_items_assign(name, &items, module)
}

fn class_parent_assoc_items(
    class_name: &str,
    span: crate::span::Span,
    module: &WasmModule,
) -> Option<Vec<(Expr, Expr)>> {
    module.object_class(class_name)?;
    let mut names = Vec::new();
    let mut current = class_name.to_string();
    while let Some(class_info) = module.object_class(&current) {
        let Some(parent_name) = class_info
            .parent
            .as_deref()
            .and_then(|parent| module.object_class(parent))
            .map(|parent| parent.name.clone())
        else {
            break;
        };
        names.push(parent_name.clone());
        current = parent_name;
    }
    Some(
        names
            .into_iter()
            .map(|name| {
                let key = Expr::new(ExprKind::StringLiteral(name.clone()), span);
                let value = Expr::new(ExprKind::StringLiteral(name), span);
                (key, value)
            })
            .collect(),
    )
}

fn emit_class_attribute_names_array_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let [target] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web class_attribute_names() expects one argument",
        ));
    };
    let Some(class_name) = evaluated_static_or_tracked_string_value(target, module)? else {
        return Err(CompileError::new(
            target.span,
            "wasm32-web class_attribute_names() currently requires a static class-string argument",
        ));
    };
    let Some(class_info) = module.object_class(&class_name) else {
        return Err(CompileError::new(
            target.span,
            "wasm32-web class_attribute_names() currently requires a declared class target",
        ));
    };
    emit_assoc_array_items_assign(
        name,
        &assoc_string_set_items(class_info.attribute_names.clone(), target.span),
        module,
    )
}

fn emit_class_attribute_args_array_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let [target, attr] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web class_attribute_args() expects two arguments",
        ));
    };
    let Some(class_name) = evaluated_static_or_tracked_string_value(target, module)? else {
        return Err(CompileError::new(
            target.span,
            "wasm32-web class_attribute_args() currently requires a static class-string argument",
        ));
    };
    let Some(attr_name) = evaluated_static_or_tracked_string_value(attr, module)? else {
        return Err(CompileError::new(
            attr.span,
            "wasm32-web class_attribute_args() currently requires a static attribute-name argument",
        ));
    };
    let attr_args = class_attribute_args(&class_name, &attr_name, module, target.span)?;
    let items = attr_args
        .iter()
        .map(|arg| attr_arg_expr(arg, call.span))
        .collect::<Vec<_>>();
    emit_value_array_items_assign(name, &items, module)
}

fn class_attribute_args(
    class_name: &str,
    attr_name: &str,
    module: &WasmModule,
    span: crate::span::Span,
) -> Result<Vec<AttrArgValue>, CompileError> {
    let Some(class_info) = module.object_class(class_name) else {
        return Err(CompileError::new(
            span,
            "wasm32-web class_attribute_args() currently requires a declared class target",
        ));
    };
    let attr_key = attr_name.trim_start_matches('\\').to_ascii_lowercase();
    let Some((index, _)) = class_info
        .attribute_names
        .iter()
        .enumerate()
        .find(|(_, name)| name.trim_start_matches('\\').to_ascii_lowercase() == attr_key)
    else {
        return Ok(Vec::new());
    };
    match class_info.attribute_args.get(index) {
        Some(Some(args)) => Ok(args.clone()),
        _ => Err(CompileError::new(
            span,
            "wasm32-web class_attribute_args() cannot materialize unsupported attribute argument metadata",
        )),
    }
}

fn attr_arg_expr(arg: &AttrArgValue, span: crate::span::Span) -> Expr {
    let kind = match arg {
        AttrArgValue::Null => ExprKind::Null,
        AttrArgValue::Int(value) => ExprKind::IntLiteral(*value),
        AttrArgValue::Bool(value) => ExprKind::BoolLiteral(*value),
        AttrArgValue::Str(value) => ExprKind::StringLiteral(value.clone()),
    };
    Expr::new(kind, span)
}

fn emit_class_implements_array_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let ([target] | [target, _]) = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web class_implements() expects one or two arguments",
        ));
    };
    let names = if let Some(class_name) = evaluated_static_or_tracked_string_value(target, module)? {
        if let Some(names) = module.implemented_interface_names_for_class(&class_name) {
            names
        } else if let Some(names) = module.parent_interface_names_for_interface(&class_name) {
            names
        } else {
            return Err(CompileError::new(
                target.span,
                "wasm32-web class_implements() currently requires a declared class or interface target",
            ));
        }
    } else if let Some(class_name) = object_class_name_for_expr(target, module) {
        if emit_expr(target, module)? != ValueKind::Object {
            return Err(CompileError::new(
                target.span,
                "wasm32-web class_implements() expected an object or static class/interface-string argument",
            ));
        }
        module.body().line("drop");
        module
            .implemented_interface_names_for_class(&class_name)
            .ok_or_else(|| {
                CompileError::new(
                    target.span,
                    "wasm32-web class_implements() currently requires a declared class target",
                )
            })?
    } else if let Some(cell) = materialize_mixed_value_cell(target, module)? {
        return emit_mixed_object_relation_array_assign(
            name,
            &cell,
            target.span,
            "class_implements",
            class_implements_assoc_items,
            module,
        );
    } else {
        return Err(CompileError::new(
            target.span,
            "wasm32-web class_implements() currently requires a known object or static class/interface-string argument",
        ));
    };
    emit_assoc_array_items_assign(name, &assoc_string_set_items(names, target.span), module)
}

fn class_implements_assoc_items(
    class_name: &str,
    span: crate::span::Span,
    module: &WasmModule,
) -> Option<Vec<(Expr, Expr)>> {
    module
        .implemented_interface_names_for_class(class_name)
        .map(|names| assoc_string_set_items(names, span))
}

fn assoc_string_set_items(names: Vec<String>, span: crate::span::Span) -> Vec<(Expr, Expr)> {
    names
        .into_iter()
        .map(|name| {
            let key = Expr::new(ExprKind::StringLiteral(name.clone()), span);
            let value = Expr::new(ExprKind::StringLiteral(name), span);
            (key, value)
        })
        .collect()
}

fn static_or_tracked_string_value(expr: &Expr, module: &WasmModule) -> Option<String> {
    match &expr.kind {
        ExprKind::Variable(name) => module.string_static_value(name),
        ExprKind::FunctionCall { name, args } => module
            .function_static_string_return_for_call(name, args)
            .or_else(|| module.function_static_string_return(name)),
        _ => static_string_value(expr, module),
    }
}

fn evaluated_static_or_tracked_string_value(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<Option<String>, CompileError> {
    let Some(value) = static_or_tracked_string_value(expr, module) else {
        return Ok(None);
    };
    if matches!(expr.kind, ExprKind::FunctionCall { .. }) {
        emit_string_value_to_stack(expr, module)?;
        module.body().line("drop");
        module.body().line("drop");
    }
    Ok(Some(value))
}

fn emit_class_uses_array_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let ([target] | [target, _]) = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web class_uses() expects one or two arguments",
        ));
    };
    let names = if let Some(class_name) = evaluated_static_or_tracked_string_value(target, module)? {
        module
            .used_trait_names_for_class_or_trait(&class_name)
            .ok_or_else(|| {
                CompileError::new(
                    target.span,
                    "wasm32-web class_uses() currently requires a declared class or trait target",
                )
            })?
    } else if let Some(class_name) = object_class_name_for_expr(target, module) {
        if emit_expr(target, module)? != ValueKind::Object {
            return Err(CompileError::new(
                target.span,
                "wasm32-web class_uses() expected an object or static class/trait-string argument",
            ));
        }
        module.body().line("drop");
        module
            .used_trait_names_for_class_or_trait(&class_name)
            .ok_or_else(|| {
                CompileError::new(
                    target.span,
                    "wasm32-web class_uses() currently requires a declared class target",
                )
            })?
    } else if let Some(cell) = materialize_mixed_value_cell(target, module)? {
        return emit_mixed_object_relation_array_assign(
            name,
            &cell,
            target.span,
            "class_uses",
            class_uses_assoc_items,
            module,
        );
    } else {
        return Err(CompileError::new(
            target.span,
            "wasm32-web class_uses() currently requires a known object or static class/trait-string argument",
        ));
    };
    emit_assoc_array_items_assign(name, &assoc_string_set_items(names, target.span), module)
}

fn class_uses_assoc_items(
    class_name: &str,
    span: crate::span::Span,
    module: &WasmModule,
) -> Option<Vec<(Expr, Expr)>> {
    module
        .used_trait_names_for_class_or_trait(class_name)
        .map(|names| assoc_string_set_items(names, span))
}

fn emit_mixed_object_relation_array_assign(
    name: &str,
    cell: &str,
    span: crate::span::Span,
    _builtin: &str,
    items_for_class: fn(&str, crate::span::Span, &WasmModule) -> Option<Vec<(Expr, Expr)>>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let classes = module.object_class_names_by_id();
    if classes.is_empty() {
        return Err(CompileError::new(
            span,
            "wasm32-web object relation arrays from mixed object cells require object class metadata",
        ));
    }
    let class_id = module
        .next_label("mixed_object_relation_array_class_id")
        .trim_start_matches('$')
        .to_string();
    module.declare_i64_local(class_id.clone());
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_OBJECT));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line("i64.load");
    module.body().line(&format!("local.set ${}", class_id));
    emit_mixed_object_relation_array_branch(name, &class_id, &classes, 0, span, items_for_class, module)?;
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.clear_array_length(name);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_value_constants(name, None);
    module.set_array_runtime_value_cell_kind(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.set_array_object_classes(name, None);
    module.set_array_key_kinds(name, None);
    module.set_array_key_values(name, None);
    module.set_array_php_normalized_runtime_keys(name, true);
    Ok(())
}

fn emit_mixed_object_relation_array_branch(
    name: &str,
    class_id: &str,
    classes: &[(u64, String)],
    index: usize,
    span: crate::span::Span,
    items_for_class: fn(&str, crate::span::Span, &WasmModule) -> Option<Vec<(Expr, Expr)>>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if index == classes.len() {
        module.body().line("unreachable");
        return Ok(());
    }
    let (candidate_id, class_name) = &classes[index];
    module.body().line(&format!("local.get ${}", class_id));
    module.body().line(&format!("i64.const {}", candidate_id));
    module.body().line("i64.eq");
    module.body().open("if");
    let items = items_for_class(class_name, span, module).ok_or_else(|| {
        CompileError::new(
            span,
            "wasm32-web object relation arrays currently require declared class metadata",
        )
    })?;
    emit_assoc_array_items_assign(name, &items, module)?;
    module.body().line("else");
    emit_mixed_object_relation_array_branch(
        name,
        class_id,
        classes,
        index + 1,
        span,
        items_for_class,
        module,
    )?;
    module.body().close("end");
    Ok(())
}

fn emit_array_access_assign(
    name: &str,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let metadata = array_access_array_metadata(value, module);
    match emit_expr(value, module)? {
        ValueKind::Array => {
            module.body().line(&format!("local.set ${}_len", name));
            module.body().line(&format!("local.set ${}_ptr", name));
            if let Some(metadata) = metadata {
                emit_copy_array_access_payload(name, &metadata, module);
                let NestedArrayMetadata {
                    layout,
                    len,
                    value_kinds,
                    key_values,
                    nested_values,
                } = metadata;
                module.set_array_length(name, len);
                module.set_array_layout(name, layout);
                module.set_array_value_cell_kinds(name, value_kinds);
                module.set_array_nested_value_metadata(name, nested_values);
                if layout == ArrayLayout::Assoc {
                    module.set_array_key_kinds(
                        name,
                        key_values
                            .as_ref()
                            .map(|values| values.iter().map(assoc_key_kind_for_value).collect()),
                    );
                    module.set_array_key_values(name, key_values);
                }
            } else {
                module.set_array_layout(name, ArrayLayout::Value);
                module.set_array_value_cell_kinds(name, None);
                module.set_array_nested_value_metadata(name, None);
            }
            Ok(())
        }
        _ => Err(CompileError::new(
            value.span,
            "wasm32-web array assignment requires an indexed array value",
        )),
    }
}

fn emit_copy_array_access_payload(
    name: &str,
    metadata: &NestedArrayMetadata,
    module: &mut WasmModule,
) {
    let source_ptr = module.next_label("array_access_copy_source_ptr");
    let source_len = module.next_label("array_access_copy_source_len");
    module.declare_i32_local(source_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(source_len.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.set {}", source_ptr));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line(&format!("local.set {}", source_len));
    match metadata.layout {
        ArrayLayout::CompactInt => emit_copy_compact_array_access_payload(name, &source_ptr, &source_len, module),
        ArrayLayout::Value => emit_copy_value_array_access_payload(name, &source_ptr, &source_len, module),
        ArrayLayout::Assoc => emit_copy_assoc_array_access_payload(name, &source_ptr, &source_len, module),
    }
}

fn emit_copy_compact_array_access_payload(
    name: &str,
    source_ptr: &str,
    source_len: &str,
    module: &mut WasmModule,
) {
    let index = module.next_label("array_access_compact_copy_index");
    let done_label = module.next_label("array_access_compact_copy_done");
    let loop_label = module.next_label("array_access_compact_copy_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_copy_value_array_access_payload(
    name: &str,
    source_ptr: &str,
    source_len: &str,
    module: &mut WasmModule,
) {
    let index = module.next_label("array_access_value_copy_index");
    let done_label = module.next_label("array_access_value_copy_done");
    let loop_label = module.next_label("array_access_value_copy_loop");
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_copy_value_cell(name, source_ptr, &index, &index, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_copy_assoc_array_access_payload(
    name: &str,
    source_ptr: &str,
    source_len: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("call $__rt_assoc_array_copy_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
}
