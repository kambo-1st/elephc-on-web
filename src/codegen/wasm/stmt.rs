//! Purpose:
//! Lowers supported PHP statements into browser WebAssembly text instructions.
//! Keeps statement control flow separate from expression and module bookkeeping.
//!
//! Called from:
//! - `crate::codegen::wasm::wat`
//!
//! Key details:
//! - Unsupported constructs produce compile diagnostics so native paths remain untouched.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, Program, StaticReceiver, Stmt, StmtKind};

use super::expr::{
    emit_assoc_array_assign, emit_assoc_array_pop_shift_discard, emit_array_int_assign,
    emit_array_assign, emit_array_push, emit_assign_value, emit_dynamic_object_property_assign,
    emit_mixed_assign, emit_nested_array_assign, emit_nested_array_push, emit_object_property_assign,
    emit_output_expr, emit_static_property_access_expr, emit_static_property_assign,
    emit_store_value_cell, emit_string_offset_assign, emit_string_value_to_stack,
    expression_is_stringy, require_int, WASM_ASSOC_KEY_INT,
};
use super::module::{ArrayLayout, LocalKind, ValueKind, WasmModule};

mod control;
mod foreach;

use control::{
    emit_break, emit_continue, emit_do_while, emit_for, emit_if, emit_return, emit_switch,
    emit_while,
};
use foreach::emit_foreach;

const WASM_HEAP_KIND_INDEXED_ARRAY: i32 = 2;
const WASM_HEAP_KIND_ASSOC_ARRAY: i32 = 3;

pub(super) fn emit_program(
    program: &Program,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    for stmt in program {
        if matches!(stmt.kind, StmtKind::FunctionDecl { .. }) {
            continue;
        }
        emit_stmt(stmt, module)?;
    }
    Ok(())
}

pub(super) fn emit_function_body(
    body: &[Stmt],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    for stmt in body {
        emit_stmt(stmt, module)?;
    }
    Ok(())
}

fn emit_stmt(stmt: &Stmt, module: &mut WasmModule) -> Result<(), CompileError> {
    match &stmt.kind {
        StmtKind::Echo(expr) => emit_output_expr(expr, module),
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            emit_assign_value(name, value, module)
        }
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } if module.local_kind(array) == Some(LocalKind::Str) => {
            emit_string_offset_assign(array, index, value, module)
        }
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } if module.local_kind(array) == Some(LocalKind::Mixed)
            && module.mixed_value_cell_kind(array).is_none() =>
        {
            emit_unknown_mixed_array_assign(array, index, value, module)
        }
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } if module.local_kind(array) == Some(LocalKind::Array)
            && module.array_layout(array) == ArrayLayout::Assoc =>
        {
            emit_assoc_array_assign(array, index, value, module)
        }
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } if module.local_kind(array) == Some(LocalKind::Array) => {
            emit_array_int_assign(array, index, value, module)
        }
        StmtKind::ArrayAssign { array, .. } => Err(CompileError::new(
            stmt.span,
            &format!("wasm32-web PHP array assignment is not supported yet for ${array}"),
        )),
        StmtKind::ArrayPush { array, value }
            if module.local_kind(array) == Some(LocalKind::Array) =>
        {
            emit_array_push(array, value, module)
        }
        StmtKind::ArrayPush { array, value }
            if module.local_kind(array) == Some(LocalKind::Mixed)
                && module.mixed_value_cell_kind(array).is_none() =>
        {
            emit_unknown_mixed_array_push(array, value, module)
        }
        StmtKind::ArrayPush { array, .. } => Err(CompileError::new(
            stmt.span,
            &format!("wasm32-web PHP array assignment is not supported yet for ${array}"),
        )),
        StmtKind::NestedArrayAssign { target, value } => emit_nested_array_assign(target, value, module),
        StmtKind::NestedArrayPush { target, value } => emit_nested_array_push(target, value, module),
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => emit_object_property_assign(object, property, value, module),
        StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        } => emit_static_property_assign(receiver, property, value, module),
        StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value,
        } => emit_static_property_array_push(stmt, receiver, property, value, module),
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index,
            value,
        } => emit_static_property_array_assign(stmt, receiver, property, index, value, module),
        StmtKind::ExprStmt(expr @ Expr {
            kind: ExprKind::FunctionCall { name, args },
            ..
        }) if matches!(name.to_ascii_lowercase().as_str(), "array_pop" | "array_shift")
            && is_unknown_mixed_array_pop_shift_args(args, module) =>
        {
            emit_unknown_mixed_array_pop_shift_discard(expr, module)
        }
        StmtKind::ExprStmt(expr @ Expr {
            kind: ExprKind::FunctionCall { name, args },
            ..
        }) if matches!(name.to_ascii_lowercase().as_str(), "array_pop" | "array_shift")
            && matches!(
                args.as_slice(),
                [Expr {
                    kind: ExprKind::Variable(array),
                    ..
                }] if module.local_kind(array) == Some(LocalKind::Array)
                    && module.array_layout(array) == ArrayLayout::Assoc
            ) =>
        {
            emit_assoc_array_pop_shift_discard(expr, name, args, module)
        }
        StmtKind::ExprStmt(Expr {
            kind:
                ExprKind::Assignment {
                    target,
                    value,
                    ..
                },
            ..
        }) => match &target.kind {
            ExprKind::DynamicPropertyAccess { object, property } => {
                emit_dynamic_object_property_assign(object, property, value, module)
            }
            ExprKind::NullsafeDynamicPropertyAccess { .. } => Err(CompileError::new(
                target.span,
                "wasm32-web nullsafe dynamic property writes are not supported yet",
            )),
            _ => Err(CompileError::new(
                target.span,
                "wasm32-web expression assignments are not supported yet",
            )),
        },
        StmtKind::ExprStmt(expr) => {
            match super::expr::emit_expr(expr, module)? {
                ValueKind::Array => {
                    module.body().line("drop");
                    module.body().line("drop");
                }
                ValueKind::Object => module.body().line("drop"),
                ValueKind::Null | ValueKind::Never => {}
                _ => module.body().line("drop"),
            }
            Ok(())
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => emit_if(condition, then_body, elseif_clauses, else_body.as_deref(), module),
        StmtKind::While { condition, body } => emit_while(condition, body, module),
        StmtKind::DoWhile { body, condition } => emit_do_while(body, condition, module),
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => emit_for(init.as_deref(), condition.as_ref(), update.as_deref(), body, module),
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            value_by_ref,
            body,
        } => emit_foreach(array, key_var.as_deref(), value_var, *value_by_ref, body, module),
        StmtKind::ListUnpack { vars, value } => emit_list_unpack(vars, value, module),
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => emit_switch(subject, cases, default.as_deref(), module),
        StmtKind::Return(value) => emit_return(value.as_ref(), module),
        StmtKind::Break(levels) => emit_break(*levels, module, stmt.span),
        StmtKind::Continue(levels) => emit_continue(*levels, module, stmt.span),
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for stmt in stmts {
                emit_stmt(stmt, module)?;
            }
            Ok(())
        }
        StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::ConstDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::FunctionDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. } => Ok(()),
        _ => Err(unsupported_stmt(stmt)),
    }
}

fn emit_list_unpack(
    vars: &[String],
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("list_unpack_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    emit_array_assign(&temp, value, module)?;
    for (index, var) in vars.iter().enumerate() {
        let access = Expr::new(
            ExprKind::ArrayAccess {
                array: Box::new(Expr::new(ExprKind::Variable(temp.clone()), value.span)),
                index: Box::new(Expr::int_lit(index as i64)),
            },
            value.span,
        );
        emit_assign_value(var, &access, module)?;
    }
    Ok(())
}

fn is_unknown_mixed_array_pop_shift_args(args: &[Expr], module: &WasmModule) -> bool {
    let [arg] = args else {
        return false;
    };
    let ExprKind::Variable(array) = &arg.kind else {
        return false;
    };
    module.local_kind(array) == Some(LocalKind::Mixed) && module.mixed_value_cell_kind(array).is_none()
}

fn emit_unknown_mixed_array_pop_shift_discard(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module
        .next_label("discard_unknown_mixed_array_pop_shift")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(temp.clone());
    module.body().line("call $__rt_alloc_null_mixed_cell");
    module.body().line(&format!("local.set ${}", temp));
    emit_mixed_assign(&temp, expr, module)?;
    module.body().line(&format!("local.get ${}", temp));
    module.body().line("call $__rt_value_release");
    Ok(())
}

fn emit_static_property_array_assign(
    stmt: &Stmt,
    receiver: &StaticReceiver,
    property: &str,
    index: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = emit_static_property_array_cell(stmt, receiver, property, module)?;
    emit_unknown_mixed_array_assign(&temp, index, value, module)
}

fn emit_static_property_array_push(
    stmt: &Stmt,
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = emit_static_property_array_cell(stmt, receiver, property, module)?;
    emit_unknown_mixed_array_push(&temp, value, module)
}

fn emit_static_property_array_cell(
    stmt: &Stmt,
    receiver: &StaticReceiver,
    property: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let temp = module
        .next_label("static_array_property_cell")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(temp.clone());
    let expr = Expr::new(
        ExprKind::StaticPropertyAccess {
            receiver: receiver.clone(),
            property: property.to_string(),
        },
        stmt.span,
    );
    match emit_static_property_access_expr(&expr, receiver, property, module)? {
        ValueKind::Mixed => {
            module.body().line(&format!("local.set ${}", temp));
            Ok(temp)
        }
        _ => Err(CompileError::new(
            stmt.span,
            "wasm32-web static property array mutation requires array static-property storage",
        )),
    }
}

fn unsupported_stmt(stmt: &Stmt) -> CompileError {
    CompileError::new(
        stmt.span,
        "wasm32-web WAT output currently supports scalar assignment, echo, if, loops, break, and continue",
    )
}

fn emit_load_unknown_mixed_array_header(
    array: &str,
    ptr: &str,
    len: &str,
    heap_kind: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}", array));
    module.body().line("call $__rt_mixed_array_ptr");
    module.body().line(&format!("local.set {}", ptr));
    module.body().line(&format!("local.get ${}", array));
    module.body().line("call $__rt_mixed_array_len_i32");
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.get ${}", array));
    module.body().line("call $__rt_mixed_array_heap_kind");
    module.body().line(&format!("local.set {}", heap_kind));
}

fn emit_unknown_mixed_array_assign(
    array: &str,
    index: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if expression_is_stringy(index, module) {
        return emit_unknown_mixed_array_string_assign(array, index, value, module);
    }
    emit_unknown_mixed_array_int_assign(array, index, value, module)
}

fn emit_unknown_mixed_array_int_assign(
    array: &str,
    index: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let index64 = module.next_label("unknown_mixed_array_assign_index64");
    let index32 = module.next_label("unknown_mixed_array_assign_index32");
    let ptr = module.next_label("unknown_mixed_array_assign_ptr");
    let len = module.next_label("unknown_mixed_array_assign_len");
    let cell = module.next_label("unknown_mixed_array_assign_cell");
    let heap_kind = module.next_label("unknown_mixed_array_assign_heap_kind");
    module.declare_i64_local(index64.trim_start_matches('$').to_string());
    for local in [&index32, &ptr, &len, &cell, &heap_kind] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    require_int(index, module)?;
    module.body().line(&format!("local.set {}", index64));
    emit_load_unknown_mixed_array_header(array, &ptr, &len, &heap_kind, module);
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_INDEXED_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_unknown_mixed_indexed_int_assign(array, value, &index64, &index32, &ptr, &len, &cell, module)?;
    module.body().close("else");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_unknown_mixed_assoc_int_assign(array, value, &index64, &ptr, &len, &cell, module)?;
    module.body().close("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_unknown_mixed_indexed_int_assign(
    array: &str,
    value: &Expr,
    index64: &str,
    index32: &str,
    ptr: &str,
    len: &str,
    cell: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let new_ptr = module.next_label("unknown_mixed_indexed_sparse_assign_new_ptr");
    let scan = module.next_label("unknown_mixed_indexed_sparse_assign_scan");
    let source_cell = module.next_label("unknown_mixed_indexed_sparse_assign_source_cell");
    let target_entry = module.next_label("unknown_mixed_indexed_sparse_assign_target_entry");
    let target_cell = module.next_label("unknown_mixed_indexed_sparse_assign_target_cell");
    let done_label = module.next_label("unknown_mixed_indexed_sparse_assign_done");
    let loop_label = module.next_label("unknown_mixed_indexed_sparse_assign_loop");
    for local in [&new_ptr, &scan, &source_cell, &target_entry, &target_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get {}", index64));
    module.body().line(&format!("local.get {}", len));
    module.body().line("call $__rt_value_index_in_bounds");
    module.body().open("if");
    emit_unknown_mixed_indexed_int_assign_in_bounds(array, index64, index32, ptr, cell, module);
    module.body().close("else");
    emit_unknown_mixed_indexed_int_assign_sparse(
        array,
        index64,
        ptr,
        len,
        cell,
        &new_ptr,
        &scan,
        &source_cell,
        &target_entry,
        &target_cell,
        done_label,
        loop_label,
        module,
    );
    module.body().close("end");
    emit_store_value_cell(cell, value, module)
}

fn emit_unknown_mixed_indexed_int_assign_in_bounds(
    array: &str,
    index64: &str,
    index32: &str,
    ptr: &str,
    cell: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", index64));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", index32));
    module.body().line(&format!("local.get {}", ptr));
    module.body().line("call $__rt_ensure_unique");
    module.body().line(&format!("local.set {}", ptr));
    module.body().line(&format!("local.get ${}", array));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line("i32.store");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", index32));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", cell));
}

fn emit_unknown_mixed_indexed_int_assign_sparse(
    array: &str,
    index64: &str,
    ptr: &str,
    len: &str,
    cell: &str,
    new_ptr: &str,
    scan: &str,
    source_cell: &str,
    target_entry: &str,
    target_cell: &str,
    done_label: String,
    loop_label: String,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set {}", new_ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", scan));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.get {}", scan));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i64.extend_i32_u");
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", scan));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", source_cell));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", target_cell));
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get {}", source_cell));
    module.body().line("call $__rt_value_copy");
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.get {}", len));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", index64));
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    module.body().line(&format!("local.get ${}", array));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line("i32.store");
    module.body().line(&format!("local.get ${}", array));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("i32.store");
}

fn emit_unknown_mixed_assoc_int_assign(
    array: &str,
    value: &Expr,
    index64: &str,
    ptr: &str,
    len: &str,
    cell: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_unknown_mixed_assoc_int_cell_for_key_or_append(array, index64, ptr, len, cell, module);
    emit_store_value_cell(cell, value, module)
}

fn emit_unknown_mixed_array_string_assign(
    array: &str,
    index: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let key_ptr = module.next_label("unknown_mixed_array_assign_key_ptr");
    let key_len = module.next_label("unknown_mixed_array_assign_key_len");
    let ptr = module.next_label("unknown_mixed_array_assign_ptr");
    let len = module.next_label("unknown_mixed_array_assign_len");
    let cell = module.next_label("unknown_mixed_array_assign_cell");
    let heap_kind = module.next_label("unknown_mixed_array_assign_heap_kind");
    for local in [&key_ptr, &key_len, &ptr, &len, &cell, &heap_kind] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_string_value_to_stack(index, module)?;
    module.body().line(&format!("local.set {}", key_len));
    module.body().line(&format!("local.set {}", key_ptr));
    emit_load_unknown_mixed_array_header(array, &ptr, &len, &heap_kind, module);
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    emit_unknown_mixed_assoc_string_cell_for_key_or_append(array, &key_ptr, &key_len, &ptr, &len, &cell, module);
    emit_store_value_cell(&cell, value, module)
}

fn emit_unknown_mixed_assoc_int_cell_for_key_or_append(
    array: &str,
    key: &str,
    ptr: &str,
    len: &str,
    cell: &str,
    module: &mut WasmModule,
) {
    let scan = module.next_label("unknown_mixed_assoc_assign_index");
    let entry = module.next_label("unknown_mixed_assoc_assign_entry");
    let target_entry = module.next_label("unknown_mixed_assoc_assign_target_entry");
    let new_ptr = module.next_label("unknown_mixed_assoc_assign_new_ptr");
    let found = module.next_label("unknown_mixed_assoc_assign_found");
    let found_index = module.next_label("unknown_mixed_assoc_assign_found_index");
    let done_label = module.next_label("unknown_mixed_assoc_assign_done");
    let loop_label = module.next_label("unknown_mixed_assoc_assign_loop");
    let copy_done_label = module.next_label("unknown_mixed_assoc_assign_copy_done");
    let copy_loop_label = module.next_label("unknown_mixed_assoc_assign_copy_loop");
    for local in [&scan, &entry, &target_entry, &new_ptr, &found, &found_index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", scan));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", scan));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get {}", key));
    module.body().line("call $__rt_assoc_key_eq_int");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.set {}", found_index));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
    module.body().open("if");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line("call $__rt_ensure_unique");
    module.body().line(&format!("local.set {}", ptr));
    module.body().line(&format!("local.get ${}", array));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line("i32.store");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", found_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    module.body().close("else");
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set {}", new_ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", scan));
    module.body().open(&format!("block {}", copy_done_label));
    module.body().open(&format!("loop {}", copy_loop_label));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", copy_done_label));
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", scan));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.get {}", scan));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_copy_entry");
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().line(&format!("br {}", copy_loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.get {}", len));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", key));
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    module.body().line(&format!("local.get ${}", array));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line("i32.store");
    module.body().line(&format!("local.get ${}", array));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("i32.store");
    module.body().close("end");
}

fn emit_unknown_mixed_assoc_string_cell_for_key_or_append(
    array: &str,
    key_ptr: &str,
    key_len: &str,
    ptr: &str,
    len: &str,
    cell: &str,
    module: &mut WasmModule,
) {
    let scan = module.next_label("unknown_mixed_assoc_string_assign_index");
    let entry = module.next_label("unknown_mixed_assoc_string_assign_entry");
    let target_entry = module.next_label("unknown_mixed_assoc_string_assign_target_entry");
    let new_ptr = module.next_label("unknown_mixed_assoc_string_assign_new_ptr");
    let found = module.next_label("unknown_mixed_assoc_string_assign_found");
    let found_index = module.next_label("unknown_mixed_assoc_string_assign_found_index");
    let done_label = module.next_label("unknown_mixed_assoc_string_assign_done");
    let loop_label = module.next_label("unknown_mixed_assoc_string_assign_loop");
    let copy_done_label = module.next_label("unknown_mixed_assoc_string_assign_copy_done");
    let copy_loop_label = module.next_label("unknown_mixed_assoc_string_assign_copy_loop");
    for local in [&scan, &entry, &target_entry, &new_ptr, &found, &found_index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", scan));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", scan));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get {}", key_ptr));
    module.body().line(&format!("local.get {}", key_len));
    module.body().line("call $__rt_assoc_key_eq_php_string");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.set {}", found_index));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
    module.body().open("if");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line("call $__rt_ensure_unique");
    module.body().line(&format!("local.set {}", ptr));
    module.body().line(&format!("local.get ${}", array));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line("i32.store");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", found_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    module.body().close("else");
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set {}", new_ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", scan));
    module.body().open(&format!("block {}", copy_done_label));
    module.body().open(&format!("loop {}", copy_loop_label));
    module.body().line(&format!("local.get {}", scan));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", copy_done_label));
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", scan));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.get {}", scan));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_copy_entry");
    module.body().line(&format!("local.get {}", scan));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", scan));
    module.body().line(&format!("br {}", copy_loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.get {}", len));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", key_ptr));
    module.body().line(&format!("local.get {}", key_len));
    module.body().line("call $__rt_assoc_store_string_key");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    module.body().line(&format!("local.get ${}", array));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line("i32.store");
    module.body().line(&format!("local.get ${}", array));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("i32.store");
    module.body().close("end");
}

pub(in crate::codegen::wasm) fn emit_unknown_mixed_array_push(
    array: &str,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let old_ptr = module.next_label("unknown_mixed_array_push_old_ptr");
    let new_ptr = module.next_label("unknown_mixed_array_push_new_ptr");
    let len = module.next_label("unknown_mixed_array_push_len");
    let index = module.next_label("unknown_mixed_array_push_index");
    let source_cell = module.next_label("unknown_mixed_array_push_source_cell");
    let target_cell = module.next_label("unknown_mixed_array_push_target_cell");
    let source_entry = module.next_label("unknown_mixed_array_push_source_entry");
    let target_entry = module.next_label("unknown_mixed_array_push_target_entry");
    let next_key = module.next_label("unknown_mixed_array_push_next_key");
    let int_key = module.next_label("unknown_mixed_array_push_int_key");
    let found_int_key = module.next_label("unknown_mixed_array_push_found_int_key");
    let heap_kind = module.next_label("unknown_mixed_array_push_heap_kind");
    let done_label = module.next_label("unknown_mixed_array_push_done");
    let loop_label = module.next_label("unknown_mixed_array_push_loop");
    let assoc_done_label = module.next_label("unknown_mixed_array_push_assoc_done");
    let assoc_loop_label = module.next_label("unknown_mixed_array_push_assoc_loop");
    for local in [
        &old_ptr,
        &new_ptr,
        &len,
        &index,
        &source_cell,
        &target_cell,
        &source_entry,
        &target_entry,
        &found_int_key,
        &heap_kind,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&next_key, &int_key] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    emit_load_unknown_mixed_array_header(array, &old_ptr, &len, &heap_kind, module);
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_INDEXED_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_unknown_mixed_indexed_array_push_body(array, value, &old_ptr, &new_ptr, &len, &index, &source_cell, &target_cell, done_label, loop_label, module)?;
    module.body().close("else");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    emit_unknown_mixed_assoc_array_push_body(
        array,
        value,
        &old_ptr,
        &new_ptr,
        &len,
        &index,
        &source_entry,
        &target_entry,
        &target_cell,
        &next_key,
        &int_key,
        &found_int_key,
        assoc_done_label,
        assoc_loop_label,
        module,
    )?;
    module.body().close("end");
    Ok(())
}

pub(in crate::codegen::wasm) fn emit_unknown_mixed_array_unshift(
    array: &str,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let old_ptr = module.next_label("unknown_mixed_array_unshift_old_ptr");
    let new_ptr = module.next_label("unknown_mixed_array_unshift_new_ptr");
    let len = module.next_label("unknown_mixed_array_unshift_len");
    let index = module.next_label("unknown_mixed_array_unshift_index");
    let source_cell = module.next_label("unknown_mixed_array_unshift_source_cell");
    let target_cell = module.next_label("unknown_mixed_array_unshift_target_cell");
    let source_entry = module.next_label("unknown_mixed_array_unshift_source_entry");
    let target_entry = module.next_label("unknown_mixed_array_unshift_target_entry");
    let target_index = module.next_label("unknown_mixed_array_unshift_target_index");
    let next_key = module.next_label("unknown_mixed_array_unshift_next_key");
    let heap_kind = module.next_label("unknown_mixed_array_unshift_heap_kind");
    let key_kind = module.next_label("unknown_mixed_array_unshift_key_kind");
    let done_label = module.next_label("unknown_mixed_array_unshift_done");
    let loop_label = module.next_label("unknown_mixed_array_unshift_loop");
    let assoc_done_label = module.next_label("unknown_mixed_array_unshift_assoc_done");
    let assoc_loop_label = module.next_label("unknown_mixed_array_unshift_assoc_loop");
    for local in [
        &old_ptr,
        &new_ptr,
        &len,
        &index,
        &source_cell,
        &target_cell,
        &source_entry,
        &target_entry,
        &target_index,
        &heap_kind,
        &key_kind,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(next_key.trim_start_matches('$').to_string());
    emit_load_unknown_mixed_array_header(array, &old_ptr, &len, &heap_kind, module);
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_INDEXED_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_unknown_mixed_indexed_array_unshift_body(
        array,
        value,
        &old_ptr,
        &new_ptr,
        &len,
        &index,
        &source_cell,
        &target_cell,
        done_label,
        loop_label,
        module,
    )?;
    module.body().close("else");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    emit_unknown_mixed_assoc_array_unshift_body(
        array,
        value,
        &old_ptr,
        &new_ptr,
        &len,
        &index,
        &source_entry,
        &target_entry,
        &target_index,
        &target_cell,
        &next_key,
        &key_kind,
        assoc_done_label,
        assoc_loop_label,
        module,
    )?;
    module.body().close("end");
    Ok(())
}

fn emit_unknown_mixed_indexed_array_unshift_body(
    array: &str,
    value: &Expr,
    old_ptr: &str,
    new_ptr: &str,
    len: &str,
    index: &str,
    source_cell: &str,
    target_cell: &str,
    done_label: String,
    loop_label: String,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", new_ptr));
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line("i32.const 0");
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", target_cell));
    emit_store_value_cell(target_cell, value, module)?;
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", old_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", source_cell));
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", target_cell));
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get {}", source_cell));
    module.body().line("call $__rt_value_copy");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    emit_update_unknown_mixed_array_payload(array, new_ptr, len, module);
    Ok(())
}

fn emit_unknown_mixed_assoc_array_unshift_body(
    array: &str,
    value: &Expr,
    old_ptr: &str,
    new_ptr: &str,
    len: &str,
    index: &str,
    source_entry: &str,
    target_entry: &str,
    target_index: &str,
    target_cell: &str,
    next_key: &str,
    key_kind: &str,
    done_label: String,
    loop_label: String,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set {}", new_ptr));
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line("i32.const 0");
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("i64.const 0");
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", target_cell));
    emit_store_value_cell(target_cell, value, module)?;
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", target_index));
    module.body().line("i64.const 1");
    module.body().line(&format!("local.set {}", next_key));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", old_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", source_entry));
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.get {}", target_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", key_kind));
    module.body().line(&format!("local.get {}", key_kind));
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", next_key));
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", next_key));
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", next_key));
    module.body().line("else");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("call $__rt_assoc_copy_key");
    module.body().close("end");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", target_cell));
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line("call $__rt_value_copy");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("local.get {}", target_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", target_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    emit_update_unknown_mixed_array_payload(array, new_ptr, len, module);
    Ok(())
}

fn emit_unknown_mixed_indexed_array_push_body(
    array: &str,
    value: &Expr,
    old_ptr: &str,
    new_ptr: &str,
    len: &str,
    index: &str,
    source_cell: &str,
    target_cell: &str,
    done_label: String,
    loop_label: String,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", new_ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", old_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", source_cell));
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", target_cell));
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get {}", source_cell));
    module.body().line("call $__rt_value_copy");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.get {}", len));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", target_cell));
    emit_store_value_cell(target_cell, value, module)?;
    emit_update_unknown_mixed_array_payload(array, new_ptr, len, module);
    Ok(())
}

fn emit_unknown_mixed_assoc_array_push_body(
    array: &str,
    value: &Expr,
    old_ptr: &str,
    new_ptr: &str,
    len: &str,
    index: &str,
    source_entry: &str,
    target_entry: &str,
    target_cell: &str,
    next_key: &str,
    int_key: &str,
    found_int_key: &str,
    done_label: String,
    loop_label: String,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.body().line("i64.const 0");
    module.body().line(&format!("local.set {}", next_key));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found_int_key));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set {}", new_ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", old_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", source_entry));
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("call $__rt_assoc_copy_entry");
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("call $__rt_assoc_key_payload_i64");
    module.body().line(&format!("local.set {}", int_key));
    module.body().line(&format!("local.get {}", found_int_key));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line(&format!("local.get {}", int_key));
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", next_key));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found_int_key));
    module.body().close("else");
    module.body().line(&format!("local.get {}", int_key));
    module.body().line(&format!("local.get {}", next_key));
    module.body().line("i64.ge_s");
    module.body().open("if");
    module.body().line(&format!("local.get {}", int_key));
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", next_key));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.get {}", len));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", next_key));
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", target_cell));
    emit_store_value_cell(target_cell, value, module)?;
    emit_update_unknown_mixed_array_payload(array, new_ptr, len, module);
    Ok(())
}

fn emit_update_unknown_mixed_array_payload(
    array: &str,
    new_ptr: &str,
    len: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}", array));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line("i32.store");
    module.body().line(&format!("local.get ${}", array));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("i32.store");
}
