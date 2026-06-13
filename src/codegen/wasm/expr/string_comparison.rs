//! Purpose:
//! Provides wasm32-web static string extraction, PHP string comparison, and numeric-string helpers.
//! Keeps string comparison/coercion support code out of the expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` scalar, string, array, and callable lowering modules.
//!
//! Key details:
//! - Helpers mirror PHP string equality, lexical comparison, and numeric-string coercion rules.

use super::*;

pub(in crate::codegen::wasm) fn static_string_value(expr: &Expr, module: &WasmModule) -> Option<String> {
    match &expr.kind {
        ExprKind::StringLiteral(value) => Some(value.clone()),
        ExprKind::ConstRef(name) => match module.constant_value(name)? {
            ConstantValue::Str(value) => Some(value),
            _ => None,
        },
        ExprKind::ClassConstant { receiver } => module.class_name_for_receiver(receiver),
        ExprKind::ScopedConstantAccess { receiver, name } => {
            match module.class_constant_value(receiver, name)? {
                ConstantValue::Str(value) => Some(value),
                _ => None,
            }
        }
        ExprKind::BinaryOp {
            left,
            op: BinOp::Concat,
            right,
        } => Some(format!(
            "{}{}",
            static_string_value(left, module)?,
            static_string_value(right, module)?
        )),
        ExprKind::Cast {
            target: CastType::String,
            expr,
        } => static_scalar_cast_string(expr, module),
        ExprKind::ArrayAccess { array, index } => {
            if matches!(array.kind, ExprKind::ConstRef(_)) {
                return match static_scalar_value(expr, module)? {
                    ConstantValue::Str(value) => Some(value),
                    _ => None,
                };
            }
            let value = static_string_value(array, module)?;
            let index = static_or_const_int_value(index)?;
            php_string_index(&value, index)
        }
        _ => None,
    }
}

pub(in crate::codegen::wasm) fn static_string_coercion_value(expr: &Expr, module: &WasmModule) -> Option<String> {
    static_string_value(expr, module).or_else(|| static_scalar_cast_string(expr, module))
}

pub(in crate::codegen::wasm) fn static_callback_function_name(expr: &Expr, module: &WasmModule) -> Option<String> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => {
            let [receiver, method] = items.as_slice() else {
                return None;
            };
            static_callable_array_function_name(receiver, method, module)
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            let receiver = static_callable_assoc_value(items, 0)?;
            let method = static_callable_assoc_value(items, 1)?;
            static_callable_array_function_name(receiver, method, module)
        }
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => Some(name.to_string()),
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            let class_name = module.class_name_for_receiver(receiver)?;
            let method_info = module.object_static_method_in_hierarchy(&class_name, method)?;
            module
                .object_member_is_accessible(&method_info.owner_class, &method_info.visibility)
                .then_some(method_info.symbol)
        }
        ExprKind::Variable(name) => module.callable_target(name).or_else(|| module.string_static_value(name)),
        ExprKind::FunctionCall { name, args } => module
            .function_static_string_return_for_call(name, args)
            .or_else(|| module.function_static_string_return(name))
            .or_else(|| module.function_callable_return_target(name.as_str())),
        ExprKind::BinaryOp {
            left,
            op: BinOp::Concat,
            right,
        } => Some(format!(
            "{}{}",
            static_callback_function_name(left, module)?,
            static_callback_function_name(right, module)?
        )),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_target = static_callback_function_name(then_expr, module)?;
            let else_target = static_callback_function_name(else_expr, module)?;
            then_target
                .eq_ignore_ascii_case(&else_target)
                .then_some(then_target)
        }
        _ => static_string_value(expr, module),
    }
}

fn static_callable_array_function_name(
    receiver: &Expr,
    method: &Expr,
    module: &WasmModule,
) -> Option<String> {
    let class_name = static_string_value(receiver, module)?;
    let method_name = static_string_value(method, module)?;
    let method_info = module.object_static_method_in_hierarchy(&class_name, &method_name)?;
    module
        .object_member_is_accessible(&method_info.owner_class, &method_info.visibility)
        .then_some(method_info.symbol)
}

fn static_callable_assoc_value(items: &[(Expr, Expr)], needle: i64) -> Option<&Expr> {
    items.iter().rev().find_map(|(key, value)| {
        matches!(key.kind, ExprKind::IntLiteral(key) if key == needle).then_some(value)
    })
}

pub(in crate::codegen::wasm) fn evaluated_static_callback_function_name(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<Option<String>, CompileError> {
    if let ExprKind::Ternary {
        condition,
        then_expr,
        else_expr,
    } = &expr.kind
    {
        let Some(then_target) = static_callback_function_name(then_expr, module) else {
            return Ok(None);
        };
        let Some(else_target) = static_callback_function_name(else_expr, module) else {
            return Ok(None);
        };
        if !then_target.eq_ignore_ascii_case(&else_target) {
            return Ok(None);
        }
        emit_condition(condition, module)?;
        module.body().line("drop");
        return Ok(Some(then_target));
    }
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return Ok(static_callback_function_name(expr, module));
    };
    let Some(callback) = module
        .function_static_string_return_for_call(name, args)
        .or_else(|| module.function_static_string_return(name))
        .or_else(|| module.function_callable_return_target(name.as_str()))
    else {
        return Ok(None);
    };
    let kind = emit_expr(expr, module)?;
    match kind {
        ValueKind::Str | ValueKind::Array => {
            module.body().line("drop");
            module.body().line("drop");
        }
        ValueKind::Callable => {
            module.body().line("drop");
        }
        _ => {}
    }
    Ok(Some(callback))
}

pub(in crate::codegen::wasm) fn php_string_index(value: &str, index: i64) -> Option<String> {
    let len = i64::try_from(value.len()).ok()?;
    let index = if index < 0 { len + index } else { index };
    if index < 0 || index >= len {
        return Some(String::new());
    }
    Some(char::from(value.as_bytes()[usize::try_from(index).ok()?]).to_string())
}

pub(in crate::codegen::wasm) fn static_scalar_cast_string(expr: &Expr, module: &WasmModule) -> Option<String> {
    match static_scalar_value(expr, module)? {
        ConstantValue::Int(value) => Some(value.to_string()),
        ConstantValue::Float(value) => Some(value.to_string()),
        ConstantValue::Bool(true) => Some("1".to_string()),
        ConstantValue::Bool(false) | ConstantValue::Null => Some(String::new()),
        ConstantValue::Str(value) => Some(value),
    }
}

pub(in crate::codegen::wasm) fn emit_string_ref(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if expression_is_stringy(expr, module) {
        return emit_string_value_to_stack(expr, module);
    }
    Err(CompileError::new(
        expr.span,
        "wasm32-web string comparison currently requires string values",
    ))
}

pub(in crate::codegen::wasm) fn emit_string_equality(
    expr: &Expr,
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if !expression_is_stringy(left, module) || !expression_is_stringy(right, module) {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web string comparison currently requires both operands to be strings",
        ));
    }

    let left_ptr = module.next_label("str_left_ptr").trim_start_matches('$').to_string();
    let left_len = module.next_label("str_left_len").trim_start_matches('$').to_string();
    let right_ptr = module.next_label("str_right_ptr").trim_start_matches('$').to_string();
    let right_len = module.next_label("str_right_len").trim_start_matches('$').to_string();
    let index = module.next_label("str_index").trim_start_matches('$').to_string();
    let result = module.next_label("str_eq").trim_start_matches('$').to_string();
    module.declare_i32_local(left_ptr.clone());
    module.declare_i32_local(left_len.clone());
    module.declare_i32_local(right_ptr.clone());
    module.declare_i32_local(right_len.clone());
    module.declare_i32_local(index.clone());
    module.declare_i32_local(result.clone());

    emit_string_ref(left, module)?;
    module.body().line(&format!("local.set ${}", left_len));
    module.body().line(&format!("local.set ${}", left_ptr));
    emit_string_ref(right, module)?;
    module.body().line(&format!("local.set ${}", right_len));
    module.body().line(&format!("local.set ${}", right_ptr));

    let done_label = module.next_label("str_cmp_done");
    let loop_label = module.next_label("str_cmp_loop");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}", result));
    module.body().line(&format!("local.get ${}", left_len));
    module.body().line(&format!("local.get ${}", right_len));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set ${}", result));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get ${}", index));
    module.body().line(&format!("local.get ${}", left_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}", left_ptr));
    module.body().line(&format!("local.get ${}", index));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.get ${}", right_ptr));
    module.body().line(&format!("local.get ${}", index));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}", result));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get ${}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set ${}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get ${}", result));
    if matches!(op, BinOp::NotEq | BinOp::StrictNotEq) {
        module.body().line("i32.eqz");
    }
    Ok(ValueKind::Bool)
}

pub(in crate::codegen::wasm) fn emit_runtime_php_string_comparison(
    expr: &Expr,
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let Some(left_var) = concat_string_arg_or_materialize(left, "cmp_left", module)? else {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web string comparison currently requires scalar string-coercible operands",
        ));
    };
    let Some(right_var) = concat_string_arg_or_materialize(right, "cmp_right", module)? else {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web string comparison currently requires scalar string-coercible operands",
        ));
    };

    let left_is_numeric = module.next_label("cmp_left_numeric");
    let right_is_numeric = module.next_label("cmp_right_numeric");
    let left_number = module.next_label("cmp_left_number");
    let right_number = module.next_label("cmp_right_number");
    for local in [&left_is_numeric, &right_is_numeric] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left_number, &right_number] {
        module.declare_f64_local(local.trim_start_matches('$').to_string());
    }

    emit_runtime_numeric_string_value(&left_var, &left_is_numeric, &left_number, module);
    emit_runtime_numeric_string_value(&right_var, &right_is_numeric, &right_number, module);
    module.body().line(&format!("local.get {}", left_is_numeric));
    module.body().line(&format!("local.get {}", right_is_numeric));
    module.body().line("i32.and");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", left_number));
    module.body().line(&format!("local.get {}", right_number));
    emit_f64_comparison_op(op, module);
    module.body().line("else");
    emit_runtime_lexical_string_comparison(&left_var, &right_var, op, module);
    module.body().close("end");
    Ok(ValueKind::Bool)
}

pub(in crate::codegen::wasm) fn emit_runtime_numeric_string_value(
    var: &str,
    is_numeric: &str,
    number: &str,
    module: &mut WasmModule,
) {
    let out_ptr = module.next_label("numeric_string_value_ptr");
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("call $host_numeric_string_value");
    module.body().line(&format!("local.set {}", is_numeric));
    module.body().line(&format!("local.get {}", is_numeric));
    module.body().open("if");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", number));
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn emit_runtime_leading_numeric_string_value(var: &str, number: &str, module: &mut WasmModule) {
    let out_ptr = module.next_label("leading_numeric_string_value_ptr");
    let matched = module.next_label("leading_numeric_string_value_matched");
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(matched.trim_start_matches('$').to_string());
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("call $host_leading_numeric_string_value");
    module.body().line(&format!("local.set {}", matched));
    module.body().line("f64.const 0");
    module.body().line(&format!("local.set {}", number));
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", number));
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn emit_runtime_leading_numeric_string_value_checked(var: &str, number: &str, module: &mut WasmModule) {
    let out_ptr = module.next_label("checked_leading_numeric_string_value_ptr");
    let matched = module.next_label("checked_leading_numeric_string_value_matched");
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(matched.trim_start_matches('$').to_string());
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("call $host_leading_numeric_string_value");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", number));
}

pub(in crate::codegen::wasm) fn emit_mixed_string_leading_numeric_value(name: &str, number: &str, module: &mut WasmModule) {
    let out_ptr = module.next_label("mixed_string_numeric_value_ptr");
    let matched = module.next_label("mixed_string_numeric_value_matched");
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(matched.trim_start_matches('$').to_string());
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get ${}", name));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.get ${}", name));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("call $host_leading_numeric_string_value");
    module.body().line(&format!("local.set {}", matched));
    module.body().line("f64.const 0");
    module.body().line(&format!("local.set {}", number));
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", number));
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn emit_value_cell_string_leading_numeric_value(cell: &str, number: &str, module: &mut WasmModule) {
    let out_ptr = module.next_label("value_cell_string_numeric_value_ptr");
    let matched = module.next_label("value_cell_string_numeric_value_matched");
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(matched.trim_start_matches('$').to_string());
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("call $host_leading_numeric_string_value");
    module.body().line(&format!("local.set {}", matched));
    module.body().line("f64.const 0");
    module.body().line(&format!("local.set {}", number));
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", number));
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn emit_mixed_string_leading_numeric_value_checked(name: &str, number: &str, module: &mut WasmModule) {
    let out_ptr = module.next_label("checked_mixed_string_numeric_value_ptr");
    let matched = module.next_label("checked_mixed_string_numeric_value_matched");
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(matched.trim_start_matches('$').to_string());
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get ${}", name));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.get ${}", name));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("call $host_leading_numeric_string_value");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("local.get {}", matched));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", number));
}

pub(in crate::codegen::wasm) fn emit_runtime_lexical_string_comparison(
    left_var: &str,
    right_var: &str,
    op: &BinOp,
    module: &mut WasmModule,
) {
    let index = module.next_label("lex_cmp_index");
    let result = module.next_label("lex_cmp_result");
    let left_byte = module.next_label("lex_cmp_left_byte");
    let right_byte = module.next_label("lex_cmp_right_byte");
    let loop_label = module.next_label("lex_cmp_loop");
    let done_label = module.next_label("lex_cmp_done");
    for local in [&index, &result, &left_byte, &right_byte] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", result));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", left_var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", right_var));
    module.body().line("i32.ge_u");
    module.body().line("i32.or");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_string_byte(left_var, &index, module);
    module.body().line(&format!("local.set {}", left_byte));
    emit_load_string_byte(right_var, &index, module);
    module.body().line(&format!("local.set {}", right_byte));
    module.body().line(&format!("local.get {}", left_byte));
    module.body().line(&format!("local.get {}", right_byte));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line(&format!("local.get {}", left_byte));
    module.body().line(&format!("local.get {}", right_byte));
    module.body().line("i32.lt_u");
    module.body().open("if");
    module.body().line("i32.const -1");
    module.body().line(&format!("local.set {}", result));
    module.body().line("else");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", result));
    module.body().close("end");
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", result));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_len", left_var));
    module.body().line(&format!("local.get ${}_len", right_var));
    module.body().line("i32.lt_u");
    module.body().open("if");
    module.body().line("i32.const -1");
    module.body().line(&format!("local.set {}", result));
    module.body().line("else");
    module.body().line(&format!("local.get ${}_len", left_var));
    module.body().line(&format!("local.get ${}_len", right_var));
    module.body().line("i32.gt_u");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", result));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", result));
    module.body().line("i32.const 0");
    emit_i32_comparison_op(op, module);
}

pub(in crate::codegen::wasm) fn emit_f64_comparison_op(op: &BinOp, module: &mut WasmModule) {
    let instr = match op {
        BinOp::Eq => "f64.eq",
        BinOp::NotEq => "f64.ne",
        BinOp::Lt => "f64.lt",
        BinOp::Gt => "f64.gt",
        BinOp::LtEq => "f64.le",
        BinOp::GtEq => "f64.ge",
        _ => unreachable!(),
    };
    module.body().line(instr);
}

pub(in crate::codegen::wasm) fn emit_i32_comparison_op(op: &BinOp, module: &mut WasmModule) {
    let instr = match op {
        BinOp::Eq => "i32.eq",
        BinOp::NotEq => "i32.ne",
        BinOp::Lt => "i32.lt_s",
        BinOp::Gt => "i32.gt_s",
        BinOp::LtEq => "i32.le_s",
        BinOp::GtEq => "i32.ge_s",
        _ => unreachable!(),
    };
    module.body().line(instr);
}
