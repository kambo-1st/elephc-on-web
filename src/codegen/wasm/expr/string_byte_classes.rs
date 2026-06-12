//! Purpose:
//! Emits ASCII byte-class predicates used by wasm32-web string and numeric helpers.
//! Keeps ctype loop lowering separate from higher-level string builtin dispatch.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::string_predicates`.
//! - sibling wasm modules that need ASCII alpha/digit/space checks.
//!
//! Key details:
//! - Implements PHP-compatible ASCII byte classifications for the supported wasm subset.

use super::*;

#[derive(Clone, Copy)]
pub(super) enum ByteClass {
    Alpha,
    Digit,
    Alnum,
    Space,
}

pub(super) fn emit_runtime_ctype(var: &str, class: ByteClass, module: &mut WasmModule) {
    let idx = module.next_label("ctype_idx");
    let byte = module.next_label("ctype_byte");
    let ok = module.next_label("ctype_ok");
    let loop_label = module.next_label("ctype_loop");
    let done_label = module.next_label("ctype_done");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    module.declare_i32_local(ok.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.eqz");
    module.body().open("if (result i32)");
    module.body().line("i32.const 0");
    module.body().line("else");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", ok));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_string_byte(var, &idx, module);
    module.body().line(&format!("local.set {}", byte));
    emit_byte_class_condition(&byte, class, module);
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", ok));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", ok));
    module.body().close("end");
}

pub(super) fn emit_byte_class_condition(byte_local: &str, class: ByteClass, module: &mut WasmModule) {
    match class {
        ByteClass::Alpha => emit_ascii_alpha_condition(byte_local, module),
        ByteClass::Digit => emit_ascii_digit_condition(byte_local, module),
        ByteClass::Alnum => {
            emit_ascii_alpha_condition(byte_local, module);
            emit_ascii_digit_condition(byte_local, module);
            module.body().line("i32.or");
        }
        ByteClass::Space => {
            for byte in [9, 10, 11, 12, 13, 32] {
                module.body().line(&format!("local.get {}", byte_local));
                module.body().line(&format!("i32.const {}", byte));
                module.body().line("i32.eq");
            }
            for _ in 1..6 {
                module.body().line("i32.or");
            }
        }
    }
}

pub(super) fn emit_ascii_alpha_condition(byte_local: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 65");
    module.body().line("i32.ge_u");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 90");
    module.body().line("i32.le_u");
    module.body().line("i32.and");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 97");
    module.body().line("i32.ge_u");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 122");
    module.body().line("i32.le_u");
    module.body().line("i32.and");
    module.body().line("i32.or");
}

pub(super) fn emit_ascii_digit_condition(byte_local: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 48");
    module.body().line("i32.ge_u");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.const 57");
    module.body().line("i32.le_u");
    module.body().line("i32.and");
}
