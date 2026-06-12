//! Purpose:
//! Lowers scalar constant expressions for wasm32-web expression evaluation.
//! Keeps global and class constant value emission out of the main dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::emit_expr`
//!
//! Key details:
//! - String constants use the normal wasm string value ABI: pointer then length
//!   on the operand stack.

use super::*;

pub(super) fn emit_constant_expr(
    expr: &Expr,
    name: &Name,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if let Some(value) = module.constant_value(name) {
        return emit_constant_value(expr, value, module);
    }
    let value = const_int_value(name).ok_or_else(|| {
        CompileError::new(expr.span, "wasm32-web constant is not supported yet")
    })?;
    module.body().line(&format!("i64.const {}", value));
    Ok(ValueKind::Int)
}

pub(super) fn emit_constant_value(
    _expr: &Expr,
    value: ConstantValue,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    match value {
        ConstantValue::Int(value) => {
            module.body().line(&format!("i64.const {}", value));
            Ok(ValueKind::Int)
        }
        ConstantValue::Float(value) => {
            module.body().line(&format!("f64.const {}", value));
            Ok(ValueKind::Float)
        }
        ConstantValue::Bool(value) => {
            module.body().line(&format!("i32.const {}", i32::from(value)));
            Ok(ValueKind::Bool)
        }
        ConstantValue::Null => {
            module.body().line("i32.const 0");
            Ok(ValueKind::Null)
        }
        ConstantValue::Str(value) => {
            let (ptr, len) = module.intern_string(&value);
            module.body().line(&format!("i32.const {}", ptr));
            module.body().line(&format!("i32.const {}", len));
            Ok(ValueKind::Str)
        }
    }
}

pub(super) fn emit_scoped_constant_expr(
    expr: &Expr,
    receiver: &StaticReceiver,
    name: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let Some(value) = module.class_constant_value(receiver, name) else {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web class constant is not supported yet",
        ));
    };
    emit_constant_value(expr, value, module)
}
