//! Purpose:
//! Lowers wasm32-web enum case expressions into singleton object pointers.
//! Keeps enum-specific object allocation separate from general class object lowering.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::dispatch::emit_expr()`.
//!
//! Key details:
//! - Enum cases use object payloads with the normal wasm object class-id header.
//! - Case globals are lazy initialized so repeated `Enum::Case` reads preserve PHP identity.

use super::*;
use crate::codegen::wasm::module::{EnumCaseBackingValue, ObjectPropertyKind};

const WASM_OBJECT_HEAP_KIND: i32 = 4;

pub(super) fn emit_enum_case_expr(
    expr: &Expr,
    receiver: &StaticReceiver,
    case_name: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let class_name = module.enum_case_class(receiver, case_name).ok_or_else(|| {
        CompileError::new(expr.span, "wasm32-web enum case is not supported yet")
    })?;
    let class_info = module.object_class(&class_name).cloned().ok_or_else(|| {
        CompileError::new(expr.span, "wasm32-web enum case requires object metadata")
    })?;
    let global = module.enum_case_global(&class_name, case_name);
    module.body().line(&format!("global.get ${}", global));
    module.body().line("i32.eqz");
    module.body().open("if");
    let object_size = 8 + class_info.properties.len() * 24;
    module.body().line(&format!("i32.const {}", object_size));
    module.body().line("call $__rt_alloc_bytes");
    module.body().line(&format!("i32.const {}", WASM_OBJECT_HEAP_KIND));
    module.body().line("call $__rt_heap_set_kind");
    module.body().line(&format!("global.set ${}", global));
    module.body().line(&format!("global.get ${}", global));
    module.body().line(&format!("i64.const {}", class_info.class_id));
    module.body().line("i64.store");
    if let Some(property) = class_info.properties.iter().find(|property| property.name == "value") {
        let backing_value = module.enum_case_backing_value(receiver, case_name).ok_or_else(|| {
            CompileError::new(expr.span, "wasm32-web backed enum case requires a backing value")
        })?;
        match (property.kind, backing_value) {
            (ObjectPropertyKind::Int, EnumCaseBackingValue::Int(value)) => {
                module.body().line(&format!("global.get ${}", global));
                module.body().line(&format!("i32.const {}", property.offset));
                module.body().line("i32.add");
                module.body().line(&format!("i64.const {}", value));
                module.body().line("i64.store");
            }
            (ObjectPropertyKind::Str, EnumCaseBackingValue::Str(value)) => {
                let (ptr, len) = module.intern_string(&value);
                module.body().line(&format!("global.get ${}", global));
                module.body().line(&format!("i32.const {}", property.offset));
                module.body().line("i32.add");
                module.body().line(&format!("i32.const {}", ptr));
                module.body().line("i32.store");
                module.body().line(&format!("global.get ${}", global));
                module.body().line(&format!("i32.const {}", property.offset + 8));
                module.body().line("i32.add");
                module.body().line(&format!("i32.const {}", len));
                module.body().line("i32.store");
            }
            _ => {
                return Err(CompileError::new(
                    expr.span,
                    "wasm32-web backed enum case metadata does not match object layout",
                ));
            }
        }
    }
    module.body().close("end");
    module.body().line(&format!("global.get ${}", global));
    Ok(ValueKind::Object)
}

pub(super) fn emit_backed_enum_lookup_call(
    expr: &Expr,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    if !method.eq_ignore_ascii_case("from") && !method.eq_ignore_ascii_case("tryFrom") {
        return Ok(None);
    }
    let class_name = module.class_name_for_receiver(receiver).ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web backed enum lookup requires enum metadata",
        )
    })?;
    if module.enum_case_names(&class_name).is_none() {
        return Ok(None);
    }
    if args.len() != 1 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web backed enum lookup expects exactly one argument",
        ));
    }
    let backing_value = static_backed_enum_lookup_value(&args[0], module).ok_or_else(|| {
        CompileError::new(
            args[0].span,
            "wasm32-web backed enum lookup requires a static int or string value",
        )
    })?;
    let Some(case_name) = module.enum_case_name_for_backing_value(&class_name, &backing_value)
    else {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web backed enum lookup requires a statically matching case value",
        ));
    };
    let case_expr = Expr::new(
        ExprKind::ScopedConstantAccess {
            receiver: receiver.clone(),
            name: case_name.clone(),
        },
        expr.span,
    );
    emit_enum_case_expr(&case_expr, receiver, &case_name, module)?;
    Ok(Some(ValueKind::Object))
}

fn static_backed_enum_lookup_value(
    expr: &Expr,
    module: &WasmModule,
) -> Option<EnumCaseBackingValue> {
    if let Some(value) = static_or_module_const_int_value(expr, module) {
        return Some(EnumCaseBackingValue::Int(value));
    }
    static_string_value(expr, module).map(EnumCaseBackingValue::Str)
}

pub(super) fn emit_enum_cases_array_assign(
    name: &str,
    expr: &Expr,
    receiver: &StaticReceiver,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if !args.is_empty() {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web enum cases() expects no arguments",
        ));
    }
    let class_name = module.class_name_for_receiver(receiver).ok_or_else(|| {
        CompileError::new(expr.span, "wasm32-web enum cases() requires enum metadata")
    })?;
    let cases = module.enum_case_names(&class_name).ok_or_else(|| {
        CompileError::new(expr.span, "wasm32-web enum cases() requires enum metadata")
    })?;
    emit_release_current_value_array(name, module);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_length(name, cases.len());
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, None);
    module.set_array_nested_value_metadata(name, None);
    module.set_array_object_classes(
        name,
        Some(cases.iter().map(|_| Some(class_name.clone())).collect()),
    );
    module.body().line(&format!("i32.const {}", cases.len()));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", cases.len()));
    module.body().line(&format!("local.set ${}_len", name));
    for (index, case_name) in cases.iter().enumerate() {
        let cell = module.next_label("enum_cases_cell");
        module.declare_i32_local(cell.trim_start_matches('$').to_string());
        module.body().line(&format!("local.get ${}_ptr", name));
        module
            .body()
            .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", cell));
        module.body().line(&format!("local.get {}", cell));
        let case_expr = Expr::new(
            ExprKind::ScopedConstantAccess {
                receiver: receiver.clone(),
                name: case_name.clone(),
            },
            expr.span,
        );
        emit_enum_case_expr(&case_expr, receiver, case_name, module)?;
        module.body().line("call $__rt_value_store_object");
    }
    Ok(())
}

pub(super) fn static_enum_cases_len(expr: &Expr, module: &WasmModule) -> Option<usize> {
    let ExprKind::StaticMethodCall {
        receiver,
        method,
        args,
    } = &expr.kind
    else {
        return None;
    };
    if !method.eq_ignore_ascii_case("cases") || !args.is_empty() {
        return None;
    }
    let class_name = module.class_name_for_receiver(receiver)?;
    module.enum_case_names(&class_name).map(|cases| cases.len())
}

pub(super) fn emit_enum_case_array_identity_comparison(
    left: &Expr,
    right: &Expr,
    op: &BinOp,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !matches!(op, BinOp::StrictEq | BinOp::StrictNotEq) {
        return Ok(false);
    }
    if emit_enum_case_array_identity_comparison_ordered(left, right, op, module)? {
        return Ok(true);
    }
    emit_enum_case_array_identity_comparison_ordered(right, left, op, module)
}

fn emit_enum_case_array_identity_comparison_ordered(
    array_access: &Expr,
    enum_case: &Expr,
    op: &BinOp,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let ExprKind::ArrayAccess { array, index } = &array_access.kind else {
        return Ok(false);
    };
    let ExprKind::ScopedConstantAccess { receiver, name } = &enum_case.kind else {
        return Ok(false);
    };
    if module.enum_case_class(receiver, name).is_none() {
        return Ok(false);
    }
    if emit_nested_enum_case_array_identity_comparison(
        array_access,
        array,
        receiver,
        name,
        op,
        module,
    )? {
        return Ok(true);
    }
    let ExprKind::Variable(array_name) = &array.kind else {
        return Ok(false);
    };
    if module.local_kind(array_name) != Some(LocalKind::Array)
        || module.array_layout(array_name) != ArrayLayout::Value
    {
        return Ok(false);
    }
    let Some(index) = static_or_const_int_value(index).and_then(|value| usize::try_from(value).ok()) else {
        return Ok(false);
    };
    if module.array_length(array_name).is_some_and(|len| index >= len) {
        return Ok(false);
    }
    let cell = module.next_label("enum_case_array_cell");
    module.declare_i32_local(cell.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", array_name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line("i32.const 6");
    module.body().line("i32.eq");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    emit_enum_case_expr(enum_case, receiver, name, module)?;
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

fn emit_nested_enum_case_array_identity_comparison(
    array_access: &Expr,
    array: &Expr,
    receiver: &StaticReceiver,
    name: &str,
    op: &BinOp,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !matches!(array.kind, ExprKind::ArrayAccess { .. }) {
        return Ok(false);
    }
    let Some(cell) = materialize_mixed_value_cell(array_access, module)? else {
        return Ok(false);
    };
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_OBJECT));
    module.body().line("i32.eq");
    module.body().open("if (result i32)");
    emit_mixed_i32_payload(&cell, 8, module);
    emit_enum_case_expr(array_access, receiver, name, module)?;
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
