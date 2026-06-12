//! Purpose:
//! Emits wasm32-web runtime-length `array_slice()` lowering for assoc, indexed,
//! and value-cell arrays.
//!
//! Called from:
//! - `super::array_slice::emit_indexed_array_slice_assign()`.
//!
//! Key details:
//! - Dynamic paths compute PHP slice bounds in WAT and copy runtime cells/entries.
//! - Metadata is preserved when source array shape is known, otherwise runtime kind
//!   markers stay attached to the target array.

use super::*;
use super::array_transform_sets::emit_assoc_entry_address;

pub(super) fn emit_dynamic_assoc_array_slice_assign(
    name: &str,
    source: &str,
    span: crate::span::Span,
    offset: i64,
    length: Option<i64>,
    preserve_keys: bool,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let offset = i32::try_from(offset).map_err(|_| {
        CompileError::new(span, "wasm32-web array_slice() offset is out of range")
    })?;
    let length = length
        .map(|length| {
            i32::try_from(length)
                .map_err(|_| CompileError::new(span, "wasm32-web array_slice() length is out of range"))
        })
        .transpose()?;
    if module.array_runtime_key_kind(source).is_none()
        && !module.array_has_php_normalized_runtime_keys(source)
    {
        return Err(CompileError::new(
            span,
            "wasm32-web array_slice() on dynamic associative arrays requires runtime key metadata",
        ));
    }
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_runtime_key_kind(name, module.array_runtime_key_kind(source));
    module.set_array_php_normalized_runtime_keys(
        name,
        module.array_has_php_normalized_runtime_keys(source),
    );
    module.set_array_runtime_value_cell_kind(name, module.array_runtime_value_cell_kind(source));
    module.set_array_runtime_nested_value_metadata(name, module.array_runtime_nested_value_metadata(source));
    let source_ptr = module.next_label("assoc_array_slice_source_ptr");
    let source_len = module.next_label("assoc_array_slice_source_len");
    let start = module.next_label("assoc_array_slice_start");
    let end = module.next_label("assoc_array_slice_end");
    let take = module.next_label("assoc_array_slice_take");
    let index = module.next_label("assoc_array_slice_index");
    let source_index = module.next_label("assoc_array_slice_source_index");
    let target_entry = module.next_label("assoc_array_slice_target_entry");
    let source_entry = module.next_label("assoc_array_slice_source_entry");
    let next_int_key = module.next_label("assoc_array_slice_next_int_key");
    let done_label = module.next_label("assoc_array_slice_done");
    let loop_label = module.next_label("assoc_array_slice_loop");
    for local in [
        &source_ptr,
        &source_len,
        &start,
        &end,
        &take,
        &index,
        &source_index,
        &target_entry,
        &source_entry,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(next_int_key.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.set {}", source_ptr));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", source_len));
    emit_dynamic_slice_bounds(offset, length, &source_len, &start, &end, &take, module);
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", take));
    module.body().line(&format!("i32.const {}", WASM_ASSOC_ENTRY_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get {}", take));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().line("i64.const 0");
    module.body().line(&format!("local.set {}", next_int_key));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", take));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    emit_assoc_entry_address(&format!("${}_ptr", name), &index, &target_entry, module);
    emit_assoc_entry_address(&source_ptr, &source_index, &source_entry, module);
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    if preserve_keys {
        copy_assoc_entry_key(&target_entry, &source_entry, module);
    } else {
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.store");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", next_int_key));
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", next_int_key));
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", next_int_key));
    }
    module.body().line("else");
    copy_assoc_entry_key(&target_entry, &source_entry, module);
    module.body().close("end");
    copy_assoc_entry_value(&target_entry, &source_entry, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn emit_dynamic_slice_bounds(
    offset: i32,
    length: Option<i32>,
    source_len: &str,
    start: &str,
    end: &str,
    take: &str,
    module: &mut WasmModule,
) {
    if offset >= 0 {
        module.body().line(&format!("i32.const {}", offset));
        module.body().line(&format!("local.get {}", source_len));
        module.body().line("i32.gt_u");
        module.body().open("if (result i32)");
        module.body().line(&format!("local.get {}", source_len));
        module.body().line("else");
        module.body().line(&format!("i32.const {}", offset));
        module.body().close("end");
        module.body().line(&format!("local.set {}", start));
    } else {
        module.body().line(&format!("i32.const {}", offset.abs()));
        module.body().line(&format!("local.get {}", source_len));
        module.body().line("i32.gt_u");
        module.body().open("if (result i32)");
        module.body().line("i32.const 0");
        module.body().line("else");
        module.body().line(&format!("local.get {}", source_len));
        module.body().line(&format!("i32.const {}", offset.abs()));
        module.body().line("i32.sub");
        module.body().close("end");
        module.body().line(&format!("local.set {}", start));
    }
    match length {
        Some(length) if length >= 0 => {
            module.body().line(&format!("local.get {}", start));
            module.body().line(&format!("i32.const {}", length));
            module.body().line("i32.add");
            module.body().line(&format!("local.get {}", source_len));
            module.body().line("i32.gt_u");
            module.body().open("if (result i32)");
            module.body().line(&format!("local.get {}", source_len));
            module.body().line("else");
            module.body().line(&format!("local.get {}", start));
            module.body().line(&format!("i32.const {}", length));
            module.body().line("i32.add");
            module.body().close("end");
            module.body().line(&format!("local.set {}", end));
        }
        Some(length) => {
            module.body().line(&format!("i32.const {}", length.abs()));
            module.body().line(&format!("local.get {}", source_len));
            module.body().line("i32.gt_u");
            module.body().open("if (result i32)");
            module.body().line("i32.const 0");
            module.body().line("else");
            module.body().line(&format!("local.get {}", source_len));
            module.body().line(&format!("i32.const {}", length.abs()));
            module.body().line("i32.sub");
            module.body().close("end");
            module.body().line(&format!("local.set {}", end));
        }
        None => {
            module.body().line(&format!("local.get {}", source_len));
            module.body().line(&format!("local.set {}", end));
        }
    }
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line("i32.const 0");
    module.body().line("else");
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.sub");
    module.body().close("end");
    module.body().line(&format!("local.set {}", take));
}

pub(super) fn emit_dynamic_indexed_array_slice_assign(
    name: &str,
    source: &Expr,
    offset: i64,
    length: Option<i64>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let offset = i32::try_from(offset).map_err(|_| {
        CompileError::new(source.span, "wasm32-web array_slice() offset is out of range")
    })?;
    let length = length
        .map(|length| {
            i32::try_from(length).map_err(|_| {
                CompileError::new(source.span, "wasm32-web array_slice() length is out of range")
            })
        })
        .transpose()?;
    let source_ptr = module.next_label("array_slice_source_ptr");
    let source_len = module.next_label("array_slice_source_len");
    let start = module.next_label("array_slice_start");
    let end = module.next_label("array_slice_end");
    let take = module.next_label("array_slice_take");
    let index = module.next_label("array_slice_index");
    let source_index = module.next_label("array_slice_source_index");
    let done_label = module.next_label("array_slice_done");
    let loop_label = module.next_label("array_slice_loop");
    for local in [
        &source_ptr,
        &source_len,
        &start,
        &end,
        &take,
        &index,
        &source_index,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.set_array_layout(name, ArrayLayout::CompactInt);
    if let ExprKind::Variable(source_name) = &source.kind {
        if let Some(len) = module.array_length(source_name) {
            let (_, take) = static_array_slice_bounds(len, offset as i64, length.map(i64::from));
            module.set_array_length(name, take);
        }
    } else if let ExprKind::FunctionCall { name: function_name, .. } = &source.kind {
        if let Some(len) = module.function_array_return_length(function_name) {
            let (_, take) = static_array_slice_bounds(len, offset as i64, length.map(i64::from));
            module.set_array_length(name, take);
        }
    }
    emit_array_value_to_stack(source, module)?;
    module.body().line(&format!("local.set {}", source_len));
    module.body().line(&format!("local.set {}", source_ptr));
    if offset >= 0 {
        module.body().line(&format!("i32.const {}", offset));
        module.body().line(&format!("local.get {}", source_len));
        module.body().line("i32.gt_u");
        module.body().open("if (result i32)");
        module.body().line(&format!("local.get {}", source_len));
        module.body().line("else");
        module.body().line(&format!("i32.const {}", offset));
        module.body().close("end");
        module.body().line(&format!("local.set {}", start));
    } else {
        module.body().line(&format!("i32.const {}", offset.abs()));
        module.body().line(&format!("local.get {}", source_len));
        module.body().line("i32.gt_u");
        module.body().open("if (result i32)");
        module.body().line("i32.const 0");
        module.body().line("else");
        module.body().line(&format!("local.get {}", source_len));
        module.body().line(&format!("i32.const {}", offset.abs()));
        module.body().line("i32.sub");
        module.body().close("end");
        module.body().line(&format!("local.set {}", start));
    }
    match length {
        Some(length) if length >= 0 => {
            module.body().line(&format!("local.get {}", start));
            module.body().line(&format!("i32.const {}", length));
            module.body().line("i32.add");
            module.body().line(&format!("local.get {}", source_len));
            module.body().line("i32.gt_u");
            module.body().open("if (result i32)");
            module.body().line(&format!("local.get {}", source_len));
            module.body().line("else");
            module.body().line(&format!("local.get {}", start));
            module.body().line(&format!("i32.const {}", length));
            module.body().line("i32.add");
            module.body().close("end");
            module.body().line(&format!("local.set {}", end));
        }
        Some(length) => {
            module.body().line(&format!("i32.const {}", length.abs()));
            module.body().line(&format!("local.get {}", source_len));
            module.body().line("i32.gt_u");
            module.body().open("if (result i32)");
            module.body().line("i32.const 0");
            module.body().line("else");
            module.body().line(&format!("local.get {}", source_len));
            module.body().line(&format!("i32.const {}", length.abs()));
            module.body().line("i32.sub");
            module.body().close("end");
            module.body().line(&format!("local.set {}", end));
        }
        None => {
            module.body().line(&format!("local.get {}", source_len));
            module.body().line(&format!("local.set {}", end));
        }
    }
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line("i32.const 0");
    module.body().line("else");
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.sub");
    module.body().close("end");
    module.body().line(&format!("local.set {}", take));
    module.body().line(&format!("local.get {}", take));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", take));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", take));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", source_index));
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
    Ok(())
}

pub(super) fn emit_dynamic_value_array_slice_assign(
    name: &str,
    source: &Expr,
    offset: i64,
    length: Option<i64>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let offset = i32::try_from(offset).map_err(|_| {
        CompileError::new(source.span, "wasm32-web array_slice() offset is out of range")
    })?;
    let length = length
        .map(|length| {
            i32::try_from(length).map_err(|_| {
                CompileError::new(source.span, "wasm32-web array_slice() length is out of range")
            })
        })
        .transpose()?;
    let source_ptr = module.next_label("value_array_slice_source_ptr");
    let source_len = module.next_label("value_array_slice_source_len");
    let start = module.next_label("value_array_slice_start");
    let end = module.next_label("value_array_slice_end");
    let take = module.next_label("value_array_slice_take");
    let index = module.next_label("value_array_slice_index");
    let source_index = module.next_label("value_array_slice_source_index");
    let done_label = module.next_label("value_array_slice_done");
    let loop_label = module.next_label("value_array_slice_loop");
    for local in [
        &source_ptr,
        &source_len,
        &start,
        &end,
        &take,
        &index,
        &source_index,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.set_array_layout(name, ArrayLayout::Value);
    if let ExprKind::Variable(source_name) = &source.kind {
        module.set_array_runtime_value_cell_kind(name, module.array_runtime_value_cell_kind(source_name));
    }
    if let ExprKind::Variable(source_name) = &source.kind {
        if let Some(len) = module.array_length(source_name) {
            let (_, take) = static_array_slice_bounds(len, offset as i64, length.map(i64::from));
            module.set_array_length(name, take);
            let (start, take) = static_array_slice_bounds(len, offset as i64, length.map(i64::from));
            let sliced_value_constants = module
                .array_value_constants(source_name)
                .map(|values| values[start..start + take].to_vec());
            module.set_array_value_cell_kinds(
                name,
                module
                    .array_value_cell_kinds(source_name)
                    .map(|kinds| kinds[start..start + take].to_vec()),
            );
            module.set_array_value_constants(name, sliced_value_constants);
            module.set_array_nested_value_metadata(
                name,
                module
                    .array_nested_value_metadata_items(source_name)
                    .map(|items| items[start..start + take].to_vec()),
            );
            module.set_array_object_classes(
                name,
                module
                    .array_object_classes(source_name)
                    .map(|classes| classes[start..start + take].to_vec()),
            );
        }
    } else if let ExprKind::FunctionCall { name: function_name, .. } = &source.kind {
        if let Some(len) = module.function_array_return_length(function_name) {
            let (_, take) = static_array_slice_bounds(len, offset as i64, length.map(i64::from));
            module.set_array_length(name, take);
            let (start, take) = static_array_slice_bounds(len, offset as i64, length.map(i64::from));
            module.set_array_value_cell_kinds(
                name,
                module
                    .function_array_return_value_kinds(function_name)
                    .map(|kinds| kinds[start..start + take].to_vec()),
            );
            module.set_array_nested_value_metadata(
                name,
                module
                    .function_array_return_nested_values(function_name)
                    .map(|items| items[start..start + take].to_vec()),
            );
        }
    }
    emit_array_value_to_stack(source, module)?;
    module.body().line(&format!("local.set {}", source_len));
    module.body().line(&format!("local.set {}", source_ptr));
    if offset >= 0 {
        module.body().line(&format!("i32.const {}", offset));
        module.body().line(&format!("local.get {}", source_len));
        module.body().line("i32.gt_u");
        module.body().open("if (result i32)");
        module.body().line(&format!("local.get {}", source_len));
        module.body().line("else");
        module.body().line(&format!("i32.const {}", offset));
        module.body().close("end");
        module.body().line(&format!("local.set {}", start));
    } else {
        module.body().line(&format!("i32.const {}", offset.abs()));
        module.body().line(&format!("local.get {}", source_len));
        module.body().line("i32.gt_u");
        module.body().open("if (result i32)");
        module.body().line("i32.const 0");
        module.body().line("else");
        module.body().line(&format!("local.get {}", source_len));
        module.body().line(&format!("i32.const {}", offset.abs()));
        module.body().line("i32.sub");
        module.body().close("end");
        module.body().line(&format!("local.set {}", start));
    }
    match length {
        Some(length) if length >= 0 => {
            module.body().line(&format!("local.get {}", start));
            module.body().line(&format!("i32.const {}", length));
            module.body().line("i32.add");
            module.body().line(&format!("local.get {}", source_len));
            module.body().line("i32.gt_u");
            module.body().open("if (result i32)");
            module.body().line(&format!("local.get {}", source_len));
            module.body().line("else");
            module.body().line(&format!("local.get {}", start));
            module.body().line(&format!("i32.const {}", length));
            module.body().line("i32.add");
            module.body().close("end");
            module.body().line(&format!("local.set {}", end));
        }
        Some(length) => {
            module.body().line(&format!("i32.const {}", length.abs()));
            module.body().line(&format!("local.get {}", source_len));
            module.body().line("i32.gt_u");
            module.body().open("if (result i32)");
            module.body().line("i32.const 0");
            module.body().line("else");
            module.body().line(&format!("local.get {}", source_len));
            module.body().line(&format!("i32.const {}", length.abs()));
            module.body().line("i32.sub");
            module.body().close("end");
            module.body().line(&format!("local.set {}", end));
        }
        None => {
            module.body().line(&format!("local.get {}", source_len));
            module.body().line(&format!("local.set {}", end));
        }
    }
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line("i32.const 0");
    module.body().line("else");
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.sub");
    module.body().close("end");
    module.body().line(&format!("local.set {}", take));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", take));
    module
        .body()
        .line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get {}", take));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", take));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    emit_copy_value_cell(name, &source_ptr, &index, &source_index, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}
