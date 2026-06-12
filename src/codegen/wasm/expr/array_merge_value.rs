//! Purpose:
//! Lowers wasm32-web value-cell and dynamic indexed array_merge() helpers.
//! Keeps runtime-length and mixed-value merge copying separate from assoc merge lowering.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_merge`.
//!
//! Key details:
//! - Preserves value-cell metadata, nested array metadata, and runtime ownership copies.

use super::*;

pub(super) fn indexed_array_merge_needs_value_layout(args: &[Expr], module: &WasmModule) -> bool {
    args.iter().any(|arg| match &arg.kind {
        ExprKind::ArrayLiteral(items) => array_literal_needs_value_cells(items),
        ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
            module.array_layout(source) == ArrayLayout::Value
        }
        ExprKind::FunctionCall { name, .. }
            if module.has_function(name) && module.function_return_kind(name) == Some(ValueKind::Array) =>
        {
            module.function_array_return_layout(name) == ArrayLayout::Value
        }
        ExprKind::FunctionCall { .. } if expression_has_array_type(arg, module) => true,
        _ => false,
    })
}

pub(super) fn indexed_array_merge_needs_dynamic_value_layout(args: &[Expr], module: &WasmModule) -> bool {
    indexed_array_merge_needs_value_layout(args, module)
        && args.iter().any(|arg| match &arg.kind {
            ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
                module.array_length(source).is_none()
            }
            ExprKind::FunctionCall { name, .. }
                if module.has_function(name)
                    && module.function_return_kind(name) == Some(ValueKind::Array) =>
            {
                module.function_array_return_length(name).is_none()
            }
            ExprKind::FunctionCall { .. } if expression_has_array_type(arg, module) => true,
            _ => false,
        })
}

pub(super) fn emit_value_array_merge_assign(
    name: &str,
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let mut sources = Vec::new();
    let mut total_len = 0usize;
    let mut merged_value_kinds = Some(Vec::new());
    let mut merged_value_constants = Some(Vec::new());
    let mut merged_nested_values = Some(Vec::new());
    for arg in args {
        match &arg.kind {
            ExprKind::ArrayLiteral(items) => {
                total_len += items.len();
                append_optional_value_kinds(
                    &mut merged_value_kinds,
                    value_cell_kinds_for_items(items, module),
                );
                append_optional_value_constants(
                    &mut merged_value_constants,
                    value_cell_constants_for_items(items, module),
                );
                append_optional_nested_values(
                    &mut merged_nested_values,
                    nested_array_metadata_for_items(items, module),
                );
                sources.push(ValueArrayMergeSource::Static(items.clone()));
            }
            ExprKind::ArrayLiteralAssoc(_) => return Err(array_unsupported(arg)),
            ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
                let Some(len) = module.array_length(source) else {
                    return Err(CompileError::new(
                        arg.span,
                        "wasm32-web value-array array_merge() requires known indexed array lengths",
                    ));
                };
                total_len += len;
                let source_value_kinds = match module.array_layout(source) {
                    ArrayLayout::Value => module.array_value_cell_kinds(source).map(|kinds| kinds.to_vec()),
                    ArrayLayout::CompactInt => Some(vec![ValueCellKind::Int; len]),
                    ArrayLayout::Assoc => None,
                };
                append_optional_value_kinds(&mut merged_value_kinds, source_value_kinds);
                append_optional_value_constants(
                    &mut merged_value_constants,
                    module.array_value_constants(source).map(|values| values.to_vec()),
                );
                append_optional_nested_values(
                    &mut merged_nested_values,
                    module
                        .array_nested_value_metadata_items(source)
                        .map(|metadata| metadata.to_vec()),
                );
                let source_ptr = preserve_array_ptr(source, "array_merge", module);
                sources.push(ValueArrayMergeSource::Runtime {
                    ptr: source_ptr,
                    len,
                    layout: module.array_layout(source),
                });
            }
            ExprKind::FunctionCall {
                name: function_name,
                args: call_args,
            } if module.has_function(function_name)
                && module.function_return_kind(function_name) == Some(ValueKind::Array) =>
            {
                let Some(len) = module.function_array_return_length(function_name) else {
                    return Err(CompileError::new(
                        arg.span,
                        "wasm32-web value-array array_merge() requires known function return lengths",
                    ));
                };
                total_len += len;
                emit_user_function_args(arg, function_name, call_args, module)?;
                module
                    .body()
                    .line(&format!("call ${}", wasm_function_name(function_name)));
                let source_ptr = module.next_label("array_merge_return_ptr");
                let source_len = module.next_label("array_merge_return_len");
                for local in [&source_ptr, &source_len] {
                    module.declare_i32_local(local.trim_start_matches('$').to_string());
                }
                module.body().line(&format!("local.set {}", source_len));
                module.body().line(&format!("local.set {}", source_ptr));
                append_optional_value_kinds(
                    &mut merged_value_kinds,
                    module
                        .function_array_return_value_kinds(function_name)
                        .map(|kinds| kinds.to_vec()),
                );
                append_optional_value_constants(&mut merged_value_constants, None);
                append_optional_nested_values(
                    &mut merged_nested_values,
                    module
                        .function_array_return_nested_values(function_name)
                        .map(|metadata| metadata.to_vec()),
                );
                sources.push(ValueArrayMergeSource::Runtime {
                    ptr: source_ptr,
                    len,
                    layout: module.function_array_return_layout(function_name),
                });
            }
            _ => {
                return Err(CompileError::new(
                    arg.span,
                    "wasm32-web value-array array_merge() currently supports indexed literals and assigned array variables only",
                ));
            }
        }
    }
    module.set_array_length(name, total_len);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, merged_value_kinds);
    module.set_array_value_constants(name, merged_value_constants);
    module.set_array_nested_value_metadata(name, merged_nested_values);
    module.body().line(&format!("i32.const {}", total_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", total_len));
    module.body().line(&format!("local.set ${}_len", name));
    let mut out_index = 0usize;
    for source in sources {
        match source {
            ValueArrayMergeSource::Static(items) => {
                for item in items {
                    emit_value_array_store_expr(name, out_index, &item, module)?;
                    out_index += 1;
                }
            }
            ValueArrayMergeSource::Runtime {
                ptr,
                len,
                layout: ArrayLayout::Value,
            } => {
                for source_index in 0..len {
                    emit_copy_static_value_cell(name, out_index, &ptr, source_index, module);
                    out_index += 1;
                }
            }
            ValueArrayMergeSource::Runtime {
                ptr,
                len,
                layout: ArrayLayout::CompactInt,
            } => {
                for source_index in 0..len {
                    emit_static_int_slot_as_value_cell(name, out_index, &ptr, source_index, module);
                    out_index += 1;
                }
            }
            ValueArrayMergeSource::Runtime {
                layout: ArrayLayout::Assoc,
                ..
            } => return Err(array_unsupported(expr)),
        }
    }
    let _ = expr;
    Ok(())
}

pub(super) fn emit_dynamic_value_array_merge_assign(
    name: &str,
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let mut sources = Vec::new();
    let total_len = module.next_label("value_array_merge_total_len");
    let out_index = module.next_label("value_array_merge_out_index");
    let copy_index = module.next_label("value_array_merge_copy_index");
    for local in [&total_len, &out_index, &copy_index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", total_len));
    for arg in args {
        if matches!(arg.kind, ExprKind::ArrayLiteralAssoc(_)) {
            return Err(array_unsupported(arg));
        }
        let ptr = module.next_label("value_array_merge_source_ptr");
        let len = module.next_label("value_array_merge_source_len");
        for local in [&ptr, &len] {
            module.declare_i32_local(local.trim_start_matches('$').to_string());
        }
        let layout = match &arg.kind {
            ExprKind::ArrayLiteral(items) => {
                let temp = module
                    .next_label("value_array_merge_literal")
                    .trim_start_matches('$')
                    .to_string();
                module.declare_array_local(temp.clone());
                let layout = if array_literal_needs_value_cells(items) {
                    emit_value_array_items_assign(&temp, items, module)?;
                    ArrayLayout::Value
                } else {
                    emit_static_array_items_assign(&temp, items, module)?;
                    ArrayLayout::CompactInt
                };
                module.body().line(&format!("local.get ${}_ptr", temp));
                module.body().line(&format!("local.set {}", ptr));
                module.body().line(&format!("local.get ${}_len", temp));
                module.body().line(&format!("local.set {}", len));
                layout
            }
            ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
                module.body().line(&format!("local.get ${}_ptr", source));
                module.body().line(&format!("local.set {}", ptr));
                module.body().line(&format!("local.get ${}_len", source));
                module.body().line(&format!("local.set {}", len));
                module.array_layout(source)
            }
            ExprKind::FunctionCall { name: function_name, args: call_args }
                if module.has_function(function_name)
                    && module.function_return_kind(function_name) == Some(ValueKind::Array) =>
            {
                emit_user_function_args(arg, function_name, call_args, module)?;
                module
                    .body()
                    .line(&format!("call ${}", wasm_function_name(function_name)));
                module.body().line(&format!("local.set {}", len));
                module.body().line(&format!("local.set {}", ptr));
                module.function_array_return_layout(function_name)
            }
            ExprKind::FunctionCall { .. } if expression_has_array_type(arg, module) => {
                let source = materialize_array_map_multi_source(arg, "value_array_merge_source", module)?;
                module.body().line(&format!("local.get ${}_ptr", source));
                module.body().line(&format!("local.set {}", ptr));
                module.body().line(&format!("local.get ${}_len", source));
                module.body().line(&format!("local.set {}", len));
                module.array_layout(&source)
            }
            _ => {
                return Err(CompileError::new(
                    arg.span,
                    "wasm32-web value-array array_merge() currently supports indexed array values only",
                ));
            }
        };
        if layout == ArrayLayout::Assoc {
            return Err(array_unsupported(arg));
        }
        module.body().line(&format!("local.get {}", total_len));
        module.body().line(&format!("local.get {}", len));
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", total_len));
        sources.push(ValueArrayDynamicMergeSource { ptr, len, layout });
    }
    module.clear_array_length(name);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, None);
    module.body().line(&format!("local.get {}", total_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", total_len));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    for source in sources {
        emit_dynamic_value_array_merge_source(name, &source, &out_index, &copy_index, module);
    }
    let _ = expr;
    Ok(())
}

pub(super) struct ValueArrayDynamicMergeSource {
    ptr: String,
    len: String,
    layout: ArrayLayout,
}

pub(super) fn emit_dynamic_value_array_merge_source(
    name: &str,
    source: &ValueArrayDynamicMergeSource,
    out_index: &str,
    copy_index: &str,
    module: &mut WasmModule,
) {
    let done_label = module.next_label("value_array_merge_copy_done");
    let loop_label = module.next_label("value_array_merge_copy_loop");
    let dest_cell = module.next_label("value_array_merge_dest_cell");
    module.declare_i32_local(dest_cell.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", copy_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", copy_index));
    module.body().line(&format!("local.get {}", source.len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("call $__rt_value_cell");
    module.body().line(&format!("local.set {}", dest_cell));
    match source.layout {
        ArrayLayout::Value => {
            module.body().line(&format!("local.get {}", dest_cell));
            module.body().line(&format!("local.get {}", source.ptr));
            module.body().line(&format!("local.get {}", copy_index));
            module.body().line("call $__rt_value_cell");
            module.body().line("call $__rt_value_copy");
        }
        ArrayLayout::CompactInt => {
            module.body().line(&format!("local.get {}", dest_cell));
            module.body().line(&format!("local.get {}", source.ptr));
            module.body().line(&format!("local.get {}", copy_index));
            module.body().line("i32.const 8");
            module.body().line("i32.mul");
            module.body().line("i32.add");
            module.body().line("i64.load");
            module.body().line("call $__rt_value_store_int");
        }
        ArrayLayout::Assoc => {}
    }
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    module.body().line(&format!("local.get {}", copy_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", copy_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn append_optional_value_kinds(
    target: &mut Option<Vec<ValueCellKind>>,
    source: Option<Vec<ValueCellKind>>,
) {
    match (target, source) {
        (Some(target), Some(source)) => target.extend(source),
        (target, _) => *target = None,
    }
}

pub(super) fn append_optional_value_constants(
    target: &mut Option<Vec<ConstantValue>>,
    source: Option<Vec<ConstantValue>>,
) {
    match (target, source) {
        (Some(target), Some(source)) => target.extend(source),
        (target, _) => *target = None,
    }
}

pub(super) fn append_optional_nested_values(
    target: &mut Option<Vec<Option<NestedArrayMetadata>>>,
    source: Option<Vec<Option<NestedArrayMetadata>>>,
) {
    match (target, source) {
        (Some(target), Some(source)) => target.extend(source),
        (target, _) => *target = None,
    }
}

enum ValueArrayMergeSource {
    Static(Vec<Expr>),
    Runtime {
        ptr: String,
        len: usize,
        layout: ArrayLayout,
    },
}

pub(super) fn indexed_array_merge_needs_dynamic(arg: &Expr, module: &WasmModule) -> bool {
    match &arg.kind {
        ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
            module.array_length(source).is_none()
        }
        ExprKind::FunctionCall { name, .. } => module.function_return_kind(name) == Some(ValueKind::Array),
        _ => false,
    }
}

pub(super) fn emit_dynamic_indexed_array_merge_assign(
    name: &str,
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let mut sources = Vec::new();
    for arg in args {
        if matches!(arg.kind, ExprKind::ArrayLiteralAssoc(_)) {
            return Err(array_unsupported(arg));
        }
        if !expression_is_arrayy(arg, module) {
            return Err(CompileError::new(
                arg.span,
                "wasm32-web array_merge() currently supports indexed array values only",
            ));
        }
    }
    let total_len = module.next_label("array_merge_total_len");
    let out_index = module.next_label("array_merge_out_index");
    let copy_index = module.next_label("array_merge_copy_index");
    for local in [&total_len, &out_index, &copy_index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", total_len));
    for arg in args {
        let ptr = module.next_label("array_merge_source_ptr");
        let len = module.next_label("array_merge_source_len");
        for local in [&ptr, &len] {
            module.declare_i32_local(local.trim_start_matches('$').to_string());
        }
        emit_array_value_to_stack(arg, module)?;
        module.body().line(&format!("local.set {}", len));
        module.body().line(&format!("local.set {}", ptr));
        module.body().line(&format!("local.get {}", total_len));
        module.body().line(&format!("local.get {}", len));
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", total_len));
        sources.push((ptr, len));
    }
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", total_len));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get {}", total_len));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    for (ptr, len) in sources {
        let done_label = module.next_label("array_merge_copy_done");
        let loop_label = module.next_label("array_merge_copy_loop");
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", copy_index));
        module.body().open(&format!("block {}", done_label));
        module.body().open(&format!("loop {}", loop_label));
        module.body().line(&format!("local.get {}", copy_index));
        module.body().line(&format!("local.get {}", len));
        module.body().line("i32.ge_u");
        module.body().line(&format!("br_if {}", done_label));
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("local.get {}", out_index));
        module.body().line("i32.const 8");
        module.body().line("i32.mul");
        module.body().line("i32.add");
        module.body().line(&format!("local.get {}", ptr));
        module.body().line(&format!("local.get {}", copy_index));
        module.body().line("i32.const 8");
        module.body().line("i32.mul");
        module.body().line("i32.add");
        module.body().line("i64.load");
        module.body().line("i64.store");
        module.body().line(&format!("local.get {}", out_index));
        module.body().line("i32.const 1");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", out_index));
        module.body().line(&format!("local.get {}", copy_index));
        module.body().line("i32.const 1");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", copy_index));
        module.body().line(&format!("br {}", loop_label));
        module.body().close("end");
        module.body().close("end");
    }
    let _ = expr;
    Ok(())
}
