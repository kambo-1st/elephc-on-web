//! Purpose:
//! Emits wasm32-web integer-returning string comparison builtins.
//! Keeps strcmp/strcasecmp runtime loops separate from boolean predicate lowering.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` scalar builtin dispatch.
//!
//! Key details:
//! - Preserves PHP strcmp sign semantics and case-insensitive ASCII comparison behavior.

use super::*;
use super::string_search_literals::eval_literal_string_int_call;

pub(super) fn emit_literal_string_int_call(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if emit_runtime_string_int_call(call, name, args, module)? {
        return Ok(ValueKind::Int);
    }
    let result = eval_literal_string_int_call(call, name, args, module)?;
    let Some(result) = result else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web string search returned false; use direct output support or compare once supported",
        ));
    };
    module.body().line(&format!("i64.const {}", result));
    Ok(ValueKind::Int)
}

fn emit_runtime_string_int_call(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !matches!(name.to_ascii_lowercase().as_str(), "strcmp" | "strcasecmp") {
        return Ok(false);
    }
    let [left, right] = args else {
        return Err(CompileError::new(
            call.span,
            &format!("wasm32-web {}() expects exactly two arguments", name),
        ));
    };
    let Some(var) = concat_string_arg_or_materialize(left, "str_int_left", module)? else {
        return Ok(false);
    };
    match name.to_ascii_lowercase().as_str() {
        "strcmp" => {
            if let Some(right_var) = concat_string_arg_or_materialize(right, "strcmp_right", module)? {
                emit_runtime_strcmp_var(&var, &right_var, false, module);
            } else {
                let right = static_ascii_string_arg(call, right, module)?;
                emit_runtime_strcmp(&var, &right, false, module);
            }
        }
        "strcasecmp" => {
            if let Some(right_var) = concat_string_arg_or_materialize(right, "strcasecmp_right", module)? {
                emit_runtime_strcmp_var(&var, &right_var, true, module);
            } else {
                let right = static_ascii_string_arg(call, right, module)?;
                emit_runtime_strcmp(&var, &right, true, module);
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn emit_runtime_strcmp(
    var: &str,
    right: &str,
    case_insensitive: bool,
    module: &mut WasmModule,
) {
    let idx = module.next_label("cmp_idx");
    let left_byte = module.next_label("cmp_left");
    let right_byte = module.next_label("cmp_right");
    let result = module.next_label("cmp_result");
    let loop_label = module.next_label("cmp_loop");
    let done_label = module.next_label("cmp_done");
    let (right_ptr, right_len) = module.intern_string(right);
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(left_byte.trim_start_matches('$').to_string());
    module.declare_i32_local(right_byte.trim_start_matches('$').to_string());
    module.declare_i32_local(result.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", result));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", right_len));
    module.body().line("i32.ge_u");
    module.body().line("i32.or");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_string_byte(var, &idx, module);
    module.body().line(&format!("local.set {}", left_byte));
    module.body().line(&format!("i32.const {}", right_ptr));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", right_byte));
    if case_insensitive {
        emit_ascii_case_value(&left_byte, AsciiCase::Lower, module);
        module.body().line(&format!("local.set {}", left_byte));
        emit_ascii_case_value(&right_byte, AsciiCase::Lower, module);
        module.body().line(&format!("local.set {}", right_byte));
    }
    module.body().line(&format!("local.get {}", left_byte));
    module.body().line(&format!("local.get {}", right_byte));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line(&format!("local.get {}", left_byte));
    module.body().line(&format!("local.get {}", right_byte));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", result));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", result));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("i32.const {}", right_len));
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line("i32.const -1");
    module.body().line("else");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("i32.const {}", right_len));
    module.body().line("i32.gt_u");
    module.body().open("if (result i32)");
    module.body().line("i32.const 1");
    module.body().line("else");
    module.body().line("i32.const 0");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.set {}", result));
    module.body().close("end");
    module.body().line(&format!("local.get {}", result));
    module.body().line("i64.extend_i32_s");
}

fn emit_runtime_strcmp_var(
    var: &str,
    right_var: &str,
    case_insensitive: bool,
    module: &mut WasmModule,
) {
    let idx = module.next_label("cmp_idx");
    let left_byte = module.next_label("cmp_left");
    let right_byte = module.next_label("cmp_right");
    let result = module.next_label("cmp_result");
    let loop_label = module.next_label("cmp_loop");
    let done_label = module.next_label("cmp_done");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(left_byte.trim_start_matches('$').to_string());
    module.declare_i32_local(right_byte.trim_start_matches('$').to_string());
    module.declare_i32_local(result.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", result));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", right_var));
    module.body().line("i32.ge_u");
    module.body().line("i32.or");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_string_byte(var, &idx, module);
    module.body().line(&format!("local.set {}", left_byte));
    emit_load_string_byte(right_var, &idx, module);
    module.body().line(&format!("local.set {}", right_byte));
    if case_insensitive {
        emit_ascii_case_value(&left_byte, AsciiCase::Lower, module);
        module.body().line(&format!("local.set {}", left_byte));
        emit_ascii_case_value(&right_byte, AsciiCase::Lower, module);
        module.body().line(&format!("local.set {}", right_byte));
    }
    module.body().line(&format!("local.get {}", left_byte));
    module.body().line(&format!("local.get {}", right_byte));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line(&format!("local.get {}", left_byte));
    module.body().line(&format!("local.get {}", right_byte));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", result));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", result));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get ${}_len", right_var));
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line("i32.const -1");
    module.body().line("else");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get ${}_len", right_var));
    module.body().line("i32.gt_u");
    module.body().open("if (result i32)");
    module.body().line("i32.const 1");
    module.body().line("else");
    module.body().line("i32.const 0");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.set {}", result));
    module.body().close("end");
    module.body().line(&format!("local.get {}", result));
    module.body().line("i64.extend_i32_s");
}
