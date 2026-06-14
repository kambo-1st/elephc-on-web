//! Purpose:
//! Lowers wasm32-web scalar, numeric-string, mixed-cell, and comparison operations.
//! Keeps operator semantics separate from the top-level expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::emit_expr()` for binary/null-coalesce lowering.
//! - Sibling wasm expression modules that need mixed value-cell materialization/coercion.
//!
//! Key details:
//! - Preserves PHP-compatible scalar comparisons and boxed Mixed/value-cell runtime contracts.

use super::*;
use crate::codegen::wasm::expr::array_value_cells::emit_store_emitted_value_kind;
use super::scalar_mixed_numeric::*;
use super::scalar_mixed_comparisons::emit_known_mixed_numeric_comparison;

pub(in crate::codegen::wasm) fn emit_binary(
    expr: &Expr,
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if let Some(result) = emit_static_scalar_comparison(left, op, right, module) {
        module.body().line(&format!("i32.const {}", i32::from(result)));
        return Ok(ValueKind::Bool);
    }

    match op {
        BinOp::And => {
            emit_condition(left, module)?;
            module.body().open("if (result i32)");
            emit_condition(right, module)?;
            module.body().line("else");
            module.body().line("i32.const 0");
            module.body().close("end");
            return Ok(ValueKind::Bool);
        }
        BinOp::Or => {
            emit_condition(left, module)?;
            module.body().open("if (result i32)");
            module.body().line("i32.const 1");
            module.body().line("else");
            emit_condition(right, module)?;
            module.body().close("end");
            return Ok(ValueKind::Bool);
        }
        BinOp::Xor => {
            emit_condition(left, module)?;
            emit_condition(right, module)?;
            module.body().line("i32.ne");
            return Ok(ValueKind::Bool);
        }
        _ => {}
    }

    if expression_is_arrayy(left, module) || expression_is_arrayy(right, module) {
        return emit_indexed_array_equality(expr, left, op, right, module);
    }

    if matches!(op, BinOp::StrictEq | BinOp::StrictNotEq) {
        if emit_enum_case_array_identity_comparison(left, right, op, module)? {
            return Ok(ValueKind::Bool);
        }
        let left_object = object_class_name_for_expr(left, module).is_some();
        let right_object = object_class_name_for_expr(right, module).is_some();
        if right_object && emit_mixed_object_identity_comparison(left, right, op, module)? {
            return Ok(ValueKind::Bool);
        }
        if left_object && emit_mixed_object_identity_comparison(right, left, op, module)? {
            return Ok(ValueKind::Bool);
        }
        if left_object && right_object {
            emit_expr(left, module)?;
            emit_expr(right, module)?;
            module
                .body()
                .line(if matches!(op, BinOp::StrictEq) { "i32.eq" } else { "i32.ne" });
            return Ok(ValueKind::Bool);
        }
        if left_object || right_object {
            let left_kind = emit_expr(left, module)?;
            emit_drop_value_kind(left_kind, module);
            let right_kind = emit_expr(right, module)?;
            emit_drop_value_kind(right_kind, module);
            module.body().line(if matches!(op, BinOp::StrictEq) {
                "i32.const 0"
            } else {
                "i32.const 1"
            });
            return Ok(ValueKind::Bool);
        }
        if emit_mixed_literal_comparison(left, right, op, module)? {
            return Ok(ValueKind::Bool);
        }
        if emit_mixed_literal_comparison(right, left, op, module)? {
            return Ok(ValueKind::Bool);
        }
        if let Some(search) = array_search_false_comparison(left, right) {
            if emit_array_search_false_comparison_bool(
                search,
                matches!(op, BinOp::StrictNotEq),
                module,
            )? {
                return Ok(ValueKind::Bool);
            }
            emit_array_search_index(search, module)?;
            module.body().line("i64.const 0");
            module.body().line("i64.lt_s");
            if matches!(op, BinOp::StrictNotEq) {
                module.body().line("i32.eqz");
            }
            return Ok(ValueKind::Bool);
        }
        if let Some(search) = array_search_false_comparison(right, left) {
            if emit_array_search_false_comparison_bool(
                search,
                matches!(op, BinOp::StrictNotEq),
                module,
            )? {
                return Ok(ValueKind::Bool);
            }
            emit_array_search_index(search, module)?;
            module.body().line("i64.const 0");
            module.body().line("i64.lt_s");
            if matches!(op, BinOp::StrictNotEq) {
                module.body().line("i32.eqz");
            }
            return Ok(ValueKind::Bool);
        }
    }

    if matches!(
        op,
        BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq
    ) && (expression_is_booly(left, module) || expression_is_booly(right, module))
    {
        return emit_bool_comparison(left, op, right, module);
    }

    if matches!(
        op,
        BinOp::Eq
            | BinOp::NotEq
            | BinOp::Lt
            | BinOp::Gt
            | BinOp::LtEq
            | BinOp::GtEq
            | BinOp::StrictEq
            | BinOp::StrictNotEq
    ) && (expression_is_stringy(left, module) || expression_is_stringy(right, module))
    {
        if let Some(err) = unsupported_string_coercion_expr(left) {
            return Err(err);
        }
        if let Some(err) = unsupported_string_coercion_expr(right) {
            return Err(err);
        }
        if let Some(err) = unsupported_object_string_coercion_expr(left, module) {
            return Err(err);
        }
        if let Some(err) = unsupported_object_string_coercion_expr(right, module) {
            return Err(err);
        }
    }

    if matches!(
        op,
        BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq
    ) && (expression_is_stringy(left, module) || expression_is_stringy(right, module))
        && string_cast_value_supported(left, module)
        && string_cast_value_supported(right, module)
    {
        return emit_runtime_php_string_comparison(expr, left, op, right, module);
    }

    if matches!(
        op,
        BinOp::Eq | BinOp::NotEq | BinOp::StrictEq | BinOp::StrictNotEq
    ) && (expression_is_stringy(left, module) || expression_is_stringy(right, module))
    {
        return emit_string_equality(expr, left, op, right, module);
    }

    match op {
        BinOp::Div => {
            if let Some(kind) = emit_known_mixed_numeric_binary(expr, left, op, right, module)? {
                return Ok(kind);
            }
            if let Some(kind) = emit_static_string_numeric_binary(expr, left, op, right, module)? {
                return Ok(kind);
            }
            if let Some(kind) = emit_runtime_string_numeric_binary(expr, left, op, right, module)? {
                return Ok(kind);
            }
            require_float(left, module)?;
            require_float(right, module)?;
            module.body().line("f64.div");
            Ok(ValueKind::Float)
        }
        BinOp::Pow => {
            if let Some(kind) = emit_known_mixed_numeric_binary(expr, left, op, right, module)? {
                return Ok(kind);
            }
            if let Some(kind) = emit_static_string_numeric_binary(expr, left, op, right, module)? {
                return Ok(kind);
            }
            if let Some(kind) = emit_runtime_string_numeric_binary(expr, left, op, right, module)? {
                return Ok(kind);
            }
            require_float(left, module)?;
            require_float(right, module)?;
            module.body().line("call $host_pow");
            Ok(ValueKind::Float)
        }
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Mod => {
            if matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Mod) {
                if let Some(kind) = emit_known_mixed_numeric_binary(expr, left, op, right, module)? {
                    return Ok(kind);
                }
            }
            if let Some(kind) = emit_static_string_numeric_binary(expr, left, op, right, module)? {
                return Ok(kind);
            }
            if let Some(kind) = emit_runtime_string_numeric_binary(expr, left, op, right, module)? {
                return Ok(kind);
            }
            if expression_is_floaty(left, module) || expression_is_floaty(right, module) {
                require_float(left, module)?;
                require_float(right, module)?;
                let instr = match op {
                    BinOp::Add => "f64.add",
                    BinOp::Sub => "f64.sub",
                    BinOp::Mul => "f64.mul",
                    BinOp::Mod => {
                        return Err(CompileError::new(
                            expr.span,
                            "wasm32-web float modulo is not supported yet",
                        ));
                    }
                    _ => unreachable!(),
                };
                module.body().line(instr);
                return Ok(ValueKind::Float);
            }
            require_int(left, module)?;
            require_int(right, module)?;
            let instr = match op {
                BinOp::Add => "i64.add",
                BinOp::Sub => "i64.sub",
                BinOp::Mul => "i64.mul",
                BinOp::Mod => "i64.rem_s",
                _ => unreachable!(),
            };
            module.body().line(instr);
            Ok(ValueKind::Int)
        }
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight => {
            if matches!(
                op,
                BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight
            ) {
                if let Some(kind) = emit_known_mixed_numeric_binary(expr, left, op, right, module)? {
                    return Ok(kind);
                }
            }
            if let Some(kind) = emit_static_string_numeric_binary(expr, left, op, right, module)? {
                return Ok(kind);
            }
            if let Some(kind) = emit_runtime_string_numeric_binary(expr, left, op, right, module)? {
                return Ok(kind);
            }
            require_int(left, module)?;
            require_int(right, module)?;
            let instr = match op {
                BinOp::BitAnd => "i64.and",
                BinOp::BitOr => "i64.or",
                BinOp::BitXor => "i64.xor",
                BinOp::ShiftLeft => "i64.shl",
                BinOp::ShiftRight => "i64.shr_s",
                _ => unreachable!(),
            };
            module.body().line(instr);
            Ok(ValueKind::Int)
        }
        BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq | BinOp::Eq | BinOp::NotEq => {
            if emit_known_mixed_numeric_comparison(left, op, right, module)? {
                return Ok(ValueKind::Bool);
            }
            if expression_is_floaty(left, module) || expression_is_floaty(right, module) {
                require_float(left, module)?;
                require_float(right, module)?;
                let instr = match op {
                    BinOp::Lt => "f64.lt",
                    BinOp::Gt => "f64.gt",
                    BinOp::LtEq => "f64.le",
                    BinOp::GtEq => "f64.ge",
                    BinOp::Eq => "f64.eq",
                    BinOp::NotEq => "f64.ne",
                    _ => unreachable!(),
                };
                module.body().line(instr);
                return Ok(ValueKind::Bool);
            }
            require_int(left, module)?;
            require_int(right, module)?;
            let instr = match op {
                BinOp::Lt => "i64.lt_s",
                BinOp::Gt => "i64.gt_s",
                BinOp::LtEq => "i64.le_s",
                BinOp::GtEq => "i64.ge_s",
                BinOp::Eq => "i64.eq",
                BinOp::NotEq => "i64.ne",
                _ => unreachable!(),
            };
            module.body().line(instr);
            Ok(ValueKind::Bool)
        }
        BinOp::StrictEq | BinOp::StrictNotEq => {
            let left_float = expression_is_floaty(left, module);
            let right_float = expression_is_floaty(right, module);
            let left_bool = expression_is_booly(left, module);
            let right_bool = expression_is_booly(right, module);
            if left_float && right_float {
                require_float(left, module)?;
                require_float(right, module)?;
                module
                    .body()
                    .line(if matches!(op, BinOp::StrictEq) { "f64.eq" } else { "f64.ne" });
            } else if left_bool && right_bool {
                emit_condition(left, module)?;
                emit_condition(right, module)?;
                module
                    .body()
                    .line(if matches!(op, BinOp::StrictEq) { "i32.eq" } else { "i32.ne" });
            } else if left_float || right_float || left_bool || right_bool {
                emit_expr(left, module)?;
                module.body().line("drop");
                emit_expr(right, module)?;
                module.body().line("drop");
                module.body().line(if matches!(op, BinOp::StrictEq) {
                    "i32.const 0"
                } else {
                    "i32.const 1"
                });
            } else {
                require_int(left, module)?;
                require_int(right, module)?;
                module
                    .body()
                    .line(if matches!(op, BinOp::StrictEq) { "i64.eq" } else { "i64.ne" });
            }
            Ok(ValueKind::Bool)
        }
        _ => Err(CompileError::new(
            expr.span,
            "wasm32-web runtime WAT output does not support this operator yet",
        )),
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

fn emit_mixed_object_identity_comparison(
    mixed_expr: &Expr,
    object_expr: &Expr,
    op: &BinOp,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let Some(cell) = materialize_mixed_value_cell(mixed_expr, module)? else {
        return Ok(false);
    };
    let object_local = module
        .next_label("mixed_object_identity_ptr")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(object_local.clone());
    let kind = emit_expr(object_expr, module)?;
    if kind != ValueKind::Object {
        return Err(CompileError::new(
            object_expr.span,
            "wasm32-web strict object identity expected an object expression",
        ));
    }
    module.body().line(&format!("local.set ${}", object_local));
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_OBJECT));
    module.body().line("i32.eq");
    module.body().open("if (result i32)");
    emit_mixed_i32_payload(&cell, 8, module);
    module.body().line(&format!("local.get ${}", object_local));
    module
        .body()
        .line(if matches!(op, BinOp::StrictEq) { "i32.eq" } else { "i32.ne" });
    module.body().line("else");
    module.body().line(if matches!(op, BinOp::StrictEq) {
        "i32.const 0"
    } else {
        "i32.const 1"
    });
    module.body().close("end");
    Ok(true)
}

pub(in crate::codegen::wasm) fn emit_mixed_literal_comparison(
    candidate: &Expr,
    literal: &Expr,
    op: &BinOp,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let Some(name) = materialize_mixed_value_cell(candidate, module)? else {
        return Ok(false);
    };
    match &literal.kind {
        ExprKind::BoolLiteral(value) => {
            emit_mixed_tag_payload_comparison(
                &name,
                WASM_VALUE_TAG_BOOL,
                i64::from(*value),
                op,
                module,
            );
            Ok(true)
        }
        ExprKind::Null => {
            emit_mixed_tag_payload_comparison(&name, WASM_VALUE_TAG_NULL, 0, op, module);
            Ok(true)
        }
        ExprKind::StringLiteral(_) => {
            let value = static_string_value(literal, module).expect("string literal is static");
            emit_mixed_string_comparison(&name, &value, op, module);
            Ok(true)
        }
        ExprKind::FloatLiteral(value) => {
            emit_mixed_float_comparison(&name, *value, op, module);
            Ok(true)
        }
        _ => {
            let Some(value) = static_or_const_int_value(literal) else {
                return Ok(false);
            };
            emit_mixed_tag_payload_comparison(&name, WASM_VALUE_TAG_INT, value, op, module);
            Ok(true)
        }
    }
}

pub(in crate::codegen::wasm) fn materialize_mixed_value_cell(
    candidate: &Expr,
    module: &mut WasmModule,
) -> Result<Option<String>, CompileError> {
    match &candidate.kind {
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Mixed) => {
            Ok(Some(name.clone()))
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_search") => {
            let local = module
                .next_label("mixed_array_search_value")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(local.clone());
            emit_mixed_array_search_assign(&local, candidate, args, module)?;
            Ok(Some(local))
        }
        ExprKind::FunctionCall { name, args }
            if matches!(name.to_ascii_lowercase().as_str(), "array_pop" | "array_shift") =>
        {
            let local = module
                .next_label("mixed_array_pop_shift_value")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(local.clone());
            emit_mixed_value_array_pop_shift_assign(&local, candidate, name, args, module)?;
            Ok(Some(local))
        }
        ExprKind::FunctionCall { name, args }
            if module.has_function(name) && module.function_return_kind(name) == Some(ValueKind::Mixed) =>
        {
            let local = module
                .next_label("mixed_compare_value")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(local.clone());
            emit_user_function_args(candidate, name, args, module)?;
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(name)));
            module.body().line(&format!("local.set ${}", local));
            Ok(Some(local))
        }
        ExprKind::MethodCall { object, method, args }
            if method_call_return_kind(object, method, module) == Some(ValueKind::Mixed) =>
        {
            let local = module
                .next_label("mixed_method_value")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(local.clone());
            match emit_method_call_expr(candidate, object, method, args, module)? {
                ValueKind::Mixed => {
                    module.body().line(&format!("local.set ${}", local));
                    Ok(Some(local))
                }
                _ => unreachable!("mixed method metadata must return a mixed value"),
            }
        }
        ExprKind::DynamicMethodCall { object, method, args }
            if dynamic_method_call_return_kind(object, method, module) == Some(ValueKind::Mixed) =>
        {
            let local = module
                .next_label("mixed_dynamic_method_value")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(local.clone());
            match emit_dynamic_method_call_expr(candidate, object, method, args, module)? {
                ValueKind::Mixed => {
                    module.body().line(&format!("local.set ${}", local));
                    Ok(Some(local))
                }
                _ => unreachable!("mixed dynamic method metadata must return a mixed value"),
            }
        }
        ExprKind::StaticMethodCall { receiver, method, args }
            if static_method_call_return_kind(receiver, method, module) == Some(ValueKind::Mixed) =>
        {
            let local = module
                .next_label("mixed_static_method_value")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(local.clone());
            match emit_static_method_call_expr(candidate, receiver, method, args, module)? {
                ValueKind::Mixed => {
                    module.body().line(&format!("local.set ${}", local));
                    Ok(Some(local))
                }
                _ => unreachable!("mixed static method metadata must return a mixed value"),
            }
        }
        ExprKind::DynamicStaticMethodCall { receiver, method, args }
            if dynamic_static_method_call_return_kind(receiver, method, module)
                == Some(ValueKind::Mixed) =>
        {
            let local = module
                .next_label("mixed_dynamic_static_method_value")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(local.clone());
            match emit_dynamic_static_method_call_expr(candidate, receiver, method, args, module)? {
                ValueKind::Mixed => {
                    module.body().line(&format!("local.set ${}", local));
                    Ok(Some(local))
                }
                _ => unreachable!("mixed dynamic static method metadata must return a mixed value"),
            }
        }
        ExprKind::NullsafePropertyAccess { .. } => {
            let local = module
                .next_label("mixed_nullsafe_property_value")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(local.clone());
            emit_alloc_mixed_cell(&local, module);
            let emitted = emit_expr(candidate, module)?;
            emit_store_emitted_value_kind(
                &format!("${}", local),
                emitted,
                "mixed_nullsafe_property_value",
                module,
            )?;
            Ok(Some(local))
        }
        ExprKind::NullsafeMethodCall { .. } | ExprKind::NullsafeDynamicMethodCall { .. } => {
            let local = module
                .next_label("mixed_nullsafe_method_value")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(local.clone());
            emit_alloc_mixed_cell(&local, module);
            let emitted = emit_expr(candidate, module)?;
            emit_store_emitted_value_kind(
                &format!("${}", local),
                emitted,
                "mixed_nullsafe_method_value",
                module,
            )?;
            Ok(Some(local))
        }
        ExprKind::PropertyAccess { object, property }
            if object_property_value_kind(object, property, module) == Some(ValueKind::Mixed) =>
        {
            let local = module
                .next_label("mixed_property_value")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(local.clone());
            match emit_property_access_expr(candidate, object, property, module)? {
                ValueKind::Mixed => {
                    module.body().line(&format!("local.set ${}", local));
                    Ok(Some(local))
                }
                _ => unreachable!("mixed property metadata must load a mixed value"),
            }
        }
        ExprKind::StaticPropertyAccess { receiver, property }
            if static_property_value_kind(receiver, property, module) == Some(ValueKind::Mixed) =>
        {
            let local = module
                .next_label("mixed_static_property_value")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(local.clone());
            match emit_static_property_access_expr(candidate, receiver, property, module)? {
                ValueKind::Mixed => {
                    module.body().line(&format!("local.set ${}", local));
                    Ok(Some(local))
                }
                _ => unreachable!("mixed static property metadata must load a mixed value"),
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            if expression_is_stringy(array, module) {
                let local = module
                    .next_label("mixed_string_index_value")
                    .trim_start_matches('$')
                    .to_string();
                module.declare_i32_local(local.clone());
                emit_alloc_mixed_cell(&local, module);
                module.body().line(&format!("local.get ${}", local));
                emit_string_index_to_stack(candidate, array, index, module)?;
                module.body().line("call $__rt_value_store_string");
                return Ok(Some(local));
            }
            if let ExprKind::StaticPropertyAccess { receiver, property } = &array.kind {
                let source = module
                    .next_label("mixed_static_property_array_source")
                    .trim_start_matches('$')
                    .to_string();
                let local = module
                    .next_label("mixed_static_property_array_access_value")
                    .trim_start_matches('$')
                    .to_string();
                module.declare_mixed_local(source.clone());
                module.declare_i32_local(local.clone());
                match emit_static_property_access_expr(array, receiver, property, module)? {
                    ValueKind::Mixed => {
                        module.body().line(&format!("local.set ${}", source));
                    }
                    _ => {
                        return Err(CompileError::new(
                            candidate.span,
                            "wasm32-web static property array reads require array static-property storage",
                        ));
                    }
                }
                emit_alloc_mixed_cell(&local, module);
                let temp_access = Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(Expr::new(ExprKind::Variable(source), array.span)),
                        index: index.clone(),
                    },
                    candidate.span,
                );
                emit_store_value_cell(&format!("${}", local), &temp_access, module)?;
                return Ok(Some(local));
            }
            if nested_array_access_requires_layout(array) {
                let local = module
                    .next_label("mixed_nested_array_access_value")
                    .trim_start_matches('$')
                    .to_string();
                module.declare_i32_local(local.clone());
                emit_alloc_mixed_cell(&local, module);
                emit_store_value_cell(&format!("${}", local), candidate, module)?;
                return Ok(Some(local));
            }
            if !matches!(array.kind, ExprKind::Variable(_)) && expression_has_array_type(array, module) {
                let temp = module
                    .next_label("mixed_direct_array_access_source")
                    .trim_start_matches('$')
                    .to_string();
                let local = module
                    .next_label("mixed_direct_array_access_value")
                    .trim_start_matches('$')
                    .to_string();
                module.declare_array_local(temp.clone());
                module.declare_i32_local(local.clone());
                emit_array_assign(&temp, array, module)?;
                emit_alloc_mixed_cell(&local, module);
                let temp_access = Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(Expr::new(ExprKind::Variable(temp), array.span)),
                        index: index.clone(),
                    },
                    candidate.span,
                );
                emit_store_value_cell(&format!("${}", local), &temp_access, module)?;
                return Ok(Some(local));
            }
            if let ExprKind::Variable(source) = &array.kind {
                if module.local_kind(source) == Some(LocalKind::Mixed) {
                    let local = module
                        .next_label("mixed_unknown_array_access_value")
                        .trim_start_matches('$')
                        .to_string();
                    module.declare_i32_local(local.clone());
                    emit_alloc_mixed_cell(&local, module);
                    emit_store_value_cell(&format!("${}", local), candidate, module)?;
                    return Ok(Some(local));
                }
                if module.local_kind(source) == Some(LocalKind::Array)
                    && matches!(module.array_layout(source), ArrayLayout::Value | ArrayLayout::Assoc)
                {
                    let local = module
                        .next_label("mixed_array_access_value")
                        .trim_start_matches('$')
                        .to_string();
                    module.declare_i32_local(local.clone());
                    emit_alloc_mixed_cell(&local, module);
                    emit_store_value_cell(&format!("${}", local), candidate, module)?;
                    return Ok(Some(local));
                }
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}

pub(in crate::codegen::wasm) fn emit_mixed_float_comparison(
    name: &str,
    payload: f64,
    op: &BinOp,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}", name));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("call $__rt_mixed_tag_equals");
    emit_mixed_f64_payload(name, module);
    module.body().line(&format!("f64.const {}", payload));
    module.body().line("f64.eq");
    module.body().line("i32.and");
    if matches!(op, BinOp::StrictNotEq) {
        module.body().line("i32.eqz");
    }
}

pub(in crate::codegen::wasm) fn emit_mixed_tag_payload_comparison(
    name: &str,
    tag: i32,
    payload: i64,
    op: &BinOp,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}", name));
    module.body().line(&format!("i32.const {}", tag));
    module.body().line("call $__rt_mixed_tag_equals");
    emit_mixed_i64_payload(name, module);
    module.body().line(&format!("i64.const {}", payload));
    module.body().line("i64.eq");
    module.body().line("i32.and");
    if matches!(op, BinOp::StrictNotEq) {
        module.body().line("i32.eqz");
    }
}

pub(in crate::codegen::wasm) fn emit_mixed_string_comparison(
    name: &str,
    value: &str,
    op: &BinOp,
    module: &mut WasmModule,
) {
    let (needle_ptr, needle_len) = module.intern_string(value);
    let matched = module.next_label("mixed_string_compare_matched");
    let offset = module.next_label("mixed_string_compare_offset");
    let item_ptr = module.next_label("mixed_string_compare_ptr");
    let item_len = module.next_label("mixed_string_compare_len");
    let mismatch = module.next_label("mixed_string_compare_mismatch");
    let done = module.next_label("mixed_string_compare_done");
    let loop_label = module.next_label("mixed_string_compare_loop");
    for local in [&matched, &offset, &item_ptr, &item_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().open(&format!("block {}", done));
    module.body().line(&format!("local.get ${}", name));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("call $__rt_mixed_tag_equals");
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", done));
    emit_mixed_i32_payload(name, 8, module);
    module.body().line(&format!("local.set {}", item_ptr));
    emit_mixed_i32_payload(name, 12, module);
    module.body().line(&format!("local.set {}", item_len));
    module.body().line(&format!("local.get {}", item_len));
    module.body().line(&format!("i32.const {}", needle_len));
    module.body().line("i32.ne");
    module.body().line(&format!("br_if {}", done));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", matched));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", offset));
    module.body().open(&format!("block {}", mismatch));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", offset));
    module.body().line(&format!("i32.const {}", needle_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", mismatch));
    module.body().line(&format!("local.get {}", item_ptr));
    module.body().line(&format!("local.get {}", offset));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("i32.const {}", needle_ptr));
    module.body().line(&format!("local.get {}", offset));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("br {}", mismatch));
    module.body().close("end");
    module.body().line(&format!("local.get {}", offset));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", offset));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", matched));
    if matches!(op, BinOp::StrictNotEq) {
        module.body().line("i32.eqz");
    }
}

pub(in crate::codegen::wasm) fn emit_indexed_array_equality(
    expr: &Expr,
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if !matches!(
        op,
        BinOp::Eq | BinOp::NotEq | BinOp::StrictEq | BinOp::StrictNotEq
    ) {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web indexed array comparisons currently support equality only",
        ));
    }
    let left_len = indexed_array_len_for_compare(left, module)?;
    let right_len = indexed_array_len_for_compare(right, module)?;
    if left_len != right_len {
        let equals = false;
        let result = if matches!(op, BinOp::NotEq | BinOp::StrictNotEq) {
            !equals
        } else {
            equals
        };
        module.body().line(&format!("i32.const {}", i32::from(result)));
        return Ok(ValueKind::Bool);
    }
    let result = module.next_label("array_eq");
    module.declare_i32_local(result.trim_start_matches('$').to_string());
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", result));
    for index in 0..left_len {
        emit_indexed_array_value_for_compare(left, index, module)?;
        emit_indexed_array_value_for_compare(right, index, module)?;
        module.body().line("i64.eq");
        module.body().line(&format!("local.get {}", result));
        module.body().line("i32.and");
        module.body().line(&format!("local.set {}", result));
    }
    module.body().line(&format!("local.get {}", result));
    if matches!(op, BinOp::NotEq | BinOp::StrictNotEq) {
        module.body().line("i32.eqz");
    }
    Ok(ValueKind::Bool)
}

pub(in crate::codegen::wasm) fn indexed_array_len_for_compare(
    array: &Expr,
    module: &WasmModule,
) -> Result<usize, CompileError> {
    match &array.kind {
        ExprKind::ArrayLiteral(items) => Ok(items.len()),
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Array) => {
            module.array_length(name).ok_or_else(|| {
                CompileError::new(
                    array.span,
                    "wasm32-web array comparison requires a known indexed array length",
                )
            })
        }
        ExprKind::ArrayLiteralAssoc(_) => Err(array_unsupported(array)),
        _ => Err(CompileError::new(
            array.span,
            "wasm32-web array comparison currently supports indexed array values only",
        )),
    }
}

pub(in crate::codegen::wasm) fn emit_indexed_array_value_for_compare(
    array: &Expr,
    index: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match &array.kind {
        ExprKind::ArrayLiteral(items) => {
            let item = items.get(index).ok_or_else(|| {
                CompileError::new(
                    array.span,
                    "wasm32-web array comparison index is out of bounds",
                )
            })?;
            require_int(item, module)
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Array) => {
            module.body().line(&format!("local.get ${}_ptr", name));
            module.body().line(&format!("i32.const {}", index * 8));
            module.body().line("i32.add");
            module.body().line("i64.load");
            Ok(())
        }
        _ => Err(CompileError::new(
            array.span,
            "wasm32-web array comparison currently supports indexed array values only",
        )),
    }
}

pub(in crate::codegen::wasm) fn emit_static_scalar_comparison(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    module: &WasmModule,
) -> Option<bool> {
    if !matches!(
        op,
        BinOp::Eq
            | BinOp::NotEq
            | BinOp::Lt
            | BinOp::Gt
            | BinOp::LtEq
            | BinOp::GtEq
            | BinOp::StrictEq
            | BinOp::StrictNotEq
    ) {
        return None;
    }
    let (left, right) = if matches!(op, BinOp::StrictEq | BinOp::StrictNotEq) {
        (static_scalar_value(left, module)?, static_scalar_value(right, module)?)
    } else {
        (
            static_or_tracked_scalar_value(left, module)?,
            static_or_tracked_scalar_value(right, module)?,
        )
    };
    Some(compare_static_scalars(&left, op, &right))
}

pub(in crate::codegen::wasm) fn emit_bool_comparison(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    emit_condition(left, module)?;
    emit_condition(right, module)?;
    let instr = match op {
        BinOp::Eq => "i32.eq",
        BinOp::NotEq => "i32.ne",
        BinOp::Lt => "i32.lt_u",
        BinOp::Gt => "i32.gt_u",
        BinOp::LtEq => "i32.le_u",
        BinOp::GtEq => "i32.ge_u",
        _ => unreachable!(),
    };
    module.body().line(instr);
    Ok(ValueKind::Bool)
}

pub(in crate::codegen::wasm) fn emit_null_coalesce_expr(
    _expr: &Expr,
    value: &Expr,
    default: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if matches!(value.kind, ExprKind::Null) {
        return emit_expr(default, module);
    }
    if expression_is_stringy(value, module) {
        emit_string_value_to_stack(value, module)?;
        return Ok(ValueKind::Str);
    }
    if expression_is_stringy(default, module) {
        return emit_expr(value, module);
    }
    emit_expr(value, module)
}
