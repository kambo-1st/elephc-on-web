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
            return emit_value_array_items_assign(name, items, module);
        }
        ExprKind::ArrayLiteralAssoc(items) => {
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
        ExprKind::ArrayLiteral(items) => items.clone(),
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
