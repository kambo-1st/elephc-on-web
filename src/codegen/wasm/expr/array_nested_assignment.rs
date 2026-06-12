//! Purpose:
//! Lowers nested PHP array assignment cases for the wasm32-web backend.
//! Keeps dynamic associative traversal and metadata updates out of the expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::emit_nested_array_assign()`
//!
//! Key details:
//! - Mutating nested associative arrays must copy payloads before writes so wasm COW metadata stays coherent.

use super::*;

pub(crate) fn emit_nested_array_assign(
    target: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if emit_static_property_nested_array_assign(target, value, module)? {
        return Ok(());
    }
    if emit_four_level_static_nested_assoc_assign(target, value, module)? {
        return Ok(());
    }
    if emit_deeper_dynamic_nested_assoc_assign(target, value, module)? {
        return Ok(());
    }
    let ExprKind::ArrayAccess {
        array: outer_access,
        index: inner_index,
    } = &target.kind
    else {
        return Err(nested_array_access_unsupported(target));
    };
    let ExprKind::ArrayAccess {
        array: outer_array,
        index: outer_index,
    } = &outer_access.kind
    else {
        return Err(nested_array_access_unsupported(target));
    };
    let ExprKind::Variable(outer_name) = &outer_array.kind else {
        return Err(nested_array_access_unsupported(target));
    };
    if module.local_kind(outer_name) != Some(LocalKind::Array)
        || module.array_layout(outer_name) != ArrayLayout::Assoc
    {
        return Err(nested_array_access_unsupported(target));
    }
    let outer_key = static_assoc_access_key(outer_index, module)
        .ok_or_else(|| nested_array_access_unsupported(target))?;
    let outer_entry_index = module
        .array_key_values(outer_name)
        .and_then(|keys| keys.iter().position(|key| *key == outer_key))
        .ok_or_else(|| nested_array_access_unsupported(target))?;
    let inner_metadata = nested_array_metadata_for_access_expr(outer_access, module)
        .ok_or_else(|| nested_array_access_unsupported(target))?;
    if inner_metadata.layout != ArrayLayout::Assoc {
        return Err(nested_array_access_unsupported(target));
    }
    if static_assoc_access_key(inner_index, module).is_none() {
        return emit_dynamic_nested_assoc_assign(
            outer_name,
            outer_entry_index,
            &inner_metadata,
            inner_index,
            value,
            module,
        );
    }
    let inner_key = static_assoc_access_key(inner_index, module)
        .ok_or_else(|| nested_array_access_unsupported(target))?;
    let Some(inner_entry_index) = inner_metadata
        .key_values
        .as_ref()
        .and_then(|keys| keys.iter().position(|key| *key == inner_key))
    else {
        return emit_dynamic_nested_assoc_assign(
            outer_name,
            outer_entry_index,
            &inner_metadata,
            inner_index,
            value,
            module,
        );
    };
    if inner_metadata
        .value_kinds
        .as_ref()
        .and_then(|kinds| kinds.get(inner_entry_index))
        .is_some_and(|kind| *kind == ValueCellKind::Array)
        && value_cell_kind_for_expr(value, module) != Some(ValueCellKind::Array)
    {
        return Err(nested_array_access_unsupported(target));
    }
    emit_ensure_unique_array_payload(outer_name, module);
    let outer_cell = module.next_label("nested_assign_outer_cell");
    let inner_ptr = module.next_label("nested_assign_inner_ptr");
    let inner_len = module.next_label("nested_assign_inner_len");
    let copied_inner = module.next_label("nested_assign_inner_copy");
    let inner_entry = module.next_label("nested_assign_inner_entry");
    let inner_cell = module.next_label("nested_assign_inner_cell");
    for local in [
        &outer_cell,
        &inner_ptr,
        &inner_len,
        &copied_inner,
        &inner_entry,
        &inner_cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_ptr", outer_name));
    module.body().line(&format!("i32.const {}", outer_entry_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", outer_cell));
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", inner_ptr));
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", inner_len));
    module.body().line(&format!("local.get {}", inner_ptr));
    module.body().line(&format!("local.get {}", inner_len));
    module.body().line("call $__rt_assoc_array_copy_entries");
    module.body().line(&format!("local.set {}", copied_inner));
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line(&format!("local.get {}", copied_inner));
    module.body().line(&format!("local.get {}", inner_len));
    module.body().line("call $__rt_value_store_array");
    module.body().line(&format!("local.get {}", copied_inner));
    module.body().line(&format!("i32.const {}", inner_entry_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", inner_entry));
    module.body().line(&format!("local.get {}", inner_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", inner_cell));
    emit_store_value_cell(&inner_cell, value, module)?;
    update_existing_nested_assoc_entry_metadata(
        outer_name,
        outer_entry_index,
        &inner_metadata,
        inner_entry_index,
        value,
        module,
    );
    Ok(())
}

fn update_existing_nested_assoc_entry_metadata(
    outer_name: &str,
    outer_entry_index: usize,
    inner_metadata: &NestedArrayMetadata,
    inner_entry_index: usize,
    value: &Expr,
    module: &mut WasmModule,
) {
    let value_kind = value_cell_kind_for_expr(value, module);
    let mut nested_values = inner_metadata.nested_values.clone();
    if value_kind == Some(ValueCellKind::Array) {
        let values = nested_values.get_or_insert_with(|| vec![None; inner_metadata.len]);
        values[inner_entry_index] = nested_array_metadata_for_expr(value, module);
    } else if let Some(values) = nested_values.as_mut() {
        values[inner_entry_index] = None;
    }
    module.set_array_nested_value_metadata_at(
        outer_name,
        outer_entry_index,
        Some(NestedArrayMetadata {
            layout: inner_metadata.layout,
            len: inner_metadata.len,
            value_kinds: inner_metadata.value_kinds.clone(),
            key_values: inner_metadata.key_values.clone(),
            nested_values,
        }),
    );
}

fn emit_four_level_static_nested_assoc_assign(
    target: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let ExprKind::ArrayAccess {
        array: leaf_array,
        index: target_index,
    } = &target.kind
    else {
        return Ok(false);
    };
    let ExprKind::ArrayAccess {
        array: middle_array,
        index: leaf_index,
    } = &leaf_array.kind
    else {
        return Ok(false);
    };
    let ExprKind::ArrayAccess {
        array: outer_access,
        index: middle_index,
    } = &middle_array.kind
    else {
        return Ok(false);
    };
    let ExprKind::ArrayAccess {
        array: outer_array,
        index: outer_index,
    } = &outer_access.kind
    else {
        return Ok(false);
    };
    let ExprKind::Variable(outer_name) = &outer_array.kind else {
        return Ok(false);
    };
    if module.local_kind(outer_name) != Some(LocalKind::Array)
        || module.array_layout(outer_name) != ArrayLayout::Assoc
    {
        return Ok(false);
    }
    let Some(outer_key) = static_assoc_access_key(outer_index, module) else {
        return Ok(false);
    };
    let Some(middle_key) = static_assoc_access_key(middle_index, module) else {
        return Ok(false);
    };
    let Some(leaf_key) = static_assoc_access_key(leaf_index, module) else {
        return Ok(false);
    };
    let target_key = assoc_assign_key(target_index, module)?;
    let target_key_value = static_assoc_assign_key_metadata(&target_key).map(|(_, key)| key);
    let Some(outer_entry_index) = module
        .array_key_values(outer_name)
        .and_then(|keys| keys.iter().position(|key| *key == outer_key))
    else {
        return Ok(false);
    };
    let Some(mut outer_metadata) = module.array_nested_value_metadata(outer_name, outer_entry_index) else {
        return Ok(false);
    };
    if outer_metadata.layout != ArrayLayout::Assoc {
        return Ok(false);
    }
    let (middle_entry_index, middle_entry_is_exact) = if let Some(index) = outer_metadata
        .key_values
        .as_ref()
        .and_then(|keys| keys.iter().position(|key| *key == middle_key))
    {
        (index, true)
    } else {
        let known_key_count = outer_metadata
            .key_values
            .as_ref()
            .map_or(0, Vec::len);
        let Some(nested_values) = outer_metadata.nested_values.as_ref() else {
            return Ok(false);
        };
        let candidate_indexes = nested_values
            .iter()
            .enumerate()
            .skip(known_key_count)
            .filter_map(|(index, metadata)| {
                metadata
                    .as_ref()
                    .is_some_and(|metadata| metadata.layout == ArrayLayout::Assoc)
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        let [index] = candidate_indexes.as_slice() else {
            return Ok(false);
        };
        (*index, false)
    };
    let Some(outer_nested_values) = outer_metadata.nested_values.as_mut() else {
        return Ok(false);
    };
    let Some(Some(mut middle_metadata)) = outer_nested_values.get(middle_entry_index).cloned() else {
        return Ok(false);
    };
    if middle_metadata.layout != ArrayLayout::Assoc {
        return Ok(false);
    }
    let Some(leaf_entry_index) = middle_metadata
        .key_values
        .as_ref()
        .and_then(|keys| keys.iter().position(|key| *key == leaf_key))
    else {
        return Ok(false);
    };
    let Some(middle_nested_values) = middle_metadata.nested_values.as_mut() else {
        return Ok(false);
    };
    let Some(Some(mut leaf_metadata)) = middle_nested_values.get(leaf_entry_index).cloned() else {
        return Ok(false);
    };
    if leaf_metadata.layout != ArrayLayout::Assoc {
        return Ok(false);
    }
    let value_kind = value_cell_kind_for_expr(value, module)
        .ok_or_else(|| nested_array_access_unsupported(target))?;
    let target_entry_index = target_key_value.as_ref().and_then(|target_key| {
        leaf_metadata
            .key_values
            .as_ref()
            .and_then(|keys| keys.iter().position(|key| key == target_key))
    });
    if let Some(target_entry_index) = target_entry_index {
        let Some(existing_kind) = leaf_metadata
            .value_kinds
            .as_ref()
            .and_then(|kinds| kinds.get(target_entry_index))
            .copied()
        else {
            return Ok(false);
        };
        if existing_kind != value_kind {
            return Err(nested_array_access_unsupported(target));
        }
    }
    emit_four_level_static_nested_assoc_runtime_assign(
        outer_name,
        outer_entry_index,
        middle_entry_index,
        &middle_key,
        middle_entry_is_exact,
        leaf_entry_index,
        target_entry_index,
        &target_key,
        value,
        module,
    )?;
    update_four_level_leaf_metadata(&mut leaf_metadata, target_key_value, value_kind);
    middle_nested_values[leaf_entry_index] = Some(leaf_metadata);
    outer_nested_values[middle_entry_index] = Some(middle_metadata);
    module.set_array_nested_value_metadata_at(outer_name, outer_entry_index, Some(outer_metadata));
    Ok(true)
}

fn emit_four_level_static_nested_assoc_runtime_assign(
    outer_name: &str,
    outer_entry_index: usize,
    middle_entry_index: usize,
    middle_key: &AssocKeyValue,
    middle_entry_is_exact: bool,
    leaf_entry_index: usize,
    target_entry_index: Option<usize>,
    target_key: &AssocAssignKey,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_ensure_unique_array_payload(outer_name, module);
    let outer_cell = module.next_label("four_nested_outer_cell");
    let parent_ptr = module.next_label("four_nested_parent_ptr");
    let parent_len = module.next_label("four_nested_parent_len");
    let copied_parent = module.next_label("four_nested_parent_copy");
    let middle_entry = module.next_label("four_nested_middle_entry");
    let middle_cell = module.next_label("four_nested_middle_cell");
    let middle_ptr = module.next_label("four_nested_middle_ptr");
    let middle_len = module.next_label("four_nested_middle_len");
    let copied_middle = module.next_label("four_nested_middle_copy");
    let leaf_entry = module.next_label("four_nested_leaf_entry");
    let leaf_cell = module.next_label("four_nested_leaf_cell");
    let leaf_ptr = module.next_label("four_nested_leaf_ptr");
    let leaf_len = module.next_label("four_nested_leaf_len");
    let copied_leaf = module.next_label("four_nested_leaf_copy");
    let source_index = module.next_label("four_nested_leaf_source_index");
    let out_index = module.next_label("four_nested_leaf_out_index");
    let source_entry = module.next_label("four_nested_leaf_source_entry");
    let target_entry = module.next_label("four_nested_target_entry");
    let matched = module.next_label("four_nested_leaf_matched");
    let found = module.next_label("four_nested_leaf_found");
    let value_cell = module.next_label("four_nested_value_cell");
    for local in [
        &outer_cell,
        &parent_ptr,
        &parent_len,
        &copied_parent,
        &middle_entry,
        &middle_cell,
        &middle_ptr,
        &middle_len,
        &copied_middle,
        &leaf_entry,
        &leaf_cell,
        &leaf_ptr,
        &leaf_len,
        &copied_leaf,
        &source_index,
        &out_index,
        &source_entry,
        &target_entry,
        &matched,
        &found,
        &value_cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_ptr", outer_name));
    module.body().line(&format!("i32.const {}", outer_entry_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", outer_cell));
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", parent_ptr));
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", parent_len));
    module.body().line(&format!("local.get {}", parent_ptr));
    module.body().line(&format!("local.get {}", parent_len));
    module.body().line("call $__rt_assoc_array_copy_entries");
    module.body().line(&format!("local.set {}", copied_parent));
    if middle_entry_is_exact {
        module.body().line(&format!("local.get {}", copied_parent));
        module.body().line(&format!("i32.const {}", middle_entry_index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", middle_entry));
        module.body().line(&format!("local.get {}", middle_entry));
        module.body().line("call $__rt_assoc_value_cell");
        module.body().line(&format!("local.set {}", middle_cell));
    } else {
        let key = AssocAssignKey::Static(static_key_from_assoc_value(middle_key));
        let found_cell = emit_find_assoc_value_cell_or_trap(
            &copied_parent,
            &parent_len,
            &key,
            "four_nested_middle_lookup",
            module,
        );
        module.body().line(&format!("local.get {}", found_cell));
        module.body().line(&format!("local.set {}", middle_cell));
    }
    module.body().line(&format!("local.get {}", middle_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", middle_ptr));
    module.body().line(&format!("local.get {}", middle_cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", middle_len));
    module.body().line(&format!("local.get {}", middle_ptr));
    module.body().line(&format!("local.get {}", middle_len));
    module.body().line("call $__rt_assoc_array_copy_entries");
    module.body().line(&format!("local.set {}", copied_middle));
    module.body().line(&format!("local.get {}", copied_middle));
    module.body().line(&format!("i32.const {}", leaf_entry_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", leaf_entry));
    module.body().line(&format!("local.get {}", leaf_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", leaf_cell));
    module.body().line(&format!("local.get {}", leaf_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", leaf_ptr));
    module.body().line(&format!("local.get {}", leaf_cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", leaf_len));
    if let Some(index) = target_entry_index {
        module.body().line(&format!("local.get {}", leaf_ptr));
        module.body().line(&format!("local.get {}", leaf_len));
        module.body().line("call $__rt_assoc_array_copy_entries");
        module.body().line(&format!("local.set {}", copied_leaf));
        module.body().line(&format!("local.get {}", copied_leaf));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", target_entry));
        emit_assoc_value_cell_address(&target_entry, &value_cell, module);
        emit_store_value_cell(&value_cell, value, module)?;
    } else {
        emit_four_level_dynamic_leaf_write(
            &leaf_ptr,
            &leaf_len,
            &copied_leaf,
            &source_index,
            &out_index,
            &source_entry,
            &target_entry,
            &matched,
            &found,
            &value_cell,
            target_key,
            value,
            module,
        )?;
    }
    module.body().line(&format!("local.get {}", leaf_cell));
    module.body().line(&format!("local.get {}", copied_leaf));
    if target_entry_index.is_some() {
        module.body().line(&format!("local.get {}", leaf_len));
    } else {
        module.body().line(&format!("local.get {}", out_index));
    }
    module.body().line("call $__rt_value_store_array");
    module.body().line(&format!("local.get {}", middle_cell));
    module.body().line(&format!("local.get {}", copied_middle));
    module.body().line(&format!("local.get {}", middle_len));
    module.body().line("call $__rt_value_store_array");
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line(&format!("local.get {}", copied_parent));
    module.body().line(&format!("local.get {}", parent_len));
    module.body().line("call $__rt_value_store_array");
    Ok(())
}

fn emit_static_property_nested_array_assign(
    target: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let ExprKind::ArrayAccess {
        array: outer_access,
        index: inner_index,
    } = &target.kind
    else {
        return Ok(false);
    };
    let ExprKind::ArrayAccess {
        array: outer_array,
        index: outer_index,
    } = &outer_access.kind
    else {
        return Ok(false);
    };
    let ExprKind::StaticPropertyAccess { receiver, property } = &outer_array.kind else {
        return Ok(false);
    };
    let property_info = module
        .object_static_property(receiver, property)
        .ok_or_else(|| nested_array_access_unsupported(target))?;
    if !module.object_member_is_accessible(&property_info.owner_class, &property_info.visibility) {
        return Err(nested_array_access_unsupported(target));
    }
    let metadata = nested_array_metadata_for_expr(
        property_info
            .default
            .as_ref()
            .ok_or_else(|| nested_array_access_unsupported(target))?,
        module,
    )
    .ok_or_else(|| nested_array_access_unsupported(target))?;
    let temp = module
        .next_label("static_property_nested_array")
        .trim_start_matches('$')
        .to_string();
    let cell = module
        .next_label("static_property_nested_cell")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    module.declare_i32_local(cell.clone());
    let property_expr = Expr::new(
        ExprKind::StaticPropertyAccess {
            receiver: receiver.clone(),
            property: property.clone(),
        },
        outer_array.span,
    );
    match emit_static_property_access_expr(&property_expr, receiver, property, module)? {
        ValueKind::Mixed => {
            module.body().line(&format!("local.set ${}", cell));
        }
        _ => return Err(nested_array_access_unsupported(target)),
    }
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("call $__rt_mixed_array_ptr");
    module.body().line(&format!("local.set ${}_ptr", temp));
    module.body().line(&format!("local.get ${}", cell));
    module.body().line("call $__rt_mixed_array_len_i32");
    module.body().line(&format!("local.set ${}_len", temp));
    apply_nested_array_metadata_to_temp(&temp, &metadata, module);
    let staged_outer = Expr::new(ExprKind::Variable(temp.clone()), outer_array.span);
    let staged_outer_access = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(staged_outer),
            index: outer_index.clone(),
        },
        outer_access.span,
    );
    let staged_target = Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(staged_outer_access),
            index: inner_index.clone(),
        },
        target.span,
    );
    emit_nested_array_assign(&staged_target, value, module)?;
    let updated_temp = Expr::new(ExprKind::Variable(temp.clone()), target.span);
    let updated_metadata = nested_array_metadata_for_expr(&updated_temp, module)
        .ok_or_else(|| nested_array_access_unsupported(target))?;
    module.set_static_property_nested_value_metadata(&property_info, updated_metadata);
    module.body().line(&format!("local.get ${}", cell));
    module.body().line(&format!("local.get ${}_ptr", temp));
    module.body().line(&format!("local.get ${}_len", temp));
    module.body().line("call $__rt_value_store_array");
    Ok(true)
}

fn apply_nested_array_metadata_to_temp(
    name: &str,
    metadata: &NestedArrayMetadata,
    module: &mut WasmModule,
) {
    module.set_array_layout(name, metadata.layout);
    module.set_array_length(name, metadata.len);
    module.set_array_value_cell_kinds(name, metadata.value_kinds.clone());
    module.set_array_key_kinds(
        name,
        metadata.key_values.as_ref().map(|values| {
            values
                .iter()
                .map(|value| match value {
                    AssocKeyValue::Int(_) => AssocKeyKind::Int,
                    AssocKeyValue::Str(_) => AssocKeyKind::Str,
                })
                .collect()
        }),
    );
    module.set_array_key_values(name, metadata.key_values.clone());
    module.set_array_nested_value_metadata(name, metadata.nested_values.clone());
}

fn emit_four_level_dynamic_leaf_write(
    leaf_ptr: &str,
    leaf_len: &str,
    copied_leaf: &str,
    source_index: &str,
    out_index: &str,
    source_entry: &str,
    target_entry: &str,
    matched: &str,
    found: &str,
    value_cell: &str,
    target_key: &AssocAssignKey,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let done_label = module.next_label("four_nested_leaf_done");
    let loop_label = module.next_label("four_nested_leaf_loop");
    module.body().line(&format!("local.get {}", leaf_len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set {}", copied_leaf));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("local.get {}", leaf_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", leaf_ptr));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", source_entry));
    module.body().line(&format!("local.get {}", copied_leaf));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    emit_assoc_entry_matches_assign_key(source_entry, target_key, matched, module);
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    copy_assoc_entry_key(target_entry, source_entry, module);
    emit_assoc_value_cell_address(target_entry, value_cell, module);
    emit_store_value_cell(value_cell, value, module)?;
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line("else");
    copy_assoc_entry(target_entry, source_entry, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line(&format!("local.get {}", copied_leaf));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    emit_store_assoc_assign_key_at_entry(target_entry, target_key, module);
    emit_assoc_value_cell_address(target_entry, value_cell, module);
    emit_store_value_cell(value_cell, value, module)?;
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    module.body().close("end");
    Ok(())
}

fn update_four_level_leaf_metadata(
    leaf_metadata: &mut NestedArrayMetadata,
    target_key: Option<AssocKeyValue>,
    value_kind: ValueCellKind,
) {
    if let Some(target_key) = target_key {
        update_assoc_leaf_metadata(leaf_metadata, target_key, value_kind);
        return;
    }
    let Some(value_kinds) = dynamic_leaf_value_kinds_after_runtime_key_write(leaf_metadata, value_kind) else {
        leaf_metadata.key_values = None;
        leaf_metadata.value_kinds = None;
        leaf_metadata.nested_values = None;
        return;
    };
    leaf_metadata.len += 1;
    leaf_metadata.key_values = None;
    leaf_metadata.value_kinds = Some(value_kinds);
    leaf_metadata.nested_values = None;
}

fn static_key_from_assoc_value(key: &AssocKeyValue) -> StaticAssocKey {
    match key {
        AssocKeyValue::Int(value) => StaticAssocKey::Int(*value),
        AssocKeyValue::Str(value) => StaticAssocKey::Str(value.clone()),
    }
}

pub(crate) fn emit_nested_array_push(
    target: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let ExprKind::ArrayAccess {
        array: outer_access,
        index: leaf_index,
    } = &target.kind
    else {
        return Err(nested_array_access_unsupported(target));
    };
    let ExprKind::ArrayAccess {
        array: outer_array,
        index: outer_index,
    } = &outer_access.kind
    else {
        return Err(nested_array_access_unsupported(target));
    };
    let ExprKind::Variable(outer_name) = &outer_array.kind else {
        return Err(nested_array_access_unsupported(target));
    };
    if module.local_kind(outer_name) != Some(LocalKind::Array)
        || module.array_layout(outer_name) != ArrayLayout::Assoc
    {
        return Err(nested_array_access_unsupported(target));
    }
    let outer_key = static_assoc_access_key(outer_index, module)
        .ok_or_else(|| nested_array_access_unsupported(target))?;
    let outer_entry_index = module
        .array_key_values(outer_name)
        .and_then(|keys| keys.iter().position(|key| *key == outer_key))
        .ok_or_else(|| nested_array_access_unsupported(target))?;
    let inner_metadata = nested_array_metadata_for_access_expr(outer_access, module)
        .ok_or_else(|| nested_array_access_unsupported(target))?;
    if inner_metadata.layout != ArrayLayout::Assoc {
        return Err(nested_array_access_unsupported(target));
    }
    let leaf_key = static_assoc_access_key(leaf_index, module)
        .ok_or_else(|| nested_array_access_unsupported(target))?;
    let leaf_entry_index = inner_metadata
        .key_values
        .as_ref()
        .and_then(|keys| keys.iter().position(|key| *key == leaf_key))
        .ok_or_else(|| nested_array_access_unsupported(target))?;
    let leaf_metadata = inner_metadata
        .nested_values
        .as_ref()
        .and_then(|values| values.get(leaf_entry_index))
        .cloned()
        .flatten()
        .ok_or_else(|| nested_array_access_unsupported(target))?;
    let value_kind = value_cell_kind_for_expr(value, module)
        .ok_or_else(|| nested_array_access_unsupported(target))?;
    if leaf_metadata.layout == ArrayLayout::Assoc {
        return emit_nested_assoc_leaf_push(
            outer_name,
            outer_entry_index,
            &inner_metadata,
            leaf_entry_index,
            &leaf_metadata,
            value_kind,
            value,
            module,
        );
    }
    if leaf_metadata.layout != ArrayLayout::Value {
        return Err(nested_array_access_unsupported(target));
    }
    if let Some(kinds) = leaf_metadata.value_kinds.as_ref() {
        if !kinds.iter().all(|kind| *kind == value_kind) {
            return Err(nested_array_access_unsupported(target));
        }
    }
    emit_ensure_unique_array_payload(outer_name, module);
    let outer_cell = module.next_label("nested_push_outer_cell");
    let inner_ptr = module.next_label("nested_push_inner_ptr");
    let inner_len = module.next_label("nested_push_inner_len");
    let copied_inner = module.next_label("nested_push_inner_copy");
    let leaf_entry = module.next_label("nested_push_leaf_entry");
    let leaf_cell = module.next_label("nested_push_leaf_cell");
    let leaf_ptr = module.next_label("nested_push_leaf_ptr");
    let copied_leaf = module.next_label("nested_push_leaf_copy");
    let source_index = module.next_label("nested_push_source_index");
    let source_cell = module.next_label("nested_push_source_cell");
    let target_cell = module.next_label("nested_push_target_cell");
    let done_label = module.next_label("nested_push_done");
    let loop_label = module.next_label("nested_push_loop");
    for local in [
        &outer_cell,
        &inner_ptr,
        &inner_len,
        &copied_inner,
        &leaf_entry,
        &leaf_cell,
        &leaf_ptr,
        &copied_leaf,
        &source_index,
        &source_cell,
        &target_cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_ptr", outer_name));
    module.body().line(&format!("i32.const {}", outer_entry_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", outer_cell));
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", inner_ptr));
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", inner_len));
    module.body().line(&format!("local.get {}", inner_ptr));
    module.body().line(&format!("local.get {}", inner_len));
    module.body().line("call $__rt_assoc_array_copy_entries");
    module.body().line(&format!("local.set {}", copied_inner));
    module.body().line(&format!("local.get {}", copied_inner));
    module.body().line(&format!("i32.const {}", leaf_entry_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", leaf_entry));
    module.body().line(&format!("local.get {}", leaf_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", leaf_cell));
    module.body().line(&format!("local.get {}", leaf_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", leaf_ptr));
    module.body().line(&format!("i32.const {}", leaf_metadata.len + 1));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", copied_leaf));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", source_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("i32.const {}", leaf_metadata.len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", leaf_ptr));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", source_cell));
    module.body().line(&format!("local.get {}", copied_leaf));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", target_cell));
    module.body().line(&format!("local.get {}", target_cell));
    module.body().line(&format!("local.get {}", source_cell));
    module.body().line("call $__rt_value_copy");
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", copied_leaf));
    module.body().line(&format!("i32.const {}", leaf_metadata.len));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", target_cell));
    emit_store_value_cell(&target_cell, value, module)?;
    module.body().line(&format!("local.get {}", leaf_cell));
    module.body().line(&format!("local.get {}", copied_leaf));
    module.body().line(&format!("i32.const {}", leaf_metadata.len + 1));
    module.body().line("call $__rt_value_store_array");
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line(&format!("local.get {}", copied_inner));
    module.body().line(&format!("local.get {}", inner_len));
    module.body().line("call $__rt_value_store_array");
    update_nested_array_push_metadata(outer_name, outer_entry_index, &inner_metadata, leaf_entry_index, value_kind, module);
    Ok(())
}

fn emit_nested_assoc_leaf_push(
    outer_name: &str,
    outer_entry_index: usize,
    inner_metadata: &NestedArrayMetadata,
    leaf_entry_index: usize,
    leaf_metadata: &NestedArrayMetadata,
    value_kind: ValueCellKind,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let next_key = next_nested_assoc_append_key(leaf_metadata)
        .ok_or_else(|| nested_array_access_unsupported(value))?;
    emit_ensure_unique_array_payload(outer_name, module);
    let outer_cell = module.next_label("nested_assoc_push_outer_cell");
    let inner_ptr = module.next_label("nested_assoc_push_inner_ptr");
    let inner_len = module.next_label("nested_assoc_push_inner_len");
    let copied_inner = module.next_label("nested_assoc_push_inner_copy");
    let leaf_entry = module.next_label("nested_assoc_push_leaf_entry");
    let leaf_cell = module.next_label("nested_assoc_push_leaf_cell");
    let leaf_ptr = module.next_label("nested_assoc_push_leaf_ptr");
    let leaf_len = module.next_label("nested_assoc_push_leaf_len");
    let copied_leaf = module.next_label("nested_assoc_push_leaf_copy");
    let source_index = module.next_label("nested_assoc_push_source_index");
    let source_entry = module.next_label("nested_assoc_push_source_entry");
    let target_entry = module.next_label("nested_assoc_push_target_entry");
    let value_cell = module.next_label("nested_assoc_push_value_cell");
    let done_label = module.next_label("nested_assoc_push_done");
    let loop_label = module.next_label("nested_assoc_push_loop");
    for local in [
        &outer_cell,
        &inner_ptr,
        &inner_len,
        &copied_inner,
        &leaf_entry,
        &leaf_cell,
        &leaf_ptr,
        &leaf_len,
        &copied_leaf,
        &source_index,
        &source_entry,
        &target_entry,
        &value_cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_ptr", outer_name));
    module.body().line(&format!("i32.const {}", outer_entry_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", outer_cell));
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", inner_ptr));
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", inner_len));
    module.body().line(&format!("local.get {}", inner_ptr));
    module.body().line(&format!("local.get {}", inner_len));
    module.body().line("call $__rt_assoc_array_copy_entries");
    module.body().line(&format!("local.set {}", copied_inner));
    module.body().line(&format!("local.get {}", copied_inner));
    module.body().line(&format!("i32.const {}", leaf_entry_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", leaf_entry));
    module.body().line(&format!("local.get {}", leaf_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", leaf_cell));
    module.body().line(&format!("local.get {}", leaf_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", leaf_ptr));
    module.body().line(&format!("local.get {}", leaf_cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", leaf_len));
    module.body().line(&format!("local.get {}", leaf_len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set {}", copied_leaf));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", source_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("local.get {}", leaf_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", leaf_ptr));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", source_entry));
    module.body().line(&format!("local.get {}", copied_leaf));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    copy_assoc_entry(&target_entry, &source_entry, module);
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", copied_leaf));
    module.body().line(&format!("local.get {}", leaf_len));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    emit_store_assoc_key_at_entry(&target_entry, &StaticAssocKey::Int(next_key), module);
    emit_assoc_value_cell_address(&target_entry, &value_cell, module);
    emit_store_value_cell(&value_cell, value, module)?;
    module.body().line(&format!("local.get {}", leaf_cell));
    module.body().line(&format!("local.get {}", copied_leaf));
    module.body().line(&format!("local.get {}", leaf_len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("call $__rt_value_store_array");
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line(&format!("local.get {}", copied_inner));
    module.body().line(&format!("local.get {}", inner_len));
    module.body().line("call $__rt_value_store_array");
    update_nested_assoc_leaf_push_metadata(
        outer_name,
        outer_entry_index,
        inner_metadata,
        leaf_entry_index,
        next_key,
        value_kind,
        module,
    );
    Ok(())
}

fn next_nested_assoc_append_key(metadata: &NestedArrayMetadata) -> Option<i64> {
    let mut next = 0i64;
    for value in metadata.key_values.as_ref()? {
        if let AssocKeyValue::Int(key) = value {
            if *key >= next {
                next = key + 1;
            }
        }
    }
    Some(next)
}

fn emit_deeper_dynamic_nested_assoc_assign(
    target: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let ExprKind::ArrayAccess {
        array: leaf_array,
        index: leaf_index,
    } = &target.kind
    else {
        return Ok(false);
    };
    let ExprKind::ArrayAccess {
        array: middle_array,
        index: middle_index,
    } = &leaf_array.kind
    else {
        return Ok(false);
    };
    let ExprKind::ArrayAccess {
        array: outer_array,
        index: outer_index,
    } = &middle_array.kind
    else {
        return Ok(false);
    };
    let ExprKind::Variable(outer_name) = &outer_array.kind else {
        return Ok(false);
    };
    if module.local_kind(outer_name) != Some(LocalKind::Array)
        || module.array_layout(outer_name) != ArrayLayout::Assoc
    {
        return Ok(false);
    }
    let outer_key = assoc_assign_key(outer_index, module)?;
    let middle_key = assoc_assign_key(middle_index, module)?;
    let leaf_key = assoc_assign_key(leaf_index, module)?;
    let Some(leaf_metadata) = dynamic_nested_array_value_metadata(leaf_array, module)
        .or_else(|| nested_array_metadata_for_access_expr(leaf_array, module))
    else {
        if let Some(leaf_metadata) = dynamic_existing_middle_assoc_leaf_metadata(middle_array, module) {
            let value_kind = value_cell_kind_for_expr(value, module)
                .ok_or_else(|| nested_array_access_unsupported(target))?;
            emit_deeper_dynamic_nested_assoc_leaf_assign(
                outer_name,
                &outer_key,
                &middle_key,
                &leaf_key,
                &leaf_metadata,
                value_kind,
                value,
                module,
            )?;
            update_deeper_dynamic_nested_assoc_assign_metadata(
                outer_name,
                &outer_key,
                &middle_key,
                &leaf_key,
                value_kind,
                module,
            );
            return Ok(true);
        }
        if emit_deeper_dynamic_nested_assoc_missing_middle_assign(
            outer_name,
            outer_index,
            middle_array,
            &outer_key,
            &middle_key,
            &leaf_key,
            value,
            module,
        )? {
            return Ok(true);
        }
        return Ok(false);
    };
    if leaf_metadata.layout != ArrayLayout::Assoc {
        return Ok(false);
    }
    let leaf_key_value = static_assoc_assign_key_metadata(&leaf_key).map(|(_, value)| value);
    let leaf_entry_index = leaf_metadata
        .key_values
        .as_ref()
        .and_then(|keys| {
            leaf_key_value
                .as_ref()
                .and_then(|leaf_key| keys.iter().position(|key| key == leaf_key))
        });
    let value_kind = value_cell_kind_for_expr(value, module)
        .ok_or_else(|| nested_array_access_unsupported(target))?;
    if leaf_key_value.is_none() {
        emit_deeper_dynamic_nested_assoc_leaf_assign(
            outer_name,
            &outer_key,
            &middle_key,
            &leaf_key,
            &leaf_metadata,
            value_kind,
            value,
            module,
        )?;
        return Ok(true);
    }
    if let Some(leaf_entry_index) = leaf_entry_index {
        let Some(existing_kind) = leaf_metadata
            .value_kinds
            .as_ref()
            .and_then(|kinds| kinds.get(leaf_entry_index))
            .copied()
        else {
            return Err(nested_array_access_unsupported(target));
        };
        if value_kind != existing_kind {
            return Err(nested_array_access_unsupported(target));
        }
    }
    emit_ensure_unique_array_payload(outer_name, module);
    let outer_cell = emit_find_assoc_value_cell_or_trap(
        &format!("${}_ptr", outer_name),
        &format!("${}_len", outer_name),
        &outer_key,
        "deeper_nested_outer",
        module,
    );
    let parent_ptr = module.next_label("deeper_nested_parent_ptr");
    let parent_len = module.next_label("deeper_nested_parent_len");
    let copied_parent = module.next_label("deeper_nested_parent_copy");
    let leaf_ptr = module.next_label("deeper_nested_leaf_ptr");
    let leaf_len = module.next_label("deeper_nested_leaf_len");
    let copied_leaf = module.next_label("deeper_nested_leaf_copy");
    let leaf_entry = module.next_label("deeper_nested_leaf_entry");
    let leaf_cell = module.next_label("deeper_nested_leaf_cell");
    for local in [
        &parent_ptr,
        &parent_len,
        &copied_parent,
        &leaf_ptr,
        &leaf_len,
        &copied_leaf,
        &leaf_entry,
        &leaf_cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", parent_ptr));
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", parent_len));
    module.body().line(&format!("local.get {}", parent_ptr));
    module.body().line(&format!("local.get {}", parent_len));
    module.body().line("call $__rt_assoc_array_copy_entries");
    module.body().line(&format!("local.set {}", copied_parent));
    let middle_cell = emit_find_assoc_value_cell_or_trap(
        &copied_parent,
        &parent_len,
        &middle_key,
        "deeper_nested_middle",
        module,
    );
    module.body().line(&format!("local.get {}", middle_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", leaf_ptr));
    module.body().line(&format!("local.get {}", middle_cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", leaf_len));
    if leaf_entry_index.is_some() {
        module.body().line(&format!("local.get {}", leaf_ptr));
        module.body().line(&format!("local.get {}", leaf_len));
        module.body().line("call $__rt_assoc_array_copy_entries");
        module.body().line(&format!("local.set {}", copied_leaf));
    } else {
        emit_copy_assoc_entries_with_extra_slot(
            &leaf_ptr,
            &leaf_len,
            &copied_leaf,
            "deeper_nested_leaf_grow",
            module,
        );
    }
    module.body().line(&format!("local.get {}", middle_cell));
    module.body().line(&format!("local.get {}", copied_leaf));
    module.body().line(&format!("local.get {}", leaf_len));
    if leaf_entry_index.is_none() {
        module.body().line("i32.const 1");
        module.body().line("i32.add");
    }
    module.body().line("call $__rt_value_store_array");
    module.body().line(&format!("local.get {}", copied_leaf));
    if let Some(leaf_entry_index) = leaf_entry_index {
        module.body().line(&format!("i32.const {}", leaf_entry_index));
    } else {
        module.body().line(&format!("local.get {}", leaf_len));
    }
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", leaf_entry));
    if leaf_entry_index.is_none() {
        emit_store_assoc_assign_key_at_entry(&leaf_entry, &leaf_key, module);
    }
    module.body().line(&format!("local.get {}", leaf_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", leaf_cell));
    emit_store_value_cell(&leaf_cell, value, module)?;
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line(&format!("local.get {}", copied_parent));
    module.body().line(&format!("local.get {}", parent_len));
    module.body().line("call $__rt_value_store_array");
    update_deeper_dynamic_nested_assoc_assign_metadata(
        outer_name,
        &outer_key,
        &middle_key,
        &leaf_key,
        value_kind,
        module,
    );
    Ok(true)
}

fn dynamic_existing_middle_assoc_leaf_metadata(
    middle_array: &Expr,
    module: &WasmModule,
) -> Option<NestedArrayMetadata> {
    let parent_metadata = nested_array_metadata_for_access_expr(middle_array, module)?;
    if parent_metadata.layout != ArrayLayout::Assoc
        || homogeneous_nested_assoc_value_kind(&parent_metadata) != Some(ValueCellKind::Array)
        || parent_metadata
            .key_values
            .as_ref()
            .is_some_and(|keys| keys.len() == parent_metadata.len)
    {
        return None;
    }
    let nested_values = parent_metadata.nested_values.as_ref()?;
    if !nested_values
        .iter()
        .flatten()
        .any(|metadata| metadata.layout == ArrayLayout::Assoc)
    {
        return None;
    }
    Some(NestedArrayMetadata {
        layout: ArrayLayout::Assoc,
        len: 0,
        value_kinds: None,
        key_values: None,
        nested_values: None,
    })
}

fn emit_deeper_dynamic_nested_assoc_missing_middle_assign(
    outer_name: &str,
    outer_index: &Expr,
    middle_array: &Expr,
    outer_key: &AssocAssignKey,
    middle_key: &AssocAssignKey,
    leaf_key: &AssocAssignKey,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    let Some(parent_metadata) = nested_array_metadata_for_access_expr(middle_array, module) else {
        return Ok(false);
    };
    let Some(value_kind) = value_cell_kind_for_expr(value, module) else {
        return Err(nested_array_access_unsupported(value));
    };
    let Some(outer_key_value) = static_assoc_access_key(outer_index, module) else {
        return Ok(false);
    };
    let Some(outer_entry_index) = module
        .array_key_values(outer_name)
        .and_then(|keys| keys.iter().position(|key| *key == outer_key_value))
    else {
        return Ok(false);
    };
    emit_ensure_unique_array_payload(outer_name, module);
    let outer_cell = emit_find_assoc_value_cell_or_trap(
        &format!("${}_ptr", outer_name),
        &format!("${}_len", outer_name),
        outer_key,
        "deeper_missing_middle_outer",
        module,
    );
    let parent_ptr = module.next_label("deeper_missing_middle_parent_ptr");
    let parent_len = module.next_label("deeper_missing_middle_parent_len");
    let copied_parent = module.next_label("deeper_missing_middle_parent_copy");
    let middle_entry = module.next_label("deeper_missing_middle_entry");
    let middle_cell = module.next_label("deeper_missing_middle_cell");
    let leaf_ptr = module.next_label("deeper_missing_middle_leaf_ptr");
    let leaf_entry = module.next_label("deeper_missing_middle_leaf_entry");
    let leaf_cell = module.next_label("deeper_missing_middle_leaf_cell");
    for local in [
        &parent_ptr,
        &parent_len,
        &copied_parent,
        &middle_entry,
        &middle_cell,
        &leaf_ptr,
        &leaf_entry,
        &leaf_cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", parent_ptr));
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", parent_len));
    emit_copy_assoc_entries_with_extra_slot(
        &parent_ptr,
        &parent_len,
        &copied_parent,
        "deeper_missing_middle_parent_grow",
        module,
    );
    module.body().line("i32.const 1");
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set {}", leaf_ptr));
    module.body().line(&format!("local.get {}", copied_parent));
    module.body().line(&format!("local.get {}", parent_len));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", middle_entry));
    emit_store_assoc_assign_key_at_entry(&middle_entry, middle_key, module);
    module.body().line(&format!("local.get {}", middle_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", middle_cell));
    module.body().line(&format!("local.get {}", middle_cell));
    module.body().line(&format!("local.get {}", leaf_ptr));
    module.body().line("i32.const 1");
    module.body().line("call $__rt_value_store_array");
    module.body().line(&format!("local.get {}", leaf_ptr));
    module.body().line("i32.const 0");
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", leaf_entry));
    emit_store_assoc_assign_key_at_entry(&leaf_entry, leaf_key, module);
    module.body().line(&format!("local.get {}", leaf_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", leaf_cell));
    emit_store_value_cell(&leaf_cell, value, module)?;
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line(&format!("local.get {}", copied_parent));
    module.body().line(&format!("local.get {}", parent_len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("call $__rt_value_store_array");
    update_deeper_dynamic_missing_middle_metadata(
        outer_name,
        outer_entry_index,
        &parent_metadata,
        middle_key,
        leaf_key,
        value,
        value_kind,
        module,
    );
    Ok(true)
}

fn emit_copy_assoc_entries_with_extra_slot(
    source_ptr: &str,
    source_len: &str,
    copied_ptr: &str,
    label_prefix: &str,
    module: &mut WasmModule,
) {
    let source_index = module.next_label(&format!("{label_prefix}_index"));
    let source_entry = module.next_label(&format!("{label_prefix}_source_entry"));
    let target_entry = module.next_label(&format!("{label_prefix}_target_entry"));
    let done_label = module.next_label(&format!("{label_prefix}_done"));
    let loop_label = module.next_label(&format!("{label_prefix}_loop"));
    for local in [&source_index, &source_entry, &target_entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set {}", copied_ptr));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", source_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", source_ptr));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", source_entry));
    module.body().line(&format!("local.get {}", copied_ptr));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    copy_assoc_entry(&target_entry, &source_entry, module);
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

fn emit_deeper_dynamic_nested_assoc_leaf_assign(
    outer_name: &str,
    outer_key: &AssocAssignKey,
    middle_key: &AssocAssignKey,
    leaf_key: &AssocAssignKey,
    leaf_metadata: &NestedArrayMetadata,
    value_kind: ValueCellKind,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_ensure_unique_array_payload(outer_name, module);
    let outer_cell = emit_find_assoc_value_cell_or_trap(
        &format!("${}_ptr", outer_name),
        &format!("${}_len", outer_name),
        outer_key,
        "deeper_dynamic_leaf_outer",
        module,
    );
    let parent_ptr = module.next_label("deeper_dynamic_leaf_parent_ptr");
    let parent_len = module.next_label("deeper_dynamic_leaf_parent_len");
    let copied_parent = module.next_label("deeper_dynamic_leaf_parent_copy");
    let leaf_ptr = module.next_label("deeper_dynamic_leaf_ptr");
    let leaf_len = module.next_label("deeper_dynamic_leaf_len");
    let copied_leaf = module.next_label("deeper_dynamic_leaf_copy");
    let source_index = module.next_label("deeper_dynamic_leaf_source_index");
    let out_index = module.next_label("deeper_dynamic_leaf_out_index");
    let source_entry = module.next_label("deeper_dynamic_leaf_source_entry");
    let target_entry = module.next_label("deeper_dynamic_leaf_target_entry");
    let matched = module.next_label("deeper_dynamic_leaf_matched");
    let found = module.next_label("deeper_dynamic_leaf_found");
    let value_cell = module.next_label("deeper_dynamic_leaf_value_cell");
    let done_label = module.next_label("deeper_dynamic_leaf_done");
    let loop_label = module.next_label("deeper_dynamic_leaf_loop");
    for local in [
        &parent_ptr,
        &parent_len,
        &copied_parent,
        &leaf_ptr,
        &leaf_len,
        &copied_leaf,
        &source_index,
        &out_index,
        &source_entry,
        &target_entry,
        &matched,
        &found,
        &value_cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", parent_ptr));
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", parent_len));
    module.body().line(&format!("local.get {}", parent_ptr));
    module.body().line(&format!("local.get {}", parent_len));
    module.body().line("call $__rt_assoc_array_copy_entries");
    module.body().line(&format!("local.set {}", copied_parent));
    let middle_cell = emit_find_assoc_value_cell_or_trap(
        &copied_parent,
        &parent_len,
        middle_key,
        "deeper_dynamic_leaf_middle",
        module,
    );
    module.body().line(&format!("local.get {}", middle_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", leaf_ptr));
    module.body().line(&format!("local.get {}", middle_cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", leaf_len));
    module.body().line(&format!("local.get {}", leaf_len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set {}", copied_leaf));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("local.get {}", leaf_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", leaf_ptr));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", source_entry));
    module.body().line(&format!("local.get {}", copied_leaf));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    emit_assoc_entry_matches_assign_key(&source_entry, leaf_key, &matched, module);
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    copy_assoc_entry_key(&target_entry, &source_entry, module);
    emit_assoc_value_cell_address(&target_entry, &value_cell, module);
    emit_store_value_cell(&value_cell, value, module)?;
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line("else");
    copy_assoc_entry(&target_entry, &source_entry, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line(&format!("local.get {}", copied_leaf));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    emit_store_assoc_assign_key_at_entry(&target_entry, leaf_key, module);
    emit_assoc_value_cell_address(&target_entry, &value_cell, module);
    emit_store_value_cell(&value_cell, value, module)?;
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    module.body().close("end");
    module.body().line(&format!("local.get {}", middle_cell));
    module.body().line(&format!("local.get {}", copied_leaf));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_value_store_array");
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line(&format!("local.get {}", copied_parent));
    module.body().line(&format!("local.get {}", parent_len));
    module.body().line("call $__rt_value_store_array");
    mark_deeper_dynamic_leaf_metadata_runtime_keyed(
        outer_name,
        outer_key,
        middle_key,
        leaf_metadata,
        value_kind,
        module,
    );
    Ok(())
}

fn mark_deeper_dynamic_leaf_metadata_runtime_keyed(
    outer_name: &str,
    outer_key: &AssocAssignKey,
    middle_key: &AssocAssignKey,
    leaf_metadata: &NestedArrayMetadata,
    value_kind: ValueCellKind,
    module: &mut WasmModule,
) {
    let Some((_, outer_key_value)) = static_assoc_assign_key_metadata(outer_key) else {
        return;
    };
    let Some((_, middle_key_value)) = static_assoc_assign_key_metadata(middle_key) else {
        return;
    };
    let Some(outer_entry_index) = module
        .array_key_values(outer_name)
        .and_then(|keys| keys.iter().position(|key| *key == outer_key_value))
    else {
        return;
    };
    let Some(mut outer_metadata) = module.array_nested_value_metadata(outer_name, outer_entry_index) else {
        return;
    };
    let Some(middle_entry_index) = outer_metadata
        .key_values
        .as_ref()
        .and_then(|keys| keys.iter().position(|key| *key == middle_key_value))
    else {
        return;
    };
    let Some(nested_values) = outer_metadata.nested_values.as_mut() else {
        return;
    };
    nested_values[middle_entry_index] = Some(NestedArrayMetadata {
        layout: ArrayLayout::Assoc,
        len: leaf_metadata.len + 1,
        value_kinds: dynamic_leaf_value_kinds_after_runtime_key_write(leaf_metadata, value_kind),
        key_values: None,
        nested_values: None,
    });
    module.set_array_nested_value_metadata_at(outer_name, outer_entry_index, Some(outer_metadata));
}

fn dynamic_leaf_value_kinds_after_runtime_key_write(
    leaf_metadata: &NestedArrayMetadata,
    value_kind: ValueCellKind,
) -> Option<Vec<ValueCellKind>> {
    let existing = leaf_metadata.value_kinds.as_ref()?;
    if existing.iter().all(|kind| *kind == value_kind) {
        Some(vec![value_kind; leaf_metadata.len + 1])
    } else {
        None
    }
}

fn emit_find_assoc_value_cell_or_trap(
    ptr: &str,
    len: &str,
    key: &AssocAssignKey,
    label_prefix: &str,
    module: &mut WasmModule,
) -> String {
    let index = module.next_label(&format!("{label_prefix}_index"));
    let entry = module.next_label(&format!("{label_prefix}_entry"));
    let matched = module.next_label(&format!("{label_prefix}_matched"));
    let cell = module.next_label(&format!("{label_prefix}_cell"));
    let found = module.next_label(&format!("{label_prefix}_found"));
    let done_label = module.next_label(&format!("{label_prefix}_done"));
    let loop_label = module.next_label(&format!("{label_prefix}_loop"));
    for local in [&index, &entry, &matched, &cell, &found] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", cell));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", ptr));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    emit_assoc_entry_matches_assign_key(&entry, key, &matched, module);
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    cell
}

fn emit_dynamic_nested_assoc_assign(
    outer_name: &str,
    outer_entry_index: usize,
    inner_metadata: &NestedArrayMetadata,
    inner_index: &Expr,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let key = assoc_assign_key(inner_index, module)?;
    let value_kind = value_cell_kind_for_expr(value, module);
    emit_ensure_unique_array_payload(outer_name, module);
    let outer_cell = module.next_label("nested_dynamic_assign_outer_cell");
    let inner_ptr = module.next_label("nested_dynamic_assign_inner_ptr");
    let inner_len = module.next_label("nested_dynamic_assign_inner_len");
    let copied_inner = module.next_label("nested_dynamic_assign_inner_copy");
    let source_index = module.next_label("nested_dynamic_assign_source_index");
    let out_index = module.next_label("nested_dynamic_assign_out_index");
    let source_entry = module.next_label("nested_dynamic_assign_source_entry");
    let target_entry = module.next_label("nested_dynamic_assign_target_entry");
    let matched = module.next_label("nested_dynamic_assign_matched");
    let found = module.next_label("nested_dynamic_assign_found");
    let value_cell = module.next_label("nested_dynamic_assign_value_cell");
    let done_label = module.next_label("nested_dynamic_assign_done");
    let loop_label = module.next_label("nested_dynamic_assign_loop");
    for local in [
        &outer_cell,
        &inner_ptr,
        &inner_len,
        &copied_inner,
        &source_index,
        &out_index,
        &source_entry,
        &target_entry,
        &matched,
        &found,
        &value_cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_ptr", outer_name));
    module.body().line(&format!("i32.const {}", outer_entry_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", outer_cell));
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", inner_ptr));
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", inner_len));
    module.body().line(&format!("local.get {}", inner_len));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set {}", copied_inner));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("local.get {}", inner_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", inner_ptr));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", source_entry));
    module.body().line(&format!("local.get {}", copied_inner));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    emit_assoc_entry_matches_assign_key(&source_entry, &key, &matched, module);
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    copy_assoc_entry_key(&target_entry, &source_entry, module);
    emit_assoc_value_cell_address(&target_entry, &value_cell, module);
    emit_store_value_cell(&value_cell, value, module)?;
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line("else");
    copy_assoc_entry(&target_entry, &source_entry, module);
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
    module.body().line("i32.eqz");
    module.body().open("if");
    module.body().line(&format!("local.get {}", copied_inner));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", target_entry));
    emit_store_assoc_assign_key_at_entry(&target_entry, &key, module);
    emit_assoc_value_cell_address(&target_entry, &value_cell, module);
    emit_store_value_cell(&value_cell, value, module)?;
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    module.body().close("end");
    module.body().line(&format!("local.get {}", outer_cell));
    module.body().line(&format!("local.get {}", copied_inner));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_value_store_array");
    update_dynamic_nested_assoc_assign_metadata(
        outer_name,
        outer_entry_index,
        inner_metadata,
        &key,
        value_kind,
        module,
    );
    Ok(())
}

fn update_dynamic_nested_assoc_assign_metadata(
    outer_name: &str,
    outer_entry_index: usize,
    inner_metadata: &NestedArrayMetadata,
    key: &AssocAssignKey,
    value_kind: Option<ValueCellKind>,
    module: &mut WasmModule,
) {
    let Some(value_kind) = value_kind else {
        module.set_array_nested_value_metadata_at(outer_name, outer_entry_index, None);
        return;
    };
    if let Some((_, key_value)) = static_assoc_assign_key_metadata(key) {
        if let (Some(old_keys), Some(old_kinds)) = (
            inner_metadata.key_values.as_ref(),
            inner_metadata.value_kinds.as_ref(),
        ) {
            let mut key_values = old_keys.clone();
            let mut value_kinds = old_kinds.clone();
            let mut nested_values = inner_metadata.nested_values.clone();
            if let Some(existing) = key_values.iter().position(|key| *key == key_value) {
                value_kinds[existing] = value_kind;
                if let Some(values) = nested_values.as_mut() {
                    values[existing] = None;
                }
            } else {
                key_values.push(key_value);
                value_kinds.push(value_kind);
                if let Some(values) = nested_values.as_mut() {
                    values.push(None);
                }
            }
            module.set_array_nested_value_metadata_at(
                outer_name,
                outer_entry_index,
                Some(NestedArrayMetadata {
                    layout: ArrayLayout::Assoc,
                    len: key_values.len(),
                    value_kinds: Some(value_kinds),
                    key_values: Some(key_values),
                    nested_values,
                }),
            );
            return;
        }
    }
    let Some(existing_kind) = homogeneous_nested_assoc_value_kind(inner_metadata) else {
        module.set_array_nested_value_metadata_at(outer_name, outer_entry_index, None);
        return;
    };
    if existing_kind != value_kind {
        module.set_array_nested_value_metadata_at(outer_name, outer_entry_index, None);
        return;
    }
    module.set_array_nested_value_metadata_at(
        outer_name,
        outer_entry_index,
        Some(NestedArrayMetadata {
            layout: ArrayLayout::Assoc,
            len: inner_metadata.len + 1,
            value_kinds: Some(vec![value_kind; inner_metadata.len + 1]),
            key_values: None,
            nested_values: None,
        }),
    );
}

fn update_deeper_dynamic_nested_assoc_assign_metadata(
    outer_name: &str,
    outer_key: &AssocAssignKey,
    middle_key: &AssocAssignKey,
    leaf_key: &AssocAssignKey,
    value_kind: ValueCellKind,
    module: &mut WasmModule,
) {
    let Some((_, outer_key_value)) = static_assoc_assign_key_metadata(outer_key) else {
        return;
    };
    let Some((_, middle_key_value)) = static_assoc_assign_key_metadata(middle_key) else {
        update_deeper_dynamic_runtime_middle_metadata(
            outer_name,
            outer_key,
            leaf_key,
            value_kind,
            module,
        );
        return;
    };
    let Some((_, leaf_key_value)) = static_assoc_assign_key_metadata(leaf_key) else {
        return;
    };
    let Some(outer_entry_index) = module
        .array_key_values(outer_name)
        .and_then(|keys| keys.iter().position(|key| *key == outer_key_value))
    else {
        module.set_array_nested_value_metadata(outer_name, None);
        return;
    };
    let Some(mut outer_metadata) = module.array_nested_value_metadata(outer_name, outer_entry_index) else {
        module.set_array_nested_value_metadata_at(outer_name, outer_entry_index, None);
        return;
    };
    let Some(middle_entry_index) = outer_metadata
        .key_values
        .as_ref()
        .and_then(|keys| keys.iter().position(|key| *key == middle_key_value))
    else {
        update_deeper_dynamic_static_middle_partial_metadata(
            outer_name,
            outer_entry_index,
            outer_metadata,
            leaf_key_value,
            value_kind,
            module,
        );
        return;
    };
    let Some(nested_values) = outer_metadata.nested_values.as_mut() else {
        module.set_array_nested_value_metadata_at(outer_name, outer_entry_index, None);
        return;
    };
    let Some(Some(mut leaf_metadata)) = nested_values.get(middle_entry_index).cloned() else {
        module.set_array_nested_value_metadata_at(outer_name, outer_entry_index, None);
        return;
    };
    update_assoc_leaf_metadata(&mut leaf_metadata, leaf_key_value, value_kind);
    nested_values[middle_entry_index] = Some(leaf_metadata);
    module.set_array_nested_value_metadata_at(outer_name, outer_entry_index, Some(outer_metadata));
}

fn update_deeper_dynamic_static_middle_partial_metadata(
    outer_name: &str,
    outer_entry_index: usize,
    mut outer_metadata: NestedArrayMetadata,
    leaf_key_value: AssocKeyValue,
    value_kind: ValueCellKind,
    module: &mut WasmModule,
) {
    let known_key_count = outer_metadata
        .key_values
        .as_ref()
        .map_or(0, Vec::len);
    if known_key_count >= outer_metadata.len {
        module.set_array_nested_value_metadata_at(outer_name, outer_entry_index, None);
        return;
    }
    let Some(nested_values) = outer_metadata.nested_values.as_mut() else {
        module.set_array_nested_value_metadata_at(outer_name, outer_entry_index, None);
        return;
    };
    let candidate_indexes = nested_values
        .iter()
        .enumerate()
        .skip(known_key_count)
        .filter_map(|(index, metadata)| {
            metadata
                .as_ref()
                .is_some_and(|metadata| metadata.layout == ArrayLayout::Assoc)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    let [middle_entry_index] = candidate_indexes.as_slice() else {
        module.set_array_nested_value_metadata_at(outer_name, outer_entry_index, None);
        return;
    };
    let Some(Some(mut leaf_metadata)) = nested_values.get(*middle_entry_index).cloned() else {
        module.set_array_nested_value_metadata_at(outer_name, outer_entry_index, None);
        return;
    };
    update_assoc_leaf_metadata(&mut leaf_metadata, leaf_key_value, value_kind);
    nested_values[*middle_entry_index] = Some(leaf_metadata);
    module.set_array_nested_value_metadata_at(outer_name, outer_entry_index, Some(outer_metadata));
}

fn update_deeper_dynamic_missing_middle_metadata(
    outer_name: &str,
    outer_entry_index: usize,
    parent_metadata: &NestedArrayMetadata,
    middle_key: &AssocAssignKey,
    leaf_key: &AssocAssignKey,
    value: &Expr,
    value_kind: ValueCellKind,
    module: &mut WasmModule,
) {
    let leaf_key_values = static_assoc_assign_key_metadata(leaf_key).map(|(_, key)| vec![key]);
    let leaf_nested_values = if value_kind == ValueCellKind::Array {
        Some(vec![nested_array_metadata_for_expr(value, module)])
    } else {
        Some(vec![None])
    };
    let new_leaf = NestedArrayMetadata {
        layout: ArrayLayout::Assoc,
        len: 1,
        value_kinds: Some(vec![value_kind]),
        key_values: leaf_key_values,
        nested_values: leaf_nested_values,
    };
    let new_parent = match static_assoc_assign_key_metadata(middle_key) {
        Some((_, middle_key_value)) if parent_metadata.key_values.is_some() || parent_metadata.len == 0 => {
            dynamic_missing_middle_metadata_with_static_key(parent_metadata, middle_key_value, new_leaf)
        }
        _ => dynamic_missing_middle_metadata_with_runtime_key(parent_metadata, new_leaf),
    };
    module.set_array_nested_value_metadata_at(
        outer_name,
        outer_entry_index,
        Some(new_parent),
    );
}

fn dynamic_missing_middle_metadata_with_static_key(
    parent_metadata: &NestedArrayMetadata,
    middle_key: AssocKeyValue,
    new_leaf: NestedArrayMetadata,
) -> NestedArrayMetadata {
    let mut key_values = parent_metadata.key_values.clone().unwrap_or_default();
    let mut value_kinds = parent_metadata
        .value_kinds
        .clone()
        .unwrap_or_else(|| vec![ValueCellKind::Array; parent_metadata.len]);
    let mut nested_values = parent_metadata
        .nested_values
        .clone()
        .unwrap_or_else(|| vec![None; parent_metadata.len]);
    key_values.push(middle_key);
    value_kinds.push(ValueCellKind::Array);
    nested_values.push(Some(new_leaf));
    NestedArrayMetadata {
        layout: ArrayLayout::Assoc,
        len: key_values.len(),
        value_kinds: Some(value_kinds),
        key_values: Some(key_values),
        nested_values: Some(nested_values),
    }
}

fn dynamic_missing_middle_metadata_with_runtime_key(
    parent_metadata: &NestedArrayMetadata,
    new_leaf: NestedArrayMetadata,
) -> NestedArrayMetadata {
    let mut nested_values = parent_metadata
        .nested_values
        .clone()
        .unwrap_or_else(|| vec![None; parent_metadata.len]);
    nested_values.push(Some(new_leaf));
    let value_kinds = if parent_metadata
        .value_kinds
        .as_ref()
        .is_some_and(|kinds| kinds.iter().all(|kind| *kind == ValueCellKind::Array))
    {
        Some(vec![ValueCellKind::Array; parent_metadata.len + 1])
    } else {
        None
    };
    NestedArrayMetadata {
        layout: ArrayLayout::Assoc,
        len: parent_metadata.len + 1,
        value_kinds,
        key_values: parent_metadata.key_values.clone(),
        nested_values: Some(nested_values),
    }
}

fn update_deeper_dynamic_runtime_middle_metadata(
    outer_name: &str,
    outer_key: &AssocAssignKey,
    leaf_key: &AssocAssignKey,
    value_kind: ValueCellKind,
    module: &mut WasmModule,
) {
    let Some((_, outer_key_value)) = static_assoc_assign_key_metadata(outer_key) else {
        return;
    };
    let Some((_, leaf_key_value)) = static_assoc_assign_key_metadata(leaf_key) else {
        return;
    };
    let Some(outer_entry_index) = module
        .array_key_values(outer_name)
        .and_then(|keys| keys.iter().position(|key| *key == outer_key_value))
    else {
        return;
    };
    let Some(mut parent_metadata) = module.array_nested_value_metadata(outer_name, outer_entry_index) else {
        return;
    };
    if parent_metadata.layout != ArrayLayout::Assoc || parent_metadata.key_values.is_some() {
        return;
    }
    let Some(mut nested_values) = parent_metadata
        .nested_values
        .as_ref()
        .and_then(|values| values.iter().cloned().collect::<Option<Vec<_>>>())
    else {
        return;
    };
    let Some(first) = nested_values.first().cloned() else {
        return;
    };
    if !nested_values.iter().all(|candidate| *candidate == first) {
        return;
    }
    let mut updated_leaf = first;
    update_assoc_leaf_metadata(&mut updated_leaf, leaf_key_value, value_kind);
    nested_values = vec![updated_leaf; parent_metadata.len];
    parent_metadata.nested_values = Some(nested_values.into_iter().map(Some).collect());
    module.set_array_nested_value_metadata_at(outer_name, outer_entry_index, Some(parent_metadata));
}

fn update_nested_array_push_metadata(
    outer_name: &str,
    outer_entry_index: usize,
    inner_metadata: &NestedArrayMetadata,
    leaf_entry_index: usize,
    value_kind: ValueCellKind,
    module: &mut WasmModule,
) {
    let Some(mut nested_values) = inner_metadata.nested_values.clone() else {
        module.set_array_nested_value_metadata_at(outer_name, outer_entry_index, None);
        return;
    };
    let Some(Some(leaf_metadata)) = nested_values.get(leaf_entry_index).cloned() else {
        module.set_array_nested_value_metadata_at(outer_name, outer_entry_index, None);
        return;
    };
    let mut value_kinds = leaf_metadata
        .value_kinds
        .unwrap_or_else(|| vec![value_kind; leaf_metadata.len]);
    value_kinds.push(value_kind);
    nested_values[leaf_entry_index] = Some(NestedArrayMetadata {
        layout: ArrayLayout::Value,
        len: leaf_metadata.len + 1,
        value_kinds: Some(value_kinds),
        key_values: None,
        nested_values: None,
    });
    module.set_array_nested_value_metadata_at(
        outer_name,
        outer_entry_index,
        Some(NestedArrayMetadata {
            layout: inner_metadata.layout,
            len: inner_metadata.len,
            value_kinds: inner_metadata.value_kinds.clone(),
            key_values: inner_metadata.key_values.clone(),
            nested_values: Some(nested_values),
        }),
    );
}

fn update_nested_assoc_leaf_push_metadata(
    outer_name: &str,
    outer_entry_index: usize,
    inner_metadata: &NestedArrayMetadata,
    leaf_entry_index: usize,
    appended_key: i64,
    value_kind: ValueCellKind,
    module: &mut WasmModule,
) {
    let Some(mut nested_values) = inner_metadata.nested_values.clone() else {
        module.set_array_nested_value_metadata_at(outer_name, outer_entry_index, None);
        return;
    };
    let Some(Some(leaf_metadata)) = nested_values.get(leaf_entry_index).cloned() else {
        module.set_array_nested_value_metadata_at(outer_name, outer_entry_index, None);
        return;
    };
    let mut key_values = leaf_metadata.key_values.unwrap_or_default();
    key_values.push(AssocKeyValue::Int(appended_key));
    let mut value_kinds = leaf_metadata
        .value_kinds
        .unwrap_or_else(|| vec![value_kind; leaf_metadata.len]);
    value_kinds.push(value_kind);
    let mut leaf_nested_values = leaf_metadata.nested_values;
    if let Some(values) = leaf_nested_values.as_mut() {
        values.push(None);
    }
    nested_values[leaf_entry_index] = Some(NestedArrayMetadata {
        layout: ArrayLayout::Assoc,
        len: leaf_metadata.len + 1,
        value_kinds: Some(value_kinds),
        key_values: Some(key_values),
        nested_values: leaf_nested_values,
    });
    module.set_array_nested_value_metadata_at(
        outer_name,
        outer_entry_index,
        Some(NestedArrayMetadata {
            layout: inner_metadata.layout,
            len: inner_metadata.len,
            value_kinds: inner_metadata.value_kinds.clone(),
            key_values: inner_metadata.key_values.clone(),
            nested_values: Some(nested_values),
        }),
    );
}

fn update_assoc_leaf_metadata(
    metadata: &mut NestedArrayMetadata,
    key: AssocKeyValue,
    value_kind: ValueCellKind,
) {
    if metadata.layout == ArrayLayout::Assoc
        && metadata.len == 0
        && metadata.key_values.is_none()
        && metadata.value_kinds.is_none()
    {
        metadata.len = 1;
        metadata.key_values = Some(vec![key]);
        metadata.value_kinds = Some(vec![value_kind]);
        metadata.nested_values = Some(vec![None]);
        return;
    }
    let Some(keys) = metadata.key_values.as_mut() else {
        metadata.value_kinds = None;
        metadata.nested_values = None;
        return;
    };
    let Some(kinds) = metadata.value_kinds.as_mut() else {
        metadata.key_values = None;
        metadata.nested_values = None;
        return;
    };
    if let Some(index) = keys.iter().position(|candidate| *candidate == key) {
        kinds[index] = value_kind;
        if let Some(nested_values) = metadata.nested_values.as_mut() {
            nested_values[index] = None;
        }
        return;
    }
    keys.push(key);
    kinds.push(value_kind);
    metadata.len += 1;
    if let Some(nested_values) = metadata.nested_values.as_mut() {
        nested_values.push(None);
    }
}
