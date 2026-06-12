//! Purpose:
//! Lowers str_pad() expressions for the wasm32-web backend.
//! Owns output and value-stack padding paths for literal and runtime pad strings.
//!
//! Called from:
//! - `crate::codegen::wasm::expr`
//!
//! Key details:
//! - Reuses parent string-copy helpers and keeps pad-side semantics isolated from native codegen.

use super::*;

pub(super) fn emit_str_pad_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !name.eq_ignore_ascii_case("str_pad") {
        return Ok(false);
    }
    if args.len() < 2 || args.len() > 4 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web str_pad() expects two to four arguments",
        ));
    }
    if static_string_value(&args[0], module).is_some() {
        let value = eval_output_string_builtin(call, name, args, module)?;
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(true);
    }
    let Some(var) = runtime_string_arg_or_materialize(&args[0], "str_pad_value_arg", module)? else {
        return Ok(false);
    };
    let target = module.next_label("str_pad_value_target");
    module.declare_i32_local(target.trim_start_matches('$').to_string());
    emit_str_pad_target_len(&args[1], &target, module)?;
    let pad_type = runtime_str_pad_type(call, args.get(3), module)?;
    let pad_var = args
        .get(2)
        .map(|arg| runtime_string_arg_or_materialize(arg, "str_pad_value_pad", module))
        .transpose()?
        .flatten();
    if let Some(pad_var) = pad_var {
        emit_runtime_str_pad_value_to_stack(&var, &target, WasmPadSource::Variable(&pad_var), pad_type, module);
    } else {
        let pad = args
            .get(2)
            .map(|arg| static_ascii_string_arg(call, arg, module))
            .transpose()?
            .unwrap_or_else(|| " ".to_string());
        if pad.is_empty() {
            return Err(CompileError::new(
                call.span,
                "wasm32-web str_pad() requires a non-empty literal pad string",
            ));
        }
        emit_runtime_str_pad_value_to_stack(&var, &target, WasmPadSource::Literal(&pad), pad_type, module);
    }
    Ok(true)
}

fn emit_str_pad_target_len(
    target_len: &Expr,
    target: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(target_len) = static_int_value(target_len) {
        let target_len = target_len.max(0).min(i64::from(i32::MAX));
        module.body().line(&format!("i32.const {}", target_len));
        module.body().line(&format!("local.set {}", target));
        return Ok(());
    }
    let target64 = module.next_label("str_pad_value_target64");
    module.declare_i64_local(target64.trim_start_matches('$').to_string());
    require_int(target_len, module)?;
    module.body().line(&format!("local.set {}", target64));
    module.body().line(&format!("local.get {}", target64));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", target));
    module.body().line("else");
    module.body().line(&format!("local.get {}", target64));
    module.body().line("i64.const 2147483647");
    module.body().line("i64.gt_s");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().line("else");
    module.body().line(&format!("local.get {}", target64));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", target));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_runtime_str_pad_sides(
    needed: &str,
    left: &str,
    right: &str,
    pad_type: RuntimePadType<'_>,
    module: &mut WasmModule,
) {
    match pad_type {
        RuntimePadType::Static(0) => {
            module.body().line(&format!("local.get {}", needed));
            module.body().line(&format!("local.set {}", left));
            module.body().line("i32.const 0");
            module.body().line(&format!("local.set {}", right));
        }
        RuntimePadType::Static(2) => emit_runtime_str_pad_both_sides(needed, left, right, module),
        RuntimePadType::Static(_) => {
            module.body().line("i32.const 0");
            module.body().line(&format!("local.set {}", left));
            module.body().line(&format!("local.get {}", needed));
            module.body().line(&format!("local.set {}", right));
        }
        RuntimePadType::Variable(var) => {
            module.body().line(&format!("local.get ${}", var));
            module.body().line("i64.const 0");
            module.body().line("i64.eq");
            module.body().open("if");
            module.body().line(&format!("local.get {}", needed));
            module.body().line(&format!("local.set {}", left));
            module.body().line("i32.const 0");
            module.body().line(&format!("local.set {}", right));
            module.body().line("else");
            module.body().line(&format!("local.get ${}", var));
            module.body().line("i64.const 2");
            module.body().line("i64.eq");
            module.body().open("if");
            emit_runtime_str_pad_both_sides(needed, left, right, module);
            module.body().line("else");
            module.body().line("i32.const 0");
            module.body().line(&format!("local.set {}", left));
            module.body().line(&format!("local.get {}", needed));
            module.body().line(&format!("local.set {}", right));
            module.body().close("end");
            module.body().close("end");
        }
    }
}

fn emit_runtime_str_pad_both_sides(
    needed: &str,
    left: &str,
    right: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", needed));
    module.body().line("i32.const 2");
    module.body().line("i32.div_u");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get {}", needed));
    module.body().line(&format!("local.get {}", left));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", right));
}

fn emit_runtime_str_pad_value_to_stack(
    var: &str,
    target: &str,
    pad: WasmPadSource<'_>,
    pad_type: RuntimePadType<'_>,
    module: &mut WasmModule,
) {
    let needed = module.next_label("pad_value_needed");
    let left = module.next_label("pad_value_left");
    let right = module.next_label("pad_value_right");
    let out_ptr = module.next_label("pad_value_ptr");
    let out_idx = module.next_label("pad_value_idx");
    module.declare_i32_local(needed.trim_start_matches('$').to_string());
    module.declare_i32_local(left.trim_start_matches('$').to_string());
    module.declare_i32_local(right.trim_start_matches('$').to_string());
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(out_idx.trim_start_matches('$').to_string());
    if let WasmPadSource::Variable(pad_var) = &pad {
        module.body().line(&format!("local.get ${}_len", pad_var));
        module.body().line("i32.eqz");
        module.body().open("if");
        module.body().line("unreachable");
        module.body().close("end");
    }
    module.body().line(&format!("local.get {}", target));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.le_u");
    module.body().open("if (result i32 i32)");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("else");
    module.body().line(&format!("local.get {}", target));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", needed));
    emit_runtime_str_pad_sides(&needed, &left, &right, pad_type, module);
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", target));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
    emit_copy_pad_to_memory(&pad, &left, &out_ptr, &out_idx, module);
    emit_copy_string_to_memory(var, &out_ptr, &out_idx, module);
    emit_copy_pad_to_memory(&pad, &right, &out_ptr, &out_idx, module);
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", target));
    module.body().close("end");
}

pub(super) fn emit_runtime_str_pad(
    var: &str,
    target_len: i64,
    pad: &str,
    pad_type: RuntimePadType<'_>,
    module: &mut WasmModule,
) {
    let needed = module.next_label("pad_needed");
    let left = module.next_label("pad_left");
    let right = module.next_label("pad_right");
    module.declare_i32_local(needed.trim_start_matches('$').to_string());
    module.declare_i32_local(left.trim_start_matches('$').to_string());
    module.declare_i32_local(right.trim_start_matches('$').to_string());
    let target = target_len.max(0).min(i32::MAX as i64);
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("i32.const {}", target));
    module.body().line("i32.ge_u");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("call $host_write");
    module.body().line("else");
    module.body().line(&format!("i32.const {}", target));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", needed));
    emit_runtime_str_pad_sides(&needed, &left, &right, pad_type, module);
    emit_write_pad_bytes(pad, &left, module);
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("call $host_write");
    emit_write_pad_bytes(pad, &right, module);
    module.body().close("end");
}

pub(super) fn emit_runtime_str_pad_var_pad(
    var: &str,
    target_len: i64,
    pad_var: &str,
    pad_type: RuntimePadType<'_>,
    module: &mut WasmModule,
) {
    let needed = module.next_label("pad_needed");
    let left = module.next_label("pad_left");
    let right = module.next_label("pad_right");
    module.declare_i32_local(needed.trim_start_matches('$').to_string());
    module.declare_i32_local(left.trim_start_matches('$').to_string());
    module.declare_i32_local(right.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_len", pad_var));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    let target = target_len.max(0).min(i32::MAX as i64);
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("i32.const {}", target));
    module.body().line("i32.ge_u");
    module.body().open("if");
    emit_write_string_var(var, module);
    module.body().line("else");
    module.body().line(&format!("i32.const {}", target));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", needed));
    emit_runtime_str_pad_sides(&needed, &left, &right, pad_type, module);
    emit_write_pad_bytes_var(pad_var, &left, module);
    emit_write_string_var(var, module);
    emit_write_pad_bytes_var(pad_var, &right, module);
    module.body().close("end");
}

pub(super) fn emit_runtime_str_pad_dynamic(
    var: &str,
    target_len: &Expr,
    pad: &str,
    pad_type: RuntimePadType<'_>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let target64 = module.next_label("pad_target64");
    let needed = module.next_label("pad_needed");
    let left = module.next_label("pad_left");
    let right = module.next_label("pad_right");
    module.declare_i64_local(target64.trim_start_matches('$').to_string());
    module.declare_i32_local(needed.trim_start_matches('$').to_string());
    module.declare_i32_local(left.trim_start_matches('$').to_string());
    module.declare_i32_local(right.trim_start_matches('$').to_string());
    require_int(target_len, module)?;
    module.body().line(&format!("local.set {}", target64));
    module.body().line(&format!("local.get {}", target64));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i64.extend_i32_u");
    module.body().line("i64.le_s");
    module.body().open("if");
    emit_write_string_var(var, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", target64));
    module.body().line("i64.const 2147483647");
    module.body().line("i64.gt_s");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", target64));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", needed));
    emit_runtime_str_pad_sides(&needed, &left, &right, pad_type, module);
    emit_write_pad_bytes(pad, &left, module);
    emit_write_string_var(var, module);
    emit_write_pad_bytes(pad, &right, module);
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_runtime_str_pad_dynamic_var_pad(
    var: &str,
    target_len: &Expr,
    pad_var: &str,
    pad_type: RuntimePadType<'_>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let target64 = module.next_label("pad_target64");
    let needed = module.next_label("pad_needed");
    let left = module.next_label("pad_left");
    let right = module.next_label("pad_right");
    module.declare_i64_local(target64.trim_start_matches('$').to_string());
    module.declare_i32_local(needed.trim_start_matches('$').to_string());
    module.declare_i32_local(left.trim_start_matches('$').to_string());
    module.declare_i32_local(right.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_len", pad_var));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    require_int(target_len, module)?;
    module.body().line(&format!("local.set {}", target64));
    module.body().line(&format!("local.get {}", target64));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i64.extend_i32_u");
    module.body().line("i64.le_s");
    module.body().open("if");
    emit_write_string_var(var, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", target64));
    module.body().line("i64.const 2147483647");
    module.body().line("i64.gt_s");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", target64));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", needed));
    emit_runtime_str_pad_sides(&needed, &left, &right, pad_type, module);
    emit_write_pad_bytes_var(pad_var, &left, module);
    emit_write_string_var(var, module);
    emit_write_pad_bytes_var(pad_var, &right, module);
    module.body().close("end");
    Ok(())
}
