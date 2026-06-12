//! Purpose:
//! Lowers PHP truthiness checks for wasm32-web expressions.
//! Keeps scalar, string, array, and mixed-cell boolean semantics out of the dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr`
//! - `crate::codegen::wasm::stmt`
//!
//! Key details:
//! - PHP treats only `""` and `"0"` as falsey strings, and mixed cells must
//!   inspect their runtime tag before reading the payload.

use super::*;
use super::array_search::{
    array_search_false_comparison, emit_array_search_false_comparison_bool,
};

pub(crate) fn emit_condition(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match &expr.kind {
        ExprKind::StringLiteral(value) => {
            module
                .body()
                .line(&format!("i32.const {}", i32::from(string_is_truthy(value))));
            return Ok(());
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Str) => {
            emit_string_local_truthiness(name, module);
            return Ok(());
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Mixed) => {
            emit_mixed_local_truthiness(name, module);
            return Ok(());
        }
        ExprKind::ConstRef(name) => {
            if let Some(value) = module.constant_value(name) {
                emit_constant_truthiness(value, module);
                return Ok(());
            }
        }
        ExprKind::ClassConstant { receiver } => {
            if let Some(class_name) = module.class_name_for_receiver(receiver) {
                module
                    .body()
                    .line(&format!("i32.const {}", i32::from(string_is_truthy(&class_name))));
                return Ok(());
            }
        }
        ExprKind::ScopedConstantAccess { receiver, name } => {
            if let Some(value) = module.class_constant_value(receiver, name) {
                emit_constant_truthiness(value, module);
                return Ok(());
            }
        }
        ExprKind::BinaryOp { left, op, right }
            if matches!(op, BinOp::StrictEq | BinOp::StrictNotEq) =>
        {
            if let Some(search) = array_search_false_comparison(left, right) {
                if emit_array_search_false_comparison_bool(
                    search,
                    matches!(op, BinOp::StrictNotEq),
                    module,
                )? {
                    return Ok(());
                }
            }
            if let Some(search) = array_search_false_comparison(right, left) {
                if emit_array_search_false_comparison_bool(
                    search,
                    matches!(op, BinOp::StrictNotEq),
                    module,
                )? {
                    return Ok(());
                }
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            if let Some(truthy) = static_array_offset_truthiness(array, index, module) {
                module.body().line(&format!("i32.const {}", i32::from(truthy)));
                return Ok(());
            }
        }
        _ => {}
    }

    if let Some(value) = static_string_value(expr, module) {
        module
            .body()
            .line(&format!("i32.const {}", i32::from(string_is_truthy(&value))));
        return Ok(());
    }
    if expression_is_stringy(expr, module) {
        let local = materialize_runtime_string_expr(expr, "truthy_string_arg", module)?;
        emit_string_local_truthiness(&local, module);
        return Ok(());
    }
    if expression_is_arrayy(expr, module) || expression_has_array_type(expr, module) {
        emit_array_truthiness(expr, module)?;
        return Ok(());
    }
    if let Some(cell) = materialize_mixed_value_cell(expr, module)? {
        emit_mixed_local_truthiness(&cell, module);
        return Ok(());
    }

    let kind = emit_expr(expr, module)?;
    match kind {
        ValueKind::Bool | ValueKind::Null => {}
        ValueKind::Int => {
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
        }
        ValueKind::Float => {
            module.body().line("f64.const 0");
            module.body().line("f64.ne");
        }
        ValueKind::Str => {
            let local = module
                .next_label("truthy_string_value")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(format!("{}_ptr", local));
            module.declare_i32_local(format!("{}_len", local));
            module.body().line(&format!("local.set ${}_len", local));
            module.body().line(&format!("local.set ${}_ptr", local));
            emit_string_local_truthiness(&local, module);
        }
        ValueKind::Array => {
            let len = module.next_label("array_truthy_len");
            module.declare_i32_local(len.trim_start_matches('$').to_string());
            module.body().line(&format!("local.set {}", len));
            module.body().line("drop");
            module.body().line(&format!("local.get {}", len));
            module.body().line("i32.const 0");
            module.body().line("i32.ne");
        }
        ValueKind::Object => {
            module.body().line("i32.const 0");
            module.body().line("i32.ne");
        }
        ValueKind::Mixed => {
            let local = module
                .next_label("mixed_truthy_value")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(local.clone());
            module.body().line(&format!("local.set ${}", local));
            emit_mixed_local_truthiness(&local, module);
        }
        ValueKind::Never => module.body().line("unreachable"),
    }
    Ok(())
}

fn emit_constant_truthiness(value: ConstantValue, module: &mut WasmModule) {
    let truthy = match value {
        ConstantValue::Int(value) => value != 0,
        ConstantValue::Float(value) => value != 0.0,
        ConstantValue::Bool(value) => value,
        ConstantValue::Str(value) => string_is_truthy(&value),
        ConstantValue::Null => false,
    };
    module.body().line(&format!("i32.const {}", i32::from(truthy)));
}

pub(crate) fn string_is_truthy(value: &str) -> bool {
    !value.is_empty() && value != "0"
}

pub(crate) fn emit_string_local_truthiness(name: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.const 1");
    module.body().line("i32.ne");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line("i32.ne");
    module.body().line("else");
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line("i32.load8_u");
    module.body().line("i32.const 48");
    module.body().line("i32.ne");
    module.body().close("end");
}

pub(crate) fn emit_mixed_local_truthiness(name: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get ${}", name));
    module.body().line("call $__rt_mixed_truthy");
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
                        .and_then(|value| static_array_item_truthiness(value, module))
                        .unwrap_or(false),
                )
            }
            ConstantArrayValue::Assoc(items) => {
                let key = static_truthiness_assoc_access_key(index, module)?;
                Some(
                    items
                        .iter()
                        .rev()
                        .find_map(|(candidate, value)| {
                            let candidate = static_truthiness_assoc_access_key(candidate, module)?;
                            (candidate == key).then(|| static_array_item_truthiness(value, module))
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
                    .and_then(|value| static_array_item_truthiness(value, module))
                    .unwrap_or(false),
            )
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            let key = static_truthiness_assoc_access_key(index, module)?;
            Some(
                items
                    .iter()
                    .rev()
                    .find_map(|(candidate, value)| {
                        let candidate = static_truthiness_assoc_access_key(candidate, module)?;
                        (candidate == key).then(|| static_array_item_truthiness(value, module))
                    })
                    .flatten()
                    .unwrap_or(false),
            )
        }
        _ => None,
    }
}

fn static_array_item_truthiness(value: &Expr, module: &WasmModule) -> Option<bool> {
    static_scalar_value(value, module)
        .map(|value| static_scalar_truthiness(&value))
        .or_else(|| static_value_cell_truthiness_for_filter(value))
}

fn static_truthiness_assoc_access_key(index: &Expr, module: &WasmModule) -> Option<AssocKeyValue> {
    static_assoc_access_key(index, module).or_else(|| match &index.kind {
        ExprKind::Variable(name) => module.string_static_value(name).map(AssocKeyValue::Str),
        _ => None,
    })
}
