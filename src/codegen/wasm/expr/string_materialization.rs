//! Purpose:
//! Lowers wasm32-web string assignment and runtime string materialization.
//! Keeps string locals, offset mutation, casts, concat, and string argument staging out of expr.rs.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` dispatch and sibling builtin/array helpers.
//!
//! Key details:
//! - Helpers preserve PHP coercion checks, heap string layout, and ptr/len stack ordering.

use super::*;
use super::string_builtin_materialization::emit_string_builtin_value_to_stack_if_supported;
use super::string_arg_materialization::*;
use super::string_cast_materialization::*;

pub(in crate::codegen::wasm) fn emit_string_assign(
    name: &str,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(err) = unsupported_string_coercion_in_string_context(value) {
        return Err(err);
    }
    if let Some(err) = unsupported_object_string_coercion_in_string_context(value, module) {
        return Err(err);
    }
    if let Some(value) = static_string_value(value, module) {
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("local.set ${}_ptr", name));
        module.body().line(&format!("i32.const {}", len));
        module.body().line(&format!("local.set ${}_len", name));
        module.set_string_static_value(name, Some(value));
        return Ok(());
    }
    if let Some(err) = unsupported_object_string_coercion_expr(value, module) {
        return Err(err);
    }
    module.set_string_static_value(name, None);
    if emit_object_tostring_value_to_stack(value, module)? {
        module.body().line(&format!("local.set ${}_len", name));
        module.body().line(&format!("local.set ${}_ptr", name));
        return Ok(());
    }
    if let ExprKind::FunctionCall { name: function_name, args } = &value.kind {
        if function_name.eq_ignore_ascii_case("array_reduce") && array_reduce_call_is_string(args, module) {
            emit_string_value_to_stack(value, module)?;
            module.body().line(&format!("local.set ${}_len", name));
            module.body().line(&format!("local.set ${}_ptr", name));
            return Ok(());
        }
        if emit_string_builtin_value_to_stack_if_supported(
            value,
            function_name.as_str(),
            args,
            module,
        )? {
            module.body().line(&format!("local.set ${}_len", name));
            module.body().line(&format!("local.set ${}_ptr", name));
            return Ok(());
        }
    }

    match &value.kind {
        ExprKind::StringLiteral(value) => {
            let (ptr, len) = module.intern_string(value);
            module.body().line(&format!("i32.const {}", ptr));
            module.body().line(&format!("local.set ${}_ptr", name));
            module.body().line(&format!("i32.const {}", len));
            module.body().line(&format!("local.set ${}_len", name));
            Ok(())
        }
        ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Str) => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.set ${}_ptr", name));
            module.body().line(&format!("local.get ${}_len", source));
            module.body().line(&format!("local.set ${}_len", name));
            module.set_string_static_value(name, module.string_static_value(source));
            Ok(())
        }
        ExprKind::PropertyAccess { object, property }
        | ExprKind::NullsafePropertyAccess { object, property }
            if object_property_value_kind(object, property, module) == Some(ValueKind::Str) =>
        {
            match emit_expr(value, module)? {
                ValueKind::Str => {
                    module.body().line(&format!("local.set ${}_len", name));
                    module.body().line(&format!("local.set ${}_ptr", name));
                    Ok(())
                }
                _ => unreachable!("string property metadata must load a string"),
            }
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property }
            if object_dynamic_property_value_kind(object, property, module) == Some(ValueKind::Str) =>
        {
            match emit_expr(value, module)? {
                ValueKind::Str => {
                    module.body().line(&format!("local.set ${}_len", name));
                    module.body().line(&format!("local.set ${}_ptr", name));
                    Ok(())
                }
                _ => unreachable!("string property metadata must load a string"),
            }
        }
        ExprKind::Assignment { target, .. } if expression_is_stringy(target, module) => {
            match emit_expr(value, module)? {
                ValueKind::Str => {
                    module.body().line(&format!("local.set ${}_len", name));
                    module.body().line(&format!("local.set ${}_ptr", name));
                    Ok(())
                }
                _ => unreachable!("string assignment target metadata must load a string"),
            }
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        } if method_call_return_kind(object, method, module) == Some(ValueKind::Str) => {
            match emit_method_call_expr(value, object, method, args, module)? {
                ValueKind::Str => {
                    module.body().line(&format!("local.set ${}_len", name));
                    module.body().line(&format!("local.set ${}_ptr", name));
                    Ok(())
                }
                _ => unreachable!("string object method target must return a string"),
            }
        }
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } if object_expr_is_known_non_null(object, module)
            && method_call_return_kind(object, method, module) == Some(ValueKind::Str) =>
        {
            match emit_method_call_expr(value, object, method, args, module)? {
                ValueKind::Str => {
                    module.body().line(&format!("local.set ${}_len", name));
                    module.body().line(&format!("local.set ${}_ptr", name));
                    Ok(())
                }
                _ => unreachable!("string object method target must return a string"),
            }
        }
        ExprKind::MethodCall { object, method, args } => {
            match emit_method_call_expr(value, object, method, args, module)? {
                ValueKind::Str => {
                    module.body().line(&format!("local.set ${}_len", name));
                    module.body().line(&format!("local.set ${}_ptr", name));
                    Ok(())
                }
                _ => Err(CompileError::new(
                    value.span,
                    "wasm32-web string assignment requires a string-returning method call",
                )),
            }
        }
        ExprKind::FunctionCall { name: function_name, args }
            if module.has_function(function_name)
                && module.function_return_kind(function_name) == Some(ValueKind::Str) =>
        {
            emit_user_function_args(value, function_name, args, module)?;
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(function_name)));
            module.body().line(&format!("local.set ${}_len", name));
            module.body().line(&format!("local.set ${}_ptr", name));
            module.set_string_static_value(
                name,
                module
                    .function_static_string_return_for_call(function_name, args)
                    .or_else(|| module.function_static_string_return(function_name)),
            );
            Ok(())
        }
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } if static_method_call_return_kind(receiver, method, module) == Some(ValueKind::Str) => {
            match emit_static_method_call_expr(value, receiver, method, args, module)? {
                ValueKind::Str => {
                    module.body().line(&format!("local.set ${}_len", name));
                    module.body().line(&format!("local.set ${}_ptr", name));
                    Ok(())
                }
                _ => unreachable!("string static method target must return a string"),
            }
        }
        ExprKind::ExprCall { callee, args }
            if callable_expr_return_kind(module, callee, args) == Some(ValueKind::Str) =>
        {
            match emit_callable_expr_call(value, callee, args, module)? {
                ValueKind::Str => {
                    module.body().line(&format!("local.set ${}_len", name));
                    module.body().line(&format!("local.set ${}_ptr", name));
                    Ok(())
                }
                _ => unreachable!("string callable expression target must return a string"),
            }
        }
        ExprKind::ClosureCall { var, args }
            if callable_variable_return_kind(module, var, args) == Some(ValueKind::Str) =>
        {
            match emit_callable_variable_call(value, var, args, module)? {
                ValueKind::Str => {
                    module.body().line(&format!("local.set ${}_len", name));
                    module.body().line(&format!("local.set ${}_ptr", name));
                    Ok(())
                }
                _ => unreachable!("string callable variable target must return a string"),
            }
        }
        ExprKind::FunctionCall { name: function_name, .. }
            if matches!(
                function_name.to_ascii_lowercase().as_str(),
                "call_user_func" | "call_user_func_array"
            ) && expression_is_stringy(value, module) =>
        {
            emit_string_value_to_locals(
                value,
                &format!("${}_ptr", name),
                &format!("${}_len", name),
                module,
            )
        }
        ExprKind::ArrayAccess { array, .. }
            if expression_is_arrayy(array, module)
                || expression_has_array_type(array, module)
                || matches!(
                    value.kind,
                    ExprKind::ArrayAccess { ref array, ref index }
                        if dynamic_outer_nested_assoc_static_access_kind(array, index, module)
                            == Some(ValueCellKind::Str)
                            || dynamic_parent_nested_assoc_static_access_kind(array, index, module)
                                == Some(ValueCellKind::Str)
                )
                || expression_is_stringy(value, module) =>
        {
            emit_string_value_to_locals(
                value,
                &format!("${}_ptr", name),
                &format!("${}_len", name),
                module,
            )
        }
        ExprKind::BinaryOp {
            op: BinOp::Concat, ..
        } if expression_is_stringy(value, module) => {
            emit_string_value_to_locals(
                value,
                &format!("${}_ptr", name),
                &format!("${}_len", name),
                module,
            )
        }
        ExprKind::Cast {
            target: CastType::String,
            ..
        } if expression_is_stringy(value, module) => {
            emit_string_value_to_locals(
                value,
                &format!("${}_ptr", name),
                &format!("${}_len", name),
                module,
            )
        }
        ExprKind::Ternary { .. } | ExprKind::ShortTernary { .. } | ExprKind::Match { .. }
            if expression_is_stringy(value, module) =>
        {
            emit_string_value_to_locals(
                value,
                &format!("${}_ptr", name),
                &format!("${}_len", name),
                module,
            )
        }
        ExprKind::ConstRef(const_name) => {
            let Some(ConstantValue::Str(value)) = module.constant_value(const_name) else {
                return Err(CompileError::new(
                    value.span,
                    "wasm32-web string assignments currently require a string literal, string constant, or string variable",
                ));
            };
            let (ptr, len) = module.intern_string(&value);
            module.body().line(&format!("i32.const {}", ptr));
            module.body().line(&format!("local.set ${}_ptr", name));
            module.body().line(&format!("i32.const {}", len));
            module.body().line(&format!("local.set ${}_len", name));
            Ok(())
        }
        ExprKind::ClassConstant { receiver } => {
            let Some(class_name) = module.class_name_for_receiver(receiver) else {
                return Err(CompileError::new(
                    value.span,
                    "wasm32-web ::class currently requires a named class receiver",
                ));
            };
            let (ptr, len) = module.intern_string(&class_name);
            module.body().line(&format!("i32.const {}", ptr));
            module.body().line(&format!("local.set ${}_ptr", name));
            module.body().line(&format!("i32.const {}", len));
            module.body().line(&format!("local.set ${}_len", name));
            Ok(())
        }
        ExprKind::ScopedConstantAccess {
            receiver,
            name: const_name,
        } => {
            let Some(ConstantValue::Str(value)) =
                module.class_constant_value(receiver, const_name)
            else {
                return Err(CompileError::new(
                    value.span,
                    "wasm32-web string assignments currently require a string literal, string constant, class name, class string constant, or string variable",
                ));
            };
            let (ptr, len) = module.intern_string(&value);
            module.body().line(&format!("i32.const {}", ptr));
            module.body().line(&format!("local.set ${}_ptr", name));
            module.body().line(&format!("i32.const {}", len));
            module.body().line(&format!("local.set ${}_len", name));
            Ok(())
        }
        ExprKind::NullCoalesce { value, default } if matches!(value.kind, ExprKind::Null) => {
            emit_string_assign(name, default, module)
        }
        ExprKind::NullCoalesce { value, .. } if expression_is_stringy(value, module) => {
            emit_string_assign(name, value, module)
        }
        ExprKind::Pipe {
            value: pipe_value,
            callable,
        } if expression_is_stringy(value, module) => {
            let synthetic = synthetic_pipe_call_expr(pipe_value, callable, value.span);
            emit_string_assign(name, &synthetic, module)
        }
        _ => Err(CompileError::new(
            value.span,
            "wasm32-web string assignments currently require a string literal or string variable",
        )),
    }
}

pub(in crate::codegen::wasm) fn emit_string_value_to_stack(
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(err) = unsupported_string_coercion_in_string_context(value) {
        return Err(err);
    }
    if let Some(err) = unsupported_object_string_coercion_in_string_context(value, module) {
        return Err(err);
    }
    if let Some(value) = static_string_value(value, module) {
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(());
    }
    if emit_object_tostring_value_to_stack(value, module)? {
        return Ok(());
    }
    if let Some(err) = unsupported_object_string_coercion_expr(value, module) {
        return Err(err);
    }
    if dynamic_scalar_mixed_string_coercion_supported(value, module)
        || variable_index_scalar_mixed_string_coercion_candidate(value, module)
        || unknown_mixed_string_coercion_candidate(value, module)
    {
        let cell = if let Some(cell) = materialize_dynamic_string_coercion_value_cell(value, module)? {
            cell
        } else if let Some(cell) = materialize_mixed_value_cell(value, module)? {
            cell
        } else {
            return Err(CompileError::new(
                value.span,
                "wasm32-web dynamic mixed string value could not materialize this value cell",
            ));
        };
        emit_dynamic_mixed_string_cast_value_to_stack(&cell, module);
        return Ok(());
    }
    if let Some(kind) = known_mixed_string_coercion_kind(value, module)? {
        let Some(cell) = materialize_mixed_value_cell(value, module)? else {
            return Err(CompileError::new(
                value.span,
                "wasm32-web mixed string value could not materialize this value cell",
            ));
        };
        return emit_known_mixed_string_cast_value_to_stack(&cell, kind, value.span, module);
    }
    if matches!(value.kind, ExprKind::ArrayAccess { .. }) {
        if let Some(cell) = materialize_mixed_value_cell(value, module)? {
            emit_dynamic_mixed_string_cast_value_to_stack(&cell, module);
            return Ok(());
        }
    }

    match &value.kind {
        ExprKind::StringLiteral(value) => {
            let (ptr, len) = module.intern_string(value);
            module.body().line(&format!("i32.const {}", ptr));
            module.body().line(&format!("i32.const {}", len));
            Ok(())
        }
        ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Str) => {
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get ${}_len", source));
            Ok(())
        }
        ExprKind::PropertyAccess { object, property }
        | ExprKind::NullsafePropertyAccess { object, property }
            if object_property_value_kind(object, property, module) == Some(ValueKind::Str) =>
        {
            match emit_expr(value, module)? {
                ValueKind::Str => Ok(()),
                _ => unreachable!("string property metadata must load a string"),
            }
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property }
            if object_dynamic_property_value_kind(object, property, module) == Some(ValueKind::Str) =>
        {
            match emit_expr(value, module)? {
                ValueKind::Str => Ok(()),
                _ => unreachable!("string property metadata must load a string"),
            }
        }
        ExprKind::StaticPropertyAccess { receiver, property }
            if static_property_value_kind(receiver, property, module) == Some(ValueKind::Str) =>
        {
            match emit_expr(value, module)? {
                ValueKind::Str => Ok(()),
                _ => unreachable!("string static property metadata must load a string"),
            }
        }
        ExprKind::Assignment { target, .. } if expression_is_stringy(target, module) => {
            match emit_expr(value, module)? {
                ValueKind::Str => Ok(()),
                _ => unreachable!("string assignment target metadata must load a string"),
            }
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        } if method_call_return_kind(object, method, module) == Some(ValueKind::Str) => {
            match emit_method_call_expr(value, object, method, args, module)? {
                ValueKind::Str => Ok(()),
                _ => unreachable!("string object method target must return a string"),
            }
        }
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } if object_expr_is_known_non_null(object, module)
            && method_call_return_kind(object, method, module) == Some(ValueKind::Str) =>
        {
            match emit_method_call_expr(value, object, method, args, module)? {
                ValueKind::Str => Ok(()),
                _ => unreachable!("string object method target must return a string"),
            }
        }
        ExprKind::MethodCall { object, method, args } => {
            match emit_method_call_expr(value, object, method, args, module)? {
                ValueKind::Str => Ok(()),
                _ => Err(CompileError::new(
                    value.span,
                    "wasm32-web string value requires a string-returning method call",
                )),
            }
        }
        ExprKind::BinaryOp {
            left,
            op: BinOp::Concat,
            right,
        } => emit_concat_string_value_to_stack(value, left, right, module),
        ExprKind::Cast {
            target: CastType::String,
            expr,
        } => emit_string_cast_value_to_stack(value, expr, module),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } if expression_is_stringy(then_expr, module) && expression_is_stringy(else_expr, module) => {
            emit_string_ternary_value_to_stack(value, condition, then_expr, else_expr, module)
        }
        ExprKind::ShortTernary { value, default }
            if expression_is_stringy(value, module) && expression_is_stringy(default, module) =>
        {
            emit_short_string_ternary_value_to_stack(value, default, module)
        }
        ExprKind::Match { .. } if expression_is_stringy(value, module) => {
            match emit_scalar_match(value, match_subject(value), match_arms(value), match_default(value), module)? {
                ValueKind::Str => Ok(()),
                _ => unreachable!("stringy match must produce a string result"),
            }
        }
        ExprKind::NullCoalesce { value, default } if matches!(value.kind, ExprKind::Null) => {
            emit_string_value_to_stack(default, module)
        }
        ExprKind::NullCoalesce { value, .. } if expression_is_stringy(value, module) => {
            emit_string_value_to_stack(value, module)
        }
        ExprKind::Pipe {
            value: pipe_value,
            callable,
        } if expression_is_stringy(value, module) => {
            let synthetic = synthetic_pipe_call_expr(pipe_value, callable, value.span);
            emit_string_value_to_stack(&synthetic, module)
        }
        ExprKind::ConstRef(const_name) => {
            let Some(ConstantValue::Str(value)) = module.constant_value(const_name) else {
                return Err(CompileError::new(
                    value.span,
                    "wasm32-web string value currently requires a string literal, string constant, string variable, or string-returning user function",
                ));
            };
            let (ptr, len) = module.intern_string(&value);
            module.body().line(&format!("i32.const {}", ptr));
            module.body().line(&format!("i32.const {}", len));
            Ok(())
        }
        ExprKind::FunctionCall { name, args }
            if module.has_function(name) && module.function_return_kind(name) == Some(ValueKind::Str) =>
        {
            emit_user_function_args(value, name, args, module)?;
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(name)));
            Ok(())
        }
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } if static_method_call_return_kind(receiver, method, module) == Some(ValueKind::Str) => {
            match emit_static_method_call_expr(value, receiver, method, args, module)? {
                ValueKind::Str => Ok(()),
                _ => unreachable!("string static method target must return a string"),
            }
        }
        ExprKind::ClosureCall { var, args }
            if callable_variable_return_kind(module, var, args) == Some(ValueKind::Str) =>
        {
            match emit_callable_variable_call(value, var, args, module)? {
                ValueKind::Str => Ok(()),
                _ => unreachable!("string callable target must return a string"),
            }
        }
        ExprKind::ExprCall { callee, args }
            if callable_expr_return_kind(module, callee, args) == Some(ValueKind::Str) =>
        {
            match emit_callable_expr_call(value, callee, args, module)? {
                ValueKind::Str => Ok(()),
                _ => unreachable!("string callable expression target must return a string"),
            }
        }
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("array_reduce") && array_reduce_call_is_string(args, module) =>
        {
            match emit_array_reduce_call(value, args, module)? {
                ValueKind::Str => Ok(()),
                _ => unreachable!("string array_reduce must produce a string result"),
            }
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("gettype") => {
            emit_gettype_string_value_to_stack(value, args, module)
        }
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("call_user_func")
                && call_user_func_return_kind(args, module) == Some(ValueKind::Str) =>
        {
            match emit_call_user_func_call(value, args, module)? {
                ValueKind::Str => Ok(()),
                _ => unreachable!("string call_user_func target must return a string"),
            }
        }
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("call_user_func_array")
                && call_user_func_array_return_kind(args, module) == Some(ValueKind::Str) =>
        {
            match emit_call_user_func_array_call(value, args, module)? {
                ValueKind::Str => Ok(()),
                _ => unreachable!("string call_user_func_array target must return a string"),
            }
        }
        ExprKind::FunctionCall { name, args } if is_output_string_builtin(name.as_str()) => {
            if emit_string_builtin_value_to_stack_if_supported(value, name.as_str(), args, module)? {
                Ok(())
            } else {
                unreachable!("output string builtin should materialize a string value")
            }
        }
        ExprKind::ArrayAccess { array, index }
            if expression_is_arrayy(array, module)
                || expression_has_array_type(array, module)
                || nested_array_static_access_kind(array, index, module) == Some(ValueCellKind::Str)
                || dynamic_outer_nested_assoc_static_access_kind(array, index, module)
                    == Some(ValueCellKind::Str)
                || dynamic_parent_nested_assoc_static_access_kind(array, index, module)
                    == Some(ValueCellKind::Str)
                || direct_array_chunk_first_scalar_kind(array, index, module) == Some(ValueCellKind::Str) =>
        {
            match emit_expr(value, module)? {
                ValueKind::Str => Ok(()),
                _ => Err(CompileError::new(
                    value.span,
                    "wasm32-web string value-array access requires a known string element",
                )),
            }
        }
        ExprKind::ArrayAccess { array, index } if is_direct_array_chunk_first_scalar_access(array, index) => {
            Err(CompileError::new(
                value.span,
                "wasm32-web scalar array_chunk(...)[0][0] requires known indexed source metadata",
            ))
        }
        ExprKind::ArrayAccess { array, index } => emit_string_index_to_stack(value, array, index, module),
        _ => Err(CompileError::new(
            value.span,
            "wasm32-web string value currently requires a string literal, string variable, or string-returning user function",
        )),
    }
}

fn synthetic_pipe_call_expr(value: &Expr, callable: &Expr, span: Span) -> Expr {
    let synth_args = vec![value.clone()];
    match &callable.kind {
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => Expr::new(
            ExprKind::FunctionCall {
                name: name.clone(),
                args: synth_args,
            },
            span,
        ),
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            Expr::new(
                ExprKind::StaticMethodCall {
                    receiver: receiver.clone(),
                    method: method.clone(),
                    args: synth_args,
                },
                span,
            )
        }
        ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) => Expr::new(
            ExprKind::MethodCall {
                object: object.clone(),
                method: method.clone(),
                args: synth_args,
            },
            span,
        ),
        ExprKind::Variable(var) => Expr::new(
            ExprKind::ClosureCall {
                var: var.clone(),
                args: synth_args,
            },
            span,
        ),
        _ => Expr::new(
            ExprKind::ExprCall {
                callee: Box::new(callable.clone()),
                args: synth_args,
            },
            span,
        ),
    }
}

pub(in crate::codegen::wasm) fn emit_string_value_to_locals(
    value: &Expr,
    ptr_local: &str,
    len_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_string_value_to_stack(value, module)?;
    module.body().line(&format!("local.set {}", len_local));
    module.body().line(&format!("local.set {}", ptr_local));
    Ok(())
}

pub(in crate::codegen::wasm) fn emit_concat_string_value_to_stack(
    expr: &Expr,
    left: &Expr,
    right: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(err) = unsupported_string_coercion_expr(left) {
        return Err(err);
    }
    if let Some(err) = unsupported_string_coercion_expr(right) {
        return Err(err);
    }
    if !string_cast_value_supported(left, module) || !string_cast_value_supported(right, module) {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web string concatenation currently requires scalar string-coercible operands",
        ));
    }

    let left_var = concat_string_arg_or_materialize(left, "concat_left", module)?.ok_or_else(|| {
        CompileError::new(
            left.span,
            "wasm32-web string concatenation currently requires scalar string-coercible operands",
        )
    })?;
    let right_var = concat_string_arg_or_materialize(right, "concat_right", module)?.ok_or_else(|| {
        CompileError::new(
            right.span,
            "wasm32-web string concatenation currently requires scalar string-coercible operands",
        )
    })?;
    let out_ptr = module.next_label("concat_out_ptr");
    let out_len = module.next_label("concat_out_len");
    let out_idx = module.next_label("concat_out_idx");
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(out_len.trim_start_matches('$').to_string());
    module.declare_i32_local(out_idx.trim_start_matches('$').to_string());

    module.body().line(&format!("local.get ${}_len", left_var));
    module.body().line(&format!("local.get ${}_len", right_var));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
    emit_copy_string_to_memory(&left_var, &out_ptr, &out_idx, module);
    emit_copy_string_to_memory(&right_var, &out_ptr, &out_idx, module);
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_len));
    Ok(())
}

pub(in crate::codegen::wasm) fn concat_string_arg_or_materialize(
    value: &Expr,
    prefix: &str,
    module: &mut WasmModule,
) -> Result<Option<String>, CompileError> {
    if let Some(var) = string_arg_or_materialize(value, prefix, module)? {
        return Ok(Some(var));
    }
    if string_cast_value_supported(value, module) {
        return materialize_string_cast_expr(value, prefix, module).map(Some);
    }
    Ok(None)
}

pub(in crate::codegen::wasm) fn string_coercion_arg_or_materialize(
    value: &Expr,
    prefix: &str,
    module: &mut WasmModule,
) -> Result<Option<String>, CompileError> {
    if let Some(var) = string_arg_or_materialize(value, prefix, module)? {
        return Ok(Some(var));
    }
    if string_cast_value_supported(value, module) && static_string_coercion_value(value, module).is_none() {
        return materialize_string_cast_expr(value, prefix, module).map(Some);
    }
    Ok(None)
}
