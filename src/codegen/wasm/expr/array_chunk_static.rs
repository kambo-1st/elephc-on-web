//! Purpose:
//! Emits static and known-length wasm32-web array_chunk inner materialization helpers.
//! Keeps literal chunk construction and inner chunk copy loops out of runtime chunk lowering.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_chunk`.
//!
//! Key details:
//! - Preserves PHP reindexing, preserve_keys metadata, and nested value-cell metadata for known sources.

use super::*;

pub(super) fn static_array_chunk_items(expr: &Expr, items: &[Expr], chunk_size: usize) -> Vec<Expr> {
    items
        .chunks(chunk_size)
        .map(|chunk| Expr::new(ExprKind::ArrayLiteral(chunk.to_vec()), expr.span))
        .collect()
}

pub(super) fn static_array_chunk_preserve_key_items(expr: &Expr, items: &[Expr], chunk_size: usize) -> Vec<Expr> {
    items
        .chunks(chunk_size)
        .enumerate()
        .map(|(chunk_index, chunk)| {
            let start = chunk_index * chunk_size;
            let items = chunk
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    (
                        Expr::new(ExprKind::IntLiteral((start + index) as i64), expr.span),
                        value.clone(),
                    )
                })
                .collect();
            Expr::new(ExprKind::ArrayLiteralAssoc(items), expr.span)
        })
        .collect()
}

pub(super) fn static_assoc_array_chunk_items(
    expr: &Expr,
    items: &[(Expr, Expr)],
    chunk_size: usize,
) -> Vec<Expr> {
    items
        .chunks(chunk_size)
        .map(|chunk| Expr::new(ExprKind::ArrayLiteralAssoc(chunk.to_vec()), expr.span))
        .collect()
}

pub(super) fn emit_known_array_chunk_assign(
    name: &str,
    source_ptr: &str,
    source_layout: ArrayLayout,
    len: usize,
    chunk_size: usize,
    source_value_kinds: Option<Vec<ValueCellKind>>,
    source_nested_metadata: Option<Vec<Option<NestedArrayMetadata>>>,
    source_key_values: Option<Vec<AssocKeyValue>>,
    preserve_keys: bool,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let outer_len = len.div_ceil(chunk_size);
    let chunk_layout = if preserve_keys {
        ArrayLayout::Assoc
    } else if source_layout == ArrayLayout::CompactInt {
        ArrayLayout::CompactInt
    } else {
        ArrayLayout::Value
    };
    module.set_array_length(name, outer_len);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Array; outer_len]));
    module.body().line(&format!("i32.const {}", outer_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", outer_len));
    module.body().line(&format!("local.set ${}_len", name));
    for chunk_index in 0..outer_len {
        let start = chunk_index * chunk_size;
        let take = chunk_size.min(len - start);
        let chunk = module
            .next_label("array_chunk_inner")
            .trim_start_matches('$')
            .to_string();
        module.declare_array_local(chunk.clone());
        if preserve_keys {
            let value_kinds = source_value_kinds
                .as_ref()
                .map(|kinds| kinds[start..start + take].to_vec());
            let nested_metadata = source_nested_metadata
                .as_ref()
                .map(|metadata| metadata[start..start + take].to_vec());
            let key_values = preserved_chunk_key_values(
                source_layout,
                &source_key_values,
                start,
                take,
            );
            emit_known_preserve_key_array_chunk_inner(
                &chunk,
                source_ptr,
                source_layout,
                start,
                take,
                value_kinds,
                nested_metadata,
                key_values,
                module,
            );
        } else if source_layout == ArrayLayout::Value || source_layout == ArrayLayout::Assoc {
            let value_kinds = source_value_kinds
                .as_ref()
                .map(|kinds| kinds[start..start + take].to_vec());
            let nested_metadata = source_nested_metadata
                .as_ref()
                .map(|metadata| metadata[start..start + take].to_vec());
            emit_known_value_array_chunk_inner(
                &chunk,
                source_ptr,
                source_layout,
                start,
                take,
                value_kinds,
                nested_metadata,
                module,
            );
        } else {
            emit_known_compact_array_chunk_inner(&chunk, source_ptr, start, take, module);
        }
        module.set_array_nested_value_metadata(
            name,
            Some(
                (0..outer_len)
                    .map(|index| {
                        let start = index * chunk_size;
                        let take = chunk_size.min(len - start);
                        Some(NestedArrayMetadata {
                            layout: chunk_layout,
                            len: take,
                            value_kinds: source_value_kinds
                                .as_ref()
                                .map(|kinds| kinds[start..start + take].to_vec()),
                            key_values: if preserve_keys {
                                preserved_chunk_key_values(
                                    source_layout,
                                    &source_key_values,
                                    start,
                                    take,
                                )
                            } else {
                                None
                            },
                            nested_values: source_nested_metadata
                                .as_ref()
                                .map(|metadata| metadata[start..start + take].to_vec()),
                        })
                    })
                    .collect(),
            ),
        );
        emit_store_array_value_cell_from_local(name, chunk_index, &chunk, module);
    }
    Ok(())
}

pub(super) fn preserved_chunk_key_values(
    source_layout: ArrayLayout,
    source_key_values: &Option<Vec<AssocKeyValue>>,
    start: usize,
    take: usize,
) -> Option<Vec<AssocKeyValue>> {
    if source_layout == ArrayLayout::Assoc {
        return source_key_values
            .as_ref()
            .map(|keys| keys[start..start + take].to_vec());
    }
    Some(
        (start..start + take)
            .map(|index| AssocKeyValue::Int(index as i64))
            .collect(),
    )
}

pub(super) fn emit_known_compact_array_chunk_inner(
    chunk: &str,
    source_ptr: &str,
    start: usize,
    take: usize,
    module: &mut WasmModule,
) {
    module.set_array_length(chunk, take);
    module.set_array_layout(chunk, ArrayLayout::CompactInt);
    module.body().line(&format!("i32.const {}", take));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", chunk));
    module.body().line(&format!("i32.const {}", take));
    module.body().line(&format!("local.set ${}_len", chunk));
    for index in 0..take {
        emit_array_store_load(chunk, index, source_ptr, start + index, module);
    }
}

pub(super) fn emit_known_value_array_chunk_inner(
    chunk: &str,
    source_ptr: &str,
    source_layout: ArrayLayout,
    start: usize,
    take: usize,
    value_kinds: Option<Vec<ValueCellKind>>,
    nested_metadata: Option<Vec<Option<NestedArrayMetadata>>>,
    module: &mut WasmModule,
) {
    module.set_array_length(chunk, take);
    module.set_array_layout(chunk, ArrayLayout::Value);
    module.set_array_value_cell_kinds(chunk, value_kinds);
    module.set_array_nested_value_metadata(chunk, nested_metadata);
    module.body().line(&format!("i32.const {}", take));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", chunk));
    module.body().line(&format!("i32.const {}", take));
    module.body().line(&format!("local.set ${}_len", chunk));
    for index in 0..take {
        if source_layout == ArrayLayout::Assoc {
            emit_copy_static_assoc_value_cell(chunk, index, source_ptr, start + index, module);
        } else {
            emit_copy_static_value_cell(chunk, index, source_ptr, start + index, module);
        }
    }
}

pub(super) fn emit_known_preserve_key_array_chunk_inner(
    chunk: &str,
    source_ptr: &str,
    source_layout: ArrayLayout,
    start: usize,
    take: usize,
    value_kinds: Option<Vec<ValueCellKind>>,
    nested_metadata: Option<Vec<Option<NestedArrayMetadata>>>,
    key_values: Option<Vec<AssocKeyValue>>,
    module: &mut WasmModule,
) {
    module.set_array_length(chunk, take);
    module.set_array_layout(chunk, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(chunk, value_kinds);
    module.set_array_nested_value_metadata(chunk, nested_metadata);
    module.set_array_key_kinds(
        chunk,
        key_values.as_ref().map(|keys| {
            keys.iter()
                .map(|key| match key {
                    AssocKeyValue::Int(_) => AssocKeyKind::Int,
                    AssocKeyValue::Str(_) => AssocKeyKind::Str,
                })
                .collect()
        }),
    );
    module.set_array_key_values(chunk, key_values);
    module.body().line(&format!("i32.const {}", take));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", chunk));
    module.body().line(&format!("i32.const {}", take));
    module.body().line(&format!("local.set ${}_len", chunk));
    for index in 0..take {
        match source_layout {
            ArrayLayout::Assoc => {
                emit_assoc_array_copy_key_from_assoc(chunk, index, source_ptr, start + index, module);
                emit_assoc_array_copy_value_cell_from_assoc(chunk, index, source_ptr, start + index, module);
            }
            ArrayLayout::CompactInt => {
                emit_assoc_array_store_int_key(chunk, index, (start + index) as i64, module);
                emit_assoc_array_store_int_slot_value(chunk, index, source_ptr, start + index, module);
            }
            ArrayLayout::Value => {
                emit_assoc_array_store_int_key(chunk, index, (start + index) as i64, module);
                emit_assoc_array_copy_value_cell_from_indexed(chunk, index, source_ptr, start + index, module);
            }
        }
    }
}

pub(super) fn emit_assoc_array_copy_key_from_assoc(
    name: &str,
    index: usize,
    source_ptr: &str,
    source_index: usize,
    module: &mut WasmModule,
) {
    let target_entry = module.next_label("array_chunk_target_assoc_entry");
    let source_entry = module.next_label("array_chunk_source_assoc_entry");
    module.declare_i32_local(target_entry.trim_start_matches('$').to_string());
    module.declare_i32_local(source_entry.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("i32.const {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("i32.const {}", source_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", source_entry));
    copy_assoc_entry_key(&target_entry, &source_entry, module);
}

pub(super) fn emit_assoc_array_copy_value_cell_from_assoc(
    name: &str,
    index: usize,
    source_ptr: &str,
    source_index: usize,
    module: &mut WasmModule,
) {
    let target_entry = module.next_label("array_chunk_target_assoc_value_entry");
    let source_entry = module.next_label("array_chunk_source_assoc_value_entry");
    module.declare_i32_local(target_entry.trim_start_matches('$').to_string());
    module.declare_i32_local(source_entry.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("i32.const {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("i32.const {}", source_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", source_entry));
    copy_assoc_entry_value(&target_entry, &source_entry, module);
}

pub(super) fn emit_store_array_value_cell_from_local(
    name: &str,
    index: usize,
    array: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
    module.body().line("i32.add");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_ARRAY));
    module.body().line("i32.store");
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE + 8));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_ptr", array));
    module.body().line("i32.store");
    module.body().line(&format!("local.get ${}_ptr", name));
    module
        .body()
        .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE + 12));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", array));
    module.body().line("i32.store");
}

