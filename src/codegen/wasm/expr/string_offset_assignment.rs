//! Purpose:
//! Lowers wasm32-web string offset assignment into heap-backed string mutation steps.
//! Keeps string mutation separate from generic string value materialization.
//!
//! Called from:
//! - `crate::codegen::wasm::stmt` through the expression module re-export.
//!
//! Key details:
//! - PHP string offset writes copy the old bytes, pad with spaces when extending,
//!   and store only the first byte of the replacement string.

use super::*;
use super::string_materialization::emit_string_value_to_stack;

pub(in crate::codegen::wasm) fn emit_string_offset_assign(
    name: &str,
    index: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if matches!(static_string_value(value, module), Some(value) if value.is_empty()) {
        return Err(CompileError::new(
            value.span,
            "wasm32-web string offset assignment cannot assign an empty string",
        ));
    }

    let old_ptr = module.next_label("string_set_old_ptr");
    let old_len = module.next_label("string_set_old_len");
    let replacement_ptr = module.next_label("string_set_replacement_ptr");
    let replacement_len = module.next_label("string_set_replacement_len");
    let idx64 = module.next_label("string_set_idx64");
    let idx32 = module.next_label("string_set_idx32");
    let new_ptr = module.next_label("string_set_new_ptr");
    let new_len = module.next_label("string_set_new_len");
    let copy_idx = module.next_label("string_set_copy_idx");
    let fill_idx = module.next_label("string_set_fill_idx");
    let done_label = module.next_label("string_set_done");
    let copy_done = module.next_label("string_set_copy_done");
    let copy_loop = module.next_label("string_set_copy_loop");
    let fill_done = module.next_label("string_set_fill_done");
    let fill_loop = module.next_label("string_set_fill_loop");

    for local in [
        &old_ptr,
        &old_len,
        &replacement_ptr,
        &replacement_len,
        &idx32,
        &new_ptr,
        &new_len,
        &copy_idx,
        &fill_idx,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(idx64.trim_start_matches('$').to_string());

    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.set {}", old_ptr));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line(&format!("local.set {}", old_len));

    emit_string_value_to_stack(value, module)?;
    module.body().line(&format!("local.set {}", replacement_len));
    module.body().line(&format!("local.set {}", replacement_ptr));
    module.body().line(&format!("local.get {}", replacement_len));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");

    require_int(index, module)?;
    module.body().line(&format!("local.set {}", idx64));
    module.body().line(&format!("local.get {}", idx64));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().open("if");
    module.body().line(&format!("local.get {}", old_len));
    module.body().line("i64.extend_i32_u");
    module.body().line(&format!("local.get {}", idx64));
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", idx64));
    module.body().close("end");

    module.body().open(&format!("block {}", done_label));
    module.body().line(&format!("local.get {}", idx64));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", idx64));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", idx32));
    module.body().line(&format!("local.get {}", old_len));
    module.body().line(&format!("local.set {}", new_len));
    module.body().line(&format!("local.get {}", idx32));
    module.body().line(&format!("local.get {}", old_len));
    module.body().line("i32.ge_u");
    module.body().open("if");
    module.body().line(&format!("local.get {}", idx32));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", new_len));
    module.body().close("end");

    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", new_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", new_len));
    module.body().line("i32.add");
    module.body().line("global.set $heap");

    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", copy_idx));
    module.body().open(&format!("block {}", copy_done));
    module.body().open(&format!("loop {}", copy_loop));
    module.body().line(&format!("local.get {}", copy_idx));
    module.body().line(&format!("local.get {}", old_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", copy_done));
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.get {}", copy_idx));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", old_ptr));
    module.body().line(&format!("local.get {}", copy_idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.store8");
    module.body().line(&format!("local.get {}", copy_idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", copy_idx));
    module.body().line(&format!("br {}", copy_loop));
    module.body().close("end");
    module.body().close("end");

    module.body().line(&format!("local.get {}", old_len));
    module.body().line(&format!("local.set {}", fill_idx));
    module.body().open(&format!("block {}", fill_done));
    module.body().open(&format!("loop {}", fill_loop));
    module.body().line(&format!("local.get {}", fill_idx));
    module.body().line(&format!("local.get {}", idx32));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", fill_done));
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.get {}", fill_idx));
    module.body().line("i32.add");
    module.body().line("i32.const 32");
    module.body().line("i32.store8");
    module.body().line(&format!("local.get {}", fill_idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", fill_idx));
    module.body().line(&format!("br {}", fill_loop));
    module.body().close("end");
    module.body().close("end");

    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.get {}", idx32));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", replacement_ptr));
    module.body().line("i32.load8_u");
    module.body().line("i32.store8");
    module.body().line(&format!("local.get {}", new_ptr));
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", new_len));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().close("end");

    Ok(())
}
