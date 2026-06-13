//! Purpose:
//! Lowers WASM callable helper expressions and callable-variable invocations.
//!
//! Called from:
//! - `super::emit_expr()` and related expression-kind probes.
//!
//! Key details:
//! - `call_user_func_array()` normalizes static associative argument arrays through
//!   the shared PHP call-argument planner before invoking builtin targets.

use crate::errors::CompileError;
use crate::names::Name;
use crate::parser::ast::{CallableTarget, Expr, ExprKind};
use crate::span::Span;
use crate::types::{builtin_call_sig, call_args};

use crate::codegen::wasm::module::{
    wasm_function_name, wasm_value_type, ArrayLayout, AssocKeyValue, LocalKind, ValueKind,
    WasmModule,
};

use super::{
    emit_condition, emit_expr, emit_known_mixed_numeric_operand, emit_mixed_arg_assign,
    emit_string_parts_equal, emit_string_value_to_stack, emit_value_array_arg_assign, emit_value_as_local_kind,
    expression_is_floaty,
    is_output_optional_int_builtin, is_output_string_builtin, known_mixed_numeric_kind,
    dynamic_numeric_operand_may_materialize, require_float, require_int,
    evaluated_static_callback_function_name, static_assoc_access_key, static_callback_function_name,
    static_or_const_int_value, static_range_items_if_possible, static_string_value, wasm_known_builtin_exists,
    inherited_property_layout_available, method_body_uses_this_property, object_class_name_for_expr,
};

pub(super) fn emit_call_user_func_call(
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if let Some(Expr {
        kind: ExprKind::Variable(var),
        ..
    }) = args.first()
    {
        if let Some((target, capture_local)) = module.callable_instance_target(var) {
            return emit_instance_callable_target(expr, &target, &capture_local, &args[1..], module);
        }
        if let Some(target) = invokable_object_target(module, var) {
            return emit_instance_callable_target(expr, &target, var, &args[1..], module);
        }
    }
    if let Some(Expr {
        kind: ExprKind::FirstClassCallable(CallableTarget::Method { object, method }),
        ..
    }) = args.first()
    {
        return emit_direct_instance_callable_target(expr, object, method, &args[1..], module);
    }
    if let Some(Expr {
        kind:
            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            },
        ..
    }) = args.first()
    {
        if let Some(kind) = emit_instance_callable_ternary_dispatch(
            expr,
            condition,
            then_expr,
            else_expr,
            &args[1..],
            &args[1..],
            module,
        )?
        {
            return Ok(kind);
        }
        if let Some(kind) = emit_static_callable_ternary_dispatch(
            expr,
            condition,
            then_expr,
            else_expr,
            &args[1..],
            &args[1..],
            module,
        )?
        {
            return Ok(kind);
        }
    }
    if let Some(callback) = args.first() {
        if object_expr_is_invokable(module, callback) {
            return emit_direct_instance_callable_target(expr, callback, "__invoke", &args[1..], module);
        }
        if let Some(target) = callable_array_target(callback, module) {
            return emit_callable_array_target(expr, target, &args[1..], module);
        }
    }
    if let Some((target, call_args)) = evaluated_call_user_func_target(args, module)? {
        return emit_static_callable_target(expr, &target, call_args, module);
    }
    if let Some(kind) = dynamic_call_user_func_return_kind(args, module)? {
        return emit_dynamic_call_user_func_dispatch(expr, args, kind, module);
    }
    Err(CompileError::new(
        expr.span,
        "wasm32-web call_user_func() requires a statically-known user-function callback",
    ))
}

pub(super) fn emit_call_user_func_array_call(
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if let [
        Expr {
            kind: ExprKind::Variable(var),
            ..
        },
        packed_args,
    ] = args
    {
        if let Some((target, capture_local)) = module.callable_instance_target(var) {
            if let Some((_, call_args)) =
                call_user_func_array_target_from_parts(&target, packed_args, module)
            {
                return emit_instance_callable_target(
                    expr,
                    &target,
                    &capture_local,
                    &call_args,
                    module,
                );
            }
        }
        if let Some(target) = invokable_object_target(module, var) {
            if let Some((_, call_args)) =
                call_user_func_array_target_from_parts(&target, packed_args, module)
            {
                return emit_instance_callable_target(expr, &target, var, &call_args, module);
            }
        }
    }
    if let [
        Expr {
            kind: ExprKind::FirstClassCallable(CallableTarget::Method { object, method }),
            ..
        },
        packed_args,
    ] = args
    {
        if let Some((_, call_args)) =
            call_user_func_array_target_from_parts(
                &direct_instance_callable_target(expr, object, method, module)?.0,
                packed_args,
                module,
            )
        {
            return emit_direct_instance_callable_target(expr, object, method, &call_args, module);
        }
    }
    if let [
        Expr {
            kind:
                ExprKind::Ternary {
                    condition,
                    then_expr,
                    else_expr,
                },
            ..
        },
        packed_args,
    ] = args
    {
        if let Some(targets) =
            instance_callable_ternary_targets(then_expr, else_expr, module)?
        {
            if let (Some((_, then_call_args)), Some((_, else_call_args))) = (
                call_user_func_array_target_from_parts(&targets.then_target, packed_args, module),
                call_user_func_array_target_from_parts(&targets.else_target, packed_args, module),
            )
            {
                let kind = emit_instance_callable_ternary_dispatch_with_targets(
                    expr,
                    condition,
                    targets,
                    &then_call_args,
                    &else_call_args,
                    module,
                )?;
                return Ok(kind);
            }
        }
        if let Some(targets) = static_callable_ternary_targets(then_expr, else_expr, module)? {
            if let (Some((_, then_call_args)), Some((_, else_call_args))) = (
                call_user_func_array_target_from_parts(&targets.then_target, packed_args, module),
                call_user_func_array_target_from_parts(&targets.else_target, packed_args, module),
            )
            {
                let kind = emit_static_callable_ternary_dispatch_with_targets(
                    expr,
                    condition,
                    targets,
                    &then_call_args,
                    &else_call_args,
                    module,
                )?;
                return Ok(kind);
            }
        }
    }
    if let [callback, packed_args] = args {
        if object_expr_is_invokable(module, callback) {
            if let Some((_, call_args)) = call_user_func_array_target_from_parts(
                &direct_instance_callable_target(expr, callback, "__invoke", module)?.0,
                packed_args,
                module,
            ) {
                return emit_direct_instance_callable_target(
                    expr,
                    callback,
                    "__invoke",
                    &call_args,
                    module,
                );
            }
        }
        if let Some(target) = callable_array_target(callback, module) {
            if let Some((_, call_args)) =
                call_user_func_array_target_from_parts(callable_array_target_name(&target), packed_args, module)
            {
                return emit_callable_array_target(expr, target, &call_args, module);
            }
        }
    }
    let Some((target, call_args)) = evaluated_call_user_func_array_target_owned(args, module)? else {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web call_user_func_array() requires a statically-known user-function callback and known packed argument array",
        ));
    };
    emit_static_callable_target(expr, &target, &call_args, module)
}

pub(super) fn emit_user_function_args(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let expanded_args;
    let args = if args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_))) {
        expanded_args = expand_wasm_static_spread_args(args)?;
        expanded_args.as_slice()
    } else {
        args
    };
    if args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_))) {
        return Err(CompileError::new(
            call.span,
            "wasm32-web user function calls currently require statically expandable spread arguments",
        ));
    }
    let params = module.function_param_names(name).ok_or_else(|| {
        CompileError::new(call.span, "wasm32-web user function metadata is missing")
    })?;
    let param_kinds = module.function_param_kinds(name).ok_or_else(|| {
        CompileError::new(call.span, "wasm32-web user function metadata is missing")
    })?;
    let defaults = module.function_param_defaults(name).ok_or_else(|| {
        CompileError::new(call.span, "wasm32-web user function metadata is missing")
    })?;
    if params.len() != defaults.len() || params.len() != param_kinds.len() {
        return Err(CompileError::new(
            call.span,
            "wasm32-web user function metadata is inconsistent",
        ));
    }
    let call_id = module.next_label("call_arg").trim_start_matches('$').to_string();
    let arg_locals = params
        .iter()
        .enumerate()
        .map(|(index, _)| format!("{}_{}", call_id, index))
        .collect::<Vec<_>>();
    for (local, kind) in arg_locals.iter().zip(param_kinds.iter().copied()) {
        match kind {
            LocalKind::I64 => module.declare_i64_local(local.clone()),
            LocalKind::F64 => module.declare_f64_local(local.clone()),
            LocalKind::I32 => module.declare_i32_local(local.clone()),
            LocalKind::Str => {
                module.declare_i32_local(format!("{}_ptr", local));
                module.declare_i32_local(format!("{}_len", local));
            }
            LocalKind::Array => {
                module.declare_array_local(local.clone());
            }
            LocalKind::Object => module.declare_i32_local(local.clone()),
            LocalKind::Mixed => module.declare_i32_local(local.clone()),
            LocalKind::Callable => module.declare_i32_local(local.clone()),
        }
    }

    let mut filled = vec![false; params.len()];
    let mut next_positional = 0usize;
    let mut seen_named = false;
    for arg in args {
        match &arg.kind {
            ExprKind::NamedArg { name, value } => {
                seen_named = true;
                let Some(index) = params.iter().position(|param| param == name) else {
                    return Err(CompileError::new(
                        arg.span,
                        "wasm32-web user function call has an unknown named argument",
                    ));
                };
                if filled[index] {
                    return Err(CompileError::new(
                        arg.span,
                        "wasm32-web user function call has a duplicate argument",
                    ));
                }
                emit_value_as_arg_local(&arg_locals[index], value, param_kinds[index], module)?;
                filled[index] = true;
            }
            _ => {
                if seen_named {
                    return Err(CompileError::new(
                        arg.span,
                        "wasm32-web user function call cannot use positional arguments after named arguments",
                    ));
                }
                if next_positional >= params.len() {
                    return Err(CompileError::new(
                        call.span,
                        "wasm32-web user function call has too many arguments",
                    ));
                }
                emit_value_as_arg_local(
                    &arg_locals[next_positional],
                    arg,
                    param_kinds[next_positional],
                    module,
                )?;
                filled[next_positional] = true;
                next_positional += 1;
            }
        }
    }

    for (index, default) in defaults.iter().enumerate() {
        if filled[index] {
            continue;
        }
        if let Some(default) = default {
            emit_value_as_arg_local(&arg_locals[index], default, param_kinds[index], module)?;
        } else {
            return Err(CompileError::new(
                call.span,
                "wasm32-web user function call is missing a required argument",
            ));
        }
    }

    for (local, kind) in arg_locals.iter().zip(param_kinds.iter().copied()) {
        match kind {
            LocalKind::Str => {
                module.body().line(&format!("local.get ${}_ptr", local));
                module.body().line(&format!("local.get ${}_len", local));
            }
            LocalKind::Array => {
                module.body().line(&format!("local.get ${}_ptr", local));
                module.body().line(&format!("local.get ${}_len", local));
            }
            LocalKind::Mixed => module.body().line(&format!("local.get ${}", local)),
            _ => module.body().line(&format!("local.get ${}", local)),
        }
    }
    Ok(())
}

fn expand_wasm_static_spread_args(args: &[Expr]) -> Result<Vec<Expr>, CompileError> {
    let expanded = call_args::expand_static_assoc_spread_args(args);
    let mut result = Vec::with_capacity(expanded.len());
    for arg in expanded {
        match &arg.kind {
            ExprKind::Spread(inner) => match &inner.kind {
                ExprKind::ArrayLiteral(values) => {
                    result.extend(
                        values
                            .iter()
                            .map(|value| Expr::new(value.kind.clone(), arg.span)),
                    );
                }
                _ => result.push(arg),
            },
            _ => result.push(arg),
        }
    }
    Ok(result)
}

pub(super) fn emit_callable_assign(
    name: &str,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match &value.kind {
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            if let Some((target, class_name, object)) =
                same_instance_callable_ternary_target(then_expr, else_expr, module)?
            {
                emit_condition(condition, module)?;
                module.body().line("drop");
                return assign_instance_callable_capture(name, object, target, class_name, module);
            }
            let Some(then_target) = static_callback_function_name(then_expr, module) else {
                return Err(CompileError::new(
                    then_expr.span,
                    "wasm32-web callable ternary arms require static callable targets",
                ));
            };
            let Some(else_target) = static_callback_function_name(else_expr, module) else {
                return Err(CompileError::new(
                    else_expr.span,
                    "wasm32-web callable ternary arms require static callable targets",
                ));
            };
            if !then_target.eq_ignore_ascii_case(&else_target) {
                return Err(CompileError::new(
                    value.span,
                    "wasm32-web callable ternaries require both arms to resolve to the same target",
                ));
            }
            emit_condition(condition, module)?;
            module.body().line("drop");
            module.set_callable_target(name, Some(then_target));
            Ok(())
        }
        ExprKind::FirstClassCallable(CallableTarget::Function(target))
            if module.has_function(&target.to_string()) || wasm_known_builtin_exists(&target.to_string()) =>
        {
            module.set_callable_target(name, Some(target.to_string()));
            Ok(())
        }
        ExprKind::FirstClassCallable(CallableTarget::Function(_)) => Err(CompileError::new(
            value.span,
            "wasm32-web callable variables currently require a user-defined function or known builtin target",
        )),
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            let class_name = module.class_name_for_receiver(receiver).ok_or_else(|| {
                CompileError::new(
                    value.span,
                    "wasm32-web static method callable variables require a class receiver",
                )
            })?;
            let method_info = module
                .object_static_method_in_hierarchy(&class_name, method)
                .ok_or_else(|| {
                    CompileError::new(
                        value.span,
                        "wasm32-web static method callable variables require fixed method metadata",
                    )
                })?;
            if !module.object_member_is_accessible(&method_info.owner_class, &method_info.visibility) {
                return Err(CompileError::new(
                    value.span,
                    "wasm32-web static method callable variables require visible fixed method metadata",
                ));
            }
            module.set_callable_target(name, Some(method_info.symbol));
            Ok(())
        }
        ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) => {
            let (target, class_name) = direct_instance_callable_target(value, object, method, module)?;
            assign_instance_callable_capture(name, object, target, class_name, module)
        }
        ExprKind::Variable(source) if module.callable_target(source).is_some() => {
            module.set_callable_target(name, module.callable_target(source));
            Ok(())
        }
        ExprKind::Variable(source) if module.callable_instance_target(source).is_some() => {
            let Some((target, source_capture)) = module.callable_instance_target(source) else {
                unreachable!("callable instance target was checked above")
            };
            let capture_local = format!("{}_callable_object", name);
            module.declare_object_local(capture_local.clone());
            module.body().line(&format!("local.get ${}", source_capture));
            module.body().line(&format!("local.set ${}", capture_local));
            module.set_object_class_for_local(&capture_local, module.object_class_for_local(&source_capture));
            module.set_callable_instance_target(name, Some((target, capture_local)));
            Ok(())
        }
        ExprKind::Closure { .. } | ExprKind::ClosureCall { .. } => Err(CompileError::new(
            value.span,
            "wasm32-web closure callable variables require callable runtime support",
        )),
        _ => Err(CompileError::new(
            value.span,
            "wasm32-web callable variables currently require a direct first-class user-function target",
        )),
    }
}

fn same_instance_callable_ternary_target<'a>(
    then_expr: &'a Expr,
    else_expr: &'a Expr,
    module: &WasmModule,
) -> Result<Option<(String, String, &'a Expr)>, CompileError> {
    let Some(targets) = instance_callable_ternary_targets(then_expr, else_expr, module)? else {
        return Ok(None);
    };
    if !targets
        .then_target
        .eq_ignore_ascii_case(&targets.else_target)
    {
        return Err(CompileError::new(
            else_expr.span,
            "wasm32-web callable ternaries require both arms to resolve to the same target",
        ));
    }
    Ok(Some((
        targets.then_target,
        targets.then_class_name,
        targets.then_object,
    )))
}

struct InstanceCallableTernaryTargets<'a> {
    then_object: &'a Expr,
    then_method: &'a str,
    then_target: String,
    then_class_name: String,
    else_object: &'a Expr,
    else_method: &'a str,
    else_target: String,
}

fn instance_callable_ternary_targets<'a>(
    then_expr: &'a Expr,
    else_expr: &'a Expr,
    module: &WasmModule,
) -> Result<Option<InstanceCallableTernaryTargets<'a>>, CompileError> {
    let ExprKind::FirstClassCallable(CallableTarget::Method {
        object: then_object,
        method: then_method,
    }) = &then_expr.kind
    else {
        return Ok(None);
    };
    let ExprKind::FirstClassCallable(CallableTarget::Method {
        object: else_object,
        method: else_method,
    }) = &else_expr.kind
    else {
        return Ok(None);
    };
    if !same_receiver_variable(then_object, else_object) {
        return Err(CompileError::new(
            else_object.span,
            "wasm32-web callable ternary instance-method arms require the same receiver variable",
        ));
    }
    let (then_target, then_class_name) =
        direct_instance_callable_target(then_expr, then_object, then_method, module)?;
    let (else_target, _) =
        direct_instance_callable_target(else_expr, else_object, else_method, module)?;
    Ok(Some(InstanceCallableTernaryTargets {
        then_object,
        then_method,
        then_target,
        then_class_name,
        else_object,
        else_method,
        else_target,
    }))
}

fn same_instance_callable_ternary_target_static(
    then_expr: &Expr,
    else_expr: &Expr,
    module: &WasmModule,
) -> Option<String> {
    let ExprKind::FirstClassCallable(CallableTarget::Method {
        object: then_object,
        method: then_method,
    }) = &then_expr.kind
    else {
        return None;
    };
    let ExprKind::FirstClassCallable(CallableTarget::Method {
        object: else_object,
        method: else_method,
    }) = &else_expr.kind
    else {
        return None;
    };
    if !same_receiver_variable(then_object, else_object) {
        return None;
    }
    let (then_target, _) =
        direct_instance_callable_target_static(then_object, then_method, module)?;
    let (else_target, _) =
        direct_instance_callable_target_static(else_object, else_method, module)?;
    then_target
        .eq_ignore_ascii_case(&else_target)
        .then_some(then_target)
}

struct StaticCallableTernaryTargets {
    then_target: String,
    else_target: String,
}

fn static_callable_ternary_targets(
    then_expr: &Expr,
    else_expr: &Expr,
    module: &WasmModule,
) -> Result<Option<StaticCallableTernaryTargets>, CompileError> {
    let ExprKind::FirstClassCallable(CallableTarget::StaticMethod {
        receiver: then_receiver,
        method: then_method,
    }) = &then_expr.kind
    else {
        return Ok(None);
    };
    let ExprKind::FirstClassCallable(CallableTarget::StaticMethod {
        receiver: else_receiver,
        method: else_method,
    }) = &else_expr.kind
    else {
        return Ok(None);
    };
    let then_target = direct_static_callable_target(then_expr, then_receiver, then_method, module)?;
    let else_target = direct_static_callable_target(else_expr, else_receiver, else_method, module)?;
    Ok(Some(StaticCallableTernaryTargets {
        then_target,
        else_target,
    }))
}

fn direct_static_callable_target(
    expr: &Expr,
    receiver: &crate::parser::ast::StaticReceiver,
    method: &str,
    module: &WasmModule,
) -> Result<String, CompileError> {
    let class_name = module.class_name_for_receiver(receiver).ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web static method callables require a class receiver",
        )
    })?;
    let method_info = module
        .object_static_method_in_hierarchy(&class_name, method)
        .ok_or_else(|| {
            CompileError::new(
                expr.span,
                "wasm32-web static method callables require fixed method metadata",
            )
        })?;
    if !module.object_member_is_accessible(&method_info.owner_class, &method_info.visibility) {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web static method callables require visible fixed method metadata",
        ));
    }
    Ok(method_info.symbol)
}

fn same_receiver_variable(left: &Expr, right: &Expr) -> bool {
    matches!(
        (&left.kind, &right.kind),
        (ExprKind::Variable(left_name), ExprKind::Variable(right_name)) if left_name == right_name
    )
}

fn assign_instance_callable_capture(
    name: &str,
    object: &Expr,
    target: String,
    class_name: String,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let capture_local = format!("{}_callable_object", name);
    module.declare_object_local(capture_local.clone());
    let kind = emit_expr(object, module)?;
    if kind != ValueKind::Object {
        return Err(CompileError::new(
            object.span,
            "wasm32-web method callable variables require an object receiver",
        ));
    }
    module.body().line(&format!("local.set ${}", capture_local));
    module.set_object_class_for_local(&capture_local, Some(class_name));
    module.set_callable_instance_target(name, Some((target, capture_local)));
    module.set_callable_target(name, None);
    Ok(())
}

fn emit_value_as_arg_local(
    local: &str,
    expr: &Expr,
    kind: LocalKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match kind {
        LocalKind::I64 => {
            emit_value_as_numeric_arg(expr, false, module)?;
            module.body().line(&format!("local.set ${}", local));
        }
        LocalKind::F64 => {
            emit_value_as_numeric_arg(expr, true, module)?;
            module.body().line(&format!("local.set ${}", local));
        }
        LocalKind::I32 => {
            emit_value_as_local_kind(expr, kind, module)?;
            module.body().line(&format!("local.set ${}", local));
        }
        LocalKind::Str => {
            emit_string_value_to_stack(expr, module)?;
            module.body().line(&format!("local.set ${}_len", local));
            module.body().line(&format!("local.set ${}_ptr", local));
        }
        LocalKind::Array => {
            emit_value_array_arg_assign(local, expr, module)?;
        }
        LocalKind::Object => {
            if matches!(expr.kind, ExprKind::Null) {
                module.body().line("i32.const 0");
                module.body().line(&format!("local.set ${}", local));
                return Ok(());
            }
            let kind = emit_expr(expr, module)?;
            if kind != ValueKind::Object {
                return Err(CompileError::new(
                    expr.span,
                    "wasm32-web object parameter expected object value",
                ));
            }
            module.body().line(&format!("local.set ${}", local));
        }
        LocalKind::Mixed => {
            emit_mixed_arg_assign(local, expr, module)?;
        }
        LocalKind::Callable => {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web callable parameters require callable runtime support",
            ));
        }
    }
    Ok(())
}

fn emit_value_as_numeric_arg(
    expr: &Expr,
    use_float: bool,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if known_mixed_numeric_kind(expr, module)?.is_some()
        || dynamic_numeric_operand_may_materialize(expr, module)?
    {
        emit_known_mixed_numeric_operand(expr, use_float, module)?;
        return Ok(());
    }
    if use_float {
        require_float(expr, module)
    } else {
        require_int(expr, module)
    }
}

fn emit_static_callable_target(
    expr: &Expr,
    target: &str,
    call_args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if module.has_function(target) {
        emit_user_function_args(expr, target, call_args, module)?;
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(target)));
        return module.function_return_kind(target).ok_or_else(|| {
            CompileError::new(expr.span, "wasm32-web callable target metadata is missing")
        });
    }
    if matches!(
        target.to_ascii_lowercase().as_str(),
        "call_user_func" | "call_user_func_array"
    ) {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web callable helper recursion is not supported yet",
        ));
    }
    let call = Expr::new(
        ExprKind::FunctionCall {
            name: Name::from(target),
            args: call_args.to_vec(),
        },
        expr.span,
    );
    if callable_builtin_return_kind(target, call_args, module) == Some(ValueKind::Str) {
        emit_string_value_to_stack(&call, module)?;
        return Ok(ValueKind::Str);
    }
    emit_expr(&call, module)
}

fn emit_instance_callable_ternary_dispatch(
    expr: &Expr,
    condition: &Expr,
    then_expr: &Expr,
    else_expr: &Expr,
    then_call_args: &[Expr],
    else_call_args: &[Expr],
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    let Some(targets) = instance_callable_ternary_targets(then_expr, else_expr, module)? else {
        return Ok(None);
    };
    emit_instance_callable_ternary_dispatch_with_targets(
        expr,
        condition,
        targets,
        then_call_args,
        else_call_args,
        module,
    )
    .map(Some)
}

fn emit_instance_callable_ternary_dispatch_with_targets(
    expr: &Expr,
    condition: &Expr,
    targets: InstanceCallableTernaryTargets<'_>,
    then_call_args: &[Expr],
    else_call_args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let then_kind = callable_return_kind(&targets.then_target, then_call_args, module)
        .ok_or_else(|| {
            CompileError::new(expr.span, "wasm32-web callable target metadata is missing")
        })?;
    let else_kind = callable_return_kind(&targets.else_target, else_call_args, module)
        .ok_or_else(|| {
            CompileError::new(expr.span, "wasm32-web callable target metadata is missing")
        })?;
    if then_kind != else_kind {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web callable ternary method arms require matching return kinds",
        ));
    }
    emit_condition(condition, module)?;
    module
        .body()
        .open(&format!("if (result {})", wasm_value_type(then_kind)));
    let emitted_then = emit_direct_instance_callable_target(
        expr,
        targets.then_object,
        targets.then_method,
        then_call_args,
        module,
    )?;
    if emitted_then != then_kind {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web callable ternary method metadata is inconsistent",
        ));
    }
    module.body().line("else");
    let emitted_else = emit_direct_instance_callable_target(
        expr,
        targets.else_object,
        targets.else_method,
        else_call_args,
        module,
    )?;
    if emitted_else != else_kind {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web callable ternary method metadata is inconsistent",
        ));
    }
    module.body().close("end");
    Ok(then_kind)
}

fn emit_static_callable_ternary_dispatch(
    expr: &Expr,
    condition: &Expr,
    then_expr: &Expr,
    else_expr: &Expr,
    then_call_args: &[Expr],
    else_call_args: &[Expr],
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    let Some(targets) = static_callable_ternary_targets(then_expr, else_expr, module)? else {
        return Ok(None);
    };
    emit_static_callable_ternary_dispatch_with_targets(
        expr,
        condition,
        targets,
        then_call_args,
        else_call_args,
        module,
    )
    .map(Some)
}

fn emit_static_callable_ternary_dispatch_with_targets(
    expr: &Expr,
    condition: &Expr,
    targets: StaticCallableTernaryTargets,
    then_call_args: &[Expr],
    else_call_args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let then_kind = callable_return_kind(&targets.then_target, then_call_args, module)
        .ok_or_else(|| {
            CompileError::new(expr.span, "wasm32-web callable target metadata is missing")
        })?;
    let else_kind = callable_return_kind(&targets.else_target, else_call_args, module)
        .ok_or_else(|| {
            CompileError::new(expr.span, "wasm32-web callable target metadata is missing")
        })?;
    if then_kind != else_kind {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web callable ternary static-method arms require matching return kinds",
        ));
    }
    emit_condition(condition, module)?;
    module
        .body()
        .open(&format!("if (result {})", wasm_value_type(then_kind)));
    let emitted_then = emit_static_callable_target(expr, &targets.then_target, then_call_args, module)?;
    if emitted_then != then_kind {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web callable ternary static-method metadata is inconsistent",
        ));
    }
    module.body().line("else");
    let emitted_else = emit_static_callable_target(expr, &targets.else_target, else_call_args, module)?;
    if emitted_else != else_kind {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web callable ternary static-method metadata is inconsistent",
        ));
    }
    module.body().close("end");
    Ok(then_kind)
}

pub(super) fn call_user_func_target<'a>(
    args: &'a [Expr],
    module: &WasmModule,
) -> Option<(String, &'a [Expr])> {
    let callback = args.first()?;
    let target = static_callback_function_name(callback, module)?;
    Some((target, &args[1..]))
}

pub(super) fn call_user_func_return_kind(
    args: &[Expr],
    module: &WasmModule,
) -> Option<ValueKind> {
    if let Some(Expr {
        kind: ExprKind::Variable(var),
        ..
    }) = args.first()
    {
        if let Some((target, _)) = module.callable_instance_target(var) {
            return callable_return_kind(&target, &args[1..], module);
        }
        if let Some(target) = invokable_object_target(module, var) {
            return callable_return_kind(&target, &args[1..], module);
        }
    }
    if let Some(Expr {
        kind: ExprKind::FirstClassCallable(CallableTarget::Method { object, method }),
        ..
    }) = args.first()
    {
        let (target, _) = direct_instance_callable_target_static(object, method, module)?;
        return callable_return_kind(&target, &args[1..], module);
    }
    if let Some(Expr {
        kind:
            ExprKind::Ternary {
                condition: _,
                then_expr,
                else_expr,
            },
        ..
    }) = args.first()
    {
        if let Some(kind) = instance_callable_ternary_return_kind(
            then_expr,
            else_expr,
            &args[1..],
            &args[1..],
            module,
        ) {
            return Some(kind);
        }
        if let Some(kind) =
            static_callable_ternary_return_kind(then_expr, else_expr, &args[1..], &args[1..], module)
        {
            return Some(kind);
        }
        if let Some(target) = same_instance_callable_ternary_target_static(then_expr, else_expr, module) {
            return callable_return_kind(&target, &args[1..], module);
        }
        if let Some(callback) = args.first() {
            let target = static_callback_function_name(callback, module)?;
            return callable_return_kind(&target, &args[1..], module);
        }
    }
    if let Some(callback) = args.first() {
        if let Some((target, _)) = direct_instance_callable_target_static(callback, "__invoke", module)
        {
            return callable_return_kind(&target, &args[1..], module);
        }
        if let Some(target) = callable_array_target(callback, module) {
            return callable_return_kind(callable_array_target_name(&target), &args[1..], module);
        }
    }
    call_user_func_target(args, module).and_then(|(target, call_args)| {
        callable_return_kind(&target, call_args, module)
    })
    .or_else(|| dynamic_call_user_func_return_kind(args, module).ok().flatten())
}

fn evaluated_call_user_func_target<'a>(
    args: &'a [Expr],
    module: &mut WasmModule,
) -> Result<Option<(String, &'a [Expr])>, CompileError> {
    let callback = match args.first() {
        Some(callback) => callback,
        None => return Ok(None),
    };
    let Some(target) = evaluated_static_callback_function_name(callback, module)? else {
        return Ok(None);
    };
    Ok(Some((target, &args[1..])))
}

fn dynamic_call_user_func_return_kind(
    args: &[Expr],
    module: &WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    let Some(callback) = args.first() else {
        return Ok(None);
    };
    let Some(callbacks) = dynamic_call_user_func_callback_names(callback, module) else {
        return Ok(None);
    };
    let mut return_kind = None;
    for target in callbacks {
        if !module.has_function(&target) {
            return Err(CompileError::new(
                callback.span,
                "wasm32-web dynamic call_user_func() callback helper can only return declared user functions",
            ));
        }
        let Some(kind) = callable_return_kind(&target, &args[1..], module) else {
            return Ok(None);
        };
        if return_kind.is_some_and(|existing| existing != kind) {
            return Err(CompileError::new(
                callback.span,
                "wasm32-web dynamic call_user_func() callback helper currently requires callbacks with matching return kinds",
            ));
        }
        return_kind = Some(kind);
    }
    Ok(return_kind)
}

fn dynamic_call_user_func_callback_names(expr: &Expr, module: &WasmModule) -> Option<Vec<String>> {
    match &expr.kind {
        ExprKind::FunctionCall { name, .. } => module
            .function_possible_static_string_returns(name.as_str())
            .map(<[_]>::to_vec),
        ExprKind::Variable(name) => module.possible_static_string_values(name).map(<[_]>::to_vec),
        _ => None,
    }
}

fn emit_dynamic_call_user_func_dispatch(
    expr: &Expr,
    args: &[Expr],
    kind: ValueKind,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let callback_expr = args.first().expect("dynamic call_user_func callback exists");
    let callbacks = dynamic_call_user_func_callback_names(callback_expr, module)
        .expect("dynamic call_user_func callback names were validated");
    let callback_ptr = module.next_label("call_user_func_callback_ptr");
    let callback_len = module.next_label("call_user_func_callback_len");
    let matched = module.next_label("call_user_func_callback_matched");
    for local in [&callback_ptr, &callback_len, &matched] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    let result = module.next_label("call_user_func_result");
    let result_aux = module.next_label("call_user_func_result_aux");
    declare_dynamic_call_result_locals(kind, &result, &result_aux, module);
    emit_string_value_to_stack(callback_expr, module)?;
    module.body().line(&format!("local.set {}", callback_len));
    module.body().line(&format!("local.set {}", callback_ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));

    for callback in callbacks {
        let candidate_ptr = module.next_label("call_user_func_candidate_ptr");
        let candidate_len = module.next_label("call_user_func_candidate_len");
        let candidate_match = module.next_label("call_user_func_candidate_match");
        for local in [&candidate_ptr, &candidate_len, &candidate_match] {
            module.declare_i32_local(local.trim_start_matches('$').to_string());
        }
        let (ptr, len) = module.intern_string(&callback);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("local.set {}", candidate_ptr));
        module.body().line(&format!("i32.const {}", len));
        module.body().line(&format!("local.set {}", candidate_len));
        emit_string_parts_equal(
            &callback_ptr,
            &callback_len,
            &candidate_ptr,
            &candidate_len,
            &candidate_match,
            module,
        );
        module.body().line(&format!("local.get {}", candidate_match));
        module.body().open("if");
        let emitted = emit_static_callable_target(expr, &callback, &args[1..], module)?;
        if emitted != kind {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web dynamic call_user_func() metadata is inconsistent",
            ));
        }
        store_dynamic_call_result(kind, &result, &result_aux, module);
        module.body().line("i32.const 1");
        module.body().line(&format!("local.set {}", matched));
        module.body().close("end");
    }

    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    load_dynamic_call_result(kind, &result, &result_aux, module);
    Ok(kind)
}

fn declare_dynamic_call_result_locals(
    kind: ValueKind,
    result: &str,
    result_aux: &str,
    module: &mut WasmModule,
) {
    match kind {
        ValueKind::Int => module.declare_i64_local(result.trim_start_matches('$').to_string()),
        ValueKind::Float => module.declare_f64_local(result.trim_start_matches('$').to_string()),
        ValueKind::Bool | ValueKind::Mixed | ValueKind::Object => {
            module.declare_i32_local(result.trim_start_matches('$').to_string());
        }
        ValueKind::Str | ValueKind::Array => {
            module.declare_i32_local(result.trim_start_matches('$').to_string());
            module.declare_i32_local(result_aux.trim_start_matches('$').to_string());
        }
        ValueKind::Null | ValueKind::Never => {}
    }
}

fn store_dynamic_call_result(
    kind: ValueKind,
    result: &str,
    result_aux: &str,
    module: &mut WasmModule,
) {
    match kind {
        ValueKind::Str | ValueKind::Array => {
            module.body().line(&format!("local.set {}", result_aux));
            module.body().line(&format!("local.set {}", result));
        }
        ValueKind::Int | ValueKind::Float | ValueKind::Bool | ValueKind::Mixed | ValueKind::Object => {
            module.body().line(&format!("local.set {}", result));
        }
        ValueKind::Null | ValueKind::Never => {}
    }
}

fn load_dynamic_call_result(
    kind: ValueKind,
    result: &str,
    result_aux: &str,
    module: &mut WasmModule,
) {
    match kind {
        ValueKind::Str | ValueKind::Array => {
            module.body().line(&format!("local.get {}", result));
            module.body().line(&format!("local.get {}", result_aux));
        }
        ValueKind::Int | ValueKind::Float | ValueKind::Bool | ValueKind::Mixed | ValueKind::Object => {
            module.body().line(&format!("local.get {}", result));
        }
        ValueKind::Null | ValueKind::Never => {}
    }
}

pub(super) fn call_user_func_array_target_owned(
    args: &[Expr],
    module: &WasmModule,
) -> Option<(String, Vec<Expr>)> {
    let [callback, packed_args] = args else {
        return None;
    };
    let target = static_callback_function_name(callback, module)?;
    call_user_func_array_target_from_parts(&target, packed_args, module)
}

pub(super) fn call_user_func_array_return_kind(
    args: &[Expr],
    module: &WasmModule,
) -> Option<ValueKind> {
    let [callback, packed_args] = args else {
        return None;
    };
    if let ExprKind::Variable(var) = &callback.kind {
        if let Some((target, _)) = module.callable_instance_target(var) {
            let (_, call_args) = call_user_func_array_target_from_parts(&target, packed_args, module)?;
            return callable_return_kind(&target, call_args.as_slice(), module);
        }
        if let Some(target) = invokable_object_target(module, var) {
            let (_, call_args) = call_user_func_array_target_from_parts(&target, packed_args, module)?;
            return callable_return_kind(&target, call_args.as_slice(), module);
        }
    }
    if let ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) = &callback.kind {
        let (target, _) = direct_instance_callable_target_static(object, method, module)?;
        let (_, call_args) = call_user_func_array_target_from_parts(&target, packed_args, module)?;
        return callable_return_kind(&target, call_args.as_slice(), module);
    }
    if let ExprKind::Ternary {
        condition: _,
        then_expr,
        else_expr,
    } = &callback.kind
    {
        if let Some(targets) = instance_callable_ternary_targets_static(then_expr, else_expr, module)
        {
            if let (Some((_, then_call_args)), Some((_, else_call_args))) = (
                call_user_func_array_target_from_parts(&targets.then_target, packed_args, module),
                call_user_func_array_target_from_parts(&targets.else_target, packed_args, module),
            ) {
                return compatible_callable_return_kind(
                    &targets.then_target,
                    then_call_args.as_slice(),
                    &targets.else_target,
                    else_call_args.as_slice(),
                    module,
                );
            }
        }
        if let Some(targets) = static_callable_ternary_targets_static(then_expr, else_expr, module)
        {
            if let (Some((_, then_call_args)), Some((_, else_call_args))) = (
                call_user_func_array_target_from_parts(&targets.then_target, packed_args, module),
                call_user_func_array_target_from_parts(&targets.else_target, packed_args, module),
            ) {
                return compatible_callable_return_kind(
                    &targets.then_target,
                    then_call_args.as_slice(),
                    &targets.else_target,
                    else_call_args.as_slice(),
                    module,
                );
            }
        }
        let target = same_instance_callable_ternary_target_static(then_expr, else_expr, module)
            .or_else(|| static_callback_function_name(callback, module))?;
        let (_, call_args) = call_user_func_array_target_from_parts(&target, packed_args, module)?;
        return callable_return_kind(&target, call_args.as_slice(), module);
    }
    if let Some((target, _)) = direct_instance_callable_target_static(callback, "__invoke", module) {
        let (_, call_args) = call_user_func_array_target_from_parts(&target, packed_args, module)?;
        return callable_return_kind(&target, call_args.as_slice(), module);
    }
    if let Some(target) = callable_array_target(callback, module) {
        let (_, call_args) =
            call_user_func_array_target_from_parts(callable_array_target_name(&target), packed_args, module)?;
        return callable_return_kind(callable_array_target_name(&target), call_args.as_slice(), module);
    }
    call_user_func_array_target_owned(args, module).and_then(|(target, call_args)| {
        callable_return_kind(&target, call_args.as_slice(), module)
    })
}

fn instance_callable_ternary_return_kind(
    then_expr: &Expr,
    else_expr: &Expr,
    then_call_args: &[Expr],
    else_call_args: &[Expr],
    module: &WasmModule,
) -> Option<ValueKind> {
    let targets = instance_callable_ternary_targets_static(then_expr, else_expr, module)?;
    compatible_callable_return_kind(
        &targets.then_target,
        then_call_args,
        &targets.else_target,
        else_call_args,
        module,
    )
}

fn static_callable_ternary_return_kind(
    then_expr: &Expr,
    else_expr: &Expr,
    then_call_args: &[Expr],
    else_call_args: &[Expr],
    module: &WasmModule,
) -> Option<ValueKind> {
    let targets = static_callable_ternary_targets_static(then_expr, else_expr, module)?;
    compatible_callable_return_kind(
        &targets.then_target,
        then_call_args,
        &targets.else_target,
        else_call_args,
        module,
    )
}

fn compatible_callable_return_kind(
    then_target: &str,
    then_call_args: &[Expr],
    else_target: &str,
    else_call_args: &[Expr],
    module: &WasmModule,
) -> Option<ValueKind> {
    let then_kind = callable_return_kind(then_target, then_call_args, module)?;
    let else_kind = callable_return_kind(else_target, else_call_args, module)?;
    (then_kind == else_kind).then_some(then_kind)
}

enum CallableArrayTarget<'a> {
    Static(String),
    Instance {
        target: String,
        receiver: &'a Expr,
        method: String,
    },
}

fn callable_array_target_name<'a>(target: &'a CallableArrayTarget<'_>) -> &'a str {
    match target {
        CallableArrayTarget::Static(target) => target,
        CallableArrayTarget::Instance { target, .. } => target,
    }
}

fn callable_array_target<'a>(
    callback: &'a Expr,
    module: &WasmModule,
) -> Option<CallableArrayTarget<'a>> {
    match &callback.kind {
        ExprKind::ArrayLiteral(items) => {
            let [receiver, method] = items.as_slice() else {
                return None;
            };
            fixed_callable_array_pair_target(receiver, method, module)
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            let receiver = fixed_callable_assoc_value(items, 0, module)?;
            let method = fixed_callable_assoc_value(items, 1, module)?;
            fixed_callable_array_pair_target(receiver, method, module)
        }
        _ => None,
    }
}

fn fixed_callable_assoc_value<'a>(
    items: &'a [(Expr, Expr)],
    needle: i64,
    module: &WasmModule,
) -> Option<&'a Expr> {
    items
        .iter()
        .rev()
        .find_map(|(key, value)| {
            matches!(
                static_assoc_access_key(key, module),
                Some(AssocKeyValue::Int(value)) if value == needle
            )
            .then_some(value)
        })
}

fn fixed_callable_array_pair_target<'a>(
    receiver: &'a Expr,
    method: &Expr,
    module: &WasmModule,
) -> Option<CallableArrayTarget<'a>> {
    let method = fixed_callable_static_string_value(method, module)?;
    if let Some((target, _)) = direct_instance_callable_target_static(receiver, &method, module) {
        return Some(CallableArrayTarget::Instance {
            target,
            receiver,
            method,
        });
    }
    let class_name = fixed_callable_static_string_value(receiver, module)?;
    let method_info = module.object_static_method_in_hierarchy(&class_name, &method)?;
    module
        .object_member_is_accessible(&method_info.owner_class, &method_info.visibility)
        .then_some(CallableArrayTarget::Static(method_info.symbol))
}

fn fixed_callable_static_string_value(expr: &Expr, module: &WasmModule) -> Option<String> {
    match &expr.kind {
        ExprKind::Variable(name) => module.string_static_value(name),
        _ => static_string_value(expr, module),
    }
}

fn emit_callable_array_target(
    expr: &Expr,
    target: CallableArrayTarget<'_>,
    call_args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    match target {
        CallableArrayTarget::Static(target) => {
            emit_static_callable_target(expr, &target, call_args, module)
        }
        CallableArrayTarget::Instance {
            receiver, method, ..
        } => {
            emit_direct_instance_callable_target(expr, receiver, &method, call_args, module)
        }
    }
}

struct StaticInstanceCallableTernaryTargets {
    then_target: String,
    else_target: String,
}

fn instance_callable_ternary_targets_static(
    then_expr: &Expr,
    else_expr: &Expr,
    module: &WasmModule,
) -> Option<StaticInstanceCallableTernaryTargets> {
    let ExprKind::FirstClassCallable(CallableTarget::Method {
        object: then_object,
        method: then_method,
    }) = &then_expr.kind
    else {
        return None;
    };
    let ExprKind::FirstClassCallable(CallableTarget::Method {
        object: else_object,
        method: else_method,
    }) = &else_expr.kind
    else {
        return None;
    };
    if !same_receiver_variable(then_object, else_object) {
        return None;
    }
    let (then_target, _) =
        direct_instance_callable_target_static(then_object, then_method, module)?;
    let (else_target, _) =
        direct_instance_callable_target_static(else_object, else_method, module)?;
    Some(StaticInstanceCallableTernaryTargets {
        then_target,
        else_target,
    })
}

fn static_callable_ternary_targets_static(
    then_expr: &Expr,
    else_expr: &Expr,
    module: &WasmModule,
) -> Option<StaticInstanceCallableTernaryTargets> {
    let ExprKind::FirstClassCallable(CallableTarget::StaticMethod {
        receiver: then_receiver,
        method: then_method,
    }) = &then_expr.kind
    else {
        return None;
    };
    let ExprKind::FirstClassCallable(CallableTarget::StaticMethod {
        receiver: else_receiver,
        method: else_method,
    }) = &else_expr.kind
    else {
        return None;
    };
    let then_target = static_callback_function_name(
        &Expr::new(
            ExprKind::FirstClassCallable(CallableTarget::StaticMethod {
                receiver: then_receiver.clone(),
                method: then_method.clone(),
            }),
            then_expr.span,
        ),
        module,
    )?;
    let else_target = static_callback_function_name(
        &Expr::new(
            ExprKind::FirstClassCallable(CallableTarget::StaticMethod {
                receiver: else_receiver.clone(),
                method: else_method.clone(),
            }),
            else_expr.span,
        ),
        module,
    )?;
    Some(StaticInstanceCallableTernaryTargets {
        then_target,
        else_target,
    })
}

fn call_user_func_array_target_from_parts(
    target: &str,
    packed_args: &Expr,
    module: &WasmModule,
) -> Option<(String, Vec<Expr>)> {
    let (call_args, has_named_arg) = match &packed_args.kind {
        ExprKind::ArrayLiteral(call_args) => (call_args.clone(), false),
        ExprKind::ArrayLiteralAssoc(items) => {
            call_user_func_array_assoc_literal_args(items, packed_args.span, module)?
        }
        ExprKind::FunctionCall { name, args }
            if matches!(name.to_ascii_lowercase().as_str(), "range" | "array_fill") =>
        {
            call_user_func_array_direct_builder_args(name.as_str(), packed_args, args, module)?
        }
        ExprKind::Variable(name)
            if module.local_kind(name) == Some(LocalKind::Array)
                && module.array_layout(name) == ArrayLayout::Assoc =>
        {
            call_user_func_array_assoc_local_args(name, packed_args.span, module)?
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Array) => {
            let len = module.array_length(name)?;
            let call_args = (0..len)
                .map(|index| {
                    Expr::new(
                        ExprKind::ArrayAccess {
                            array: Box::new(Expr::new(
                                ExprKind::Variable(name.clone()),
                                packed_args.span,
                            )),
                            index: Box::new(Expr::new(
                                ExprKind::IntLiteral(index as i64),
                                packed_args.span,
                            )),
                        },
                        packed_args.span,
                    )
                })
                .collect();
            (call_args, false)
        }
        _ => return None,
    };
    let call_args = if has_named_arg && !module.has_function(&target) {
        normalize_builtin_named_call_args(target, &call_args, packed_args.span)?
    } else {
        call_args
    };
    if !module.has_function(target) && module.function_return_kind(target).is_none() {
        if callable_builtin_return_kind(target, call_args.as_slice(), module).is_none() {
            return None;
        }
    }
    Some((target.to_string(), call_args))
}

fn call_user_func_array_direct_builder_args(
    name: &str,
    packed_args: &Expr,
    args: &[Expr],
    module: &WasmModule,
) -> Option<(Vec<Expr>, bool)> {
    if name.eq_ignore_ascii_case("range") {
        return static_range_items_if_possible(packed_args, args, module)
            .ok()
            .flatten()
            .map(|items| (items, false));
    }
    if !name.eq_ignore_ascii_case("array_fill") || args.len() != 3 {
        return None;
    }
    static_array_fill_call_args(args).map(|items| (items, false))
}

fn static_array_fill_call_args(args: &[Expr]) -> Option<Vec<Expr>> {
    let count = static_or_const_int_value(&args[1])?;
    let count = usize::try_from(count).ok()?;
    array_fill_value_is_static_scalar(&args[2]).then(|| vec![args[2].clone(); count])
}

fn array_fill_value_is_static_scalar(value: &Expr) -> bool {
    matches!(
        value.kind,
        ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::Null
            | ExprKind::ConstRef(_)
    )
}

fn evaluated_call_user_func_array_target_owned(
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<Option<(String, Vec<Expr>)>, CompileError> {
    let [callback, packed_args] = args else {
        return Ok(None);
    };
    let Some(target) = evaluated_static_callback_function_name(callback, module)? else {
        return Ok(None);
    };
    let Some((target, call_args)) = call_user_func_array_target_from_parts(&target, packed_args, module) else {
        return Ok(None);
    };
    Ok(Some((target, call_args)))
}

fn call_user_func_array_assoc_local_args(
    name: &str,
    span: Span,
    module: &WasmModule,
) -> Option<(Vec<Expr>, bool)> {
    let keys = module.array_key_values(name)?;
    let mut has_named_arg = false;
    let mut call_args = Vec::with_capacity(keys.len());
    for key in keys {
        match key {
            AssocKeyValue::Int(index) => {
                call_args.push(call_user_func_array_local_access(
                    name,
                    ExprKind::IntLiteral(*index),
                    span,
                ));
            }
            AssocKeyValue::Str(key) => {
                has_named_arg = true;
                call_args.push(Expr::new(
                    ExprKind::NamedArg {
                        name: key.clone(),
                        value: Box::new(call_user_func_array_local_access(
                            name,
                            ExprKind::StringLiteral(key.clone()),
                            span,
                        )),
                    },
                    span,
                ));
            }
        }
    }
    Some((call_args, has_named_arg))
}

fn call_user_func_array_assoc_literal_args(
    items: &[(Expr, Expr)],
    span: Span,
    module: &WasmModule,
) -> Option<(Vec<Expr>, bool)> {
    let mut has_named_arg = false;
    let mut call_args = Vec::with_capacity(items.len());
    for (key, value) in items {
        if static_or_const_int_value(key).is_some() {
            call_args.push(value.clone());
            continue;
        }
        let name = static_string_value(key, module)?;
        has_named_arg = true;
        call_args.push(Expr::new(
            ExprKind::NamedArg {
                name,
                value: Box::new(value.clone()),
            },
            span,
        ));
    }
    Some((call_args, has_named_arg))
}

fn call_user_func_array_local_access(name: &str, index: ExprKind, span: Span) -> Expr {
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(Expr::new(ExprKind::Variable(name.to_string()), span)),
            index: Box::new(Expr::new(index, span)),
        },
        span,
    )
}

fn normalize_builtin_named_call_args(
    target: &str,
    call_args: &[Expr],
    span: Span,
) -> Option<Vec<Expr>> {
    let sig = builtin_call_sig(target)?;
    call_args::plan_call_args(&sig, call_args, span, true, false)
        .ok()
        .map(|plan| plan.normalized_args())
}

pub(super) fn callable_return_kind(
    target: &str,
    call_args: &[Expr],
    module: &WasmModule,
) -> Option<ValueKind> {
    module
        .function_return_kind(target)
        .or_else(|| callable_builtin_return_kind(target, call_args, module))
}

fn callable_builtin_return_kind(
    target: &str,
    call_args: &[Expr],
    module: &WasmModule,
) -> Option<ValueKind> {
    if is_output_string_builtin(target) {
        return Some(ValueKind::Str);
    }
    if is_output_optional_int_builtin(target) {
        return Some(ValueKind::Int);
    }
    if matches!(
        target.to_ascii_lowercase().as_str(),
        "strlen" | "ord" | "intdiv" | "intval" | "count" | "array_sum" | "array_product"
    ) {
        return Some(ValueKind::Int);
    }
    if target.eq_ignore_ascii_case("abs") {
        return if call_args
            .first()
            .is_some_and(|arg| expression_is_floaty(arg, module))
        {
            Some(ValueKind::Float)
        } else {
            Some(ValueKind::Int)
        };
    }
    if target.eq_ignore_ascii_case("min") || target.eq_ignore_ascii_case("max") {
        return if call_args.iter().any(|arg| expression_is_floaty(arg, module)) {
            Some(ValueKind::Float)
        } else {
            Some(ValueKind::Int)
        };
    }
    if matches!(
        target.to_ascii_lowercase().as_str(),
        "floatval"
            | "fdiv"
            | "floor"
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
        return Some(ValueKind::Float);
    }
    if target.eq_ignore_ascii_case("json_last_error") {
        return Some(ValueKind::Int);
    }
    if matches!(
        target.to_ascii_lowercase().as_str(),
        "boolval"
            | "empty"
            | "is_numeric"
            | "is_nan"
            | "is_finite"
            | "is_infinite"
            | "is_int"
            | "is_float"
            | "is_bool"
            | "is_null"
            | "is_string"
            | "is_iterable"
            | "is_callable"
            | "method_exists"
            | "property_exists"
            | "isset"
            | "json_validate"
            | "function_exists"
            | "class_exists"
            | "interface_exists"
            | "trait_exists"
            | "enum_exists"
            | "str_contains"
            | "str_starts_with"
            | "str_ends_with"
            | "ctype_alpha"
            | "ctype_digit"
            | "ctype_alnum"
            | "ctype_space"
    ) {
        return Some(ValueKind::Bool);
    }
    None
}

pub(super) fn emit_callable_variable_call(
    expr: &Expr,
    var: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if let Some((target, capture_local)) = module.callable_instance_target(var) {
        return emit_instance_callable_target(expr, &target, &capture_local, args, module);
    }
    if let Some(target) = invokable_object_target(module, var) {
        return emit_instance_callable_target(expr, &target, var, args, module);
    }
    let Some(target) = callable_variable_target(module, var) else {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web callable variable calls require a direct first-class or compile-time-known string target",
        ));
    };
    emit_static_callable_target(expr, &target, args, module)
}

pub(super) fn emit_callable_expr_call(
    expr: &Expr,
    callee: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if direct_instance_callable_target_static(callee, "__invoke", module).is_some() {
        return emit_direct_instance_callable_target(expr, callee, "__invoke", args, module);
    }
    Err(CompileError::new(
        expr.span,
        "wasm32-web expression calls require a supported invokable object",
    ))
}

pub(super) fn callable_variable_target(module: &WasmModule, var: &str) -> Option<String> {
    module.callable_target(var).or_else(|| module.string_static_value(var))
}

pub(super) fn callable_expr_return_kind(
    module: &WasmModule,
    callee: &Expr,
    args: &[Expr],
) -> Option<ValueKind> {
    direct_instance_callable_target_static(callee, "__invoke", module)
        .and_then(|(target, _)| callable_return_kind(&target, args, module))
}

pub(super) fn callable_variable_return_kind(
    module: &WasmModule,
    var: &str,
    args: &[Expr],
) -> Option<ValueKind> {
    if let Some((target, _)) = module.callable_instance_target(var) {
        return callable_return_kind(&target, args, module);
    }
    if let Some(target) = invokable_object_target(module, var) {
        return callable_return_kind(&target, args, module);
    }
    callable_variable_target(module, var).and_then(|target| callable_return_kind(&target, args, module))
}

pub(super) fn object_expr_is_invokable(module: &WasmModule, object: &Expr) -> bool {
    direct_instance_callable_target_static(object, "__invoke", module).is_some()
}

fn invokable_object_target(module: &WasmModule, var: &str) -> Option<String> {
    let object = Expr::new(ExprKind::Variable(var.to_string()), Span::new(0, 0));
    direct_instance_callable_target_static(&object, "__invoke", module)
        .map(|(target, _)| target)
}

fn emit_instance_callable_target(
    expr: &Expr,
    target: &str,
    capture_local: &str,
    call_args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let mut method_args = Vec::with_capacity(call_args.len() + 1);
    method_args.push(Expr::new(
        ExprKind::Variable(capture_local.to_string()),
        expr.span,
    ));
    method_args.extend(call_args.iter().cloned());
    emit_user_function_args(expr, target, &method_args, module)?;
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(target)));
    module.function_return_kind(target).ok_or_else(|| {
        CompileError::new(expr.span, "wasm32-web callable target metadata is missing")
    })
}

fn emit_direct_instance_callable_target(
    expr: &Expr,
    object: &Expr,
    method: &str,
    call_args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let (target, class_name) = direct_instance_callable_target(expr, object, method, module)?;
    let capture_local = module
        .next_label("callable_object")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(capture_local.clone());
    let kind = emit_expr(object, module)?;
    if kind != ValueKind::Object {
        return Err(CompileError::new(
            object.span,
            "wasm32-web method callable variables require an object receiver",
        ));
    }
    module.body().line(&format!("local.set ${}", capture_local));
    module.set_object_class_for_local(&capture_local, Some(class_name));
    emit_instance_callable_target(expr, &target, &capture_local, call_args, module)
}

fn direct_instance_callable_target(
    expr: &Expr,
    object: &Expr,
    method: &str,
    module: &WasmModule,
) -> Result<(String, String), CompileError> {
    direct_instance_callable_target_static(object, method, module).ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web instance method callables require fixed visible method metadata",
        )
    })
}

fn direct_instance_callable_target_static(
    object: &Expr,
    method: &str,
    module: &WasmModule,
) -> Option<(String, String)> {
    let class_name = object_class_name_for_expr(object, module)?;
    module.object_class(&class_name)?;
    let (declaring_class, method_info) = module.object_method_in_hierarchy(&class_name, method)?;
    if !module.object_member_is_accessible(&method_info.owner_class, &method_info.visibility) {
        return None;
    }
    if !declaring_class.eq_ignore_ascii_case(&class_name)
        && method_body_uses_this_property(&method_info.body)
        && !inherited_property_layout_available(&class_name, &declaring_class, module)
    {
        return None;
    }
    Some((method_info.symbol, class_name))
}
