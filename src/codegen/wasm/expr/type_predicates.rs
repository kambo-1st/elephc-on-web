//! Purpose:
//! Lowers PHP scalar type-predicate builtins for wasm32-web.
//! Keeps is_int/is_float/is_bool/is_null/is_string/is_array/is_iterable handling out of the main expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` builtin dispatch.
//!
//! Key details:
//! - Mixed/value-cell predicates inspect runtime tags; statically known values are classified without materializing extra runtime state.

use super::*;

pub(super) fn emit_type_predicate_call(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let [arg] = args else {
        return Err(CompileError::new(
            call.span,
            &format!("wasm32-web {}() expects exactly one argument", name),
        ));
    };
    if matches!(arg.kind, ExprKind::ArrayAccess { .. }) {
        if let Some(cell) = materialize_mixed_value_cell(arg, module)? {
            emit_mixed_type_predicate(&cell, name, module);
            return Ok(ValueKind::Bool);
        }
    }
    if expression_is_arrayy(arg, module) || expression_has_array_type(arg, module) {
        let matches = matches!(name.to_ascii_lowercase().as_str(), "is_array" | "is_iterable");
        module.body().line(&format!("i32.const {}", i32::from(matches)));
        return Ok(ValueKind::Bool);
    }
    if let ExprKind::Variable(var) = &arg.kind {
        if module.local_kind(var) == Some(LocalKind::Mixed) {
            emit_mixed_type_predicate(var, name, module);
            return Ok(ValueKind::Bool);
        }
    }
    if let Some(cell) = materialize_mixed_value_cell(arg, module)? {
        emit_mixed_type_predicate(&cell, name, module);
        return Ok(ValueKind::Bool);
    }
    if let ExprKind::FunctionCall { name: function_name, args } = &arg.kind {
        if module.has_function(function_name)
            && module.function_return_kind(function_name) == Some(ValueKind::Mixed)
        {
            let local = module
                .next_label("mixed_predicate_value")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(local.clone());
            emit_user_function_args(arg, function_name, args, module)?;
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(function_name)));
            module.body().line(&format!("local.set ${}", local));
            emit_mixed_type_predicate(&local, name, module);
            return Ok(ValueKind::Bool);
        }
    }
    if matches!(name.to_ascii_lowercase().as_str(), "is_null" | "is_object")
        && emit_dynamic_backed_enum_try_from_pointer(arg, module)?
    {
        module.body().line("i32.eqz");
        if name.eq_ignore_ascii_case("is_object") {
            module.body().line("i32.eqz");
        }
        return Ok(ValueKind::Bool);
    }
    let kind = classify_predicate_arg(arg, module)?;
    let matches = match name.to_ascii_lowercase().as_str() {
        "is_int" => kind == ValueKind::Int,
        "is_float" => kind == ValueKind::Float,
        "is_bool" => kind == ValueKind::Bool,
        "is_null" => kind == ValueKind::Null,
        "is_string" => kind == ValueKind::Str,
        "is_array" => false,
        "is_iterable" => false,
        "is_object" => kind == ValueKind::Object,
        _ => unreachable!(),
    };
    module.body().line(&format!("i32.const {}", i32::from(matches)));
    Ok(ValueKind::Bool)
}

fn emit_mixed_type_predicate(var: &str, name: &str, module: &mut WasmModule) {
    let tag = match name.to_ascii_lowercase().as_str() {
        "is_int" => WASM_VALUE_TAG_INT,
        "is_float" => WASM_VALUE_TAG_FLOAT,
        "is_bool" => WASM_VALUE_TAG_BOOL,
        "is_null" => WASM_VALUE_TAG_NULL,
        "is_string" => WASM_VALUE_TAG_STRING,
        "is_array" => WASM_VALUE_TAG_ARRAY,
        "is_iterable" => WASM_VALUE_TAG_ARRAY,
        "is_object" => WASM_VALUE_TAG_OBJECT,
        _ => unreachable!(),
    };
    module.body().line(&format!("local.get ${}", var));
    module.body().line(&format!("i32.const {}", tag));
    module.body().line("call $__rt_mixed_tag_equals");
}

fn classify_predicate_arg(
    arg: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if expression_is_stringy(arg, module) {
        return Ok(ValueKind::Str);
    }

    match &arg.kind {
        ExprKind::StringLiteral(_) => Ok(ValueKind::Str),
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Str) => {
            Ok(ValueKind::Str)
        }
        ExprKind::ConstRef(name) => match module.constant_value(name) {
            Some(ConstantValue::Int(_)) => Ok(ValueKind::Int),
            Some(ConstantValue::Float(_)) => Ok(ValueKind::Float),
            Some(ConstantValue::Bool(_)) => Ok(ValueKind::Bool),
            Some(ConstantValue::Str(_)) => Ok(ValueKind::Str),
            Some(ConstantValue::Null) => Ok(ValueKind::Null),
            None if const_int_value(name).is_some() => Ok(ValueKind::Int),
            None => {
                let kind = emit_expr(arg, module)?;
                emit_drop_value_kind(kind, module);
                Ok(kind)
            }
        },
        ExprKind::ClassConstant { receiver } => {
            if module.class_name_for_receiver(receiver).is_some() {
                Ok(ValueKind::Str)
            } else {
                Err(CompileError::new(
                    arg.span,
                    "wasm32-web ::class currently requires a named class receiver",
                ))
            }
        }
        ExprKind::ScopedConstantAccess { receiver, name } => {
            if module.enum_case_class(receiver, name).is_some() {
                return Ok(ValueKind::Object);
            }
            match module.class_constant_value(receiver, name) {
                Some(ConstantValue::Int(_)) => Ok(ValueKind::Int),
                Some(ConstantValue::Float(_)) => Ok(ValueKind::Float),
                Some(ConstantValue::Bool(_)) => Ok(ValueKind::Bool),
                Some(ConstantValue::Str(_)) => Ok(ValueKind::Str),
                Some(ConstantValue::Null) => Ok(ValueKind::Null),
                None => Err(CompileError::new(
                    arg.span,
                    "wasm32-web class constant is not supported yet",
                )),
            }
        }
        _ => {
            let kind = emit_expr(arg, module)?;
            emit_drop_value_kind(kind, module);
            Ok(kind)
        }
    }
}

fn emit_drop_value_kind(kind: ValueKind, module: &mut WasmModule) {
    match kind {
        ValueKind::Str | ValueKind::Array => {
            module.body().line("drop");
            module.body().line("drop");
        }
        ValueKind::Mixed
        | ValueKind::Object
        | ValueKind::Callable
        | ValueKind::Int
        | ValueKind::Float
        | ValueKind::Bool
        | ValueKind::Null => {
            module.body().line("drop");
        }
        ValueKind::Never => {}
    }
}
