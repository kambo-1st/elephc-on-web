//! Purpose:
//! Emits wasm32-web runtime lowering for PHP chr() output and string value contexts.
//! Keeps the PHP-facing chr helpers separate from shared byte-writing primitives.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::string_builtins`
//! - `crate::codegen::wasm::expr::string_values`
//!
//! Key details:
//! - Masks integer inputs to one byte to match the existing wasm32-web lowering.
//! - Direct output writes one byte; value mode allocates a one-byte heap string.

use super::*;

pub(super) fn emit_runtime_chr_output(expr: &Expr, module: &mut WasmModule) -> Result<(), CompileError> {
    require_int(expr, module)?;
    module.body().line("i32.wrap_i64");
    module.body().line("i32.const 255");
    module.body().line("i32.and");
    emit_write_stack_byte(module);
    Ok(())
}

pub(super) fn emit_runtime_chr_value_to_stack(expr: &Expr, module: &mut WasmModule) -> Result<(), CompileError> {
    let out_ptr = module.next_label("chr_value_ptr");
    let byte = module.next_label("chr_value_byte");
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    require_int(expr, module)?;
    module.body().line("i32.wrap_i64");
    module.body().line("i32.const 255");
    module.body().line("i32.and");
    module.body().line(&format!("local.set {}", byte));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.store8");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("i32.const 1");
    Ok(())
}
