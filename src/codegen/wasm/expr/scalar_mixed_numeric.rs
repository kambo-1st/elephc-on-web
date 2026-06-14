//! Purpose:
//! Emits wasm32-web mixed numeric and value-cell numeric coercion helpers.
//! Keeps boxed mixed arithmetic support separate from top-level operator dispatch.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::scalar_ops`
//! - Sibling wasm expression modules that consume mixed numeric operands.
//!
//! Key details:
//! - Preserves boxed Mixed/value-cell tag handling and traps unsupported array coercions.

use super::*;

pub(in crate::codegen::wasm) fn emit_known_mixed_numeric_binary(
    expr: &Expr,
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    let left_cell_kind = known_mixed_value_cell_kind(left, module)?;
    let right_cell_kind = known_mixed_value_cell_kind(right, module)?;
    if left_cell_kind == Some(ValueCellKind::Str) || right_cell_kind == Some(ValueCellKind::Str) {
        if left_cell_kind == Some(ValueCellKind::Array) || right_cell_kind == Some(ValueCellKind::Array) {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web mixed numeric arithmetic does not support array values yet",
            ));
        }
        let integer_op = matches!(
            op,
            BinOp::Mod | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight
        );
        let use_float = !integer_op;
        emit_known_mixed_string_numeric_operand(left, use_float, module)?;
        emit_known_mixed_string_numeric_operand(right, use_float, module)?;
        let instr = match (op, use_float) {
            (BinOp::Add, true) => "f64.add",
            (BinOp::Sub, true) => "f64.sub",
            (BinOp::Mul, true) => "f64.mul",
            (BinOp::Div, true) => "f64.div",
            (BinOp::Pow, true) => "call $host_pow",
            (BinOp::Mod, false) => "i64.rem_s",
            (BinOp::BitAnd, false) => "i64.and",
            (BinOp::BitOr, false) => "i64.or",
            (BinOp::BitXor, false) => "i64.xor",
            (BinOp::ShiftLeft, false) => "i64.shl",
            (BinOp::ShiftRight, false) => "i64.shr_s",
            _ => {
                return Err(CompileError::new(
                    expr.span,
                    "wasm32-web mixed string numeric operations currently support +, -, *, /, %, **, &, |, ^, <<, and >>",
                ))
            }
        };
        module.body().line(instr);
        return Ok(Some(if use_float { ValueKind::Float } else { ValueKind::Int }));
    }
    let left_kind = known_mixed_numeric_kind(left, module)?;
    let right_kind = known_mixed_numeric_kind(right, module)?;
    let left_dynamic = dynamic_numeric_operand_may_materialize(left, module)?;
    let right_dynamic = dynamic_numeric_operand_may_materialize(right, module)?;
    if left_kind.is_none() && right_kind.is_none() && !left_dynamic && !right_dynamic {
        return Ok(None);
    }
    let use_float = matches!(op, BinOp::Pow)
        || (!matches!(
        op,
        BinOp::Mod | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight
    )
        && (matches!(op, BinOp::Div)
            || left_kind == Some(ValueKind::Float)
            || right_kind == Some(ValueKind::Float)
            || left_dynamic
            || right_dynamic
            || expression_is_floaty(left, module)
            || expression_is_floaty(right, module)));
    emit_known_mixed_numeric_operand(left, use_float, module)?;
    emit_known_mixed_numeric_operand(right, use_float, module)?;
    let instr = match (op, use_float) {
        (BinOp::Add, true) => "f64.add",
        (BinOp::Sub, true) => "f64.sub",
        (BinOp::Mul, true) => "f64.mul",
        (BinOp::Div, true) => "f64.div",
        (BinOp::Pow, true) => "call $host_pow",
        (BinOp::Add, false) => "i64.add",
        (BinOp::Sub, false) => "i64.sub",
        (BinOp::Mul, false) => "i64.mul",
        (BinOp::Mod, false) => "i64.rem_s",
        (BinOp::BitAnd, false) => "i64.and",
        (BinOp::BitOr, false) => "i64.or",
        (BinOp::BitXor, false) => "i64.xor",
        (BinOp::ShiftLeft, false) => "i64.shl",
        (BinOp::ShiftRight, false) => "i64.shr_s",
        _ => {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web mixed numeric operations currently support +, -, *, /, %, **, &, |, ^, <<, and >>",
            ))
        }
    };
    module.body().line(instr);
    Ok(Some(if use_float { ValueKind::Float } else { ValueKind::Int }))
}

pub(in crate::codegen::wasm) fn emit_known_mixed_negate(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    if let ExprKind::Variable(name) = &expr.kind {
        if module.local_kind(name) == Some(LocalKind::Mixed) {
            match module.mixed_value_cell_kind(name) {
                Some(ValueCellKind::Float) => {
                    emit_mixed_f64_payload(name, module);
                    module.body().line("f64.neg");
                    return Ok(Some(ValueKind::Float));
                }
                Some(ValueCellKind::Int) | Some(ValueCellKind::Bool) => {
                    module.body().line("i64.const 0");
                    emit_mixed_i64_payload(name, module);
                    module.body().line("i64.sub");
                    return Ok(Some(ValueKind::Int));
                }
                Some(ValueCellKind::Null) => {
                    module.body().line("i64.const 0");
                    return Ok(Some(ValueKind::Int));
                }
                Some(ValueCellKind::Str) | Some(ValueCellKind::Array) => {
                    return Err(CompileError::new(
                        expr.span,
                        "wasm32-web unary minus does not support string or array mixed values yet",
                    ))
                }
                None => {}
            }
        }
    }
    match known_mixed_numeric_kind(expr, module)? {
        Some(ValueKind::Float) => {
            emit_known_mixed_numeric_operand(expr, true, module)?;
            module.body().line("f64.neg");
            Ok(Some(ValueKind::Float))
        }
        Some(ValueKind::Int) | Some(ValueKind::Bool) | Some(ValueKind::Null) => {
            module.body().line("i64.const 0");
            emit_known_mixed_numeric_operand(expr, false, module)?;
            module.body().line("i64.sub");
            Ok(Some(ValueKind::Int))
        }
        None => Ok(None),
        Some(ValueKind::Str)
        | Some(ValueKind::Array)
        | Some(ValueKind::Object)
        | Some(ValueKind::Callable)
        | Some(ValueKind::Mixed)
        | Some(ValueKind::Never) => {
            unreachable!("mixed numeric kind only returns numeric/null kinds")
        }
    }
}

pub(in crate::codegen::wasm) fn emit_known_mixed_bit_not(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    if let ExprKind::Variable(name) = &expr.kind {
        if module.local_kind(name) == Some(LocalKind::Mixed) {
            match module.mixed_value_cell_kind(name) {
                Some(ValueCellKind::Int) => {
                    emit_mixed_i64_payload(name, module);
                    module.body().line("i64.const -1");
                    module.body().line("i64.xor");
                    return Ok(Some(ValueKind::Int));
                }
                Some(ValueCellKind::Float) => {
                    emit_mixed_f64_payload(name, module);
                    module.body().line("i64.trunc_f64_s");
                    module.body().line("i64.const -1");
                    module.body().line("i64.xor");
                    return Ok(Some(ValueKind::Int));
                }
                Some(ValueCellKind::Bool) | Some(ValueCellKind::Null) => {
                    return Err(CompileError::new(
                        expr.span,
                        "wasm32-web bitwise not does not support bool or null mixed values",
                    ))
                }
                Some(ValueCellKind::Str) | Some(ValueCellKind::Array) => {
                    return Err(CompileError::new(
                        expr.span,
                        "wasm32-web bitwise not does not support string or array mixed values yet",
                    ))
                }
                None => {}
            }
        }
    }
    if dynamic_bitwise_value_cell_kind(expr, module).is_some() {
        emit_known_mixed_numeric_operand(expr, false, module)?;
        module.body().line("i64.const -1");
        module.body().line("i64.xor");
        return Ok(Some(ValueKind::Int));
    }
    match known_mixed_value_cell_kind(expr, module)? {
        Some(ValueCellKind::Int) => {
            emit_known_mixed_numeric_operand(expr, false, module)?;
            module.body().line("i64.const -1");
            module.body().line("i64.xor");
            Ok(Some(ValueKind::Int))
        }
        Some(ValueCellKind::Float) => {
            emit_known_mixed_numeric_operand(expr, false, module)?;
            module.body().line("i64.const -1");
            module.body().line("i64.xor");
            Ok(Some(ValueKind::Int))
        }
        Some(ValueCellKind::Bool) | Some(ValueCellKind::Null) => Err(CompileError::new(
            expr.span,
            "wasm32-web bitwise not does not support bool or null mixed values",
        )),
        Some(ValueCellKind::Str) | Some(ValueCellKind::Array) => Err(CompileError::new(
            expr.span,
            "wasm32-web bitwise not does not support string or array mixed values yet",
        )),
        None => Ok(None),
    }
}

pub(in crate::codegen::wasm) fn dynamic_bitwise_value_cell_kind(expr: &Expr, module: &WasmModule) -> Option<ValueKind> {
    let ExprKind::ArrayAccess { array, index } = &expr.kind else {
        return None;
    };
    let ExprKind::Variable(name) = &array.kind else {
        return None;
    };
    if module.local_kind(name) != Some(LocalKind::Array) {
        return None;
    }
    let dynamic_index = match module.array_layout(name) {
        ArrayLayout::Value => static_or_const_or_i64_local_value(index, module).is_none(),
        ArrayLayout::Assoc => assoc_static_access_kind(array, index, module).is_none(),
        ArrayLayout::CompactInt => false,
    };
    if !dynamic_index {
        return None;
    }
    let kinds = module.array_value_cell_kinds(name)?;
    if kinds
        .iter()
        .any(|kind| !matches!(kind, ValueCellKind::Int | ValueCellKind::Float))
    {
        return None;
    }
    Some(ValueKind::Int)
}

pub(in crate::codegen::wasm) fn known_mixed_numeric_kind(
    expr: &Expr,
    module: &WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    if let Some(kind) = dynamic_numeric_value_cell_kind(expr, module) {
        return Ok(Some(kind));
    }
    if let ExprKind::Variable(name) = &expr.kind {
        if module.local_kind(name) == Some(LocalKind::Mixed) && module.mixed_value_cell_kind(name).is_none() {
            return Ok(Some(ValueKind::Float));
        }
    }
    match known_mixed_value_cell_kind(expr, module)? {
        Some(ValueCellKind::Float) => Ok(Some(ValueKind::Float)),
        Some(ValueCellKind::Int) | Some(ValueCellKind::Bool) | Some(ValueCellKind::Null) => {
            Ok(Some(ValueKind::Int))
        }
        Some(ValueCellKind::Str) | Some(ValueCellKind::Array) => Err(CompileError::new(
            expr.span,
            "wasm32-web mixed numeric arithmetic does not support string or array values yet",
        )),
        None => Ok(None),
    }
}

pub(in crate::codegen::wasm) fn dynamic_numeric_operand_may_materialize(
    expr: &Expr,
    module: &WasmModule,
) -> Result<bool, CompileError> {
    if dynamic_numeric_value_cell_kind(expr, module).is_some() {
        return Ok(true);
    }
    if known_mixed_value_cell_kind(expr, module)?.is_some() {
        return Ok(true);
    }
    match &expr.kind {
        ExprKind::ArrayAccess { array, .. } if nested_array_access_requires_layout(array) => Ok(true),
        ExprKind::ArrayAccess { array, .. } => {
            let ExprKind::Variable(source) = &array.kind else {
                return Ok(false);
            };
            Ok(matches!(
                module.local_kind(source),
                Some(LocalKind::Mixed) | Some(LocalKind::Array)
            ))
        }
        ExprKind::Variable(name) => Ok(module.local_kind(name) == Some(LocalKind::Mixed)),
        ExprKind::FunctionCall { name, .. } => Ok(
            (module.has_function(name) && module.function_return_kind(name) == Some(ValueKind::Mixed))
                || matches!(name.to_ascii_lowercase().as_str(), "array_search" | "array_pop" | "array_shift"),
        ),
        _ => Ok(false),
    }
}

pub(in crate::codegen::wasm) fn dynamic_numeric_value_cell_kind(expr: &Expr, module: &WasmModule) -> Option<ValueKind> {
    dynamic_numeric_value_cell_kind_with(expr, module, true)
}

pub(in crate::codegen::wasm) fn dynamic_numeric_value_cell_kind_with(
    expr: &Expr,
    module: &WasmModule,
    allow_bool: bool,
) -> Option<ValueKind> {
    let ExprKind::ArrayAccess { array, index } = &expr.kind else {
        return None;
    };
    if allow_bool
        && nested_array_access_requires_layout(array)
        && nested_array_index_metadata(array, index, module).is_some_and(|(metadata, _)| {
            metadata.layout == ArrayLayout::Value && metadata.value_kinds.is_none()
        })
    {
        return Some(ValueKind::Float);
    }
    let ExprKind::Variable(name) = &array.kind else {
        return None;
    };
    if module.local_kind(name) != Some(LocalKind::Array) {
        return None;
    }
    let dynamic_index = match module.array_layout(name) {
        ArrayLayout::Value => static_or_const_or_i64_local_value(index, module).is_none(),
        ArrayLayout::Assoc => assoc_static_access_kind(array, index, module).is_none(),
        ArrayLayout::CompactInt => false,
    };
    if !dynamic_index {
        return None;
    }
    let kinds = module.array_value_cell_kinds(name)?;
    if kinds
        .iter()
        .any(|kind| matches!(kind, ValueCellKind::Array) || (!allow_bool && *kind == ValueCellKind::Bool))
    {
        return None;
    }
    if kinds
        .iter()
        .any(|kind| matches!(kind, ValueCellKind::Float | ValueCellKind::Str))
    {
        Some(ValueKind::Float)
    } else {
        Some(ValueKind::Int)
    }
}

pub(in crate::codegen::wasm) fn known_mixed_value_cell_kind(
    expr: &Expr,
    module: &WasmModule,
) -> Result<Option<ValueCellKind>, CompileError> {
    match &expr.kind {
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Mixed) => {
            Ok(module.mixed_value_cell_kind(name))
        }
        ExprKind::FunctionCall { name, .. }
            if module.has_function(name) && module.function_return_kind(name) == Some(ValueKind::Mixed) =>
        {
            Ok(module.function_mixed_return_kind(name))
        }
        ExprKind::PropertyAccess { object, property } => {
            Ok(object_property_mixed_value_cell_kind(object, property, module))
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            if !object_expr_is_known_non_null(object, module) {
                return Ok(None);
            }
            Ok(object_property_mixed_value_cell_kind(object, property, module))
        }
        ExprKind::DynamicPropertyAccess { object, property } => {
            let Some(property_name) = static_string_value(property, module) else {
                return Ok(None);
            };
            Ok(object_property_mixed_value_cell_kind(
                object,
                &property_name,
                module,
            ))
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            if !object_expr_is_known_non_null(object, module) {
                return Ok(None);
            }
            let Some(property_name) = static_string_value(property, module) else {
                return Ok(None);
            };
            Ok(object_property_mixed_value_cell_kind(
                object,
                &property_name,
                module,
            ))
        }
        ExprKind::MethodCall { object, method, .. } => {
            Ok(method_call_mixed_return_kind(object, method, module))
        }
        ExprKind::NullsafeMethodCall { object, method, .. } => {
            if !object_expr_is_known_non_null(object, module) {
                return Ok(None);
            }
            Ok(method_call_mixed_return_kind(object, method, module))
        }
        ExprKind::StaticMethodCall { receiver, method, .. } => {
            Ok(static_method_call_mixed_return_kind(receiver, method, module))
        }
        ExprKind::ArrayAccess { array, index } => {
            if let Some(kind) = nested_array_static_access_kind(array, index, module) {
                return Ok(Some(kind));
            }
            let ExprKind::Variable(name) = &array.kind else {
                return Ok(None);
            };
            if module.local_kind(name) != Some(LocalKind::Array) {
                return Ok(None);
            }
            if module.array_layout(name) == ArrayLayout::Assoc {
                if let Some(kind) = assoc_static_access_kind(array, index, module) {
                    return Ok(Some(kind));
                }
                let Some(kinds) = module.array_value_cell_kinds(name) else {
                    return Ok(None);
                };
                let Some(first) = kinds.first().copied() else {
                    return Ok(None);
                };
                return if kinds.iter().all(|kind| *kind == first) {
                    Ok(Some(first))
                } else {
                    Ok(None)
                };
            }
            if module.array_layout(name) != ArrayLayout::Value {
                return Ok(None);
            }
            if let Some(index_value) = static_or_const_or_i64_local_value(index, module) {
                let Ok(index) = usize::try_from(index_value) else {
                    return Ok(None);
                };
                return Ok(module.array_value_cell_kind(name, index));
            }
            let Some(kinds) = module.array_value_cell_kinds(name) else {
                return Ok(None);
            };
            let Some(first) = kinds.first().copied() else {
                return Ok(None);
            };
            if kinds.iter().all(|kind| *kind == first) {
                Ok(Some(first))
            } else {
                Ok(None)
            }
        }
        _ => Ok(None),
    }
}

pub(in crate::codegen::wasm) fn emit_known_mixed_string_numeric_operand(
    expr: &Expr,
    use_float: bool,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if known_mixed_value_cell_kind(expr, module)? == Some(ValueCellKind::Str) {
        let Some(cell) = materialize_mixed_value_cell(expr, module)? else {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web mixed string numeric operation could not materialize this value cell",
            ));
        };
        let number = module.next_label("mixed_string_numeric_operand_number");
        module.declare_f64_local(number.trim_start_matches('$').to_string());
        emit_mixed_string_leading_numeric_value_checked(&cell, &number, module);
        module.body().line(&format!("local.get {}", number));
        if !use_float {
            module.body().line("i64.trunc_f64_s");
        }
        return Ok(());
    }
    emit_known_mixed_numeric_operand(expr, use_float, module)
}

pub(in crate::codegen::wasm) fn emit_known_mixed_numeric_operand(
    expr: &Expr,
    use_float: bool,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match &expr.kind {
        ExprKind::BoolLiteral(value) => {
            if use_float {
                module
                    .body()
                    .line(&format!("f64.const {}", if *value { 1 } else { 0 }));
            } else {
                module
                    .body()
                    .line(&format!("i64.const {}", if *value { 1 } else { 0 }));
            }
            return Ok(());
        }
        ExprKind::Null => {
            module
                .body()
                .line(if use_float { "f64.const 0" } else { "i64.const 0" });
            return Ok(());
        }
        _ => {}
    }
    if let ExprKind::Variable(name) = &expr.kind {
        if module.local_kind(name) == Some(LocalKind::Mixed) {
            match module.mixed_value_cell_kind(name) {
                Some(ValueCellKind::Float) => {
                    emit_mixed_f64_payload(name, module);
                    if !use_float {
                        module.body().line("i64.trunc_f64_s");
                    }
                    return Ok(());
                }
                Some(ValueCellKind::Int) | Some(ValueCellKind::Bool) => {
                    emit_mixed_i64_payload(name, module);
                    if use_float {
                        module.body().line("f64.convert_i64_s");
                    }
                    return Ok(());
                }
                Some(ValueCellKind::Null) => {
                    module
                        .body()
                        .line(if use_float { "f64.const 0" } else { "i64.const 0" });
                    return Ok(());
                }
                Some(ValueCellKind::Str) | Some(ValueCellKind::Array) => {
                    return Err(CompileError::new(
                        expr.span,
                        "wasm32-web mixed numeric arithmetic does not support string or array values yet",
                    ))
                }
                None => {}
            }
            emit_dynamic_value_cell_numeric_operand(name, use_float, module);
            return Ok(());
        }
    }
    if let Some(kind) = known_mixed_value_cell_kind(expr, module)? {
        let Some(cell) = materialize_mixed_value_cell(expr, module)? else {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web mixed numeric arithmetic could not materialize this value cell",
            ));
        };
        emit_known_value_cell_numeric_operand(&cell, kind, use_float, expr.span, module)?;
        return Ok(());
    }
    if dynamic_numeric_value_cell_kind(expr, module).is_some() {
        let Some(cell) = materialize_mixed_value_cell(expr, module)? else {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web mixed numeric arithmetic could not materialize this value cell",
            ));
        };
        emit_dynamic_value_cell_numeric_operand(&cell, use_float, module);
        return Ok(());
    }
    if matches!(expr.kind, ExprKind::ArrayAccess { .. }) {
        if let Some(cell) = materialize_mixed_value_cell(expr, module)? {
            emit_dynamic_value_cell_numeric_operand(&cell, use_float, module);
            return Ok(());
        }
    }
    if expression_is_booly(expr, module) {
        emit_condition(expr, module)?;
        module.body().line(if use_float {
            "f64.convert_i32_s"
        } else {
            "i64.extend_i32_s"
        });
        return Ok(());
    }
    if use_float {
        require_float(expr, module)
    } else {
        require_int(expr, module)
    }
}

pub(in crate::codegen::wasm) fn emit_dynamic_value_cell_numeric_operand(cell: &str, use_float: bool, module: &mut WasmModule) {
    let tag = module.next_label("dynamic_numeric_tag");
    module.declare_i32_local(tag.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("call $__rt_mixed_tag");
    module.body().line(&format!("local.set {}", tag));
    if use_float {
        let result = module.next_label("dynamic_numeric_f64");
        module.declare_f64_local(result.trim_start_matches('$').to_string());
        module.body().line("f64.const 0");
        module.body().line(&format!("local.set {}", result));
        module.body().line(&format!("local.get {}", tag));
        module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_ARRAY));
        module.body().line("i32.eq");
        module.body().open("if");
        module.body().line("unreachable");
        module.body().close("end");
        module.body().line(&format!("local.get {}", tag));
        module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
        module.body().line("i32.eq");
        module.body().open("if");
        module.body().line(&format!("local.get ${}", cell));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line("f64.load");
        module.body().line(&format!("local.set {}", result));
        module.body().close("end");
        module.body().line(&format!("local.get {}", tag));
        module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
        module.body().line("i32.eq");
        module.body().line(&format!("local.get {}", tag));
        module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
        module.body().line("i32.eq");
        module.body().line("i32.or");
        module.body().open("if");
        module.body().line(&format!("local.get ${}", cell));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line("i64.load");
        module.body().line("f64.convert_i64_s");
        module.body().line(&format!("local.set {}", result));
        module.body().close("end");
        module.body().line(&format!("local.get {}", tag));
        module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
        module.body().line("i32.eq");
        module.body().open("if");
        emit_mixed_string_leading_numeric_value_checked(cell, &result, module);
        module.body().close("end");
        module.body().line(&format!("local.get {}", result));
    } else {
        let result = module.next_label("dynamic_numeric_i64");
        module.declare_i64_local(result.trim_start_matches('$').to_string());
        module.body().line("i64.const 0");
        module.body().line(&format!("local.set {}", result));
        module.body().line(&format!("local.get {}", tag));
        module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_ARRAY));
        module.body().line("i32.eq");
        module.body().open("if");
        module.body().line("unreachable");
        module.body().close("end");
        module.body().line(&format!("local.get {}", tag));
        module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
        module.body().line("i32.eq");
        module.body().open("if");
        module.body().line(&format!("local.get ${}", cell));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line("f64.load");
        module.body().line("i64.trunc_f64_s");
        module.body().line(&format!("local.set {}", result));
        module.body().close("end");
        module.body().line(&format!("local.get {}", tag));
        module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
        module.body().line("i32.eq");
        module.body().line(&format!("local.get {}", tag));
        module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
        module.body().line("i32.eq");
        module.body().line("i32.or");
        module.body().open("if");
        module.body().line(&format!("local.get ${}", cell));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line("i64.load");
        module.body().line(&format!("local.set {}", result));
        module.body().close("end");
        module.body().line(&format!("local.get {}", tag));
        module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
        module.body().line("i32.eq");
        module.body().open("if");
        let number = module.next_label("dynamic_numeric_string_f64");
        module.declare_f64_local(number.trim_start_matches('$').to_string());
        emit_mixed_string_leading_numeric_value_checked(cell, &number, module);
        module.body().line(&format!("local.get {}", number));
        module.body().line("i64.trunc_f64_s");
        module.body().line(&format!("local.set {}", result));
        module.body().close("end");
        module.body().line(&format!("local.get {}", result));
    }
}

pub(in crate::codegen::wasm) fn emit_known_value_cell_numeric_operand(
    cell: &str,
    kind: ValueCellKind,
    use_float: bool,
    span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match kind {
        ValueCellKind::Float => {
            module.body().line(&format!("local.get ${}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("f64.load");
            if !use_float {
                module.body().line("i64.trunc_f64_s");
            }
            Ok(())
        }
        ValueCellKind::Int | ValueCellKind::Bool => {
            module.body().line(&format!("local.get ${}", cell));
            module.body().line("i32.const 8");
            module.body().line("i32.add");
            module.body().line("i64.load");
            if use_float {
                module.body().line("f64.convert_i64_s");
            }
            Ok(())
        }
        ValueCellKind::Null => {
            module
                .body()
                .line(if use_float { "f64.const 0" } else { "i64.const 0" });
            Ok(())
        }
        ValueCellKind::Str | ValueCellKind::Array => Err(CompileError::new(
            span,
            "wasm32-web mixed numeric arithmetic does not support string or array values yet",
        )),
    }
}
