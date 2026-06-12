//! Purpose:
//! Lowers wasm32-web array_splice helpers for compact, value-cell, and associative arrays.
//! Keeps removal, replacement, and reindexing metadata out of expr.rs.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` array assignment lowering.
//!
//! Key details:
//! - Helpers preserve PHP splice reindexing, static metadata, and value-cell ownership shape.

mod assoc;

use super::*;
use assoc::{
    emit_assoc_array_splice_assign, emit_assoc_array_splice_local_replacement_assign,
    emit_assoc_array_splice_runtime_replacement_assign,
};

pub(super) fn emit_indexed_array_splice_assign(
    name: &str,
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if !(2..=4).contains(&args.len()) {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_splice() expects two, three, or four arguments",
        ));
    }
    let ExprKind::Variable(source) = &args[0].kind else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_splice() currently requires an indexed array variable",
        ));
    };
    if module.local_kind(source) != Some(LocalKind::Array) {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_splice() currently supports indexed array variables only",
        ));
    }
    let Some(len) = module.array_length(source) else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_splice() requires a known indexed array length",
        ));
    };
    let Some(offset) = static_or_const_or_i64_local_value(&args[1], module) else {
        return Err(CompileError::new(
            args[1].span,
            "wasm32-web array_splice() requires a static integer offset",
        ));
    };
    let length = if let Some(length_arg) = args.get(2) {
        let Some(length) = static_or_const_or_i64_local_value(length_arg, module) else {
            return Err(CompileError::new(
                length_arg.span,
                "wasm32-web array_splice() requires a static integer length",
            ));
        };
        Some(length)
    } else {
        None
    };
    let (start, take) = static_array_slice_bounds(len, offset, length);
    if let Some(replacement) = args.get(3) {
        if let ExprKind::Variable(replacement_name) = &replacement.kind {
            if source == name && module.local_kind(replacement_name) == Some(LocalKind::Array) {
                return emit_indexed_array_splice_same_target_assign(name, len, start, take, module);
            }
            if module.array_layout(source) == ArrayLayout::CompactInt
                && module.array_layout(replacement_name) == ArrayLayout::CompactInt
                && module.local_kind(replacement_name) == Some(LocalKind::Array)
                && source != name
            {
                if let Some(replacement_len) = module.array_length(replacement_name) {
                    return emit_compact_array_splice_local_replacement_assign(
                        name,
                        source,
                        len,
                        start,
                        take,
                        replacement_name,
                        replacement_len,
                        module,
                    );
                }
                return emit_compact_array_splice_runtime_replacement_assign(
                    name,
                    source,
                    len,
                    start,
                    take,
                    replacement_name,
                    module,
                );
            }
            if module.array_layout(source) == ArrayLayout::Value
                && module.array_layout(replacement_name) == ArrayLayout::Value
                && module.local_kind(replacement_name) == Some(LocalKind::Array)
                && source != name
            {
                if let Some(replacement_len) = module.array_length(replacement_name) {
                    return emit_value_array_splice_local_replacement_assign(
                        name,
                        source,
                        len,
                        start,
                        take,
                        replacement_name,
                        replacement_len,
                        module,
                    );
                }
                return emit_value_array_splice_runtime_replacement_assign(
                    name,
                    source,
                    len,
                    start,
                    take,
                    replacement_name,
                    module,
                );
            }
            if module.array_layout(source) == ArrayLayout::Assoc
                && module.local_kind(replacement_name) == Some(LocalKind::Array)
                && matches!(
                    module.array_layout(replacement_name),
                    ArrayLayout::CompactInt | ArrayLayout::Value | ArrayLayout::Assoc
                )
                && source != name
            {
                if module.array_key_kinds(source).is_none() {
                    return Err(CompileError::new(
                        args[0].span,
                        "wasm32-web array_splice() on associative arrays requires statically-known key kinds",
                    ));
                }
                if let Some(replacement_len) = module.array_length(replacement_name) {
                    return emit_assoc_array_splice_local_replacement_assign(
                        name,
                        source,
                        len,
                        start,
                        take,
                        replacement_name,
                        replacement_len,
                        module,
                    );
                }
                return emit_assoc_array_splice_runtime_replacement_assign(
                    name,
                    source,
                    len,
                    start,
                    take,
                    replacement_name,
                    module,
                );
            }
        }
    }
    let replacement_items = if let Some(replacement) = args.get(3) {
        let ExprKind::ArrayLiteral(items) = &replacement.kind else {
            return Err(CompileError::new(
                replacement.span,
                "wasm32-web array_splice() replacement currently requires a static indexed array",
            ));
        };
        Some(items.as_slice())
    } else {
        None
    };
    if module.array_layout(source) == ArrayLayout::Assoc {
        if module.array_key_kinds(source).is_none() {
            return Err(CompileError::new(
                args[0].span,
                "wasm32-web array_splice() on associative arrays requires statically-known key kinds",
            ));
        }
        return emit_assoc_array_splice_assign(name, source, len, start, take, replacement_items, module);
    }
    if source == name {
        return emit_indexed_array_splice_same_target_assign(name, len, start, take, module);
    }
    if module.array_layout(source) == ArrayLayout::Value {
        return emit_value_array_splice_assign(name, source, len, start, take, replacement_items, module);
    }
    if let Some(items) = replacement_items {
        if array_literal_needs_value_cells(items) {
            return emit_compact_array_splice_promote_assign(
                name,
                source,
                len,
                start,
                take,
                items,
                module,
            );
        }
    }
    let source_ptr = preserve_array_ptr(source, "array_splice", module);
    emit_array_alloc_prelude(name, take, module);
    for index in 0..take {
        emit_array_store_load(name, index, &source_ptr, start + index, module);
    }
    let replacement_len = replacement_items.map_or(0, <[Expr]>::len);
    let remaining_len = len - take + replacement_len;
    emit_array_alloc_prelude(source, remaining_len, module);
    for index in 0..start {
        emit_array_store_load(source, index, &source_ptr, index, module);
    }
    if let Some(items) = replacement_items {
        for (index, item) in items.iter().enumerate() {
            emit_array_store_expr(source, start + index, item, module)?;
        }
    }
    for source_index in start + take..len {
        emit_array_store_load(
            source,
            source_index - take + replacement_len,
            &source_ptr,
            source_index,
            module,
        );
    }
    Ok(())
}

pub(super) fn emit_compact_array_splice_local_replacement_assign(
    name: &str,
    source: &str,
    len: usize,
    start: usize,
    take: usize,
    replacement: &str,
    replacement_len: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let source_ptr = preserve_array_ptr(source, "array_splice_local_replacement", module);
    let replacement_ptr = preserve_array_ptr(replacement, "array_splice_local_replacement", module);
    emit_array_alloc_prelude(name, take, module);
    for index in 0..take {
        emit_array_store_load(name, index, &source_ptr, start + index, module);
    }
    let remaining_len = len - take + replacement_len;
    emit_array_alloc_prelude(source, remaining_len, module);
    for index in 0..start {
        emit_array_store_load(source, index, &source_ptr, index, module);
    }
    for index in 0..replacement_len {
        emit_array_store_load(source, start + index, &replacement_ptr, index, module);
    }
    for source_index in start + take..len {
        emit_array_store_load(
            source,
            source_index - take + replacement_len,
            &source_ptr,
            source_index,
            module,
        );
    }
    Ok(())
}

pub(super) fn emit_compact_array_splice_runtime_replacement_assign(
    name: &str,
    source: &str,
    len: usize,
    start: usize,
    take: usize,
    replacement: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let source_ptr = preserve_array_ptr(source, "array_splice_runtime_replacement", module);
    let replacement_ptr = preserve_array_ptr(replacement, "array_splice_runtime_replacement", module);
    let replacement_len = module.next_label("array_splice_replacement_len");
    let remaining_len = module.next_label("array_splice_remaining_len");
    let index = module.next_label("array_splice_replacement_index");
    for local in [&replacement_len, &remaining_len, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_len", replacement));
    module.body().line(&format!("local.set {}", replacement_len));
    emit_array_alloc_prelude(name, take, module);
    for index in 0..take {
        emit_array_store_load(name, index, &source_ptr, start + index, module);
    }
    module.body().line(&format!("i32.const {}", len - take));
    module.body().line(&format!("local.get {}", replacement_len));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", remaining_len));
    module.body().line(&format!("local.get {}", remaining_len));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", source));
    module.body().line(&format!("local.get {}", remaining_len));
    module.body().line(&format!("local.set ${}_len", source));
    module.clear_array_length(source);
    module.set_array_layout(source, ArrayLayout::CompactInt);
    for index in 0..start {
        emit_array_store_load(source, index, &source_ptr, index, module);
    }
    emit_compact_array_splice_runtime_replacement_loop(
        source,
        start,
        &replacement_ptr,
        &replacement_len,
        &index,
        module,
    );
    for source_index in start + take..len {
        emit_compact_array_splice_dynamic_store_load(
            source,
            source_index - take,
            &replacement_len,
            &source_ptr,
            source_index,
            module,
        );
    }
    Ok(())
}

fn emit_compact_array_splice_runtime_replacement_loop(
    source: &str,
    start: usize,
    replacement_ptr: &str,
    replacement_len: &str,
    index: &str,
    module: &mut WasmModule,
) {
    let loop_label = module.next_label("array_splice_replacement_loop");
    let done_label = module.next_label("array_splice_replacement_done");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", replacement_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", start));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.add");
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", replacement_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_compact_array_splice_dynamic_store_load(
    target: &str,
    target_static_offset: usize,
    target_dynamic_offset: &str,
    source_ptr: &str,
    source_index: usize,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", target));
    module.body().line(&format!("i32.const {}", target_static_offset));
    module.body().line(&format!("local.get {}", target_dynamic_offset));
    module.body().line("i32.add");
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("i32.const {}", source_index * 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.store");
}

pub(super) fn emit_compact_array_splice_promote_assign(
    name: &str,
    source: &str,
    len: usize,
    start: usize,
    take: usize,
    replacement_items: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let source_ptr = preserve_array_ptr(source, "array_splice_promote", module);
    emit_array_alloc_prelude(name, take, module);
    for index in 0..take {
        emit_array_store_load(name, index, &source_ptr, start + index, module);
    }

    let replacement_len = replacement_items.len();
    let remaining_len = len - take + replacement_len;
    let replacement_span = replacement_items
        .first()
        .expect("compact array_splice() value promotion requires replacement items")
        .span;
    let mut value_kinds = Vec::with_capacity(remaining_len);
    value_kinds.extend(std::iter::repeat(ValueCellKind::Int).take(start));
    value_kinds.extend(value_cell_kinds_for_items(replacement_items, module).ok_or_else(|| {
        CompileError::new(
            replacement_span,
            "wasm32-web compact array_splice() replacement does not support this value type yet",
        )
    })?);
    value_kinds.extend(std::iter::repeat(ValueCellKind::Int).take(len - (start + take)));
    module.body().line(&format!("i32.const {}", remaining_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", source));
    module.body().line(&format!("i32.const {}", remaining_len));
    module.body().line(&format!("local.set ${}_len", source));
    module.set_array_length(source, remaining_len);
    module.set_array_layout(source, ArrayLayout::Value);
    module.set_array_value_cell_kinds(source, Some(value_kinds));
    for index in 0..start {
        emit_static_int_slot_as_value_cell(source, index, &source_ptr, index, module);
    }
    for (index, item) in replacement_items.iter().enumerate() {
        emit_value_array_store_expr(source, start + index, item, module)?;
    }
    for source_index in start + take..len {
        emit_static_int_slot_as_value_cell(
            source,
            source_index - take + replacement_len,
            &source_ptr,
            source_index,
            module,
        );
    }
    Ok(())
}

pub(super) fn emit_indexed_array_splice_same_target_assign(
    name: &str,
    len: usize,
    start: usize,
    take: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(name) == ArrayLayout::Assoc {
        return emit_assoc_array_splice_assign(name, name, len, start, take, None, module);
    }
    if module.array_layout(name) == ArrayLayout::Value {
        return emit_value_array_splice_same_target_assign(name, len, start, take, module);
    }
    let source_ptr = preserve_array_ptr(name, "array_splice_same_target", module);
    emit_array_alloc_prelude(name, take, module);
    for index in 0..take {
        emit_array_store_load(name, index, &source_ptr, start + index, module);
    }
    Ok(())
}

pub(super) fn emit_value_array_splice_same_target_assign(
    name: &str,
    _len: usize,
    start: usize,
    take: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let source_ptr = preserve_array_ptr(name, "value_array_splice_same_target", module);
    let value_kinds = module
        .array_value_cell_kinds(name)
        .map(|kinds| kinds[start..start + take].to_vec());
    let value_constants = module
        .array_value_constants(name)
        .map(|values| values[start..start + take].to_vec());
    module.body().line(&format!("i32.const {}", take));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", take));
    module.body().line(&format!("local.set ${}_len", name));
    module.set_array_length(name, take);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, value_kinds);
    module.set_array_value_constants(name, value_constants);
    for index in 0..take {
        emit_copy_static_value_cell(name, index, &source_ptr, start + index, module);
    }
    Ok(())
}

pub(super) fn emit_value_array_splice_assign(
    name: &str,
    source: &str,
    len: usize,
    start: usize,
    take: usize,
    replacement_items: Option<&[Expr]>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let source_ptr = preserve_array_ptr(source, "value_array_splice", module);
    let removed_value_kinds = module
        .array_value_cell_kinds(source)
        .map(|kinds| kinds[start..start + take].to_vec());
    let removed_value_constants = module
        .array_value_constants(source)
        .map(|values| values[start..start + take].to_vec());
    let remaining_value_kinds = value_splice_remaining_value_kinds(
        module.array_value_cell_kinds(source),
        start,
        take,
        replacement_items,
        module,
    );
    let remaining_value_constants = splice_remaining_value_constants(
        module.array_value_constants(source),
        start,
        take,
        replacement_items,
        module,
    );
    module.body().line(&format!("i32.const {}", take));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", take));
    module.body().line(&format!("local.set ${}_len", name));
    module.set_array_length(name, take);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, removed_value_kinds);
    module.set_array_value_constants(name, removed_value_constants);
    for index in 0..take {
        emit_copy_static_value_cell(name, index, &source_ptr, start + index, module);
    }

    let replacement_len = replacement_items.map_or(0, <[Expr]>::len);
    let remaining_len = len - take + replacement_len;
    module.body().line(&format!("i32.const {}", remaining_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", source));
    module.body().line(&format!("i32.const {}", remaining_len));
    module.body().line(&format!("local.set ${}_len", source));
    module.set_array_length(source, remaining_len);
    module.set_array_layout(source, ArrayLayout::Value);
    module.set_array_value_cell_kinds(source, remaining_value_kinds);
    module.set_array_value_constants(source, remaining_value_constants);
    for index in 0..start {
        emit_copy_static_value_cell(source, index, &source_ptr, index, module);
    }
    if let Some(items) = replacement_items {
        for (index, item) in items.iter().enumerate() {
            emit_value_array_store_expr(source, start + index, item, module)?;
        }
    }
    for source_index in start + take..len {
        emit_copy_static_value_cell(
            source,
            source_index - take + replacement_len,
            &source_ptr,
            source_index,
            module,
        );
    }
    Ok(())
}

pub(super) fn emit_value_array_splice_local_replacement_assign(
    name: &str,
    source: &str,
    len: usize,
    start: usize,
    take: usize,
    replacement: &str,
    replacement_len: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let source_ptr = preserve_array_ptr(source, "value_array_splice_local_replacement", module);
    let replacement_ptr = preserve_array_ptr(replacement, "value_array_splice_local_replacement", module);
    let removed_value_kinds = module
        .array_value_cell_kinds(source)
        .map(|kinds| kinds[start..start + take].to_vec());
    let removed_value_constants = module
        .array_value_constants(source)
        .map(|values| values[start..start + take].to_vec());
    let remaining_value_kinds = value_splice_local_replacement_value_kinds(
        module.array_value_cell_kinds(source),
        module.array_value_cell_kinds(replacement),
        start,
        take,
    );
    let remaining_value_constants = value_splice_local_replacement_value_constants(
        module.array_value_constants(source),
        module.array_value_constants(replacement),
        start,
        take,
    );
    module.body().line(&format!("i32.const {}", take));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", take));
    module.body().line(&format!("local.set ${}_len", name));
    module.set_array_length(name, take);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, removed_value_kinds);
    module.set_array_value_constants(name, removed_value_constants);
    for index in 0..take {
        emit_copy_static_value_cell(name, index, &source_ptr, start + index, module);
    }

    let remaining_len = len - take + replacement_len;
    module.body().line(&format!("i32.const {}", remaining_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", source));
    module.body().line(&format!("i32.const {}", remaining_len));
    module.body().line(&format!("local.set ${}_len", source));
    module.set_array_length(source, remaining_len);
    module.set_array_layout(source, ArrayLayout::Value);
    module.set_array_value_cell_kinds(source, remaining_value_kinds);
    module.set_array_value_constants(source, remaining_value_constants);
    for index in 0..start {
        emit_copy_static_value_cell(source, index, &source_ptr, index, module);
    }
    for index in 0..replacement_len {
        emit_copy_static_value_cell(source, start + index, &replacement_ptr, index, module);
    }
    for source_index in start + take..len {
        emit_copy_static_value_cell(
            source,
            source_index - take + replacement_len,
            &source_ptr,
            source_index,
            module,
        );
    }
    Ok(())
}

pub(super) fn emit_value_array_splice_runtime_replacement_assign(
    name: &str,
    source: &str,
    len: usize,
    start: usize,
    take: usize,
    replacement: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let source_ptr = preserve_array_ptr(source, "value_array_splice_runtime_replacement", module);
    let replacement_ptr = preserve_array_ptr(replacement, "value_array_splice_runtime_replacement", module);
    let replacement_len = module.next_label("value_array_splice_replacement_len");
    let remaining_len = module.next_label("value_array_splice_remaining_len");
    let index = module.next_label("value_array_splice_replacement_index");
    for local in [&replacement_len, &remaining_len, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    let removed_value_kinds = module
        .array_value_cell_kinds(source)
        .map(|kinds| kinds[start..start + take].to_vec());
    let removed_value_constants = module
        .array_value_constants(source)
        .map(|values| values[start..start + take].to_vec());
    module.body().line(&format!("local.get ${}_len", replacement));
    module.body().line(&format!("local.set {}", replacement_len));
    module.body().line(&format!("i32.const {}", take));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", take));
    module.body().line(&format!("local.set ${}_len", name));
    module.set_array_length(name, take);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, removed_value_kinds);
    module.set_array_value_constants(name, removed_value_constants);
    for index in 0..take {
        emit_copy_static_value_cell(name, index, &source_ptr, start + index, module);
    }

    module.body().line(&format!("i32.const {}", len - take));
    module.body().line(&format!("local.get {}", replacement_len));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", remaining_len));
    module.body().line(&format!("local.get {}", remaining_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", source));
    module.body().line(&format!("local.get {}", remaining_len));
    module.body().line(&format!("local.set ${}_len", source));
    module.clear_array_length(source);
    module.set_array_layout(source, ArrayLayout::Value);
    module.set_array_value_cell_kinds(source, None);
    module.set_array_value_constants(source, None);
    module.set_array_runtime_value_cell_kind(source, None);
    for index in 0..start {
        emit_copy_static_value_cell(source, index, &source_ptr, index, module);
    }
    emit_value_array_splice_runtime_replacement_loop(
        source,
        start,
        &replacement_ptr,
        &replacement_len,
        &index,
        module,
    );
    for source_index in start + take..len {
        emit_value_array_splice_dynamic_copy(
            source,
            source_index - take,
            &replacement_len,
            &source_ptr,
            source_index,
            module,
        );
    }
    Ok(())
}

fn emit_value_array_splice_runtime_replacement_loop(
    source: &str,
    start: usize,
    replacement_ptr: &str,
    replacement_len: &str,
    index: &str,
    module: &mut WasmModule,
) {
    let loop_label = module.next_label("value_array_splice_replacement_loop");
    let done_label = module.next_label("value_array_splice_replacement_done");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", replacement_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", start));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.add");
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.get {}", replacement_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_cell");
    module.body().line("call $__rt_value_copy");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_value_array_splice_dynamic_copy(
    target: &str,
    target_static_offset: usize,
    target_dynamic_offset: &str,
    source_ptr: &str,
    source_index: usize,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", target));
    module.body().line(&format!("i32.const {}", target_static_offset));
    module.body().line(&format!("local.get {}", target_dynamic_offset));
    module.body().line("i32.add");
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("i32.const {}", source_index));
    module.body().line("call $__rt_value_cell");
    module.body().line("call $__rt_value_copy");
}
