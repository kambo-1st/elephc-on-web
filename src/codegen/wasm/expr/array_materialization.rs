//! Purpose:
//! Lowers array copy and return-stack materialization helpers for wasm32-web.
//! Keeps local array copying and function return array materialization out of the expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` assignment and statement lowering paths.
//!
//! Key details:
//! - Copy helpers must preserve array layout, value-cell metadata, key metadata, and runtime ownership.

use super::*;
use super::array_transform_sets::emit_assoc_entry_address;

pub(in crate::codegen::wasm) fn array_access_array_metadata(
    value: &Expr,
    module: &WasmModule,
) -> Option<NestedArrayMetadata> {
    nested_array_metadata_for_access_expr(value, module)
        .or_else(|| dynamic_nested_array_value_metadata(value, module))
}

pub(in crate::codegen::wasm) fn emit_indexed_array_copy_assign(
    name: &str,
    source: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) == ArrayLayout::Assoc {
        return emit_assoc_array_copy_assign(name, source, module);
    }
    if module.array_layout(source) == ArrayLayout::Value {
        return emit_value_array_copy_assign(name, source, module);
    }
    if let Some(len) = module.array_length(source) {
        module.set_array_length(name, len);
    }
    module.set_array_layout(name, ArrayLayout::CompactInt);
    module.set_array_nested_value_metadata(name, None);
    let source_ptr = module.next_label("array_copy_source_ptr");
    let source_len = module.next_label("array_copy_source_len");
    let index = module.next_label("array_copy_index");
    let done_label = module.next_label("array_copy_done");
    let loop_label = module.next_label("array_copy_loop");
    for local in [&source_ptr, &source_len, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.set {}", source_ptr));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", source_len));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get {}", source_len));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", source_ptr));
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
    Ok(())
}

pub(in crate::codegen::wasm) fn emit_assoc_array_copy_assign(
    name: &str,
    source: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(len) = module.array_length(source) {
        module.set_array_length(name, len);
    }
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(
        name,
        module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec()),
    );
    module.set_array_value_constants(
        name,
        module.array_value_constants(source).map(|values| values.to_vec()),
    );
    module.set_array_nested_value_metadata(
        name,
        module
            .array_nested_value_metadata_items(source)
            .map(|metadata| metadata.to_vec()),
    );
    module.set_array_runtime_nested_value_metadata(
        name,
        module.array_runtime_nested_value_metadata(source),
    );
    module.set_array_key_kinds(name, module.array_key_kinds(source).map(|kinds| kinds.to_vec()));
    module.set_array_key_values(
        name,
        module.array_key_values(source).map(|values| values.to_vec()),
    );
    let source_ptr = module.next_label("assoc_array_copy_source_ptr");
    let source_len = module.next_label("assoc_array_copy_source_len");
    let index = module.next_label("assoc_array_copy_index");
    let target_entry = module.next_label("assoc_array_copy_target_entry");
    let source_entry = module.next_label("assoc_array_copy_source_entry");
    let done_label = module.next_label("assoc_array_copy_done");
    let loop_label = module.next_label("assoc_array_copy_loop");
    for local in [&source_ptr, &source_len, &index, &target_entry, &source_entry] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.set {}", source_ptr));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", source_len));
    if name != source {
        emit_release_current_assoc_array(name, module);
    }
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_assoc_entry_address(&format!("${}_ptr", name), &index, &target_entry, module);
    emit_assoc_entry_address(&source_ptr, &index, &source_entry, module);
    copy_assoc_entry(&target_entry, &source_entry, module);
    module.body().line(&format!("local.get {}", target_entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line("call $__rt_value_retain");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(in crate::codegen::wasm) fn emit_value_array_copy_assign(
    name: &str,
    source: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(len) = module.array_length(source) {
        module.set_array_length(name, len);
        module.set_array_value_cell_kinds(name, Some(vec![ValueCellKind::Int; len]));
        module.set_array_runtime_value_cell_kind(name, None);
    } else {
        module.clear_array_length(name);
        module.set_array_value_cell_kinds(name, None);
        module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Int));
    }
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(
        name,
        module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec()),
    );
    module.set_array_nested_value_metadata(
        name,
        module
            .array_nested_value_metadata_items(source)
            .map(|metadata| metadata.to_vec()),
    );
    module.set_array_runtime_value_cell_kind(name, module.array_runtime_value_cell_kind(source));
    module.set_array_runtime_nested_value_metadata(name, module.array_runtime_nested_value_metadata(source));
    module.set_array_value_constants(
        name,
        module.array_value_constants(source).map(|values| values.to_vec()),
    );
    module.set_array_object_classes(
        name,
        module.array_object_classes(source).map(|classes| classes.to_vec()),
    );
    let source_ptr = module.next_label("value_array_copy_source_ptr");
    let source_len = module.next_label("value_array_copy_source_len");
    let index = module.next_label("value_array_copy_index");
    let done_label = module.next_label("value_array_copy_done");
    let loop_label = module.next_label("value_array_copy_loop");
    for local in [&source_ptr, &source_len, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.set {}", source_ptr));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", source_len));
    emit_release_current_value_array(name, module);
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", source_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_copy_value_cell(name, &source_ptr, &index, &index, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

pub(crate) fn emit_array_value_to_stack(
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match &value.kind {
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Array) => {
            module.body().line(&format!("local.get ${}_ptr", name));
            module.body().line(&format!("local.get ${}_len", name));
            Ok(())
        }
        ExprKind::ArrayLiteral(items) => {
            let ptr = module.next_label("array_return_ptr");
            module.declare_i32_local(ptr.trim_start_matches('$').to_string());
            module.body().line("global.get $heap");
            module.body().line(&format!("local.set {}", ptr));
            module.body().line("global.get $heap");
            module.body().line(&format!("i32.const {}", items.len() * 8));
            module.body().line("i32.add");
            module.body().line("global.set $heap");
            for (index, item) in items.iter().enumerate() {
                module.body().line(&format!("local.get {}", ptr));
                module.body().line(&format!("i32.const {}", index * 8));
                module.body().line("i32.add");
                require_int(item, module)?;
                module.body().line("i64.store");
            }
            module.body().line(&format!("local.get {}", ptr));
            module.body().line(&format!("i32.const {}", items.len()));
            Ok(())
        }
        ExprKind::FunctionCall { name, args }
            if module.has_function(name) && module.function_return_kind(name) == Some(ValueKind::Array) =>
        {
            emit_user_function_args(value, name, args, module)?;
            module
                .body()
                .line(&format!("call ${}", wasm_function_name(name)));
            Ok(())
        }
        ExprKind::MethodCall { object, method, .. }
        | ExprKind::NullsafeMethodCall { object, method, .. }
            if method_call_array_return_metadata(object, method, module).is_some() =>
        {
            match emit_expr(value, module)? {
                ValueKind::Array => Ok(()),
                _ => unreachable!("array-returning method metadata must emit an array value"),
            }
        }
        ExprKind::FunctionCall { name, .. }
            if name.eq_ignore_ascii_case("array_fill") || name.eq_ignore_ascii_case("array_map") =>
        {
            emit_materialized_array_return_to_stack("array_return", value, module)
        }
        ExprKind::ArrayLiteralAssoc(_) => Err(array_unsupported(value)),
        _ => Err(array_unsupported(value)),
    }
}

pub(crate) fn emit_return_array_value_to_stack(
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match module.current_array_return_layout() {
        ArrayLayout::CompactInt => emit_array_value_to_stack(value, module),
        ArrayLayout::Value => emit_value_array_return_to_stack(value, module),
        ArrayLayout::Assoc => emit_assoc_array_return_to_stack(value, module),
    }
}

fn emit_assoc_array_return_to_stack(
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match &value.kind {
        ExprKind::ArrayLiteralAssoc(items) => {
            let temp = module.next_label("assoc_array_return").trim_start_matches('$').to_string();
            module.declare_array_local(temp.clone());
            emit_assoc_array_items_assign(&temp, items, module)?;
            module.body().line(&format!("local.get ${}_ptr", temp));
            module.body().line(&format!("local.get ${}_len", temp));
            Ok(())
        }
        ExprKind::Variable(name)
            if module.local_kind(name) == Some(LocalKind::Array)
                && module.array_layout(name) == ArrayLayout::Assoc =>
        {
            module.body().line(&format!("local.get ${}_ptr", name));
            module.body().line(&format!("local.get ${}_len", name));
            Ok(())
        }
        ExprKind::FunctionCall { name, .. }
            if name.eq_ignore_ascii_case("array_fill") || name.eq_ignore_ascii_case("array_filter") =>
        {
            emit_materialized_array_return_to_stack("assoc_array_return", value, module)
        }
        _ => Err(array_unsupported(value)),
    }
}

fn emit_value_array_return_to_stack(
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match &value.kind {
        ExprKind::ArrayLiteral(items) => {
            let temp = module.next_label("value_array_return").trim_start_matches('$').to_string();
            module.declare_array_local(temp.clone());
            emit_value_array_items_assign(&temp, items, module)?;
            module.body().line(&format!("local.get ${}_ptr", temp));
            module.body().line(&format!("local.get ${}_len", temp));
            Ok(())
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Array) => {
            if module.array_layout(name) == ArrayLayout::Value {
                module.body().line(&format!("local.get ${}_ptr", name));
                module.body().line(&format!("local.get ${}_len", name));
                Ok(())
            } else {
                let temp = module.next_label("value_array_return").trim_start_matches('$').to_string();
                module.declare_array_local(temp.clone());
                emit_compact_array_to_value_array_assign(&temp, name, module)?;
                module.body().line(&format!("local.get ${}_ptr", temp));
                module.body().line(&format!("local.get ${}_len", temp));
                Ok(())
            }
        }
        ExprKind::FunctionCall { name, .. }
            if name.eq_ignore_ascii_case("array_pad")
                || name.eq_ignore_ascii_case("array_merge")
                || name.eq_ignore_ascii_case("array_fill")
                || name.eq_ignore_ascii_case("array_map")
                || name.eq_ignore_ascii_case("explode")
                || name.eq_ignore_ascii_case("str_split") =>
        {
            emit_materialized_array_return_to_stack("value_array_return", value, module)
        }
        _ => Err(array_unsupported(value)),
    }
}

fn emit_materialized_array_return_to_stack(
    prefix: &str,
    value: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let temp = module.next_label(prefix).trim_start_matches('$').to_string();
    module.declare_array_local(temp.clone());
    emit_array_assign(&temp, value, module)?;
    module.body().line(&format!("local.get ${}_ptr", temp));
    module.body().line(&format!("local.get ${}_len", temp));
    Ok(())
}
