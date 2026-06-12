//! Purpose:
//! Emits wasm32-web runtime lowering for PHP substr() and substr_replace().
//! Keeps substring bounds handling and replacement assembly out of the main expression file.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::string_builtins`
//! - `crate::codegen::wasm::expr::string_values`
//!
//! Key details:
//! - Handles static and runtime offset/length expressions with PHP-compatible clamping.
//! - Emits both direct output paths and pointer/length value materialization paths.

use super::*;

pub(super) fn emit_runtime_substr(
    var: &str,
    offset: i64,
    length: Option<i64>,
    module: &mut WasmModule,
) {
    let start = module.next_label("substr_start");
    let end = module.next_label("substr_end");
    module.declare_i32_local(start.trim_start_matches('$').to_string());
    module.declare_i32_local(end.trim_start_matches('$').to_string());
    emit_runtime_substring_bounds(var, offset, length, &start, &end, module);
    emit_write_string_range(var, &start, &end, module);
}

pub(super) fn emit_runtime_substr_dynamic(
    var: &str,
    offset: &Expr,
    length: Option<&Expr>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let start = module.next_label("substr_start");
    let end = module.next_label("substr_end");
    let offset64 = module.next_label("substr_offset64");
    let length64 = module.next_label("substr_length64");
    let temp64 = module.next_label("substr_temp64");
    module.declare_i32_local(start.trim_start_matches('$').to_string());
    module.declare_i32_local(end.trim_start_matches('$').to_string());
    module.declare_i64_local(offset64.trim_start_matches('$').to_string());
    module.declare_i64_local(length64.trim_start_matches('$').to_string());
    module.declare_i64_local(temp64.trim_start_matches('$').to_string());

    require_int(offset, module)?;
    module.body().line(&format!("local.set {}", offset64));
    module.body().line(&format!("local.get {}", offset64));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i64.extend_i32_u");
    module.body().line(&format!("local.get {}", offset64));
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", temp64));
    module.body().line(&format!("local.get {}", temp64));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", start));
    module.body().line("else");
    module.body().line(&format!("local.get {}", temp64));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", start));
    module.body().close("end");
    module.body().line("else");
    module.body().line(&format!("local.get {}", offset64));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i64.extend_i32_u");
    module.body().line("i64.gt_u");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.set {}", start));
    module.body().line("else");
    module.body().line(&format!("local.get {}", offset64));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", start));
    module.body().close("end");
    module.body().close("end");

    if let Some(length) = length {
        require_int(length, module)?;
        module.body().line(&format!("local.set {}", length64));
        module.body().line(&format!("local.get {}", length64));
        module.body().line("i64.const 0");
        module.body().line("i64.lt_s");
        module.body().open("if");
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line("i64.extend_i32_u");
        module.body().line(&format!("local.get {}", length64));
        module.body().line("i64.add");
        module.body().line(&format!("local.set {}", temp64));
        module.body().line(&format!("local.get {}", temp64));
        module.body().line(&format!("local.get {}", start));
        module.body().line("i64.extend_i32_u");
        module.body().line("i64.lt_s");
        module.body().open("if");
        module.body().line(&format!("local.get {}", start));
        module.body().line(&format!("local.set {}", end));
        module.body().line("else");
        module.body().line(&format!("local.get {}", temp64));
        module.body().line("i32.wrap_i64");
        module.body().line(&format!("local.set {}", end));
        module.body().close("end");
        module.body().line("else");
        module.body().line(&format!("local.get {}", start));
        module.body().line("i64.extend_i32_u");
        module.body().line(&format!("local.get {}", length64));
        module.body().line("i64.add");
        module.body().line(&format!("local.set {}", temp64));
        module.body().line(&format!("local.get {}", temp64));
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line("i64.extend_i32_u");
        module.body().line("i64.gt_u");
        module.body().open("if");
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line(&format!("local.set {}", end));
        module.body().line("else");
        module.body().line(&format!("local.get {}", temp64));
        module.body().line("i32.wrap_i64");
        module.body().line(&format!("local.set {}", end));
        module.body().close("end");
        module.body().close("end");
    } else {
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line(&format!("local.set {}", end));
    }
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.lt_u");
    module.body().open("if");
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.set {}", end));
    module.body().close("end");
    emit_write_string_range(var, &start, &end, module);
    Ok(())
}

pub(super) fn emit_runtime_substring_dynamic_bounds(
    var: &str,
    offset: &Expr,
    length: Option<&Expr>,
    start: &str,
    end: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let offset64 = module.next_label("substr_offset64");
    let length64 = module.next_label("substr_length64");
    let temp64 = module.next_label("substr_temp64");
    module.declare_i64_local(offset64.trim_start_matches('$').to_string());
    module.declare_i64_local(length64.trim_start_matches('$').to_string());
    module.declare_i64_local(temp64.trim_start_matches('$').to_string());

    require_int(offset, module)?;
    module.body().line(&format!("local.set {}", offset64));
    module.body().line(&format!("local.get {}", offset64));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i64.extend_i32_u");
    module.body().line(&format!("local.get {}", offset64));
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", temp64));
    module.body().line(&format!("local.get {}", temp64));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", start));
    module.body().line("else");
    module.body().line(&format!("local.get {}", temp64));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", start));
    module.body().close("end");
    module.body().line("else");
    module.body().line(&format!("local.get {}", offset64));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i64.extend_i32_u");
    module.body().line("i64.gt_u");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.set {}", start));
    module.body().line("else");
    module.body().line(&format!("local.get {}", offset64));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", start));
    module.body().close("end");
    module.body().close("end");

    if let Some(length) = length {
        require_int(length, module)?;
        module.body().line(&format!("local.set {}", length64));
        module.body().line(&format!("local.get {}", length64));
        module.body().line("i64.const 0");
        module.body().line("i64.lt_s");
        module.body().open("if");
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line("i64.extend_i32_u");
        module.body().line(&format!("local.get {}", length64));
        module.body().line("i64.add");
        module.body().line(&format!("local.set {}", temp64));
        module.body().line(&format!("local.get {}", temp64));
        module.body().line(&format!("local.get {}", start));
        module.body().line("i64.extend_i32_u");
        module.body().line("i64.lt_s");
        module.body().open("if");
        module.body().line(&format!("local.get {}", start));
        module.body().line(&format!("local.set {}", end));
        module.body().line("else");
        module.body().line(&format!("local.get {}", temp64));
        module.body().line("i32.wrap_i64");
        module.body().line(&format!("local.set {}", end));
        module.body().close("end");
        module.body().line("else");
        module.body().line(&format!("local.get {}", start));
        module.body().line("i64.extend_i32_u");
        module.body().line(&format!("local.get {}", length64));
        module.body().line("i64.add");
        module.body().line(&format!("local.set {}", temp64));
        module.body().line(&format!("local.get {}", temp64));
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line("i64.extend_i32_u");
        module.body().line("i64.gt_u");
        module.body().open("if");
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line(&format!("local.set {}", end));
        module.body().line("else");
        module.body().line(&format!("local.get {}", temp64));
        module.body().line("i32.wrap_i64");
        module.body().line(&format!("local.set {}", end));
        module.body().close("end");
        module.body().close("end");
    } else {
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line(&format!("local.set {}", end));
    }
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.lt_u");
    module.body().open("if");
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.set {}", end));
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_runtime_substring_bounds(
    var: &str,
    offset: i64,
    length: Option<i64>,
    start: &str,
    end: &str,
    module: &mut WasmModule,
) {
    if offset < 0 {
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line(&format!("i32.const {}", (-offset).min(i32::MAX as i64)));
        module.body().line("i32.gt_u");
        module.body().open("if (result i32)");
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line(&format!("i32.const {}", (-offset).min(i32::MAX as i64)));
        module.body().line("i32.sub");
        module.body().line("else");
        module.body().line("i32.const 0");
        module.body().close("end");
    } else {
        module.body().line(&format!("i32.const {}", offset.min(i32::MAX as i64)));
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line("i32.gt_u");
        module.body().open("if (result i32)");
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line("else");
        module.body().line(&format!("i32.const {}", offset.min(i32::MAX as i64)));
        module.body().close("end");
    }
    module.body().line(&format!("local.set {}", start));
    match length {
        Some(length) if length >= 0 => {
            module.body().line(&format!("local.get {}", start));
            module.body().line(&format!("i32.const {}", length.min(i32::MAX as i64)));
            module.body().line("i32.add");
            module.body().line(&format!("local.get ${}_len", var));
            module.body().line("i32.gt_u");
            module.body().open("if (result i32)");
            module.body().line(&format!("local.get ${}_len", var));
            module.body().line("else");
            module.body().line(&format!("local.get {}", start));
            module.body().line(&format!("i32.const {}", length.min(i32::MAX as i64)));
            module.body().line("i32.add");
            module.body().close("end");
        }
        Some(length) => {
            module.body().line(&format!("local.get ${}_len", var));
            module.body().line(&format!("i32.const {}", (-length).min(i32::MAX as i64)));
            module.body().line("i32.gt_u");
            module.body().open("if (result i32)");
            module.body().line(&format!("local.get ${}_len", var));
            module.body().line(&format!("i32.const {}", (-length).min(i32::MAX as i64)));
            module.body().line("i32.sub");
            module.body().line("else");
            module.body().line(&format!("local.get {}", start));
            module.body().close("end");
        }
        None => {
            module.body().line(&format!("local.get ${}_len", var));
        }
    }
    module.body().line(&format!("local.set {}", end));
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.lt_u");
    module.body().open("if");
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.set {}", end));
    module.body().close("end");
}

pub(super) fn emit_runtime_substr_replace(
    var: &str,
    replacement: &str,
    offset: i64,
    length: Option<i64>,
    module: &mut WasmModule,
) {
    let start = module.next_label("subrep_start");
    let end = module.next_label("subrep_end");
    let zero = module.next_label("subrep_zero");
    let full = module.next_label("subrep_full");
    let (replacement_ptr, replacement_len) = module.intern_string(replacement);
    module.declare_i32_local(start.trim_start_matches('$').to_string());
    module.declare_i32_local(end.trim_start_matches('$').to_string());
    module.declare_i32_local(zero.trim_start_matches('$').to_string());
    module.declare_i32_local(full.trim_start_matches('$').to_string());
    emit_runtime_substring_bounds(var, offset, length, &start, &end, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", zero));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.set {}", full));
    emit_write_string_range(var, &zero, &start, module);
    emit_write_static_string(replacement_ptr, replacement_len, module);
    emit_write_string_range(var, &end, &full, module);
}

pub(super) fn emit_runtime_substr_replace_value_to_stack(
    var: &str,
    replacement: WasmReplacement<'_>,
    start: &str,
    end: &str,
    module: &mut WasmModule,
) {
    let out_ptr = module.next_label("subrep_value_ptr");
    let out_len = module.next_label("subrep_value_len");
    let out_idx = module.next_label("subrep_value_idx");
    let zero = module.next_label("subrep_value_zero");
    let full = module.next_label("subrep_value_full");
    let literal_replacement = match replacement {
        WasmReplacement::Literal(value) => Some(module.intern_string(value)),
        WasmReplacement::Variable(_) => None,
    };
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(out_len.trim_start_matches('$').to_string());
    module.declare_i32_local(out_idx.trim_start_matches('$').to_string());
    module.declare_i32_local(zero.trim_start_matches('$').to_string());
    module.declare_i32_local(full.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", zero));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.set {}", full));
    module.body().line(&format!("local.get {}", start));
    match replacement {
        WasmReplacement::Literal(_) => {
            let (_, len) = literal_replacement.unwrap();
            module.body().line(&format!("i32.const {}", len));
        }
        WasmReplacement::Variable(replacement_var) => {
            module.body().line(&format!("local.get ${}_len", replacement_var));
        }
    }
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.sub");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
    emit_copy_string_range_to_memory(var, &zero, start, &out_ptr, &out_idx, module);
    match replacement {
        WasmReplacement::Literal(_) => {
            let (ptr, len) = literal_replacement.unwrap();
            emit_copy_static_range_to_memory(ptr, len, &out_ptr, &out_idx, module);
        }
        WasmReplacement::Variable(replacement_var) => {
            emit_copy_string_to_memory(replacement_var, &out_ptr, &out_idx, module);
        }
    }
    emit_copy_string_range_to_memory(var, end, &full, &out_ptr, &out_idx, module);
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_len));
}

pub(super) fn emit_runtime_substr_replace_var_replacement(
    var: &str,
    replacement_var: &str,
    offset: i64,
    length: Option<i64>,
    module: &mut WasmModule,
) {
    let start = module.next_label("subrep_start");
    let end = module.next_label("subrep_end");
    let zero = module.next_label("subrep_zero");
    let full = module.next_label("subrep_full");
    module.declare_i32_local(start.trim_start_matches('$').to_string());
    module.declare_i32_local(end.trim_start_matches('$').to_string());
    module.declare_i32_local(zero.trim_start_matches('$').to_string());
    module.declare_i32_local(full.trim_start_matches('$').to_string());
    emit_runtime_substring_bounds(var, offset, length, &start, &end, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", zero));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.set {}", full));
    emit_write_string_range(var, &zero, &start, module);
    emit_write_string_var(replacement_var, module);
    emit_write_string_range(var, &end, &full, module);
}

pub(super) fn emit_runtime_substr_replace_dynamic(
    var: &str,
    replacement: WasmReplacement<'_>,
    offset: &Expr,
    length: Option<&Expr>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let start = module.next_label("subrep_start");
    let end = module.next_label("subrep_end");
    let zero = module.next_label("subrep_zero");
    let full = module.next_label("subrep_full");
    let literal_replacement = match replacement {
        WasmReplacement::Literal(value) => Some(module.intern_string(value)),
        WasmReplacement::Variable(_) => None,
    };
    module.declare_i32_local(start.trim_start_matches('$').to_string());
    module.declare_i32_local(end.trim_start_matches('$').to_string());
    module.declare_i32_local(zero.trim_start_matches('$').to_string());
    module.declare_i32_local(full.trim_start_matches('$').to_string());
    emit_runtime_substring_dynamic_bounds(var, offset, length, &start, &end, module)?;
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", zero));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.set {}", full));
    emit_write_string_range(var, &zero, &start, module);
    match replacement {
        WasmReplacement::Literal(_) => {
            let (replacement_ptr, replacement_len) = literal_replacement.unwrap();
            emit_write_static_string(replacement_ptr, replacement_len, module);
        }
        WasmReplacement::Variable(replacement_var) => emit_write_string_var(replacement_var, module),
    }
    emit_write_string_range(var, &end, &full, module);
    Ok(())
}
