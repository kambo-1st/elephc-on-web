//! Purpose:
//! Lowers PHP string search builtins for wasm32-web.
//! Handles static and runtime `strpos`/`strrpos` style searches that return optional integer positions.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` output and string-search dispatch.
//!
//! Key details:
//! - Runtime searches preserve PHP offset bounds and empty-needle behavior while using `-1` internally for "not found" index consumers.

use super::*;
use super::string_search_find::{
    emit_runtime_empty_search_result, emit_runtime_find_literal_forward,
    emit_runtime_find_literal_reverse, emit_runtime_find_var_forward,
    emit_runtime_find_var_reverse,
};
use super::string_search_literals::eval_literal_string_int_call;

pub(super) fn emit_output_optional_int_builtin(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if emit_runtime_output_optional_int_builtin(call, name, args, module)? {
        return Ok(());
    }
    if let Some(result) = eval_literal_string_int_call(call, name, args, module)? {
        module.body().line(&format!("i64.const {}", result));
        module.body().line("call $host_write_int");
    }
    Ok(())
}

fn emit_runtime_output_optional_int_builtin(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if name.eq_ignore_ascii_case("array_search") {
        emit_output_array_search(call, args, module)?;
        return Ok(true);
    }
    if args.len() < 2 || args.len() > 3 {
        return Err(CompileError::new(
            call.span,
            &format!("wasm32-web {}() expects two or three arguments", name),
        ));
    }
    let haystack = &args[0];
    let needle_arg = &args[1];
    let haystack_storage;
    let var = if let Some(var) = runtime_string_arg_or_materialize(haystack, "strpos_haystack", module)? {
        var
    } else if string_cast_value_supported(haystack, module) {
        haystack_storage = materialize_string_cast_expr(haystack, "strpos_haystack", module)?;
        haystack_storage
    } else {
        return Ok(false);
    };
    let needle_var =
        if let Some(needle_var) = runtime_string_arg_or_materialize(needle_arg, "strpos_needle", module)? {
            Some(needle_var)
        } else if string_cast_value_supported(needle_arg, module)
            && static_string_value(needle_arg, module).is_none()
        {
            Some(materialize_string_cast_expr(needle_arg, "strpos_needle", module)?)
        } else {
            None
        };
    let needle = if needle_var.is_none() {
        Some(static_ascii_string_arg(call, needle_arg, module)?)
    } else {
        None
    };
    let offset = args.get(2).map(literal_int_arg).transpose()?.unwrap_or(0);
    match name.to_ascii_lowercase().as_str() {
        "strpos" => {
            if let Some(needle_var) = needle_var {
                emit_runtime_output_strpos_var(&var, &needle_var, false, offset, module);
            } else {
                emit_runtime_output_strpos(&var, &needle.unwrap(), false, offset, module);
            }
        }
        "strrpos" => {
            if let Some(needle_var) = needle_var {
                emit_runtime_output_strpos_var(&var, &needle_var, true, offset, module);
            } else {
                emit_runtime_output_strpos(&var, &needle.unwrap(), true, offset, module);
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

pub(super) fn emit_string_search_index_from_args(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(CompileError::new(
            call.span,
            &format!("wasm32-web {}() expects two or three arguments", name),
        ));
    }
    let haystack = &args[0];
    let needle_arg = &args[1];
    let haystack_storage;
    let var = if let Some(var) = runtime_string_arg_or_materialize(haystack, "strpos_haystack", module)? {
        Some(var)
    } else if string_cast_value_supported(haystack, module) {
        haystack_storage = materialize_string_cast_expr(haystack, "strpos_haystack", module)?;
        Some(haystack_storage)
    } else {
        None
    };
    let Some(var) = var else {
        let result = eval_literal_string_int_call(call, name, args, module)?;
        module.body().line(&format!("i64.const {}", result.unwrap_or(-1)));
        return Ok(());
    };
    let needle_var =
        if let Some(needle_var) = runtime_string_arg_or_materialize(needle_arg, "strpos_needle", module)? {
            Some(needle_var)
        } else if string_cast_value_supported(needle_arg, module)
            && static_string_value(needle_arg, module).is_none()
        {
            Some(materialize_string_cast_expr(needle_arg, "strpos_needle", module)?)
        } else {
            None
        };
    let offset = args.get(2).map(literal_int_arg).transpose()?.unwrap_or(0);
    match name.to_ascii_lowercase().as_str() {
        "strpos" => {
            if let Some(needle_var) = needle_var {
                emit_runtime_strpos_index_var(&var, &needle_var, false, offset, module);
            } else {
                let needle = static_ascii_string_arg(call, needle_arg, module)?;
                emit_runtime_strpos_index(&var, &needle, false, offset, module);
            }
        }
        "strrpos" => {
            if let Some(needle_var) = needle_var {
                emit_runtime_strpos_index_var(&var, &needle_var, true, offset, module);
            } else {
                let needle = static_ascii_string_arg(call, needle_arg, module)?;
                emit_runtime_strpos_index(&var, &needle, true, offset, module);
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
fn emit_runtime_output_strpos(
    var: &str,
    needle: &str,
    reverse: bool,
    offset: i64,
    module: &mut WasmModule,
) {
    let result = module.next_label("strpos_result");
    let found = module.next_label("strpos_found");
    module.declare_i32_local(result.trim_start_matches('$').to_string());
    module.declare_i32_local(found.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", result));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    if needle.is_empty() {
        emit_runtime_empty_search_result(var, reverse, offset, module);
        module.body().line(&format!("local.set {}", result));
        module.body().line("i32.const 1");
        module.body().line(&format!("local.set {}", found));
    } else if reverse {
        emit_runtime_find_literal_reverse(var, needle, offset, &result, &found, module);
    } else {
        emit_runtime_find_literal_forward(var, needle, offset, &result, &found, module);
    }
    module.body().line(&format!("local.get {}", found));
    module.body().open("if");
    module.body().line(&format!("local.get {}", result));
    module.body().line("i64.extend_i32_u");
    module.body().line("call $host_write_int");
    module.body().close("end");
}

fn emit_runtime_output_strpos_var(
    var: &str,
    needle_var: &str,
    reverse: bool,
    offset: i64,
    module: &mut WasmModule,
) {
    let result = module.next_label("strpos_result");
    let found = module.next_label("strpos_found");
    module.declare_i32_local(result.trim_start_matches('$').to_string());
    module.declare_i32_local(found.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", result));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("local.get ${}_len", needle_var));
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_runtime_empty_search_result(var, reverse, offset, module);
    module.body().line(&format!("local.set {}", result));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line("else");
    if reverse {
        emit_runtime_find_var_reverse(var, needle_var, offset, &result, &found, module);
    } else {
        emit_runtime_find_var_forward(var, needle_var, offset, &result, &found, module);
    }
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
    module.body().open("if");
    module.body().line(&format!("local.get {}", result));
    module.body().line("i64.extend_i32_u");
    module.body().line("call $host_write_int");
    module.body().close("end");
}

fn emit_runtime_strpos_index(
    var: &str,
    needle: &str,
    reverse: bool,
    offset: i64,
    module: &mut WasmModule,
) {
    let result = module.next_label("strpos_index_result");
    let found = module.next_label("strpos_index_found");
    module.declare_i32_local(result.trim_start_matches('$').to_string());
    module.declare_i32_local(found.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", result));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    if needle.is_empty() {
        emit_runtime_empty_search_result(var, reverse, offset, module);
        module.body().line(&format!("local.set {}", result));
        module.body().line("i32.const 1");
        module.body().line(&format!("local.set {}", found));
    } else if reverse {
        emit_runtime_find_literal_reverse(var, needle, offset, &result, &found, module);
    } else {
        emit_runtime_find_literal_forward(var, needle, offset, &result, &found, module);
    }
    emit_runtime_search_result_as_i64(&result, &found, module);
}

fn emit_runtime_strpos_index_var(
    var: &str,
    needle_var: &str,
    reverse: bool,
    offset: i64,
    module: &mut WasmModule,
) {
    let result = module.next_label("strpos_index_result");
    let found = module.next_label("strpos_index_found");
    module.declare_i32_local(result.trim_start_matches('$').to_string());
    module.declare_i32_local(found.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", result));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("local.get ${}_len", needle_var));
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_runtime_empty_search_result(var, reverse, offset, module);
    module.body().line(&format!("local.set {}", result));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line("else");
    if reverse {
        emit_runtime_find_var_reverse(var, needle_var, offset, &result, &found, module);
    } else {
        emit_runtime_find_var_forward(var, needle_var, offset, &result, &found, module);
    }
    module.body().close("end");
    emit_runtime_search_result_as_i64(&result, &found, module);
}

fn emit_runtime_search_result_as_i64(result: &str, found: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", found));
    module.body().open("if (result i64)");
    module.body().line(&format!("local.get {}", result));
    module.body().line("i64.extend_i32_u");
    module.body().line("else");
    module.body().line("i64.const -1");
    module.body().close("end");
}
