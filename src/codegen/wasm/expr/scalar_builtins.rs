//! Purpose:
//! Lowers wasm32-web scalar builtins and static output-string builtin evaluation.
//! Keeps numeric/string builtin plumbing out of the expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` call and output lowering paths.
//! - Sibling wasm string materialization/value modules for static string builtin folding.
//!
//! Key details:
//! - Runtime-only paths preserve boxed Mixed/value-cell contracts and PHP-style coercion checks.

use super::*;

pub(in crate::codegen::wasm) fn emit_inc_dec(
    expr: &Expr,
    name: &str,
    delta: i64,
    pre: bool,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if module.local_kind(name) != Some(LocalKind::I64) {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web increment/decrement currently requires an integer local",
        ));
    }
    if let Some(address_local) = module.i64_ref_alias(name) {
        module.body().line(&format!("local.get {}", address_local));
        module.body().line("i64.load");
        if pre {
            module.body().line(&format!("i64.const {}", delta.abs()));
            module
                .body()
                .line(if delta >= 0 { "i64.add" } else { "i64.sub" });
            module.body().line(&format!("local.set ${}", name));
            module.body().line(&format!("local.get {}", address_local));
            module.body().line(&format!("local.get ${}", name));
            module.body().line("i64.store");
            module.body().line(&format!("local.get ${}", name));
        } else {
            module.body().line(&format!("local.set ${}", name));
            module.body().line(&format!("local.get ${}", name));
            module.body().line(&format!("local.get ${}", name));
            module.body().line(&format!("i64.const {}", delta.abs()));
            module
                .body()
                .line(if delta >= 0 { "i64.add" } else { "i64.sub" });
            module.body().line(&format!("local.set ${}", name));
            module.body().line(&format!("local.get {}", address_local));
            module.body().line(&format!("local.get ${}", name));
            module.body().line("i64.store");
        }
        return Ok(ValueKind::Int);
    }
    if pre {
        module.body().line(&format!("local.get ${}", name));
        module.body().line(&format!("i64.const {}", delta.abs()));
        module
            .body()
            .line(if delta >= 0 { "i64.add" } else { "i64.sub" });
        module.body().line(&format!("local.tee ${}", name));
    } else {
        module.body().line(&format!("local.get ${}", name));
        module.body().line(&format!("local.get ${}", name));
        module.body().line(&format!("i64.const {}", delta.abs()));
        module
            .body()
            .line(if delta >= 0 { "i64.add" } else { "i64.sub" });
        module.body().line(&format!("local.set ${}", name));
    }
    Ok(ValueKind::Int)
}

pub(in crate::codegen::wasm) fn emit_abs_call(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let [arg] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web abs() expects exactly one argument",
        ));
    };
    if expression_is_floaty(arg, module) {
        require_float(arg, module)?;
        module.body().line("f64.abs");
        return Ok(ValueKind::Float);
    }

    let value_local = module.next_label("abs_value").trim_start_matches('$').to_string();
    module.declare_i64_local(value_local.clone());
    require_int(arg, module)?;
    module.body().line(&format!("local.set ${}", value_local));
    module.body().line(&format!("local.get ${}", value_local));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().open("if (result i64)");
    module.body().line("i64.const 0");
    module.body().line(&format!("local.get ${}", value_local));
    module.body().line("i64.sub");
    module.body().line("else");
    module.body().line(&format!("local.get ${}", value_local));
    module.body().close("end");
    Ok(ValueKind::Int)
}

pub(in crate::codegen::wasm) fn emit_intdiv_call(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let [left, right] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web intdiv() expects exactly two arguments",
        ));
    };
    require_int(left, module)?;
    require_int(right, module)?;
    module.body().line("i64.div_s");
    Ok(ValueKind::Int)
}

pub(in crate::codegen::wasm) fn emit_fdiv_call(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let [left, right] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web fdiv() expects exactly two arguments",
        ));
    };
    require_float(left, module)?;
    require_float(right, module)?;
    module.body().line("f64.div");
    Ok(ValueKind::Float)
}

pub(in crate::codegen::wasm) fn emit_gettype_output_call(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_gettype_string_value_to_stack(call, args, module)?;
    module.body().line("call $host_write");
    Ok(())
}

pub(in crate::codegen::wasm) fn emit_gettype_string_value_to_stack(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let [arg] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web gettype() expects exactly one argument",
        ));
    };
    if matches!(arg.kind, ExprKind::ArrayAccess { .. }) {
        if let Some(cell) = materialize_mixed_value_cell(arg, module)? {
            emit_mixed_gettype_string_value_to_stack(&cell, module);
            return Ok(());
        }
    }
    if expression_is_stringy(arg, module) {
        let (ptr, len) = module.intern_string("string");
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(());
    }
    if expression_is_arrayy(arg, module) || expression_has_array_type(arg, module) {
        let (ptr, len) = module.intern_string("array");
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(());
    }
    if matches!(arg.kind, ExprKind::NewObject { .. })
        || matches!(&arg.kind, ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Object))
    {
        let (ptr, len) = module.intern_string("object");
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(());
    }
    if let ExprKind::Variable(name) = &arg.kind {
        if module.local_kind(name) == Some(LocalKind::Mixed) {
            emit_mixed_gettype_string_value_to_stack(name, module);
            return Ok(());
        }
    }
    if let Some(cell) = materialize_mixed_value_cell(arg, module)? {
        emit_mixed_gettype_string_value_to_stack(&cell, module);
        return Ok(());
    }
    if emit_dynamic_backed_enum_try_from_pointer(arg, module)? {
        let object = module
            .next_label("enum_try_from_gettype_object")
            .trim_start_matches('$')
            .to_string();
        module.declare_i32_local(object.clone());
        module.body().line(&format!("local.set ${}", object));
        module.body().line(&format!("local.get ${}", object));
        module.body().line("i32.eqz");
        module.body().open("if (result i32 i32)");
        let (ptr, len) = module.intern_string("NULL");
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        module.body().line("else");
        let (ptr, len) = module.intern_string("object");
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        module.body().close("end");
        return Ok(());
    }
    let type_name = match &arg.kind {
        ExprKind::StringLiteral(_) => "string",
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Str) => "string",
        ExprKind::ConstRef(name) => match module.constant_value(name) {
            Some(ConstantValue::Int(_)) => "integer",
            Some(ConstantValue::Float(_)) => "double",
            Some(ConstantValue::Bool(_)) => "boolean",
            Some(ConstantValue::Str(_)) => "string",
            Some(ConstantValue::Null) => "NULL",
            None => {
                if const_int_value(name).is_some() {
                    "integer"
                } else {
                    return Err(CompileError::new(
                        arg.span,
                        "wasm32-web constant is not supported yet",
                    ));
                }
            }
        },
        ExprKind::ClassConstant { receiver } => {
            if module.class_name_for_receiver(receiver).is_some() {
                "string"
            } else {
                return Err(CompileError::new(
                    arg.span,
                    "wasm32-web ::class currently requires a named class receiver",
                ));
            }
        }
        ExprKind::ScopedConstantAccess { receiver, name } => {
            match module.class_constant_value(receiver, name) {
                Some(ConstantValue::Int(_)) => "integer",
                Some(ConstantValue::Float(_)) => "double",
                Some(ConstantValue::Bool(_)) => "boolean",
                Some(ConstantValue::Str(_)) => "string",
                Some(ConstantValue::Null) => "NULL",
                None => {
                    return Err(CompileError::new(
                        arg.span,
                        "wasm32-web class constant is not supported yet",
                    ));
                }
            }
        }
        _ => match emit_expr(arg, module)? {
            ValueKind::Int => {
                module.body().line("drop");
                "integer"
            }
            ValueKind::Float => {
                module.body().line("drop");
                "double"
            }
            ValueKind::Bool => {
                module.body().line("drop");
                "boolean"
            }
            ValueKind::Null => {
                module.body().line("drop");
                "NULL"
            }
            ValueKind::Str => {
                module.body().line("drop");
                module.body().line("drop");
                "string"
            }
            ValueKind::Array => {
                module.body().line("drop");
                module.body().line("drop");
                "array"
            }
            ValueKind::Object => {
                module.body().line("drop");
                "object"
            }
            ValueKind::Mixed => {
                let local = module
                    .next_label("mixed_gettype_value")
                    .trim_start_matches('$')
                    .to_string();
                module.declare_i32_local(local.clone());
                module.body().line(&format!("local.set ${}", local));
                emit_mixed_gettype_string_value_to_stack(&local, module);
                return Ok(());
            }
            ValueKind::Never => {
                module.body().line("unreachable");
                return Ok(());
            }
        },
    };
    let (ptr, len) = module.intern_string(type_name);
    module.body().line(&format!("i32.const {}", ptr));
    module.body().line(&format!("i32.const {}", len));
    Ok(())
}

fn emit_mixed_gettype_string_value_to_stack(name: &str, module: &mut WasmModule) {
    let ptr = module.next_label("mixed_gettype_ptr");
    let len = module.next_label("mixed_gettype_len");
    module.declare_i32_local(ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(len.trim_start_matches('$').to_string());
    let type_names = [
        (WASM_VALUE_TAG_INT, "integer"),
        (WASM_VALUE_TAG_FLOAT, "double"),
        (WASM_VALUE_TAG_BOOL, "boolean"),
        (WASM_VALUE_TAG_STRING, "string"),
        (WASM_VALUE_TAG_ARRAY, "array"),
        (WASM_VALUE_TAG_NULL, "NULL"),
        (WASM_VALUE_TAG_OBJECT, "object"),
    ];
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", len));
    for (tag, type_name) in type_names {
        module.body().line(&format!("local.get ${}", name));
        module.body().line(&format!("i32.const {}", tag));
        module.body().line("call $__rt_mixed_tag_equals");
        module.body().open("if");
        let (type_ptr, type_len) = module.intern_string(type_name);
        module.body().line(&format!("i32.const {}", type_ptr));
        module.body().line(&format!("local.set {}", ptr));
        module.body().line(&format!("i32.const {}", type_len));
        module.body().line(&format!("local.set {}", len));
        module.body().close("end");
    }
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", len));
}

pub(in crate::codegen::wasm) fn emit_min_max_call(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if args.len() < 2 {
        return Err(CompileError::new(
            call.span,
            &format!("wasm32-web {}() expects at least two arguments", name),
        ));
    }
    if args.iter().any(|arg| expression_is_floaty(arg, module)) {
        return emit_float_min_max(name, args, module);
    }
    emit_int_min_max(name, args, module)
}

fn emit_int_min_max(
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let result_local = module.next_label(name).trim_start_matches('$').to_string();
    let candidate_local = module
        .next_label(&format!("{}_candidate", name))
        .trim_start_matches('$')
        .to_string();
    module.declare_i64_local(result_local.clone());
    module.declare_i64_local(candidate_local.clone());

    require_int(&args[0], module)?;
    module.body().line(&format!("local.set ${}", result_local));
    for arg in &args[1..] {
        require_int(arg, module)?;
        module.body().line(&format!("local.set ${}", candidate_local));
        module.body().line(&format!("local.get ${}", candidate_local));
        module.body().line(&format!("local.get ${}", result_local));
        module.body().line(if name.eq_ignore_ascii_case("min") {
            "i64.lt_s"
        } else {
            "i64.gt_s"
        });
        module.body().open("if");
        module.body().line(&format!("local.get ${}", candidate_local));
        module.body().line(&format!("local.set ${}", result_local));
        module.body().close("end");
    }
    module.body().line(&format!("local.get ${}", result_local));
    Ok(ValueKind::Int)
}

fn emit_float_min_max(
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let result_local = module.next_label(name).trim_start_matches('$').to_string();
    let candidate_local = module
        .next_label(&format!("{}_candidate", name))
        .trim_start_matches('$')
        .to_string();
    module.declare_f64_local(result_local.clone());
    module.declare_f64_local(candidate_local.clone());

    require_float(&args[0], module)?;
    module.body().line(&format!("local.set ${}", result_local));
    for arg in &args[1..] {
        require_float(arg, module)?;
        module.body().line(&format!("local.set ${}", candidate_local));
        module.body().line(&format!("local.get ${}", candidate_local));
        module.body().line(&format!("local.get ${}", result_local));
        module.body().line(if name.eq_ignore_ascii_case("min") {
            "f64.lt"
        } else {
            "f64.gt"
        });
        module.body().open("if");
        module.body().line(&format!("local.get ${}", candidate_local));
        module.body().line(&format!("local.set ${}", result_local));
        module.body().close("end");
    }
    module.body().line(&format!("local.get ${}", result_local));
    Ok(ValueKind::Float)
}

pub(in crate::codegen::wasm) fn emit_strlen_call(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let [arg] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web strlen() expects exactly one argument",
        ));
    };
    if let Some(err) = unsupported_string_coercion_expr(arg) {
        return Err(err);
    }
    if let Some(err) = unsupported_object_string_coercion_expr(arg, module) {
        return Err(err);
    }
    if let Some(value) = static_string_value(arg, module) {
        module.body().line(&format!("i64.const {}", value.len()));
        return Ok(ValueKind::Int);
    }
    if let Some(kind) = known_mixed_value_cell_kind(arg, module)? {
        if !array_map_strlen_value_kind_is_supported(kind) {
            return Err(CompileError::new(
                arg.span,
                "wasm32-web strlen() currently supports scalar string-coercible mixed values",
            ));
        }
        let Some(cell) = materialize_mixed_value_cell(arg, module)? else {
            return Err(CompileError::new(
                arg.span,
                "wasm32-web strlen() could not materialize mixed value cell",
            ));
        };
        emit_value_cell_pointer_strlen_length(&format!("${cell}"), kind, module);
        return Ok(ValueKind::Int);
    }
    if object_tostring_supported(arg, module) {
        if let Some(cell) = materialize_mixed_value_cell(arg, module)? {
            emit_mixed_object_or_dynamic_strlen_length(arg, &cell, module)?;
            return Ok(ValueKind::Int);
        }
        let ptr = module.next_label("strlen_tostring_ptr");
        let len = module.next_label("strlen_tostring_len");
        module.declare_i32_local(ptr.trim_start_matches('$').to_string());
        module.declare_i32_local(len.trim_start_matches('$').to_string());
        emit_string_value_to_locals(arg, &ptr, &len, module)?;
        module.body().line(&format!("local.get {}", len));
        module.body().line("i64.extend_i32_u");
        return Ok(ValueKind::Int);
    }
    if let Some(cell) = materialize_mixed_value_cell(arg, module)? {
        emit_dynamic_mixed_strlen_length(&cell, module);
        return Ok(ValueKind::Int);
    }
    match &arg.kind {
        ExprKind::StringLiteral(value) => {
            module.body().line(&format!("i64.const {}", value.len()));
            Ok(ValueKind::Int)
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Str) => {
            module.body().line(&format!("local.get ${}_len", name));
            module.body().line("i64.extend_i32_u");
            Ok(ValueKind::Int)
        }
        _ if expression_is_stringy(arg, module) => {
            let ptr = module.next_label("strlen_ptr");
            let len = module.next_label("strlen_len");
            module.declare_i32_local(ptr.trim_start_matches('$').to_string());
            module.declare_i32_local(len.trim_start_matches('$').to_string());
            emit_string_value_to_locals(arg, &ptr, &len, module)?;
            module.body().line(&format!("local.get {}", len));
            module.body().line("i64.extend_i32_u");
            Ok(ValueKind::Int)
        }
        _ => Err(CompileError::new(
            arg.span,
            "wasm32-web strlen() currently supports string literals, string variables, and string-returning user functions",
        )),
    }
}

pub(in crate::codegen::wasm) fn emit_ord_call(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let [arg] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web ord() expects exactly one argument",
        ));
    };
    if let Some(err) = unsupported_string_coercion_expr(arg) {
        return Err(err);
    }
    if let Some(err) = unsupported_object_string_coercion_expr(arg, module) {
        return Err(err);
    }
    if let Some(value) = static_string_value(arg, module) {
        let byte = value.as_bytes().first().copied().unwrap_or(0);
        module.body().line(&format!("i64.const {}", byte));
        return Ok(ValueKind::Int);
    }
    if let Some(kind) = known_mixed_value_cell_kind(arg, module)? {
        match kind {
            ValueCellKind::Str => {
                let Some(cell) = materialize_mixed_value_cell(arg, module)? else {
                    return Err(CompileError::new(
                        arg.span,
                        "wasm32-web ord() could not materialize mixed value cell",
                    ));
                };
                emit_value_cell_pointer_ord_value(&format!("${cell}"), module);
            }
            ValueCellKind::Int => {
                let Some(cell) = materialize_mixed_value_cell(arg, module)? else {
                    return Err(CompileError::new(
                        arg.span,
                        "wasm32-web ord() could not materialize mixed value cell",
                    ));
                };
                emit_mixed_i64_payload(&cell, module);
                emit_i64_stack_string_cast_value_to_stack("mixed_ord_int", module);
                emit_stack_string_ord_value("mixed_ord_int", module);
            }
            ValueCellKind::Float => {
                let Some(cell) = materialize_mixed_value_cell(arg, module)? else {
                    return Err(CompileError::new(
                        arg.span,
                        "wasm32-web ord() could not materialize mixed value cell",
                    ));
                };
                emit_mixed_f64_payload(&cell, module);
                emit_f64_stack_string_cast_value_to_stack("mixed_ord_float", module);
                emit_stack_string_ord_value("mixed_ord_float", module);
            }
            ValueCellKind::Bool => {
                let Some(cell) = materialize_mixed_value_cell(arg, module)? else {
                    return Err(CompileError::new(
                        arg.span,
                        "wasm32-web ord() could not materialize mixed value cell",
                    ));
                };
                emit_mixed_i64_payload(&cell, module);
                module.body().line("i64.const 0");
                module.body().line("i64.ne");
                module.body().open("if (result i64)");
                module.body().line("i64.const 49");
                module.body().line("else");
                module.body().line("i64.const 0");
                module.body().close("end");
            }
            ValueCellKind::Null => {
                module.body().line("i64.const 0");
            }
            ValueCellKind::Array => {
                return Err(CompileError::new(
                    arg.span,
                    "wasm32-web ord() currently supports scalar mixed values",
                ));
            }
        }
        return Ok(ValueKind::Int);
    }
    if object_tostring_supported(arg, module) {
        if let Some(cell) = materialize_mixed_value_cell(arg, module)? {
            emit_mixed_object_or_dynamic_ord_value(arg, &cell, module)?;
            return Ok(ValueKind::Int);
        }
        let ptr = module.next_label("ord_tostring_ptr");
        let len = module.next_label("ord_tostring_len");
        module.declare_i32_local(ptr.trim_start_matches('$').to_string());
        module.declare_i32_local(len.trim_start_matches('$').to_string());
        emit_string_value_to_locals(arg, &ptr, &len, module)?;
        module.body().line(&format!("local.get {}", len));
        module.body().line("i32.eqz");
        module.body().open("if (result i64)");
        module.body().line("i64.const 0");
        module.body().line("else");
        module.body().line(&format!("local.get {}", ptr));
        module.body().line("i32.load8_u");
        module.body().line("i64.extend_i32_u");
        module.body().close("end");
        return Ok(ValueKind::Int);
    }
    if let Some(cell) = materialize_mixed_value_cell(arg, module)? {
        emit_dynamic_mixed_ord_value(&cell, module);
        return Ok(ValueKind::Int);
    }
    match &arg.kind {
        ExprKind::StringLiteral(value) => {
            let byte = value.as_bytes().first().copied().unwrap_or(0);
            module.body().line(&format!("i64.const {}", byte));
            Ok(ValueKind::Int)
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Str) => {
            module.body().line(&format!("local.get ${}_len", name));
            module.body().line("i32.eqz");
            module.body().open("if (result i64)");
            module.body().line("i64.const 0");
            module.body().line("else");
            module.body().line(&format!("local.get ${}_ptr", name));
            module.body().line("i32.load8_u");
            module.body().line("i64.extend_i32_u");
            module.body().close("end");
            Ok(ValueKind::Int)
        }
        _ if expression_is_stringy(arg, module) => {
            let ptr = module.next_label("ord_ptr");
            let len = module.next_label("ord_len");
            module.declare_i32_local(ptr.trim_start_matches('$').to_string());
            module.declare_i32_local(len.trim_start_matches('$').to_string());
            emit_string_value_to_locals(arg, &ptr, &len, module)?;
            module.body().line(&format!("local.get {}", len));
            module.body().line("i32.eqz");
            module.body().open("if (result i64)");
            module.body().line("i64.const 0");
            module.body().line("else");
            module.body().line(&format!("local.get {}", ptr));
            module.body().line("i32.load8_u");
            module.body().line("i64.extend_i32_u");
            module.body().close("end");
            Ok(ValueKind::Int)
        }
        _ => Err(CompileError::new(
            arg.span,
            "wasm32-web ord() currently supports string literals, string variables, and string-returning user functions",
        )),
    }
}

fn emit_mixed_object_or_dynamic_strlen_length(
    arg: &Expr,
    cell: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let ptr = module.next_label("strlen_mixed_tostring_ptr");
    let len = module.next_label("strlen_mixed_tostring_len");
    module.declare_i32_local(ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(len.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${cell}"));
    module.body().line("call $__rt_mixed_tag");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_OBJECT));
    module.body().line("i32.eq");
    module.body().open("if (result i64)");
    match emit_mixed_object_no_arg_method_call(arg, cell, "__toString", &[], module)? {
        ValueKind::Str => {}
        _ => unreachable!("supported mixed object __toString metadata must return a string"),
    }
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.set {}", ptr));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i64.extend_i32_u");
    module.body().line("else");
    emit_dynamic_mixed_strlen_length(cell, module);
    module.body().close("end");
    Ok(())
}

fn emit_mixed_object_or_dynamic_ord_value(
    arg: &Expr,
    cell: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let ptr = module.next_label("ord_mixed_tostring_ptr");
    let len = module.next_label("ord_mixed_tostring_len");
    module.declare_i32_local(ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(len.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${cell}"));
    module.body().line("call $__rt_mixed_tag");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_OBJECT));
    module.body().line("i32.eq");
    module.body().open("if (result i64)");
    match emit_mixed_object_no_arg_method_call(arg, cell, "__toString", &[], module)? {
        ValueKind::Str => {}
        _ => unreachable!("supported mixed object __toString metadata must return a string"),
    }
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.set {}", ptr));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.eqz");
    module.body().open("if (result i64)");
    module.body().line("i64.const 0");
    module.body().line("else");
    module.body().line(&format!("local.get {}", ptr));
    module.body().line("i32.load8_u");
    module.body().line("i64.extend_i32_u");
    module.body().close("end");
    module.body().line("else");
    emit_dynamic_mixed_ord_value(cell, module);
    module.body().close("end");
    Ok(())
}

pub(in crate::codegen::wasm) fn is_output_string_builtin(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "strtolower"
            | "strtoupper"
            | "lcfirst"
            | "ucfirst"
            | "ucwords"
            | "strrev"
            | "trim"
            | "ltrim"
            | "rtrim"
            | "str_repeat"
            | "substr"
            | "chr"
            | "addslashes"
            | "stripslashes"
            | "bin2hex"
            | "hex2bin"
            | "nl2br"
            | "str_pad"
            | "substr_replace"
            | "str_replace"
            | "str_ireplace"
            | "strstr"
            | "wordwrap"
            | "urlencode"
            | "urldecode"
            | "rawurlencode"
            | "rawurldecode"
            | "base64_encode"
            | "base64_decode"
            | "htmlspecialchars"
            | "htmlentities"
            | "html_entity_decode"
            | "md5"
            | "sha1"
            | "hash"
            | "implode"
            | "number_format"
            | "sprintf"
            | "basename"
            | "dirname"
            | "pathinfo"
            | "json_encode"
            | "json_last_error_msg"
            | "get_class"
            | "get_parent_class"
    )
}

pub(in crate::codegen::wasm) fn is_output_optional_int_builtin(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "strpos" | "strrpos" | "array_search"
    )
}

pub(in crate::codegen::wasm) fn static_or_tracked_string_value(expr: &Expr, module: &WasmModule) -> Option<String> {
    match &expr.kind {
        ExprKind::Variable(name) => module.string_static_value(name),
        ExprKind::FunctionCall { name, args } if args.is_empty() => {
            module.function_static_string_return(name)
        }
        _ => static_string_value(expr, module),
    }
}





pub(in crate::codegen::wasm) fn eval_output_string_builtin(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &WasmModule,
) -> Result<String, CompileError> {
    let normalized_args = normalize_static_string_args(args, module);
    let args = normalized_args.as_slice();
    match name.to_ascii_lowercase().as_str() {
        "strtolower" | "strtoupper" | "lcfirst" | "ucfirst" | "ucwords" | "strrev"
        | "addslashes" | "stripslashes" | "bin2hex" | "hex2bin"
        | "nl2br" | "urlencode" | "urldecode" | "rawurlencode" | "rawurldecode"
        | "base64_encode" => {
            let [arg] = args else {
                return Err(CompileError::new(
                    call.span,
                    &format!("wasm32-web {}() expects exactly one argument", name),
                ));
            };
            let value = static_ascii_string_arg(call, arg, module)?;
            let result = match name.to_ascii_lowercase().as_str() {
                "strtolower" => ascii_lower(&value),
                "strtoupper" => ascii_upper(&value),
                "lcfirst" => ascii_lcfirst(&value),
                "ucfirst" => ascii_ucfirst(&value),
                "ucwords" => ascii_ucwords(&value),
                "strrev" => value.as_bytes().iter().rev().map(|b| char::from(*b)).collect(),
                "addslashes" => addslashes(&value),
                "stripslashes" => stripslashes(&value),
                "bin2hex" => bin2hex(value.as_bytes()),
                "hex2bin" => hex2bin(call, &value)?,
                "nl2br" => nl2br(&value),
                "urlencode" => url_encode(&value, SpaceEncoding::Plus),
                "urldecode" => url_decode(&value, SpaceEncoding::Plus)?,
                "rawurlencode" => url_encode(&value, SpaceEncoding::Percent20),
                "rawurldecode" => url_decode(&value, SpaceEncoding::Percent20)?,
                "base64_encode" => base64_encode(value.as_bytes()),
                _ => unreachable!(),
            };
            Ok(result)
        }
        "htmlspecialchars" | "htmlentities" => {
            if args.is_empty() || args.len() > 4 {
                return Err(CompileError::new(
                    call.span,
                    &format!("wasm32-web {}() expects one to four literal arguments", name),
                ));
            }
            let value = static_ascii_string_arg(call, &args[0], module)?;
            let flags = args
                .get(1)
                .map(const_or_literal_int_arg)
                .transpose()?
                .unwrap_or(3);
            validate_html_encoding(call, args.get(2), module)?;
            let double_encode = args.get(3).map(literal_bool_arg).transpose()?.unwrap_or(true);
            Ok(html_escape(name, &value, flags, double_encode))
        }
        "html_entity_decode" => {
            if args.is_empty() || args.len() > 3 {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web html_entity_decode() expects one to three literal arguments",
                ));
            }
            let value = static_ascii_string_arg(call, &args[0], module)?;
            let flags = args
                .get(1)
                .map(const_or_literal_int_arg)
                .transpose()?
                .unwrap_or(3);
            validate_html_encoding(call, args.get(2), module)?;
            Ok(html_entity_decode(&value, flags))
        }
        "base64_decode" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web base64_decode() expects one or two arguments",
                ));
            }
            let value = static_ascii_string_arg(call, &args[0], module)?;
            let strict = args
                .get(1)
                .map(literal_bool_arg)
                .transpose()?
                .unwrap_or(false);
            base64_decode(call, &value, strict)
        }
        "trim" | "ltrim" | "rtrim" => eval_literal_trim(call, name, args),
        "chr" => {
            let [codepoint] = args else {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web chr() expects exactly one argument",
                ));
            };
            Ok(char::from(literal_int_arg(codepoint)? as u8).to_string())
        }
        "str_repeat" => {
            let [string, times] = args else {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web str_repeat() expects exactly two arguments",
                ));
            };
            let string = static_ascii_string_arg(call, string, module)?;
            let times = literal_int_arg(times)?;
            if times < 0 {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web str_repeat() does not support negative repeat counts",
                ));
            }
            Ok(string.repeat(times as usize))
        }
        "substr" => eval_literal_substr(call, args),
        "str_pad" => eval_literal_str_pad(call, args),
        "substr_replace" => eval_literal_substr_replace(call, args),
        "str_replace" | "str_ireplace" => eval_literal_str_replace(call, name, args),
        "strstr" => eval_literal_strstr(call, args, module),
        "wordwrap" => eval_literal_wordwrap(call, args),
        "md5" => eval_literal_md5(call, args),
        "sha1" => eval_literal_sha1(call, args),
        "hash" => eval_literal_hash(call, args),
        "implode" => eval_literal_implode(call, args),
        "number_format" => eval_literal_number_format(call, args),
        "sprintf" => eval_literal_sprintf(call, args),
        "basename" => eval_literal_basename(call, args),
        "dirname" => eval_literal_dirname(call, args),
        "pathinfo" => eval_literal_pathinfo(call, args),
        "json_encode" => eval_literal_json_encode(call, args),
        _ => unreachable!(),
    }
}
