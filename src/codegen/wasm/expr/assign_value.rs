//! Purpose:
//! Lowers wasm32-web assignment into predeclared locals by local kind.
//! Keeps assignment-specific dispatch out of the main expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::stmt` for PHP variable assignment statements.
//!
//! Key details:
//! - Updates wasm local static metadata and delegates strings, arrays, mixed values, and callables to focused helpers.

use super::*;

pub(in crate::codegen::wasm) fn emit_assign_value(
    name: &str,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match module.local_kind(name) {
        Some(LocalKind::Str) => {
            emit_string_assign(name, value, module)?;
            if let Some(address_local) = module.str_ref_alias(name) {
                module.body().line(&format!("local.get {}", address_local));
                module.body().open("if");
                module.body().line(&format!("local.get {}", address_local));
                module.body().line(&format!("local.get ${}_ptr", name));
                module.body().line("i32.store");
                module.body().line(&format!("local.get {}", address_local));
                module.body().line("i32.const 4");
                module.body().line("i32.add");
                module.body().line(&format!("local.get ${}_len", name));
                module.body().line("i32.store");
                module.body().close("end");
            }
            Ok(())
        }
        Some(LocalKind::Array) => {
            emit_array_assign(name, value, module)?;
            if let Some(address_local) = module.array_ref_alias(name) {
                module.body().line(&format!("local.get {}", address_local));
                module.body().open("if");
                module.body().line(&format!("local.get {}", address_local));
                module.body().line(&format!("local.get ${}_ptr", name));
                module.body().line("i32.store");
                module.body().line(&format!("local.get {}", address_local));
                module.body().line("i32.const 4");
                module.body().line("i32.add");
                module.body().line(&format!("local.get ${}_len", name));
                module.body().line("i32.store");
                module.body().close("end");
                if let Some(source) = module.array_ref_alias_source(name) {
                    module.set_array_nested_value_metadata(&source, None);
                    module.set_array_runtime_nested_value_metadata(&source, None);
                }
            }
            Ok(())
        }
        Some(LocalKind::I64) => {
            if is_array_value_expr(value) {
                return Err(array_unsupported(value));
            }
            let static_value = static_or_const_int_value(value);
            require_int(value, module)?;
            module.body().line(&format!("local.set ${}", name));
            if let Some(address_local) = module.i64_ref_alias(name) {
                module.body().line(&format!("local.get {}", address_local));
                module.body().open("if");
                module.body().line(&format!("local.get {}", address_local));
                module.body().line(&format!("local.get ${}", name));
                module.body().line("i64.store");
                module.body().close("end");
            }
            module.set_i64_static_value(name, static_value);
            Ok(())
        }
        Some(LocalKind::F64) => {
            if is_array_value_expr(value) {
                return Err(array_unsupported(value));
            }
            let static_value = static_or_const_or_f64_local_value(value, module);
            require_float(value, module)?;
            module.body().line(&format!("local.set ${}", name));
            if let Some(address_local) = module.f64_ref_alias(name) {
                module.body().line(&format!("local.get {}", address_local));
                module.body().open("if");
                module.body().line(&format!("local.get {}", address_local));
                module.body().line(&format!("local.get ${}", name));
                module.body().line("f64.store");
                module.body().close("end");
            }
            module.set_f64_static_value(name, static_value);
            Ok(())
        }
        Some(LocalKind::I32) => {
            if is_array_value_expr(value) {
                return Err(array_unsupported(value));
            }
            let static_value = static_or_const_or_i32_bool_value(value, module);
            emit_condition(value, module)?;
            module.body().line(&format!("local.set ${}", name));
            if let Some(address_local) = module.i32_ref_alias(name) {
                module.body().line(&format!("local.get {}", address_local));
                module.body().open("if");
                module.body().line(&format!("local.get {}", address_local));
                module.body().line(&format!("local.get ${}", name));
                module.body().line("i64.extend_i32_u");
                module.body().line("i64.store");
                module.body().close("end");
            }
            module.set_bool_static_value(name, static_value);
            Ok(())
        }
        Some(LocalKind::Mixed) => {
            emit_mixed_assign(name, value, module)?;
            if let Some(address_local) = module.mixed_ref_alias(name) {
                module.body().line(&format!("local.get {}", address_local));
                module.body().open("if");
                emit_copy_value_cell_from_addr_to_addr(&address_local, &format!("${}", name), module);
                module.body().close("end");
            }
            Ok(())
        }
        Some(LocalKind::Object) => emit_object_assign(name, value, module),
        Some(LocalKind::Callable) => emit_callable_assign(name, value, module),
        None => Err(CompileError::new(
            value.span,
            "wasm32-web assignment target was not predeclared",
        )),
    }
}
