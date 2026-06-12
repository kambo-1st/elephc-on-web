//! Purpose:
//! Builds preserve-key and reindex metadata for wasm32-web `array_slice()` results.
//!
//! Called from:
//! - `super::array_slice` and other array transform helpers that reuse PHP key reindexing.
//!
//! Key details:
//! - PHP reindexes integer keys when preserve_keys is false, while string keys stay stable.
//! - Known indexed preserve-key slices are represented as associative value cells.

use super::*;

pub(super) fn sliced_assoc_default_items(
    span: crate::span::Span,
    items: &[(Expr, Expr)],
) -> Result<Vec<(Expr, Expr)>, CompileError> {
    let mut next_int_key = 0i64;
    let mut out = Vec::with_capacity(items.len());
    for (key, value) in items {
        let key = if static_int_value(key).is_some() {
            let key = Expr::new(ExprKind::IntLiteral(next_int_key), key.span);
            next_int_key += 1;
            key
        } else {
            match &key.kind {
                ExprKind::StringLiteral(_) => key.clone(),
                _ => {
                    return Err(CompileError::new(
                        span,
                        "wasm32-web array_slice() on associative arrays currently requires static integer or string keys",
                    ));
                }
            }
        };
        out.push((key, value.clone()));
    }
    Ok(out)
}

pub(super) fn sliced_indexed_preserve_key_items(
    expr: &Expr,
    items: &[Expr],
    start: usize,
) -> Vec<(Expr, Expr)> {
    items
        .iter()
        .enumerate()
        .map(|(index, value)| {
            (
                Expr::new(ExprKind::IntLiteral((start + index) as i64), expr.span),
                value.clone(),
            )
        })
        .collect()
}

pub(super) fn sliced_assoc_preserve_key_items(
    span: crate::span::Span,
    items: &[(Expr, Expr)],
) -> Result<Vec<(Expr, Expr)>, CompileError> {
    let mut out = Vec::with_capacity(items.len());
    for (key, value) in items {
        if static_int_value(key).is_none() && !matches!(key.kind, ExprKind::StringLiteral(_)) {
            return Err(CompileError::new(
                span,
                "wasm32-web array_slice() on associative arrays currently requires static integer or string keys",
            ));
        }
        out.push((key.clone(), value.clone()));
    }
    Ok(out)
}

pub(super) fn emit_known_indexed_array_slice_preserve_keys_assign(
    name: &str,
    source: &str,
    source_layout: ArrayLayout,
    start: usize,
    take: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_length(name, take);
    module.set_array_key_kinds(name, Some(vec![AssocKeyKind::Int; take]));
    module.set_array_key_values(
        name,
        Some((start..start + take).map(|index| AssocKeyValue::Int(index as i64)).collect()),
    );
    match source_layout {
        ArrayLayout::CompactInt => {
            module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Int; take]));
        }
        ArrayLayout::Value => {
            module.set_array_value_cell_kinds(
                name,
                module
                    .array_value_cell_kinds(source)
                    .map(|kinds| kinds[start..start + take].to_vec()),
            );
            module.set_array_value_constants(
                name,
                module
                    .array_value_constants(source)
                    .map(|values| values[start..start + take].to_vec()),
            );
            module.set_array_nested_value_metadata(
                name,
                module
                    .array_nested_value_metadata_items(source)
                    .map(|items| items[start..start + take].to_vec()),
            );
            module.set_array_object_classes(
                name,
                module
                    .array_object_classes(source)
                    .map(|classes| classes[start..start + take].to_vec()),
            );
        }
        ArrayLayout::Assoc => unreachable!("indexed preserve-key slice requires indexed source"),
    }

    let source_ptr = preserve_array_ptr(source, "array_slice_preserve_keys", module);
    let target_entry = module.next_label("array_slice_preserve_target_entry");
    let target_cell = module.next_label("array_slice_preserve_target_cell");
    let source_cell = module.next_label("array_slice_preserve_source_cell");
    module.declare_i32_local(target_entry.trim_start_matches('$').to_string());
    module.declare_i32_local(target_cell.trim_start_matches('$').to_string());
    module.declare_i32_local(source_cell.trim_start_matches('$').to_string());
    module.body().line(&format!("i32.const {}", take));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", take));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..take {
        emit_assoc_entry_address_const_index(&format!("${}_ptr", name), index, &target_entry, module);
        module.body().line(&format!("local.get {}", target_entry));
        module.body().line(&format!("i64.const {}", start + index));
        module.body().line("call $__rt_assoc_store_int_key");
        emit_assoc_value_cell_address(&target_entry, &target_cell, module);
        match source_layout {
            ArrayLayout::CompactInt => {
                module.body().line(&format!("local.get {}", target_cell));
                module.body().line(&format!("i64.const {}", WASM_VALUE_TAG_INT));
                module.body().line("i64.store");
                module.body().line(&format!("local.get {}", target_cell));
                module.body().line("i32.const 8");
                module.body().line("i32.add");
                module.body().line(&format!("local.get {}", source_ptr));
                module.body().line(&format!("i32.const {}", (start + index) * 8));
                module.body().line("i32.add");
                module.body().line("i64.load");
                module.body().line("i64.store");
            }
            ArrayLayout::Value => {
                module.body().line(&format!("local.get {}", source_ptr));
                module.body().line(&format!("i32.const {}", start + index));
                module.body().line("call $__rt_value_cell");
                module.body().line(&format!("local.set {}", source_cell));
                module.body().line(&format!("local.get {}", target_cell));
                module.body().line(&format!("local.get {}", source_cell));
                module.body().line("call $__rt_value_copy");
            }
            ArrayLayout::Assoc => unreachable!("indexed preserve-key slice requires indexed source"),
        }
    }
    Ok(())
}

pub(super) fn reindexed_assoc_key_kinds(key_kinds: &[AssocKeyKind]) -> Vec<AssocKeyKind> {
    key_kinds
        .iter()
        .map(|kind| match kind {
            AssocKeyKind::Int => AssocKeyKind::Int,
            AssocKeyKind::Str => AssocKeyKind::Str,
        })
        .collect()
}

pub(super) fn reindexed_assoc_key_values(
    key_kinds: &[AssocKeyKind],
    key_values: &[AssocKeyValue],
) -> Vec<AssocKeyValue> {
    let mut next_int_key = 0i64;
    key_kinds
        .iter()
        .zip(key_values.iter())
        .map(|(kind, value)| match kind {
            AssocKeyKind::Int => {
                let value = AssocKeyValue::Int(next_int_key);
                next_int_key += 1;
                value
            }
            AssocKeyKind::Str => value.clone(),
        })
        .collect()
}
