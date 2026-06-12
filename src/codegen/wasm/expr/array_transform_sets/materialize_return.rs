//! Purpose:
//! Materializes user-function array returns into temporary wasm32-web array locals.
//! Keeps return-layout and metadata staging out of the array-transform dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_transform_sets::emit_indexed_array_transform_assign`.
//!
//! Key details:
//! - The staged local must preserve layout, length, value-cell metadata, nested metadata, and assoc key metadata.

use super::*;

pub(in crate::codegen::wasm::expr) fn materialize_function_array_return_local(
    expr: &Expr,
    callee_name: &Name,
    call_args: &[Expr],
    prefix: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let Some(len) = module.function_array_return_length(callee_name) else {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array transforms require a known function return length",
        ));
    };
    let temp = module.next_label(prefix).trim_start_matches('$').to_string();
    module.declare_array_local(temp.clone());
    emit_user_function_args(expr, callee_name, call_args, module)?;
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callee_name)));
    module.body().line(&format!("local.set ${}_len", temp));
    module.body().line(&format!("local.set ${}_ptr", temp));
    let layout = module.function_array_return_layout(callee_name);
    module.set_array_layout(&temp, layout);
    module.set_array_length(&temp, len);
    if layout == ArrayLayout::Value {
        module.set_array_value_cell_kinds(
            &temp,
            module
                .function_array_return_value_kinds(callee_name)
                .map(|kinds| kinds.to_vec()),
        );
        module.set_array_nested_value_metadata(
            &temp,
            module
                .function_array_return_nested_values(callee_name)
                .map(|metadata| metadata.to_vec()),
        );
    } else if layout == ArrayLayout::Assoc {
        module.set_array_value_cell_kinds(
            &temp,
            module
                .function_array_return_value_kinds(callee_name)
                .map(|kinds| kinds.to_vec()),
        );
        module.set_array_nested_value_metadata(
            &temp,
            module
                .function_array_return_nested_values(callee_name)
                .map(|metadata| metadata.to_vec()),
        );
        module.set_array_key_kinds(
            &temp,
            module
                .function_array_return_key_kinds(callee_name)
                .map(|kinds| kinds.to_vec()),
        );
        module.set_array_key_values(
            &temp,
            module
                .function_array_return_key_values(callee_name)
                .map(|values| values.to_vec()),
        );
    } else {
        module.set_array_value_cell_kinds(&temp, None);
        module.set_array_nested_value_metadata(&temp, None);
    }
    Ok(temp)
}
