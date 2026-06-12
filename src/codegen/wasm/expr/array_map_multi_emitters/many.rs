//! Purpose:
//! Emits shared wasm32-web array_map lowering for many-source mixed array assignments.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_map_multi_emitters::{seven,eight}`.
//!
//! Key details:
//! - Keeps high-arity mixed callback loops in one wasm-only helper.
//! - Uses boxed mixed argument cells and PHP null-fill semantics for shorter sources.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_array_map_many_mixed_locals_assign(
    name: &str,
    sources: &[&str],
    arity_label: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    for source in sources {
        if module.local_kind(source) != Some(LocalKind::Array)
            || !matches!(
                module.array_layout(source),
                ArrayLayout::CompactInt | ArrayLayout::Value | ArrayLayout::Assoc
            )
        {
            return Err(CompileError::new(
                source_span,
                "wasm32-web mixed high-arity array_map() currently requires compact/value/associative arrays",
            ));
        }
    }
    let result_kind = match module.function_return_kind(callback) {
        Some(ValueKind::Int) => ValueCellKind::Int,
        Some(ValueKind::Str) => ValueCellKind::Str,
        Some(ValueKind::Bool) => ValueCellKind::Bool,
        Some(ValueKind::Float) => ValueCellKind::Float,
        _ => {
            return Err(CompileError::new(
                source_span,
                "wasm32-web mixed high-arity array_map() currently requires int, string, bool, or float callback results",
            ));
        }
    };
    let result_len = module.next_label(&format!("array_map_mixed_{arity_label}_len"));
    let index = module.next_label(&format!("array_map_mixed_{arity_label}_index"));
    let result_cell = module.next_label(&format!("array_map_mixed_{arity_label}_result_cell"));
    let source_cell = module.next_label(&format!("array_map_mixed_{arity_label}_source_cell"));
    let arg_cells: Vec<_> = sources
        .iter()
        .enumerate()
        .map(|(arg_index, _)| {
            module.next_label(&format!("array_map_mixed_{arity_label}_arg_{arg_index}"))
        })
        .collect();
    for local in [&result_len, &index, &result_cell, &source_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for arg in &arg_cells {
        module.declare_i32_local(arg.trim_start_matches('$').to_string());
    }
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_nested_value_metadata(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(result_kind));
    if sources.iter().all(|source| module.array_length(source).is_some()) {
        let len = sources
            .iter()
            .filter_map(|source| module.array_length(source))
            .max()
            .unwrap_or(0);
        module.set_array_length(name, len);
        module.set_array_value_cell_kinds(name, Some(vec![result_kind; len]));
        module.body().line(&format!("i32.const {}", len));
    } else {
        module.clear_array_length(name);
        module.set_array_value_cell_kinds(name, None);
        let Some((first, rest)) = sources.split_first() else {
            return Err(CompileError::new(
                source_span,
                "wasm32-web array_map() requires at least one array source",
            ));
        };
        module.body().line(&format!("local.get ${}_len", first));
        module.body().line(&format!("local.set {}", result_len));
        for source in rest {
            module.body().line(&format!("local.get {}", result_len));
            module.body().line(&format!("local.get ${}_len", source));
            module.body().line("i32.gt_u");
            module.body().open("if (result i32)");
            module.body().line(&format!("local.get {}", result_len));
            module.body().close("else");
            module.body().line(&format!("local.get ${}_len", source));
            module.body().close("end");
            module.body().line(&format!("local.set {}", result_len));
        }
        module.body().line(&format!("local.get {}", result_len));
    }
    module.body().line(&format!("local.set {}", result_len));
    module.body().line(&format!("local.get {}", result_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", result_len));
    module.body().line(&format!("local.set ${}_len", name));
    for arg in &arg_cells {
        emit_alloc_mixed_cell(arg.trim_start_matches('$'), module);
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    let loop_label = module.next_label(&format!("array_map_mixed_{arity_label}_loop"));
    let done_label = module.next_label(&format!("array_map_mixed_{arity_label}_done"));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", result_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(name, &index, &result_cell, module);
    module.body().line(&format!("local.get {}", result_cell));
    for (source, arg) in sources.iter().zip(arg_cells.iter()) {
        emit_array_map_two_mixed_arg_cell(source, &index, arg, &source_cell, module);
    }
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    match result_kind {
        ValueCellKind::Int => module.body().line("call $__rt_value_store_int"),
        ValueCellKind::Str => module.body().line("call $__rt_value_store_string"),
        ValueCellKind::Bool => module.body().line("call $__rt_value_store_bool"),
        ValueCellKind::Float => module.body().line("call $__rt_value_store_float"),
        _ => unreachable!("mixed many-array array_map() result kind was checked"),
    }
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    for arg in &arg_cells {
        module.body().line(&format!("local.get {}", arg));
        module.body().line("call $__rt_value_release");
    }
    Ok(())
}
