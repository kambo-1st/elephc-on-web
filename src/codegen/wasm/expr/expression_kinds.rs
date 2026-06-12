//! Purpose:
//! Provides wasm32-web expression kind classifiers used by scalar, string, and array lowering.
//! Keeps type-shape probing helpers out of the expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` and focused expression lowering modules.
//!
//! Key details:
//! - Helpers inspect static AST shape plus wasm local/function metadata without emitting code.

use super::*;

pub(in crate::codegen::wasm) fn expression_is_inty(expr: &Expr, module: &WasmModule) -> bool {
    match &expr.kind {
        ExprKind::IntLiteral(_) => true,
        ExprKind::Variable(name) => module.local_kind(name) == Some(LocalKind::I64),
        ExprKind::PropertyAccess { object, property } => {
            object_property_value_kind(object, property, module) == Some(ValueKind::Int)
        }
        ExprKind::DynamicPropertyAccess { object, property } => {
            object_dynamic_property_value_kind(object, property, module) == Some(ValueKind::Int)
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            object_expr_is_known_non_null(object, module)
                && object_property_value_kind(object, property, module) == Some(ValueKind::Int)
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            object_expr_is_known_non_null(object, module)
                && object_dynamic_property_value_kind(object, property, module) == Some(ValueKind::Int)
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            static_property_value_kind(receiver, property, module) == Some(ValueKind::Int)
        }
        ExprKind::ConstRef(name) => matches!(module.constant_value(name), Some(ConstantValue::Int(_))),
        ExprKind::ScopedConstantAccess { receiver, name } => {
            matches!(
                module.class_constant_value(receiver, name),
                Some(ConstantValue::Int(_))
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("call_user_func") => {
            call_user_func_return_kind(args, module) == Some(ValueKind::Int)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("call_user_func_array") => {
            call_user_func_array_return_kind(args, module) == Some(ValueKind::Int)
        }
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("count")
                && args.len() == 1
                && (expression_is_arrayy(&args[0], module) || expression_has_array_type(&args[0], module)) =>
        {
            true
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("array_search") => false,
        ExprKind::FunctionCall { name, .. } => module.function_return_kind(name) == Some(ValueKind::Int),
        ExprKind::ClosureCall { var, args } => {
            callable_variable_return_kind(module, var, args) == Some(ValueKind::Int)
        }
        ExprKind::ExprCall { callee, args } => {
            callable_expr_return_kind(module, callee, args) == Some(ValueKind::Int)
        }
        ExprKind::Cast {
            target: CastType::Int,
            ..
        } => true,
        ExprKind::BinaryOp { op, left, right } => {
            matches!(
                op,
                BinOp::Add
                    | BinOp::Sub
                    | BinOp::Mul
                    | BinOp::Mod
                    | BinOp::BitAnd
                    | BinOp::BitOr
                    | BinOp::BitXor
                    | BinOp::ShiftLeft
                    | BinOp::ShiftRight
            ) && expression_is_inty(left, module)
                && expression_is_inty(right, module)
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => expression_is_inty(then_expr, module) && expression_is_inty(else_expr, module),
        ExprKind::ShortTernary { value, default } => {
            expression_is_inty(value, module) && expression_is_inty(default, module)
        }
        ExprKind::Match { arms, default, .. } => {
            let mut values = arms.iter().map(|(_, value)| value).chain(default.as_deref());
            values
                .clone()
                .next()
                .is_some()
                && values.all(|value| expression_is_inty(value, module))
        }
        ExprKind::ArrayAccess { array, index } => {
            assoc_static_access_kind(array, index, module) == Some(ValueCellKind::Int)
                || method_call_assoc_static_access_kind(array, index, module)
                    == Some(ValueCellKind::Int)
                || static_property_array_access_kind(array, index, module) == Some(ValueCellKind::Int)
                || nested_array_static_access_kind(array, index, module) == Some(ValueCellKind::Int)
                || dynamic_outer_nested_assoc_static_access_kind(array, index, module)
                    == Some(ValueCellKind::Int)
                || dynamic_parent_nested_assoc_static_access_kind(array, index, module)
                    == Some(ValueCellKind::Int)
                || direct_array_chunk_first_scalar_kind(array, index, module) == Some(ValueCellKind::Int)
        }
        _ => false,
    }
}

pub(in crate::codegen::wasm) fn expression_is_floaty(expr: &Expr, module: &WasmModule) -> bool {
    match &expr.kind {
        ExprKind::FloatLiteral(_) => true,
        ExprKind::Variable(name) => module.local_kind(name) == Some(LocalKind::F64),
        ExprKind::PropertyAccess { object, property } => {
            object_property_value_kind(object, property, module) == Some(ValueKind::Float)
        }
        ExprKind::DynamicPropertyAccess { object, property } => {
            object_dynamic_property_value_kind(object, property, module) == Some(ValueKind::Float)
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            object_expr_is_known_non_null(object, module)
                && object_property_value_kind(object, property, module) == Some(ValueKind::Float)
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            object_expr_is_known_non_null(object, module)
                && object_dynamic_property_value_kind(object, property, module) == Some(ValueKind::Float)
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            static_property_value_kind(receiver, property, module) == Some(ValueKind::Float)
        }
        ExprKind::Negate(inner) => expression_is_floaty(inner, module),
        ExprKind::BitNot(_) => false,
        ExprKind::BinaryOp {
            op: BinOp::Div | BinOp::Pow,
            ..
        } => true,
        ExprKind::FunctionCall { name, .. }
            if module.function_return_kind(name) == Some(ValueKind::Float) =>
        {
            true
        }
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("array_reduce")
                && array_reduce_static_callback_return_kind(args, module) == Some(ValueKind::Float) =>
        {
            true
        }
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("call_user_func")
                && call_user_func_return_kind(args, module) == Some(ValueKind::Float) =>
        {
            true
        }
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("call_user_func_array")
                && call_user_func_array_return_kind(args, module) == Some(ValueKind::Float) =>
        {
            true
        }
        ExprKind::ClosureCall { var, args }
            if callable_variable_return_kind(module, var, args) == Some(ValueKind::Float) =>
        {
            true
        }
        ExprKind::ExprCall { callee, args }
            if callable_expr_return_kind(module, callee, args) == Some(ValueKind::Float) =>
        {
            true
        }
        ExprKind::FunctionCall { name, .. }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "floor"
                    | "ceil"
                    | "sqrt"
                    | "pi"
                    | "fdiv"
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
            ) =>
        {
            true
        }
        ExprKind::ArrayAccess { array, index } => {
            assoc_static_access_kind(array, index, module) == Some(ValueCellKind::Float)
                || method_call_assoc_static_access_kind(array, index, module)
                    == Some(ValueCellKind::Float)
                || static_property_array_access_kind(array, index, module) == Some(ValueCellKind::Float)
                || nested_array_static_access_kind(array, index, module) == Some(ValueCellKind::Float)
                || dynamic_outer_nested_assoc_static_access_kind(array, index, module)
                    == Some(ValueCellKind::Float)
                || dynamic_parent_nested_assoc_static_access_kind(array, index, module)
                    == Some(ValueCellKind::Float)
                || direct_array_chunk_first_scalar_kind(array, index, module) == Some(ValueCellKind::Float)
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            !expression_is_stringy(then_expr, module)
                && !expression_is_stringy(else_expr, module)
                && (expression_is_floaty(then_expr, module) || expression_is_floaty(else_expr, module))
        }
        ExprKind::ShortTernary { value, default } => {
            !expression_is_stringy(value, module)
                && !expression_is_stringy(default, module)
                && (expression_is_floaty(value, module) || expression_is_floaty(default, module))
        }
        ExprKind::BinaryOp { left, right, .. } => {
            expression_is_floaty(left, module) || expression_is_floaty(right, module)
        }
        _ => false,
    }
}

pub(in crate::codegen::wasm) fn expression_is_stringy(expr: &Expr, module: &WasmModule) -> bool {
    if matches!(
        known_mixed_value_cell_kind(expr, module),
        Ok(Some(ValueCellKind::Str))
    ) {
        return true;
    }
    if object_tostring_supported(expr, module) {
        return true;
    }
    match &expr.kind {
        ExprKind::StringLiteral(_) => true,
        ExprKind::BinaryOp {
            op: BinOp::Concat,
            left,
            right,
        } => string_cast_value_supported(left, module) && string_cast_value_supported(right, module),
        ExprKind::Cast {
            target: CastType::String,
            expr,
        } => string_cast_value_supported(expr, module),
        ExprKind::Variable(name) => module.local_kind(name) == Some(LocalKind::Str),
        ExprKind::PropertyAccess { object, property } => {
            object_property_value_kind(object, property, module) == Some(ValueKind::Str)
        }
        ExprKind::DynamicPropertyAccess { object, property } => {
            object_dynamic_property_value_kind(object, property, module) == Some(ValueKind::Str)
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            object_expr_is_known_non_null(object, module)
                && object_property_value_kind(object, property, module) == Some(ValueKind::Str)
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            object_expr_is_known_non_null(object, module)
                && object_dynamic_property_value_kind(object, property, module) == Some(ValueKind::Str)
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            static_property_value_kind(receiver, property, module) == Some(ValueKind::Str)
        }
        ExprKind::Assignment { target, .. } => match &target.kind {
            ExprKind::PropertyAccess { object, property }
            | ExprKind::NullsafePropertyAccess { object, property } => {
                object_property_value_kind(object, property, module) == Some(ValueKind::Str)
            }
            ExprKind::DynamicPropertyAccess { object, property }
            | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
                object_dynamic_property_value_kind(object, property, module) == Some(ValueKind::Str)
            }
            ExprKind::StaticPropertyAccess { receiver, property } => {
                static_property_value_kind(receiver, property, module) == Some(ValueKind::Str)
            }
            _ => false,
        },
        ExprKind::ClassConstant { receiver } => module.class_name_for_receiver(receiver).is_some(),
        ExprKind::ScopedConstantAccess { receiver, name } => {
            matches!(
                module.class_constant_value(receiver, name),
                Some(ConstantValue::Str(_))
            )
        }
        ExprKind::ConstRef(name) => {
            matches!(module.constant_value(name), Some(ConstantValue::Str(_)))
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_reduce") => {
            array_reduce_call_is_string(args, module)
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("gettype") => true,
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("call_user_func") => {
            call_user_func_return_kind(args, module) == Some(ValueKind::Str)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("call_user_func_array") => {
            call_user_func_array_return_kind(args, module) == Some(ValueKind::Str)
        }
        ExprKind::FunctionCall { name, .. } => {
            module.function_return_kind(name) == Some(ValueKind::Str)
                || is_output_string_builtin(name.as_str())
        }
        ExprKind::MethodCall { object, method, .. } => {
            method_call_return_kind(object, method, module) == Some(ValueKind::Str)
        }
        ExprKind::NullsafeMethodCall { object, method, .. } => {
            object_expr_is_known_non_null(object, module)
                && method_call_return_kind(object, method, module) == Some(ValueKind::Str)
        }
        ExprKind::StaticMethodCall { receiver, method, .. } => {
            static_method_call_return_kind(receiver, method, module) == Some(ValueKind::Str)
        }
        ExprKind::ClosureCall { var, args } => {
            callable_variable_return_kind(module, var, args) == Some(ValueKind::Str)
        }
        ExprKind::ExprCall { callee, args } => {
            callable_expr_return_kind(module, callee, args) == Some(ValueKind::Str)
        }
        ExprKind::Pipe { value, callable } => {
            let synthetic = synthetic_pipe_call_expr(value, callable, expr.span);
            expression_is_stringy(&synthetic, module)
        }
        ExprKind::ArrayAccess { array, index } => {
            expression_is_stringy(array, module)
                || assoc_static_access_kind(array, index, module) == Some(ValueCellKind::Str)
                || method_call_assoc_static_access_kind(array, index, module)
                    == Some(ValueCellKind::Str)
                || static_property_array_access_kind(array, index, module) == Some(ValueCellKind::Str)
                || nested_array_static_access_kind(array, index, module) == Some(ValueCellKind::Str)
                || dynamic_parent_nested_assoc_static_access_kind(array, index, module) == Some(ValueCellKind::Str)
                || direct_array_chunk_first_scalar_kind(array, index, module) == Some(ValueCellKind::Str)
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => expression_is_stringy(then_expr, module) && expression_is_stringy(else_expr, module),
        ExprKind::ShortTernary { value, default } => {
            expression_is_stringy(value, module) && expression_is_stringy(default, module)
        }
        ExprKind::Match { arms, default, .. } => {
            let mut values = arms.iter().map(|(_, value)| value).chain(default.as_deref());
            values
                .clone()
                .next()
                .is_some()
                && values.all(|value| expression_is_stringy(value, module))
        }
        _ => false,
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

pub(in crate::codegen::wasm) fn direct_array_chunk_first_scalar_kind(
    array: &Expr,
    index: &Expr,
    module: &WasmModule,
) -> Option<ValueCellKind> {
    if let Some((_, _, _, _, kind, _, _)) = direct_array_chunk_return_homogeneous_access(array, index, module) {
        return Some(kind);
    }
    if let Some((_, _, _, kind, _, _)) = direct_array_chunk_homogeneous_access(array, index, module) {
        return Some(kind);
    }
    if !is_direct_array_chunk_first_scalar_access(array, index) {
        return None;
    }
    let ExprKind::ArrayAccess { array: chunks, .. } = &array.kind else {
        return None;
    };
    let ExprKind::FunctionCall { args, .. } = &chunks.kind else {
        return None;
    };
    match &args[0].kind {
        ExprKind::ArrayLiteral(items) => items.first().and_then(|item| value_cell_kind_for_expr(item, module)),
        ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
            module
                .array_value_cell_kind(source, 0)
                .or_else(|| (module.array_layout(source) == ArrayLayout::CompactInt).then_some(ValueCellKind::Int))
        }
        ExprKind::FunctionCall { name, .. }
            if module.has_function(name) && module.function_return_kind(name) == Some(ValueKind::Array) =>
        {
            module
                .function_array_return_value_kinds(name)
                .and_then(|kinds| kinds.first())
                .copied()
                .or_else(|| {
                    (module.function_array_return_layout(name) == ArrayLayout::CompactInt)
                        .then_some(ValueCellKind::Int)
                })
        }
        _ => None,
    }
}

pub(in crate::codegen::wasm) fn is_direct_array_chunk_first_scalar_access(array: &Expr, index: &Expr) -> bool {
    if static_or_const_int_value(index) != Some(0) {
        return false;
    }
    let ExprKind::ArrayAccess { array: chunks, index: outer_index } = &array.kind else {
        return false;
    };
    if static_or_const_int_value(outer_index) != Some(0) {
        return false;
    }
    let ExprKind::FunctionCall { name, args } = &chunks.kind else {
        return false;
    };
    name.eq_ignore_ascii_case("array_chunk") && args.len() == 2
}

pub(in crate::codegen::wasm) fn assoc_static_access_kind(
    array: &Expr,
    index: &Expr,
    module: &WasmModule,
) -> Option<ValueCellKind> {
    let ExprKind::Variable(name) = &array.kind else {
        return None;
    };
    if module.local_kind(name) != Some(LocalKind::Array) || module.array_layout(name) != ArrayLayout::Assoc {
        return None;
    }
    if let Some(key) = static_or_const_int_value(index) {
        return assoc_value_kind_for_static_key(name, &AssocKeyValue::Int(key), module)
            .or_else(|| assoc_runtime_scan_value_kind(name, module));
    }
    let key = static_string_value(index, module)?;
    assoc_value_kind_for_static_key(name, &AssocKeyValue::Str(key), module)
        .or_else(|| assoc_runtime_scan_value_kind(name, module))
}

fn method_call_assoc_static_access_kind(
    array: &Expr,
    index: &Expr,
    module: &WasmModule,
) -> Option<ValueCellKind> {
    let metadata = match &array.kind {
        ExprKind::MethodCall { object, method, .. } => {
            method_call_array_return_metadata(object, method, module)?
        }
        ExprKind::NullsafeMethodCall { object, method, .. }
            if object_expr_is_known_non_null(object, module) =>
        {
            method_call_array_return_metadata(object, method, module)?
        }
        ExprKind::StaticMethodCall { receiver, method, .. } => {
            static_method_call_array_return_metadata(receiver, method, module)?
        }
        _ => return None,
    };
    if metadata.layout != ArrayLayout::Assoc {
        return None;
    }
    let key = if let Some(key) = static_or_const_int_value(index) {
        AssocKeyValue::Int(key)
    } else {
        AssocKeyValue::Str(static_string_value(index, module)?)
    };
    metadata
        .key_values
        .as_ref()
        .and_then(|keys| keys.iter().position(|candidate| *candidate == key))
        .and_then(|index| metadata.value_kinds.as_ref()?.get(index).copied())
        .or(metadata.runtime_value_kind)
}

pub(in crate::codegen::wasm) fn static_property_array_access_kind(
    array: &Expr,
    index: &Expr,
    module: &WasmModule,
) -> Option<ValueCellKind> {
    let ExprKind::StaticPropertyAccess { receiver, property } = &array.kind else {
        return None;
    };
    let property_info = module.object_static_property(receiver, property)?;
    if !module.object_member_is_accessible(&property_info.owner_class, &property_info.visibility) {
        return None;
    }
    let index = usize::try_from(static_or_const_int_value(index)?).ok()?;
    let default = property_info.default.as_ref()?;
    let ExprKind::ArrayLiteral(items) = &default.kind else {
        return None;
    };
    value_cell_kinds_for_items(items, module)
        .or_else(|| {
            items
                .iter()
                .all(|item| matches!(item.kind, ExprKind::IntLiteral(_)))
                .then(|| vec![ValueCellKind::Int; items.len()])
        })
        .and_then(|kinds| kinds.get(index).copied())
}

pub(in crate::codegen::wasm) fn expression_is_arrayy(expr: &Expr, module: &WasmModule) -> bool {
    match &expr.kind {
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => true,
        ExprKind::ConstRef(name) => module.array_constant_value(name).is_some(),
        ExprKind::Variable(name) => module.local_kind(name) == Some(LocalKind::Array),
        ExprKind::FunctionCall { name, .. } => {
            module.function_return_kind(name) == Some(ValueKind::Array)
        }
        _ => false,
    }
}

pub(in crate::codegen::wasm) fn expression_has_array_type(expr: &Expr, module: &WasmModule) -> bool {
    match &expr.kind {
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => true,
        ExprKind::ConstRef(name) => module.array_constant_value(name).is_some(),
        ExprKind::Variable(name) => module.local_kind(name) == Some(LocalKind::Array),
        ExprKind::FunctionCall { name, .. } => {
            let builtin_name = name.trim_start_matches('\\').to_ascii_lowercase();
            module.function_return_kind(name) == Some(ValueKind::Array)
                || matches!(
                    builtin_name.as_str(),
                    "range"
                        | "array_values"
                        | "array_reverse"
                        | "array_keys"
                        | "array_unique"
                        | "array_flip"
                        | "array_diff"
                        | "array_intersect"
                        | "array_diff_key"
                        | "array_intersect_key"
                        | "array_merge"
                        | "array_slice"
                        | "array_splice"
                        | "array_chunk"
                        | "array_pad"
                        | "array_fill"
                        | "array_fill_keys"
                        | "array_combine"
                        | "array_column"
                        | "array_filter"
                        | "array_map"
                        | "explode"
                        | "str_split"
                        | "pathinfo"
                )
        }
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } if method.eq_ignore_ascii_case("cases") && args.is_empty() => module
            .class_name_for_receiver(receiver)
            .and_then(|class_name| module.enum_case_names(&class_name))
            .is_some(),
        ExprKind::StaticMethodCall { receiver, method, .. } => {
            static_method_call_array_return_metadata(receiver, method, module).is_some()
        }
        ExprKind::MethodCall { object, method, .. } => {
            method_call_array_return_metadata(object, method, module).is_some()
        }
        ExprKind::NullsafeMethodCall { object, method, .. } => {
            object_expr_is_known_non_null(object, module)
                && method_call_array_return_metadata(object, method, module).is_some()
        }
        _ => false,
    }
}
