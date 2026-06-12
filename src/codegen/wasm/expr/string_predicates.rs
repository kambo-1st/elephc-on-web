//! Purpose:
//! Lowers string predicate and comparison builtins for wasm32-web.
//! Keeps runtime match loops and literal predicate evaluation out of the expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` scalar builtin dispatch.
//! - sibling wasm string/path builtin modules that need shared match-at helpers.
//!
//! Key details:
//! - Helpers preserve PHP ASCII string semantics currently supported by wasm32-web.

use super::*;
use super::string_byte_classes::emit_runtime_ctype;

pub(super) fn emit_literal_string_bool_call(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if emit_runtime_string_bool_call(call, name, args, module)? {
        return Ok(ValueKind::Bool);
    }
    let result = eval_literal_string_bool_call(call, name, args, module)?;
    module.body().line(&format!("i32.const {}", i32::from(result)));
    Ok(ValueKind::Bool)
}

fn emit_runtime_string_bool_call(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    match name.to_ascii_lowercase().as_str() {
        "str_contains" | "str_starts_with" | "str_ends_with" => {
            let [haystack, needle] = args else {
                return Err(CompileError::new(
                    call.span,
                    &format!("wasm32-web {}() expects exactly two arguments", name),
                ));
            };
            let var_storage;
            let var = if let Some(var) = runtime_string_variable_arg(haystack, module) {
                var
            } else if expression_is_stringy(haystack, module) {
                var_storage = materialize_runtime_string_expr(haystack, "str_bool_hay", module)?;
                &var_storage
            } else if string_cast_value_supported(haystack, module) {
                var_storage = materialize_string_cast_expr(haystack, "str_bool_hay", module)?;
                &var_storage
            } else {
                return Ok(false);
            };
            let needle_storage;
            if let Some(needle_var) = runtime_string_variable_arg(needle, module) {
                match name.to_ascii_lowercase().as_str() {
                    "str_contains" => emit_runtime_str_contains_var(var, needle_var, module),
                    "str_starts_with" => emit_runtime_str_prefix_var(var, needle_var, module),
                    "str_ends_with" => emit_runtime_str_suffix_var(var, needle_var, module),
                    _ => unreachable!(),
                }
            } else if expression_is_stringy(needle, module) && static_string_value(needle, module).is_none() {
                needle_storage = materialize_runtime_string_expr(needle, "str_bool_needle", module)?;
                match name.to_ascii_lowercase().as_str() {
                    "str_contains" => emit_runtime_str_contains_var(var, &needle_storage, module),
                    "str_starts_with" => emit_runtime_str_prefix_var(var, &needle_storage, module),
                    "str_ends_with" => emit_runtime_str_suffix_var(var, &needle_storage, module),
                    _ => unreachable!(),
                }
            } else if string_cast_value_supported(needle, module) && static_string_value(needle, module).is_none() {
                needle_storage = materialize_string_cast_expr(needle, "str_bool_needle", module)?;
                match name.to_ascii_lowercase().as_str() {
                    "str_contains" => emit_runtime_str_contains_var(var, &needle_storage, module),
                    "str_starts_with" => emit_runtime_str_prefix_var(var, &needle_storage, module),
                    "str_ends_with" => emit_runtime_str_suffix_var(var, &needle_storage, module),
                    _ => unreachable!(),
                }
            } else {
                let needle = static_ascii_string_arg(call, needle, module)?;
                match name.to_ascii_lowercase().as_str() {
                    "str_contains" => emit_runtime_str_contains(var, &needle, module),
                    "str_starts_with" => emit_runtime_str_prefix(var, &needle, module),
                    "str_ends_with" => emit_runtime_str_suffix(var, &needle, module),
                    _ => unreachable!(),
                }
            }
            Ok(true)
        }
        "ctype_alpha" | "ctype_digit" | "ctype_alnum" | "ctype_space" => {
            let [arg] = args else {
                return Err(CompileError::new(
                    call.span,
                    &format!("wasm32-web {}() expects exactly one argument", name),
                ));
            };
            let var_storage;
            let var = if let Some(var) = runtime_string_variable_arg(arg, module) {
                var
            } else if expression_is_stringy(arg, module) && static_string_value(arg, module).is_none() {
                var_storage = materialize_runtime_string_expr(arg, "ctype_arg", module)?;
                &var_storage
            } else {
                return Ok(false);
            };
            let class = match name.to_ascii_lowercase().as_str() {
                "ctype_alpha" => ByteClass::Alpha,
                "ctype_digit" => ByteClass::Digit,
                "ctype_alnum" => ByteClass::Alnum,
                "ctype_space" => ByteClass::Space,
                _ => unreachable!(),
            };
            emit_runtime_ctype(var, class, module);
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn emit_runtime_str_contains(var: &str, needle: &str, module: &mut WasmModule) {
    if needle.is_empty() {
        module.body().line("i32.const 1");
        return;
    }
    let idx = module.next_label("find_idx");
    let found = module.next_label("find_found");
    let scan_loop = module.next_label("find_loop");
    let scan_done = module.next_label("find_done");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(found.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().open(&format!("block {}", scan_done));
    module.body().open(&format!("loop {}", scan_loop));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", needle.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.gt_u");
    module.body().line(&format!("br_if {}", scan_done));
    emit_runtime_literal_match_at(var, &idx, needle, module);
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", scan_done));
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", scan_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
}

fn emit_runtime_str_prefix(var: &str, needle: &str, module: &mut WasmModule) {
    if needle.is_empty() {
        module.body().line("i32.const 1");
        return;
    }
    let offset = module.next_label("prefix_offset");
    module.declare_i32_local(offset.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("i32.const {}", needle.len()));
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line("i32.const 0");
    module.body().line("else");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", offset));
    emit_runtime_literal_match_at(var, &offset, needle, module);
    module.body().close("end");
}

fn emit_runtime_str_suffix(var: &str, needle: &str, module: &mut WasmModule) {
    if needle.is_empty() {
        module.body().line("i32.const 1");
        return;
    }
    let offset = module.next_label("suffix_offset");
    module.declare_i32_local(offset.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("i32.const {}", needle.len()));
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line("i32.const 0");
    module.body().line("else");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("i32.const {}", needle.len()));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", offset));
    emit_runtime_literal_match_at(var, &offset, needle, module);
    module.body().close("end");
}

fn emit_runtime_str_contains_var(var: &str, needle_var: &str, module: &mut WasmModule) {
    let idx = module.next_label("find_idx");
    let found = module.next_label("find_found");
    let scan_loop = module.next_label("find_loop");
    let scan_done = module.next_label("find_done");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(found.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_len", needle_var));
    module.body().line("i32.eqz");
    module.body().open("if (result i32)");
    module.body().line("i32.const 1");
    module.body().line("else");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().open(&format!("block {}", scan_done));
    module.body().open(&format!("loop {}", scan_loop));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", needle_var));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.gt_u");
    module.body().line(&format!("br_if {}", scan_done));
    emit_runtime_var_match_at(var, &idx, needle_var, module);
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", scan_done));
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", scan_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
    module.body().close("end");
}

fn emit_runtime_str_prefix_var(var: &str, needle_var: &str, module: &mut WasmModule) {
    let offset = module.next_label("prefix_offset");
    module.declare_i32_local(offset.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_len", needle_var));
    module.body().line("i32.eqz");
    module.body().open("if (result i32)");
    module.body().line("i32.const 1");
    module.body().line("else");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get ${}_len", needle_var));
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line("i32.const 0");
    module.body().line("else");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", offset));
    emit_runtime_var_match_at(var, &offset, needle_var, module);
    module.body().close("end");
    module.body().close("end");
}

fn emit_runtime_str_suffix_var(var: &str, needle_var: &str, module: &mut WasmModule) {
    let offset = module.next_label("suffix_offset");
    module.declare_i32_local(offset.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_len", needle_var));
    module.body().line("i32.eqz");
    module.body().open("if (result i32)");
    module.body().line("i32.const 1");
    module.body().line("else");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get ${}_len", needle_var));
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line("i32.const 0");
    module.body().line("else");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get ${}_len", needle_var));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", offset));
    emit_runtime_var_match_at(var, &offset, needle_var, module);
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_literal_match_at(
    var: &str,
    offset_local: &str,
    needle: &str,
    module: &mut WasmModule,
) {
    let idx = module.next_label("match_idx");
    let matched = module.next_label("match_ok");
    let loop_label = module.next_label("match_loop");
    let done_label = module.next_label("match_done");
    let (needle_ptr, needle_len) = module.intern_string(needle);
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(matched.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", matched));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", needle_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", offset_local));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("i32.const {}", needle_ptr));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", matched));
}

pub(super) fn emit_runtime_var_match_at(
    var: &str,
    offset_local: &str,
    needle_var: &str,
    module: &mut WasmModule,
) {
    let idx = module.next_label("match_idx");
    let matched = module.next_label("match_ok");
    let loop_label = module.next_label("match_loop");
    let done_label = module.next_label("match_done");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(matched.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", matched));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", needle_var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", offset_local));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.get ${}_ptr", needle_var));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", matched));
}

pub(super) fn emit_runtime_var_match_at_case_insensitive(
    var: &str,
    offset_local: &str,
    needle_var: &str,
    module: &mut WasmModule,
) {
    let idx = module.next_label("imatch_idx");
    let left = module.next_label("imatch_left");
    let right = module.next_label("imatch_right");
    let matched = module.next_label("imatch_ok");
    let loop_label = module.next_label("imatch_loop");
    let done_label = module.next_label("imatch_done");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(left.trim_start_matches('$').to_string());
    module.declare_i32_local(right.trim_start_matches('$').to_string());
    module.declare_i32_local(matched.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", matched));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", needle_var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", offset_local));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", left));
    emit_ascii_case_value(&left, AsciiCase::Lower, module);
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get ${}_ptr", needle_var));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", right));
    emit_ascii_case_value(&right, AsciiCase::Lower, module);
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", matched));
}

pub(super) fn emit_runtime_literal_match_at_case_insensitive(
    var: &str,
    offset_local: &str,
    needle: &str,
    module: &mut WasmModule,
) {
    let idx = module.next_label("imatch_idx");
    let left = module.next_label("imatch_left");
    let matched = module.next_label("imatch_ok");
    let loop_label = module.next_label("imatch_loop");
    let done_label = module.next_label("imatch_done");
    let lower_needle = ascii_lower(needle);
    let (needle_ptr, needle_len) = module.intern_string(&lower_needle);
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(left.trim_start_matches('$').to_string());
    module.declare_i32_local(matched.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", matched));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", needle_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", offset_local));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", left));
    emit_ascii_case_value(&left, AsciiCase::Lower, module);
    module.body().line(&format!("i32.const {}", needle_ptr));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", matched));
}

fn eval_literal_string_bool_call(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &WasmModule,
) -> Result<bool, CompileError> {
    match name.to_ascii_lowercase().as_str() {
        "str_contains" | "str_starts_with" | "str_ends_with" => {
            let [haystack, needle] = args else {
                return Err(CompileError::new(
                    call.span,
                    &format!("wasm32-web {}() expects exactly two arguments", name),
                ));
            };
            let haystack = static_ascii_string_arg(call, haystack, module)?;
            let needle = static_ascii_string_arg(call, needle, module)?;
            Ok(match name.to_ascii_lowercase().as_str() {
                "str_contains" => haystack.contains(&needle),
                "str_starts_with" => haystack.starts_with(&needle),
                "str_ends_with" => haystack.ends_with(&needle),
                _ => unreachable!(),
            })
        }
        "ctype_alpha" | "ctype_digit" | "ctype_alnum" | "ctype_space" => {
            let [arg] = args else {
                return Err(CompileError::new(
                    call.span,
                    &format!("wasm32-web {}() expects exactly one argument", name),
                ));
            };
            let value = static_ascii_string_arg(call, arg, module)?;
            if value.is_empty() {
                return Ok(false);
            }
            Ok(match name.to_ascii_lowercase().as_str() {
                "ctype_alpha" => value.bytes().all(|byte| byte.is_ascii_alphabetic()),
                "ctype_digit" => value.bytes().all(|byte| byte.is_ascii_digit()),
                "ctype_alnum" => value.bytes().all(|byte| byte.is_ascii_alphanumeric()),
                "ctype_space" => value.bytes().all(|byte| byte.is_ascii_whitespace()),
                _ => unreachable!(),
            })
        }
        _ => unreachable!(),
    }
}
