//! Purpose:
//! Lowers the first wasm32-web object operations using the native fixed object layout.
//! Owns object allocation, local assignment, and static public property reads.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::dispatch`
//! - `crate::codegen::wasm::expr::assign_value`
//!
//! Key details:
//! - Object values are i32 heap payload pointers in WAT.
//! - Payload layout is `[class_id: i64] + 16-byte property slots`, matching native codegen.
//! - Direct methods and constructors are fixed-layout calls; runtime dynamic property names branch
//!   over fixed metadata when the receiver class is known. Broader dynamic dispatch still errors.

use super::*;
use crate::codegen::wasm::expr::array_value_cells::emit_store_emitted_value_kind;
use crate::codegen::wasm::module::{
    method_call_return_key, static_method_call_return_key, ObjectClassInfo, ObjectPropertyInfo,
    ObjectPropertyKind, ObjectStaticPropertyInfo, EnumCaseBackingValue,
};
use crate::names::php_symbol_key;
use crate::parser::ast::{InstanceOfTarget, StaticReceiver, Stmt, StmtKind};

const WASM_OBJECT_HEAP_KIND: i32 = 4;

pub(in crate::codegen::wasm) fn emit_new_object_expr(
    expr: &Expr,
    class_name: &Name,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let class_info = module.object_class(class_name.as_str()).cloned().ok_or_else(|| {
        CompileError::new(expr.span, "wasm32-web object allocation requires a declared class")
    })?;
    let constructor = module.object_constructor(class_name.as_str());
    if !args.is_empty() && constructor.is_none() {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web object constructor arguments require a supported constructor",
        ));
    }
    if module.object_has_constructor_in_hierarchy(class_name.as_str()) && constructor.is_none() {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web object constructors require a supported public fixed constructor",
        ));
    }
    if let Some(constructor) = constructor.as_ref() {
        if !module.object_member_is_accessible(&constructor.owner_class, &constructor.visibility) {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web object constructors require visible fixed constructor metadata",
            ));
        }
    }
    emit_alloc_object(&class_info, expr.span, module)?;
    if let Some(constructor) = constructor {
        let object_local = module
            .next_label("constructed_object")
            .trim_start_matches('$')
            .to_string();
        module.declare_object_local(object_local.clone());
        module.body().line(&format!("local.set ${}", object_local));
        module.set_object_class_for_local(&object_local, Some(class_info.name.clone()));
        let object_arg = Expr {
            kind: ExprKind::Variable(object_local.clone()),
            span: expr.span,
        };
        let mut call_args = Vec::with_capacity(args.len() + 1);
        call_args.push(object_arg);
        call_args.extend(args.iter().cloned());
        emit_user_function_args(expr, &constructor.symbol, &call_args, module)?;
        module
            .body()
            .line(&format!("call ${}", wasm_function_name(&constructor.symbol)));
        emit_drop_value_kind(constructor.return_kind, module);
        module.body().line(&format!("local.get ${}", object_local));
    }
    Ok(ValueKind::Object)
}

pub(in crate::codegen::wasm) fn emit_new_scoped_object_expr(
    expr: &Expr,
    receiver: &StaticReceiver,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let class_name = module.class_name_for_receiver(receiver).ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web scoped object allocation requires self::/parent:: context; new static() needs late-static binding metadata",
        )
    })?;
    let scoped_name = Name::unqualified(class_name);
    emit_new_object_expr(expr, &scoped_name, args, module)
}

pub(in crate::codegen::wasm) fn emit_object_assign(
    name: &str,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let class_name = object_class_name_for_expr(value, module);
    if class_name.is_none() && !dynamic_object_receiver_candidate(value, module) {
        return Err(CompileError::new(
            value.span,
            "wasm32-web object assignment requires a statically known object class",
        ));
    }
    let kind = emit_expr(value, module)?;
    if kind != ValueKind::Object {
        return Err(CompileError::new(
            value.span,
            "wasm32-web object assignment expected an object value",
        ));
    }
    module.body().line(&format!("local.set ${}", name));
    module.set_object_class_for_local(name, class_name);
    Ok(())
}

pub(in crate::codegen::wasm) fn emit_property_access_expr(
    expr: &Expr,
    object: &Expr,
    property: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if object_class_name_for_expr(object, module).is_none() {
        if let Some(cell) = materialize_mixed_value_cell(object, module)? {
            return emit_mixed_object_scalar_property_access(expr, &cell, property, module);
        }
    }
    if object_receiver_needs_runtime_class_id(object, module) {
        return emit_dynamic_object_scalar_property_access(expr, object, property, module);
    }
    let class_name = object_class_name_for_expr(object, module).ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web property reads require a statically known object class",
        )
    })?;
    let class_info = module.object_class(&class_name).cloned().ok_or_else(|| {
        CompileError::new(expr.span, "wasm32-web property read requires a declared class")
    })?;
    let property_info = class_info
        .properties
        .iter()
        .find(|candidate| candidate.name == property)
        .cloned()
        .ok_or_else(|| {
            CompileError::new(
                expr.span,
                "wasm32-web property read requires a supported public fixed property",
            )
        })?;
    if !module.object_member_is_accessible(&property_info.owner_class, &property_info.visibility) {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web property read requires visible fixed property metadata",
        ));
    }
    let object_kind = emit_object_receiver_for_property_read(object, module)?;
    if object_kind != ValueKind::Object {
        return Err(CompileError::new(
            object.span,
            "wasm32-web property read expected an object receiver",
        ));
    }
    emit_load_property(&property_info, module)
}

fn emit_object_receiver_for_property_read(
    object: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if let ExprKind::ArrayAccess { array, index } = &object.kind {
        if let ExprKind::Variable(name) = &array.kind {
            if module.local_kind(name) == Some(LocalKind::Array)
                && module.array_layout(name) == ArrayLayout::Value
            {
                let Some(index) = static_or_const_int_value(index)
                    .and_then(|value| usize::try_from(value).ok())
                else {
                    return Err(CompileError::new(
                        object.span,
                        "wasm32-web object array-cell property reads require a static integer index",
                    ));
                };
                if module.array_object_class(name, index).is_some() {
                    let Some(len) = module.array_length(name) else {
                        return Err(CompileError::new(
                            object.span,
                            "wasm32-web object array-cell property reads require a known array length",
                        ));
                    };
                    if index >= len {
                        return Err(CompileError::new(
                            object.span,
                            "wasm32-web object array-cell property reads do not support missing indexes yet",
                        ));
                    }
                    emit_value_array_static_payload_addr(name, index, module);
                    module.body().line("i32.load");
                    return Ok(ValueKind::Object);
                }
            }
        }
    }
    emit_expr(object, module)
}

pub(in crate::codegen::wasm) fn emit_nullable_exact_object_property_access(
    expr: &Expr,
    object: &Expr,
    property: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let class_name = object_class_name_for_expr(object, module).ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web nullable property reads require object return metadata",
        )
    })?;
    let class_info = module.object_class(&class_name).cloned().ok_or_else(|| {
        CompileError::new(expr.span, "wasm32-web nullable property read requires a declared class")
    })?;
    let property_info = class_info
        .properties
        .iter()
        .find(|candidate| candidate.name == property)
        .cloned()
        .ok_or_else(|| {
            CompileError::new(
                expr.span,
                "wasm32-web nullable property read requires supported fixed property metadata",
            )
        })?;
    if !module.object_member_is_accessible(&property_info.owner_class, &property_info.visibility) {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web nullable property read requires visible fixed property metadata",
        ));
    }
    let result = module
        .next_label("nullsafe_exact_object_property_result")
        .trim_start_matches('$')
        .to_string();
    let object_local = module
        .next_label("nullsafe_exact_object_property_object")
        .trim_start_matches('$')
        .to_string();
    module.declare_mixed_local(result.clone());
    module.declare_object_local(object_local.clone());
    module.set_object_class_for_local(&object_local, Some(class_name));
    emit_alloc_mixed_cell(&result, module);
    let object_kind = emit_expr(object, module)?;
    if !matches!(object_kind, ValueKind::Object | ValueKind::Null) {
        return Err(CompileError::new(
            object.span,
            "wasm32-web nullable property read expected object-or-null receiver",
        ));
    }
    module.body().line(&format!("local.set ${}", object_local));
    module.body().line(&format!("local.get ${}", object_local));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line(&format!("local.get ${}", result));
    module.body().line("call $__rt_value_store_null");
    module.body().line("else");
    module.body().line(&format!("local.get ${}", object_local));
    let property_kind = emit_load_property(&property_info, module)?;
    emit_store_emitted_value_kind(
        &format!("${}", result),
        property_kind,
        "nullsafe_exact_object_property",
        module,
    )?;
    module.body().close("end");
    module.body().line(&format!("local.get ${}", result));
    Ok(ValueKind::Mixed)
}

fn emit_runtime_dynamic_property_access_expr(
    expr: &Expr,
    object: &Expr,
    property: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if object_receiver_needs_runtime_class_id(object, module) {
        return emit_runtime_dynamic_object_property_access_by_class_expr(
            expr, object, property, module,
        );
    }
    let class_name = object_class_name_for_expr(object, module).ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web runtime dynamic property reads require a statically known object class",
        )
    })?;
    let class_info = module.object_class(&class_name).cloned().ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web runtime dynamic property read requires a declared class",
        )
    })?;
    let mut properties = Vec::new();
    let mut expected_kind = None;
    for property_info in class_info.properties.iter() {
        if !module.object_member_is_accessible(&property_info.owner_class, &property_info.visibility)
        {
            continue;
        }
        if expected_kind.is_some_and(|kind| kind != property_info.kind) {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web runtime dynamic property reads require consistent property metadata",
            ));
        }
        expected_kind = Some(property_info.kind);
        properties.push(property_info.clone());
    }
    let Some(kind) = expected_kind else {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web runtime dynamic property reads require visible fixed property metadata",
        ));
    };
    if emit_expr(object, module)? != ValueKind::Object {
        return Err(CompileError::new(
            object.span,
            "wasm32-web runtime dynamic property read expected an object receiver",
        ));
    }
    let object_local = module
        .next_label("dynamic_property_object")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(object_local.clone());
    module.body().line(&format!("local.set ${}", object_local));
    let property_local = materialize_runtime_string_expr(property, "dynamic_property_name", module)?;
    emit_runtime_dynamic_property_read_branch(
        &object_local,
        &property_local,
        kind,
        &properties,
        0,
        module,
    )?;
    Ok(value_kind_for_object_property_kind(kind))
}

fn emit_runtime_dynamic_object_property_access_by_class_expr(
    expr: &Expr,
    object: &Expr,
    property: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let receiver_type = dynamic_object_receiver_declared_type(object, module);
    let candidates = dynamic_object_property_candidates(expr, receiver_type.as_deref(), module)?;
    let properties = candidates
        .iter()
        .map(|(_, property_info)| property_info.clone())
        .collect::<Vec<_>>();
    let Some(kind) = consistent_property_kind(&properties) else {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web runtime dynamic property reads require consistent property metadata",
        ));
    };
    if emit_expr(object, module)? != ValueKind::Object {
        return Err(CompileError::new(
            object.span,
            "wasm32-web runtime dynamic property read expected an object receiver",
        ));
    }
    let object_local = module
        .next_label("dynamic_object_property_object")
        .trim_start_matches('$')
        .to_string();
    let class_id_local = module
        .next_label("dynamic_object_property_class_id")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(object_local.clone());
    module.declare_i64_local(class_id_local.clone());
    module.body().line(&format!("local.tee ${}", object_local));
    module.body().line("i64.load");
    module.body().line(&format!("local.set ${}", class_id_local));
    let property_local = materialize_runtime_string_expr(property, "dynamic_object_property_name", module)?;
    emit_dynamic_object_runtime_property_read_branch(
        &object_local,
        &class_id_local,
        &property_local,
        kind,
        &candidates,
        0,
        module,
    )?;
    Ok(value_kind_for_object_property_kind(kind))
}

pub(in crate::codegen::wasm) fn emit_nullsafe_mixed_object_property_access_expr(
    expr: &Expr,
    object: &Expr,
    property: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let Some(cell) = materialize_mixed_value_cell(object, module)? else {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web object property path is not supported yet",
        ));
    };
    emit_nullsafe_mixed_object_scalar_property_access(expr, &cell, property, module)
}

pub(in crate::codegen::wasm) fn emit_nullsafe_mixed_object_dynamic_property_access_expr(
    expr: &Expr,
    object: &Expr,
    property: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let Some(cell) = materialize_mixed_value_cell(object, module)? else {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web runtime dynamic nullsafe property reads require object/null runtime metadata",
        ));
    };
    let candidates = mixed_object_dynamic_property_candidates(expr, module)?;
    let result = module
        .next_label("nullsafe_dynamic_property_result")
        .trim_start_matches('$')
        .to_string();
    let object_local = module
        .next_label("nullsafe_dynamic_property_object")
        .trim_start_matches('$')
        .to_string();
    let class_id_local = module
        .next_label("nullsafe_dynamic_property_class_id")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(result.clone());
    module.declare_object_local(object_local.clone());
    module.declare_i64_local(class_id_local.clone());
    emit_alloc_mixed_cell(&result, module);
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("call $__rt_mixed_tag");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get ${}", result));
    module.body().line("call $__rt_value_store_null");
    module.body().line("else");
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("call $__rt_mixed_tag");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_OBJECT));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.tee ${}", object_local));
    module.body().line("i64.load");
    module.body().line(&format!("local.set ${}", class_id_local));
    let property_local =
        materialize_runtime_string_expr(property, "nullsafe_dynamic_property_name", module)?;
    emit_mixed_object_dynamic_property_branch(
        &result,
        &object_local,
        &class_id_local,
        &property_local,
        &candidates,
        0,
        module,
    )?;
    module.body().close("end");
    module.body().line(&format!("local.get ${}", result));
    Ok(ValueKind::Mixed)
}

pub(in crate::codegen::wasm) fn emit_nullsafe_mixed_object_method_call_expr(
    expr: &Expr,
    object: &Expr,
    method: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let Some(cell) = materialize_mixed_value_cell(object, module)? else {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web nullable nullsafe method calls require object/null runtime metadata",
        ));
    };
    emit_nullsafe_mixed_object_no_arg_method_call(expr, &cell, method, args, module)
}

pub(in crate::codegen::wasm) fn emit_dynamic_property_access_expr(
    expr: &Expr,
    object: &Expr,
    property: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if let Some(property_name) = static_object_property_name(property, module) {
        return emit_property_access_expr(expr, object, &property_name, module);
    }
    emit_runtime_dynamic_property_access_expr(expr, object, property, module)
}

pub(in crate::codegen::wasm) fn emit_object_property_isset_expr(
    expr: &Expr,
    object: &Expr,
    property: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let kind = emit_property_access_expr(expr, object, property, module)?;
    emit_loaded_property_isset(kind, module);
    Ok(ValueKind::Bool)
}

pub(in crate::codegen::wasm) fn emit_dynamic_object_property_isset_expr(
    expr: &Expr,
    object: &Expr,
    property: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if object_receiver_needs_runtime_class_id(object, module) {
        return emit_dynamic_object_property_isset_by_class_expr(expr, object, property, module);
    }
    if let Some(property_name) = static_object_property_name(property, module) {
        return emit_object_property_isset_expr(expr, object, &property_name, module);
    }
    emit_runtime_dynamic_property_isset_expr(expr, object, property, module)
}

pub(in crate::codegen::wasm) fn emit_nullsafe_mixed_object_dynamic_property_isset_expr(
    expr: &Expr,
    object: &Expr,
    property: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let Some(cell) = materialize_mixed_value_cell(object, module)? else {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web runtime dynamic nullsafe property isset requires object/null runtime metadata",
        ));
    };
    let candidates = mixed_object_dynamic_property_candidates(expr, module)?;
    let object_local = module
        .next_label("nullsafe_dynamic_property_isset_object")
        .trim_start_matches('$')
        .to_string();
    let class_id_local = module
        .next_label("nullsafe_dynamic_property_isset_class_id")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(object_local.clone());
    module.declare_i64_local(class_id_local.clone());
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("call $__rt_mixed_tag");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.eq");
    module.body().open("if (result i32)");
    module.body().line("i32.const 0");
    module.body().line("else");
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("call $__rt_mixed_tag");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_OBJECT));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.tee ${}", object_local));
    module.body().line("i64.load");
    module.body().line(&format!("local.set ${}", class_id_local));
    let property_local =
        materialize_runtime_string_expr(property, "nullsafe_dynamic_property_isset_name", module)?;
    emit_mixed_object_dynamic_property_isset_branch(
        &object_local,
        &class_id_local,
        &property_local,
        &candidates,
        0,
        module,
    )?;
    module.body().close("end");
    Ok(ValueKind::Bool)
}

pub(in crate::codegen::wasm) fn emit_dynamic_object_property_empty_expr(
    expr: &Expr,
    object: &Expr,
    property: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if object_receiver_needs_runtime_class_id(object, module) {
        return emit_dynamic_object_property_empty_by_class_expr(expr, object, property, module);
    }
    let class_name = object_class_name_for_expr(object, module).ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web dynamic property empty requires a statically known object class",
        )
    })?;
    let class_info = module.object_class(&class_name).cloned().ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web dynamic property empty requires a declared class",
        )
    })?;
    let properties = visible_fixed_properties(&class_info, module);
    if emit_expr(object, module)? != ValueKind::Object {
        return Err(CompileError::new(
            object.span,
            "wasm32-web dynamic property empty expected an object receiver",
        ));
    }
    let object_local = module
        .next_label("dynamic_property_empty_object")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(object_local.clone());
    module.body().line(&format!("local.set ${}", object_local));
    if let Some(property_name) = static_object_property_name(property, module) {
        let Some(property_info) = properties
            .iter()
            .find(|candidate| candidate.name == property_name)
        else {
            module.body().line("i32.const 1");
            return Ok(());
        };
        module.body().line(&format!("local.get ${}", object_local));
        emit_load_property(property_info, module)?;
        emit_loaded_property_empty(property_info.kind, module);
        return Ok(());
    }
    let property_local =
        materialize_runtime_string_expr(property, "dynamic_property_empty_name", module)?;
    emit_runtime_dynamic_property_empty_branch(&object_local, &property_local, &properties, 0, module)
}

fn emit_dynamic_object_property_isset_by_class_expr(
    expr: &Expr,
    object: &Expr,
    property: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let receiver_type = dynamic_object_receiver_declared_type(object, module);
    let candidates = dynamic_object_property_candidates(expr, receiver_type.as_deref(), module)?;
    let object_local = module
        .next_label("dynamic_object_property_isset_object")
        .trim_start_matches('$')
        .to_string();
    let class_id_local = module
        .next_label("dynamic_object_property_isset_class_id")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(object_local.clone());
    module.declare_i64_local(class_id_local.clone());
    if emit_expr(object, module)? != ValueKind::Object {
        return Err(CompileError::new(
            object.span,
            "wasm32-web dynamic object property isset expected an object receiver",
        ));
    }
    module.body().line(&format!("local.tee ${}", object_local));
    module.body().line("i64.load");
    module.body().line(&format!("local.set ${}", class_id_local));
    let property_local =
        materialize_runtime_string_expr(property, "dynamic_object_property_isset_name", module)?;
    emit_mixed_object_dynamic_property_isset_branch(
        &object_local,
        &class_id_local,
        &property_local,
        &candidates,
        0,
        module,
    )?;
    Ok(ValueKind::Bool)
}

fn emit_dynamic_object_property_empty_by_class_expr(
    expr: &Expr,
    object: &Expr,
    property: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let receiver_type = dynamic_object_receiver_declared_type(object, module);
    let candidates = dynamic_object_property_candidates(expr, receiver_type.as_deref(), module)?;
    let object_local = module
        .next_label("dynamic_object_property_empty_object")
        .trim_start_matches('$')
        .to_string();
    let class_id_local = module
        .next_label("dynamic_object_property_empty_class_id")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(object_local.clone());
    module.declare_i64_local(class_id_local.clone());
    if emit_expr(object, module)? != ValueKind::Object {
        return Err(CompileError::new(
            object.span,
            "wasm32-web dynamic object property empty expected an object receiver",
        ));
    }
    module.body().line(&format!("local.tee ${}", object_local));
    module.body().line("i64.load");
    module.body().line(&format!("local.set ${}", class_id_local));
    let property_local =
        materialize_runtime_string_expr(property, "dynamic_object_property_empty_name", module)?;
    emit_mixed_object_dynamic_property_empty_branch(
        &object_local,
        &class_id_local,
        &property_local,
        &candidates,
        0,
        module,
    )
}

fn emit_runtime_dynamic_property_isset_expr(
    expr: &Expr,
    object: &Expr,
    property: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let class_name = object_class_name_for_expr(object, module).ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web runtime dynamic property isset requires a statically known object class",
        )
    })?;
    let class_info = module.object_class(&class_name).cloned().ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web runtime dynamic property isset requires a declared class",
        )
    })?;
    let properties: Vec<ObjectPropertyInfo> = class_info
        .properties
        .iter()
        .filter(|property_info| {
            module.object_member_is_accessible(&property_info.owner_class, &property_info.visibility)
        })
        .cloned()
        .collect();
    if properties.is_empty() {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web runtime dynamic property isset requires visible fixed property metadata",
        ));
    }
    if emit_expr(object, module)? != ValueKind::Object {
        return Err(CompileError::new(
            object.span,
            "wasm32-web runtime dynamic property isset expected an object receiver",
        ));
    }
    let object_local = module
        .next_label("dynamic_property_isset_object")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(object_local.clone());
    module.body().line(&format!("local.set ${}", object_local));
    let property_local = materialize_runtime_string_expr(property, "dynamic_property_isset_name", module)?;
    emit_runtime_dynamic_property_isset_branch(&object_local, &property_local, &properties, 0, module)?;
    Ok(ValueKind::Bool)
}

pub(in crate::codegen::wasm) fn emit_method_call_expr(
    expr: &Expr,
    object: &Expr,
    method: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if object_receiver_needs_runtime_class_id(object, module) {
        return emit_dynamic_object_method_call(expr, object, method, args, module);
    }
    if object_class_name_for_expr(object, module).is_none() {
        if let Some(cell) = materialize_mixed_value_cell(object, module)? {
            return emit_mixed_object_no_arg_method_call(expr, &cell, method, args, module);
        }
        if dynamic_object_receiver_candidate(object, module) {
            return emit_dynamic_object_method_call(expr, object, method, args, module);
        }
    }
    let class_name = object_class_name_for_expr(object, module).ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web method calls require a statically known object class",
        )
    })?;
    module.object_class(&class_name).ok_or_else(|| {
        CompileError::new(expr.span, "wasm32-web method call requires a declared class")
    })?;
    let (declaring_class, method_info) = module.object_method_in_hierarchy(&class_name, method).ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web method call requires a supported public fixed method",
        )
    })?;
    if !module.object_member_is_accessible(&method_info.owner_class, &method_info.visibility) {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web method call requires visible fixed method metadata",
        ));
    }
    if !declaring_class.eq_ignore_ascii_case(&class_name)
        && method_body_uses_this_property(&method_info.body)
        && !inherited_property_layout_available(&class_name, &declaring_class, module)
    {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web inherited methods that read parent properties require inherited object layout metadata",
        ));
    }
    let mut call_args = Vec::with_capacity(args.len() + 1);
    call_args.push(object.clone());
    call_args.extend(args.iter().cloned());
    emit_user_function_args(expr, &method_info.symbol, &call_args, module)?;
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(&method_info.symbol)));
    Ok(method_info.return_kind)
}

pub(in crate::codegen::wasm) fn emit_nullable_exact_object_method_call(
    expr: &Expr,
    object: &Expr,
    method: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let class_name = object_class_name_for_expr(object, module).ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web nullable method calls require object return metadata",
        )
    })?;
    let (_, method_info) = module
        .object_method_in_hierarchy(&class_name, method)
        .ok_or_else(|| {
            CompileError::new(
                expr.span,
                "wasm32-web nullable method calls require supported fixed method metadata",
            )
        })?;
    let method_symbol = method_info.symbol.clone();
    let return_kind = method_info.return_kind;
    let result = module
        .next_label("nullsafe_exact_object_method_result")
        .trim_start_matches('$')
        .to_string();
    let object_local = module
        .next_label("nullsafe_exact_object_method_object")
        .trim_start_matches('$')
        .to_string();
    module.declare_mixed_local(result.clone());
    module.declare_object_local(object_local.clone());
    module.set_object_class_for_local(&object_local, Some(class_name));
    emit_alloc_mixed_cell(&result, module);
    let kind = emit_expr(object, module)?;
    if !matches!(kind, ValueKind::Object | ValueKind::Null) {
        return Err(CompileError::new(
            object.span,
            "wasm32-web nullable method calls expected object-or-null receiver",
        ));
    }
    module.body().line(&format!("local.set ${}", object_local));
    module.body().line(&format!("local.get ${}", object_local));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line(&format!("local.get ${}", result));
    module.body().line("call $__rt_value_store_null");
    module.body().line("else");
    let object_arg = Expr {
        kind: ExprKind::Variable(object_local.clone()),
        span: object.span,
    };
    let mut call_args = Vec::with_capacity(args.len() + 1);
    call_args.push(object_arg);
    call_args.extend(args.iter().cloned());
    emit_user_function_args(expr, &method_symbol, &call_args, module)?;
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(&method_symbol)));
    emit_store_method_result_value_kind(
        &format!("${}", result),
        return_kind,
        "nullsafe_exact_object_method",
        module,
    )?;
    module.body().close("end");
    module.body().line(&format!("local.get ${}", result));
    Ok(ValueKind::Mixed)
}

pub(in crate::codegen::wasm) fn emit_static_property_access_expr(
    expr: &Expr,
    receiver: &StaticReceiver,
    property: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let property_info = module
        .object_static_property(receiver, property)
        .ok_or_else(|| {
            CompileError::new(
                expr.span,
                "wasm32-web static property reads require a supported public scalar/static string property",
            )
        })?;
    if !module.object_member_is_accessible(&property_info.owner_class, &property_info.visibility) {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web static property reads require visible fixed static property metadata",
        ));
    }
    emit_load_static_property(&property_info, expr.span, module)
}

pub(in crate::codegen::wasm) fn emit_static_property_isset_expr(
    expr: &Expr,
    receiver: &StaticReceiver,
    property: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let kind = emit_static_property_access_expr(expr, receiver, property, module)?;
    match kind {
        ValueKind::Mixed => {
            module.body().line("call $__rt_mixed_tag");
            module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
            module.body().line("i32.ne");
        }
        ValueKind::Null => {
            module.body().line("drop");
            module.body().line("i32.const 0");
        }
        _ => {
            emit_drop_value_kind(kind, module);
            module.body().line("i32.const 1");
        }
    }
    Ok(ValueKind::Bool)
}

pub(in crate::codegen::wasm) fn static_property_value_kind(
    receiver: &StaticReceiver,
    property: &str,
    module: &WasmModule,
) -> Option<ValueKind> {
    let property_info = module.object_static_property(receiver, property)?;
    if !module.object_member_is_accessible(&property_info.owner_class, &property_info.visibility) {
        return None;
    }
    Some(value_kind_for_object_property_kind(
        property_info.kind,
    ))
}

pub(in crate::codegen::wasm) fn emit_static_property_assign(
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let property_info = module
        .object_static_property(receiver, property)
        .ok_or_else(|| {
            CompileError::new(
                value.span,
                "wasm32-web static property writes require a supported public scalar/static string property",
            )
        })?;
    if !module.object_member_is_accessible(&property_info.owner_class, &property_info.visibility) {
        return Err(CompileError::new(
            value.span,
            "wasm32-web static property writes require visible fixed static property metadata",
        ));
    }
    emit_store_static_property(&property_info, value, module)
}

pub(in crate::codegen::wasm) fn inherited_property_layout_available(
    class_name: &str,
    declaring_class: &str,
    module: &WasmModule,
) -> bool {
    let Some(class_info) = module.object_class(class_name) else {
        return false;
    };
    let Some(declaring_info) = module.object_class(declaring_class) else {
        return false;
    };
    declaring_info.properties.iter().all(|property| {
        class_info.properties.iter().any(|candidate| {
            candidate.name == property.name
                && candidate.offset == property.offset
                && candidate.kind == property.kind
        })
    })
}

pub(in crate::codegen::wasm) fn method_call_return_kind(
    object: &Expr,
    method: &str,
    module: &WasmModule,
) -> Option<ValueKind> {
    if let Some(class_name) = object_class_name_for_expr(object, module) {
        return Some(module.object_method_in_hierarchy(&class_name, method)?.1.return_kind);
    }
    if dynamic_object_receiver_candidate(object, module) {
        let receiver_type = dynamic_object_receiver_declared_type(object, module);
        return mixed_object_no_arg_method_return_kind(method, receiver_type.as_deref(), module);
    }
    mixed_object_cell_receiver_candidate(object, module)
        .then(|| mixed_object_no_arg_method_return_kind(method, None, module))
        .flatten()
}

pub(in crate::codegen::wasm) fn method_call_compact_int_array_return_len(
    object: &Expr,
    method: &str,
    module: &WasmModule,
) -> Option<usize> {
    let candidates = if let Some(class_name) = object_class_name_for_expr(object, module) {
        let (_, method_info) = module.object_method_in_hierarchy(&class_name, method)?;
        vec![method_info]
    } else if mixed_object_cell_receiver_candidate(object, module) {
        module
            .object_class_names_by_id()
            .into_iter()
            .filter_map(|(_, class_name)| {
                module
                    .object_method_in_hierarchy(&class_name, method)
                    .map(|(_, method_info)| method_info)
            })
            .collect()
    } else {
        return None;
    };
    let mut len = None;
    for method_info in candidates {
        if method_info.return_kind != ValueKind::Array {
            return None;
        }
        let candidate_len = compact_int_array_return_len(&method_info.body)?;
        if len.is_some_and(|existing| existing != candidate_len) {
            return None;
        }
        len = Some(candidate_len);
    }
    len
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::codegen::wasm) struct MethodArrayReturnMetadata {
    pub(in crate::codegen::wasm) layout: ArrayLayout,
    pub(in crate::codegen::wasm) len: Option<usize>,
    pub(in crate::codegen::wasm) value_kinds: Option<Vec<ValueCellKind>>,
    pub(in crate::codegen::wasm) value_constants: Option<Vec<ConstantValue>>,
    pub(in crate::codegen::wasm) runtime_value_kind: Option<ValueCellKind>,
    pub(in crate::codegen::wasm) nested_values: Option<Vec<Option<NestedArrayMetadata>>>,
    pub(in crate::codegen::wasm) key_kinds: Option<Vec<AssocKeyKind>>,
    pub(in crate::codegen::wasm) key_values: Option<Vec<AssocKeyValue>>,
}

pub(in crate::codegen::wasm) fn method_call_array_return_metadata(
    object: &Expr,
    method: &str,
    module: &WasmModule,
) -> Option<MethodArrayReturnMetadata> {
    let candidates = if let Some(class_name) = object_class_name_for_expr(object, module) {
        let (_, method_info) = module.object_method_in_hierarchy(&class_name, method)?;
        vec![method_info]
    } else if mixed_object_cell_receiver_candidate(object, module) {
        module
            .object_class_names_by_id()
            .into_iter()
            .filter_map(|(_, class_name)| {
                module
                    .object_method_in_hierarchy(&class_name, method)
                    .map(|(_, method_info)| method_info)
            })
            .collect()
    } else if let Some(receiver_type) = dynamic_object_receiver_declared_type(object, module) {
        module
            .object_class_names_by_id()
            .into_iter()
            .filter(|(_, class_name)| {
                candidate_class_matches_declared_receiver(class_name, Some(&receiver_type), module)
            })
            .filter_map(|(_, class_name)| {
                module
                    .object_method_in_hierarchy(&class_name, method)
                    .map(|(_, method_info)| method_info)
            })
            .collect()
    } else {
        return None;
    };
    let mut metadata: Option<MethodArrayReturnMetadata> = None;
    for method_info in candidates {
        if method_info.return_kind != ValueKind::Array {
            return None;
        }
        let candidate = MethodArrayReturnMetadata {
            layout: module.function_array_return_layout(&method_info.symbol),
            len: module.function_array_return_length(&method_info.symbol),
            value_kinds: module
                .function_array_return_value_kinds(&method_info.symbol)
                .map(|kinds| kinds.to_vec()),
            value_constants: module
                .function_array_return_value_constants(&method_info.symbol)
                .map(|values| values.to_vec()),
            runtime_value_kind: module.function_array_return_runtime_value_kind(&method_info.symbol),
            nested_values: module
                .function_array_return_nested_values(&method_info.symbol)
                .map(|metadata| metadata.to_vec()),
            key_kinds: module
                .function_array_return_key_kinds(&method_info.symbol)
                .map(|kinds| kinds.to_vec()),
            key_values: module
                .function_array_return_key_values(&method_info.symbol)
                .map(|values| values.to_vec()),
        };
        if let Some(existing) = metadata.as_mut() {
            if existing.layout != candidate.layout
                || existing.len != candidate.len
                || existing.value_kinds != candidate.value_kinds
                || existing.runtime_value_kind != candidate.runtime_value_kind
                || existing.nested_values != candidate.nested_values
                || existing.key_kinds != candidate.key_kinds
                || existing.key_values != candidate.key_values
            {
                return None;
            }
            if existing.value_constants != candidate.value_constants {
                existing.value_constants = None;
            }
        } else {
            metadata = Some(candidate);
        }
    }
    metadata
}

pub(in crate::codegen::wasm) fn static_method_call_array_return_metadata(
    receiver: &StaticReceiver,
    method: &str,
    module: &WasmModule,
) -> Option<MethodArrayReturnMetadata> {
    let class_name = module.class_name_for_receiver(receiver)?;
    let method_info = module.object_static_method_in_hierarchy(&class_name, method)?;
    if method_info.return_kind != ValueKind::Array {
        return None;
    }
    Some(MethodArrayReturnMetadata {
        layout: module.function_array_return_layout(&method_info.symbol),
        len: module.function_array_return_length(&method_info.symbol),
        value_kinds: module
            .function_array_return_value_kinds(&method_info.symbol)
            .map(|kinds| kinds.to_vec()),
        value_constants: module
            .function_array_return_value_constants(&method_info.symbol)
            .map(|values| values.to_vec()),
        runtime_value_kind: module.function_array_return_runtime_value_kind(&method_info.symbol),
        nested_values: module
            .function_array_return_nested_values(&method_info.symbol)
            .map(|metadata| metadata.to_vec()),
        key_kinds: module
            .function_array_return_key_kinds(&method_info.symbol)
            .map(|kinds| kinds.to_vec()),
        key_values: module
            .function_array_return_key_values(&method_info.symbol)
            .map(|values| values.to_vec()),
    })
}

fn compact_int_array_return_len(body: &[Stmt]) -> Option<usize> {
    let mut len = None;
    for stmt in body {
        let StmtKind::Return(Some(expr)) = &stmt.kind else {
            continue;
        };
        let ExprKind::ArrayLiteral(items) = &expr.kind else {
            return None;
        };
        if !items
            .iter()
            .all(|item| matches!(item.kind, ExprKind::IntLiteral(_)))
        {
            return None;
        }
        if len.is_some_and(|existing| existing != items.len()) {
            return None;
        }
        len = Some(items.len());
    }
    len
}

pub(in crate::codegen::wasm) fn method_body_uses_this_property(body: &[Stmt]) -> bool {
    body.iter().any(stmt_uses_this_property)
}

fn stmt_uses_this_property(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Echo(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::Return(Some(expr))
        | StmtKind::Throw(expr)
        | StmtKind::Include { path: expr, .. }
        | StmtKind::StaticVar { init: expr, .. } => expr_uses_this_property(expr),
        StmtKind::Assign { value, .. } | StmtKind::TypedAssign { value, .. } => {
            expr_uses_this_property(value)
        }
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. } => {
            expr_uses_this_property(index) || expr_uses_this_property(value)
        }
        StmtKind::NestedArrayAssign { target, value }
        | StmtKind::NestedArrayPush { target, value } => {
            expr_uses_this_property(target) || expr_uses_this_property(value)
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_uses_this_property(object) || expr_uses_this_property(value)
        }
        StmtKind::ArrayPush { value, .. }
        | StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. }
        | StmtKind::PropertyArrayPush { value, .. } => expr_uses_this_property(value),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_uses_this_property(condition)
                || then_body.iter().any(stmt_uses_this_property)
                || elseif_clauses.iter().any(|(cond, body)| {
                    expr_uses_this_property(cond) || body.iter().any(stmt_uses_this_property)
                })
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_uses_this_property))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_uses_this_property)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_uses_this_property))
        }
        StmtKind::While { condition, body } => {
            expr_uses_this_property(condition) || body.iter().any(stmt_uses_this_property)
        }
        StmtKind::DoWhile { body, condition } => {
            body.iter().any(stmt_uses_this_property) || expr_uses_this_property(condition)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_uses_this_property)
                || condition
                    .as_ref()
                    .is_some_and(expr_uses_this_property)
                || update.as_deref().is_some_and(stmt_uses_this_property)
                || body.iter().any(stmt_uses_this_property)
        }
        StmtKind::Foreach {
            array,
            body,
            ..
        } => expr_uses_this_property(array) || body.iter().any(stmt_uses_this_property),
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_uses_this_property(subject)
                || cases.iter().any(|(conds, body)| {
                    conds.iter().any(expr_uses_this_property)
                        || body.iter().any(stmt_uses_this_property)
                })
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_uses_this_property))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(stmt_uses_this_property)
                || catches
                    .iter()
                    .any(|catch| catch.body.iter().any(stmt_uses_this_property))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_uses_this_property))
        }
        StmtKind::Synthetic(stmts) | StmtKind::IncludeOnceGuard { body: stmts, .. } => {
            stmts.iter().any(stmt_uses_this_property)
        }
        StmtKind::NamespaceBlock { body, .. } => body.iter().any(stmt_uses_this_property),
        StmtKind::ConstDecl { value, .. } | StmtKind::ListUnpack { value, .. } => {
            expr_uses_this_property(value)
        }
        StmtKind::Return(None)
        | StmtKind::RefAssign { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::FunctionDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::Global { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => false,
    }
}

fn expr_uses_this_property(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::PropertyAccess { object, .. } | ExprKind::NullsafePropertyAccess { object, .. } => {
            matches!(object.kind, ExprKind::This) || expr_uses_this_property(object)
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            matches!(object.kind, ExprKind::This)
                || expr_uses_this_property(object)
                || expr_uses_this_property(property)
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_uses_this_property(array) || expr_uses_this_property(index)
        }
        ExprKind::Assignment { target, value, .. }
        | ExprKind::NullCoalesce {
            value: target,
            default: value,
        }
        | ExprKind::Pipe {
            value: target,
            callable: value,
        }
        | ExprKind::BinaryOp {
            left: target,
            right: value,
            ..
        } => expr_uses_this_property(target) || expr_uses_this_property(value),
        ExprKind::Negate(expr)
        | ExprKind::Not(expr)
        | ExprKind::BitNot(expr)
        | ExprKind::Throw(expr)
        | ExprKind::ErrorSuppress(expr)
        | ExprKind::Print(expr)
        | ExprKind::Spread(expr)
        | ExprKind::YieldFrom(expr)
        | ExprKind::Cast { expr, .. } => expr_uses_this_property(expr),
        ExprKind::FunctionCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. }
        | ExprKind::ClosureCall { args, .. } => args.iter().any(expr_uses_this_property),
        ExprKind::ExprCall { callee, args } => {
            expr_uses_this_property(callee) || args.iter().any(expr_uses_this_property)
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_uses_this_property(object) || args.iter().any(expr_uses_this_property)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_uses_this_property(condition)
                || expr_uses_this_property(then_expr)
                || expr_uses_this_property(else_expr)
        }
        ExprKind::ShortTernary { value, default } => {
            expr_uses_this_property(value) || expr_uses_this_property(default)
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_uses_this_property(subject)
                || arms.iter().any(|(conds, value)| {
                    conds.iter().any(expr_uses_this_property) || expr_uses_this_property(value)
                })
                || default
                    .as_deref()
                    .is_some_and(expr_uses_this_property)
        }
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_uses_this_property),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .any(|(key, value)| expr_uses_this_property(key) || expr_uses_this_property(value)),
        ExprKind::Closure { body, .. } => body.iter().any(stmt_uses_this_property),
        ExprKind::NamedArg { value, .. } => expr_uses_this_property(value),
        ExprKind::FirstClassCallable(target) => match target {
            CallableTarget::Method { object, .. } => expr_uses_this_property(object),
            CallableTarget::Function(_) | CallableTarget::StaticMethod { .. } => false,
        },
        ExprKind::InstanceOf { value, target } => {
            expr_uses_this_property(value)
                || matches!(target, InstanceOfTarget::Expr(expr) if expr_uses_this_property(expr))
        }
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(expr_uses_this_property)
                || value.as_deref().is_some_and(expr_uses_this_property)
        }
        _ => false,
    }
}

pub(in crate::codegen::wasm) fn object_property_value_kind(
    object: &Expr,
    property: &str,
    module: &WasmModule,
) -> Option<ValueKind> {
    if object_receiver_needs_runtime_class_id(object, module) {
        let receiver_type = dynamic_object_receiver_declared_type(object, module);
        return dynamic_object_scalar_property_candidates(
            object,
            property,
            receiver_type.as_deref(),
            module,
        )
        .ok()
        .map(|(kind, _)| value_kind_for_object_property_kind(kind));
    }
    let class_name = object_class_name_for_expr(object, module)?;
    let class_info = module.object_class(&class_name)?;
    let property_info = class_info
        .properties
        .iter()
        .find(|candidate| candidate.name == property)?;
    Some(match property_info.kind {
        ObjectPropertyKind::Int => ValueKind::Int,
        ObjectPropertyKind::Float => ValueKind::Float,
        ObjectPropertyKind::Bool => ValueKind::Bool,
        ObjectPropertyKind::Str => ValueKind::Str,
        ObjectPropertyKind::Object => ValueKind::Object,
        ObjectPropertyKind::Mixed => ValueKind::Mixed,
    })
}

pub(in crate::codegen::wasm) fn object_dynamic_property_value_kind(
    object: &Expr,
    property: &Expr,
    module: &WasmModule,
) -> Option<ValueKind> {
    if object_receiver_needs_runtime_class_id(object, module) {
        let receiver_type = dynamic_object_receiver_declared_type(object, module);
        if let Some(property_name) = static_object_property_name(property, module) {
            return dynamic_object_scalar_property_candidates(
                object,
                &property_name,
                receiver_type.as_deref(),
                module,
            )
            .ok()
            .map(|(kind, _)| value_kind_for_object_property_kind(kind));
        }
        let candidates = dynamic_object_property_candidates(object, receiver_type.as_deref(), module).ok()?;
        let properties = candidates
            .into_iter()
            .map(|(_, property_info)| property_info)
            .collect::<Vec<_>>();
        return consistent_property_kind(&properties).map(value_kind_for_object_property_kind);
    }
    if let Some(property_name) = static_object_property_name(property, module) {
        return object_property_value_kind(object, &property_name, module);
    }
    let class_name = object_class_name_for_expr(object, module)?;
    let class_info = module.object_class(&class_name)?;
    let properties = visible_fixed_properties(class_info, module);
    consistent_property_kind(&properties).map(value_kind_for_object_property_kind)
}

fn value_kind_for_object_property_kind(kind: ObjectPropertyKind) -> ValueKind {
    match kind {
        ObjectPropertyKind::Int => ValueKind::Int,
        ObjectPropertyKind::Float => ValueKind::Float,
        ObjectPropertyKind::Bool => ValueKind::Bool,
        ObjectPropertyKind::Str => ValueKind::Str,
        ObjectPropertyKind::Object => ValueKind::Object,
        ObjectPropertyKind::Mixed => ValueKind::Mixed,
    }
}

fn visible_fixed_properties(
    class_info: &ObjectClassInfo,
    module: &WasmModule,
) -> Vec<ObjectPropertyInfo> {
    class_info
        .properties
        .iter()
        .filter(|property| module.object_member_is_accessible(&property.owner_class, &property.visibility))
        .cloned()
        .collect()
}

fn consistent_property_kind(properties: &[ObjectPropertyInfo]) -> Option<ObjectPropertyKind> {
    let mut expected = None;
    for property in properties {
        if expected.is_some_and(|kind| kind != property.kind) {
            return None;
        }
        expected = Some(property.kind);
    }
    expected
}

pub(in crate::codegen::wasm) fn object_tostring_supported(
    object: &Expr,
    module: &WasmModule,
) -> bool {
    let Some(class_name) = object_class_name_for_expr(object, module) else {
        return mixed_object_cell_receiver_candidate(object, module)
            && mixed_object_no_arg_method_return_kind("__toString", None, module) == Some(ValueKind::Str);
    };
    module
        .object_method_in_hierarchy(&class_name, "__toString")
        .is_some_and(|(_, method)| method.params.is_empty() && method.return_kind == ValueKind::Str)
}

pub(in crate::codegen::wasm) fn unsupported_object_string_coercion_expr(
    object: &Expr,
    module: &WasmModule,
) -> Option<CompileError> {
    object_class_name_for_expr(object, module)?;
    (!object_tostring_supported(object, module)).then(|| {
        CompileError::new(
            object.span,
            "wasm32-web object string coercion requires object __toString/runtime support",
        )
    })
}

pub(in crate::codegen::wasm) fn unsupported_object_string_coercion_in_string_context(
    expr: &Expr,
    module: &WasmModule,
) -> Option<CompileError> {
    if let Some(err) = unsupported_object_string_coercion_expr(expr, module) {
        return Some(err);
    }
    match &expr.kind {
        ExprKind::Cast {
            target: CastType::String,
            expr,
        } => unsupported_object_string_coercion_expr(expr, module),
        ExprKind::BinaryOp {
            op: BinOp::Concat,
            left,
            right,
        } => unsupported_object_string_coercion_in_string_context(left, module)
            .or_else(|| unsupported_object_string_coercion_in_string_context(right, module)),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => unsupported_object_string_coercion_in_string_context(then_expr, module)
            .or_else(|| unsupported_object_string_coercion_in_string_context(else_expr, module)),
        ExprKind::ShortTernary { value, default } => {
            unsupported_object_string_coercion_in_string_context(value, module)
                .or_else(|| unsupported_object_string_coercion_in_string_context(default, module))
        }
        ExprKind::Match { arms, default, .. } => arms
            .iter()
            .find_map(|(_, value)| unsupported_object_string_coercion_in_string_context(value, module))
            .or_else(|| {
                default
                    .as_deref()
                    .and_then(|value| unsupported_object_string_coercion_in_string_context(value, module))
            }),
        ExprKind::FunctionCall { name, args } if function_call_args_are_string_context(name) => args
            .iter()
            .find_map(|arg| unsupported_object_string_coercion_in_string_context(arg, module)),
        _ => None,
    }
}

fn function_call_args_are_string_context(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "strlen" | "ord" | "sprintf" | "printf" | "basename" | "hash" | "str_contains"
    )
}

pub(in crate::codegen::wasm) fn emit_object_tostring_value_to_stack(
    object: &Expr,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !object_tostring_supported(object, module) {
        return Ok(false);
    }
    match emit_method_call_expr(object, object, "__toString", &[], module)? {
        ValueKind::Str => Ok(true),
        _ => unreachable!("supported __toString metadata must return a string"),
    }
}

pub(in crate::codegen::wasm) fn emit_static_method_call_expr(
    expr: &Expr,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if matches!(receiver, StaticReceiver::Parent) && method.eq_ignore_ascii_case("__construct") {
        return emit_parent_constructor_call(expr, receiver, args, module);
    }
    if let Some(kind) = emit_backed_enum_lookup_call(expr, receiver, method, args, module)? {
        return Ok(kind);
    }
    let class_name = module.class_name_for_receiver(receiver).ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web static method calls require an explicit class receiver or self::/parent:: context",
        )
    })?;
    let class_info = module.object_class(&class_name).cloned().ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web static method call requires a declared class",
        )
    })?;
    let Some(method_info) = module.object_static_method_in_hierarchy(&class_name, method) else {
        if class_info.parent.is_some() || !class_info.interfaces.is_empty() {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web inherited/interface static method dispatch requires object runtime metadata",
            ));
        }
        return Err(CompileError::new(
            expr.span,
            "wasm32-web static method call requires a supported public fixed static method",
        ));
    };
    if !module.object_member_is_accessible(&method_info.owner_class, &method_info.visibility) {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web static method call requires visible fixed static method metadata",
        ));
    }
    emit_user_function_args(expr, &method_info.symbol, args, module)?;
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(&method_info.symbol)));
    Ok(method_info.return_kind)
}

pub(in crate::codegen::wasm) fn static_method_call_return_kind(
    receiver: &StaticReceiver,
    method: &str,
    module: &WasmModule,
) -> Option<ValueKind> {
    let class_name = module.class_name_for_receiver(receiver)?;
    Some(
        module
            .object_static_method_in_hierarchy(&class_name, method)?
            .return_kind,
    )
}

fn emit_parent_constructor_call(
    expr: &Expr,
    receiver: &StaticReceiver,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let parent_class = module.class_name_for_receiver(receiver).ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web parent constructor calls require parent:: context",
        )
    })?;
    let current_class = module.object_class_for_local("this").ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web parent constructor calls require a statically known current object",
        )
    })?;
    let constructor = module.object_constructor(&parent_class).ok_or_else(|| {
        CompileError::new(
            expr.span,
            "wasm32-web parent constructor calls require a supported public fixed parent constructor",
        )
    })?;
    if !module.object_member_is_accessible(&constructor.owner_class, &constructor.visibility) {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web parent constructor calls require visible fixed constructor metadata",
        ));
    }
    if !inherited_property_layout_available(&current_class, &parent_class, module) {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web parent constructor calls require inherited object layout metadata",
        ));
    }
    let this_arg = Expr {
        kind: ExprKind::Variable("this".to_string()),
        span: expr.span,
    };
    let mut call_args = Vec::with_capacity(args.len() + 1);
    call_args.push(this_arg);
    call_args.extend(args.iter().cloned());
    emit_user_function_args(expr, &constructor.symbol, &call_args, module)?;
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(&constructor.symbol)));
    Ok(constructor.return_kind)
}

pub(in crate::codegen::wasm) fn emit_object_property_assign(
    object: &Expr,
    property: &str,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let class_name = object_class_name_for_expr(object, module).ok_or_else(|| {
        CompileError::new(
            object.span,
            "wasm32-web property writes require a statically known object class",
        )
    })?;
    let class_info = module.object_class(&class_name).cloned().ok_or_else(|| {
        CompileError::new(object.span, "wasm32-web property write requires a declared class")
    })?;
    let property_info = class_info
        .properties
        .iter()
        .find(|candidate| candidate.name == property)
        .cloned()
        .ok_or_else(|| {
            CompileError::new(
                object.span,
                "wasm32-web property write requires a supported public fixed property",
            )
        })?;
    if emit_expr(object, module)? != ValueKind::Object {
        return Err(CompileError::new(
            object.span,
            "wasm32-web property write expected an object receiver",
        ));
    }
    let object_local = module
        .next_label("object_write")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(object_local.clone());
    module.body().line(&format!("local.set ${}", object_local));
    emit_store_property_value(&object_local, &property_info, value, module)
}

pub(in crate::codegen::wasm) fn emit_dynamic_object_property_assign(
    object: &Expr,
    property: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if object_receiver_needs_runtime_class_id(object, module) {
        return emit_dynamic_object_scalar_property_assign(object, property, value, module);
    }
    if let Some(property_name) = static_object_property_name(property, module) {
        return emit_object_property_assign(object, &property_name, value, module);
    }
    emit_runtime_dynamic_property_assign(object, property, value, module)
}

fn emit_dynamic_object_scalar_property_assign(
    object: &Expr,
    property: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let receiver_type = dynamic_object_receiver_declared_type(object, module);
    let candidates = if let Some(property_name) = static_object_property_name(property, module) {
        let (_, candidates) = dynamic_object_scalar_property_candidates(
            object,
            &property_name,
            receiver_type.as_deref(),
            module,
        )?;
        candidates
    } else {
        let candidates = dynamic_object_property_candidates(object, receiver_type.as_deref(), module)?;
        if consistent_property_kind(
            &candidates
                .iter()
                .map(|(_, property)| property.clone())
                .collect::<Vec<_>>(),
        )
        .is_none()
        {
            return Err(CompileError::new(
                property.span,
                "wasm32-web runtime dynamic object property writes require consistent property metadata",
            ));
        }
        candidates
    };
    if emit_expr(object, module)? != ValueKind::Object {
        return Err(CompileError::new(
            object.span,
            "wasm32-web dynamic object property write expected an object receiver",
        ));
    }
    let object_local = module
        .next_label("dynamic_object_property_write_object")
        .trim_start_matches('$')
        .to_string();
    let class_id_local = module
        .next_label("dynamic_object_property_write_class_id")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(object_local.clone());
    module.declare_i64_local(class_id_local.clone());
    module.body().line(&format!("local.tee ${}", object_local));
    module.body().line("i64.load");
    module.body().line(&format!("local.set ${}", class_id_local));
    if static_object_property_name(property, module).is_some() {
        emit_dynamic_object_scalar_property_write_branch(
            &object_local,
            &class_id_local,
            &candidates,
            0,
            value,
            module,
        )
    } else {
        let property_local =
            materialize_runtime_string_expr(property, "dynamic_object_property_write_name", module)?;
        emit_dynamic_object_runtime_property_write_branch(
            &object_local,
            &class_id_local,
            &property_local,
            &candidates,
            0,
            value,
            module,
        )
    }
}

fn emit_runtime_dynamic_property_assign(
    object: &Expr,
    property: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let class_name = object_class_name_for_expr(object, module).ok_or_else(|| {
        CompileError::new(
            object.span,
            "wasm32-web runtime dynamic property writes require a statically known object class",
        )
    })?;
    let class_info = module.object_class(&class_name).cloned().ok_or_else(|| {
        CompileError::new(
            object.span,
            "wasm32-web runtime dynamic property write requires a declared class",
        )
    })?;
    let properties = visible_fixed_properties(&class_info, module);
    if consistent_property_kind(&properties).is_none() {
        return Err(CompileError::new(
            property.span,
            "wasm32-web runtime dynamic property writes require consistent property metadata",
        ));
    }
    if properties.is_empty() {
        return Err(CompileError::new(
            object.span,
            "wasm32-web runtime dynamic property write requires visible fixed property metadata",
        ));
    }
    if emit_expr(object, module)? != ValueKind::Object {
        return Err(CompileError::new(
            object.span,
            "wasm32-web runtime dynamic property write expected an object receiver",
        ));
    }
    let object_local = module
        .next_label("dynamic_property_write_object")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(object_local.clone());
    module.body().line(&format!("local.set ${}", object_local));
    let property_local = materialize_runtime_string_expr(property, "dynamic_property_write_name", module)?;
    emit_runtime_dynamic_property_write_branch(
        &object_local,
        &property_local,
        &properties,
        0,
        value,
        module,
    )
}

pub(in crate::codegen::wasm) fn emit_object_property_assignment_expr(
    expr: &Expr,
    object: &Expr,
    property: &str,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let class_name = object_class_name_for_expr(object, module).ok_or_else(|| {
        CompileError::new(
            object.span,
            "wasm32-web property assignment expressions require a statically known object class",
        )
    })?;
    let class_info = module.object_class(&class_name).cloned().ok_or_else(|| {
        CompileError::new(
            object.span,
            "wasm32-web property assignment expression requires a declared class",
        )
    })?;
    let property_info = class_info
        .properties
        .iter()
        .find(|candidate| candidate.name == property)
        .cloned()
        .ok_or_else(|| {
            CompileError::new(
                object.span,
                "wasm32-web property assignment expression requires a supported public fixed property",
            )
        })?;
    if emit_expr(object, module)? != ValueKind::Object {
        return Err(CompileError::new(
            object.span,
            "wasm32-web property assignment expression expected an object receiver",
        ));
    }
    let object_local = module
        .next_label("object_assign_expr")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(object_local.clone());
    module.body().line(&format!("local.set ${}", object_local));
    emit_store_property_value(&object_local, &property_info, value, module)?;
    module.body().line(&format!("local.get ${}", object_local));
    emit_load_property(&property_info, module).map_err(|err| CompileError::new(expr.span, &err.message))
}

pub(in crate::codegen::wasm) fn emit_dynamic_object_property_assignment_expr(
    expr: &Expr,
    object: &Expr,
    property: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if object_receiver_needs_runtime_class_id(object, module) {
        return emit_dynamic_object_scalar_property_assignment_expr(expr, object, property, value, module);
    }
    if let Some(property_name) = static_object_property_name(property, module) {
        return emit_object_property_assignment_expr(expr, object, &property_name, value, module);
    }
    emit_runtime_dynamic_property_assignment_expr(expr, object, property, value, module)
}

fn emit_dynamic_object_scalar_property_assignment_expr(
    expr: &Expr,
    object: &Expr,
    property: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let receiver_type = dynamic_object_receiver_declared_type(object, module);
    let (kind, candidates, is_static_property_name) =
        if let Some(property_name) = static_object_property_name(property, module) {
            let (kind, candidates) = dynamic_object_scalar_property_candidates(
                expr,
                &property_name,
                receiver_type.as_deref(),
                module,
            )?;
            (kind, candidates, true)
        } else {
            let candidates = dynamic_object_property_candidates(expr, receiver_type.as_deref(), module)?;
            let properties = candidates
                .iter()
                .map(|(_, property)| property.clone())
                .collect::<Vec<_>>();
            let Some(kind) = consistent_property_kind(&properties) else {
                return Err(CompileError::new(
                    property.span,
                    "wasm32-web runtime dynamic object property assignment expressions require consistent property metadata",
                ));
            };
            (kind, candidates, false)
        };
    if emit_expr(object, module)? != ValueKind::Object {
        return Err(CompileError::new(
            object.span,
            "wasm32-web dynamic object property assignment expression expected an object receiver",
        ));
    }
    let object_local = module
        .next_label("dynamic_object_property_assign_expr_object")
        .trim_start_matches('$')
        .to_string();
    let class_id_local = module
        .next_label("dynamic_object_property_assign_expr_class_id")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(object_local.clone());
    module.declare_i64_local(class_id_local.clone());
    module.body().line(&format!("local.tee ${}", object_local));
    module.body().line("i64.load");
    module.body().line(&format!("local.set ${}", class_id_local));
    if is_static_property_name {
        emit_dynamic_object_scalar_property_assignment_expr_branch(
            expr,
            &object_local,
            &class_id_local,
            kind,
            &candidates,
            0,
            value,
            module,
        )?;
    } else {
        let property_local = materialize_runtime_string_expr(
            property,
            "dynamic_object_property_assign_expr_name",
            module,
        )?;
        emit_dynamic_object_runtime_property_assignment_expr_branch(
            expr,
            &object_local,
            &class_id_local,
            &property_local,
            kind,
            &candidates,
            0,
            value,
            module,
        )?;
    }
    Ok(value_kind_for_object_property_kind(kind))
}

fn emit_runtime_dynamic_property_assignment_expr(
    expr: &Expr,
    object: &Expr,
    property: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let class_name = object_class_name_for_expr(object, module).ok_or_else(|| {
        CompileError::new(
            object.span,
            "wasm32-web runtime dynamic property assignment expressions require a statically known object class",
        )
    })?;
    let class_info = module.object_class(&class_name).cloned().ok_or_else(|| {
        CompileError::new(
            object.span,
            "wasm32-web runtime dynamic property assignment expression requires a declared class",
        )
    })?;
    let properties = visible_fixed_properties(&class_info, module);
    let Some(kind) = consistent_property_kind(&properties) else {
        return Err(CompileError::new(
            property.span,
            "wasm32-web runtime dynamic property assignment expressions require consistent property metadata",
        ));
    };
    if properties.is_empty() {
        return Err(CompileError::new(
            object.span,
            "wasm32-web runtime dynamic property assignment expression requires visible fixed property metadata",
        ));
    }
    if emit_expr(object, module)? != ValueKind::Object {
        return Err(CompileError::new(
            object.span,
            "wasm32-web runtime dynamic property assignment expression expected an object receiver",
        ));
    }
    let object_local = module
        .next_label("dynamic_property_assign_expr_object")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(object_local.clone());
    module.body().line(&format!("local.set ${}", object_local));
    let property_local =
        materialize_runtime_string_expr(property, "dynamic_property_assign_expr_name", module)?;
    emit_runtime_dynamic_property_assignment_expr_branch(
        expr,
        &object_local,
        &property_local,
        kind,
        &properties,
        0,
        value,
        module,
    )?;
    Ok(value_kind_for_object_property_kind(kind))
}

pub(in crate::codegen::wasm) fn emit_instanceof_expr(
    expr: &Expr,
    value: &Expr,
    target: &InstanceOfTarget,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let target_expr = match target {
        InstanceOfTarget::Name(_) => None,
        InstanceOfTarget::Expr(target_expr) => Some(target_expr.as_ref()),
    };
    let target_name = match target {
        InstanceOfTarget::Name(target_name) => target_name.to_string(),
        InstanceOfTarget::Expr(target_expr) => {
            static_class_string_value(target_expr, module).ok_or_else(|| {
                CompileError::new(
                    target_expr.span,
                    "wasm32-web dynamic instanceof targets currently require a statically known class string",
                )
            })?
        }
    };
    if let Some(cell) = materialize_mixed_value_cell(value, module)? {
        if let Some(target_expr) = target_expr {
            emit_static_class_string_side_effect(target_expr, module)?;
        }
        emit_mixed_cell_object_class_match(expr, &cell, target_name.as_str(), false, module)?;
        return Ok(ValueKind::Bool);
    }
    let value_class = object_class_name_for_expr(value, module);
    let kind = emit_expr(value, module)?;
    if let Some(target_expr) = target_expr {
        emit_static_class_string_side_effect(target_expr, module)?;
    }
    if kind != ValueKind::Object {
        emit_drop_value_kind(kind, module);
        module.body().line("i32.const 0");
        return Ok(ValueKind::Bool);
    }
    if object_receiver_needs_runtime_class_id(value, module) {
        return emit_dynamic_object_class_match(expr, value, target_name.as_str(), false, None, module);
    }
    let Some(value_class) = value_class else {
        if dynamic_object_receiver_candidate(value, module) {
            return emit_dynamic_object_class_match(expr, value, target_name.as_str(), false, None, module);
        }
        module.body().line("drop");
        return Err(CompileError::new(
            value.span,
            "wasm32-web instanceof requires a statically known object class",
        ));
    };
    module.body().line("drop");
    if static_object_class_match(&value_class, target_name.as_str(), false, expr, module)? {
        module.body().line("i32.const 1");
        return Ok(ValueKind::Bool);
    }
    module.body().line("i32.const 0");
    Ok(ValueKind::Bool)
}

pub(in crate::codegen::wasm) fn emit_is_a_call(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web is_a() expects two or three arguments",
        ));
    }
    let allow_string = args
        .get(2)
        .map(literal_bool_arg)
        .transpose()?
        .unwrap_or(false);
    if allow_string {
        if let Some(value_class) = evaluated_static_class_string_value(&args[0], module)? {
            let target = evaluated_static_class_string_value(&args[1], module)?.ok_or_else(|| {
                CompileError::new(
                    args[1].span,
                    "wasm32-web is_a() currently requires a static class-string target",
                )
            })?;
            let matches_target =
                static_class_string_object_match(&value_class, &target, false, call, module)?;
            module.body().line(&format!(
                "i32.const {}",
                i32::from(matches_target)
            ));
            return Ok(ValueKind::Bool);
        }
        if expression_is_stringy(&args[0], module) {
            return Err(CompileError::new(
                call.span,
                "wasm32-web is_a() string class mode currently requires a static class-string value",
            ));
        }
    }
    let target = static_class_string_value(&args[1], module).ok_or_else(|| {
        CompileError::new(
            args[1].span,
            "wasm32-web is_a() currently requires a static class-string target",
        )
    })?;
    if let Some(cell) = materialize_mixed_value_cell(&args[0], module)? {
        emit_static_class_string_side_effect(&args[1], module)?;
        emit_mixed_cell_object_class_match(call, &cell, &target, false, module)?;
        return Ok(ValueKind::Bool);
    }
    emit_exact_object_class_match(call, &args[0], &target, false, Some(&args[1]), module)
}

pub(in crate::codegen::wasm) fn emit_is_subclass_of_call(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web is_subclass_of() expects two or three arguments",
        ));
    }
    let allow_string = args
        .get(2)
        .map(literal_bool_arg)
        .transpose()?
        .unwrap_or(true);
    if allow_string {
        if let Some(value_class) = evaluated_static_class_string_value(&args[0], module)? {
            let target = evaluated_static_class_string_value(&args[1], module)?.ok_or_else(|| {
                CompileError::new(
                    args[1].span,
                    "wasm32-web is_subclass_of() currently requires a static class-string target",
                )
            })?;
            let matches_target =
                static_class_string_object_match(&value_class, &target, true, call, module)?;
            module.body().line(&format!(
                "i32.const {}",
                i32::from(matches_target)
            ));
            return Ok(ValueKind::Bool);
        }
        if expression_is_stringy(&args[0], module) {
            return Err(CompileError::new(
                call.span,
                "wasm32-web is_subclass_of() string class mode currently requires a static class-string value",
            ));
        }
    }
    let target = static_class_string_value(&args[1], module).ok_or_else(|| {
        CompileError::new(
            args[1].span,
            "wasm32-web is_subclass_of() currently requires a static class-string target",
        )
    })?;
    if let Some(cell) = materialize_mixed_value_cell(&args[0], module)? {
        emit_static_class_string_side_effect(&args[1], module)?;
        emit_mixed_cell_object_class_match(call, &cell, &target, true, module)?;
        return Ok(ValueKind::Bool);
    }
    emit_exact_object_class_match(call, &args[0], &target, true, Some(&args[1]), module)
}

pub(in crate::codegen::wasm) fn emit_get_class_value_to_stack(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let [object] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web get_class() expects exactly one argument",
        ));
    };
    if let Some(cell) = materialize_mixed_value_cell(object, module)? {
        emit_get_class_from_mixed_cell(&cell, object.span, module)?;
        return Ok(());
    }
    let class_name = object_class_name_for_expr(object, module);
    if emit_expr(object, module)? != ValueKind::Object {
        return Err(CompileError::new(
            object.span,
            "wasm32-web get_class() expected an object argument",
        ));
    }
    if object_receiver_needs_runtime_class_id(object, module) {
        emit_get_class_from_dynamic_object(object, module)?;
        return Ok(());
    }
    let Some(class_name) = class_name else {
        if dynamic_object_receiver_candidate(object, module) {
            emit_get_class_from_dynamic_object(object, module)?;
            return Ok(());
        }
        module.body().line("drop");
        return Err(CompileError::new(
            object.span,
            "wasm32-web get_class() requires a statically known object class",
        ));
    };
    module.body().line("drop");
    let (ptr, len) = module.intern_string(&class_name);
    module.body().line(&format!("i32.const {}", ptr));
    module.body().line(&format!("i32.const {}", len));
    Ok(())
}

pub(in crate::codegen::wasm) fn emit_get_parent_class_value_to_stack(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let class_name = match args {
        [] => module.current_class().map(str::to_string).ok_or_else(|| {
            CompileError::new(
                call.span,
                "wasm32-web get_parent_class() without arguments requires a class context",
            )
        })?,
        [target] if object_class_name_for_expr(target, module).is_some() => {
            let class_name = object_class_name_for_expr(target, module).expect("checked by guard");
            if emit_expr(target, module)? != ValueKind::Object {
                return Err(CompileError::new(
                    target.span,
                    "wasm32-web get_parent_class() expected an object or static class-string argument",
                ));
            }
            module.body().line("drop");
            class_name
        }
        [target] => {
            if let Some(class_name) = evaluated_static_class_string_value(target, module)? {
                class_name
            } else if static_callback_function_name(target, module).is_some() {
                evaluated_static_callback_function_name(target, module)?.ok_or_else(|| {
                    CompileError::new(
                        target.span,
                        "wasm32-web get_parent_class() currently requires a known object or static class-string argument",
                    )
                })?
            } else {
                return Err(CompileError::new(
                    target.span,
                    "wasm32-web get_parent_class() currently requires a known object or static class-string argument",
                ));
            }
        }
        _ => {
            return Err(CompileError::new(
                call.span,
                "wasm32-web get_parent_class() expects zero or one arguments",
            ))
        }
    };
    let Some(class_info) = module.object_class(&class_name) else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web get_parent_class() currently requires a declared class target",
        ));
    };
    let parent_name = class_info
        .parent
        .as_deref()
        .and_then(|parent| module.object_class(parent))
        .map(|parent| parent.name.clone())
        .unwrap_or_default();
    let (ptr, len) = module.intern_string(&parent_name);
    module.body().line(&format!("i32.const {}", ptr));
    module.body().line(&format!("i32.const {}", len));
    Ok(())
}

fn emit_get_class_from_dynamic_object(object: &Expr, module: &mut WasmModule) -> Result<(), CompileError> {
    let receiver_type = dynamic_object_receiver_declared_type(object, module);
    let classes = module
        .object_class_names_by_id()
        .into_iter()
        .filter(|(_, class_name)| candidate_class_matches_declared_receiver(class_name, receiver_type.as_deref(), module))
        .collect::<Vec<_>>();
    if classes.is_empty() {
        return Err(CompileError::new(
            object.span,
            "wasm32-web get_class() from dynamic object receivers requires object class metadata",
        ));
    }
    let object_local = module
        .next_label("dynamic_object_get_class_object")
        .trim_start_matches('$')
        .to_string();
    let class_id_local = module
        .next_label("dynamic_object_get_class_class_id")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(object_local.clone());
    module.declare_i64_local(class_id_local.clone());
    module.body().line(&format!("local.set ${}", object_local));
    module.body().line(&format!("local.get ${}", object_local));
    module.body().line("i64.load");
    module.body().line(&format!("local.set ${}", class_id_local));
    emit_get_class_name_from_class_id(&class_id_local, &classes, module);
    Ok(())
}

fn emit_get_class_from_mixed_cell(
    cell: &str,
    span: crate::span::Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let classes = module.object_class_names_by_id();
    if classes.is_empty() {
        return Err(CompileError::new(
            span,
            "wasm32-web get_class() from mixed object cells requires object class metadata",
        ));
    }
    let class_id_local = module
        .next_label("mixed_object_class_id")
        .trim_start_matches('$')
        .to_string();
    module.declare_i64_local(class_id_local.clone());
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_OBJECT));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line("i64.load");
    module.body().line(&format!("local.set ${}", class_id_local));
    emit_get_class_name_from_class_id(&class_id_local, &classes, module);
    Ok(())
}

fn emit_get_class_name_from_class_id(
    class_id_local: &str,
    classes: &[(u64, String)],
    module: &mut WasmModule,
) {
    emit_get_class_name_branch(class_id_local, classes, 0, module);
}

fn emit_get_class_name_branch(
    class_id_local: &str,
    classes: &[(u64, String)],
    index: usize,
    module: &mut WasmModule,
) {
    if index == classes.len() {
        module.body().line("unreachable");
        return;
    }
    let (class_id, class_name) = &classes[index];
    module.body().line(&format!("local.get ${}", class_id_local));
    module.body().line(&format!("i64.const {}", class_id));
    module.body().line("i64.eq");
    module.body().open("if (result i32 i32)");
    let (ptr, len) = module.intern_string(class_name);
    module.body().line(&format!("i32.const {}", ptr));
    module.body().line(&format!("i32.const {}", len));
    module.body().line("else");
    emit_get_class_name_branch(class_id_local, classes, index + 1, module);
    module.body().close("end");
}

fn emit_mixed_cell_object_class_match(
    expr: &Expr,
    cell: &str,
    target: &str,
    exclude_self: bool,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let classes = object_class_match_values(target, exclude_self, expr, module)?;
    if classes.is_empty() {
        module.body().line("i32.const 0");
        return Ok(());
    }
    let class_id_local = module
        .next_label("mixed_object_match_class_id")
        .trim_start_matches('$')
        .to_string();
    module.declare_i64_local(class_id_local.clone());
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_OBJECT));
    module.body().line("i32.eq");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line("i64.load");
    module.body().line(&format!("local.set ${}", class_id_local));
    emit_class_id_match_branch(&class_id_local, &classes, 0, module);
    module.body().line("else");
    module.body().line("i32.const 0");
    module.body().close("end");
    Ok(())
}

fn emit_mixed_object_scalar_property_access(
    expr: &Expr,
    cell: &str,
    property: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let (kind, candidates) = mixed_object_scalar_property_candidates(expr, property, module)?;
    let object_local = module
        .next_label("mixed_object_property_object")
        .trim_start_matches('$')
        .to_string();
    let class_id_local = module
        .next_label("mixed_object_property_class_id")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(object_local.clone());
    module.declare_i64_local(class_id_local.clone());
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_OBJECT));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.tee ${}", object_local));
    module.body().line("i64.load");
    module.body().line(&format!("local.set ${}", class_id_local));
    emit_mixed_object_scalar_property_branch(
        &object_local,
        &class_id_local,
        kind,
        &candidates,
        0,
        module,
    )?;
    Ok(value_kind_for_object_property_kind(kind))
}

fn emit_dynamic_object_scalar_property_access(
    expr: &Expr,
    object: &Expr,
    property: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let receiver_type = dynamic_object_receiver_declared_type(object, module);
    let (kind, candidates) =
        dynamic_object_scalar_property_candidates(expr, property, receiver_type.as_deref(), module)?;
    let object_local = module
        .next_label("dynamic_object_property_object")
        .trim_start_matches('$')
        .to_string();
    let class_id_local = module
        .next_label("dynamic_object_property_class_id")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(object_local.clone());
    module.declare_i64_local(class_id_local.clone());
    if emit_expr(object, module)? != ValueKind::Object {
        return Err(CompileError::new(
            object.span,
            "wasm32-web dynamic object property read expected an object receiver",
        ));
    }
    module.body().line(&format!("local.tee ${}", object_local));
    module.body().line("i64.load");
    module.body().line(&format!("local.set ${}", class_id_local));
    emit_mixed_object_scalar_property_branch(
        &object_local,
        &class_id_local,
        kind,
        &candidates,
        0,
        module,
    )?;
    Ok(value_kind_for_object_property_kind(kind))
}

fn emit_nullsafe_mixed_object_scalar_property_access(
    expr: &Expr,
    cell: &str,
    property: &str,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let (kind, candidates) = mixed_object_scalar_property_candidates(expr, property, module)?;
    let result = module
        .next_label("nullsafe_mixed_object_property_result")
        .trim_start_matches('$')
        .to_string();
    let object_local = module
        .next_label("nullsafe_mixed_object_property_object")
        .trim_start_matches('$')
        .to_string();
    let class_id_local = module
        .next_label("nullsafe_mixed_object_property_class_id")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(result.clone());
    module.declare_object_local(object_local.clone());
    module.declare_i64_local(class_id_local.clone());
    emit_alloc_mixed_cell(&result, module);
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("call $__rt_mixed_tag");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get ${}", result));
    module.body().line("call $__rt_value_store_null");
    module.body().line("else");
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("call $__rt_mixed_tag");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_OBJECT));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.tee ${}", object_local));
    module.body().line("i64.load");
    module.body().line(&format!("local.set ${}", class_id_local));
    emit_mixed_object_scalar_property_branch(
        &object_local,
        &class_id_local,
        kind,
        &candidates,
        0,
        module,
    )?;
    emit_store_emitted_value_kind(
        &format!("${}", result),
        value_kind_for_object_property_kind(kind),
        "nullsafe_mixed_object_property",
        module,
    )?;
    module.body().close("end");
    module.body().line(&format!("local.get ${}", result));
    Ok(ValueKind::Mixed)
}

pub(in crate::codegen::wasm) fn emit_mixed_object_no_arg_method_call(
    expr: &Expr,
    cell: &str,
    method: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let (return_kind, candidates) = mixed_object_no_arg_method_candidates(expr, method, None, module)?;
    let object_local = module
        .next_label("mixed_object_method_object")
        .trim_start_matches('$')
        .to_string();
    let class_id_local = module
        .next_label("mixed_object_method_class_id")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(object_local.clone());
    module.declare_i64_local(class_id_local.clone());
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_OBJECT));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.tee ${}", object_local));
    module.body().line("i64.load");
    module.body().line(&format!("local.set ${}", class_id_local));
    emit_mixed_object_no_arg_method_branch(
        expr,
        &object_local,
        &class_id_local,
        return_kind,
        &candidates,
        args,
        0,
        module,
    )?;
    Ok(return_kind)
}

fn emit_dynamic_object_method_call(
    expr: &Expr,
    object: &Expr,
    method: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let receiver_type = dynamic_object_receiver_declared_type(object, module);
    let (return_kind, candidates) =
        mixed_object_no_arg_method_candidates(expr, method, receiver_type.as_deref(), module)?;
    let object_local = module
        .next_label("dynamic_object_method_object")
        .trim_start_matches('$')
        .to_string();
    let class_id_local = module
        .next_label("dynamic_object_method_class_id")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(object_local.clone());
    module.declare_i64_local(class_id_local.clone());
    if emit_expr(object, module)? != ValueKind::Object {
        return Err(CompileError::new(
            object.span,
            "wasm32-web dynamic object method dispatch expected an object receiver",
        ));
    }
    module.body().line(&format!("local.tee ${}", object_local));
    module.body().line("i64.load");
    module.body().line(&format!("local.set ${}", class_id_local));
    emit_mixed_object_no_arg_method_branch(
        expr,
        &object_local,
        &class_id_local,
        return_kind,
        &candidates,
        args,
        0,
        module,
    )?;
    Ok(return_kind)
}

pub(in crate::codegen::wasm) fn emit_nullsafe_dynamic_object_method_call(
    expr: &Expr,
    object: &Expr,
    method: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let receiver_type = dynamic_object_receiver_declared_type(object, module);
    let (return_kind, candidates) =
        mixed_object_no_arg_method_candidates(expr, method, receiver_type.as_deref(), module)?;
    let result = module
        .next_label("nullsafe_dynamic_object_method_result")
        .trim_start_matches('$')
        .to_string();
    let object_local = module
        .next_label("nullsafe_dynamic_object_method_object")
        .trim_start_matches('$')
        .to_string();
    let class_id_local = module
        .next_label("nullsafe_dynamic_object_method_class_id")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(result.clone());
    module.declare_object_local(object_local.clone());
    module.declare_i64_local(class_id_local.clone());
    emit_alloc_mixed_cell(&result, module);
    if emit_expr(object, module)? != ValueKind::Object {
        return Err(CompileError::new(
            object.span,
            "wasm32-web nullable dynamic object method dispatch expected an object receiver",
        ));
    }
    module.body().line(&format!("local.tee ${}", object_local));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line(&format!("local.get ${}", result));
    module.body().line("call $__rt_value_store_null");
    module.body().line("else");
    module.body().line(&format!("local.get ${}", object_local));
    module.body().line("i64.load");
    module.body().line(&format!("local.set ${}", class_id_local));
    emit_mixed_object_no_arg_method_branch(
        expr,
        &object_local,
        &class_id_local,
        return_kind,
        &candidates,
        args,
        0,
        module,
    )?;
    emit_store_method_result_value_kind(
        &format!("${}", result),
        return_kind,
        "nullsafe_dynamic_object_method",
        module,
    )?;
    module.body().close("end");
    module.body().line(&format!("local.get ${}", result));
    Ok(ValueKind::Mixed)
}

fn emit_nullsafe_mixed_object_no_arg_method_call(
    expr: &Expr,
    cell: &str,
    method: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let (return_kind, candidates) = mixed_object_no_arg_method_candidates(expr, method, None, module)?;
    let result = module
        .next_label("nullsafe_mixed_object_method_result")
        .trim_start_matches('$')
        .to_string();
    let object_local = module
        .next_label("nullsafe_mixed_object_method_object")
        .trim_start_matches('$')
        .to_string();
    let class_id_local = module
        .next_label("nullsafe_mixed_object_method_class_id")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(result.clone());
    module.declare_object_local(object_local.clone());
    module.declare_i64_local(class_id_local.clone());
    emit_alloc_mixed_cell(&result, module);
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("call $__rt_mixed_tag");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get ${}", result));
    module.body().line("call $__rt_value_store_null");
    module.body().line("else");
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("call $__rt_mixed_tag");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_OBJECT));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.tee ${}", object_local));
    module.body().line("i64.load");
    module.body().line(&format!("local.set ${}", class_id_local));
    emit_mixed_object_no_arg_method_branch(
        expr,
        &object_local,
        &class_id_local,
        return_kind,
        &candidates,
        args,
        0,
        module,
    )?;
    emit_store_method_result_value_kind(
        &format!("${}", result),
        return_kind,
        "nullsafe_mixed_object_method",
        module,
    )?;
    module.body().close("end");
    module.body().line(&format!("local.get ${}", result));
    Ok(ValueKind::Mixed)
}

fn mixed_object_scalar_property_candidates(
    expr: &Expr,
    property: &str,
    module: &WasmModule,
) -> Result<(ObjectPropertyKind, Vec<(u64, ObjectPropertyInfo)>), CompileError> {
    let mut candidates = Vec::new();
    let mut expected_kind = None;
    for (class_id, class_name) in module.object_class_names_by_id() {
        let Some(class_info) = module.object_class(&class_name) else {
            continue;
        };
        let Some(property_info) = class_info
            .properties
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(property))
        else {
            continue;
        };
        if !module.object_member_is_accessible(&property_info.owner_class, &property_info.visibility)
        {
            continue;
        }
        if expected_kind.is_some_and(|kind| kind != property_info.kind) {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web mixed object-cell property reads require consistent property metadata",
            ));
        }
        expected_kind = Some(property_info.kind);
        candidates.push((class_id, property_info.clone()));
    }
    if candidates.is_empty() {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web mixed object-cell property reads require declared scalar/object property metadata",
        ));
    }
    Ok((expected_kind.expect("non-empty candidates must set kind"), candidates))
}

fn dynamic_object_scalar_property_candidates(
    expr: &Expr,
    property: &str,
    receiver_type: Option<&str>,
    module: &WasmModule,
) -> Result<(ObjectPropertyKind, Vec<(u64, ObjectPropertyInfo)>), CompileError> {
    let mut candidates = Vec::new();
    let mut expected_kind = None;
    for (class_id, class_name) in module.object_class_names_by_id() {
        if !candidate_class_matches_declared_receiver(&class_name, receiver_type, module) {
            continue;
        }
        let Some(class_info) = module.object_class(&class_name) else {
            continue;
        };
        let Some(property_info) = class_info
            .properties
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(property))
        else {
            continue;
        };
        if !module.object_member_is_accessible(&property_info.owner_class, &property_info.visibility)
        {
            continue;
        }
        if expected_kind.is_some_and(|kind| kind != property_info.kind) {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web dynamic object property reads require consistent property metadata",
            ));
        }
        expected_kind = Some(property_info.kind);
        candidates.push((class_id, property_info.clone()));
    }
    if candidates.is_empty() {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web dynamic object property reads require declared visible property metadata",
        ));
    }
    Ok((expected_kind.expect("non-empty candidates must set kind"), candidates))
}

fn dynamic_object_property_candidates(
    expr: &Expr,
    receiver_type: Option<&str>,
    module: &WasmModule,
) -> Result<Vec<(u64, ObjectPropertyInfo)>, CompileError> {
    let mut candidates = Vec::new();
    for (class_id, class_name) in module.object_class_names_by_id() {
        if !candidate_class_matches_declared_receiver(&class_name, receiver_type, module) {
            continue;
        }
        let Some(class_info) = module.object_class(&class_name) else {
            continue;
        };
        for property_info in class_info.properties.iter() {
            if module.object_member_is_accessible(&property_info.owner_class, &property_info.visibility) {
                candidates.push((class_id, property_info.clone()));
            }
        }
    }
    if candidates.is_empty() {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web dynamic object property predicates require visible fixed property metadata",
        ));
    }
    Ok(candidates)
}

fn mixed_object_dynamic_property_candidates(
    expr: &Expr,
    module: &WasmModule,
) -> Result<Vec<(u64, ObjectPropertyInfo)>, CompileError> {
    let mut candidates = Vec::new();
    for (class_id, class_name) in module.object_class_names_by_id() {
        let Some(class_info) = module.object_class(&class_name) else {
            continue;
        };
        for property_info in class_info.properties.iter() {
            if module.object_member_is_accessible(&property_info.owner_class, &property_info.visibility) {
                candidates.push((class_id, property_info.clone()));
            }
        }
    }
    if candidates.is_empty() {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web runtime dynamic nullsafe property reads require visible fixed property metadata",
        ));
    }
    Ok(candidates)
}

fn mixed_object_no_arg_method_candidates(
    expr: &Expr,
    method: &str,
    receiver_type: Option<&str>,
    module: &WasmModule,
) -> Result<(ValueKind, Vec<(u64, String)>), CompileError> {
    let mut candidates = Vec::new();
    let mut expected_kind = None;
    for (class_id, class_name) in module.object_class_names_by_id() {
        let Some((declaring_class, method_info)) =
            module.object_method_in_hierarchy(&class_name, method)
        else {
            continue;
        };
        if !candidate_class_matches_declared_receiver(&class_name, receiver_type, module) {
            continue;
        }
        if !module.object_member_is_accessible(&method_info.owner_class, &method_info.visibility) {
            continue;
        }
        if !declaring_class.eq_ignore_ascii_case(&class_name)
            && method_body_uses_this_property(&method_info.body)
            && !inherited_property_layout_available(&class_name, &declaring_class, module)
        {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web mixed object-cell inherited methods require inherited object layout metadata",
            ));
        }
        if expected_kind.is_some_and(|kind| kind != method_info.return_kind) {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web mixed object-cell method calls require consistent return metadata",
            ));
        }
        expected_kind = Some(method_info.return_kind);
        candidates.push((class_id, method_info.symbol));
    }
    if candidates.is_empty() {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web mixed object-cell method calls require declared method metadata",
        ));
    }
    Ok((expected_kind.expect("non-empty candidates must set return kind"), candidates))
}

fn mixed_object_no_arg_method_return_kind(
    method: &str,
    receiver_type: Option<&str>,
    module: &WasmModule,
) -> Option<ValueKind> {
    let mut expected_kind = None;
    let mut found = false;
    for (_, class_name) in module.object_class_names_by_id() {
        let Some((declaring_class, method_info)) =
            module.object_method_in_hierarchy(&class_name, method)
        else {
            continue;
        };
        if !candidate_class_matches_declared_receiver(&class_name, receiver_type, module) {
            continue;
        }
        if !module.object_member_is_accessible(&method_info.owner_class, &method_info.visibility)
            || (!declaring_class.eq_ignore_ascii_case(&class_name)
                && method_body_uses_this_property(&method_info.body)
                && !inherited_property_layout_available(&class_name, &declaring_class, module))
        {
            return None;
        }
        if expected_kind.is_some_and(|kind| kind != method_info.return_kind) {
            return None;
        }
        expected_kind = Some(method_info.return_kind);
        found = true;
    }
    found.then_some(expected_kind?)
}

fn mixed_object_cell_receiver_candidate(object: &Expr, module: &WasmModule) -> bool {
    match &object.kind {
        ExprKind::Variable(name) => module.local_kind(name) == Some(LocalKind::Mixed),
        ExprKind::PropertyAccess { object, property }
        | ExprKind::NullsafePropertyAccess { object, property } => {
            object_property_value_kind(object, property, module) == Some(ValueKind::Mixed)
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            static_property_value_kind(receiver, property, module) == Some(ValueKind::Mixed)
        }
        ExprKind::ArrayAccess { .. } => true,
        _ => false,
    }
}

fn dynamic_object_receiver_candidate(object: &Expr, module: &WasmModule) -> bool {
    matches!(&object.kind, ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Object))
        && object_class_name_for_expr(object, module).is_none()
}

pub(in crate::codegen::wasm) fn object_receiver_needs_runtime_class_id(object: &Expr, module: &WasmModule) -> bool {
    match &object.kind {
        ExprKind::This => module.declared_object_type_for_local("this").is_some(),
        ExprKind::Variable(name) => {
            module.local_kind(name) == Some(LocalKind::Object)
                && module.declared_object_type_for_local(name).is_some()
        }
        _ => false,
    }
}

fn dynamic_object_receiver_declared_type(object: &Expr, module: &WasmModule) -> Option<String> {
    match &object.kind {
        ExprKind::This => module.declared_object_type_for_local("this"),
        ExprKind::Variable(name) => module.declared_object_type_for_local(name),
        _ => None,
    }
}

fn candidate_class_matches_declared_receiver(
    class_name: &str,
    receiver_type: Option<&str>,
    module: &WasmModule,
) -> bool {
    let Some(receiver_type) = receiver_type else {
        return true;
    };
    if module.is_interface_name(receiver_type) {
        let receiver_key = php_symbol_key(receiver_type);
        return static_class_implements_interface_or_parent(class_name, &receiver_key, module);
    }
    let receiver_key = php_symbol_key(receiver_type);
    static_class_implements_interface_or_parent(class_name, &receiver_key, module)
        || static_class_extends_or_equals(class_name, receiver_type, module)
}

fn static_class_implements_interface_or_parent(
    class_name: &str,
    target_key: &str,
    module: &WasmModule,
) -> bool {
    let mut current = Some(class_name.to_string());
    while let Some(class_key) = current {
        let Some(class_info) = module.object_class(&class_key) else {
            return false;
        };
        if class_implements_static_interface(class_info, target_key, module) {
            return true;
        }
        current = class_info.parent.clone();
    }
    false
}

fn static_class_extends_or_equals(class_name: &str, target: &str, module: &WasmModule) -> bool {
    let target_key = php_symbol_key(target);
    let mut current = Some(class_name.to_string());
    while let Some(class_key) = current {
        let Some(class_info) = module.object_class(&class_key) else {
            return false;
        };
        if php_symbol_key(&class_info.name) == target_key {
            return true;
        }
        current = class_info.parent.clone();
    }
    false
}

fn emit_mixed_object_scalar_property_branch(
    object_local: &str,
    class_id_local: &str,
    kind: ObjectPropertyKind,
    candidates: &[(u64, ObjectPropertyInfo)],
    index: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if index == candidates.len() {
        module.body().line("unreachable");
        return Ok(());
    }
    let (class_id, property_info) = &candidates[index];
    module.body().line(&format!("local.get ${}", class_id_local));
    module.body().line(&format!("i64.const {}", class_id));
    module.body().line("i64.eq");
    module
        .body()
        .open(&format!("if {}", wasm_result_for_object_property_kind(kind)));
    module.body().line(&format!("local.get ${}", object_local));
    emit_load_property(property_info, module)?;
    module.body().line("else");
    emit_mixed_object_scalar_property_branch(
        object_local,
        class_id_local,
        kind,
        candidates,
        index + 1,
        module,
    )?;
    module.body().close("end");
    Ok(())
}

fn emit_mixed_object_dynamic_property_branch(
    result: &str,
    object_local: &str,
    class_id_local: &str,
    property_local: &str,
    candidates: &[(u64, ObjectPropertyInfo)],
    index: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if index == candidates.len() {
        module.body().line(&format!("local.get ${}", result));
        module.body().line("call $__rt_value_store_null");
        return Ok(());
    }
    let (class_id, property_info) = &candidates[index];
    module.body().line(&format!("local.get ${}", class_id_local));
    module.body().line(&format!("i64.const {}", class_id));
    module.body().line("i64.eq");
    emit_runtime_string_local_equals_literal(property_local, &property_info.name, module);
    module.body().line("i32.and");
    module.body().open("if");
    module.body().line(&format!("local.get ${}", object_local));
    let kind = emit_load_property(property_info, module)?;
    emit_store_emitted_value_kind(
        &format!("${}", result),
        kind,
        "nullsafe_dynamic_property",
        module,
    )?;
    module.body().line("else");
    emit_mixed_object_dynamic_property_branch(
        result,
        object_local,
        class_id_local,
        property_local,
        candidates,
        index + 1,
        module,
    )?;
    module.body().close("end");
    Ok(())
}

fn emit_mixed_object_dynamic_property_isset_branch(
    object_local: &str,
    class_id_local: &str,
    property_local: &str,
    candidates: &[(u64, ObjectPropertyInfo)],
    index: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if index == candidates.len() {
        module.body().line("i32.const 0");
        return Ok(());
    }
    let (class_id, property_info) = &candidates[index];
    module.body().line(&format!("local.get ${}", class_id_local));
    module.body().line(&format!("i64.const {}", class_id));
    module.body().line("i64.eq");
    emit_runtime_string_local_equals_literal(property_local, &property_info.name, module);
    module.body().line("i32.and");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get ${}", object_local));
    let kind = emit_load_property(property_info, module)?;
    emit_loaded_property_isset(kind, module);
    module.body().line("else");
    emit_mixed_object_dynamic_property_isset_branch(
        object_local,
        class_id_local,
        property_local,
        candidates,
        index + 1,
        module,
    )?;
    module.body().close("end");
    Ok(())
}

fn emit_mixed_object_dynamic_property_empty_branch(
    object_local: &str,
    class_id_local: &str,
    property_local: &str,
    candidates: &[(u64, ObjectPropertyInfo)],
    index: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if index == candidates.len() {
        module.body().line("i32.const 1");
        return Ok(());
    }
    let (class_id, property_info) = &candidates[index];
    module.body().line(&format!("local.get ${}", class_id_local));
    module.body().line(&format!("i64.const {}", class_id));
    module.body().line("i64.eq");
    emit_runtime_string_local_equals_literal(property_local, &property_info.name, module);
    module.body().line("i32.and");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get ${}", object_local));
    emit_load_property(property_info, module)?;
    emit_loaded_property_empty(property_info.kind, module);
    module.body().line("else");
    emit_mixed_object_dynamic_property_empty_branch(
        object_local,
        class_id_local,
        property_local,
        candidates,
        index + 1,
        module,
    )?;
    module.body().close("end");
    Ok(())
}

fn emit_mixed_object_no_arg_method_branch(
    expr: &Expr,
    object_local: &str,
    class_id_local: &str,
    return_kind: ValueKind,
    candidates: &[(u64, String)],
    args: &[Expr],
    index: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if index == candidates.len() {
        module.body().line("unreachable");
        return Ok(());
    }
    let (class_id, method_symbol) = &candidates[index];
    module.body().line(&format!("local.get ${}", class_id_local));
    module.body().line(&format!("i64.const {}", class_id));
    module.body().line("i64.eq");
    if return_kind == ValueKind::Null {
        module.body().open("if");
    } else {
        module
            .body()
            .open(&format!("if {}", wasm_result_for_value_kind(return_kind)));
    }
    let object_arg = Expr {
        kind: ExprKind::Variable(object_local.to_string()),
        span: expr.span,
    };
    let mut call_args = Vec::with_capacity(args.len() + 1);
    call_args.push(object_arg);
    call_args.extend(args.iter().cloned());
    emit_user_function_args(expr, method_symbol, &call_args, module)?;
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(method_symbol)));
    module.body().line("else");
    emit_mixed_object_no_arg_method_branch(
        expr,
        object_local,
        class_id_local,
        return_kind,
        candidates,
        args,
        index + 1,
        module,
    )?;
    module.body().close("end");
    Ok(())
}

fn wasm_result_for_object_property_kind(kind: ObjectPropertyKind) -> &'static str {
    match kind {
        ObjectPropertyKind::Int => "(result i64)",
        ObjectPropertyKind::Float => "(result f64)",
        ObjectPropertyKind::Bool => "(result i32)",
        ObjectPropertyKind::Str => "(result i32 i32)",
        ObjectPropertyKind::Object => "(result i32)",
        ObjectPropertyKind::Mixed => "(result i32)",
    }
}

fn wasm_result_for_value_kind(kind: ValueKind) -> &'static str {
    match kind {
        ValueKind::Int => "(result i64)",
        ValueKind::Float => "(result f64)",
        ValueKind::Bool => "(result i32)",
        ValueKind::Str => "(result i32 i32)",
        ValueKind::Array => "(result i32 i32)",
        ValueKind::Object => "(result i32)",
        ValueKind::Mixed => "(result i32)",
        ValueKind::Null | ValueKind::Never => {
            unreachable!("void/never object-cell method calls do not produce stack results")
        }
    }
}

fn emit_store_method_result_value_kind(
    cell: &str,
    kind: ValueKind,
    label_prefix: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if kind == ValueKind::Null {
        module.body().line(&format!("local.get {}", cell));
        module.body().line("call $__rt_value_store_null");
        return Ok(());
    }
    emit_store_emitted_value_kind(cell, kind, label_prefix, module)
}

fn object_class_match_values(
    target: &str,
    exclude_self: bool,
    expr: &Expr,
    module: &WasmModule,
) -> Result<Vec<(u64, bool)>, CompileError> {
    let mut classes = Vec::new();
    for (class_id, class_name) in module.object_class_names_by_id() {
        classes.push((
            class_id,
            static_object_class_match(&class_name, target, exclude_self, expr, module)?,
        ));
    }
    Ok(classes)
}

fn emit_class_id_match_branch(
    class_id_local: &str,
    classes: &[(u64, bool)],
    index: usize,
    module: &mut WasmModule,
) {
    if index == classes.len() {
        module.body().line("i32.const 0");
        return;
    }
    let (class_id, matches_target) = classes[index];
    module.body().line(&format!("local.get ${}", class_id_local));
    module.body().line(&format!("i64.const {}", class_id));
    module.body().line("i64.eq");
    module.body().open("if (result i32)");
    module.body().line(if matches_target {
        "i32.const 1"
    } else {
        "i32.const 0"
    });
    module.body().line("else");
    emit_class_id_match_branch(class_id_local, classes, index + 1, module);
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn object_class_name_for_expr(
    expr: &Expr,
    module: &WasmModule,
) -> Option<String> {
    match &expr.kind {
        ExprKind::NewObject { class_name, .. } => Some(class_name.as_str().to_string()),
        ExprKind::NewScopedObject { receiver, .. } => module.class_name_for_receiver(receiver),
        ExprKind::FunctionCall { name, .. } => module
            .function_return_object_class(name)
            .filter(|class_name| module.object_class(class_name).is_some()),
        ExprKind::MethodCall { object, method, .. } => {
            let Some(receiver_class) = object_class_name_for_expr(object, module) else {
                if let Some(class_name) =
                    runtime_object_method_declared_return_class(object, method, module)
                {
                    return Some(class_name);
                }
                return mixed_object_method_declared_return_class(object, method, module);
            };
            module
                .function_return_object_class(&method_call_return_key(&receiver_class, method))
                .filter(|class_name| module.object_class(class_name).is_some())
        }
        ExprKind::NullsafeMethodCall { object, method, .. } => {
            if !object_expr_is_known_non_null(object, module) {
                return None;
            }
            let Some(receiver_class) = object_class_name_for_expr(object, module) else {
                if let Some(class_name) =
                    runtime_object_method_declared_return_class(object, method, module)
                {
                    return Some(class_name);
                }
                return mixed_object_method_declared_return_class(object, method, module);
            };
            module
                .function_return_object_class(&method_call_return_key(&receiver_class, method))
                .filter(|class_name| module.object_class(class_name).is_some())
        }
        ExprKind::StaticMethodCall { receiver, method, .. } => {
            let receiver_class = module.class_name_for_receiver(receiver)?;
            if matches_static_backed_enum_object_lookup(expr, &receiver_class, method, module) {
                return Some(receiver_class);
            }
            module
                .function_return_object_class(&static_method_call_return_key(
                    &receiver_class,
                    method,
                ))
                .filter(|class_name| module.object_class(class_name).is_some())
        }
        ExprKind::ScopedConstantAccess { receiver, name } => {
            module.enum_case_class(receiver, name)
        }
        ExprKind::StaticPropertyAccess { receiver, property } => module
            .object_static_property(receiver, property)
            .and_then(|property_info| property_info.declared_class),
        ExprKind::This => module.object_class_for_local("this"),
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Object) => {
            module.object_class_for_local(name)
        }
        ExprKind::ArrayAccess { array, index } => {
            match &array.kind {
                ExprKind::Variable(name) if module.array_layout(name) == ArrayLayout::Assoc => {
                    assoc_array_object_class_for_index_expr(name, index, module)
                }
                ExprKind::Variable(name) => {
                    let index = static_or_const_int_value(index).and_then(|value| usize::try_from(value).ok())?;
                    module.array_object_class(name, index)
                }
                ExprKind::StaticMethodCall {
                    receiver,
                    method,
                    args,
                } if method.eq_ignore_ascii_case("cases") && args.is_empty() => {
                    let index = static_or_const_int_value(index).and_then(|value| usize::try_from(value).ok())?;
                    let class_name = module.class_name_for_receiver(receiver)?;
                    let cases = module.enum_case_names(&class_name)?;
                    (index < cases.len()).then_some(class_name)
                }
                _ => None,
            }
        }
        ExprKind::PropertyAccess { object, property }
        | ExprKind::NullsafePropertyAccess { object, property } => {
            let Some(class_name) = object_class_name_for_expr(object, module) else {
                return mixed_object_property_declared_class(object, property, module);
            };
            let class_info = module.object_class(&class_name)?;
            class_info
                .properties
                .iter()
                .find(|candidate| candidate.name == *property)
                .and_then(|property_info| property_info.declared_class.clone())
        }
        _ => None,
    }
}

fn assoc_array_object_class_for_index_expr(
    name: &str,
    index: &Expr,
    module: &WasmModule,
) -> Option<String> {
    let key = static_or_const_int_value(index)
        .map(AssocKeyValue::Int)
        .or_else(|| static_string_value(index, module).map(AssocKeyValue::Str))?;
    let keys = module.array_key_values(name)?;
    keys.iter()
        .enumerate()
        .rev()
        .find_map(|(entry_index, candidate)| {
            (candidate == &key)
                .then(|| module.array_object_class(name, entry_index))
                .flatten()
        })
}

fn matches_static_backed_enum_object_lookup(
    expr: &Expr,
    receiver_class: &str,
    method: &str,
    module: &WasmModule,
) -> bool {
    if !method.eq_ignore_ascii_case("from") && !method.eq_ignore_ascii_case("tryFrom") {
        return false;
    }
    let ExprKind::StaticMethodCall { args, .. } = &expr.kind else {
        return false;
    };
    let Some(arg) = args.first() else {
        return false;
    };
    let backing_value = static_or_module_const_int_value(arg, module)
        .map(EnumCaseBackingValue::Int)
        .or_else(|| static_string_value(arg, module).map(EnumCaseBackingValue::Str));
    backing_value
        .as_ref()
        .and_then(|value| module.enum_case_name_for_backing_value(receiver_class, value))
        .is_some()
}

fn mixed_object_property_declared_class(
    object: &Expr,
    property: &str,
    module: &WasmModule,
) -> Option<String> {
    if !mixed_object_cell_receiver_candidate(object, module) {
        return None;
    }
    let mut expected_class: Option<String> = None;
    let mut found = false;
    for (_, class_name) in module.object_class_names_by_id() {
        let Some(class_info) = module.object_class(&class_name) else {
            continue;
        };
        let Some(property_info) = class_info
            .properties
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(property))
        else {
            continue;
        };
        if !module.object_member_is_accessible(&property_info.owner_class, &property_info.visibility)
            || property_info.kind != ObjectPropertyKind::Object
        {
            return None;
        }
        let declared_class = property_info.declared_class.clone()?;
        if expected_class
            .as_ref()
            .is_some_and(|expected| !expected.eq_ignore_ascii_case(&declared_class))
        {
            return None;
        }
        expected_class = Some(declared_class);
        found = true;
    }
    found.then_some(expected_class?)
}

fn mixed_object_method_declared_return_class(
    object: &Expr,
    method: &str,
    module: &WasmModule,
) -> Option<String> {
    if !mixed_object_cell_receiver_candidate(object, module) {
        return None;
    }
    let mut expected_class: Option<String> = None;
    let mut found = false;
    for (_, class_name) in module.object_class_names_by_id() {
        let Some((declaring_class, method_info)) =
            module.object_method_in_hierarchy(&class_name, method)
        else {
            continue;
        };
        if method_info.return_kind != ValueKind::Object
            || !module.object_member_is_accessible(&method_info.owner_class, &method_info.visibility)
            || (!declaring_class.eq_ignore_ascii_case(&class_name)
                && method_body_uses_this_property(&method_info.body)
                && !inherited_property_layout_available(&class_name, &declaring_class, module))
        {
            return None;
        }
        let return_class = module.function_return_object_class(&method_info.symbol)?;
        if expected_class
            .as_ref()
            .is_some_and(|expected| !expected.eq_ignore_ascii_case(&return_class))
        {
            return None;
        }
        expected_class = Some(return_class);
        found = true;
    }
    found.then_some(expected_class?)
}

fn runtime_object_method_declared_return_class(
    object: &Expr,
    method: &str,
    module: &WasmModule,
) -> Option<String> {
    let receiver_type = dynamic_object_receiver_declared_type(object, module)?;
    let mut expected_class: Option<String> = None;
    let mut found = false;
    for (_, class_name) in module.object_class_names_by_id() {
        if !candidate_class_matches_declared_receiver(&class_name, Some(&receiver_type), module) {
            continue;
        }
        let Some((declaring_class, method_info)) =
            module.object_method_in_hierarchy(&class_name, method)
        else {
            continue;
        };
        if method_info.return_kind != ValueKind::Object
            || !module.object_member_is_accessible(&method_info.owner_class, &method_info.visibility)
            || (!declaring_class.eq_ignore_ascii_case(&class_name)
                && method_body_uses_this_property(&method_info.body)
                && !inherited_property_layout_available(&class_name, &declaring_class, module))
        {
            return None;
        }
        let return_class = module.function_return_object_class(&method_info.symbol)?;
        if expected_class
            .as_ref()
            .is_some_and(|expected| !expected.eq_ignore_ascii_case(&return_class))
        {
            return None;
        }
        expected_class = Some(return_class);
        found = true;
    }
    found.then_some(expected_class?)
}

pub(in crate::codegen::wasm) fn object_expr_is_known_non_null(
    expr: &Expr,
    module: &WasmModule,
) -> bool {
    match &expr.kind {
        ExprKind::NewObject { .. } | ExprKind::NewScopedObject { .. } => true,
        ExprKind::This => module.local_kind("this") == Some(LocalKind::Object),
        ExprKind::Variable(name) => module.local_kind(name) == Some(LocalKind::Object),
        ExprKind::FunctionCall { name, .. } => {
            module.function_return_kind(name) == Some(ValueKind::Object)
                && !module.function_return_is_nullable(name)
        }
        ExprKind::MethodCall { object, method, .. } => {
            let Some(class_name) = object_class_name_for_expr(object, module) else {
                return false;
            };
            method_call_return_kind(object, method, module) == Some(ValueKind::Object)
                && module
                    .function_return_object_class(&method_call_return_key(&class_name, method))
                    .is_some()
        }
        ExprKind::NullsafeMethodCall { object, method, .. } => {
            if !object_expr_is_known_non_null(object, module) {
                return false;
            }
            let Some(class_name) = object_class_name_for_expr(object, module) else {
                return false;
            };
            method_call_return_kind(object, method, module) == Some(ValueKind::Object)
                && module
                    .function_return_object_class(&method_call_return_key(&class_name, method))
                    .is_some()
        }
        ExprKind::StaticMethodCall { receiver, method, .. } => {
            static_method_call_return_kind(receiver, method, module) == Some(ValueKind::Object)
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            static_property_value_kind(receiver, property, module) == Some(ValueKind::Object)
        }
        ExprKind::PropertyAccess { object, property }
        | ExprKind::NullsafePropertyAccess { object, property } => {
            object_property_value_kind(object, property, module) == Some(ValueKind::Object)
        }
        _ => false,
    }
}

fn emit_exact_object_class_match(
    expr: &Expr,
    value: &Expr,
    target: &str,
    exclude_self: bool,
    target_expr: Option<&Expr>,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let value_class = object_class_name_for_expr(value, module);
    let kind = emit_expr(value, module)?;
    if let Some(target_expr) = target_expr {
        emit_static_class_string_side_effect(target_expr, module)?;
    }
    if kind != ValueKind::Object {
        emit_drop_value_kind(kind, module);
        module.body().line("i32.const 0");
        return Ok(ValueKind::Bool);
    }
    if object_receiver_needs_runtime_class_id(value, module) {
        return emit_dynamic_object_class_match(expr, value, target, exclude_self, None, module);
    }
    let Some(value_class) = value_class else {
        if dynamic_object_receiver_candidate(value, module) {
            return emit_dynamic_object_class_match(expr, value, target, exclude_self, None, module);
        }
        module.body().line("drop");
        return Err(CompileError::new(
            value.span,
            "wasm32-web object class check requires a statically known object class",
        ));
    };
    module.body().line("drop");
    if static_object_class_match(&value_class, target, exclude_self, expr, module)? {
        module.body().line("i32.const 1");
        return Ok(ValueKind::Bool);
    }
    module.body().line("i32.const 0");
    Ok(ValueKind::Bool)
}

fn emit_dynamic_object_class_match(
    expr: &Expr,
    value: &Expr,
    target: &str,
    exclude_self: bool,
    target_expr: Option<&Expr>,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if let Some(target_expr) = target_expr {
        emit_static_class_string_side_effect(target_expr, module)?;
    }
    let receiver_type = dynamic_object_receiver_declared_type(value, module);
    let candidates =
        dynamic_object_class_match_candidates(expr, target, exclude_self, receiver_type.as_deref(), module)?;
    if candidates.is_empty() {
        module.body().line("drop");
        module.body().line("i32.const 0");
        return Ok(ValueKind::Bool);
    }
    let object_local = module
        .next_label("dynamic_object_class_check_object")
        .trim_start_matches('$')
        .to_string();
    let class_id_local = module
        .next_label("dynamic_object_class_check_class_id")
        .trim_start_matches('$')
        .to_string();
    let result_local = module
        .next_label("dynamic_object_class_check_result")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(object_local.clone());
    module.declare_i64_local(class_id_local.clone());
    module.declare_i32_local(result_local.clone());
    module.body().line(&format!("local.set ${}", object_local));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set ${}", result_local));
    module.body().line(&format!("local.get ${}", object_local));
    module.body().line("i64.load");
    module.body().line(&format!("local.set ${}", class_id_local));
    for class_id in candidates {
        module.body().line(&format!("local.get ${}", class_id_local));
        module.body().line(&format!("i64.const {}", class_id));
        module.body().line("i64.eq");
        module.body().open("if");
        module.body().line("i32.const 1");
        module.body().line(&format!("local.set ${}", result_local));
        module.body().close("end");
    }
    module.body().line(&format!("local.get ${}", result_local));
    Ok(ValueKind::Bool)
}

fn dynamic_object_class_match_candidates(
    expr: &Expr,
    target: &str,
    exclude_self: bool,
    receiver_type: Option<&str>,
    module: &WasmModule,
) -> Result<Vec<u64>, CompileError> {
    let mut candidates = Vec::new();
    for (class_id, class_name) in module.object_class_names_by_id() {
        if !candidate_class_matches_declared_receiver(&class_name, receiver_type, module) {
            continue;
        }
        if static_object_class_match(&class_name, target, exclude_self, expr, module)? {
            candidates.push(class_id);
        }
    }
    Ok(candidates)
}

fn static_object_class_match(
    value_class: &str,
    target: &str,
    exclude_self: bool,
    expr: &Expr,
    module: &WasmModule,
) -> Result<bool, CompileError> {
    let target_key = target.to_ascii_lowercase();
    if value_class.eq_ignore_ascii_case(&target_key) && !exclude_self {
        return Ok(true);
    }
    let Some(class_info) = module.object_class(&value_class) else {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web object class check requires a declared object class",
        ));
    };
    if class_implements_static_interface(class_info, &target_key, module) {
        return Ok(true);
    }
    let mut current = class_info.parent.clone();
    while let Some(class_key) = current {
        if class_key == target_key {
            return Ok(true);
        }
        let Some(parent_info) = module.object_class(&class_key) else {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web inherited object class checks require declared parent classes",
            ));
        };
        if class_implements_static_interface(parent_info, &target_key, module) {
            return Ok(true);
        }
        current = parent_info.parent.clone();
    }
    Ok(false)
}

fn static_class_string_object_match(
    value_class: &str,
    target: &str,
    exclude_self: bool,
    expr: &Expr,
    module: &WasmModule,
) -> Result<bool, CompileError> {
    if module.object_class(value_class).is_none() {
        return Ok(false);
    }
    static_object_class_match(value_class, target, exclude_self, expr, module)
}

fn static_class_string_value(expr: &Expr, module: &WasmModule) -> Option<String> {
    match &expr.kind {
        ExprKind::Variable(name) => module.string_static_value(name),
        ExprKind::FunctionCall { name, args } => module
            .function_static_string_return_for_call(name, args)
            .or_else(|| module.function_static_string_return(name)),
        _ => static_string_value(expr, module),
    }
}

fn static_object_property_name(expr: &Expr, module: &WasmModule) -> Option<String> {
    match &expr.kind {
        ExprKind::Variable(name) => module.string_static_value(name),
        _ => static_string_value(expr, module),
    }
}

fn emit_runtime_dynamic_property_read_branch(
    object_local: &str,
    property_local: &str,
    kind: ObjectPropertyKind,
    properties: &[ObjectPropertyInfo],
    index: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if index >= properties.len() {
        module.body().line("unreachable");
        return Ok(());
    }
    let property_info = &properties[index];
    emit_runtime_string_local_equals_literal(property_local, &property_info.name, module);
    module
        .body()
        .open(&format!("if {}", wasm_result_for_object_property_kind(kind)));
    module.body().line(&format!("local.get ${}", object_local));
    emit_load_property(property_info, module)?;
    module.body().line("else");
    emit_runtime_dynamic_property_read_branch(
        object_local,
        property_local,
        kind,
        properties,
        index + 1,
        module,
    )?;
    module.body().close("end");
    Ok(())
}

fn emit_dynamic_object_runtime_property_read_branch(
    object_local: &str,
    class_id_local: &str,
    property_local: &str,
    kind: ObjectPropertyKind,
    candidates: &[(u64, ObjectPropertyInfo)],
    index: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if index == candidates.len() {
        module.body().line("unreachable");
        return Ok(());
    }
    let (class_id, property_info) = &candidates[index];
    module.body().line(&format!("local.get ${}", class_id_local));
    module.body().line(&format!("i64.const {}", class_id));
    module.body().line("i64.eq");
    emit_runtime_string_local_equals_literal(property_local, &property_info.name, module);
    module.body().line("i32.and");
    module
        .body()
        .open(&format!("if {}", wasm_result_for_object_property_kind(kind)));
    module.body().line(&format!("local.get ${}", object_local));
    emit_load_property(property_info, module)?;
    module.body().line("else");
    emit_dynamic_object_runtime_property_read_branch(
        object_local,
        class_id_local,
        property_local,
        kind,
        candidates,
        index + 1,
        module,
    )?;
    module.body().close("end");
    Ok(())
}

fn emit_runtime_dynamic_property_isset_branch(
    object_local: &str,
    property_local: &str,
    properties: &[ObjectPropertyInfo],
    index: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if index >= properties.len() {
        module.body().line("i32.const 0");
        return Ok(());
    }
    let property_info = &properties[index];
    emit_runtime_string_local_equals_literal(property_local, &property_info.name, module);
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get ${}", object_local));
    let kind = emit_load_property(property_info, module)?;
    emit_loaded_property_isset(kind, module);
    module.body().line("else");
    emit_runtime_dynamic_property_isset_branch(
        object_local,
        property_local,
        properties,
        index + 1,
        module,
    )?;
    module.body().close("end");
    Ok(())
}

fn emit_runtime_dynamic_property_empty_branch(
    object_local: &str,
    property_local: &str,
    properties: &[ObjectPropertyInfo],
    index: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if index >= properties.len() {
        module.body().line("i32.const 1");
        return Ok(());
    }
    let property_info = &properties[index];
    emit_runtime_string_local_equals_literal(property_local, &property_info.name, module);
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get ${}", object_local));
    emit_load_property(property_info, module)?;
    emit_loaded_property_empty(property_info.kind, module);
    module.body().line("else");
    emit_runtime_dynamic_property_empty_branch(
        object_local,
        property_local,
        properties,
        index + 1,
        module,
    )?;
    module.body().close("end");
    Ok(())
}

fn emit_runtime_dynamic_property_write_branch(
    object_local: &str,
    property_local: &str,
    properties: &[ObjectPropertyInfo],
    index: usize,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if index >= properties.len() {
        module.body().line("unreachable");
        return Ok(());
    }
    let property_info = &properties[index];
    emit_runtime_string_local_equals_literal(property_local, &property_info.name, module);
    module.body().open("if");
    emit_store_property_value(object_local, property_info, value, module)?;
    module.body().line("else");
    emit_runtime_dynamic_property_write_branch(
        object_local,
        property_local,
        properties,
        index + 1,
        value,
        module,
    )?;
    module.body().close("end");
    Ok(())
}

fn emit_dynamic_object_scalar_property_write_branch(
    object_local: &str,
    class_id_local: &str,
    candidates: &[(u64, ObjectPropertyInfo)],
    index: usize,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if index >= candidates.len() {
        module.body().line("unreachable");
        return Ok(());
    }
    let (class_id, property_info) = &candidates[index];
    module.body().line(&format!("local.get ${}", class_id_local));
    module.body().line(&format!("i64.const {}", class_id));
    module.body().line("i64.eq");
    module.body().open("if");
    emit_store_property_value(object_local, property_info, value, module)?;
    module.body().line("else");
    emit_dynamic_object_scalar_property_write_branch(
        object_local,
        class_id_local,
        candidates,
        index + 1,
        value,
        module,
    )?;
    module.body().close("end");
    Ok(())
}

fn emit_dynamic_object_runtime_property_write_branch(
    object_local: &str,
    class_id_local: &str,
    property_local: &str,
    candidates: &[(u64, ObjectPropertyInfo)],
    index: usize,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if index >= candidates.len() {
        module.body().line("unreachable");
        return Ok(());
    }
    let (class_id, property_info) = &candidates[index];
    module.body().line(&format!("local.get ${}", class_id_local));
    module.body().line(&format!("i64.const {}", class_id));
    module.body().line("i64.eq");
    emit_runtime_string_local_equals_literal(property_local, &property_info.name, module);
    module.body().line("i32.and");
    module.body().open("if");
    emit_store_property_value(object_local, property_info, value, module)?;
    module.body().line("else");
    emit_dynamic_object_runtime_property_write_branch(
        object_local,
        class_id_local,
        property_local,
        candidates,
        index + 1,
        value,
        module,
    )?;
    module.body().close("end");
    Ok(())
}

fn emit_runtime_dynamic_property_assignment_expr_branch(
    expr: &Expr,
    object_local: &str,
    property_local: &str,
    kind: ObjectPropertyKind,
    properties: &[ObjectPropertyInfo],
    index: usize,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if index >= properties.len() {
        module.body().line("unreachable");
        return Ok(());
    }
    let property_info = &properties[index];
    emit_runtime_string_local_equals_literal(property_local, &property_info.name, module);
    module
        .body()
        .open(&format!("if {}", wasm_result_for_object_property_kind(kind)));
    emit_store_property_value(object_local, property_info, value, module)?;
    module.body().line(&format!("local.get ${}", object_local));
    emit_load_property(property_info, module).map_err(|err| CompileError::new(expr.span, &err.message))?;
    module.body().line("else");
    emit_runtime_dynamic_property_assignment_expr_branch(
        expr,
        object_local,
        property_local,
        kind,
        properties,
        index + 1,
        value,
        module,
    )?;
    module.body().close("end");
    Ok(())
}

fn emit_dynamic_object_runtime_property_assignment_expr_branch(
    expr: &Expr,
    object_local: &str,
    class_id_local: &str,
    property_local: &str,
    kind: ObjectPropertyKind,
    candidates: &[(u64, ObjectPropertyInfo)],
    index: usize,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if index >= candidates.len() {
        module.body().line("unreachable");
        return Ok(());
    }
    let (class_id, property_info) = &candidates[index];
    module.body().line(&format!("local.get ${}", class_id_local));
    module.body().line(&format!("i64.const {}", class_id));
    module.body().line("i64.eq");
    emit_runtime_string_local_equals_literal(property_local, &property_info.name, module);
    module.body().line("i32.and");
    module
        .body()
        .open(&format!("if {}", wasm_result_for_object_property_kind(kind)));
    emit_store_property_value(object_local, property_info, value, module)?;
    module.body().line(&format!("local.get ${}", object_local));
    emit_load_property(property_info, module).map_err(|err| CompileError::new(expr.span, &err.message))?;
    module.body().line("else");
    emit_dynamic_object_runtime_property_assignment_expr_branch(
        expr,
        object_local,
        class_id_local,
        property_local,
        kind,
        candidates,
        index + 1,
        value,
        module,
    )?;
    module.body().close("end");
    Ok(())
}

fn emit_dynamic_object_scalar_property_assignment_expr_branch(
    expr: &Expr,
    object_local: &str,
    class_id_local: &str,
    kind: ObjectPropertyKind,
    candidates: &[(u64, ObjectPropertyInfo)],
    index: usize,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if index >= candidates.len() {
        module.body().line("unreachable");
        return Ok(());
    }
    let (class_id, property_info) = &candidates[index];
    module.body().line(&format!("local.get ${}", class_id_local));
    module.body().line(&format!("i64.const {}", class_id));
    module.body().line("i64.eq");
    module
        .body()
        .open(&format!("if {}", wasm_result_for_object_property_kind(kind)));
    emit_store_property_value(object_local, property_info, value, module)?;
    module.body().line(&format!("local.get ${}", object_local));
    emit_load_property(property_info, module).map_err(|err| CompileError::new(expr.span, &err.message))?;
    module.body().line("else");
    emit_dynamic_object_scalar_property_assignment_expr_branch(
        expr,
        object_local,
        class_id_local,
        kind,
        candidates,
        index + 1,
        value,
        module,
    )?;
    module.body().close("end");
    Ok(())
}

fn emit_loaded_property_isset(kind: ValueKind, module: &mut WasmModule) {
    match kind {
        ValueKind::Mixed => {
            module.body().line("call $__rt_mixed_tag");
            module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
            module.body().line("i32.ne");
        }
        ValueKind::Null => {
            module.body().line("drop");
            module.body().line("i32.const 0");
        }
        _ => {
            emit_drop_value_kind(kind, module);
            module.body().line("i32.const 1");
        }
    }
}

fn emit_loaded_property_empty(kind: ObjectPropertyKind, module: &mut WasmModule) {
    match kind {
        ObjectPropertyKind::Int => {
            module.body().line("i64.const 0");
            module.body().line("i64.eq");
        }
        ObjectPropertyKind::Float => {
            module.body().line("f64.const 0");
            module.body().line("f64.eq");
        }
        ObjectPropertyKind::Bool => {
            module.body().line("i32.eqz");
        }
        ObjectPropertyKind::Str => {
            let local = module
                .next_label("dynamic_property_empty_str")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(format!("{}_ptr", local));
            module.declare_i32_local(format!("{}_len", local));
            module.body().line(&format!("local.set ${}_len", local));
            module.body().line(&format!("local.set ${}_ptr", local));
            emit_string_local_truthiness(&local, module);
            module.body().line("i32.eqz");
        }
        ObjectPropertyKind::Object => {
            module.body().line("drop");
            module.body().line("i32.const 0");
        }
        ObjectPropertyKind::Mixed => {
            module.body().line("call $__rt_mixed_truthy");
            module.body().line("i32.eqz");
        }
    }
}

fn emit_runtime_string_local_equals_literal(
    local: &str,
    literal: &str,
    module: &mut WasmModule,
) {
    let idx = module.next_label("dynamic_property_name_idx");
    let matched = module.next_label("dynamic_property_name_match");
    let done = module.next_label("dynamic_property_name_done");
    let loop_label = module.next_label("dynamic_property_name_loop");
    let (literal_ptr, literal_len) = module.intern_string(literal);
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(matched.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("local.get ${}_len", local));
    module.body().line(&format!("i32.const {}", literal_len));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", matched));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", done));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", literal_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done));
    module.body().line(&format!("local.get ${}_ptr", local));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("i32.const {}", literal_ptr));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("br {}", done));
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", matched));
}

fn evaluated_static_class_string_value(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<Option<String>, CompileError> {
    let Some(value) = static_class_string_value(expr, module) else {
        return Ok(None);
    };
    if matches!(expr.kind, ExprKind::FunctionCall { .. }) {
        emit_static_class_string_side_effect(expr, module)?;
    }
    Ok(Some(value))
}

fn emit_static_class_string_side_effect(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if matches!(expr.kind, ExprKind::FunctionCall { .. }) {
        emit_string_value_to_stack(expr, module)?;
        module.body().line("drop");
        module.body().line("drop");
    }
    Ok(())
}

fn class_implements_static_interface(
    class_info: &ObjectClassInfo,
    target_key: &str,
    module: &WasmModule,
) -> bool {
    class_info
        .interfaces
        .iter()
        .any(|interface| interface == target_key || module.interface_extends(interface, target_key))
}

fn emit_drop_value_kind(kind: ValueKind, module: &mut WasmModule) {
    match kind {
        ValueKind::Str | ValueKind::Array => {
            module.body().line("drop");
            module.body().line("drop");
        }
        ValueKind::Int
        | ValueKind::Float
        | ValueKind::Bool
        | ValueKind::Object
        | ValueKind::Mixed
        | ValueKind::Null => {
            module.body().line("drop");
        }
        ValueKind::Never => {}
    }
}

fn emit_alloc_object(
    class_info: &ObjectClassInfo,
    span: Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let object_size = 8 + class_info.properties.len() * 24;
    let object_local = module
        .next_label("object_alloc")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(object_local.clone());
    module.body().line(&format!("i32.const {}", object_size));
    module.body().line("call $__rt_alloc_bytes");
    module.body().line(&format!("i32.const {}", WASM_OBJECT_HEAP_KIND));
    module.body().line("call $__rt_heap_set_kind");
    module.body().line(&format!("local.set ${}", object_local));
    module.body().line(&format!("local.get ${}", object_local));
    module.body().line(&format!("i64.const {}", class_info.class_id));
    module.body().line("i64.store");
    for property in &class_info.properties {
        emit_zero_property_slot(&object_local, property.offset, module);
        if let Some(initialized_offset) = property.initialized_offset {
            emit_zero_property_initialized(&object_local, initialized_offset, module);
        }
    }
    for property in &class_info.properties {
        if let Some(default) = &property.default {
            emit_property_default(&object_local, property, default, span, module)?;
        }
    }
    module.body().line(&format!("local.get ${}", object_local));
    Ok(())
}

fn emit_zero_property_slot(object_local: &str, offset: usize, module: &mut WasmModule) {
    module.body().line(&format!("local.get ${}", object_local));
    module.body().line(&format!("i32.const {}", offset));
    module.body().line("i32.add");
    module.body().line("i64.const 0");
    module.body().line("i64.store");
    module.body().line(&format!("local.get ${}", object_local));
    module.body().line(&format!("i32.const {}", offset + 8));
    module.body().line("i32.add");
    module.body().line("i64.const 0");
    module.body().line("i64.store");
}

fn emit_zero_property_initialized(object_local: &str, offset: usize, module: &mut WasmModule) {
    module.body().line(&format!("local.get ${}", object_local));
    module.body().line("i32.const 0");
    module.body().line(&format!("i32.store offset={}", offset));
}

fn emit_mark_property_initialized(
    object_local: &str,
    property: &ObjectPropertyInfo,
    module: &mut WasmModule,
) {
    let Some(initialized_offset) = property.initialized_offset else {
        return;
    };
    module.body().line(&format!("local.get ${}", object_local));
    module.body().line("i32.const 1");
    module
        .body()
        .line(&format!("i32.store offset={}", initialized_offset));
}

fn emit_property_default(
    object_local: &str,
    property: &ObjectPropertyInfo,
    default: &Expr,
    span: Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match property.kind {
        ObjectPropertyKind::Int => {
            require_int(default, module)?;
            emit_store_i64_property(object_local, property.offset, module);
        }
        ObjectPropertyKind::Bool => {
            emit_condition(default, module)?;
            module.body().line("i64.extend_i32_u");
            emit_store_i64_property(object_local, property.offset, module);
        }
        ObjectPropertyKind::Float => {
            require_float(default, module)?;
            emit_store_f64_property(object_local, property.offset, module);
        }
        ObjectPropertyKind::Str => {
            emit_string_value_to_stack(default, module)?;
            emit_store_i32_pair_property(object_local, property.offset, module);
        }
        ObjectPropertyKind::Object => {
            if emit_expr(default, module)? != ValueKind::Object {
                return Err(CompileError::new(span, "wasm32-web object property default expected object"));
            }
            emit_store_i32_property(object_local, property.offset, module);
        }
        ObjectPropertyKind::Mixed => {
            emit_store_mixed_property_default(object_local, property.offset, default, span, module)?;
        }
    }
    emit_mark_property_initialized(object_local, property, module);
    Ok(())
}

fn emit_store_property_value(
    object_local: &str,
    property: &ObjectPropertyInfo,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match property.kind {
        ObjectPropertyKind::Int => {
            require_int(value, module)?;
            emit_store_i64_property(object_local, property.offset, module);
        }
        ObjectPropertyKind::Bool => {
            emit_condition(value, module)?;
            module.body().line("i64.extend_i32_u");
            emit_store_i64_property(object_local, property.offset, module);
        }
        ObjectPropertyKind::Float => {
            require_float(value, module)?;
            emit_store_f64_property(object_local, property.offset, module);
        }
        ObjectPropertyKind::Str => {
            emit_string_value_to_stack(value, module)?;
            emit_store_i32_pair_property(object_local, property.offset, module);
        }
        ObjectPropertyKind::Object => {
            if emit_expr(value, module)? != ValueKind::Object {
                return Err(CompileError::new(
                    value.span,
                    "wasm32-web object property write expected object value",
                ));
            }
            emit_store_i32_property(object_local, property.offset, module);
        }
        ObjectPropertyKind::Mixed => {
            emit_store_mixed_property_value(object_local, property.offset, value, module)?;
        }
    }
    emit_mark_property_initialized(object_local, property, module);
    Ok(())
}

fn emit_load_static_property(
    property: &ObjectStaticPropertyInfo,
    span: Span,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    emit_static_property_initialized_guard(property, module);
    match property.kind {
        ObjectPropertyKind::Int => {
            module.body().line(&format!("global.get ${}", property.global));
            Ok(ValueKind::Int)
        }
        ObjectPropertyKind::Float => {
            module.body().line(&format!("global.get ${}", property.global));
            Ok(ValueKind::Float)
        }
        ObjectPropertyKind::Bool => {
            module.body().line(&format!("global.get ${}", property.global));
            Ok(ValueKind::Bool)
        }
        ObjectPropertyKind::Str => {
            module
                .body()
                .line(&format!("global.get ${}_ptr", property.global));
            module
                .body()
                .line(&format!("global.get ${}_len", property.global));
            Ok(ValueKind::Str)
        }
        ObjectPropertyKind::Mixed => {
            emit_ensure_static_mixed_property_cell(property, span, module)?;
            Ok(ValueKind::Mixed)
        }
        ObjectPropertyKind::Object => {
            module.body().line(&format!("global.get ${}", property.global));
            Ok(ValueKind::Object)
        }
    }
}

fn emit_store_static_property(
    property: &ObjectStaticPropertyInfo,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match property.kind {
        ObjectPropertyKind::Int => {
            require_int(value, module)?;
            module.body().line(&format!("global.set ${}", property.global));
            emit_mark_static_property_initialized(property, module);
            Ok(())
        }
        ObjectPropertyKind::Float => {
            require_float(value, module)?;
            module.body().line(&format!("global.set ${}", property.global));
            emit_mark_static_property_initialized(property, module);
            Ok(())
        }
        ObjectPropertyKind::Bool => {
            emit_condition(value, module)?;
            module.body().line(&format!("global.set ${}", property.global));
            emit_mark_static_property_initialized(property, module);
            Ok(())
        }
        ObjectPropertyKind::Str => {
            let ptr_local = module
                .next_label("static_prop_str_ptr")
                .trim_start_matches('$')
                .to_string();
            let len_local = module
                .next_label("static_prop_str_len")
                .trim_start_matches('$')
                .to_string();
            module.declare_i32_local(ptr_local.clone());
            module.declare_i32_local(len_local.clone());
            emit_string_value_to_stack(value, module)?;
            module.body().line(&format!("local.set ${}", len_local));
            module.body().line(&format!("local.set ${}", ptr_local));
            module.body().line(&format!("local.get ${}", ptr_local));
            module
                .body()
                .line(&format!("global.set ${}_ptr", property.global));
            module.body().line(&format!("local.get ${}", len_local));
            module
                .body()
                .line(&format!("global.set ${}_len", property.global));
            emit_mark_static_property_initialized(property, module);
            Ok(())
        }
        ObjectPropertyKind::Mixed => {
            emit_store_static_mixed_property_value(property, value, module)?;
            emit_mark_static_property_initialized(property, module);
            Ok(())
        }
        ObjectPropertyKind::Object => {
            if emit_expr(value, module)? != ValueKind::Object {
                return Err(CompileError::new(
                    value.span,
                    "wasm32-web static object property writes expected object value",
                ));
            }
            module.body().line(&format!("global.set ${}", property.global));
            emit_mark_static_property_initialized(property, module);
            Ok(())
        }
    }
}

fn emit_static_property_initialized_guard(
    property: &ObjectStaticPropertyInfo,
    module: &mut WasmModule,
) {
    if !property.needs_initialized_guard {
        return;
    }
    module
        .body()
        .line(&format!("global.get ${}_initialized", property.global));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
}

fn emit_mark_static_property_initialized(
    property: &ObjectStaticPropertyInfo,
    module: &mut WasmModule,
) {
    if !property.needs_initialized_guard {
        return;
    }
    module.body().line("i32.const 1");
    module
        .body()
        .line(&format!("global.set ${}_initialized", property.global));
}

fn emit_ensure_static_mixed_property_cell(
    property: &ObjectStaticPropertyInfo,
    span: Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let cell = module
        .next_label("static_mixed_cell")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(cell.clone());
    module.body().line(&format!("global.get ${}", property.global));
    module.body().line(&format!("local.set ${}", cell));
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_static_mixed_property_default(&cell, property, span, module)?;
    module.body().line(&format!("local.get ${}", cell));
    module.body().line(&format!("global.set ${}", property.global));
    module.body().close("end");
    module.body().line(&format!("local.get ${}", cell));
    Ok(())
}

fn emit_static_mixed_property_default(
    cell: &str,
    property: &ObjectStaticPropertyInfo,
    span: Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_alloc_mixed_cell(cell, module);
    if let Some(default) = &property.default {
        emit_store_value_cell(&format!("${}", cell), default, module)
            .map_err(|err| CompileError::new(span, &err.message))?;
    } else {
        module.body().line(&format!("local.get ${}", cell));
        module.body().line("call $__rt_value_store_null");
    }
    Ok(())
}

fn emit_store_static_mixed_property_value(
    property: &ObjectStaticPropertyInfo,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let cell = module
        .next_label("static_mixed_write")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(cell.clone());
    emit_ensure_static_mixed_property_cell(property, value.span, module)?;
    module.body().line(&format!("local.set ${}", cell));
    emit_store_value_cell(&format!("${}", cell), value, module)
}

fn emit_load_property(
    property: &ObjectPropertyInfo,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    emit_property_initialized_guard(property, module);
    match property.kind {
        ObjectPropertyKind::Int => {
            emit_load_i64_property(property.offset, module);
            Ok(ValueKind::Int)
        }
        ObjectPropertyKind::Bool => {
            emit_load_i64_property(property.offset, module);
            module.body().line("i64.const 0");
            module.body().line("i64.ne");
            Ok(ValueKind::Bool)
        }
        ObjectPropertyKind::Float => {
            emit_load_f64_property(property.offset, module);
            Ok(ValueKind::Float)
        }
        ObjectPropertyKind::Str => {
            emit_load_i32_pair_property(property.offset, module);
            Ok(ValueKind::Str)
        }
        ObjectPropertyKind::Object => {
            emit_load_i32_property(property.offset, module);
            Ok(ValueKind::Object)
        }
        ObjectPropertyKind::Mixed => {
            emit_load_i32_property(property.offset, module);
            Ok(ValueKind::Mixed)
        }
    }
}

fn emit_property_initialized_guard(property: &ObjectPropertyInfo, module: &mut WasmModule) {
    let Some(initialized_offset) = property.initialized_offset else {
        return;
    };
    let object_local = module
        .next_label("object_prop_initialized")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(object_local.clone());
    module.body().line(&format!("local.tee ${}", object_local));
    module.body().line(&format!("i32.load offset={}", initialized_offset));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get ${}", object_local));
}

fn emit_store_mixed_property_default(
    object_local: &str,
    offset: usize,
    value: &Expr,
    span: Span,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let cell = module
        .next_label("object_mixed_default")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(cell.clone());
    emit_alloc_mixed_cell(&cell, module);
    emit_store_value_cell(&format!("${}", cell), value, module)
        .map_err(|err| CompileError::new(span, &err.message))?;
    module.body().line(&format!("local.get ${}", object_local));
    module.body().line(&format!("local.get ${}", cell));
    module.body().line(&format!("i32.store offset={}", offset));
    Ok(())
}

fn emit_store_mixed_property_value(
    object_local: &str,
    offset: usize,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let cell = module
        .next_label("object_mixed_value")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(cell.clone());
    emit_release_mixed_property_cell(object_local, offset, module);
    emit_alloc_mixed_cell(&cell, module);
    emit_store_value_cell(&format!("${}", cell), value, module)?;
    module.body().line(&format!("local.get ${}", object_local));
    module.body().line(&format!("local.get ${}", cell));
    module.body().line(&format!("i32.store offset={}", offset));
    Ok(())
}

fn emit_release_mixed_property_cell(object_local: &str, offset: usize, module: &mut WasmModule) {
    let old_cell = module
        .next_label("object_mixed_old")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(old_cell.clone());
    module.body().line(&format!("local.get ${}", object_local));
    module.body().line(&format!("i32.load offset={}", offset));
    module.body().line(&format!("local.set ${}", old_cell));
    module.body().line(&format!("local.get ${}", old_cell));
    module.body().open("if");
    module.body().line(&format!("local.get ${}", old_cell));
    module.body().line("call $__rt_value_release");
    module.body().close("end");
}

fn emit_store_i64_property(object_local: &str, offset: usize, module: &mut WasmModule) {
    let value_local = module.next_label("object_i64").trim_start_matches('$').to_string();
    module.declare_i64_local(value_local.clone());
    module.body().line(&format!("local.set ${}", value_local));
    module.body().line(&format!("local.get ${}", object_local));
    module.body().line(&format!("i32.const {}", offset));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}", value_local));
    module.body().line("i64.store");
}

fn emit_store_f64_property(object_local: &str, offset: usize, module: &mut WasmModule) {
    let value_local = module.next_label("object_f64").trim_start_matches('$').to_string();
    module.declare_f64_local(value_local.clone());
    module.body().line(&format!("local.set ${}", value_local));
    module.body().line(&format!("local.get ${}", object_local));
    module.body().line(&format!("i32.const {}", offset));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}", value_local));
    module.body().line("f64.store");
}

fn emit_store_i32_property(object_local: &str, offset: usize, module: &mut WasmModule) {
    let value_local = module.next_label("object_i32").trim_start_matches('$').to_string();
    module.declare_i32_local(value_local.clone());
    module.body().line(&format!("local.set ${}", value_local));
    module.body().line(&format!("local.get ${}", object_local));
    module.body().line(&format!("i32.const {}", offset));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}", value_local));
    module.body().line("i32.store");
}

fn emit_store_i32_pair_property(object_local: &str, offset: usize, module: &mut WasmModule) {
    let len_local = module.next_label("object_str_len").trim_start_matches('$').to_string();
    let ptr_local = module.next_label("object_str_ptr").trim_start_matches('$').to_string();
    module.declare_i32_local(len_local.clone());
    module.declare_i32_local(ptr_local.clone());
    module.body().line(&format!("local.set ${}", len_local));
    module.body().line(&format!("local.set ${}", ptr_local));
    module.body().line(&format!("local.get ${}", object_local));
    module.body().line(&format!("i32.const {}", offset));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}", ptr_local));
    module.body().line("i32.store");
    module.body().line(&format!("local.get ${}", object_local));
    module.body().line(&format!("i32.const {}", offset + 8));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}", len_local));
    module.body().line("i32.store");
}

fn emit_load_i64_property(offset: usize, module: &mut WasmModule) {
    module.body().line(&format!("i32.const {}", offset));
    module.body().line("i32.add");
    module.body().line("i64.load");
}

fn emit_load_f64_property(offset: usize, module: &mut WasmModule) {
    module.body().line(&format!("i32.const {}", offset));
    module.body().line("i32.add");
    module.body().line("f64.load");
}

fn emit_load_i32_property(offset: usize, module: &mut WasmModule) {
    module.body().line(&format!("i32.const {}", offset));
    module.body().line("i32.add");
    module.body().line("i32.load");
}

fn emit_load_i32_pair_property(offset: usize, module: &mut WasmModule) {
    let object_local = module.next_label("object_prop").trim_start_matches('$').to_string();
    module.declare_i32_local(object_local.clone());
    module.body().line(&format!("local.set ${}", object_local));
    module.body().line(&format!("local.get ${}", object_local));
    module.body().line(&format!("i32.const {}", offset));
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.get ${}", object_local));
    module.body().line(&format!("i32.const {}", offset + 8));
    module.body().line("i32.add");
    module.body().line("i32.load");
}
