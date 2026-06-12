//! Purpose:
//! Lowers wasm32-web foreach loops over concrete user-defined Iterator objects.
//!
//! Called from:
//! - `crate::codegen::wasm::stmt::foreach`
//!
//! Key details:
//! - Reuses normal method-call lowering for getIterator/rewind/valid/current/key/next.
//! - Supports by-value iteration only; by-reference object iteration remains rejected.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, Stmt};

use super::super::emit_stmt;
use super::super::super::expr::{emit_assign_value, emit_expr, object_class_name_for_expr};
use super::super::super::module::{ValueKind, WasmModule};

pub(super) fn expr_declared_as_iterator(expr: &Expr, module: &WasmModule) -> bool {
    let ExprKind::Variable(name) = &expr.kind else {
        return false;
    };
    module.declared_object_type_for_local(name).is_some_and(|declared_type| {
        declared_type.eq_ignore_ascii_case("Iterator")
            || module.interface_extends(&declared_type, "Iterator")
    })
}

pub(super) fn emit_iterator_object_foreach(
    source: &Expr,
    class_name: &str,
    key_var: Option<&str>,
    value_var: &str,
    body: &[Stmt],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    require_iterator_methods(source, class_name, module)?;

    let object_local = module
        .next_label("foreach_iterator_object")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(object_local.clone());
    module.set_object_class_for_local(&object_local, Some(class_name.to_string()));
    if emit_expr(source, module)? != ValueKind::Object {
        return Err(CompileError::new(
            source.span,
            "wasm32-web Iterator foreach expected an object receiver",
        ));
    }
    module.body().line(&format!("local.set ${}", object_local));
    emit_iterator_loop(&object_local, source, key_var, value_var, body, module)
}

pub(super) fn emit_dynamic_iterator_object_foreach(
    source: &Expr,
    key_var: Option<&str>,
    value_var: &str,
    body: &[Stmt],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let object_local = module
        .next_label("foreach_dynamic_iterator_object")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(object_local.clone());
    module.set_declared_object_type_for_local(&object_local, Some("Iterator".to_string()));
    if emit_expr(source, module)? != ValueKind::Object {
        return Err(CompileError::new(
            source.span,
            "wasm32-web Iterator foreach expected an object receiver",
        ));
    }
    module.body().line(&format!("local.set ${}", object_local));
    emit_iterator_loop(&object_local, source, key_var, value_var, body, module)
}

fn emit_iterator_loop(
    object_local: &str,
    source: &Expr,
    key_var: Option<&str>,
    value_var: &str,
    body: &[Stmt],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let break_label = module.next_label("foreach_iterator_break");
    let loop_label = module.next_label("foreach_iterator_loop");
    let continue_label = module.next_label("foreach_iterator_continue");
    emit_iterator_void_call(object_local, source, "rewind", module)?;
    module.body().open(&format!("block {}", break_label));
    module.body().open(&format!("loop {}", loop_label));
    emit_iterator_bool_call(&object_local, source, "valid", module)?;
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", break_label));

    if let Some(key_var) = key_var {
        let key = iterator_method_expr(object_local, source, "key");
        emit_assign_value(key_var, &key, module)?;
    }
    let current = iterator_method_expr(object_local, source, "current");
    emit_assign_value(value_var, &current, module)?;

    module.body().open(&format!("block {}", continue_label));
    module.push_loop(break_label.clone(), continue_label.clone());
    for stmt in body {
        emit_stmt(stmt, module)?;
    }
    module.pop_loop();
    module.body().close("end");

    emit_iterator_void_call(object_local, source, "next", module)?;
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_iterator_aggregate_object_foreach(
    source: &Expr,
    class_name: &str,
    key_var: Option<&str>,
    value_var: &str,
    body: &[Stmt],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some((_, method_info)) = module.object_method_in_hierarchy(class_name, "getIterator") else {
        return Err(CompileError::new(
            source.span,
            "wasm32-web IteratorAggregate foreach requires fixed getIterator() metadata",
        ));
    };
    if method_info.return_kind != ValueKind::Object {
        return Err(CompileError::new(
            source.span,
            "wasm32-web IteratorAggregate foreach requires getIterator() to return an object",
        ));
    }

    let iterator_expr = iterator_method_expr(
        &aggregate_object_local(source, class_name, module)?,
        source,
        "getIterator",
    );
    let Some(iterator_class) = object_class_name_for_expr(&iterator_expr, module) else {
        if method_info.return_object_class.as_ref().is_some_and(|return_class| {
            return_class.eq_ignore_ascii_case("Iterator")
                || module.interface_extends(return_class, "Iterator")
        }) {
            return emit_dynamic_iterator_object_foreach(&iterator_expr, key_var, value_var, body, module);
        }
        return Err(CompileError::new(
            source.span,
            "wasm32-web IteratorAggregate foreach requires a concrete Iterator return class",
        ));
    };
    if !module.object_class_implements_interface_or_parent(&iterator_class, "Iterator") {
        return Err(CompileError::new(
            source.span,
            "wasm32-web IteratorAggregate foreach requires getIterator() to return Iterator",
        ));
    }
    emit_iterator_object_foreach(&iterator_expr, &iterator_class, key_var, value_var, body, module)
}

fn require_iterator_methods(
    source: &Expr,
    class_name: &str,
    module: &WasmModule,
) -> Result<(), CompileError> {
    for method in ["rewind", "valid", "current", "key", "next"] {
        if module.object_method_in_hierarchy(class_name, method).is_none() {
            return Err(CompileError::new(
                source.span,
                "wasm32-web Iterator foreach requires fixed Iterator method metadata",
            ));
        }
    }
    Ok(())
}

fn aggregate_object_local(
    source: &Expr,
    class_name: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let object_local = module
        .next_label("foreach_iterator_aggregate")
        .trim_start_matches('$')
        .to_string();
    module.declare_object_local(object_local.clone());
    module.set_object_class_for_local(&object_local, Some(class_name.to_string()));
    if emit_expr(source, module)? != ValueKind::Object {
        return Err(CompileError::new(
            source.span,
            "wasm32-web IteratorAggregate foreach expected an object receiver",
        ));
    }
    module.body().line(&format!("local.set ${}", object_local));
    Ok(object_local)
}

fn emit_iterator_bool_call(
    object_local: &str,
    source: &Expr,
    method: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let expr = iterator_method_expr(object_local, source, method);
    if emit_expr(&expr, module)? != ValueKind::Bool {
        return Err(CompileError::new(
            source.span,
            "wasm32-web Iterator foreach requires valid() to return bool",
        ));
    }
    Ok(())
}

fn emit_iterator_void_call(
    object_local: &str,
    source: &Expr,
    method: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let expr = iterator_method_expr(object_local, source, method);
    let kind = emit_expr(&expr, module)?;
    discard_value_kind(kind, module);
    Ok(())
}

fn iterator_method_expr(object_local: &str, source: &Expr, method: &str) -> Expr {
    Expr {
        kind: ExprKind::MethodCall {
            object: Box::new(Expr {
                kind: ExprKind::Variable(object_local.to_string()),
                span: source.span,
            }),
            method: method.to_string(),
            args: Vec::new(),
        },
        span: source.span,
    }
}

fn discard_value_kind(kind: ValueKind, module: &mut WasmModule) {
    match kind {
        ValueKind::Str | ValueKind::Array => {
            module.body().line("drop");
            module.body().line("drop");
        }
        ValueKind::Null => {}
        _ => module.body().line("drop"),
    }
}
