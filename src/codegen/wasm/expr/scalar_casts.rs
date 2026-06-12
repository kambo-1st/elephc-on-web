//! Purpose:
//! Lowers scalar casts and scalar cast-like builtins for wasm32-web.
//! Keeps intval/floatval/boolval/empty and `(int)`/`(float)`/`(bool)` handling out of the main expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` expression and builtin dispatch.
//!
//! Key details:
//! - Mixed string casts use PHP leading-numeric scanning helpers; unsupported array/mixed cases fail explicitly.

use super::*;

pub(super) fn emit_cast_expr(
    expr: &Expr,
    target: &CastType,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    match target {
        CastType::Int => emit_int_cast(expr, module),
        CastType::Float => emit_float_cast(expr, module),
        CastType::Bool => {
            emit_condition(expr, module)?;
            Ok(ValueKind::Bool)
        }
        CastType::String => {
            emit_string_cast_value_to_stack(expr, expr, module)?;
            Ok(ValueKind::Str)
        }
        CastType::Array => Err(CompileError::new(
            expr.span,
            "wasm32-web array casts are not supported yet",
        )),
    }
}

fn emit_int_cast(expr: &Expr, module: &mut WasmModule) -> Result<ValueKind, CompileError> {
    if let Some(kind) = emit_known_mixed_int_cast(expr, module)? {
        return Ok(kind);
    }
    if known_mixed_value_cell_kind(expr, module)? == Some(ValueCellKind::Str) {
        let Some(cell) = materialize_mixed_value_cell(expr, module)? else {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web integer cast could not materialize mixed string value",
            ));
        };
        let number = module.next_label("mixed_expr_string_int_cast_number");
        module.declare_f64_local(number.trim_start_matches('$').to_string());
        emit_mixed_string_leading_numeric_value(&cell, &number, module);
        module.body().line(&format!("local.get {}", number));
        module.body().line("i64.trunc_f64_s");
        return Ok(ValueKind::Int);
    }
    if known_mixed_numeric_kind(expr, module)?.is_some() {
        emit_known_mixed_numeric_operand(expr, false, module)?;
        return Ok(ValueKind::Int);
    }
    if let Some(value) = static_string_value(expr, module) {
        module
            .body()
            .line(&format!("i64.const {}", parse_php_string_cast_int(&value)));
        return Ok(ValueKind::Int);
    }
    if let Some(var) = runtime_string_arg_or_materialize(expr, "int_cast_string", module)? {
        let number = module.next_label("int_cast_string_number");
        module.declare_f64_local(number.trim_start_matches('$').to_string());
        emit_runtime_leading_numeric_string_value(&var, &number, module);
        module.body().line(&format!("local.get {}", number));
        module.body().line("i64.trunc_f64_s");
        return Ok(ValueKind::Int);
    }
    match emit_expr(expr, module)? {
        ValueKind::Int => {}
        ValueKind::Float => module.body().line("i64.trunc_f64_s"),
        ValueKind::Bool => module.body().line("i64.extend_i32_s"),
        ValueKind::Null => {
            module.body().line("drop");
            module.body().line("i64.const 0");
        }
        ValueKind::Str => {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web integer casts do not support string inputs yet",
            ));
        }
        ValueKind::Array => {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web integer casts do not support array inputs yet",
            ));
        }
        ValueKind::Object => {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web integer casts do not support object inputs yet",
            ));
        }
        ValueKind::Mixed => {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web integer casts do not support mixed inputs yet",
            ));
        }
        ValueKind::Never => module.body().line("unreachable"),
    }
    Ok(ValueKind::Int)
}

fn emit_float_cast(expr: &Expr, module: &mut WasmModule) -> Result<ValueKind, CompileError> {
    if let Some(kind) = emit_known_mixed_float_cast(expr, module)? {
        return Ok(kind);
    }
    if known_mixed_value_cell_kind(expr, module)? == Some(ValueCellKind::Str) {
        let Some(cell) = materialize_mixed_value_cell(expr, module)? else {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web float cast could not materialize mixed string value",
            ));
        };
        let number = module.next_label("mixed_expr_string_float_cast_number");
        module.declare_f64_local(number.trim_start_matches('$').to_string());
        emit_mixed_string_leading_numeric_value(&cell, &number, module);
        module.body().line(&format!("local.get {}", number));
        return Ok(ValueKind::Float);
    }
    if known_mixed_numeric_kind(expr, module)?.is_some() {
        emit_known_mixed_numeric_operand(expr, true, module)?;
        return Ok(ValueKind::Float);
    }
    if let Some(value) = static_string_value(expr, module) {
        module
            .body()
            .line(&format!("f64.const {}", parse_php_string_cast_float(&value)));
        return Ok(ValueKind::Float);
    }
    if let Some(var) = runtime_string_arg_or_materialize(expr, "float_cast_string", module)? {
        let number = module.next_label("float_cast_string_number");
        module.declare_f64_local(number.trim_start_matches('$').to_string());
        emit_runtime_leading_numeric_string_value(&var, &number, module);
        module.body().line(&format!("local.get {}", number));
        return Ok(ValueKind::Float);
    }
    match emit_expr(expr, module)? {
        ValueKind::Float => {}
        ValueKind::Int => module.body().line("f64.convert_i64_s"),
        ValueKind::Bool => module.body().line("f64.convert_i32_s"),
        ValueKind::Null => {
            module.body().line("drop");
            module.body().line("f64.const 0");
        }
        ValueKind::Str => {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web float casts do not support string inputs yet",
            ));
        }
        ValueKind::Array => {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web float casts do not support array inputs yet",
            ));
        }
        ValueKind::Object => {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web float casts do not support object inputs yet",
            ));
        }
        ValueKind::Mixed => {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web float casts do not support mixed inputs yet",
            ));
        }
        ValueKind::Never => module.body().line("unreachable"),
    }
    Ok(ValueKind::Float)
}

fn emit_known_mixed_int_cast(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    let ExprKind::Variable(name) = &expr.kind else {
        return Ok(None);
    };
    if module.local_kind(name) != Some(LocalKind::Mixed) {
        return Ok(None);
    }
    match module.mixed_value_cell_kind(name) {
        Some(ValueCellKind::Int) | Some(ValueCellKind::Bool) => {
            emit_mixed_i64_payload(name, module);
            Ok(Some(ValueKind::Int))
        }
        Some(ValueCellKind::Float) => {
            emit_mixed_f64_payload(name, module);
            module.body().line("i64.trunc_f64_s");
            Ok(Some(ValueKind::Int))
        }
        Some(ValueCellKind::Null) => {
            module.body().line("i64.const 0");
            Ok(Some(ValueKind::Int))
        }
        Some(ValueCellKind::Str) => {
            let number = module.next_label("mixed_string_int_cast_number");
            module.declare_f64_local(number.trim_start_matches('$').to_string());
            emit_mixed_string_leading_numeric_value(name, &number, module);
            module.body().line(&format!("local.get {}", number));
            module.body().line("i64.trunc_f64_s");
            Ok(Some(ValueKind::Int))
        }
        Some(ValueCellKind::Array) => Err(CompileError::new(
            expr.span,
            "wasm32-web integer casts do not support array mixed values yet",
        )),
        None => Ok(None),
    }
}

fn emit_known_mixed_float_cast(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    let ExprKind::Variable(name) = &expr.kind else {
        return Ok(None);
    };
    if module.local_kind(name) != Some(LocalKind::Mixed) {
        return Ok(None);
    }
    match module.mixed_value_cell_kind(name) {
        Some(ValueCellKind::Float) => {
            emit_mixed_f64_payload(name, module);
            Ok(Some(ValueKind::Float))
        }
        Some(ValueCellKind::Int) | Some(ValueCellKind::Bool) => {
            emit_mixed_i64_payload(name, module);
            module.body().line("f64.convert_i64_s");
            Ok(Some(ValueKind::Float))
        }
        Some(ValueCellKind::Null) => {
            module.body().line("f64.const 0");
            Ok(Some(ValueKind::Float))
        }
        Some(ValueCellKind::Str) => {
            let number = module.next_label("mixed_string_float_cast_number");
            module.declare_f64_local(number.trim_start_matches('$').to_string());
            emit_mixed_string_leading_numeric_value(name, &number, module);
            module.body().line(&format!("local.get {}", number));
            Ok(Some(ValueKind::Float))
        }
        Some(ValueCellKind::Array) => Err(CompileError::new(
            expr.span,
            "wasm32-web float casts do not support array mixed values yet",
        )),
        None => Ok(None),
    }
}
pub(super) fn emit_intval_call(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let [arg] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web intval() expects exactly one argument",
        ));
    };
    emit_int_cast(arg, module)
}

pub(super) fn emit_floatval_call(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let [arg] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web floatval() expects exactly one argument",
        ));
    };
    emit_float_cast(arg, module)
}

pub(super) fn emit_boolval_call(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let [arg] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web boolval() expects exactly one argument",
        ));
    };
    emit_condition(arg, module)?;
    Ok(ValueKind::Bool)
}

pub(super) fn emit_empty_call(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let [arg] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web empty() expects exactly one argument",
        ));
    };
    match &arg.kind {
        ExprKind::DynamicPropertyAccess { object, property } => {
            emit_dynamic_object_property_empty_expr(arg, object, property, module)?;
            return Ok(ValueKind::Bool);
        }
        ExprKind::NullsafePropertyAccess { object, .. }
        | ExprKind::NullsafeDynamicPropertyAccess { object, .. }
            if matches!(object.kind, ExprKind::Null) =>
        {
            module.body().line("i32.const 1");
            return Ok(ValueKind::Bool);
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property }
            if object_expr_is_known_non_null(object, module) =>
        {
            emit_dynamic_object_property_empty_expr(arg, object, property, module)?;
            return Ok(ValueKind::Bool);
        }
        ExprKind::ArrayAccess { array, index } => {
            if let Some(truthy) = static_array_offset_truthiness(array, index, module) {
                module.body().line(&format!("i32.const {}", i32::from(!truthy)));
                return Ok(ValueKind::Bool);
            }
        }
        _ => {}
    }
    emit_condition(arg, module)?;
    module.body().line("i32.eqz");
    Ok(ValueKind::Bool)
}

fn static_array_offset_truthiness(
    array: &Expr,
    index: &Expr,
    module: &WasmModule,
) -> Option<bool> {
    match &array.kind {
        ExprKind::ConstRef(name) => match module.array_constant_value(name)? {
            ConstantArrayValue::Indexed(items) => {
                let offset = usize::try_from(static_or_const_or_i64_local_value(index, module)?).ok()?;
                Some(
                    items
                        .get(offset)
                        .and_then(static_value_cell_truthiness_for_filter)
                        .unwrap_or(false),
                )
            }
            ConstantArrayValue::Assoc(items) => {
                let key = static_empty_assoc_access_key(index, module)?;
                Some(
                    items
                        .iter()
                        .rev()
                        .find_map(|(candidate, value)| {
                            let candidate = static_empty_assoc_access_key(candidate, module)?;
                            (candidate == key).then(|| static_value_cell_truthiness_for_filter(value))
                        })
                        .flatten()
                        .unwrap_or(false),
                )
            }
        },
        ExprKind::ArrayLiteral(items) => {
            let offset = usize::try_from(static_or_const_or_i64_local_value(index, module)?).ok()?;
            Some(
                items
                    .get(offset)
                    .and_then(static_value_cell_truthiness_for_filter)
                    .unwrap_or(false),
            )
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            let key = static_empty_assoc_access_key(index, module)?;
            Some(
                items
                    .iter()
                    .rev()
                    .find_map(|(candidate, value)| {
                        let candidate = static_empty_assoc_access_key(candidate, module)?;
                        (candidate == key).then(|| static_value_cell_truthiness_for_filter(value))
                    })
                    .flatten()
                    .unwrap_or(false),
            )
        }
        _ => None,
    }
}

fn static_empty_assoc_access_key(index: &Expr, module: &WasmModule) -> Option<AssocKeyValue> {
    static_assoc_access_key(index, module).or_else(|| match &index.kind {
        ExprKind::Variable(name) => module.string_static_value(name).map(AssocKeyValue::Str),
        _ => None,
    })
}
