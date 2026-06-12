//! Purpose:
//! Emits wasm32-web associative-array `array_splice()` lowering.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_splice`.
//!
//! Key details:
//! - Reindexes removed integer keys like PHP while preserving string keys.
//! - Copies value cells through runtime helpers so nested ownership stays balanced.

use super::*;

pub(super) fn emit_assoc_array_splice_assign(
    name: &str,
    source: &str,
    len: usize,
    start: usize,
    take: usize,
    replacement_items: Option<&[Expr]>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let source_ptr = preserve_array_ptr(source, "assoc_array_splice", module);
    let value_kinds = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec());
    let value_constants = module.array_value_constants(source).map(|values| values.to_vec());
    let key_kinds = module
        .array_key_kinds(source)
        .expect("associative array splice key kinds were validated")
        .to_vec();
    let key_values = module.array_key_values(source).map(|values| values.to_vec());
    let removed_value_kinds = value_kinds
        .as_ref()
        .map(|kinds| kinds[start..start + take].to_vec());
    let removed_value_constants = value_constants
        .as_ref()
        .map(|values| values[start..start + take].to_vec());
    let removed_key_kinds = assoc_splice_reindexed_key_kinds(&key_kinds[start..start + take]);
    let removed_key_values = key_values
        .as_ref()
        .map(|values| reindexed_assoc_key_values(&key_kinds[start..start + take], &values[start..start + take]));
    emit_assoc_array_splice_removed_assign(
        name,
        &source_ptr,
        start,
        take,
        removed_key_kinds,
        removed_key_values,
        removed_value_kinds,
        removed_value_constants,
        &key_kinds,
        module,
    );
    if name == source {
        return Ok(());
    }
    let remaining_key_kinds =
        assoc_splice_remaining_key_kinds(&key_kinds, start, take, replacement_items.map_or(0, <[Expr]>::len));
    let remaining_key_values =
        assoc_splice_remaining_key_values(&key_kinds, key_values.as_deref(), start, take, replacement_items);
    let remaining_value_kinds = assoc_splice_remaining_value_kinds(
        value_kinds.as_deref(),
        start,
        take,
        replacement_items,
        module,
    );
    let remaining_value_constants = splice_remaining_value_constants(
        value_constants.as_deref(),
        start,
        take,
        replacement_items,
        module,
    );
    emit_assoc_array_splice_source_remainder(
        source,
        &source_ptr,
        len,
        start,
        take,
        remaining_key_kinds,
        remaining_key_values,
        remaining_value_kinds,
        remaining_value_constants,
        &key_kinds,
        replacement_items,
        module,
    )?;
    Ok(())
}

pub(super) fn emit_assoc_array_splice_runtime_replacement_assign(
    name: &str,
    source: &str,
    len: usize,
    start: usize,
    take: usize,
    replacement: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let source_ptr = preserve_array_ptr(source, "assoc_array_splice_runtime_replacement", module);
    let replacement_ptr = preserve_array_ptr(replacement, "assoc_array_splice_runtime_replacement", module);
    let key_kinds = module
        .array_key_kinds(source)
        .expect("associative array splice key kinds were validated")
        .to_vec();
    let key_values = module.array_key_values(source).map(|values| values.to_vec());
    let value_kinds = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec());
    let value_constants = module.array_value_constants(source).map(|values| values.to_vec());
    let removed_value_kinds = value_kinds
        .as_ref()
        .map(|kinds| kinds[start..start + take].to_vec());
    let removed_value_constants = value_constants
        .as_ref()
        .map(|values| values[start..start + take].to_vec());
    let removed_key_kinds = assoc_splice_reindexed_key_kinds(&key_kinds[start..start + take]);
    let removed_key_values = key_values
        .as_ref()
        .map(|values| reindexed_assoc_key_values(&key_kinds[start..start + take], &values[start..start + take]));
    emit_assoc_array_splice_removed_assign(
        name,
        &source_ptr,
        start,
        take,
        removed_key_kinds,
        removed_key_values,
        removed_value_kinds,
        removed_value_constants,
        &key_kinds,
        module,
    );
    emit_assoc_array_splice_runtime_replacement_source_remainder(
        source,
        &source_ptr,
        &replacement_ptr,
        len,
        start,
        take,
        replacement,
        &key_kinds,
        module,
    )
}

pub(super) fn emit_assoc_array_splice_local_replacement_assign(
    name: &str,
    source: &str,
    len: usize,
    start: usize,
    take: usize,
    replacement: &str,
    replacement_len: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let source_ptr = preserve_array_ptr(source, "assoc_array_splice_local_replacement", module);
    let replacement_ptr = preserve_array_ptr(replacement, "assoc_array_splice_local_replacement", module);
    let key_kinds = module
        .array_key_kinds(source)
        .expect("associative array splice key kinds were validated")
        .to_vec();
    let key_values = module.array_key_values(source).map(|values| values.to_vec());
    let value_kinds = module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec());
    let value_constants = module.array_value_constants(source).map(|values| values.to_vec());
    let removed_value_kinds = value_kinds
        .as_ref()
        .map(|kinds| kinds[start..start + take].to_vec());
    let removed_value_constants = value_constants
        .as_ref()
        .map(|values| values[start..start + take].to_vec());
    let removed_key_kinds = assoc_splice_reindexed_key_kinds(&key_kinds[start..start + take]);
    let removed_key_values = key_values
        .as_ref()
        .map(|values| reindexed_assoc_key_values(&key_kinds[start..start + take], &values[start..start + take]));
    let remaining_key_kinds = assoc_splice_remaining_key_kinds(&key_kinds, start, take, replacement_len);
    let remaining_key_values =
        assoc_splice_local_replacement_key_values(&key_kinds, key_values.as_deref(), start, take, replacement_len);
    let remaining_value_kinds = assoc_splice_local_replacement_value_kinds(
        value_kinds.as_deref(),
        replacement,
        start,
        take,
        replacement_len,
        module,
    );
    let remaining_value_constants = assoc_splice_local_replacement_value_constants(
        value_constants.as_deref(),
        replacement,
        start,
        take,
        module,
    );
    emit_assoc_array_splice_removed_assign(
        name,
        &source_ptr,
        start,
        take,
        removed_key_kinds,
        removed_key_values,
        removed_value_kinds,
        removed_value_constants,
        &key_kinds,
        module,
    );
    emit_assoc_array_splice_local_replacement_source_remainder(
        source,
        &source_ptr,
        &replacement_ptr,
        len,
        start,
        take,
        replacement,
        replacement_len,
        remaining_key_kinds,
        remaining_key_values,
        remaining_value_kinds,
        remaining_value_constants,
        &key_kinds,
        module,
    )
}

pub(super) fn emit_assoc_array_splice_removed_assign(
    name: &str,
    source_ptr: &str,
    start: usize,
    take: usize,
    key_kinds: Vec<AssocKeyKind>,
    key_values: Option<Vec<AssocKeyValue>>,
    value_kinds: Option<Vec<ValueCellKind>>,
    value_constants: Option<Vec<ConstantValue>>,
    source_key_kinds: &[AssocKeyKind],
    module: &mut WasmModule,
) {
    module.body().line(&format!("i32.const {}", take));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", take));
    module.body().line(&format!("local.set ${}_len", name));
    module.set_array_length(name, take);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_key_kinds(name, Some(key_kinds));
    module.set_array_key_values(name, key_values);
    module.set_array_value_cell_kinds(name, value_kinds);
    module.set_array_value_constants(name, value_constants);
    let mut next_int_key = 0i64;
    for index in 0..take {
        emit_assoc_array_splice_copy_entry(
            name,
            index,
            source_ptr,
            start + index,
            source_key_kinds[start + index],
            &mut next_int_key,
            module,
        );
    }
}

pub(super) fn emit_assoc_array_splice_source_remainder(
    source: &str,
    source_ptr: &str,
    len: usize,
    start: usize,
    take: usize,
    key_kinds: Vec<AssocKeyKind>,
    key_values: Option<Vec<AssocKeyValue>>,
    value_kinds: Option<Vec<ValueCellKind>>,
    value_constants: Option<Vec<ConstantValue>>,
    source_key_kinds: &[AssocKeyKind],
    replacement_items: Option<&[Expr]>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let replacement_len = replacement_items.map_or(0, <[Expr]>::len);
    let remaining_len = len - take + replacement_len;
    module.body().line(&format!("i32.const {}", remaining_len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", source));
    module.body().line(&format!("i32.const {}", remaining_len));
    module.body().line(&format!("local.set ${}_len", source));
    module.set_array_length(source, remaining_len);
    module.set_array_layout(source, ArrayLayout::Assoc);
    module.set_array_key_kinds(source, Some(key_kinds));
    module.set_array_key_values(source, key_values);
    module.set_array_value_cell_kinds(source, value_kinds);
    module.set_array_value_constants(source, value_constants);
    let mut out_index = 0usize;
    let mut next_int_key = 0i64;
    for source_index in 0..start {
        emit_assoc_array_splice_copy_entry(
            source,
            out_index,
            source_ptr,
            source_index,
            source_key_kinds[source_index],
            &mut next_int_key,
            module,
        );
        out_index += 1;
    }
    if let Some(items) = replacement_items {
        for item in items {
            emit_assoc_array_splice_store_replacement(source, out_index, item, &mut next_int_key, module)?;
            out_index += 1;
        }
    }
    for source_index in start + take..len {
        emit_assoc_array_splice_copy_entry(
            source,
            out_index,
            source_ptr,
            source_index,
            source_key_kinds[source_index],
            &mut next_int_key,
            module,
        );
        out_index += 1;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_assoc_array_splice_local_replacement_source_remainder(
    source: &str,
    source_ptr: &str,
    replacement_ptr: &str,
    len: usize,
    start: usize,
    take: usize,
    replacement: &str,
    replacement_len: usize,
    key_kinds: Vec<AssocKeyKind>,
    key_values: Option<Vec<AssocKeyValue>>,
    value_kinds: Option<Vec<ValueCellKind>>,
    value_constants: Option<Vec<ConstantValue>>,
    source_key_kinds: &[AssocKeyKind],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let remaining_len = len - take + replacement_len;
    module.body().line(&format!("i32.const {}", remaining_len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", source));
    module.body().line(&format!("i32.const {}", remaining_len));
    module.body().line(&format!("local.set ${}_len", source));
    module.set_array_length(source, remaining_len);
    module.set_array_layout(source, ArrayLayout::Assoc);
    module.set_array_key_kinds(source, Some(key_kinds));
    module.set_array_key_values(source, key_values);
    module.set_array_value_cell_kinds(source, value_kinds);
    module.set_array_value_constants(source, value_constants);
    let mut out_index = 0usize;
    let mut next_int_key = 0i64;
    for source_index in 0..start {
        emit_assoc_array_splice_copy_entry(
            source,
            out_index,
            source_ptr,
            source_index,
            source_key_kinds[source_index],
            &mut next_int_key,
            module,
        );
        out_index += 1;
    }
    for replacement_index in 0..replacement_len {
        emit_assoc_array_splice_copy_static_replacement_entry(
            source,
            out_index,
            replacement,
            replacement_ptr,
            replacement_index,
            &mut next_int_key,
            module,
        )?;
        out_index += 1;
    }
    for source_index in start + take..len {
        emit_assoc_array_splice_copy_entry(
            source,
            out_index,
            source_ptr,
            source_index,
            source_key_kinds[source_index],
            &mut next_int_key,
            module,
        );
        out_index += 1;
    }
    Ok(())
}

fn emit_assoc_array_splice_runtime_replacement_source_remainder(
    source: &str,
    source_ptr: &str,
    replacement_ptr: &str,
    len: usize,
    start: usize,
    take: usize,
    replacement: &str,
    source_key_kinds: &[AssocKeyKind],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let replacement_len = module.next_label("assoc_splice_replacement_len");
    let remaining_len = module.next_label("assoc_splice_remaining_len");
    let replacement_index = module.next_label("assoc_splice_replacement_index");
    let suffix_int_base = module.next_label("assoc_splice_suffix_int_base");
    for local in [&replacement_len, &remaining_len, &replacement_index, &suffix_int_base] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_len", replacement));
    module.body().line(&format!("local.set {}", replacement_len));
    module.body().line(&format!("i32.const {}", len - take));
    module.body().line(&format!("local.get {}", replacement_len));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", remaining_len));
    module.body().line(&format!("local.get {}", remaining_len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", source));
    module.body().line(&format!("local.get {}", remaining_len));
    module.body().line(&format!("local.set ${}_len", source));
    module.clear_array_length(source);
    module.set_array_layout(source, ArrayLayout::Assoc);
    module.set_array_key_kinds(source, None);
    module.set_array_key_values(source, None);
    module.set_array_runtime_key_kind(source, None);
    module.set_array_value_cell_kinds(source, None);
    module.set_array_value_constants(source, None);
    module.set_array_runtime_value_cell_kind(source, None);
    let mut out_index = 0usize;
    let mut next_int_key = 0i64;
    for source_index in 0..start {
        emit_assoc_array_splice_copy_entry(
            source,
            out_index,
            source_ptr,
            source_index,
            source_key_kinds[source_index],
            &mut next_int_key,
            module,
        );
        out_index += 1;
    }
    emit_assoc_array_splice_runtime_replacement_loop(
        source,
        out_index,
        next_int_key,
        replacement,
        replacement_ptr,
        &replacement_len,
        &replacement_index,
        module,
    )?;
    module.body().line(&format!("i32.const {}", next_int_key));
    module.body().line(&format!("local.get {}", replacement_len));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", suffix_int_base));
    let mut suffix_int_offset = 0i64;
    for source_index in start + take..len {
        emit_assoc_array_splice_dynamic_copy_entry(
            source,
            source_index - take,
            &replacement_len,
            source_ptr,
            source_index,
            source_key_kinds[source_index],
            &suffix_int_base,
            &mut suffix_int_offset,
            module,
        );
    }
    Ok(())
}

fn emit_assoc_array_splice_copy_static_replacement_entry(
    target: &str,
    target_index: usize,
    replacement: &str,
    replacement_ptr: &str,
    replacement_index: usize,
    next_int_key: &mut i64,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let target_entry = module.next_label("assoc_splice_local_replacement_entry");
    module.declare_i32_local(target_entry.trim_start_matches('$').to_string());
    emit_assoc_entry_address_const_index(&format!("${}_ptr", target), target_index, &target_entry, module);
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.store");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("i64.const {}", *next_int_key));
    module.body().line("i64.store");
    *next_int_key += 1;
    emit_assoc_array_splice_copy_static_replacement_value(
        &target_entry,
        replacement,
        replacement_ptr,
        replacement_index,
        module,
    )
}

fn emit_assoc_array_splice_copy_static_replacement_value(
    target_entry: &str,
    replacement: &str,
    replacement_ptr: &str,
    replacement_index: usize,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("i32.const 16");
    module.body().line("i32.add");
    match module.array_layout(replacement) {
        ArrayLayout::CompactInt => {
            module.body().line(&format!("local.get {}", replacement_ptr));
            module.body().line(&format!("i32.const {}", replacement_index * 8));
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("call $__rt_value_store_int");
        }
        ArrayLayout::Value => {
            module.body().line(&format!("local.get {}", replacement_ptr));
            module.body().line(&format!("i32.const {}", replacement_index));
            module.body().line("call $__rt_value_cell");
            module.body().line("call $__rt_value_copy");
        }
        ArrayLayout::Assoc => {
            module.body().line(&format!("local.get {}", replacement_ptr));
            module
                .body()
                .line(&format!("i32.const {}", replacement_index * WASM_ASSOC_ENTRY_SIZE));
            module.body().line("i32.add");
            module.body().line("i32.const 16");
            module.body().line("i32.add");
            module.body().line("call $__rt_value_copy");
        }
    }
    Ok(())
}

fn emit_assoc_array_splice_runtime_replacement_loop(
    target: &str,
    target_start: usize,
    key_start: i64,
    replacement: &str,
    replacement_ptr: &str,
    replacement_len: &str,
    index: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let loop_label = module.next_label("assoc_splice_replacement_loop");
    let done_label = module.next_label("assoc_splice_replacement_done");
    let target_entry = module.next_label("assoc_splice_replacement_target_entry");
    module.declare_i32_local(target_entry.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", replacement_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_assoc_array_splice_dynamic_target_entry(target, target_start, index, &target_entry, module);
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.store");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("i64.const {}", key_start));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i64.extend_i32_u");
    module.body().line("i64.add");
    module.body().line("i64.store");
    emit_assoc_array_splice_copy_replacement_value(&target_entry, replacement, replacement_ptr, index, module)?;
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_assoc_array_splice_dynamic_copy_entry(
    target: &str,
    target_static_offset: usize,
    target_dynamic_offset: &str,
    source_ptr: &str,
    source_index: usize,
    source_key_kind: AssocKeyKind,
    suffix_int_base: &str,
    suffix_int_offset: &mut i64,
    module: &mut WasmModule,
) {
    let source_entry = module.next_label("assoc_splice_dynamic_source_entry");
    let target_entry = module.next_label("assoc_splice_dynamic_target_entry");
    for local in [&source_entry, &target_entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_assoc_entry_address_const_index(source_ptr, source_index, &source_entry, module);
    emit_assoc_array_splice_dynamic_target_entry(
        target,
        target_static_offset,
        target_dynamic_offset,
        &target_entry,
        module,
    );
    if source_key_kind == AssocKeyKind::Int {
        module.body().line(&format!("local.get {}", target_entry));
        module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
        module.body().line("i32.store");
        module.body().line(&format!("local.get {}", target_entry));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line(&format!("local.get {}", suffix_int_base));
        module.body().line("i64.extend_i32_u");
        module.body().line(&format!("i64.const {}", *suffix_int_offset));
        module.body().line("i64.add");
        module.body().line("i64.store");
        *suffix_int_offset += 1;
    } else {
        copy_assoc_entry_key(&target_entry, &source_entry, module);
    }
    copy_assoc_entry_value(&target_entry, &source_entry, module);
}

fn assoc_splice_local_replacement_key_values(
    key_kinds: &[AssocKeyKind],
    key_values: Option<&[AssocKeyValue]>,
    start: usize,
    take: usize,
    replacement_len: usize,
) -> Option<Vec<AssocKeyValue>> {
    let key_values = key_values?;
    let mut out = Vec::with_capacity(key_kinds.len() - take + replacement_len);
    let mut next_int_key = 0i64;
    for index in 0..start {
        push_reindexed_assoc_key_value(&mut out, key_kinds[index], &key_values[index], &mut next_int_key);
    }
    for _ in 0..replacement_len {
        out.push(AssocKeyValue::Int(next_int_key));
        next_int_key += 1;
    }
    for index in start + take..key_kinds.len() {
        push_reindexed_assoc_key_value(&mut out, key_kinds[index], &key_values[index], &mut next_int_key);
    }
    Some(out)
}

fn assoc_splice_local_replacement_value_kinds(
    source_kinds: Option<&[ValueCellKind]>,
    replacement: &str,
    start: usize,
    take: usize,
    replacement_len: usize,
    module: &WasmModule,
) -> Option<Vec<ValueCellKind>> {
    let source_kinds = source_kinds?;
    let mut out = Vec::with_capacity(source_kinds.len() - take + replacement_len);
    out.extend_from_slice(&source_kinds[..start]);
    match module.array_layout(replacement) {
        ArrayLayout::CompactInt => out.extend(std::iter::repeat_n(ValueCellKind::Int, replacement_len)),
        ArrayLayout::Value | ArrayLayout::Assoc => {
            out.extend_from_slice(module.array_value_cell_kinds(replacement)?);
        }
    }
    out.extend_from_slice(&source_kinds[start + take..]);
    Some(out)
}

fn assoc_splice_local_replacement_value_constants(
    source_values: Option<&[ConstantValue]>,
    replacement: &str,
    start: usize,
    take: usize,
    module: &WasmModule,
) -> Option<Vec<ConstantValue>> {
    let source_values = source_values?;
    let replacement_values = module.array_value_constants(replacement)?;
    let mut out = Vec::with_capacity(source_values.len() - take + replacement_values.len());
    out.extend_from_slice(&source_values[..start]);
    out.extend_from_slice(replacement_values);
    out.extend_from_slice(&source_values[start + take..]);
    Some(out)
}

fn emit_assoc_array_splice_dynamic_target_entry(
    target: &str,
    target_static_offset: usize,
    target_dynamic_offset: &str,
    target_entry: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", target));
    module.body().line(&format!("i32.const {}", target_static_offset));
    module.body().line(&format!("local.get {}", target_dynamic_offset));
    module.body().line("i32.add");
    module
        .body()
        .line(&format!("i32.const {}", WASM_ASSOC_ENTRY_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", target_entry));
}

fn emit_assoc_array_splice_copy_replacement_value(
    target_entry: &str,
    replacement: &str,
    replacement_ptr: &str,
    index: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("i32.const 16");
    module.body().line("i32.add");
    match module.array_layout(replacement) {
        ArrayLayout::CompactInt => {
            module.body().line(&format!("local.get {}", replacement_ptr));
            module.body().line(&format!("local.get {}", index));
            module.body().line("i32.const 8");
            module.body().line("i32.mul");
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("call $__rt_value_store_int");
        }
        ArrayLayout::Value => {
            module.body().line(&format!("local.get {}", replacement_ptr));
            module.body().line(&format!("local.get {}", index));
            module.body().line("call $__rt_value_cell");
            module.body().line("call $__rt_value_copy");
        }
        ArrayLayout::Assoc => {
            module.body().line(&format!("local.get {}", replacement_ptr));
            module.body().line(&format!("local.get {}", index));
            module
                .body()
                .line(&format!("i32.const {}", WASM_ASSOC_ENTRY_SIZE));
            module.body().line("i32.mul");
            module.body().line("i32.add");
            module.body().line("i32.const 16");
            module.body().line("i32.add");
            module.body().line("call $__rt_value_copy");
        }
    }
    Ok(())
}

pub(super) fn emit_assoc_array_splice_store_replacement(
    target: &str,
    target_index: usize,
    value: &Expr,
    next_int_key: &mut i64,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let target_entry = module.next_label("assoc_splice_replacement_entry");
    module.declare_i32_local(target_entry.trim_start_matches('$').to_string());
    emit_assoc_entry_address_const_index(&format!("${}_ptr", target), target_index, &target_entry, module);
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.store");
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line(&format!("i64.const {}", *next_int_key));
    module.body().line("i64.store");
    *next_int_key += 1;
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("i32.const 16");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", target_entry));
    emit_store_value_cell(&target_entry, value, module)
}

pub(super) fn emit_assoc_array_splice_copy_entry(
    target: &str,
    target_index: usize,
    source_ptr: &str,
    source_index: usize,
    source_key_kind: AssocKeyKind,
    next_int_key: &mut i64,
    module: &mut WasmModule,
) {
    let source_entry = module.next_label("assoc_splice_source_entry");
    let target_entry = module.next_label("assoc_splice_target_entry");
    for local in [&source_entry, &target_entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    emit_assoc_entry_address_const_index(source_ptr, source_index, &source_entry, module);
    emit_assoc_entry_address_const_index(&format!("${}_ptr", target), target_index, &target_entry, module);
    if source_key_kind == AssocKeyKind::Int {
        module.body().line(&format!("local.get {}", target_entry));
        module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
        module.body().line("i32.store");
        module.body().line(&format!("local.get {}", target_entry));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line(&format!("i64.const {}", *next_int_key));
        module.body().line("i64.store");
        *next_int_key += 1;
    } else {
        copy_assoc_entry_key(&target_entry, &source_entry, module);
    }
    copy_assoc_entry_value(&target_entry, &source_entry, module);
}
