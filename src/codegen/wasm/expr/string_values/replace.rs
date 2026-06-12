//! Purpose:
//! Lowers str_replace()/str_ireplace() into wasm32-web heap string values.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::string_values`.
//!
//! Key details:
//! - Handles scalar and string-array search/replacement inputs.
//! - Count locals are updated when PHP's optional count argument is supported.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_str_replace_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let case_insensitive = match name.to_ascii_lowercase().as_str() {
        "str_replace" => false,
        "str_ireplace" => true,
        _ => return Ok(false),
    };
    if args.len() != 3 && args.len() != 4 {
        return Err(CompileError::new(
            call.span,
            &format!("wasm32-web {}() value support expects three string arguments plus an optional count local", name),
        ));
    }
    let count_local = str_replace_count_local(call, args, module)?;
    if let Some((value, count)) = eval_static_str_replace_args(call, name, args, module)? {
        if let Some(count_local) = count_local.as_deref() {
            emit_set_i64_local_const(count_local, count, module);
        }
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(true);
    }
    if count_local.is_none() && args.iter().all(|arg| static_string_value(arg, module).is_some()) {
        let value = eval_output_string_builtin(call, name, args, module)?;
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(true);
    }
    if let Some(search_array_name) =
        materialize_string_value_array_arg(&args[0], "str_replace_search_array", module)?
    {
        let search_array = search_array_name.as_str();
        if value_string_array_len(search_array, module).is_some() {
            let Some(subject_var) = string_arg_or_materialize(&args[2], "str_replace_array_subject", module)? else {
                return Ok(false);
            };
            let replacement_array =
                materialize_string_value_array_arg(&args[1], "str_replace_replacement_array", module)?;
            if let Some(replacement_array) = replacement_array.as_deref() {
                emit_runtime_str_replace_array_value_to_stack(
                    &subject_var,
                    search_array,
                    Some(replacement_array),
                    None,
                    case_insensitive,
                    count_local.as_deref(),
                    module,
                )?;
                return Ok(true);
            }
            let replacement_literal = static_string_value(&args[1], module);
            let replacement_var = if replacement_literal.is_none() {
                string_arg_or_materialize(&args[1], "str_replace_array_replacement", module)?
            } else {
                None
            };
            let replacement = replacement_literal
                .as_deref()
                .map(WasmReplacement::Literal)
                .or_else(|| replacement_var.as_deref().map(WasmReplacement::Variable));
            let Some(replacement) = replacement else {
                return Ok(false);
            };
            emit_runtime_str_replace_array_value_to_stack(
                &subject_var,
                search_array,
                None,
                Some(replacement),
                case_insensitive,
                count_local.as_deref(),
                module,
            )?;
            return Ok(true);
        }
        if runtime_string_value_array(search_array, module) {
            let Some(subject_var) = string_arg_or_materialize(&args[2], "str_replace_runtime_array_subject", module)? else {
                return Ok(false);
            };
            let replacement_array =
                materialize_string_value_array_arg(&args[1], "str_replace_runtime_replacement_array", module)?;
            if let Some(replacement_array) = replacement_array.as_deref() {
                emit_runtime_str_replace_dynamic_array_value_to_stack(
                    &subject_var,
                    search_array,
                    Some(replacement_array),
                    None,
                    case_insensitive,
                    count_local.as_deref(),
                    module,
                );
                return Ok(true);
            }
            let replacement_literal = static_string_value(&args[1], module);
            let replacement_var = if replacement_literal.is_none() {
                string_arg_or_materialize(&args[1], "str_replace_runtime_array_replacement", module)?
            } else {
                None
            };
            let replacement = replacement_literal
                .as_deref()
                .map(WasmReplacement::Literal)
                .or_else(|| replacement_var.as_deref().map(WasmReplacement::Variable));
            let Some(replacement) = replacement else {
                return Ok(false);
            };
            emit_runtime_str_replace_dynamic_array_value_to_stack(
                &subject_var,
                search_array,
                None,
                Some(replacement),
                case_insensitive,
                count_local.as_deref(),
                module,
            );
            return Ok(true);
        }
    }
    let search_literal = static_string_coercion_value(&args[0], module);
    let replacement_literal = static_string_coercion_value(&args[1], module);
    let search_var = if search_literal.is_none() {
        string_coercion_arg_or_materialize(&args[0], "str_replace_value_search", module)?
    } else {
        None
    };
    let replacement_var = if replacement_literal.is_none() {
        string_coercion_arg_or_materialize(&args[1], "str_replace_value_replacement", module)?
    } else {
        None
    };
    let Some(subject_var) = string_coercion_arg_or_materialize(&args[2], "str_replace_value_subject", module)? else {
        return Ok(false);
    };
    let search = if let Some(search) = search_literal.as_deref() {
        WasmSearch::Literal(search)
    } else {
        let Some(search_var) = search_var.as_deref() else {
            return Ok(false);
        };
        WasmSearch::Variable(search_var)
    };
    let replacement = if let Some(replacement) = replacement_literal.as_deref() {
        WasmReplacement::Literal(replacement)
    } else {
        let Some(replacement_var) = replacement_var.as_deref() else {
            return Ok(false);
        };
        WasmReplacement::Variable(replacement_var)
    };
    emit_runtime_str_replace_value_to_stack(
        &subject_var,
        search,
        replacement,
        case_insensitive,
        count_local.as_deref(),
        true,
        module,
    );
    Ok(true)
}
