//! Purpose:
//! Emits direct scalar reads from array_chunk(...)[outer][inner] access paths for wasm32-web.
//! Keeps chunk-specific indexing shortcuts separate from general array indexing lowering.
//!
//! Called from:
//! - `super::array_indexing` and expression-kind classification helpers.
//!
//! Key details:
//! - Handles compact-int and homogeneous value-cell chunk sources, including function-return sources.

use super::*;
use super::array_indexing::{
    emit_value_cell_result_from_cell, indexed_array_expr_value_cell_kinds,
};

pub(super) fn emit_direct_array_chunk_first_scalar_read(
    expr: &Expr,
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    if let Some(kind) = emit_direct_array_chunk_homogeneous_scalar_read(array, index, module)? {
        return Ok(Some(kind));
    }
    if static_or_const_int_value(index) != Some(0) {
        return Ok(None);
    }
    let ExprKind::ArrayAccess { array: chunks, index: outer_index } = &array.kind else {
        return Ok(None);
    };
    if static_or_const_int_value(outer_index) != Some(0) {
        return Ok(None);
    }
    let ExprKind::FunctionCall { name, args } = &chunks.kind else {
        return Ok(None);
    };
    if !name.eq_ignore_ascii_case("array_chunk") {
        return Ok(None);
    }
    let _preserve_keys = array_chunk_preserve_keys_arg(args, chunks.span, module)?;
    if args.len() < 2 {
        return Err(CompileError::new(
            chunks.span,
            "wasm32-web array_chunk() expects two or three arguments",
        ));
    }
    if !matches!(
        args[0].kind,
        ExprKind::ArrayLiteral(_) | ExprKind::Variable(_) | ExprKind::FunctionCall { .. }
    ) {
        return Ok(None);
    }
    let Some(source_len) = known_indexed_array_expr_len(&args[0], module) else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web scalar array_chunk(...)[0][0] requires a known non-empty indexed source length",
        ));
    };
    if source_len == 0 {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web scalar array_chunk(...)[0][0] does not support empty sources yet",
        ));
    }
    let chunk_size = module.next_label("array_chunk_first_read_size");
    module.declare_i32_local(chunk_size.trim_start_matches('$').to_string());
    require_int(&args[1], module)?;
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", chunk_size));
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.const 0");
    module.body().line("i32.le_s");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    let zero = Expr::new(ExprKind::IntLiteral(0), index.span);
    emit_array_index_expr(expr, &args[0], &zero, module).map(Some)
}
fn emit_direct_array_chunk_homogeneous_scalar_read(
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    if emit_direct_array_chunk_compact_int_scalar_read(array, index, module)? {
        return Ok(Some(ValueKind::Int));
    }
    if emit_direct_array_chunk_return_compact_int_scalar_read(array, index, module)? {
        return Ok(Some(ValueKind::Int));
    }
    if let Some(kind) = emit_direct_array_chunk_return_homogeneous_scalar_read(array, index, module)? {
        return Ok(Some(kind));
    }
    let Some((source, outer_index, inner_index, kind, source_len, chunk_size_expr)) =
        direct_array_chunk_homogeneous_access(array, index, module)
    else {
        return Ok(None);
    };
    let chunk_size = module.next_label("array_chunk_scalar_size");
    let source_index = module.next_label("array_chunk_scalar_source_index");
    let cell = module.next_label("array_chunk_scalar_cell");
    for local in [&chunk_size, &source_index, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    require_int(chunk_size_expr, module)?;
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", chunk_size));
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.const 0");
    module.body().line("i32.le_s");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("i32.const {}", inner_index));
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.ge_u");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("i32.const {}", outer_index));
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.mul");
    module.body().line(&format!("i32.const {}", inner_index));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("i32.const {}", source_len));
    module.body().line("i32.ge_u");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_value_cell_result_from_cell(&cell, kind, module).map(Some)
}
fn emit_direct_array_chunk_compact_int_scalar_read(
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let Some((source, outer_index, inner_index, source_len, chunk_size_expr)) =
        direct_array_chunk_compact_int_access(array, index, module)
    else {
        return Ok(false);
    };
    emit_direct_array_chunk_compact_int_load(
        &format!("${}_ptr", source),
        outer_index,
        inner_index,
        source_len,
        chunk_size_expr,
        module,
    )?;
    Ok(true)
}
fn emit_direct_array_chunk_return_compact_int_scalar_read(
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let Some((function_name, call_args, outer_index, inner_index, source_len, chunk_size_expr)) =
        direct_array_chunk_return_compact_int_access(array, index, module)
    else {
        return Ok(false);
    };
    let source_ptr = module.next_label("array_chunk_return_int_ptr");
    let source_len_local = module.next_label("array_chunk_return_int_len");
    for local in [&source_ptr, &source_len_local] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_user_function_args(array, function_name, call_args, module)?;
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(function_name)));
    module.body().line(&format!("local.set {}", source_len_local));
    module.body().line(&format!("local.set {}", source_ptr));
    emit_direct_array_chunk_compact_int_load(
        &source_ptr,
        outer_index,
        inner_index,
        source_len,
        chunk_size_expr,
        module,
    )?;
    Ok(true)
}
fn emit_direct_array_chunk_compact_int_load(
    source_ptr: &str,
    outer_index: usize,
    inner_index: usize,
    source_len: usize,
    chunk_size_expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let chunk_size = module.next_label("array_chunk_int_size");
    let source_index = module.next_label("array_chunk_int_source_index");
    for local in [&chunk_size, &source_index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    require_int(chunk_size_expr, module)?;
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", chunk_size));
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.const 0");
    module.body().line("i32.le_s");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("i32.const {}", inner_index));
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.ge_u");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("i32.const {}", outer_index));
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.mul");
    module.body().line(&format!("i32.const {}", inner_index));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("i32.const {}", source_len));
    module.body().line("i32.ge_u");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    Ok(())
}
fn emit_direct_array_chunk_return_homogeneous_scalar_read(
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    let Some((function_name, call_args, outer_index, inner_index, kind, source_len, chunk_size_expr)) =
        direct_array_chunk_return_homogeneous_access(array, index, module)
    else {
        return Ok(None);
    };
    let source_ptr = module.next_label("array_chunk_return_scalar_ptr");
    let source_len_local = module.next_label("array_chunk_return_scalar_len");
    let chunk_size = module.next_label("array_chunk_return_scalar_size");
    let source_index = module.next_label("array_chunk_return_scalar_source_index");
    let cell = module.next_label("array_chunk_return_scalar_cell");
    for local in [&source_ptr, &source_len_local, &chunk_size, &source_index, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_user_function_args(array, function_name, call_args, module)?;
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(function_name)));
    module.body().line(&format!("local.set {}", source_len_local));
    module.body().line(&format!("local.set {}", source_ptr));
    require_int(chunk_size_expr, module)?;
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", chunk_size));
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.const 0");
    module.body().line("i32.le_s");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("i32.const {}", inner_index));
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.ge_u");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("i32.const {}", outer_index));
    module.body().line(&format!("local.get {}", chunk_size));
    module.body().line("i32.mul");
    module.body().line(&format!("i32.const {}", inner_index));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("i32.const {}", source_len));
    module.body().line("i32.ge_u");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", cell));
    emit_value_cell_result_from_cell(&cell, kind, module).map(Some)
}
fn direct_array_chunk_compact_int_access<'a>(
    array: &'a Expr,
    index: &Expr,
    module: &WasmModule,
) -> Option<(&'a str, usize, usize, usize, &'a Expr)> {
    let inner_index = usize::try_from(static_or_const_int_value(index)?).ok()?;
    let ExprKind::ArrayAccess { array: chunks, index: outer_index } = &array.kind else {
        return None;
    };
    let outer_index = usize::try_from(static_or_const_int_value(outer_index)?).ok()?;
    let ExprKind::FunctionCall { name, args } = &chunks.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_chunk") || args.len() != 2 {
        return None;
    }
    let ExprKind::Variable(source) = &args[0].kind else {
        return None;
    };
    if module.local_kind(source) != Some(LocalKind::Array) || module.array_layout(source) != ArrayLayout::CompactInt {
        return None;
    }
    let source_len = module.array_length(source)?;
    Some((source.as_str(), outer_index, inner_index, source_len, &args[1]))
}
fn direct_array_chunk_return_compact_int_access<'a>(
    array: &'a Expr,
    index: &Expr,
    module: &WasmModule,
) -> Option<(&'a str, &'a [Expr], usize, usize, usize, &'a Expr)> {
    let inner_index = usize::try_from(static_or_const_int_value(index)?).ok()?;
    let ExprKind::ArrayAccess { array: chunks, index: outer_index } = &array.kind else {
        return None;
    };
    let outer_index = usize::try_from(static_or_const_int_value(outer_index)?).ok()?;
    let ExprKind::FunctionCall { name, args } = &chunks.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_chunk") || args.len() != 2 {
        return None;
    }
    let ExprKind::FunctionCall { name: function_name, args: call_args } = &args[0].kind else {
        return None;
    };
    if !module.has_function(function_name)
        || module.function_return_kind(function_name) != Some(ValueKind::Array)
        || module.function_array_return_layout(function_name) != ArrayLayout::CompactInt
    {
        return None;
    }
    let source_len = module.function_array_return_length(function_name)?;
    Some((function_name.as_str(), call_args.as_slice(), outer_index, inner_index, source_len, &args[1]))
}
pub(super) fn direct_array_chunk_homogeneous_access<'a>(
    array: &'a Expr,
    index: &Expr,
    module: &WasmModule,
) -> Option<(&'a str, usize, usize, ValueCellKind, usize, &'a Expr)> {
    let inner_index = usize::try_from(static_or_const_int_value(index)?).ok()?;
    let ExprKind::ArrayAccess { array: chunks, index: outer_index } = &array.kind else {
        return None;
    };
    let outer_index = usize::try_from(static_or_const_int_value(outer_index)?).ok()?;
    let ExprKind::FunctionCall { name, args } = &chunks.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_chunk") || args.len() != 2 {
        return None;
    }
    let ExprKind::Variable(source) = &args[0].kind else {
        return None;
    };
    if module.local_kind(source) != Some(LocalKind::Array) || module.array_layout(source) != ArrayLayout::Value {
        return None;
    }
    let source_len = module.array_length(source)?;
    let first_kind = module.array_value_cell_kind(source, 0)?;
    let kinds = module.array_value_cell_kinds(source)?;
    kinds
        .iter()
        .all(|kind| *kind == first_kind)
        .then_some((source.as_str(), outer_index, inner_index, first_kind, source_len, &args[1]))
}
pub(super) fn direct_array_chunk_return_homogeneous_access<'a>(
    array: &'a Expr,
    index: &Expr,
    module: &WasmModule,
) -> Option<(&'a str, &'a [Expr], usize, usize, ValueCellKind, usize, &'a Expr)> {
    let inner_index = usize::try_from(static_or_const_int_value(index)?).ok()?;
    let ExprKind::ArrayAccess { array: chunks, index: outer_index } = &array.kind else {
        return None;
    };
    let outer_index = usize::try_from(static_or_const_int_value(outer_index)?).ok()?;
    let ExprKind::FunctionCall { name, args } = &chunks.kind else {
        return None;
    };
    if !name.eq_ignore_ascii_case("array_chunk") || args.len() != 2 {
        return None;
    }
    let ExprKind::FunctionCall { name: function_name, args: call_args } = &args[0].kind else {
        return None;
    };
    if !module.has_function(function_name)
        || module.function_return_kind(function_name) != Some(ValueKind::Array)
        || module.function_array_return_layout(function_name) != ArrayLayout::Value
    {
        return None;
    }
    let (source_len, kinds) = if let Some(len) = module.function_array_return_length(function_name) {
        (len, module.function_array_return_value_kinds(function_name)?.to_vec())
    } else if let Some(param_index) = module.function_array_return_param_index(function_name) {
        let arg = call_args.get(param_index)?;
        (known_indexed_array_expr_len(arg, module)?, indexed_array_expr_value_cell_kinds(arg, module)?)
    } else {
        return None;
    };
    let first_kind = kinds.first().copied()?;
    kinds
        .iter()
        .all(|kind| *kind == first_kind)
        .then_some((function_name.as_str(), call_args.as_slice(), outer_index, inner_index, first_kind, source_len, &args[1]))
}
