//! Purpose:
//! Lowers wasm32-web array_map(null, ...) assignment forms.
//! Keeps zip/pad row construction separate from normal callback dispatch.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_map_assign`.
//!
//! Key details:
//! - Matches PHP null-callback zip semantics, padding missing runtime values with null cells.

use super::*;

pub(super) fn emit_array_map_null_two_array_assign(
    name: &str,
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let sources = args
        .iter()
        .skip(1)
        .enumerate()
        .map(|(index, source)| {
            materialize_array_map_multi_source(
                source,
                &format!("array_map_null_source_{index}"),
                module,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let Some(source_lens) = array_map_null_known_source_lens(&sources, module) else {
        return emit_array_map_null_dynamic_assign(name, call, sources, module);
    };
    let len = source_lens.iter().copied().max().unwrap_or(0);
    if sources
        .iter()
        .any(|source| !array_map_null_source_is_supported(source, module))
    {
        return Err(CompileError::new(
            call.span,
            "wasm32-web array_map(null, ...) currently supports indexed compact/value arrays",
        ));
    }

    let nested = (0..len)
        .map(|index| {
            Some(NestedArrayMetadata {
                layout: ArrayLayout::Value,
                len: sources.len(),
                value_kinds: array_map_null_row_value_kinds(
                    &sources,
                    &source_lens,
                    index,
                    module,
                ),
                key_values: None,
                nested_values: None,
            })
        })
        .collect::<Vec<_>>();
    module.set_array_length(name, len);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Array; len]));
    module.set_array_nested_value_metadata(name, Some(nested));
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..len {
        let row = module
            .next_label("array_map_null_row")
            .trim_start_matches('$')
            .to_string();
        module.declare_array_local(row.clone());
        module.set_array_length(&row, sources.len());
        module.set_array_layout(&row, ArrayLayout::Value);
        module.set_array_value_cell_kinds(
            &row,
            array_map_null_row_value_kinds(&sources, &source_lens, index, module),
        );
        module.set_array_nested_value_metadata(&row, None);
        module.body().line(&format!("i32.const {}", sources.len()));
        module.body().line("call $__rt_alloc_value_cells");
        module.body().line(&format!("local.set ${}_ptr", row));
        module.body().line(&format!("i32.const {}", sources.len()));
        module.body().line(&format!("local.set ${}_len", row));
        for (target_index, source) in sources.iter().enumerate() {
            if index < source_lens[target_index] {
                emit_array_map_null_source_cell(&row, target_index, source, index, module);
            } else {
                emit_static_null_value_cell(&row, target_index, module);
            }
        }
        emit_store_array_value_cell_from_local(name, index, &row, module);
    }
    Ok(())
}

fn emit_array_map_null_dynamic_assign(
    name: &str,
    call: &Expr,
    sources: Vec<String>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if sources
        .iter()
        .any(|source| !array_map_null_source_is_supported(source, module))
    {
        return Err(CompileError::new(
            call.span,
            "wasm32-web array_map(null, ...) currently supports indexed compact/value arrays",
        ));
    }
    let sources = array_map_null_value_sources(sources, module)?;
    let out_len = module.next_label("array_map_null_out_len");
    let index = module.next_label("array_map_null_index");
    let row_ptr = module.next_label("array_map_null_row_ptr");
    let row_cell = module.next_label("array_map_null_row_cell");
    let outer_cell = module.next_label("array_map_null_outer_cell");
    for local in [&out_len, &index, &row_ptr, &row_cell, &outer_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_len));
    for source in &sources {
        module.body().line(&format!("local.get ${}_len", source));
        module.body().line(&format!("local.get {}", out_len));
        module.body().line("i32.gt_u");
        module.body().open("if");
        module.body().line(&format!("local.get ${}_len", source));
        module.body().line(&format!("local.set {}", out_len));
        module.body().close("end");
    }
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Array));
    module.set_array_runtime_nested_value_metadata(
        name,
        Some(NestedArrayMetadata {
            layout: ArrayLayout::Value,
            len: sources.len(),
            value_kinds: None,
            key_values: None,
            nested_values: None,
        }),
    );
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    let done_label = module.next_label("array_map_null_dynamic_done");
    let loop_label = module.next_label("array_map_null_dynamic_loop");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("i32.const {}", sources.len()));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", row_ptr));
    for (target_index, source) in sources.iter().enumerate() {
        emit_array_map_null_dynamic_source_cell(
            &row_ptr,
            &row_cell,
            target_index,
            source,
            &index,
            module,
        );
    }
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", outer_cell));
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line(&format!("local.get {}", row_ptr));
    module.body().line(&format!("i32.const {}", sources.len()));
    module.body().line("call $__rt_value_store_array");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn array_map_null_value_sources(
    sources: Vec<String>,
    module: &mut WasmModule,
) -> Result<Vec<String>, CompileError> {
    let mut out = Vec::with_capacity(sources.len());
    for source in sources {
        if module.array_layout(&source) == ArrayLayout::Value {
            out.push(source);
            continue;
        }
        if module.array_layout(&source) == ArrayLayout::Assoc {
            out.push(source);
            continue;
        }
        let value_source = module
            .next_label("array_map_null_value_source")
            .trim_start_matches('$')
            .to_string();
        module.declare_array_local(value_source.clone());
        emit_compact_array_to_value_array_assign(&value_source, &source, module)?;
        out.push(value_source);
    }
    Ok(out)
}

fn array_map_null_known_source_lens(
    sources: &[String],
    module: &WasmModule,
) -> Option<Vec<usize>> {
    sources
        .iter()
        .map(|source| module.array_length(source))
        .collect()
}

fn array_map_null_source_is_supported(source: &str, module: &WasmModule) -> bool {
    module.local_kind(source) == Some(LocalKind::Array)
        && matches!(
            module.array_layout(source),
            ArrayLayout::CompactInt | ArrayLayout::Value | ArrayLayout::Assoc
        )
}

fn array_map_null_row_value_kinds(
    sources: &[String],
    source_lens: &[usize],
    index: usize,
    module: &WasmModule,
) -> Option<Vec<ValueCellKind>> {
    sources
        .iter()
        .zip(source_lens)
        .map(|(source, len)| {
            if index < *len {
                array_map_null_source_value_kind(source, index, module)
            } else {
                Some(ValueCellKind::Null)
            }
        })
        .collect()
}

fn array_map_null_source_value_kind(
    source: &str,
    index: usize,
    module: &WasmModule,
) -> Option<ValueCellKind> {
    match module.array_layout(source) {
        ArrayLayout::CompactInt => Some(ValueCellKind::Int),
        ArrayLayout::Value => module
            .array_value_cell_kinds(source)
            .and_then(|kinds| kinds.get(index).copied()),
        ArrayLayout::Assoc => module
            .array_value_cell_kinds(source)
            .and_then(|kinds| kinds.get(index).copied()),
    }
}

fn emit_array_map_null_source_cell(
    row: &str,
    target_index: usize,
    source: &str,
    source_index: usize,
    module: &mut WasmModule,
) {
    match module.array_layout(source) {
        ArrayLayout::CompactInt => {
            emit_static_int_slot_as_value_cell(row, target_index, &format!("${}_ptr", source), source_index, module);
        }
        ArrayLayout::Value => {
            emit_copy_static_value_cell(row, target_index, &format!("${}_ptr", source), source_index, module);
        }
        ArrayLayout::Assoc => {
            emit_array_map_null_assoc_source_cell(row, target_index, source, source_index, module);
        }
    }
}

fn emit_array_map_null_assoc_source_cell(
    row: &str,
    target_index: usize,
    source: &str,
    source_index: usize,
    module: &mut WasmModule,
) {
    let source_cell = module.next_label("array_map_null_assoc_source_cell");
    module.declare_i32_local(source_cell.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", source_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", source_cell));
    module.body().line(&format!("local.get ${}_ptr", row));
    module.body().line(&format!("i32.const {}", target_index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.get {}", source_cell));
    module.body().line("call $__rt_value_copy");
}

fn emit_array_map_null_dynamic_source_cell(
    row_ptr: &str,
    row_cell: &str,
    target_index: usize,
    source: &str,
    index: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", row_ptr));
    module.body().line(&format!("i32.const {}", target_index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", row_cell));
    match module.array_layout(source) {
        ArrayLayout::Assoc => {
            module.body().line(&format!("local.get {}", index));
            module.body().line(&format!("local.get ${}_len", source));
            module.body().line("i32.lt_u");
            module.body().open("if");
            module.body().line(&format!("local.get {}", row_cell));
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get {}", index));
            module.body().line("call $__rt_assoc_entry");
            module.body().line("call $__rt_assoc_value_cell");
            module.body().line("call $__rt_value_copy");
            module.body().close("else");
            module.body().line(&format!("local.get {}", row_cell));
            module.body().line("call $__rt_value_store_null");
            module.body().close("end");
        }
        _ => {
            module.body().line(&format!("local.get {}", row_cell));
            module.body().line(&format!("local.get ${}_ptr", source));
            module.body().line(&format!("local.get ${}_len", source));
            module.body().line(&format!("local.get {}", index));
            module.body().line("call $__rt_value_copy_index_or_null");
        }
    }
}
