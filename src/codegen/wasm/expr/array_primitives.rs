//! Purpose:
//! Provides shared wasm32-web primitives for indexed array allocation and slot copying.
//! Keeps low-level array slot setup helpers out of the expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` array lowering modules.
//!
//! Key details:
//! - Helpers operate on the wasm array pointer/length local convention used by array emitters.

use super::*;

pub(in crate::codegen::wasm) fn preserve_array_ptr(
    source: &str,
    label: &str,
    module: &mut WasmModule,
) -> String {
    let source_ptr = module.next_label(&format!("{label}_source_ptr"));
    module.declare_i32_local(source_ptr.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.set {}", source_ptr));
    source_ptr
}

pub(in crate::codegen::wasm) fn emit_array_alloc_prelude(
    name: &str,
    len: usize,
    module: &mut WasmModule,
) {
    module.set_array_length(name, len);
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
}

pub(in crate::codegen::wasm) fn emit_array_store_expr(
    name: &str,
    index: usize,
    item: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("i32.const {}", index * 8));
    module.body().line("i32.add");
    require_int(item, module)?;
    module.body().line("i64.store");
    Ok(())
}

pub(in crate::codegen::wasm) fn emit_array_store_load(
    name: &str,
    index: usize,
    source_ptr: &str,
    source_index: usize,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("i32.const {}", index * 8));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("i32.const {}", source_index * 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.store");
}

pub(in crate::codegen::wasm) fn static_array_slice_bounds(
    len: usize,
    offset: i64,
    length: Option<i64>,
) -> (usize, usize) {
    let len_i64 = len as i64;
    let start = if offset < 0 {
        (len_i64 + offset).max(0)
    } else {
        offset.min(len_i64)
    };
    let end = match length {
        Some(length) if length < 0 => (len_i64 + length).max(start),
        Some(length) => (start + length).min(len_i64),
        None => len_i64,
    };
    (start as usize, (end - start).max(0) as usize)
}
