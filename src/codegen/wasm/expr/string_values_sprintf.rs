//! Purpose:
//! Materializes sprintf() results as wasm32-web heap string values.
//! Keeps width, precision, and segment-copy logic out of generic string value emitters.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::string_materialization`.
//! - `crate::codegen::wasm::expr::sprintf_writers`.
//!
//! Key details:
//! - Computes formatted string capacity first, then copies literal and variable segments into heap memory.

use super::*;

pub(super) fn emit_sprintf_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !name.eq_ignore_ascii_case("sprintf") {
        return Ok(false);
    }
    let Some(segments) = sprintf_string_value_segments(call, args, module)? else {
        return Ok(false);
    };
    let out_ptr = module.next_label("sprintf_value_ptr");
    let out_len = module.next_label("sprintf_value_len");
    let out_idx = module.next_label("sprintf_value_idx");
    for local in [&out_ptr, &out_len, &out_idx] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_len));
    for segment in &segments {
        module.body().line(&format!("local.get {}", out_len));
        match segment {
            SprintfStringSegment::Literal(value) => {
                module.body().line(&format!("i32.const {}", value.len()));
            }
            SprintfStringSegment::Variable {
                var,
                precision,
                width,
                ..
            } => {
                emit_sprintf_padded_segment_len(var, *precision, *width, module);
            }
        }
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", out_len));
    }
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
    for segment in &segments {
        match segment {
            SprintfStringSegment::Literal(value) if value.is_empty() => {}
            SprintfStringSegment::Literal(value) => {
                let (ptr, len) = module.intern_string(value);
                emit_copy_static_range_to_memory(ptr, len, &out_ptr, &out_idx, module);
            }
            SprintfStringSegment::Variable {
                var,
                precision,
                width,
                left_align,
            } => {
                emit_copy_sprintf_segment_to_memory(
                    var,
                    *precision,
                    *width,
                    *left_align,
                    &out_ptr,
                    &out_idx,
                    module,
                );
            }
        }
    }
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_len));
    Ok(true)
}

pub(super) fn emit_sprintf_segment_len(var: &str, precision: Option<usize>, module: &mut WasmModule) {
    match precision {
        Some(precision) => {
            module.body().line(&format!("local.get ${}_len", var));
            module.body().line(&format!("i32.const {}", precision));
            module.body().line("i32.lt_u");
            module.body().open("if (result i32)");
            module.body().line(&format!("local.get ${}_len", var));
            module.body().line("else");
            module.body().line(&format!("i32.const {}", precision));
            module.body().close("end");
        }
        None => {
            module.body().line(&format!("local.get ${}_len", var));
        }
    }
}

fn emit_sprintf_padded_segment_len(
    var: &str,
    precision: Option<usize>,
    width: Option<usize>,
    module: &mut WasmModule,
) {
    let Some(width) = width else {
        emit_sprintf_segment_len(var, precision, module);
        return;
    };
    emit_sprintf_segment_len(var, precision, module);
    module.body().line(&format!("i32.const {}", width));
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line(&format!("i32.const {}", width));
    module.body().line("else");
    emit_sprintf_segment_len(var, precision, module);
    module.body().close("end");
}

fn emit_copy_sprintf_segment_to_memory(
    var: &str,
    precision: Option<usize>,
    width: Option<usize>,
    left_align: bool,
    out_ptr: &str,
    out_idx: &str,
    module: &mut WasmModule,
) {
    if precision.is_none() && width.is_none() {
        emit_copy_string_to_memory(var, out_ptr, out_idx, module);
        return;
    };
    let start = module.next_label("sprintf_precision_start");
    let end = module.next_label("sprintf_precision_end");
    let pad = module.next_label("sprintf_width_pad");
    module.declare_i32_local(start.trim_start_matches('$').to_string());
    module.declare_i32_local(end.trim_start_matches('$').to_string());
    module.declare_i32_local(pad.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", start));
    emit_sprintf_segment_len(var, precision, module);
    module.body().line(&format!("local.set {}", end));
    emit_sprintf_width_pad_len(&end, width, &pad, module);
    if !left_align {
        emit_copy_spaces_to_memory(&pad, out_ptr, out_idx, module);
    }
    emit_copy_string_range_to_memory(var, &start, &end, out_ptr, out_idx, module);
    if left_align {
        emit_copy_spaces_to_memory(&pad, out_ptr, out_idx, module);
    }
}

pub(super) fn emit_sprintf_width_pad_len(
    formatted_len: &str,
    width: Option<usize>,
    pad: &str,
    module: &mut WasmModule,
) {
    let Some(width) = width else {
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", pad));
        return;
    };
    module.body().line(&format!("local.get {}", formatted_len));
    module.body().line(&format!("i32.const {}", width));
    module.body().line("i32.lt_u");
    module.body().open("if");
    module.body().line(&format!("i32.const {}", width));
    module.body().line(&format!("local.get {}", formatted_len));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", pad));
    module.body().line("else");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pad));
    module.body().close("end");
}

