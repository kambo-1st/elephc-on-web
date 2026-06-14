//! Purpose:
//! Dispatches wasm32-web array transform and set-operation assignments.
//! Keeps the large orchestration match out of the module wiring file.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets`.
//!
//! Key details:
//! - Dispatch preserves PHP key/value normalization and delegates concrete lowering to sibling helpers.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_indexed_array_transform_assign(
    name: &str,
    expr: &Expr,
    function_name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if function_name.eq_ignore_ascii_case("array_diff")
        || function_name.eq_ignore_ascii_case("array_intersect")
    {
        if args.len() < 2 {
            return Err(CompileError::new(
                expr.span,
                &format!("wasm32-web {function_name}() expects at least two arguments"),
            ));
        }
    } else if function_name.eq_ignore_ascii_case("array_unique") {
        validate_array_unique_sort_flag(args, expr.span, module)?;
    } else if function_name.eq_ignore_ascii_case("array_reverse") {
        validate_array_reverse_preserve_keys_arg(args, expr.span, module)?;
    } else if args.len() != 1 {
        return Err(CompileError::new(
            expr.span,
            &format!("wasm32-web {function_name}() expects exactly one argument"),
        ));
    }
    match &args[0].kind {
        ExprKind::ConstRef(const_name) => match module.array_constant_value(const_name) {
            Some(ConstantArrayValue::Indexed(items)) => {
                let mut rewritten_args = args.to_vec();
                rewritten_args[0] = Expr::new(ExprKind::ArrayLiteral(items), args[0].span);
                emit_indexed_array_transform_assign(
                    name,
                    expr,
                    function_name,
                    &rewritten_args,
                    module,
                )
            }
            Some(ConstantArrayValue::Assoc(items)) => {
                let mut rewritten_args = args.to_vec();
                let items = normalize_assoc_items(&items).unwrap_or(items);
                rewritten_args[0] = Expr::new(ExprKind::ArrayLiteralAssoc(items), args[0].span);
                emit_indexed_array_transform_assign(
                    name,
                    expr,
                    function_name,
                    &rewritten_args,
                    module,
                )
            }
            None => Err(array_unsupported(&args[0])),
        },
        ExprKind::ArrayLiteral(items) if function_name.eq_ignore_ascii_case("array_unique") => {
            match array_unique_sort_mode(args, expr.span, module)? {
                ArrayUniqueSortMode::Regular => {
                    let items = static_array_unique_regular_items(&args[0], items, module)?;
                    emit_assoc_array_items_assign(name, &items, module)
                }
                ArrayUniqueSortMode::Numeric => {
                    let items = static_array_unique_numeric_items(&args[0], items, module)?;
                    emit_assoc_array_items_assign(name, &items, module)
                }
                ArrayUniqueSortMode::String => {
                    if let Ok(items) = static_unique_int_array_items(&args[0], items) {
                        emit_assoc_array_items_assign(name, &items, module)
                    } else if value_cell_kinds_for_items(items, module).is_some() {
                        let temp = module
                            .next_label("array_unique_value_literal")
                            .trim_start_matches('$')
                            .to_string();
                        module.declare_array_local(temp.clone());
                        emit_value_array_items_assign(&temp, items, module)?;
                        emit_known_value_string_array_unique_assign(name, &temp, args, module)
                    } else {
                        static_unique_int_array_items(&args[0], items)
                            .and_then(|items| emit_assoc_array_items_assign(name, &items, module))
                    }
                }
            }
        }
        ExprKind::ArrayLiteral(items) if function_name.eq_ignore_ascii_case("array_flip") => {
            let items = static_array_flip_items(&args[0], items, module)?;
            emit_assoc_array_items_assign(name, &items, module)
        }
        ExprKind::ArrayLiteral(items) if function_name.eq_ignore_ascii_case("array_diff") => {
            if let Ok(items) = static_array_diff_int_items(expr, items, args) {
                emit_assoc_array_items_assign(name, &items, module)
            } else if value_cell_kinds_for_items(items, module).is_some() {
                let temp = module
                    .next_label("array_diff_value_literal")
                    .trim_start_matches('$')
                    .to_string();
                module.declare_array_local(temp.clone());
                emit_value_array_items_assign(&temp, items, module)?;
                emit_known_value_string_array_value_set_assign(name, &temp, function_name, args, module)
            } else {
                static_array_diff_int_items(expr, items, args)
                    .and_then(|items| emit_assoc_array_items_assign(name, &items, module))
            }
        }
        ExprKind::ArrayLiteral(items) if function_name.eq_ignore_ascii_case("array_intersect") => {
            if let Ok(items) = static_array_intersect_int_items(expr, items, args) {
                emit_assoc_array_items_assign(name, &items, module)
            } else if value_cell_kinds_for_items(items, module).is_some() {
                let temp = module
                    .next_label("array_intersect_value_literal")
                    .trim_start_matches('$')
                    .to_string();
                module.declare_array_local(temp.clone());
                emit_value_array_items_assign(&temp, items, module)?;
                emit_known_value_string_array_value_set_assign(name, &temp, function_name, args, module)
            } else {
                static_array_intersect_int_items(expr, items, args)
                    .and_then(|items| emit_assoc_array_items_assign(name, &items, module))
            }
        }
        ExprKind::ArrayLiteral(items) => {
            let preserve_keys = if function_name.eq_ignore_ascii_case("array_reverse") {
                array_reverse_preserve_keys_arg(args, module)?
            } else {
                false
            };
            if function_name.eq_ignore_ascii_case("array_reverse") && preserve_keys {
                let items = reversed_indexed_preserve_key_items(expr, items);
                return emit_assoc_array_items_assign(name, &items, module);
            }
            let items = transformed_static_array_items(expr, function_name, items)?;
            if function_name.eq_ignore_ascii_case("array_keys") || !array_literal_needs_value_cells(&items) {
                module.set_array_layout(name, ArrayLayout::CompactInt);
                emit_static_array_items_assign(name, &items, module)
            } else {
                emit_value_array_items_assign(name, &items, module)
            }
        }
        ExprKind::ArrayLiteralAssoc(items)
            if function_name.eq_ignore_ascii_case("array_reverse") =>
        {
            let items = if array_reverse_preserve_keys_arg(args, module)? {
                reversed_assoc_preserve_key_items(&args[0], items)?
            } else {
                reversed_assoc_default_items(&args[0], items, module)?
            };
            emit_assoc_array_items_assign(name, &items, module)
        }
        ExprKind::ArrayLiteralAssoc(items)
            if function_name.eq_ignore_ascii_case("array_unique") =>
        {
            let items = match array_unique_sort_mode(args, expr.span, module)? {
                ArrayUniqueSortMode::Regular => {
                    static_assoc_array_unique_regular_items(&args[0], items, module)?
                }
                ArrayUniqueSortMode::Numeric => {
                    static_assoc_array_unique_numeric_items(&args[0], items, module)?
                }
                ArrayUniqueSortMode::String => static_assoc_array_unique_items(&args[0], items, module)?,
            };
            emit_assoc_array_items_assign(name, &items, module)
        }
        ExprKind::ArrayLiteralAssoc(items)
            if function_name.eq_ignore_ascii_case("array_values") =>
        {
            let temp = module
                .next_label("array_values_assoc_literal")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_assoc_array_items_assign(&temp, items, module)?;
            emit_assoc_array_values_assign(name, &temp, module)
        }
        ExprKind::ArrayLiteralAssoc(items)
            if function_name.eq_ignore_ascii_case("array_keys") =>
        {
            let temp = module
                .next_label("array_keys_assoc_literal")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_assoc_array_items_assign(&temp, items, module)?;
            emit_assoc_array_keys_assign(name, &temp, module)
        }
        ExprKind::ArrayLiteralAssoc(items)
            if function_name.eq_ignore_ascii_case("array_flip") =>
        {
            let items = static_assoc_array_flip_items(&args[0], items, module)?;
            emit_assoc_array_items_assign(name, &items, module)
        }
        ExprKind::ArrayLiteralAssoc(items)
            if function_name.eq_ignore_ascii_case("array_diff")
                || function_name.eq_ignore_ascii_case("array_intersect") =>
        {
            let temp = module
                .next_label("array_value_set_assoc_literal")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_assoc_array_items_assign(&temp, items, module)?;
            emit_known_assoc_array_value_set_assign(name, &temp, function_name, args, module)
        }
        ExprKind::ArrayLiteralAssoc(_) => Err(array_unsupported(&args[0])),
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args: case_args,
        } if matches!(
                function_name.to_ascii_lowercase().as_str(),
                "array_values" | "array_reverse" | "array_keys"
            )
            && method.eq_ignore_ascii_case("cases")
            && case_args.is_empty()
            && module
                .class_name_for_receiver(receiver)
                .and_then(|class_name| module.enum_case_names(&class_name))
                .is_some() =>
        {
            let temp = module
                .next_label("array_transform_enum_cases")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, &args[0], module)?;
            if function_name.eq_ignore_ascii_case("array_values") {
                emit_value_array_copy_assign(name, &temp, module)
            } else if function_name.eq_ignore_ascii_case("array_reverse")
                && array_reverse_preserve_keys_arg(args, module)?
            {
                emit_indexed_array_reverse_preserve_keys_assign(name, &temp, ArrayLayout::Value, module)
            } else if function_name.eq_ignore_ascii_case("array_reverse") {
                emit_dynamic_value_array_transform_assign(
                    name,
                    &Expr::new(ExprKind::Variable(temp), args[0].span),
                    function_name,
                    module,
                )
            } else {
                let source_ptr = preserve_array_ptr(&temp, function_name, module);
                let len = module.array_length(&temp).ok_or_else(|| {
                    CompileError::new(
                        args[0].span,
                        "wasm32-web array_keys(Enum::cases()) requires enum case metadata",
                    )
                })?;
                emit_runtime_indexed_array_transform_assign(name, &source_ptr, len, function_name, module)
            }
        }
        ExprKind::Variable(source)
            if module.local_kind(source) == Some(LocalKind::Mixed)
                && module.mixed_value_cell_kind(source).is_none()
                && matches!(
                    function_name.to_ascii_lowercase().as_str(),
                    "array_values"
                        | "array_keys"
                        | "array_reverse"
                        | "array_unique"
                        | "array_flip"
                        | "array_diff"
                        | "array_intersect"
                ) =>
        {
            emit_unknown_mixed_array_transform_assign(name, source, function_name, args[0].span, args, module)
        }
        ExprKind::PropertyAccess { .. }
        | ExprKind::NullsafePropertyAccess { .. }
            if matches!(
                    function_name.to_ascii_lowercase().as_str(),
                    "array_values"
                        | "array_keys"
                        | "array_reverse"
                        | "array_unique"
                        | "array_flip"
                        | "array_diff"
                        | "array_intersect"
                ) =>
        {
            let temp = materialize_property_mixed_array_transform_source(
                &args[0],
                "property_mixed_array_transform_source",
                module,
            )?;
            emit_unknown_mixed_array_transform_assign(name, &temp, function_name, args[0].span, args, module)
        }
        ExprKind::DynamicPropertyAccess { .. }
        | ExprKind::NullsafeDynamicPropertyAccess { .. }
            if matches!(
                    function_name.to_ascii_lowercase().as_str(),
                    "array_values"
                        | "array_keys"
                        | "array_reverse"
                        | "array_unique"
                        | "array_flip"
                        | "array_diff"
                        | "array_intersect"
                ) =>
        {
            let temp = materialize_property_mixed_array_transform_source(
                &args[0],
                "dynamic_property_mixed_array_transform_source",
                module,
            )?;
            emit_unknown_mixed_array_transform_assign(name, &temp, function_name, args[0].span, args, module)
        }
        ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
            if function_name.eq_ignore_ascii_case("array_unique")
                && module.array_layout(source) == ArrayLayout::CompactInt
            {
                return emit_known_indexed_int_array_unique_assign(name, source, args, module);
            }
            if function_name.eq_ignore_ascii_case("array_unique")
                && module.array_layout(source) == ArrayLayout::Value
            {
                return emit_known_value_string_array_unique_assign(name, source, args, module);
            }
            if function_name.eq_ignore_ascii_case("array_unique")
                && module.array_layout(source) == ArrayLayout::Assoc
            {
                return emit_known_assoc_array_unique_assign(name, source, args, module);
            }
            if function_name.eq_ignore_ascii_case("array_flip")
                && module.array_layout(source) == ArrayLayout::CompactInt
            {
                return emit_known_indexed_int_array_flip_assign(name, source, args, module);
            }
            if function_name.eq_ignore_ascii_case("array_flip")
                && module.array_layout(source) == ArrayLayout::Value
            {
                return emit_known_value_string_array_flip_assign(name, source, args, module);
            }
            if function_name.eq_ignore_ascii_case("array_flip")
                && module.array_layout(source) == ArrayLayout::Assoc
            {
                return emit_known_assoc_array_flip_assign(name, source, args, module);
            }
            if matches!(
                function_name.to_ascii_lowercase().as_str(),
                "array_diff" | "array_intersect"
            ) && module.array_layout(source) == ArrayLayout::CompactInt
            {
                return emit_known_indexed_int_array_value_set_assign(
                    name,
                    source,
                    function_name,
                    args,
                    module,
                );
            }
            if matches!(
                function_name.to_ascii_lowercase().as_str(),
                "array_diff" | "array_intersect"
            ) && module.array_layout(source) == ArrayLayout::Value
            {
                return emit_known_value_string_array_value_set_assign(
                    name,
                    source,
                    function_name,
                    args,
                    module,
                );
            }
            if matches!(
                function_name.to_ascii_lowercase().as_str(),
                "array_diff" | "array_intersect"
            ) && module.array_layout(source) == ArrayLayout::Assoc
            {
                return emit_known_assoc_array_value_set_assign(
                    name,
                    source,
                    function_name,
                    args,
                    module,
                );
            }
            if matches!(
                function_name.to_ascii_lowercase().as_str(),
                "array_unique" | "array_flip" | "array_diff" | "array_intersect"
            ) {
                return Err(CompileError::new(
                    args[0].span,
                    &format!(
                        "wasm32-web {function_name}() currently supports direct indexed literals only"
                    ),
                ));
            }
            if module.array_layout(source) == ArrayLayout::Assoc {
                if function_name.eq_ignore_ascii_case("array_values") {
                    return emit_assoc_array_values_assign(name, source, module);
                }
                if function_name.eq_ignore_ascii_case("array_keys") {
                    return emit_assoc_array_keys_assign(name, source, module);
                }
                if function_name.eq_ignore_ascii_case("array_reverse") {
                    return emit_assoc_array_reverse_assign(
                        name,
                        source,
                        args[0].span,
                        array_reverse_preserve_keys_arg(args, module)?,
                        module,
                    );
                }
                return Err(CompileError::new(
                    args[0].span,
                    &format!(
                        "wasm32-web {function_name}() does not support associative arrays yet"
                    ),
                ));
            }
            if module.array_layout(source) == ArrayLayout::Value
                && !function_name.eq_ignore_ascii_case("array_keys")
            {
                if function_name.eq_ignore_ascii_case("array_reverse")
                    && array_reverse_preserve_keys_arg(args, module)?
                {
                    return emit_indexed_array_reverse_preserve_keys_assign(
                        name,
                        source,
                        ArrayLayout::Value,
                        module,
                    );
                }
                return emit_dynamic_value_array_transform_assign(
                    name,
                    &args[0],
                    function_name,
                    module,
                );
            }
            let Some(len) = module.array_length(source) else {
                return emit_dynamic_indexed_array_transform_assign(
                    name,
                    &args[0],
                    function_name,
                    module,
                );
            };
            if function_name.eq_ignore_ascii_case("array_reverse")
                && array_reverse_preserve_keys_arg(args, module)?
            {
                return emit_indexed_array_reverse_preserve_keys_assign(
                    name,
                    source,
                    ArrayLayout::CompactInt,
                    module,
                );
            }
            let source_ptr = preserve_array_ptr(source, function_name, module);
            emit_runtime_indexed_array_transform_assign(
                name,
                &source_ptr,
                len,
                function_name,
                module,
            )
        }
        ExprKind::FunctionCall { name: callee_name, args: call_args }
            if (function_name.eq_ignore_ascii_case("array_unique")
                || function_name.eq_ignore_ascii_case("array_flip"))
                && module.has_function(callee_name)
                && module.function_return_kind(callee_name) == Some(ValueKind::Array) =>
        {
            let temp = materialize_function_array_return_local(
                &args[0],
                callee_name,
                call_args,
                "array_flip_return",
                module,
            )?;
            match module.array_layout(&temp) {
                ArrayLayout::CompactInt if function_name.eq_ignore_ascii_case("array_unique") => {
                    emit_known_indexed_int_array_unique_assign(name, &temp, args, module)
                }
                ArrayLayout::CompactInt => emit_known_indexed_int_array_flip_assign(name, &temp, args, module),
                ArrayLayout::Value if function_name.eq_ignore_ascii_case("array_unique") => {
                    emit_known_value_string_array_unique_assign(name, &temp, args, module)
                }
                ArrayLayout::Value => emit_known_value_string_array_flip_assign(name, &temp, args, module),
                ArrayLayout::Assoc if function_name.eq_ignore_ascii_case("array_unique") => {
                    emit_known_assoc_array_unique_assign(name, &temp, args, module)
                }
                ArrayLayout::Assoc if function_name.eq_ignore_ascii_case("array_flip") => {
                    emit_known_assoc_array_flip_assign(name, &temp, args, module)
                }
                ArrayLayout::Assoc => unreachable!("associative array transform dispatch is exhaustive"),
            }
        }
        ExprKind::FunctionCall { name: callee_name, args: call_args }
            if matches!(
                function_name.to_ascii_lowercase().as_str(),
                "array_diff" | "array_intersect"
            )
                && module.has_function(callee_name)
                && module.function_return_kind(callee_name) == Some(ValueKind::Array) =>
        {
            let temp = materialize_function_array_return_local(
                &args[0],
                callee_name,
                call_args,
                "array_value_set_return",
                module,
            )?;
            match module.array_layout(&temp) {
                ArrayLayout::CompactInt => emit_known_indexed_int_array_value_set_assign(
                    name,
                    &temp,
                    function_name,
                    args,
                    module,
                ),
                ArrayLayout::Value => emit_known_value_string_array_value_set_assign(
                    name,
                    &temp,
                    function_name,
                    args,
                    module,
                ),
                ArrayLayout::Assoc => emit_known_assoc_array_value_set_assign(
                    name,
                    &temp,
                    function_name,
                    args,
                    module,
                ),
            }
        }
        ExprKind::FunctionCall { name: callee_name, .. }
            if module.has_function(callee_name)
                && module.function_return_kind(callee_name) == Some(ValueKind::Array)
                && module.function_array_return_layout(callee_name) == ArrayLayout::Value
                && !function_name.eq_ignore_ascii_case("array_keys")
                && !matches!(
                    function_name.to_ascii_lowercase().as_str(),
                    "array_unique" | "array_flip" | "array_diff" | "array_intersect"
                ) =>
        {
            emit_dynamic_value_array_transform_assign(name, &args[0], function_name, module)
        }
        ExprKind::FunctionCall { name: callee_name, .. }
            if matches!(
                function_name.to_ascii_lowercase().as_str(),
                "array_values" | "array_keys"
            ) && matches!(
                callee_name.to_ascii_lowercase().as_str(),
                "array_fill_keys" | "array_combine"
            ) =>
        {
            let temp = materialize_array_map_multi_source(&args[0], "array_transform_source", module)?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                if function_name.eq_ignore_ascii_case("array_values") {
                    emit_assoc_array_values_assign(name, &temp, module)
                } else {
                    emit_assoc_array_keys_assign(name, &temp, module)
                }
            } else {
                Err(CompileError::new(
                    args[0].span,
                    &format!(
                        "wasm32-web {function_name}() direct runtime builder requires an associative source"
                    ),
                ))
            }
        }
        ExprKind::FunctionCall { .. } | ExprKind::ExprCall { .. }
            if matches!(
                function_name.to_ascii_lowercase().as_str(),
                "array_values" | "array_keys"
            ) && expression_has_array_type(&args[0], module) =>
        {
            let temp = materialize_array_map_multi_source(&args[0], "array_transform_source", module)?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                if function_name.eq_ignore_ascii_case("array_values") {
                    emit_assoc_array_values_assign(name, &temp, module)
                } else {
                    emit_assoc_array_keys_assign(name, &temp, module)
                }
            } else if module.array_layout(&temp) == ArrayLayout::Value
                && function_name.eq_ignore_ascii_case("array_values")
            {
                let source = Expr::new(ExprKind::Variable(temp.clone()), args[0].span);
                let result = emit_dynamic_value_array_transform_assign(name, &source, function_name, module);
                module.set_array_value_cell_kinds(
                    name,
                    module.array_value_cell_kinds(&temp).map(|kinds| kinds.to_vec()),
                );
                module.set_array_value_constants(
                    name,
                    module.array_value_constants(&temp).map(|values| values.to_vec()),
                );
                module.set_array_nested_value_metadata(
                    name,
                    module
                        .array_nested_value_metadata_items(&temp)
                        .map(|items| items.to_vec()),
                );
                module.set_array_runtime_nested_value_metadata(
                    name,
                    module.array_runtime_nested_value_metadata(&temp),
                );
                result
            } else {
                emit_dynamic_indexed_array_transform_assign(name, &args[0], function_name, module)
            }
        }
        ExprKind::MethodCall { object, method, .. }
        | ExprKind::NullsafeMethodCall { object, method, .. }
            if matches!(
                function_name.to_ascii_lowercase().as_str(),
                "array_values" | "array_keys" | "array_reverse"
            ) && method_call_compact_int_array_return_len(object, method, module).is_some() =>
        {
            if function_name.eq_ignore_ascii_case("array_reverse")
                && array_reverse_preserve_keys_arg(args, module)?
            {
                let temp =
                    materialize_array_map_multi_source(&args[0], "array_reverse_source", module)?;
                return emit_indexed_array_reverse_preserve_keys_assign(
                    name,
                    &temp,
                    ArrayLayout::CompactInt,
                    module,
                );
            }
            emit_dynamic_indexed_array_transform_assign(name, &args[0], function_name, module)
        }
        ExprKind::MethodCall { object, method, .. }
        | ExprKind::NullsafeMethodCall { object, method, .. }
            if matches!(
                function_name.to_ascii_lowercase().as_str(),
                "array_values"
                    | "array_keys"
                    | "array_reverse"
                    | "array_unique"
                    | "array_flip"
                    | "array_diff"
                    | "array_intersect"
            ) && method_call_array_return_metadata(object, method, module).is_some() =>
        {
            let temp = materialize_array_map_multi_source(&args[0], "array_transform_method_source", module)?;
            module.set_array_value_constants(&temp, None);
            match module.array_layout(&temp) {
                ArrayLayout::Assoc => {
                    if function_name.eq_ignore_ascii_case("array_values") {
                        emit_assoc_array_values_assign(name, &temp, module)
                    } else if function_name.eq_ignore_ascii_case("array_reverse") {
                        emit_assoc_array_reverse_assign(
                            name,
                            &temp,
                            args[0].span,
                            array_reverse_preserve_keys_arg(args, module)?,
                            module,
                        )
                    } else if function_name.eq_ignore_ascii_case("array_unique") {
                        emit_known_assoc_array_unique_assign(name, &temp, args, module)
                    } else if function_name.eq_ignore_ascii_case("array_flip") {
                        if module.array_key_kinds(&temp).is_some() {
                            emit_known_assoc_array_flip_assign(name, &temp, args, module)
                        } else {
                            emit_unknown_assoc_array_flip_assign(name, &temp, module);
                            Ok(())
                        }
                    } else if matches!(
                        function_name.to_ascii_lowercase().as_str(),
                        "array_diff" | "array_intersect"
                    ) {
                        emit_known_assoc_array_value_set_assign(name, &temp, function_name, args, module)
                    } else {
                        emit_assoc_array_keys_assign(name, &temp, module)
                    }
                }
                ArrayLayout::Value if !function_name.eq_ignore_ascii_case("array_keys") => {
                    if function_name.eq_ignore_ascii_case("array_unique") {
                        emit_known_value_string_array_unique_assign(name, &temp, args, module)
                    } else if function_name.eq_ignore_ascii_case("array_flip") {
                        emit_known_value_string_array_flip_assign(name, &temp, args, module)
                    } else if matches!(
                        function_name.to_ascii_lowercase().as_str(),
                        "array_diff" | "array_intersect"
                    ) {
                        emit_known_value_string_array_value_set_assign(
                            name,
                            &temp,
                            function_name,
                            args,
                            module,
                        )
                    } else if function_name.eq_ignore_ascii_case("array_reverse")
                        && array_reverse_preserve_keys_arg(args, module)?
                    {
                        emit_indexed_array_reverse_preserve_keys_assign(
                            name,
                            &temp,
                            ArrayLayout::Value,
                            module,
                        )
                    } else {
                        emit_dynamic_value_array_transform_assign(
                            name,
                            &Expr::new(ExprKind::Variable(temp), args[0].span),
                            function_name,
                            module,
                        )
                    }
                }
                ArrayLayout::Value => emit_dynamic_indexed_array_transform_assign(
                    name,
                    &Expr::new(ExprKind::Variable(temp), args[0].span),
                    function_name,
                    module,
                ),
                ArrayLayout::CompactInt => {
                    if function_name.eq_ignore_ascii_case("array_unique") {
                        emit_known_indexed_int_array_unique_assign(name, &temp, args, module)
                    } else if function_name.eq_ignore_ascii_case("array_flip") {
                        emit_known_indexed_int_array_flip_assign(name, &temp, args, module)
                    } else if matches!(
                        function_name.to_ascii_lowercase().as_str(),
                        "array_diff" | "array_intersect"
                    ) {
                        emit_known_indexed_int_array_value_set_assign(
                            name,
                            &temp,
                            function_name,
                            args,
                            module,
                        )
                    } else if function_name.eq_ignore_ascii_case("array_reverse")
                        && array_reverse_preserve_keys_arg(args, module)?
                    {
                        emit_indexed_array_reverse_preserve_keys_assign(
                            name,
                            &temp,
                            ArrayLayout::CompactInt,
                            module,
                        )
                    } else {
                        emit_dynamic_indexed_array_transform_assign(
                            name,
                            &Expr::new(ExprKind::Variable(temp), args[0].span),
                            function_name,
                            module,
                        )
                    }
                }
            }
        }
        ExprKind::StaticMethodCall { receiver, method, .. }
            if matches!(
                function_name.to_ascii_lowercase().as_str(),
                "array_values"
                    | "array_keys"
                    | "array_reverse"
                    | "array_unique"
                    | "array_flip"
                    | "array_diff"
                    | "array_intersect"
            ) && static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            let temp = materialize_array_map_multi_source(
                &args[0],
                "array_transform_static_method_source",
                module,
            )?;
            match module.array_layout(&temp) {
                ArrayLayout::Assoc => {
                    if function_name.eq_ignore_ascii_case("array_values") {
                        emit_assoc_array_values_assign(name, &temp, module)
                    } else if function_name.eq_ignore_ascii_case("array_reverse") {
                        emit_assoc_array_reverse_assign(
                            name,
                            &temp,
                            args[0].span,
                            array_reverse_preserve_keys_arg(args, module)?,
                            module,
                        )
                    } else if function_name.eq_ignore_ascii_case("array_unique") {
                        emit_known_assoc_array_unique_assign(name, &temp, args, module)
                    } else if function_name.eq_ignore_ascii_case("array_flip") {
                        if module.array_key_kinds(&temp).is_some() {
                            emit_known_assoc_array_flip_assign(name, &temp, args, module)
                        } else {
                            emit_unknown_assoc_array_flip_assign(name, &temp, module);
                            Ok(())
                        }
                    } else if matches!(
                        function_name.to_ascii_lowercase().as_str(),
                        "array_diff" | "array_intersect"
                    ) {
                        emit_known_assoc_array_value_set_assign(name, &temp, function_name, args, module)
                    } else {
                        emit_assoc_array_keys_assign(name, &temp, module)
                    }
                }
                ArrayLayout::Value if !function_name.eq_ignore_ascii_case("array_keys") => {
                    if function_name.eq_ignore_ascii_case("array_unique") {
                        emit_known_value_string_array_unique_assign(name, &temp, args, module)
                    } else if function_name.eq_ignore_ascii_case("array_flip") {
                        emit_known_value_string_array_flip_assign(name, &temp, args, module)
                    } else if matches!(
                        function_name.to_ascii_lowercase().as_str(),
                        "array_diff" | "array_intersect"
                    ) {
                        emit_known_value_string_array_value_set_assign(
                            name,
                            &temp,
                            function_name,
                            args,
                            module,
                        )
                    } else if function_name.eq_ignore_ascii_case("array_reverse")
                        && array_reverse_preserve_keys_arg(args, module)?
                    {
                        emit_indexed_array_reverse_preserve_keys_assign(
                            name,
                            &temp,
                            ArrayLayout::Value,
                            module,
                        )
                    } else {
                        emit_dynamic_value_array_transform_assign(
                            name,
                            &Expr::new(ExprKind::Variable(temp), args[0].span),
                            function_name,
                            module,
                        )
                    }
                }
                ArrayLayout::Value => emit_dynamic_indexed_array_transform_assign(
                    name,
                    &Expr::new(ExprKind::Variable(temp), args[0].span),
                    function_name,
                    module,
                ),
                ArrayLayout::CompactInt => {
                    if function_name.eq_ignore_ascii_case("array_unique") {
                        emit_known_indexed_int_array_unique_assign(name, &temp, args, module)
                    } else if function_name.eq_ignore_ascii_case("array_flip") {
                        emit_known_indexed_int_array_flip_assign(name, &temp, args, module)
                    } else if matches!(
                        function_name.to_ascii_lowercase().as_str(),
                        "array_diff" | "array_intersect"
                    ) {
                        emit_known_indexed_int_array_value_set_assign(
                            name,
                            &temp,
                            function_name,
                            args,
                            module,
                        )
                    } else if function_name.eq_ignore_ascii_case("array_reverse")
                        && array_reverse_preserve_keys_arg(args, module)?
                    {
                        emit_indexed_array_reverse_preserve_keys_assign(
                            name,
                            &temp,
                            ArrayLayout::CompactInt,
                            module,
                        )
                    } else {
                        emit_dynamic_indexed_array_transform_assign(
                            name,
                            &Expr::new(ExprKind::Variable(temp), args[0].span),
                            function_name,
                            module,
                        )
                    }
                }
            }
        }
        ExprKind::ArrayAccess { .. }
            if matches!(
                function_name.to_ascii_lowercase().as_str(),
                "array_values"
                    | "array_keys"
                    | "array_reverse"
                    | "array_unique"
                    | "array_flip"
                    | "array_diff"
                    | "array_intersect"
            ) && nested_array_metadata_for_access_expr(&args[0], module).is_some() =>
        {
            let temp = materialize_nested_array_transform_source(
                &args[0],
                "array_transform_nested_source",
                module,
            )?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                if function_name.eq_ignore_ascii_case("array_values") {
                    emit_assoc_array_values_assign(name, &temp, module)
                } else if function_name.eq_ignore_ascii_case("array_reverse") {
                    emit_assoc_array_reverse_assign(
                        name,
                        &temp,
                        args[0].span,
                        array_reverse_preserve_keys_arg(args, module)?,
                        module,
                    )
                } else if function_name.eq_ignore_ascii_case("array_unique") {
                    emit_known_assoc_array_unique_assign(name, &temp, args, module)
                } else if function_name.eq_ignore_ascii_case("array_flip") {
                    if module.array_key_kinds(&temp).is_some() {
                        emit_known_assoc_array_flip_assign(name, &temp, args, module)
                    } else {
                        emit_unknown_assoc_array_flip_assign(name, &temp, module);
                        Ok(())
                    }
                } else if matches!(
                    function_name.to_ascii_lowercase().as_str(),
                    "array_diff" | "array_intersect"
                ) {
                    emit_known_assoc_array_value_set_assign(name, &temp, function_name, args, module)
                } else {
                    emit_assoc_array_keys_assign(name, &temp, module)
                }
            } else if module.array_layout(&temp) == ArrayLayout::Value
                && matches!(
                    function_name.to_ascii_lowercase().as_str(),
                    "array_values"
                        | "array_reverse"
                        | "array_unique"
                        | "array_flip"
                        | "array_diff"
                        | "array_intersect"
                )
            {
                if function_name.eq_ignore_ascii_case("array_unique") {
                    emit_known_value_string_array_unique_assign(name, &temp, args, module)
                } else if function_name.eq_ignore_ascii_case("array_flip") {
                    emit_known_value_string_array_flip_assign(name, &temp, args, module)
                } else if matches!(
                    function_name.to_ascii_lowercase().as_str(),
                    "array_diff" | "array_intersect"
                ) {
                    emit_known_value_string_array_value_set_assign(
                        name,
                        &temp,
                        function_name,
                        args,
                        module,
                    )
                } else if function_name.eq_ignore_ascii_case("array_reverse")
                    && array_reverse_preserve_keys_arg(args, module)?
                {
                    emit_indexed_array_reverse_preserve_keys_assign(
                        name,
                        &temp,
                        ArrayLayout::Value,
                        module,
                    )
                } else {
                    emit_dynamic_value_array_transform_assign(
                        name,
                        &Expr::new(ExprKind::Variable(temp), args[0].span),
                        function_name,
                        module,
                    )
                }
            } else {
                if function_name.eq_ignore_ascii_case("array_unique") {
                    emit_known_indexed_int_array_unique_assign(name, &temp, args, module)
                } else if function_name.eq_ignore_ascii_case("array_reverse")
                    && array_reverse_preserve_keys_arg(args, module)?
                {
                    emit_indexed_array_reverse_preserve_keys_assign(
                        name,
                        &temp,
                        ArrayLayout::CompactInt,
                        module,
                    )
                } else if matches!(
                    function_name.to_ascii_lowercase().as_str(),
                    "array_diff" | "array_intersect"
                ) {
                    emit_known_indexed_int_array_value_set_assign(
                        name,
                        &temp,
                        function_name,
                        args,
                        module,
                    )
                } else {
                    emit_dynamic_indexed_array_transform_assign(
                        name,
                        &Expr::new(ExprKind::Variable(temp), args[0].span),
                        function_name,
                        module,
                    )
                }
            }
        }
        ExprKind::FunctionCall { .. } | ExprKind::ExprCall { .. }
            if function_name.eq_ignore_ascii_case("array_reverse")
                && expression_has_array_type(&args[0], module) =>
        {
            let temp = materialize_array_map_multi_source(&args[0], "array_reverse_source", module)?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                emit_assoc_array_reverse_assign(
                    name,
                    &temp,
                    args[0].span,
                    array_reverse_preserve_keys_arg(args, module)?,
                    module,
                )
            } else if module.array_layout(&temp) == ArrayLayout::Value {
                if array_reverse_preserve_keys_arg(args, module)? {
                    emit_indexed_array_reverse_preserve_keys_assign(
                        name,
                        &temp,
                        ArrayLayout::Value,
                        module,
                    )
                } else {
                emit_dynamic_value_array_transform_assign(
                    name,
                    &Expr::new(ExprKind::Variable(temp), args[0].span),
                    function_name,
                    module,
                )
                }
            } else {
                if array_reverse_preserve_keys_arg(args, module)? {
                    emit_indexed_array_reverse_preserve_keys_assign(
                        name,
                        &temp,
                        ArrayLayout::CompactInt,
                        module,
                    )
                } else {
                    emit_dynamic_indexed_array_transform_assign(
                        name,
                        &Expr::new(ExprKind::Variable(temp), args[0].span),
                        function_name,
                        module,
                    )
                }
            }
        }
        ExprKind::FunctionCall { .. } | ExprKind::ExprCall { .. }
            if function_name.eq_ignore_ascii_case("array_unique")
                && expression_has_array_type(&args[0], module) =>
        {
            let temp = materialize_array_map_multi_source(&args[0], "array_unique_source", module)?;
            match module.array_layout(&temp) {
                ArrayLayout::Assoc => emit_known_assoc_array_unique_assign(name, &temp, args, module),
                ArrayLayout::Value => emit_known_value_string_array_unique_assign(name, &temp, args, module),
                ArrayLayout::CompactInt => emit_known_indexed_int_array_unique_assign(name, &temp, args, module),
            }
        }
        ExprKind::FunctionCall { .. } | ExprKind::ExprCall { .. }
            if function_name.eq_ignore_ascii_case("array_flip")
                && expression_has_array_type(&args[0], module) =>
        {
            let temp = materialize_array_map_multi_source(&args[0], "array_flip_source", module)?;
            match module.array_layout(&temp) {
                ArrayLayout::Assoc if module.array_value_cell_kinds(&temp).is_some() => {
                    emit_known_assoc_array_flip_assign(name, &temp, args, module)
                }
                ArrayLayout::Assoc => {
                    emit_unknown_assoc_array_flip_assign(name, &temp, module);
                    Ok(())
                }
                ArrayLayout::Value => emit_known_value_string_array_flip_assign(name, &temp, args, module),
                ArrayLayout::CompactInt => emit_known_indexed_int_array_flip_assign(name, &temp, args, module),
            }
        }
        ExprKind::FunctionCall { .. }
            if matches!(
                function_name.to_ascii_lowercase().as_str(),
                "array_diff" | "array_intersect"
            ) && expression_has_array_type(&args[0], module) =>
        {
            let temp = materialize_array_map_multi_source(&args[0], "array_value_set_source", module)?;
            match module.array_layout(&temp) {
                ArrayLayout::Assoc => emit_known_assoc_array_value_set_assign(
                    name,
                    &temp,
                    function_name,
                    args,
                    module,
                ),
                ArrayLayout::Value => emit_known_value_string_array_value_set_assign(
                    name,
                    &temp,
                    function_name,
                    args,
                    module,
                ),
                ArrayLayout::CompactInt => emit_known_indexed_int_array_value_set_assign(
                    name,
                    &temp,
                    function_name,
                    args,
                    module,
                ),
            }
        }
        _ if expression_is_arrayy(&args[0], module) => {
            if matches!(
                function_name.to_ascii_lowercase().as_str(),
                "array_unique" | "array_flip" | "array_diff" | "array_intersect"
            ) {
                return Err(CompileError::new(
                    args[0].span,
                    &format!(
                        "wasm32-web {function_name}() currently supports direct indexed literals only"
                    ),
                ));
            }
            emit_dynamic_indexed_array_transform_assign(name, &args[0], function_name, module)
        }
        _ => Err(CompileError::new(
            args[0].span,
            &format!("wasm32-web {function_name}() currently supports indexed array values only"),
        )),
    }
}

fn materialize_nested_array_transform_source(
    source: &Expr,
    label: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let Some(metadata) = nested_array_metadata_for_access_expr(source, module) else {
        return Err(CompileError::new(
            source.span,
            "wasm32-web array transform requires nested array metadata",
        ));
    };
    let temp = module.next_label(label).trim_start_matches('$').to_string();
    module.declare_array_local(temp.clone());
    match emit_expr(source, module)? {
        ValueKind::Array => {
            module.body().line(&format!("local.set ${}_len", temp));
            module.body().line(&format!("local.set ${}_ptr", temp));
        }
        _ => {
            return Err(CompileError::new(
                source.span,
                "wasm32-web array transform expected a nested array value",
            ));
        }
    }
    let key_values = metadata.key_values.clone();
    module.set_array_layout(&temp, metadata.layout);
    module.set_array_length(&temp, metadata.len);
    module.set_array_value_cell_kinds(&temp, metadata.value_kinds);
    module.set_array_key_values(&temp, key_values.clone());
    module.set_array_key_kinds(
        &temp,
        key_values.map(|keys| {
            keys.into_iter()
                .map(|key| match key {
                    AssocKeyValue::Int(_) => AssocKeyKind::Int,
                    AssocKeyValue::Str(_) => AssocKeyKind::Str,
                })
                .collect()
        }),
    );
    module.set_array_php_normalized_runtime_keys(
        &temp,
        metadata.layout == ArrayLayout::Assoc && module.array_key_kinds(&temp).is_none(),
    );
    module.set_array_nested_value_metadata(&temp, metadata.nested_values);
    Ok(temp)
}

fn materialize_property_mixed_array_transform_source(
    source: &Expr,
    label: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let temp = module.next_label(label).trim_start_matches('$').to_string();
    module.declare_i32_local(temp.clone());
    if emit_expr(source, module)? != ValueKind::Mixed {
        return Err(CompileError::new(
            source.span,
            "wasm32-web property array transform requires a mixed array property value",
        ));
    }
    module.body().line(&format!("local.set ${}", temp));
    Ok(temp)
}

fn emit_unknown_mixed_array_transform_assign(
    name: &str,
    source: &str,
    function_name: &str,
    span: crate::span::Span,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let preserve_keys = if function_name.eq_ignore_ascii_case("array_reverse") {
        array_reverse_preserve_keys_arg(args, module)?
    } else {
        false
    };
    let temp = module
        .next_label("unknown_mixed_array_transform_source")
        .trim_start_matches('$')
        .to_string();
    let heap_kind = module.next_label("unknown_mixed_array_transform_heap_kind");
    module.declare_array_local(temp.clone());
    module.declare_i32_local(heap_kind.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_ptr");
    module.body().line(&format!("local.set ${}_ptr", temp));
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_len_i32");
    module.body().line(&format!("local.set ${}_len", temp));
    module.body().line(&format!("local.get ${}", source));
    module.body().line("call $__rt_mixed_array_heap_kind");
    module.body().line(&format!("local.set {}", heap_kind));
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_INDEXED_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    if matches!(
        function_name.to_ascii_lowercase().as_str(),
        "array_diff" | "array_intersect"
    ) {
        module.set_array_layout(&temp, ArrayLayout::Value);
        module.set_array_value_cell_kinds(&temp, None);
        module.set_array_runtime_value_cell_kind(&temp, None);
        emit_known_value_string_array_value_set_assign(name, &temp, function_name, args, module)?;
    } else if function_name.eq_ignore_ascii_case("array_unique") {
        module.set_array_layout(&temp, ArrayLayout::Value);
        module.set_array_value_cell_kinds(&temp, None);
        module.set_array_runtime_value_cell_kind(&temp, None);
        emit_known_value_string_array_unique_assign(name, &temp, args, module)?;
    } else if function_name.eq_ignore_ascii_case("array_flip") {
        module.set_array_layout(&temp, ArrayLayout::Value);
        module.set_array_value_cell_kinds(&temp, None);
        module.set_array_runtime_value_cell_kind(&temp, None);
        emit_known_value_string_array_flip_assign(name, &temp, args, module)?;
    } else if function_name.eq_ignore_ascii_case("array_values") {
        emit_unknown_mixed_indexed_array_values_assign(name, &temp, module);
    } else if function_name.eq_ignore_ascii_case("array_reverse") {
        emit_unknown_mixed_indexed_array_reverse_assign(name, &temp, preserve_keys, module);
    } else {
        emit_unknown_mixed_indexed_array_keys_value_assign(name, &temp, module);
    }
    module.body().close("else");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.set_array_layout(&temp, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(&temp, None);
    module.set_array_key_kinds(&temp, None);
    module.set_array_php_normalized_runtime_keys(&temp, true);
    if matches!(
        function_name.to_ascii_lowercase().as_str(),
        "array_diff" | "array_intersect"
    ) {
        emit_known_assoc_array_value_set_assign(name, &temp, function_name, args, module)?;
    } else if function_name.eq_ignore_ascii_case("array_unique") {
        emit_known_assoc_array_unique_assign(name, &temp, args, module)?;
    } else if function_name.eq_ignore_ascii_case("array_flip") {
        emit_unknown_assoc_array_flip_assign(name, &temp, module);
    } else if function_name.eq_ignore_ascii_case("array_values") {
        emit_assoc_array_values_assign(name, &temp, module)?;
    } else if function_name.eq_ignore_ascii_case("array_reverse") {
        emit_assoc_array_reverse_assign(name, &temp, span, preserve_keys, module)?;
    } else {
        emit_assoc_array_keys_assign(name, &temp, module)?;
    }
    module.body().close("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_unknown_mixed_indexed_array_reverse_assign(
    name: &str,
    source: &str,
    preserve_keys: bool,
    module: &mut WasmModule,
) {
    let index = module.next_label("unknown_mixed_indexed_reverse_index");
    let source_index = module.next_label("unknown_mixed_indexed_reverse_source_index");
    let target_entry = module.next_label("unknown_mixed_indexed_reverse_target_entry");
    let target_cell = module.next_label("unknown_mixed_indexed_reverse_target_cell");
    let source_cell = module.next_label("unknown_mixed_indexed_reverse_source_cell");
    let done_label = module.next_label("unknown_mixed_indexed_reverse_done");
    let loop_label = module.next_label("unknown_mixed_indexed_reverse_loop");
    for local in [&index, &source_index, &target_entry, &target_cell, &source_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_key_kinds(name, None);
    module.set_array_runtime_key_kind(name, Some(AssocKeyKind::Int));
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, None);
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", source_index));
    emit_assoc_entry_address(&format!("${}_ptr", name), &index, &target_entry, module);
    module.body().line(&format!("local.get {}", target_entry));
    if preserve_keys {
        module.body().line(&format!("local.get {}", source_index));
    } else {
        module.body().line(&format!("local.get {}", index));
    }
    module.body().line("i64.extend_i32_u");
    module.body().line("call $__rt_assoc_store_int_key");
    emit_assoc_value_cell_address(&target_entry, &target_cell, module);
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", source_cell));
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get {}", source_cell));
    module.body().line("call $__rt_value_copy");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_unknown_mixed_indexed_array_values_assign(
    name: &str,
    source: &str,
    module: &mut WasmModule,
) {
    let index = module.next_label("unknown_mixed_indexed_values_index");
    let target_cell = module.next_label("unknown_mixed_indexed_values_target_cell");
    let source_cell = module.next_label("unknown_mixed_indexed_values_source_cell");
    let done_label = module.next_label("unknown_mixed_indexed_values_done");
    let loop_label = module.next_label("unknown_mixed_indexed_values_loop");
    for local in [&index, &target_cell, &source_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, None);
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get ${}_len", source));
    module
        .body()
        .line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address(&format!("${}_ptr", name), &index, &target_cell, module);
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", source_cell));
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get {}", source_cell));
    module.body().line("call $__rt_value_copy");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_unknown_mixed_indexed_array_keys_value_assign(
    name: &str,
    source: &str,
    module: &mut WasmModule,
) {
    let index = module.next_label("unknown_mixed_indexed_keys_index");
    let cell = module.next_label("unknown_mixed_indexed_keys_cell");
    let done_label = module.next_label("unknown_mixed_indexed_keys_done");
    let loop_label = module.next_label("unknown_mixed_indexed_keys_loop");
    for local in [&index, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, None);
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get ${}_len", source));
    module
        .body()
        .line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address(&format!("${}_ptr", name), &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i64.extend_i32_u");
    module.body().line("call $__rt_value_store_int");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}
