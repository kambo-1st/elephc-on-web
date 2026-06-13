//! Purpose:
//! Dispatches PHP expression lowering for wasm32-web and owns the small scalar
//! helper functions shared by sibling expression emitters.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::emit_expr()` re-export.
//!
//! Key details:
//! - Dispatch remains table-like and delegates feature-specific lowering to
//!   focused sibling modules under `crate::codegen::wasm::expr`.

use super::*;

pub(in crate::codegen::wasm) fn emit_expr(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => {
            module.body().line(&format!("i64.const {}", value));
            Ok(ValueKind::Int)
        }
        ExprKind::FloatLiteral(value) => {
            module.body().line(&format!("f64.const {}", value));
            Ok(ValueKind::Float)
        }
        ExprKind::BoolLiteral(value) => {
            module.body().line(&format!("i32.const {}", i32::from(*value)));
            Ok(ValueKind::Bool)
        }
        ExprKind::Null => {
            module.body().line("i32.const 0");
            Ok(ValueKind::Null)
        }
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => Err(array_unsupported(expr)),
        ExprKind::NewObject { class_name, args } => {
            emit_new_object_expr(expr, class_name, args, module)
        }
        ExprKind::InstanceOf { value, target } => {
            emit_instanceof_expr(expr, value, target, module)
        }
        ExprKind::NewScopedObject { receiver, args } => {
            emit_new_scoped_object_expr(expr, receiver, args, module)
        }
        ExprKind::PropertyAccess { object, property } => {
            emit_property_access_expr(expr, object, property, module)
        }
        ExprKind::NullsafePropertyAccess { object, .. }
            if matches!(object.kind, ExprKind::Null) =>
        {
            module.body().line("i32.const 0");
            Ok(ValueKind::Null)
        }
        ExprKind::NullsafePropertyAccess { object, property }
            if object_expr_is_known_non_null(object, module) =>
        {
            emit_property_access_expr(expr, object, property, module)
        }
        ExprKind::NullsafePropertyAccess { object, property }
            if object_class_name_for_expr(object, module).is_some() =>
        {
            emit_nullable_exact_object_property_access(expr, object, property, module)
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            emit_nullsafe_mixed_object_property_access_expr(expr, object, property, module)
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            emit_static_property_access_expr(expr, receiver, property, module)
        }
        ExprKind::DynamicPropertyAccess { object, property } => {
            emit_dynamic_property_access_expr(expr, object, property, module)
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, .. }
            if matches!(object.kind, ExprKind::Null) =>
        {
            module.body().line("i32.const 0");
            Ok(ValueKind::Null)
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property }
            if object_expr_is_known_non_null(object, module) =>
        {
            emit_dynamic_property_access_expr(expr, object, property, module)
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            emit_nullsafe_mixed_object_dynamic_property_access_expr(expr, object, property, module)
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => emit_method_call_expr(expr, object, method, args, module),
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } if object_receiver_needs_runtime_class_id(object, module) => {
            emit_nullsafe_dynamic_object_method_call(expr, object, method, args, module)
        }
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } if object_expr_is_known_non_null(object, module) => {
            emit_method_call_expr(expr, object, method, args, module)
        }
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } if object_class_name_for_expr(object, module).is_some() => {
            emit_nullable_exact_object_method_call(expr, object, method, args, module)
        }
        ExprKind::NullsafeMethodCall { object, .. } if matches!(object.kind, ExprKind::Null) => {
            module.body().line("i32.const 0");
            Ok(ValueKind::Null)
        }
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => emit_nullsafe_mixed_object_method_call_expr(expr, object, method, args, module),
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => emit_static_method_call_expr(expr, receiver, method, args, module),
        ExprKind::Assignment {
            target,
            value,
            prelude,
            ..
        } => {
            emit_assignment_expr_prelude(prelude, module)?;
            match &target.kind {
                ExprKind::PropertyAccess { object, property } => {
                    emit_object_property_assignment_expr(expr, object, property, value, module)
                }
                ExprKind::DynamicPropertyAccess { object, property } => {
                    emit_dynamic_object_property_assignment_expr(expr, object, property, value, module)
                }
                ExprKind::NullsafePropertyAccess { .. }
                | ExprKind::NullsafeDynamicPropertyAccess { .. } => Err(CompileError::new(
                    target.span,
                    "wasm32-web nullsafe property assignment expressions are not supported yet",
                )),
                _ => Err(CompileError::new(
                    target.span,
                    "wasm32-web assignment expression target is not supported yet",
                )),
            }
        }
        ExprKind::ConstRef(name) => emit_constant_expr(expr, name, module),
        ExprKind::ScopedConstantAccess { receiver, name } => {
            if module.enum_case_class(receiver, name).is_some() {
                return emit_enum_case_expr(expr, receiver, name, module);
            }
            emit_scoped_constant_expr(expr, receiver, name, module)
        }
        ExprKind::ClassConstant { .. } => Err(CompileError::new(
            expr.span,
            "wasm32-web ::class is only supported in output or string-local position",
        )),
        ExprKind::ArrayAccess { array, index } if nested_array_access_requires_layout(array) => {
            emit_nested_array_index_expr(expr, array, index, module)
        }
        ExprKind::ArrayAccess { array, index }
            if static_property_array_access_kind(array, index, module).is_some() =>
        {
            emit_array_index_expr(expr, array, index, module)
        }
        ExprKind::ArrayAccess { array, index }
            if object_class_name_for_expr(expr, module).is_some() =>
        {
            emit_array_index_expr(expr, array, index, module)
        }
        ExprKind::ArrayAccess { array, index }
            if expression_is_arrayy(array, module)
                || expression_has_array_type(array, module)
                || matches!(&array.kind, ExprKind::ConstRef(name) if module.array_constant_value(name).is_some()) =>
        {
            emit_array_index_expr(expr, array, index, module)
        }
        ExprKind::Variable(name) => {
            let kind = module.local_kind(name).ok_or_else(|| {
                CompileError::new(expr.span, "wasm32-web variable is not declared before use")
            })?;
            match kind {
                LocalKind::I64 => {
                    if let Some(address_local) = module.i64_ref_alias(name) {
                        module.body().line(&format!("local.get {}", address_local));
                        module.body().line("i64.load");
                    } else {
                        module.body().line(&format!("local.get ${}", name));
                    }
                }
                LocalKind::F64 => {
                    if let Some(address_local) = module.f64_ref_alias(name) {
                        module.body().line(&format!("local.get {}", address_local));
                        module.body().line("f64.load");
                    } else {
                        module.body().line(&format!("local.get ${}", name));
                    }
                }
                LocalKind::I32 => {
                    if let Some(address_local) = module.i32_ref_alias(name) {
                        module.body().line(&format!("local.get {}", address_local));
                        module.body().line("i64.load");
                        module.body().line("i64.const 0");
                        module.body().line("i64.ne");
                    } else {
                        module.body().line(&format!("local.get ${}", name));
                    }
                }
                LocalKind::Str => {
                    return Err(CompileError::new(
                        expr.span,
                        "wasm32-web string variables are only supported in output position",
                    ));
                }
                LocalKind::Array => {
                    return Err(array_unsupported(expr));
                }
                LocalKind::Mixed => module.body().line(&format!("local.get ${}", name)),
                LocalKind::Object => module.body().line(&format!("local.get ${}", name)),
                LocalKind::Callable => {
                    return Err(CompileError::new(
                        expr.span,
                        "wasm32-web callable variables are only supported as callback arguments",
                    ));
                }
            }
            Ok(value_kind_for_local(kind))
        }
        ExprKind::This => {
            if module.local_kind("this") != Some(LocalKind::Object) {
                return Err(CompileError::new(
                    expr.span,
                    "wasm32-web $this requires an object method context",
                ));
            }
            module.body().line("local.get $this");
            Ok(ValueKind::Object)
        }
        ExprKind::Negate(inner) => {
            if let Some(kind) = emit_known_mixed_negate(inner, module)? {
                return Ok(kind);
            }
            if expression_is_floaty(inner, module) {
                require_float(inner, module)?;
                module.body().line("f64.neg");
                return Ok(ValueKind::Float);
            }
            module.body().line("i64.const 0");
            require_int(inner, module)?;
            module.body().line("i64.sub");
            Ok(ValueKind::Int)
        }
        ExprKind::Not(inner) => {
            emit_condition(inner, module)?;
            module.body().line("i32.eqz");
            Ok(ValueKind::Bool)
        }
        ExprKind::BitNot(inner) => {
            if let Some(kind) = emit_known_mixed_bit_not(inner, module)? {
                return Ok(kind);
            }
            require_int(inner, module)?;
            module.body().line("i64.const -1");
            module.body().line("i64.xor");
            Ok(ValueKind::Int)
        }
        ExprKind::Print(inner) => {
            emit_output_expr(inner, module)?;
            module.body().line("i64.const 1");
            Ok(ValueKind::Int)
        }
        ExprKind::NullCoalesce { value, default } => {
            emit_null_coalesce_expr(expr, value, default, module)
        }
        ExprKind::Cast { target, expr } => emit_cast_expr(expr, target, module),
        ExprKind::BinaryOp { left, op, right } => emit_binary(expr, left, op, right, module),
        ExprKind::PreIncrement(name) => emit_inc_dec(expr, name, 1, true, module),
        ExprKind::PostIncrement(name) => emit_inc_dec(expr, name, 1, false, module),
        ExprKind::PreDecrement(name) => emit_inc_dec(expr, name, -1, true, module),
        ExprKind::PostDecrement(name) => emit_inc_dec(expr, name, -1, false, module),
        ExprKind::FunctionCall { name, args } => {
            let name = name.as_str();
            if name.eq_ignore_ascii_case("strlen") {
                return emit_strlen_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("ord") {
                return emit_ord_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("abs") {
                return emit_abs_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("intdiv") {
                return emit_intdiv_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("fdiv") {
                return emit_fdiv_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("min") || name.eq_ignore_ascii_case("max") {
                return emit_min_max_call(expr, name, args, module);
            }
            if name.eq_ignore_ascii_case("intval") {
                return emit_intval_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("floatval") {
                return emit_floatval_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("boolval") {
                return emit_boolval_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("empty") {
                return emit_empty_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("is_numeric") {
                return emit_is_numeric_call(expr, args, module);
            }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "is_nan" | "is_finite" | "is_infinite"
            ) {
                return emit_float_predicate_call(expr, name, args, module);
            }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "is_int"
                    | "is_float"
                    | "is_bool"
                    | "is_null"
                    | "is_string"
                    | "is_iterable"
                    | "is_object"
            ) {
                return emit_type_predicate_call(expr, name, args, module);
            }
            if name.eq_ignore_ascii_case("is_callable") {
                return emit_is_callable_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("is_a") {
                return emit_is_a_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("is_subclass_of") {
                return emit_is_subclass_of_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("call_user_func") {
                return emit_call_user_func_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("call_user_func_array") {
                return emit_call_user_func_array_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("isset") {
                return emit_isset_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("count") {
                return emit_count_call(expr, args, module);
            }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "array_sum" | "array_product"
            ) {
                return emit_numeric_array_fold_call(expr, name, args, module);
            }
            if name.eq_ignore_ascii_case("array_rand") {
                return emit_array_rand_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("in_array") {
                return emit_in_array_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("array_key_exists") {
                return emit_array_key_exists_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("array_reduce") {
                return emit_array_reduce_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("array_walk") {
                return emit_array_walk_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("usort") {
                return emit_usort_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("uasort") {
                return emit_uasort_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("uksort") {
                return emit_uksort_call(expr, args, module);
            }
            if is_callback_array_builtin(name) {
                return Err(CompileError::new(
                    expr.span,
                    "wasm32-web callback array builtins require callable runtime support",
                ));
            }
            if name.eq_ignore_ascii_case("array_push") {
                return emit_array_push_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("array_pop") {
                return emit_array_pop_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("array_shift") {
                return emit_array_shift_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("array_unshift") {
                return emit_array_unshift_call(expr, args, module);
            }
            if matches!(name.to_ascii_lowercase().as_str(), "sort" | "rsort") {
                return emit_array_sort_call(expr, name, args, module);
            }
            if matches!(name.to_ascii_lowercase().as_str(), "natsort" | "natcasesort") {
                return Err(CompileError::new(
                    expr.span,
                    "wasm32-web natsort()/natcasesort() require natural string comparison runtime support",
                ));
            }
            if name.eq_ignore_ascii_case("shuffle") {
                return emit_array_shuffle_call(expr, args, module);
            }
            if matches!(name.to_ascii_lowercase().as_str(), "ksort" | "krsort") {
                return emit_array_key_sort_call(expr, name, args, module);
            }
            if matches!(name.to_ascii_lowercase().as_str(), "asort" | "arsort") {
                return emit_assoc_array_value_sort_call(expr, name, args, module);
            }
            if name.eq_ignore_ascii_case("json_validate") {
                return emit_json_validate_call(expr, args, module);
            }
            if name.eq_ignore_ascii_case("json_last_error") {
                return emit_json_last_error_call(expr, args, module);
            }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "function_exists"
                    | "class_exists"
                    | "interface_exists"
                    | "trait_exists"
                    | "enum_exists"
            ) {
                return emit_existence_call(expr, name, args, module);
            }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "method_exists" | "property_exists"
            ) {
                return emit_member_exists_call(expr, name, args, module);
            }
            if name.eq_ignore_ascii_case("printf") {
                return emit_printf_call(expr, args, module);
            }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "str_contains"
                    | "str_starts_with"
                    | "str_ends_with"
                    | "ctype_alpha"
                    | "ctype_digit"
                    | "ctype_alnum"
                    | "ctype_space"
            ) {
                return emit_literal_string_bool_call(expr, name, args, module);
            }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "strcmp" | "strcasecmp" | "strpos" | "strrpos"
            ) {
                return emit_literal_string_int_call(expr, name, args, module);
            }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "floor"
                    | "ceil"
                    | "sqrt"
                    | "pi"
                    | "pow"
                    | "sin"
                    | "cos"
                    | "tan"
                    | "asin"
                    | "acos"
                    | "atan"
                    | "sinh"
                    | "cosh"
                    | "tanh"
                    | "log"
                    | "log10"
                    | "exp"
                    | "deg2rad"
                    | "rad2deg"
                    | "fmod"
                    | "atan2"
                    | "hypot"
            ) {
                return emit_float_math_call(expr, name, args, module);
            }
            if is_literal_float_math_builtin(name) {
                return emit_literal_float_math_call(expr, name, args, module);
            }
            if is_output_string_builtin(name) {
                emit_string_value_to_stack(expr, module)?;
                return Ok(ValueKind::Str);
            }
            if !module.has_function(name) {
                return Err(CompileError::new(
                    expr.span,
                    "wasm32-web function calls currently support user-defined scalar functions and selected scalar builtins only",
                ));
            }
            emit_user_function_args(expr, name, args, module)?;
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(name)));
            module.function_return_kind(name).ok_or_else(|| {
                CompileError::new(expr.span, "wasm32-web user function metadata is missing")
            })
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => emit_scalar_ternary(expr, condition, then_expr, else_expr, module),
        ExprKind::ShortTernary { value, default } => emit_short_ternary(expr, value, default, module),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => emit_scalar_match(expr, subject, arms, default.as_deref(), module),
        ExprKind::ClosureCall { var, args } => emit_callable_variable_call(expr, var, args, module),
        ExprKind::ExprCall { callee, args } => emit_callable_expr_call(expr, callee, args, module),
        ExprKind::Pipe { value, callable } => emit_pipe_expr(expr, value, callable, module),
        _ => Err(unsupported_expr(expr)),
    }
}

fn emit_pipe_expr(
    expr: &Expr,
    value: &Expr,
    callable: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let synth_args = vec![value.clone()];
    let synthetic = match &callable.kind {
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => Expr::new(
            ExprKind::FunctionCall {
                name: name.clone(),
                args: synth_args,
            },
            expr.span,
        ),
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            Expr::new(
                ExprKind::StaticMethodCall {
                    receiver: receiver.clone(),
                    method: method.clone(),
                    args: synth_args,
                },
                expr.span,
            )
        }
        ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) => Expr::new(
            ExprKind::MethodCall {
                object: object.clone(),
                method: method.clone(),
                args: synth_args,
            },
            expr.span,
        ),
        ExprKind::Variable(var) => Expr::new(
            ExprKind::ClosureCall {
                var: var.clone(),
                args: synth_args,
            },
            expr.span,
        ),
        _ => Expr::new(
            ExprKind::ExprCall {
                callee: Box::new(callable.clone()),
                args: synth_args,
            },
            expr.span,
        ),
    };
    emit_expr(&synthetic, module)
}

pub(in crate::codegen::wasm) fn require_int(expr: &Expr, module: &mut WasmModule) -> Result<(), CompileError> {
    match emit_expr(expr, module)? {
        ValueKind::Int => Ok(()),
        _ => Err(CompileError::new(
            expr.span,
            "wasm32-web runtime WAT output expected an integer expression",
        )),
    }
}

pub(in crate::codegen::wasm) fn require_bool(expr: &Expr, module: &mut WasmModule) -> Result<(), CompileError> {
    match emit_expr(expr, module)? {
        ValueKind::Bool => Ok(()),
        _ => Err(CompileError::new(
            expr.span,
            "wasm32-web runtime WAT output expected a boolean expression",
        )),
    }
}

pub(in crate::codegen::wasm) fn require_float(expr: &Expr, module: &mut WasmModule) -> Result<(), CompileError> {
    match emit_expr(expr, module)? {
        ValueKind::Float => Ok(()),
        ValueKind::Int => {
            module.body().line("f64.convert_i64_s");
            Ok(())
        }
        _ => Err(CompileError::new(
            expr.span,
            "wasm32-web runtime WAT output expected a numeric expression",
        )),
    }
}

pub(in crate::codegen::wasm) fn emit_mixed_i64_payload(name: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get ${}", name));
    module.body().line("call $__rt_mixed_payload_i64");
}

pub(in crate::codegen::wasm) fn emit_mixed_i32_payload(name: &str, offset: i32, module: &mut WasmModule) {
    module.body().line(&format!("local.get ${}", name));
    module.body().line(&format!("i32.const {}", offset));
    module.body().line("call $__rt_mixed_payload_i32");
}

pub(in crate::codegen::wasm) fn emit_mixed_f64_payload(name: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get ${}", name));
    module.body().line("call $__rt_mixed_payload_f64");
}

fn emit_assignment_expr_prelude(
    prelude: &[Stmt],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    for stmt in prelude {
        match &stmt.kind {
            StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
                emit_assign_value(name, value, module)?;
            }
            _ => {
                return Err(CompileError::new(
                    stmt.span,
                    "wasm32-web assignment expression prelude is not supported yet",
                ));
            }
        }
    }
    Ok(())
}

fn unsupported_expr(expr: &Expr) -> CompileError {
    CompileError::new(
        expr.span,
        "wasm32-web WAT output currently supports scalar expressions only",
    )
}

pub(in crate::codegen::wasm) fn array_unsupported(expr: &Expr) -> CompileError {
    CompileError::new(
        expr.span,
        "wasm32-web PHP array values are not supported yet",
    )
}

pub(in crate::codegen::wasm) fn is_array_value_expr(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)
    )
}
