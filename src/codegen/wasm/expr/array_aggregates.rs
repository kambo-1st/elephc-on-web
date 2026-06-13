//! Purpose:
//! Lowers wasm32-web array count, numeric fold, and random-key expression builtins.
//! Keeps aggregate array helpers out of the main expression dispatcher.
//!
//! Called from:
//! - `super::emit_expr()` and array indexing helpers.
//!
//! Key details:
//! - Runtime paths preserve PHP-compatible traps or `CompileError`s for unsupported shapes.
//! - Value-cell folds share the boxed mixed-cell contract with the wasm runtime.

use super::*;
use super::array_indexing_nested::emit_static_assoc_array_child_parts;

pub(super) fn emit_count_call(
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if args.len() != 1 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web count() expects exactly one argument",
        ));
    }
    if emit_direct_array_chunk_first_count(&args[0], module)? {
        return Ok(ValueKind::Int);
    }
    if let Some(len) = static_enum_cases_len(&args[0], module) {
        module.body().line(&format!("i64.const {}", len));
        return Ok(ValueKind::Int);
    }
    match &args[0].kind {
        ExprKind::ArrayLiteral(items) => {
            module.body().line(&format!("i64.const {}", items.len()));
            Ok(ValueKind::Int)
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            let len = normalize_assoc_items(items)
                .map(|items| items.len())
                .unwrap_or(items.len());
            module.body().line(&format!("i64.const {}", len));
            Ok(ValueKind::Int)
        }
        ExprKind::ConstRef(name) => match module.array_constant_value(name) {
            Some(ConstantArrayValue::Indexed(items)) => {
                module.body().line(&format!("i64.const {}", items.len()));
                Ok(ValueKind::Int)
            }
            Some(ConstantArrayValue::Assoc(items)) => {
                let len = normalize_assoc_items(&items)
                    .map(|items| items.len())
                    .unwrap_or(items.len());
                module.body().line(&format!("i64.const {}", len));
                Ok(ValueKind::Int)
            }
            None => Err(CompileError::new(
                args[0].span,
                "wasm32-web count() currently supports indexed array values only",
            )),
        },
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Array) => {
            module.body().line(&format!("local.get ${}_len", name));
            module.body().line("i64.extend_i32_u");
            Ok(ValueKind::Int)
        }
        ExprKind::Variable(name)
            if module.local_kind(name) == Some(LocalKind::Mixed)
                && module.mixed_value_cell_kind(name) == Some(ValueCellKind::Array) =>
        {
            emit_mixed_array_count_from_cell(&format!("${}", name), module);
            Ok(ValueKind::Int)
        }
        ExprKind::Variable(name)
            if module.local_kind(name) == Some(LocalKind::Mixed)
                && module.mixed_value_cell_kind(name).is_none() =>
        {
            emit_runtime_checked_mixed_array_count_from_cell(&format!("${}", name), module);
            Ok(ValueKind::Int)
        }
        ExprKind::MethodCall { object, method, .. }
        | ExprKind::NullsafeMethodCall { object, method, .. }
            if method_call_return_kind(object, method, module) == Some(ValueKind::Array) =>
        {
            let len = module.next_label("count_method_array_len");
            module.declare_i32_local(len.trim_start_matches('$').to_string());
            match emit_expr(&args[0], module)? {
                ValueKind::Array => {
                    module.body().line(&format!("local.set {}", len));
                    module.body().line("drop");
                    module.body().line(&format!("local.get {}", len));
                    module.body().line("i64.extend_i32_u");
                    Ok(ValueKind::Int)
                }
                _ => unreachable!("array-returning method metadata must emit an array value"),
            }
        }
        ExprKind::FunctionCall { name, args }
            if module.has_function(name)
                && module.function_return_kind(name) == Some(ValueKind::Mixed)
                && module.function_mixed_return_kind(name) == Some(ValueCellKind::Array) =>
        {
            let temp = module.next_label("count_mixed_array_return");
            module.declare_i32_local(temp.trim_start_matches('$').to_string());
            emit_user_function_args(expr, name, args, module)?;
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(name)));
            module.body().line(&format!("local.set {}", temp));
            emit_mixed_array_count_from_cell(&temp, module);
            Ok(ValueKind::Int)
        }
        ExprKind::FunctionCall { name, args }
            if module.has_function(name)
                && module.function_return_kind(name) == Some(ValueKind::Mixed)
                && module.function_mixed_return_kind(name).is_none() =>
        {
            let temp = module.next_label("count_unknown_mixed_array_return");
            module.declare_i32_local(temp.trim_start_matches('$').to_string());
            emit_user_function_args(expr, name, args, module)?;
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(name)));
            module.body().line(&format!("local.set {}", temp));
            emit_runtime_checked_mixed_array_count_from_cell(&temp, module);
            Ok(ValueKind::Int)
        }
        ExprKind::ArrayAccess { .. } => {
            if emit_partial_static_nested_assoc_count(&args[0], module)? {
                return Ok(ValueKind::Int);
            }
            if array_access_uses_runtime_nested_metadata(&args[0], module) {
                let len = module.next_label("count_array_access_len");
                module.declare_i32_local(len.trim_start_matches('$').to_string());
                match emit_expr(&args[0], module)? {
                    ValueKind::Array => {
                        module.body().line(&format!("local.set {}", len));
                        module.body().line("drop");
                        module.body().line(&format!("local.get {}", len));
                        module.body().line("i64.extend_i32_u");
                        return Ok(ValueKind::Int);
                    }
                    _ => {
                        return Err(CompileError::new(
                            args[0].span,
                            "wasm32-web count() requires an indexed array value",
                        ));
                    }
                }
            }
            if !array_access_parent_has_runtime_array_cells(&args[0], module) {
                if let Some(metadata) = nested_array_metadata_for_access_expr(&args[0], module) {
                    if metadata.layout != ArrayLayout::Assoc || metadata.key_values.is_some() {
                        module.body().line(&format!("i64.const {}", metadata.len));
                        return Ok(ValueKind::Int);
                    }
                }
            }
            let len = module.next_label("count_array_access_len");
            module.declare_i32_local(len.trim_start_matches('$').to_string());
            match emit_expr(&args[0], module)? {
                ValueKind::Array => {
                    module.body().line(&format!("local.set {}", len));
                    module.body().line("drop");
                    module.body().line(&format!("local.get {}", len));
                    module.body().line("i64.extend_i32_u");
                    Ok(ValueKind::Int)
                }
                _ => Err(CompileError::new(
                    args[0].span,
                    "wasm32-web count() requires an indexed array value",
                )),
            }
        }
        _ => {
            if let Some(cell) = materialize_mixed_value_cell(&args[0], module)? {
                emit_runtime_checked_mixed_array_count_from_cell(&format!("${}", cell), module);
                return Ok(ValueKind::Int);
            }
            if matches!(args[0].kind, ExprKind::Match { .. }) {
                let len = module.next_label("count_match_array_len");
                module.declare_i32_local(len.trim_start_matches('$').to_string());
                match emit_expr(&args[0], module)? {
                    ValueKind::Array => {
                        module.body().line(&format!("local.set {}", len));
                        module.body().line("drop");
                        module.body().line(&format!("local.get {}", len));
                        module.body().line("i64.extend_i32_u");
                        return Ok(ValueKind::Int);
                    }
                    _ => {
                        return Err(CompileError::new(
                            args[0].span,
                            "wasm32-web count() requires an array value",
                        ));
                    }
                }
            }
            if expression_is_arrayy(&args[0], module) || expression_has_array_type(&args[0], module) {
                let temp = module
                    .next_label("count_array_expr")
                    .trim_start_matches('$')
                    .to_string();
                module.declare_array_local(temp.clone());
                emit_array_assign(&temp, &args[0], module)?;
                module.body().line(&format!("local.get ${}_len", temp));
                module.body().line("i64.extend_i32_u");
                return Ok(ValueKind::Int);
            }
            Err(CompileError::new(
                args[0].span,
                "wasm32-web count() currently supports indexed array values only",
            ))
        }
    }
}

fn emit_partial_static_nested_assoc_count(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let ExprKind::ArrayAccess { array: parent, index } = &expr.kind else {
        return Ok(false);
    };
    let Some(child_key) = static_assoc_access_key(index, module) else {
        return Ok(false);
    };
    let Some(parent_metadata) = nested_array_metadata_for_access_expr(parent, module) else {
        return Ok(false);
    };
    if parent_metadata.layout != ArrayLayout::Assoc {
        return Ok(false);
    }
    let Some(keys) = parent_metadata.key_values.as_ref() else {
        return Ok(false);
    };
    if keys.len() == parent_metadata.len || keys.iter().any(|key| *key == child_key) {
        return Ok(false);
    }
    let has_array_child = parent_metadata
        .nested_values
        .as_ref()
        .is_some_and(|values| values.iter().skip(keys.len()).flatten().any(|metadata| {
            metadata.layout == ArrayLayout::Assoc
        }));
    if !has_array_child {
        return Ok(false);
    }
    let parent_ptr = module.next_label("partial_nested_count_parent_ptr");
    let parent_len = module.next_label("partial_nested_count_parent_len");
    module.declare_i32_local(parent_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(parent_len.trim_start_matches('$').to_string());
    match emit_expr(parent, module)? {
        ValueKind::Array => {
            module.body().line(&format!("local.set {}", parent_len));
            module.body().line(&format!("local.set {}", parent_ptr));
        }
        _ => return Ok(false),
    }
    let (_, child_len) =
        emit_static_assoc_array_child_parts(&parent_ptr, &parent_len, &child_key, module);
    module.body().line(&format!("local.get {}", child_len));
    module.body().line("i64.extend_i32_u");
    Ok(true)
}

fn array_access_parent_has_runtime_array_cells(expr: &Expr, module: &WasmModule) -> bool {
    let ExprKind::ArrayAccess { array, .. } = &expr.kind else {
        return false;
    };
    match &array.kind {
        ExprKind::Variable(name) => module.array_runtime_value_cell_kind(name) == Some(ValueCellKind::Array),
        _ => false,
    }
}

fn array_access_uses_runtime_nested_metadata(expr: &Expr, module: &WasmModule) -> bool {
    let ExprKind::ArrayAccess { array, .. } = &expr.kind else {
        return false;
    };
    let ExprKind::Variable(name) = &array.kind else {
        return false;
    };
    module.local_kind(name) == Some(LocalKind::Array)
        && module.array_layout(name) == ArrayLayout::Value
        && module.array_runtime_nested_value_metadata(name).is_some()
}

fn emit_direct_array_chunk_first_count(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let ExprKind::ArrayAccess { array, index } = &expr.kind else {
        return Ok(false);
    };
    let Some(chunk_index) = static_or_const_int_value(index).and_then(|value| u32::try_from(value).ok()) else {
        return Ok(false);
    };
    let ExprKind::FunctionCall { name, args } = &array.kind else {
        return Ok(false);
    };
    if !name.eq_ignore_ascii_case("array_chunk") {
        return Ok(false);
    }
    let _preserve_keys = array_chunk_preserve_keys_arg(args, array.span, module)?;
    if args.len() < 2 {
        return Err(CompileError::new(
            array.span,
            "wasm32-web array_chunk() expects two or three arguments",
        ));
    }
    let Some(source_len) = known_indexed_array_expr_len(&args[0], module) else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web count(array_chunk(...)[n]) requires a known non-empty indexed source length",
        ));
    };
    if source_len == 0 {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web count(array_chunk(...)[n]) does not support empty sources yet",
        ));
    }
    let chunk_size = module.next_label("array_chunk_first_count_size");
    let chunk_start = module.next_label("array_chunk_count_start");
    let remaining = module.next_label("array_chunk_count_remaining");
    for local in [&chunk_size, &chunk_start, &remaining] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    require_int(&args[1], module)?;
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", chunk_size));
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.const 0");
    module.body().line("i32.le_s");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("i32.const {}", chunk_index));
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.mul");
    module.body().line(&format!("local.set {}", chunk_start));
    module.body().line(&format!("local.get {}", chunk_start));
    module.body().line(&format!("i32.const {}", source_len));
    module.body().line("i32.ge_u");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", chunk_start));
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.add");
    module.body().line(&format!("i32.const {}", source_len));
    module.body().line("i32.lt_u");
    module.body().open("if (result i64)");
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i64.extend_i32_u");
    module.body().line("else");
    module.body().line(&format!("i32.const {}", source_len));
    module.body().line(&format!("local.get {}", chunk_start));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", remaining));
    module.body().line(&format!("local.get {}", remaining));
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.lt_u");
    module.body().open("if (result i64)");
    module.body().line(&format!("local.get {}", remaining));
    module.body().line("i64.extend_i32_u");
    module.body().line("else");
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i64.extend_i32_u");
    module.body().close("end");
    module.body().close("end");
    Ok(true)
}

pub(super) fn known_indexed_array_expr_len(expr: &Expr, module: &WasmModule) -> Option<usize> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => Some(items.len()),
        ExprKind::Variable(name)
            if module.local_kind(name) == Some(LocalKind::Array)
                && module.array_layout(name) != ArrayLayout::Assoc =>
        {
            module.array_length(name)
        }
        ExprKind::FunctionCall { name, .. }
            if module.has_function(name)
                && module.function_return_kind(name) == Some(ValueKind::Array)
                && module.function_array_return_layout(name) != ArrayLayout::Assoc =>
        {
            module.function_array_return_length(name)
        }
        _ => None,
    }
}

fn emit_mixed_array_count_from_cell(cell: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line("call $__rt_mixed_count_array");
}

fn emit_runtime_checked_mixed_array_count_from_cell(cell: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line("call $__rt_mixed_count_array_checked");
}

pub(super) use super::array_numeric_folds::emit_numeric_array_fold_call;
pub(super) use super::array_rand::{emit_array_rand_assign, emit_array_rand_call};
