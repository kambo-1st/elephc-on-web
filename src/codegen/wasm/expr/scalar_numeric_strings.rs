//! Purpose:
//! Lowers PHP numeric-string operands for wasm32-web scalar arithmetic.
//! Keeps static and runtime string-to-number coercion helpers out of the main scalar operator file.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::scalar_ops` through the wasm expression module.
//!
//! Key details:
//! - Preserves PHP leading-numeric string behavior and traps/rejects unsupported nonnumeric paths.
//! - Emits integer or float operands according to the operator's PHP-compatible numeric mode.

use super::*;

pub(in crate::codegen::wasm) fn emit_static_string_numeric_binary(
    expr: &Expr,
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    let left_string = static_string_value(left, module);
    let right_string = static_string_value(right, module);
    if left_string.is_none() && right_string.is_none() {
        return Ok(None);
    }
    let integer_op = matches!(
        op,
        BinOp::Mod | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight
    );
    let left_float = left_string
        .as_deref()
        .and_then(php_leading_numeric_string_value)
        .map(|(_, is_float)| is_float)
        .unwrap_or_else(|| expression_is_floaty(left, module));
    let right_float = right_string
        .as_deref()
        .and_then(php_leading_numeric_string_value)
        .map(|(_, is_float)| is_float)
        .unwrap_or_else(|| expression_is_floaty(right, module));
    let use_float = !integer_op
        && (matches!(op, BinOp::Div | BinOp::Pow) || left_float || right_float);
    emit_static_string_numeric_operand(left, use_float, module)?;
    emit_static_string_numeric_operand(right, use_float, module)?;
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
                "wasm32-web static string numeric operations currently support +, -, *, /, %, **, &, |, ^, <<, and >>",
            ))
        }
    };
    module.body().line(instr);
    Ok(Some(if use_float { ValueKind::Float } else { ValueKind::Int }))
}

pub(in crate::codegen::wasm) fn emit_static_string_numeric_operand(
    expr: &Expr,
    use_float: bool,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(value) = static_string_value(expr, module) {
        let Some((number, _)) = php_leading_numeric_string_value(&value) else {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web numeric string operation requires a leading-numeric string",
            ));
        };
        if use_float {
            module.body().line(&format!("f64.const {}", number));
        } else {
            module.body().line(&format!("i64.const {}", number.trunc() as i64));
        }
        return Ok(());
    }
    if use_float {
        require_float(expr, module)
    } else {
        require_int(expr, module)
    }
}

pub(in crate::codegen::wasm) fn emit_runtime_string_numeric_binary(
    expr: &Expr,
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    let left_runtime_string = static_string_value(left, module).is_none() && expression_is_stringy(left, module);
    let right_runtime_string = static_string_value(right, module).is_none() && expression_is_stringy(right, module);
    if !left_runtime_string && !right_runtime_string {
        return Ok(None);
    }
    let integer_op = matches!(
        op,
        BinOp::Mod | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight
    );
    let use_float = !integer_op;
    emit_runtime_string_numeric_operand(left, use_float, module)?;
    emit_runtime_string_numeric_operand(right, use_float, module)?;
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
                "wasm32-web runtime string numeric operations currently support +, -, *, /, %, **, &, |, ^, <<, and >>",
            ))
        }
    };
    module.body().line(instr);
    Ok(Some(if use_float { ValueKind::Float } else { ValueKind::Int }))
}

pub(in crate::codegen::wasm) fn emit_runtime_string_numeric_operand(
    expr: &Expr,
    use_float: bool,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if static_string_value(expr, module).is_none() {
        if let Some(var) = runtime_string_arg_or_materialize(expr, "runtime_numeric_string_operand", module)? {
            let number = module.next_label("runtime_numeric_string_number");
            module.declare_f64_local(number.trim_start_matches('$').to_string());
            emit_runtime_leading_numeric_string_value_checked(&var, &number, module);
            module.body().line(&format!("local.get {}", number));
            if !use_float {
                module.body().line("i64.trunc_f64_s");
            }
            return Ok(());
        }
    }
    emit_static_string_numeric_operand(expr, use_float, module)
}
