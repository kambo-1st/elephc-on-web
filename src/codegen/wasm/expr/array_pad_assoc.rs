//! Purpose:
//! Emits wasm32-web associative-array `array_pad()` lowering and metadata propagation.
//! Keeps associative copy/fill loops separate from indexed pad orchestration.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_pad` through its re-export.
//!
//! Key details:
//! - PHP `array_pad()` reindexes integer keys while preserving string keys from the source.
//! - Metadata helpers mirror the emitted runtime copy/fill order.

use super::*;

pub(super) fn emit_assoc_array_pad_assign(
    name: &str,
    source: &str,
    target_len: i64,
    target_abs: i32,
    pad_value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let len = module.array_length(source);
    let target_abs = usize::try_from(target_abs).map_err(|_| {
        CompileError::new(pad_value.span, "wasm32-web array_pad() length is out of range")
    })?;
    let out_len = len.map(|len| target_abs.max(len));
    let pad_count = out_len.zip(len).map(|(out_len, len)| out_len - len);
    let source_ptr = preserve_array_ptr(source, "assoc_array_pad", module);
    let source_key_kinds = module.array_key_kinds(source).map(|kinds| kinds.to_vec());
    let source_key_values = module.array_key_values(source).map(|values| values.to_vec());
    let source_value_kinds = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec());
    let source_runtime_value_kind = module.array_runtime_value_cell_kind(source);
    let source_value_constants = module.array_value_constants(source).map(|values| values.to_vec());
    let source_nested_values = module
        .array_nested_value_metadata_items(source)
        .map(|items| items.to_vec());
    if let Some(out_len) = out_len {
        module.set_array_length(name, out_len);
    } else {
        module.clear_array_length(name);
    }
    module.set_array_layout(name, ArrayLayout::Assoc);
    let pad_kind = value_cell_kind_for_expr(pad_value, module);
    module.set_array_key_kinds(
        name,
        pad_count.and_then(|pad_count| {
            assoc_array_pad_key_kinds(source_key_kinds.as_deref(), target_len, pad_count)
        }),
    );
    module.set_array_key_values(
        name,
        pad_count.and_then(|pad_count| {
            assoc_array_pad_key_values(
                source_key_kinds.as_deref(),
                source_key_values.as_deref(),
                target_len,
                pad_count,
            )
        }),
    );
    module.set_array_php_normalized_runtime_keys(
        name,
        module.array_key_kinds(name).is_none()
            && (module.array_has_php_normalized_runtime_keys(source)
                || module.array_runtime_key_kind(source).is_some()),
    );
    module.set_array_value_cell_kinds(
        name,
        pad_count.and_then(|pad_count| {
            assoc_array_pad_value_kinds(source_value_kinds, pad_kind, target_len, pad_count)
        }),
    );
    module.set_array_runtime_value_cell_kind(
        name,
        match (source_runtime_value_kind, pad_kind) {
            (Some(source_kind), Some(pad_kind)) if source_kind == pad_kind => Some(source_kind),
            (Some(source_kind), _) if len.is_some_and(|len| target_abs <= len) => Some(source_kind),
            _ => None,
        },
    );
    module.set_array_value_constants(
        name,
        pad_count.and_then(|pad_count| {
            assoc_array_pad_value_constants(
                source_value_constants,
                static_scalar_value(pad_value, module),
                target_len,
                pad_count,
            )
        }),
    );
    module.set_array_nested_value_metadata(
        name,
        pad_count.and_then(|pad_count| {
            assoc_array_pad_nested_values(
                source_nested_values,
                nested_array_metadata_for_expr(pad_value, module),
                target_len,
                pad_count,
            )
        }),
    );
    let source_len = module.next_label("assoc_array_pad_source_len");
    let out_len_local = module.next_label("assoc_array_pad_out_len");
    let pad_count_local = module.next_label("assoc_array_pad_count");
    for local in [&source_len, &out_len_local, &pad_count_local] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", source_len));
    module.body().line(&format!("i32.const {}", target_abs));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.lt_u");
    module.body().open("if (result i32)");
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("else");
    module.body().line(&format!("i32.const {}", target_abs));
    module.body().close("end");
    module.body().line(&format!("local.set {}", out_len_local));
    module.body().line(&format!("local.get {}", out_len_local));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", pad_count_local));
    module.body().line(&format!("local.get {}", out_len_local));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_len_local));
    module.body().line(&format!("local.set ${}_len", name));
    let next_key = module.next_label("assoc_array_pad_next_key");
    let source_entry = module.next_label("assoc_array_pad_source_entry");
    let target_entry = module.next_label("assoc_array_pad_target_entry");
    let target_cell = module.next_label("assoc_array_pad_target_cell");
    let index = module.next_label("assoc_array_pad_index");
    let target_index = module.next_label("assoc_array_pad_target_index");
    module.declare_i64_local(next_key.trim_start_matches('$').to_string());
    for local in [&source_entry, &target_entry, &target_cell, &index, &target_index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i64.const 0");
    module.body().line(&format!("local.set {}", next_key));
    if target_len < 0 {
        emit_assoc_array_pad_values_loop(name, &index, "i32.const 0", &pad_count_local, &next_key, &target_entry, &target_cell, pad_value, module)?;
        module.body().line(&format!("local.get {}", pad_count_local));
        module.body().line("i64.extend_i32_u");
        module.body().line(&format!("local.set {}", next_key));
        emit_assoc_array_pad_copy_source_loop(
            name,
            &source_ptr,
            &source_len,
            &index,
            &target_index,
            Some(&pad_count_local),
            &next_key,
            &source_entry,
            &target_entry,
            module,
        );
    } else {
        emit_assoc_array_pad_copy_source_loop(
            name,
            &source_ptr,
            &source_len,
            &index,
            &target_index,
            None,
            &next_key,
            &source_entry,
            &target_entry,
            module,
        );
        emit_assoc_array_pad_values_loop(
            name,
            &index,
            &format!("local.get {}", source_len),
            &out_len_local,
            &next_key,
            &target_entry,
            &target_cell,
            pad_value,
            module,
        )?;
    }
    Ok(())
}

pub(super) fn emit_assoc_array_pad_copy_source_loop(
    name: &str,
    source_ptr: &str,
    source_len: &str,
    index: &str,
    target_index: &str,
    target_offset: Option<&str>,
    next_key: &str,
    source_entry: &str,
    target_entry: &str,
    module: &mut WasmModule,
) {
    let done_label = module.next_label("assoc_array_pad_copy_done");
    let loop_label = module.next_label("assoc_array_pad_copy_loop");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", source_entry));
    module.body().line(&format!("local.get ${}_ptr", name));
    if let Some(target_offset) = target_offset {
        module.body().line(&format!("local.get {}", target_offset));
        module.body().line(&format!("local.get {}", index));
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", target_index));
    } else {
        module.body().line(&format!("local.get {}", index));
        module.body().line(&format!("local.set {}", target_index));
    }
    module.body().line(&format!("local.get {}", target_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", next_key));
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", next_key));
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", next_key));
    module.body().line("else");
    copy_assoc_entry_key(target_entry, source_entry, module);
    module.body().close("end");
    copy_assoc_entry_value(target_entry, source_entry, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_assoc_array_pad_values_loop(
    name: &str,
    index: &str,
    start_expr: &str,
    end: &str,
    next_key: &str,
    target_entry: &str,
    target_cell: &str,
    pad_value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let done_label = module.next_label("assoc_array_pad_values_done");
    let loop_label = module.next_label("assoc_array_pad_values_loop");
    module.body().line(start_expr);
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("local.get {}", next_key));
    module.body().line("call $__rt_assoc_store_int_key");
    module.body().line(&format!("local.get {}", next_key));
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", next_key));
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", target_cell));
    emit_store_value_cell(target_cell, pad_value, module)?;
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(super) fn assoc_array_pad_key_kinds(
    source_key_kinds: Option<&[AssocKeyKind]>,
    target_len: i64,
    pad_count: usize,
) -> Option<Vec<AssocKeyKind>> {
    let source_key_kinds = source_key_kinds?;
    let mut out = Vec::with_capacity(source_key_kinds.len() + pad_count);
    if target_len < 0 {
        out.extend(std::iter::repeat(AssocKeyKind::Int).take(pad_count));
    }
    out.extend(reindexed_assoc_key_kinds(source_key_kinds));
    if target_len >= 0 {
        out.extend(std::iter::repeat(AssocKeyKind::Int).take(pad_count));
    }
    Some(out)
}

pub(super) fn assoc_array_pad_key_values(
    source_key_kinds: Option<&[AssocKeyKind]>,
    source_key_values: Option<&[AssocKeyValue]>,
    target_len: i64,
    pad_count: usize,
) -> Option<Vec<AssocKeyValue>> {
    let source_key_kinds = source_key_kinds?;
    let source_key_values = source_key_values?;
    let mut out = Vec::with_capacity(source_key_kinds.len() + pad_count);
    let mut next_int_key = 0i64;
    if target_len < 0 {
        for _ in 0..pad_count {
            out.push(AssocKeyValue::Int(next_int_key));
            next_int_key += 1;
        }
    }
    for (kind, value) in source_key_kinds.iter().zip(source_key_values.iter()) {
        push_reindexed_assoc_key_value(&mut out, *kind, value, &mut next_int_key);
    }
    if target_len >= 0 {
        for _ in 0..pad_count {
            out.push(AssocKeyValue::Int(next_int_key));
            next_int_key += 1;
        }
    }
    Some(out)
}

pub(super) fn assoc_array_pad_value_kinds(
    source_value_kinds: Option<Vec<ValueCellKind>>,
    pad_kind: Option<ValueCellKind>,
    target_len: i64,
    pad_count: usize,
) -> Option<Vec<ValueCellKind>> {
    let source_value_kinds = source_value_kinds?;
    let pad_kind = pad_kind?;
    let mut out = Vec::with_capacity(source_value_kinds.len() + pad_count);
    if target_len < 0 {
        out.extend(std::iter::repeat(pad_kind).take(pad_count));
    }
    out.extend(source_value_kinds);
    if target_len >= 0 {
        out.extend(std::iter::repeat(pad_kind).take(pad_count));
    }
    Some(out)
}

pub(super) fn assoc_array_pad_value_constants(
    source_values: Option<Vec<ConstantValue>>,
    pad_value: Option<ConstantValue>,
    target_len: i64,
    pad_count: usize,
) -> Option<Vec<ConstantValue>> {
    let source_values = source_values?;
    let pad_value = pad_value?;
    let mut out = Vec::with_capacity(source_values.len() + pad_count);
    if target_len < 0 {
        out.extend(std::iter::repeat(pad_value.clone()).take(pad_count));
    }
    out.extend(source_values);
    if target_len >= 0 {
        out.extend(std::iter::repeat(pad_value).take(pad_count));
    }
    Some(out)
}

pub(super) fn assoc_array_pad_nested_values(
    source_values: Option<Vec<Option<NestedArrayMetadata>>>,
    pad_value: Option<NestedArrayMetadata>,
    target_len: i64,
    pad_count: usize,
) -> Option<Vec<Option<NestedArrayMetadata>>> {
    let source_values = source_values?;
    let mut out = Vec::with_capacity(source_values.len() + pad_count);
    if target_len < 0 {
        out.extend(std::iter::repeat(pad_value.clone()).take(pad_count));
    }
    out.extend(source_values);
    if target_len >= 0 {
        out.extend(std::iter::repeat(pad_value).take(pad_count));
    }
    Some(out)
}
