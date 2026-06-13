//! Purpose:
//! Provides wasm32-web value-flow lowering for ternary/match expressions and argument cells.
//! Keeps mixed/value-array materialization helpers out of the expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` call, string, array, and output lowering modules.
//!
//! Key details:
//! - Helpers preserve wasm local kind conventions and boxed Mixed/value-cell runtime contracts.

use super::*;

pub(in crate::codegen::wasm) fn expression_is_booly(expr: &Expr, module: &WasmModule) -> bool {
    match &expr.kind {
        ExprKind::BoolLiteral(_) | ExprKind::Not(_) => true,
        ExprKind::Variable(name) => module.local_kind(name) == Some(LocalKind::I32),
        ExprKind::PropertyAccess { object, property } => {
            object_property_value_kind(object, property, module) == Some(ValueKind::Bool)
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            object_expr_is_known_non_null(object, module)
                && object_property_value_kind(object, property, module) == Some(ValueKind::Bool)
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            static_property_value_kind(receiver, property, module) == Some(ValueKind::Bool)
        }
        ExprKind::BinaryOp { op, .. } => matches!(
            op,
            BinOp::And
                | BinOp::Or
                | BinOp::Xor
                | BinOp::Eq
                | BinOp::NotEq
                | BinOp::StrictEq
                | BinOp::StrictNotEq
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::LtEq
                | BinOp::GtEq
        ),
        ExprKind::Cast {
            target: CastType::Bool,
            ..
        } => true,
        ExprKind::FunctionCall { name, .. }
            if module.function_return_kind(name) == Some(ValueKind::Bool) =>
        {
            true
        }
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("array_reduce")
                && array_reduce_static_callback_return_kind(args, module) == Some(ValueKind::Bool) =>
        {
            true
        }
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("call_user_func")
                && call_user_func_target(args, module).is_some_and(|(target, call_args)| {
                    callable_return_kind(&target, call_args, module) == Some(ValueKind::Bool)
                }) =>
        {
            true
        }
        ExprKind::ClosureCall { var, args } if callable_variable_target(module, var)
            .is_some_and(|target| callable_return_kind(&target, args, module) == Some(ValueKind::Bool)) =>
        {
            true
        }
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("call_user_func_array")
                && call_user_func_array_return_kind(args, module) == Some(ValueKind::Bool) =>
        {
            true
        }
        ExprKind::FunctionCall { name, .. }
            if name.eq_ignore_ascii_case("boolval")
                || matches!(
                    name.to_ascii_lowercase().as_str(),
                    "is_int" | "is_float" | "is_bool" | "is_null" | "is_string"
                ) =>
        {
            true
        }
        ExprKind::ArrayAccess { array, index } => {
            assoc_static_access_kind(array, index, module) == Some(ValueCellKind::Bool)
                || static_property_array_access_kind(array, index, module) == Some(ValueCellKind::Bool)
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => expression_is_booly(then_expr, module) && expression_is_booly(else_expr, module),
        ExprKind::ShortTernary { value, default } => {
            expression_is_booly(value, module) && expression_is_booly(default, module)
        }
        _ => false,
    }
}

fn expression_is_objecty(expr: &Expr, module: &WasmModule) -> bool {
    object_class_name_for_expr(expr, module).is_some() || object_expr_is_known_non_null(expr, module)
}

pub(in crate::codegen::wasm) fn emit_scalar_ternary(
    _expr: &Expr,
    condition: &Expr,
    then_expr: &Expr,
    else_expr: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    emit_condition(condition, module)?;
    if expression_is_floaty(then_expr, module) || expression_is_floaty(else_expr, module) {
        module.body().open("if (result f64)");
        require_float(then_expr, module)?;
        module.body().line("else");
        require_float(else_expr, module)?;
        module.body().close("end");
        return Ok(ValueKind::Float);
    }
    if expression_is_booly(then_expr, module) && expression_is_booly(else_expr, module) {
        module.body().open("if (result i32)");
        emit_condition(then_expr, module)?;
        module.body().line("else");
        emit_condition(else_expr, module)?;
        module.body().close("end");
        return Ok(ValueKind::Bool);
    }
    if expression_is_objecty(then_expr, module) && expression_is_objecty(else_expr, module) {
        module.body().open("if (result i32)");
        if emit_expr(then_expr, module)? != ValueKind::Object {
            return Err(CompileError::new(then_expr.span, "wasm32-web expected object value"));
        }
        module.body().line("else");
        if emit_expr(else_expr, module)? != ValueKind::Object {
            return Err(CompileError::new(else_expr.span, "wasm32-web expected object value"));
        }
        module.body().close("end");
        return Ok(ValueKind::Object);
    }
    module.body().open("if (result i64)");
    require_int(then_expr, module)?;
    module.body().line("else");
    require_int(else_expr, module)?;
    module.body().close("end");
    Ok(ValueKind::Int)
}

pub(in crate::codegen::wasm) fn emit_short_ternary(
    expr: &Expr,
    value: &Expr,
    default: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if expression_is_stringy(value, module) || expression_is_stringy(default, module) {
        if expression_is_stringy(value, module) && expression_is_stringy(default, module) {
            emit_short_string_ternary_value_to_stack(value, default, module)?;
            return Ok(ValueKind::Str);
        }
        return Err(CompileError::new(
            expr.span,
            "wasm32-web short ternary cannot mix string and non-string results yet",
        ));
    }
    if expression_is_floaty(value, module) || expression_is_floaty(default, module) {
        let temp = module.next_label("short_ternary").trim_start_matches('$').to_string();
        module.declare_f64_local(temp.clone());
        require_float(value, module)?;
        module.body().line(&format!("local.set ${}", temp));
        module.body().line(&format!("local.get ${}", temp));
        module.body().line("f64.const 0");
        module.body().line("f64.ne");
        module.body().open("if (result f64)");
        module.body().line(&format!("local.get ${}", temp));
        module.body().line("else");
        require_float(default, module)?;
        module.body().close("end");
        return Ok(ValueKind::Float);
    }
    if expression_is_booly(value, module) && expression_is_booly(default, module) {
        let temp = module.next_label("short_ternary").trim_start_matches('$').to_string();
        module.declare_i32_local(temp.clone());
        emit_condition(value, module)?;
        module.body().line(&format!("local.set ${}", temp));
        module.body().line(&format!("local.get ${}", temp));
        module.body().open("if (result i32)");
        module.body().line(&format!("local.get ${}", temp));
        module.body().line("else");
        emit_condition(default, module)?;
        module.body().close("end");
        return Ok(ValueKind::Bool);
    }
    let temp = module.next_label("short_ternary").trim_start_matches('$').to_string();
    module.declare_i64_local(temp.clone());
    require_int(value, module)?;
    module.body().line(&format!("local.set ${}", temp));
    module.body().line(&format!("local.get ${}", temp));
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().open("if (result i64)");
    module.body().line(&format!("local.get ${}", temp));
    module.body().line("else");
    require_int(default, module)?;
    module.body().close("end");
    Ok(ValueKind::Int)
}

pub(in crate::codegen::wasm) fn emit_scalar_match(
    expr: &Expr,
    subject: &Expr,
    arms: &[(Vec<Expr>, Expr)],
    default: Option<&Expr>,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if arms.is_empty() && default.is_none() {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web match expression requires at least one arm or default",
        ));
    }
    let result_kind = match_result_kind(arms, default, module)?;
    let subject_local = module.next_label("match_subject").trim_start_matches('$').to_string();
    module.declare_i64_local(subject_local.clone());
    require_int(subject, module)?;
    module.body().line(&format!("local.set ${}", subject_local));
    emit_match_chain(&subject_local, arms, default, result_kind, module)?;
    Ok(result_kind)
}

pub(in crate::codegen::wasm) fn match_result_kind(
    arms: &[(Vec<Expr>, Expr)],
    default: Option<&Expr>,
    module: &WasmModule,
) -> Result<ValueKind, CompileError> {
    let values = arms
        .iter()
        .map(|(_, value)| value)
        .chain(default)
        .collect::<Vec<_>>();
    if values.iter().any(|value| expression_is_stringy(value, module)) {
        if values.iter().all(|value| expression_is_stringy(value, module)) {
            return Ok(ValueKind::Str);
        }
        return Err(CompileError::new(values[0].span, "wasm32-web match cannot mix string and non-string results yet"));
    }
    if values.iter().any(|value| expression_is_floaty(value, module)) {
        return Ok(ValueKind::Float);
    }
    if !values.is_empty() && values.iter().all(|value| expression_is_booly(value, module)) {
        return Ok(ValueKind::Bool);
    }
    if !values.is_empty() && values.iter().all(|value| expression_is_arrayy(value, module)) {
        return Ok(ValueKind::Array);
    }
    if !values.is_empty() && values.iter().all(|value| expression_is_objecty(value, module)) {
        return Ok(ValueKind::Object);
    }
    Ok(ValueKind::Int)
}

pub(in crate::codegen::wasm) fn emit_mixed_arg_assign(
    local: &str,
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let ExprKind::FunctionCall { name, args } = &expr.kind {
        if module.has_function(name) && module.function_return_kind(name) == Some(ValueKind::Mixed) {
            emit_user_function_args(expr, name, args, module)?;
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(name)));
            module.body().line(&format!("local.set ${}", local));
            module.set_mixed_value_cell_kind(local, module.function_mixed_return_kind(name));
            return Ok(());
        }
        if matches!(name.to_ascii_lowercase().as_str(), "array_pop" | "array_shift") {
            module.set_mixed_value_cell_kind(local, None);
            if name.eq_ignore_ascii_case("array_pop")
                && emit_unknown_mixed_array_pop_assign(local, expr, args, module)?
            {
                return Ok(());
            }
            if name.eq_ignore_ascii_case("array_shift")
                && emit_unknown_mixed_array_shift_assign(local, expr, args, module)?
            {
                return Ok(());
            }
            return emit_mixed_value_array_pop_shift_assign(local, expr, name, args, module);
        }
    }
    if let ExprKind::Variable(source) = &expr.kind {
        if module.local_kind(source) == Some(LocalKind::Mixed) {
            emit_alloc_mixed_cell(local, module);
            emit_copy_value_cell_from_addr_to_addr(
                &format!("${}", local),
                &format!("${}", source),
                module,
            );
            let kind = module.mixed_value_cell_kind(source);
            module.set_mixed_value_cell_kind(local, kind);
            return Ok(());
        }
        if module.local_kind(source) == Some(LocalKind::I32) {
            emit_alloc_mixed_cell(local, module);
            module.body().line(&format!("local.get ${}", local));
            module.body().line(&format!("local.get ${}", source));
            module.body().line("call $__rt_value_store_bool");
            module.set_mixed_value_cell_kind(local, Some(ValueCellKind::Bool));
            return Ok(());
        }
        if module.local_kind(source) == Some(LocalKind::Array) {
            emit_alloc_mixed_cell(local, module);
            module.body().line(&format!("local.get ${}", local));
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get ${}_len", source));
            module.body().line("call $__rt_value_store_array");
            module.set_mixed_value_cell_kind(local, Some(ValueCellKind::Array));
            return Ok(());
        }
    }
    emit_alloc_mixed_cell(local, module);
    emit_store_value_cell(&format!("${}", local), expr, module)?;
    let kind = value_cell_kind_for_expr(expr, module);
    module.set_mixed_value_cell_kind(local, kind);
    Ok(())
}

pub(in crate::codegen::wasm) fn emit_mixed_value_to_stack(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if matches!(expr.kind, ExprKind::Null) {
        emit_null_mixed_value_to_stack(module);
        return Ok(());
    }
    if let ExprKind::FunctionCall { name, args } = &expr.kind {
        if module.has_function(name) && module.function_return_kind(name) == Some(ValueKind::Mixed) {
            emit_user_function_args(expr, name, args, module)?;
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(name)));
            return Ok(());
        }
    }
    if let ExprKind::Variable(name) = &expr.kind {
        if module.local_kind(name) == Some(LocalKind::Mixed) {
            module.body().line(&format!("local.get ${}", name));
            return Ok(());
        }
    }
    let temp = module
        .next_label("mixed_return_value")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(temp.clone());
    emit_mixed_arg_assign(&temp, expr, module)?;
    module.body().line(&format!("local.get ${}", temp));
    Ok(())
}

pub(in crate::codegen::wasm) fn emit_value_array_arg_assign(
    local: &str,
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => emit_value_array_items_assign(local, items, module),
        ExprKind::ArrayLiteralAssoc(items) => emit_assoc_array_items_assign(local, items, module),
        ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
            match module.array_layout(source) {
                ArrayLayout::Assoc => emit_assoc_array_copy_assign(local, source, module),
                ArrayLayout::Value => emit_value_array_copy_assign(local, source, module),
                ArrayLayout::CompactInt => emit_compact_array_to_value_array_assign(local, source, module),
            }
        }
        ExprKind::FunctionCall { name, .. }
            if module.has_function(name)
                && module.function_return_kind(name) == Some(ValueKind::Array) =>
        {
            let temp = module
                .next_label("array_arg_return")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, expr, module)?;
            match module.array_layout(&temp) {
                ArrayLayout::Assoc => emit_assoc_array_copy_assign(local, &temp, module),
                ArrayLayout::Value => emit_value_array_copy_assign(local, &temp, module),
                ArrayLayout::CompactInt => emit_compact_array_to_value_array_assign(local, &temp, module),
            }
        }
        _ => Err(array_unsupported(expr)),
    }
}

pub(in crate::codegen::wasm) fn emit_compact_array_to_value_array_assign(
    name: &str,
    source: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(len) = module.array_length(source) {
        module.set_array_length(name, len);
    }
    module.set_array_layout(name, ArrayLayout::Value);
    let source_ptr = module.next_label("compact_to_value_source_ptr");
    let source_len = module.next_label("compact_to_value_source_len");
    let index = module.next_label("compact_to_value_index");
    let cell = module.next_label("compact_to_value_cell");
    let done_label = module.next_label("compact_to_value_done");
    let loop_label = module.next_label("compact_to_value_loop");
    for local in [&source_ptr, &source_len, &index, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.set {}", source_ptr));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", source_len));
    emit_release_current_value_array(name, module);
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", source_len));
    module
        .body()
        .line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get {}", source_len));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(name, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("call $__rt_value_store_int");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(in crate::codegen::wasm) fn emit_match_chain(
    subject_local: &str,
    arms: &[(Vec<Expr>, Expr)],
    default: Option<&Expr>,
    result_kind: ValueKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some((conditions, value)) = arms.first() {
        emit_match_condition(subject_local, conditions, module)?;
        module
            .body()
            .open(&format!("if (result {})", wasm_result_type(result_kind)));
        emit_value_as_kind(value, result_kind, module)?;
        module.body().line("else");
        emit_match_chain(subject_local, &arms[1..], default, result_kind, module)?;
        module.body().close("end");
    } else if let Some(default) = default {
        emit_value_as_kind(default, result_kind, module)?;
    } else {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            "wasm32-web match without default can fail at runtime and is not supported yet",
        ));
    }
    Ok(())
}

pub(in crate::codegen::wasm) fn wasm_result_type(kind: ValueKind) -> &'static str {
    wasm_value_type(kind)
}

pub(in crate::codegen::wasm) fn emit_value_as_kind(
    expr: &Expr,
    kind: ValueKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match kind {
        ValueKind::Int => require_int(expr, module),
        ValueKind::Float => require_float(expr, module),
        ValueKind::Bool => emit_condition(expr, module),
        ValueKind::Str => emit_string_value_to_stack(expr, module),
        ValueKind::Array => emit_array_value_to_stack(expr, module),
        ValueKind::Object => {
            if emit_expr(expr, module)? != ValueKind::Object {
                return Err(CompileError::new(expr.span, "wasm32-web expected object value"));
            }
            Ok(())
        }
        ValueKind::Mixed => emit_mixed_value_to_stack(expr, module),
        ValueKind::Null | ValueKind::Never => unreachable!("unsupported scalar result kind"),
    }
}

pub(in crate::codegen::wasm) fn emit_value_as_local_kind(
    expr: &Expr,
    kind: LocalKind,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match kind {
        LocalKind::I64 => require_int(expr, module),
        LocalKind::F64 => require_float(expr, module),
        LocalKind::I32 => emit_condition(expr, module),
        LocalKind::Str => emit_string_value_to_stack(expr, module),
        LocalKind::Array => Err(array_unsupported(expr)),
        LocalKind::Object => {
            if emit_expr(expr, module)? != ValueKind::Object {
                return Err(CompileError::new(expr.span, "wasm32-web expected object value"));
            }
            Ok(())
        }
        LocalKind::Mixed => Err(CompileError::new(
            expr.span,
            "wasm32-web mixed values are not supported in this expression yet",
        )),
        LocalKind::Callable => Err(CompileError::new(
            expr.span,
            "wasm32-web callable values require callable runtime support",
        )),
    }
}

pub(in crate::codegen::wasm) fn emit_match_condition(
    subject_local: &str,
    conditions: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if conditions.is_empty() {
        module.body().line("i32.const 1");
        return Ok(());
    }
    for condition in conditions {
        module.body().line(&format!("local.get ${}", subject_local));
        require_int(condition, module)?;
        module.body().line("i64.eq");
    }
    for _ in 1..conditions.len() {
        module.body().line("i32.or");
    }
    Ok(())
}
