//! Purpose:
//! Lowers wasm32-web expression output, including echo/print-style string, scalar, array, and mixed values.
//! Keeps output-specific branching out of the expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::stmt` for echo statements.
//! - `crate::codegen::wasm::expr` and output helper modules for nested output expressions.
//!
//! Key details:
//! - Preserves PHP output coercion behavior and delegates runtime-backed cases to focused helpers.

use super::*;

pub(in crate::codegen::wasm) fn emit_output_expr(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let ExprKind::Assignment { target, .. } = &expr.kind {
        if matches!(
            &target.kind,
            ExprKind::PropertyAccess { .. } | ExprKind::DynamicPropertyAccess { .. }
        ) {
            let kind = emit_expr(expr, module)?;
            return emit_output_loaded_kind(expr, kind, module);
        }
    }
    if let Some(err) = unsupported_string_coercion_in_string_context(expr) {
        return Err(err);
    }
    if let Some(err) = unsupported_object_string_coercion_in_string_context(expr, module) {
        return Err(err);
    }
    if emit_object_tostring_value_to_stack(expr, module)? {
        module.body().line("call $host_write");
        return Ok(());
    }
    match &expr.kind {
        ExprKind::StringLiteral(value) => {
            let (ptr, len) = module.intern_string(value);
            module.body().line(&format!("i32.const {}", ptr));
            module.body().line(&format!("i32.const {}", len));
            module.body().line("call $host_write");
            Ok(())
        }
        ExprKind::NullsafePropertyAccess { object, .. }
        | ExprKind::NullsafeDynamicPropertyAccess { object, .. }
        | ExprKind::NullsafeMethodCall { object, .. }
            if matches!(object.kind, ExprKind::Null) =>
        {
            Ok(())
        }
        ExprKind::ConstRef(name) => emit_output_constant(expr, name, module),
        ExprKind::ClassConstant { receiver } => emit_output_class_name(expr, receiver, module),
        ExprKind::ScopedConstantAccess { receiver, name } => {
            emit_output_scoped_constant(expr, receiver, name, module)
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Str) => {
            module.body().line(&format!("local.get ${}_ptr", name));
            module.body().line(&format!("local.get ${}_len", name));
            module.body().line("call $host_write");
            Ok(())
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Array) => {
            emit_output_array_marker(module);
            Ok(())
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Mixed) => {
            emit_output_value_cell(&format!("${}", name), module);
            Ok(())
        }
        ExprKind::ArrayLiteral(_) => {
            emit_output_array_marker(module);
            Ok(())
        }
        ExprKind::ArrayLiteralAssoc(_) => {
            emit_output_array_marker(module);
            Ok(())
        }
        ExprKind::ArrayAccess { array, .. }
            if matches!(&array.kind, ExprKind::StaticPropertyAccess { .. }) =>
        {
            let Some(cell) = materialize_mixed_value_cell(expr, module)? else {
                return Err(array_unsupported(expr));
            };
            emit_output_value_cell(&format!("${}", cell), module);
            Ok(())
        }
        ExprKind::ArrayAccess { array, index } if nested_array_access_requires_layout(array) => {
            emit_output_nested_array_index(expr, array, index, module)
        }
        ExprKind::ArrayAccess { array, index }
            if expression_is_arrayy(array, module) || expression_has_array_type(array, module) =>
        {
            emit_output_array_index(expr, array, index, module)
        }
        ExprKind::ArrayAccess { array, index } => {
            if let Some(cell) = materialize_mixed_value_cell(expr, module)? {
                emit_output_value_cell(&format!("${}", cell), module);
                return Ok(());
            }
            emit_output_string_index(expr, array, index, module)
        }
        ExprKind::PropertyAccess { .. } | ExprKind::DynamicPropertyAccess { .. } => {
            let kind = emit_expr(expr, module)?;
            emit_output_loaded_kind(expr, kind, module)
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, .. }
            if object_expr_is_known_non_null(object, module) =>
        {
            let kind = emit_expr(expr, module)?;
            emit_output_loaded_kind(expr, kind, module)
        }
        ExprKind::NullsafePropertyAccess { .. } => {
            let kind = emit_expr(expr, module)?;
            emit_output_loaded_kind(expr, kind, module)
        }
        ExprKind::NullsafeDynamicPropertyAccess { .. } => {
            let kind = emit_expr(expr, module)?;
            emit_output_loaded_kind(expr, kind, module)
        }
        ExprKind::StaticPropertyAccess { .. } => {
            let kind = emit_expr(expr, module)?;
            emit_output_loaded_kind(expr, kind, module)
        }
        ExprKind::BinaryOp {
            left,
            op: BinOp::Concat,
            right,
        } => {
            if let Some(err) = unsupported_string_coercion_expr(left) {
                return Err(err);
            }
            if let Some(err) = unsupported_string_coercion_expr(right) {
                return Err(err);
            }
            if concat_output_needs_value_materialization(expr) && expression_is_stringy(expr, module) {
                emit_string_value_to_stack(expr, module)?;
                module.body().line("call $host_write");
                return Ok(());
            }
            emit_output_expr(left, module)?;
            emit_output_expr(right, module)
        }
        ExprKind::BoolLiteral(false) | ExprKind::Null => Ok(()),
        ExprKind::BoolLiteral(true) => {
            module.body().line("i64.const 1");
            module.body().line("call $host_write_int");
            Ok(())
        }
        ExprKind::Cast {
            target: CastType::String,
            expr,
        } => {
            emit_string_cast_value_to_stack(expr, expr, module)?;
            module.body().line("call $host_write");
            Ok(())
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("gettype") => {
            emit_gettype_output_call(expr, args, module)
        }
        ExprKind::FunctionCall { name, args } if is_type_predicate_name(name) => {
            emit_type_predicate_call(expr, name, args, module)?;
            emit_output_bool_from_stack(module);
            Ok(())
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("empty") => {
            emit_empty_call(expr, args, module)?;
            emit_output_bool_from_stack(module);
            Ok(())
        }
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("array_reduce") && array_reduce_call_is_string(args, module) =>
        {
            emit_string_value_to_stack(expr, module)?;
            module.body().line("call $host_write");
            Ok(())
        }
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("call_user_func")
                && call_user_func_return_kind(args, module) == Some(ValueKind::Str) =>
        {
            match emit_call_user_func_call(expr, args, module)? {
                ValueKind::Str => {
                    module.body().line("call $host_write");
                    Ok(())
                }
                _ => unreachable!("string call_user_func target must return a string"),
            }
        }
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("call_user_func_array")
                && call_user_func_array_return_kind(args, module) == Some(ValueKind::Str) =>
        {
            match emit_call_user_func_array_call(expr, args, module)? {
                ValueKind::Str => {
                    module.body().line("call $host_write");
                    Ok(())
                }
                _ => unreachable!("string call_user_func_array target must return a string"),
            }
        }
        ExprKind::FunctionCall { name, args } if is_output_string_builtin(name.as_str()) => {
            emit_output_string_builtin(expr, name.as_str(), args, module)
        }
        ExprKind::FunctionCall { name, args } if is_output_optional_int_builtin(name.as_str()) => {
            emit_output_optional_int_builtin(expr, name.as_str(), args, module)
        }
        ExprKind::FunctionCall { name, args }
            if matches!(name.to_ascii_lowercase().as_str(), "array_pop" | "array_shift")
                && is_unknown_mixed_array_single_arg(args, module) =>
        {
            emit_output_unknown_mixed_array_pop_shift(expr, module)
        }
        ExprKind::FunctionCall { name, args }
            if matches!(name.to_ascii_lowercase().as_str(), "array_pop" | "array_shift")
                && is_assoc_array_single_arg(args, module) =>
        {
            emit_output_assoc_array_pop_shift(expr, name, args, module)
        }
        ExprKind::FunctionCall { name, args }
            if matches!(name.to_ascii_lowercase().as_str(), "array_pop" | "array_shift")
                && is_value_array_single_arg(args, module) =>
        {
            emit_output_value_array_pop_shift(expr, name, args, module)
        }
        _ if expression_has_array_type(expr, module) => {
            let temp = module
                .next_label("output_array_expr")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, expr, module)?;
            emit_output_array_marker(module);
            Ok(())
        }
        ExprKind::FunctionCall { name, args }
            if module.has_function(name) && module.function_return_kind(name) == Some(ValueKind::Str) =>
        {
            emit_user_function_args(expr, name, args, module)?;
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(name)));
            module.body().line("call $host_write");
            Ok(())
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        } if method_call_return_kind(object, method, module) == Some(ValueKind::Str) => {
            match emit_method_call_expr(expr, object, method, args, module)? {
                ValueKind::Str => {
                    module.body().line("call $host_write");
                    Ok(())
                }
                _ => unreachable!("string method call target must return a string"),
            }
        }
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } if object_expr_is_known_non_null(object, module)
            && method_call_return_kind(object, method, module) == Some(ValueKind::Str) =>
        {
            match emit_method_call_expr(expr, object, method, args, module)? {
                ValueKind::Str => {
                    module.body().line("call $host_write");
                    Ok(())
                }
                _ => unreachable!("string method call target must return a string"),
            }
        }
        ExprKind::NullsafeMethodCall { .. } => {
            let kind = emit_expr(expr, module)?;
            emit_output_loaded_kind(expr, kind, module)
        }
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } if static_method_call_return_kind(receiver, method, module) == Some(ValueKind::Str) => {
            match emit_static_method_call_expr(expr, receiver, method, args, module)? {
                ValueKind::Str => {
                    module.body().line("call $host_write");
                    Ok(())
                }
                _ => unreachable!("string static method call target must return a string"),
            }
        }
        ExprKind::ClosureCall { var, args }
            if callable_variable_return_kind(module, var, args) == Some(ValueKind::Str) =>
        {
            match emit_callable_variable_call(expr, var, args, module)? {
                ValueKind::Str => {
                    module.body().line("call $host_write");
                    Ok(())
                }
                _ => unreachable!("string callable target must return a string"),
            }
        }
        ExprKind::Assignment { .. } if expression_is_stringy(expr, module) => {
            emit_string_value_to_stack(expr, module)?;
            module.body().line("call $host_write");
            Ok(())
        }
        ExprKind::Pipe { .. } if expression_is_stringy(expr, module) => {
            emit_string_value_to_stack(expr, module)?;
            module.body().line("call $host_write");
            Ok(())
        }
        ExprKind::NullCoalesce { value, default } => {
            if matches!(value.kind, ExprKind::Null) {
                emit_output_expr(default, module)
            } else if expression_is_stringy(value, module) {
                emit_output_expr(value, module)
            } else {
                emit_output_expr(value, module)
            }
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } if expression_is_stringy(then_expr, module) || expression_is_stringy(else_expr, module) => {
            emit_output_ternary(condition, then_expr, else_expr, module)
        }
        ExprKind::ShortTernary { value, default }
            if expression_is_stringy(value, module) || expression_is_stringy(default, module) =>
        {
            emit_output_short_ternary(value, default, module)
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => emit_output_match(expr, subject, arms, default.as_deref(), module),
        _ => {
            let kind = emit_expr(expr, module)?;
            match kind {
                ValueKind::Int => module.body().line("call $host_write_int"),
                ValueKind::Float => module.body().line("call $host_write_float"),
                ValueKind::Bool => {
                    module.body().open("if");
                    module.body().line("i64.const 1");
                    module.body().line("call $host_write_int");
                    module.body().close("end");
                }
                ValueKind::Null => {}
                ValueKind::Str => unreachable!("string values are handled before emit_expr"),
                ValueKind::Array => {
                    module.body().line("drop");
                    module.body().line("drop");
                    emit_output_array_marker(module);
                }
                ValueKind::Object => {
                    return Err(CompileError::new(
                        expr.span,
                        "wasm32-web object output requires __toString support",
                    ));
                }
                ValueKind::Mixed => {
                    let (array_marker_ptr, _) = module.intern_string("Array");
                    module.body().line(&format!("i32.const {}", array_marker_ptr));
                    module.body().line("call $__rt_output_value_cell");
                }
                ValueKind::Never => module.body().line("unreachable"),
            }
            Ok(())
        }
    }
}

fn emit_output_loaded_kind(
    expr: &Expr,
    kind: ValueKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match kind {
        ValueKind::Int => module.body().line("call $host_write_int"),
        ValueKind::Float => module.body().line("call $host_write_float"),
        ValueKind::Bool => {
            module.body().open("if");
            module.body().line("i64.const 1");
            module.body().line("call $host_write_int");
            module.body().close("end");
        }
        ValueKind::Null => {}
        ValueKind::Str => module.body().line("call $host_write"),
        ValueKind::Array => {
            module.body().line("drop");
            module.body().line("drop");
            emit_output_array_marker(module);
        }
        ValueKind::Object => {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web object output requires __toString support",
            ));
        }
        ValueKind::Mixed => {
            let (array_marker_ptr, _) = module.intern_string("Array");
            module.body().line(&format!("i32.const {}", array_marker_ptr));
            module.body().line("call $__rt_output_value_cell");
        }
        ValueKind::Never => module.body().line("unreachable"),
    }
    Ok(())
}

fn is_type_predicate_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "is_int" | "is_float" | "is_bool" | "is_null" | "is_string" | "is_iterable" | "is_object"
    )
}

fn emit_output_bool_from_stack(module: &mut WasmModule) {
    module.body().open("if");
    module.body().line("i64.const 1");
    module.body().line("call $host_write_int");
    module.body().close("end");
}

fn is_unknown_mixed_array_single_arg(args: &[Expr], module: &WasmModule) -> bool {
    let [arg] = args else {
        return false;
    };
    let ExprKind::Variable(name) = &arg.kind else {
        return false;
    };
    module.local_kind(name) == Some(LocalKind::Mixed) && module.mixed_value_cell_kind(name).is_none()
}

fn emit_output_unknown_mixed_array_pop_shift(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("output_unknown_mixed_array_pop_shift")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(temp.clone());
    module.body().line("call $__rt_alloc_null_mixed_cell");
    module.body().line(&format!("local.set ${}", temp));
    emit_mixed_assign(&temp, expr, module)?;
    emit_output_value_cell(&format!("${}", temp), module);
    Ok(())
}

fn concat_output_needs_value_materialization(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::BinaryOp {
            op: BinOp::Concat,
            left,
            right,
        } => {
            concat_output_needs_value_materialization(left)
                || concat_output_needs_value_materialization(right)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("get_class") => {
            args.iter().any(expr_has_call_side_effect)
        }
        _ => false,
    }
}

fn expr_has_call_side_effect(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::FunctionCall { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::NullsafeMethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::NewObject { .. }
        | ExprKind::NewScopedObject { .. }
        | ExprKind::ClosureCall { .. } => true,
        ExprKind::ArrayAccess { array, index } => {
            expr_has_call_side_effect(array) || expr_has_call_side_effect(index)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. }
        | ExprKind::DynamicPropertyAccess { object, .. }
        | ExprKind::NullsafeDynamicPropertyAccess { object, .. } => expr_has_call_side_effect(object),
        ExprKind::BinaryOp { left, right, .. } => {
            expr_has_call_side_effect(left) || expr_has_call_side_effect(right)
        }
        ExprKind::Cast { expr, .. }
        | ExprKind::Negate(expr)
        | ExprKind::Not(expr)
        | ExprKind::BitNot(expr)
        | ExprKind::Print(expr)
        | ExprKind::Spread(expr) => expr_has_call_side_effect(expr),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_has_call_side_effect(condition)
                || expr_has_call_side_effect(then_expr)
                || expr_has_call_side_effect(else_expr)
        }
        ExprKind::ShortTernary { value, default }
        | ExprKind::NullCoalesce { value, default } => {
            expr_has_call_side_effect(value) || expr_has_call_side_effect(default)
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_has_call_side_effect(subject)
                || arms.iter().any(|(conditions, value)| {
                    conditions.iter().any(expr_has_call_side_effect)
                        || expr_has_call_side_effect(value)
                })
                || default
                    .as_ref()
                    .is_some_and(|default| expr_has_call_side_effect(default))
        }
        ExprKind::NamedArg { value, .. } => expr_has_call_side_effect(value),
        _ => false,
    }
}
