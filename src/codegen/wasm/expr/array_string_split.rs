//! Purpose:
//! Lowers runtime string-to-array builtins for wasm32-web assignments.
//! Keeps `str_split()` and `explode()` array materialization out of expression dispatch.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::emit_array_assign()`
//!
//! Key details:
//! - Results are value-cell arrays containing heap strings.
//! - Runtime length/limit checks trap for unsupported PHP-invalid states instead of
//!   silently emitting malformed value-cell arrays.

use super::*;

pub(in crate::codegen::wasm) fn emit_runtime_str_split_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if args.is_empty() || args.len() > 2 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web str_split() expects one or two arguments",
        ));
    }
    let static_chunk_size = args
        .get(1)
        .and_then(|arg| static_or_const_or_i64_local_value(arg, module));
    if let Some(chunk_size) = static_chunk_size {
        if chunk_size <= 0 {
            return Err(CompileError::new(
                call.span,
                "wasm32-web str_split() length must be greater than zero",
            ));
        }
        i32::try_from(chunk_size).map_err(|_| {
            CompileError::new(
                args.get(1).unwrap_or(&args[0]).span,
                "wasm32-web runtime str_split() length is too large",
            )
        })?;
    }
    let Some(source) = string_arg_or_materialize(&args[0], "str_split_source", module)? else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web runtime str_split() requires a string value",
        ));
    };
    let out_len = module.next_label("str_split_out_len");
    let out_index = module.next_label("str_split_out_index");
    let source_index = module.next_label("str_split_source_index");
    let chunk_size64 = module.next_label("str_split_chunk_size64");
    let chunk_size = module.next_label("str_split_chunk_size");
    let chunk_len = module.next_label("str_split_chunk_len");
    let copy_index = module.next_label("str_split_copy_index");
    let part_ptr = module.next_label("str_split_part_ptr");
    let cell = module.next_label("str_split_cell");
    let loop_label = module.next_label("str_split_loop");
    let done_label = module.next_label("str_split_done");
    let copy_loop = module.next_label("str_split_copy_loop");
    let copy_done = module.next_label("str_split_copy_done");
    for local in [
        &out_len,
        &out_index,
        &source_index,
        &chunk_size,
        &chunk_len,
        &copy_index,
        &part_ptr,
        &cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(chunk_size64.trim_start_matches('$').to_string());
    emit_release_current_value_array(name, module);
    if let Some(static_chunk_size) = static_chunk_size {
        module
            .body()
            .line(&format!("i32.const {}", static_chunk_size));
        module.body().line(&format!("local.set {}", chunk_size));
    } else if let Some(arg) = args.get(1) {
        require_int(arg, module)?;
        module.body().line(&format!("local.tee {}", chunk_size64));
        module.body().line("i64.const 0");
        module.body().line("i64.le_s");
        module.body().open("if");
        module.body().line("unreachable");
        module.body().close("end");
        module.body().line(&format!("local.get {}", chunk_size64));
        module.body().line("i64.const 2147483647");
        module.body().line("i64.gt_s");
        module.body().open("if");
        module.body().line("unreachable");
        module.body().close("end");
        module.body().line(&format!("local.get {}", chunk_size64));
        module.body().line("i32.wrap_i64");
        module.body().line(&format!("local.set {}", chunk_size));
    } else {
        module.body().line("i32.const 1");
        module.body().line(&format!("local.set {}", chunk_size));
    }
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.div_u");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", source_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i32.sub");
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.gt_u");
    module.body().open("if");
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line(&format!("local.set {}", chunk_len));
    module.body().line("else");
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", chunk_len));
    module.body().close("end");
    module.body().line(&format!("local.get {}", chunk_len));
    module.body().line("call $__rt_alloc_bytes");
    module.body().line(&format!("local.set {}", part_ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", copy_index));
    module.body().open(&format!("block {}", copy_done));
    module.body().open(&format!("loop {}", copy_loop));
    module.body().line(&format!("local.get {}", copy_index));
    module.body().line(&format!("local.get {}", chunk_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", copy_done));
    module.body().line(&format!("local.get {}", part_ptr));
    module.body().line(&format!("local.get {}", copy_index));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("local.get {}", copy_index));
    module.body().line("i32.add");
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.store8");
    module.body().line(&format!("local.get {}", copy_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", copy_index));
    module.body().line(&format!("br {}", copy_loop));
    module.body().close("end");
    module.body().close("end");
    emit_value_cell_address_for_local(name, &out_index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("local.get {}", part_ptr));
    module.body().line(&format!("local.get {}", chunk_len));
    module.body().line("call $__rt_value_store_string");
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("local.get {}", chunk_len));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.set_array_layout(name, ArrayLayout::Value);
    module.clear_array_length(name);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Str));
    module.set_array_nested_value_metadata(name, None);
    Ok(())
}

pub(in crate::codegen::wasm) fn emit_runtime_explode_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if args.len() != 2 && args.len() != 3 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web runtime explode() currently expects two or three arguments",
        ));
    }
    let static_limit = args
        .get(2)
        .and_then(|arg| static_or_const_or_i64_local_value(arg, module));
    if let Some(limit) = static_limit {
        if limit < 0 {
            i32::try_from(limit.unsigned_abs()).map_err(|_| {
                CompileError::new(args[2].span, "wasm32-web runtime explode() limit is too large")
            })?;
        } else {
            i32::try_from(limit).map_err(|_| {
                CompileError::new(args[2].span, "wasm32-web runtime explode() limit is too large")
            })?;
        }
    }
    let Some(source) = string_arg_or_materialize(&args[1], "explode_source", module)? else {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web runtime explode() requires a string value",
        ));
    };
    let separator = module
        .next_label("explode_separator")
        .trim_start_matches('$')
        .to_string();
    let out_len = module.next_label("explode_out_len");
    let limit_mode = module.next_label("explode_limit_mode");
    let limit_i32 = module.next_label("explode_limit_i32");
    let limit_i64 = module.next_label("explode_limit_i64");
    let source_index = module.next_label("explode_source_index");
    let segment_start = module.next_label("explode_segment_start");
    let out_index = module.next_label("explode_out_index");
    let loop_label = module.next_label("explode_count_loop");
    let done_label = module.next_label("explode_count_done");
    let emit_loop = module.next_label("explode_emit_loop");
    let emit_done = module.next_label("explode_emit_done");
    let segment_len = module.next_label("explode_segment_len");
    let copy_index = module.next_label("explode_copy_index");
    let part_ptr = module.next_label("explode_part_ptr");
    let cell = module.next_label("explode_cell");
    for local in [
        &out_len,
        &limit_mode,
        &limit_i32,
        &source_index,
        &segment_start,
        &out_index,
        &segment_len,
        &copy_index,
        &part_ptr,
        &cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i32_local(format!("{}_ptr", separator));
    module.declare_i32_local(format!("{}_len", separator));
    module.declare_i64_local(limit_i64.trim_start_matches('$').to_string());
    if let Some(static_separator) = static_or_tracked_string_value(&args[0], module) {
        if static_separator.is_empty() {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web explode() separator must be non-empty",
            ));
        }
        let (ptr, len) = module.intern_string(&static_separator);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("local.set ${}_ptr", separator));
        module.body().line(&format!("i32.const {}", len));
        module.body().line(&format!("local.set ${}_len", separator));
    } else if let Some(dynamic_separator) =
        runtime_string_arg_or_materialize(&args[0], "explode_separator_arg", module)?
    {
        module.body().line(&format!("local.get ${}_ptr", dynamic_separator));
        module.body().line(&format!("local.set ${}_ptr", separator));
        module.body().line(&format!("local.get ${}_len", dynamic_separator));
        module.body().line(&format!("local.set ${}_len", separator));
        module.body().line(&format!("local.get ${}_len", separator));
        module.body().line("i32.eqz");
        module.body().open("if");
        module.body().line("unreachable");
        module.body().close("end");
    } else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web runtime explode() requires a string separator",
        ));
    }
    emit_release_current_value_array(name, module);
    emit_runtime_explode_limit_setup(
        static_limit,
        args.get(2),
        &limit_mode,
        &limit_i32,
        &limit_i64,
        module,
    )?;
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", source_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_runtime_explode_separator_match(&source, &source_index, &separator, module);
    emit_runtime_explode_count_limit_condition(&out_len, &limit_mode, &limit_i32, module);
    module.body().open("if");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("local.get ${}_len", separator));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line("else");
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().close("end");
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", limit_mode));
    module.body().line("i32.const 2");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.get {}", limit_i32));
    module.body().line("i32.gt_u");
    module.body().open("if");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.get {}", limit_i32));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line("else");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_len));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", segment_start));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    module.body().open(&format!("block {}", emit_done));
    module.body().open(&format!("loop {}", emit_loop));
    module.body().line(&format!("local.get {}", limit_mode));
    module.body().line("i32.const 2");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", out_index));
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", emit_done));
    module.body().close("end");
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", emit_done));
    emit_runtime_explode_separator_match(&source, &source_index, &separator, module);
    emit_runtime_explode_emit_limit_condition(&out_index, &limit_mode, &limit_i32, module);
    module.body().open("if");
    emit_runtime_explode_segment_store(
        name,
        &source,
        &segment_start,
        &source_index,
        &out_index,
        &segment_len,
        &copy_index,
        &part_ptr,
        &cell,
        module,
    );
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("local.get ${}_len", separator));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", segment_start));
    module.body().line(&format!("local.get {}", segment_start));
    module.body().line(&format!("local.set {}", source_index));
    module.body().line("else");
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().close("end");
    module.body().line(&format!("br {}", emit_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", limit_mode));
    module.body().line("i32.const 2");
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", source_index));
    emit_runtime_explode_segment_store(
        name,
        &source,
        &segment_start,
        &source_index,
        &out_index,
        &segment_len,
        &copy_index,
        &part_ptr,
        &cell,
        module,
    );
    module.body().close("end");
    module.set_array_layout(name, ArrayLayout::Value);
    module.clear_array_length(name);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Str));
    module.set_array_nested_value_metadata(name, None);
    Ok(())
}

fn emit_runtime_explode_limit_setup(
    static_limit: Option<i64>,
    dynamic_limit: Option<&Expr>,
    limit_mode: &str,
    limit_i32: &str,
    limit_i64: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match (static_limit, dynamic_limit) {
        (Some(limit), _) if limit < 0 => {
            module.body().line("i32.const 2");
            module.body().line(&format!("local.set {}", limit_mode));
            module
                .body()
                .line(&format!("i32.const {}", limit.unsigned_abs()));
            module.body().line(&format!("local.set {}", limit_i32));
        }
        (Some(0), _) => {
            module.body().line("i32.const 1");
            module.body().line(&format!("local.set {}", limit_mode));
            module.body().line("i32.const 1");
            module.body().line(&format!("local.set {}", limit_i32));
        }
        (Some(limit), _) => {
            module.body().line("i32.const 1");
            module.body().line(&format!("local.set {}", limit_mode));
            module.body().line(&format!("i32.const {}", limit));
            module.body().line(&format!("local.set {}", limit_i32));
        }
        (None, Some(arg)) => {
            require_int(arg, module)?;
            module.body().line(&format!("local.tee {}", limit_i64));
            module.body().line("i64.const 0");
            module.body().line("i64.lt_s");
            module.body().open("if");
            module.body().line(&format!("local.get {}", limit_i64));
            module.body().line("i64.const -2147483647");
            module.body().line("i64.lt_s");
            module.body().open("if");
            module.body().line("unreachable");
            module.body().close("end");
            module.body().line("i64.const 0");
            module.body().line(&format!("local.get {}", limit_i64));
            module.body().line("i64.sub");
            module.body().line("i32.wrap_i64");
            module.body().line(&format!("local.set {}", limit_i32));
            module.body().line("i32.const 2");
            module.body().line(&format!("local.set {}", limit_mode));
            module.body().line("else");
            module.body().line(&format!("local.get {}", limit_i64));
            module.body().line("i64.const 2147483647");
            module.body().line("i64.gt_s");
            module.body().open("if");
            module.body().line("unreachable");
            module.body().close("end");
            module.body().line(&format!("local.get {}", limit_i64));
            module.body().line("i64.eqz");
            module.body().open("if");
            module.body().line("i32.const 1");
            module.body().line(&format!("local.set {}", limit_i32));
            module.body().line("else");
            module.body().line(&format!("local.get {}", limit_i64));
            module.body().line("i32.wrap_i64");
            module.body().line(&format!("local.set {}", limit_i32));
            module.body().close("end");
            module.body().line("i32.const 1");
            module.body().line(&format!("local.set {}", limit_mode));
            module.body().close("end");
        }
        (None, None) => {
            module.body().line("i32.const 0");
            module.body().line(&format!("local.set {}", limit_mode));
            module.body().line("i32.const 0");
            module.body().line(&format!("local.set {}", limit_i32));
        }
    }
    Ok(())
}

fn emit_runtime_explode_count_limit_condition(
    out_len: &str,
    limit_mode: &str,
    limit_i32: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", limit_mode));
    module.body().line("i32.const 1");
    module.body().line("i32.eq");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.get {}", limit_i32));
    module.body().line("i32.lt_u");
    module.body().line("else");
    module.body().line("i32.const 1");
    module.body().close("end");
    module.body().line("i32.and");
}

fn emit_runtime_explode_emit_limit_condition(
    out_index: &str,
    limit_mode: &str,
    limit_i32: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", limit_mode));
    module.body().line("i32.const 1");
    module.body().line("i32.eq");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", out_index));
    module.body().line(&format!("local.get {}", limit_i32));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.lt_u");
    module.body().line("else");
    module.body().line("i32.const 1");
    module.body().close("end");
    module.body().line("i32.and");
}

fn emit_runtime_explode_segment_store(
    name: &str,
    source: &str,
    segment_start: &str,
    segment_end: &str,
    out_index: &str,
    segment_len: &str,
    copy_index: &str,
    part_ptr: &str,
    cell: &str,
    module: &mut WasmModule,
) {
    let copy_loop = module.next_label("explode_copy_loop");
    let copy_done = module.next_label("explode_copy_done");
    module.body().line(&format!("local.get {}", segment_end));
    module.body().line(&format!("local.get {}", segment_start));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", segment_len));
    module.body().line(&format!("local.get {}", segment_len));
    module.body().line("call $__rt_alloc_bytes");
    module.body().line(&format!("local.set {}", part_ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", copy_index));
    module.body().open(&format!("block {}", copy_done));
    module.body().open(&format!("loop {}", copy_loop));
    module.body().line(&format!("local.get {}", copy_index));
    module.body().line(&format!("local.get {}", segment_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", copy_done));
    module.body().line(&format!("local.get {}", part_ptr));
    module.body().line(&format!("local.get {}", copy_index));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", segment_start));
    module.body().line(&format!("local.get {}", copy_index));
    module.body().line("i32.add");
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.store8");
    module.body().line(&format!("local.get {}", copy_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", copy_index));
    module.body().line(&format!("br {}", copy_loop));
    module.body().close("end");
    module.body().close("end");
    emit_value_cell_address_for_local(name, out_index, cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("local.get {}", part_ptr));
    module.body().line(&format!("local.get {}", segment_len));
    module.body().line("call $__rt_value_store_string");
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
}

fn emit_runtime_explode_separator_match(
    source: &str,
    source_index: &str,
    separator: &str,
    module: &mut WasmModule,
) {
    let match_index = module.next_label("explode_separator_match_index");
    let match_ok = module.next_label("explode_separator_match_ok");
    let loop_label = module.next_label("explode_separator_match_loop");
    let done_label = module.next_label("explode_separator_match_done");
    module.declare_i32_local(match_index.trim_start_matches('$').to_string());
    module.declare_i32_local(match_ok.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("local.get ${}_len", separator));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.le_u");
    module.body().line(&format!("local.set {}", match_ok));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", match_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", match_ok));
    module.body().line("i32.eqz");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", match_index));
    module.body().line(&format!("local.get ${}_len", separator));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("local.get {}", match_index));
    module.body().line("i32.add");
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.get ${}_ptr", separator));
    module.body().line(&format!("local.get {}", match_index));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", match_ok));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", match_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", match_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", match_ok));
}
