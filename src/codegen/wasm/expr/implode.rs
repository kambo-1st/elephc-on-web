//! Purpose:
//! Lowers PHP implode() for wasm32-web string values and direct output.
//! Keeps separator handling and array-value emission loops together.
//!
//! Called from:
//! - `crate::codegen::wasm::expr`
//! - `crate::codegen::wasm::expr::string_builtins`
//!
//! Key details:
//! - Supports compact int, value-cell, runtime scalar, and associative array sources.
//! - Emits CompileError for array shapes whose element metadata is not sufficient.

use super::*;
use super::implode_runtime::{
    emit_implode_runtime_scalar_array_value_to_stack, emit_implode_runtime_string_array_value_to_stack,
};

pub(super) fn emit_implode_string_builtin_value_to_stack(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<bool, CompileError> {
    if !name.eq_ignore_ascii_case("implode") {
        return Ok(false);
    }
    let (separator, array) = match args {
        [array] => (StringValueImplodeSeparator::Static(String::new()), array),
        [separator, array] => {
            let separator = if let Some(separator) = static_or_tracked_string_value(separator, module) {
                StringValueImplodeSeparator::Static(separator)
            } else if let Some(separator) =
                runtime_string_arg_or_materialize(separator, "string_value_implode_separator", module)?
            {
                StringValueImplodeSeparator::Local(separator)
            } else {
                return Err(CompileError::new(
                    separator.span,
                    "wasm32-web implode() separator must be a string",
                ));
            };
            (separator, array)
        }
        _ => {
            return Err(CompileError::new(
                call.span,
                "wasm32-web implode() expects one or two arguments",
            ));
        }
    };
    let temp;
    let array_name = match &array.kind {
        ExprKind::Variable(array_name) if module.local_kind(array_name) == Some(LocalKind::Array) => {
            array_name.as_str()
        }
        ExprKind::ArrayLiteral(_) => {
            temp = module
                .next_label("implode_direct_literal_array")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            temp.as_str()
        }
        ExprKind::ArrayLiteralAssoc(_) => {
            temp = module
                .next_label("implode_direct_assoc_array")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            temp.as_str()
        }
        ExprKind::FunctionCall { name, .. }
            if module.has_function(name)
                && module.function_return_kind(name) == Some(ValueKind::Array)
                && matches!(
                    module.function_array_return_layout(name),
                    ArrayLayout::Value | ArrayLayout::Assoc
                ) =>
        {
            temp = module
                .next_label("implode_return_array")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            temp.as_str()
        }
        ExprKind::FunctionCall { name, .. }
            if name.eq_ignore_ascii_case("explode")
                || name.eq_ignore_ascii_case("str_split")
                || name.eq_ignore_ascii_case("range") =>
        {
            temp = module
                .next_label("implode_direct_array")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            temp.as_str()
        }
        ExprKind::MethodCall { object, method, .. }
            if method_call_array_return_metadata(object, method, module).is_some() =>
        {
            temp = module
                .next_label("implode_method_array")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            temp.as_str()
        }
        ExprKind::NullsafeMethodCall { object, method, .. }
            if object_expr_is_known_non_null(object, module)
                && method_call_array_return_metadata(object, method, module).is_some() =>
        {
            temp = module
                .next_label("implode_nullsafe_method_array")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            temp.as_str()
        }
        ExprKind::StaticMethodCall { receiver, method, .. }
            if static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            temp = module
                .next_label("implode_static_method_array")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            temp.as_str()
        }
        ExprKind::DynamicStaticMethodCall { receiver, method, .. }
            if dynamic_static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            temp = module
                .next_label("implode_dynamic_static_method_array")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            temp.as_str()
        }
        ExprKind::ArrayAccess { .. } if nested_array_metadata_for_access_expr(array, module).is_some() => {
            temp = materialize_nested_implode_source(array, module)?;
            temp.as_str()
        }
        _ => return Ok(false),
    };
    if module.array_layout(array_name) == ArrayLayout::CompactInt {
        emit_implode_compact_int_array_value_to_stack(array_name, &separator, module);
        return Ok(true);
    }
    if module.array_layout(array_name) == ArrayLayout::Assoc {
        emit_implode_assigned_assoc_array_value_to_stack(array_name, &separator, module)?;
        return Ok(true);
    }
    if module.array_layout(array_name) != ArrayLayout::Value {
        return Err(CompileError::new(
            array.span,
            "wasm32-web implode() string values currently require a value-cell array",
        ));
    }
    if module.array_length(array_name).is_none() || module.array_value_cell_kinds(array_name).is_none() {
        match module.array_runtime_value_cell_kind(array_name) {
            Some(ValueCellKind::Str) => {
                emit_implode_runtime_string_array_value_to_stack(array_name, &separator, module);
                return Ok(true);
            }
            Some(
                ValueCellKind::Int
                | ValueCellKind::Float
                | ValueCellKind::Bool
                | ValueCellKind::Null,
            ) => {
                emit_implode_runtime_scalar_array_value_to_stack(
                    array_name,
                    &separator,
                    module.array_runtime_value_cell_kind(array_name)
                        .expect("runtime value-cell kind matched above"),
                    module,
                );
                return Ok(true);
            }
            Some(ValueCellKind::Array) | None => {}
        }
    }
    let Some(len) = module.array_length(array_name) else {
        return Err(CompileError::new(
            array.span,
            "wasm32-web implode() string values currently require a known array length",
        ));
    };
    let Some(kinds) = module.array_value_cell_kinds(array_name).map(|kinds| kinds.to_vec()) else {
        return Err(CompileError::new(
            array.span,
            "wasm32-web implode() string values currently require known element types",
        ));
    };
    if kinds.len() != len
        || kinds
            .iter()
            .any(|kind| matches!(kind, ValueCellKind::Array))
    {
        return Err(CompileError::new(
            array.span,
            "wasm32-web implode() string values currently support scalar elements",
        ));
    }
    emit_implode_assigned_scalar_array_value_to_stack(array_name, len, &separator, &kinds, module);
    Ok(true)
}

fn materialize_nested_implode_source(
    source: &Expr,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let Some(metadata) = nested_array_metadata_for_access_expr(source, module) else {
        return Err(CompileError::new(
            source.span,
            "wasm32-web implode() requires nested array metadata",
        ));
    };
    let temp = module
        .next_label("implode_nested_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    match emit_expr(source, module)? {
        ValueKind::Array => {
            module.body().line(&format!("local.set ${}_len", temp));
            module.body().line(&format!("local.set ${}_ptr", temp));
        }
        _ => {
            return Err(CompileError::new(
                source.span,
                "wasm32-web implode() expected a nested array value",
            ));
        }
    }
    let key_values = metadata.key_values.clone();
    module.set_array_layout(&temp, metadata.layout);
    module.set_array_length(&temp, metadata.len);
    module.set_array_value_cell_kinds(&temp, metadata.value_kinds);
    module.set_array_key_values(&temp, key_values.clone());
    module.set_array_key_kinds(
        &temp,
        key_values.map(|keys| {
            keys.into_iter()
                .map(|key| match key {
                    AssocKeyValue::Int(_) => AssocKeyKind::Int,
                    AssocKeyValue::Str(_) => AssocKeyKind::Str,
                })
                .collect()
        }),
    );
    module.set_array_php_normalized_runtime_keys(
        &temp,
        metadata.layout == ArrayLayout::Assoc && module.array_key_kinds(&temp).is_none(),
    );
    module.set_array_nested_value_metadata(&temp, metadata.nested_values);
    Ok(temp)
}

pub(super) enum StringValueImplodeSeparator {
    Static(String),
    Local(String),
}

fn emit_implode_assigned_assoc_array_value_to_stack(
    array_name: &str,
    separator: &StringValueImplodeSeparator,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(kinds) = module.array_value_cell_kinds(array_name).map(|kinds| kinds.to_vec()) else {
        emit_implode_mixed_assoc_array_value_to_stack(array_name, separator, module);
        return Ok(());
    };
    let Some(len) = module.array_length(array_name) else {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            "wasm32-web implode() over associative arrays requires a known length",
        ));
    };
    if kinds.len() != len || kinds.iter().any(|kind| matches!(kind, ValueCellKind::Array)) {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            "wasm32-web implode() over associative arrays currently supports scalar values",
        ));
    }
    let out_ptr = module.next_label("implode_assoc_out_ptr");
    let out_len = module.next_label("implode_assoc_out_len");
    let out_idx = module.next_label("implode_assoc_out_idx");
    let entry = module.next_label("implode_assoc_entry");
    let cell = module.next_label("implode_assoc_cell");
    let part = module
        .next_label("implode_assoc_part")
        .trim_start_matches('$')
        .to_string();
    let separator_local = module
        .next_label("implode_assoc_separator")
        .trim_start_matches('$')
        .to_string();
    let true_local = module
        .next_label("implode_assoc_true")
        .trim_start_matches('$')
        .to_string();
    for local in [&out_ptr, &out_len, &out_idx, &entry, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i32_local(format!("{}_ptr", part));
    module.declare_i32_local(format!("{}_len", part));
    module.declare_i32_local(format!("{}_ptr", separator_local));
    module.declare_i32_local(format!("{}_len", separator_local));
    module.declare_i32_local(format!("{}_ptr", true_local));
    module.declare_i32_local(format!("{}_len", true_local));
    match separator {
        StringValueImplodeSeparator::Static(separator) => {
            let (separator_ptr, separator_len) = module.intern_string(separator);
            module.body().line(&format!("i32.const {}", separator_ptr));
            module.body().line(&format!("local.set ${}_ptr", separator_local));
            module.body().line(&format!("i32.const {}", separator_len));
            module.body().line(&format!("local.set ${}_len", separator_local));
        }
        StringValueImplodeSeparator::Local(separator) => {
            module.body().line(&format!("local.get ${}_ptr", separator));
            module.body().line(&format!("local.set ${}_ptr", separator_local));
            module.body().line(&format!("local.get ${}_len", separator));
            module.body().line(&format!("local.set ${}_len", separator_local));
        }
    }
    let (true_ptr, true_len) = module.intern_string("1");
    module.body().line(&format!("i32.const {}", true_ptr));
    module.body().line(&format!("local.set ${}_ptr", true_local));
    module.body().line(&format!("i32.const {}", true_len));
    module.body().line(&format!("local.set ${}_len", true_local));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_len));
    if len > 1 {
        module.body().line(&format!("local.get {}", out_len));
        module.body().line(&format!("local.get ${}_len", separator_local));
        module.body().line(&format!("i32.const {}", len - 1));
        module.body().line("i32.mul");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", out_len));
    }
    for (index, kind) in kinds.iter().copied().enumerate().take(len) {
        module.body().line(&format!("local.get {}", out_len));
        match kind {
            ValueCellKind::Int => module.body().line("i32.const 20"),
            ValueCellKind::Float => module.body().line("i32.const 64"),
            ValueCellKind::Bool => module.body().line("i32.const 1"),
            ValueCellKind::Null => module.body().line("i32.const 0"),
            ValueCellKind::Str => {
                module.body().line(&format!("local.get ${}_ptr", array_name));
                module.body().line(&format!("i32.const {}", index));
                module.body().line("call $__rt_assoc_entry");
                module.body().line("call $__rt_assoc_value_cell");
                module.body().line("i32.const 12");
                module.body().line("i32.add");
                module.body().line("i32.load");
            }
            ValueCellKind::Array => {
                unreachable!("unsupported implode assoc value-cell kind rejected before emission")
            }
        }
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", out_len));
    }
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
    for (index, kind) in kinds.iter().copied().enumerate().take(len) {
        if index > 0 {
            emit_copy_string_to_memory(&separator_local, &out_ptr, &out_idx, module);
        }
        module.body().line(&format!("local.get ${}_ptr", array_name));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_assoc_entry");
        module.body().line(&format!("local.set {}", entry));
        module.body().line(&format!("local.get {}", entry));
        module.body().line("call $__rt_assoc_value_cell");
        module.body().line(&format!("local.set {}", cell));
        match kind {
            ValueCellKind::Int => {
                module.body().line(&format!("local.get {}", cell));
                module.body().line("i32.const 8");
                module.body().line("i32.add");
                module.body().line("i64.load");
                emit_i64_stack_string_cast_value_to_stack("implode_assoc_int_part", module);
                module.body().line(&format!("local.set ${}_len", part));
                module.body().line(&format!("local.set ${}_ptr", part));
                emit_copy_string_to_memory(&part, &out_ptr, &out_idx, module);
            }
            ValueCellKind::Float => {
                module.body().line(&format!("local.get {}", cell));
                module.body().line("i32.const 8");
                module.body().line("i32.add");
                module.body().line("f64.load");
                emit_f64_stack_string_cast_value_to_stack("implode_assoc_float_part", module);
                module.body().line(&format!("local.set ${}_len", part));
                module.body().line(&format!("local.set ${}_ptr", part));
                emit_copy_string_to_memory(&part, &out_ptr, &out_idx, module);
            }
            ValueCellKind::Bool => {
                module.body().line(&format!("local.get {}", cell));
                module.body().line("i32.const 8");
                module.body().line("i32.add");
                module.body().line("i64.load");
                module.body().line("i64.const 0");
                module.body().line("i64.ne");
                module.body().open("if");
                emit_copy_string_to_memory(&true_local, &out_ptr, &out_idx, module);
                module.body().close("end");
            }
            ValueCellKind::Null => {}
            ValueCellKind::Str => {
                module.body().line(&format!("local.get {}", cell));
                module.body().line("i32.const 8");
                module.body().line("i32.add");
                module.body().line("i32.load");
                module.body().line(&format!("local.set ${}_ptr", part));
                module.body().line(&format!("local.get {}", cell));
                module.body().line("i32.const 12");
                module.body().line("i32.add");
                module.body().line("i32.load");
                module.body().line(&format!("local.set ${}_len", part));
                emit_copy_string_to_memory(&part, &out_ptr, &out_idx, module);
            }
            ValueCellKind::Array => {
                unreachable!("unsupported implode assoc value-cell kind rejected before emission")
            }
        }
    }
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    Ok(())
}

fn emit_implode_mixed_assoc_array_value_to_stack(
    array_name: &str,
    separator: &StringValueImplodeSeparator,
    module: &mut WasmModule,
) {
    let out_ptr = module.next_label("implode_mixed_assoc_out_ptr");
    let out_len = module.next_label("implode_mixed_assoc_out_len");
    let out_idx = module.next_label("implode_mixed_assoc_out_idx");
    let index = module.next_label("implode_mixed_assoc_index");
    let entry = module.next_label("implode_mixed_assoc_entry");
    let cell = module.next_label("implode_mixed_assoc_cell");
    let tag = module.next_label("implode_mixed_assoc_tag");
    let part = module
        .next_label("implode_mixed_assoc_part")
        .trim_start_matches('$')
        .to_string();
    let separator_local = module
        .next_label("implode_mixed_assoc_separator")
        .trim_start_matches('$')
        .to_string();
    let true_local = module
        .next_label("implode_mixed_assoc_true")
        .trim_start_matches('$')
        .to_string();
    for local in [&out_ptr, &out_len, &out_idx, &index, &entry, &cell, &tag] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i32_local(format!("{}_ptr", part));
    module.declare_i32_local(format!("{}_len", part));
    module.declare_i32_local(format!("{}_ptr", separator_local));
    module.declare_i32_local(format!("{}_len", separator_local));
    module.declare_i32_local(format!("{}_ptr", true_local));
    module.declare_i32_local(format!("{}_len", true_local));
    match separator {
        StringValueImplodeSeparator::Static(separator) => {
            let (separator_ptr, separator_len) = module.intern_string(separator);
            module.body().line(&format!("i32.const {}", separator_ptr));
            module.body().line(&format!("local.set ${}_ptr", separator_local));
            module.body().line(&format!("i32.const {}", separator_len));
            module.body().line(&format!("local.set ${}_len", separator_local));
        }
        StringValueImplodeSeparator::Local(separator) => {
            module.body().line(&format!("local.get ${}_ptr", separator));
            module.body().line(&format!("local.set ${}_ptr", separator_local));
            module.body().line(&format!("local.get ${}_len", separator));
            module.body().line(&format!("local.set ${}_len", separator_local));
        }
    }
    let (true_ptr, true_len) = module.intern_string("1");
    module.body().line(&format!("i32.const {}", true_ptr));
    module.body().line(&format!("local.set ${}_ptr", true_local));
    module.body().line(&format!("i32.const {}", true_len));
    module.body().line(&format!("local.set ${}_len", true_local));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.get ${}_len", array_name));
    module.body().line("i32.const 1");
    module.body().line("i32.gt_u");
    module.body().open("if");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.get ${}_len", separator_local));
    module.body().line(&format!("local.get ${}_len", array_name));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_len));
    module.body().close("end");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    let len_loop = module.next_label("implode_mixed_assoc_len_loop");
    let len_done = module.next_label("implode_mixed_assoc_len_done");
    module.body().open(&format!("block {}", len_done));
    module.body().open(&format!("loop {}", len_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", array_name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", len_done));
    emit_implode_mixed_assoc_cell_at_index(array_name, &index, &entry, &cell, module);
    emit_implode_mixed_assoc_load_tag(&cell, &tag, module);
    emit_implode_mixed_assoc_add_dynamic_cell_len(&cell, &tag, &out_len, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", len_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    let copy_loop = module.next_label("implode_mixed_assoc_copy_loop");
    let copy_done = module.next_label("implode_mixed_assoc_copy_done");
    module.body().open(&format!("block {}", copy_done));
    module.body().open(&format!("loop {}", copy_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", array_name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", copy_done));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 0");
    module.body().line("i32.gt_u");
    module.body().open("if");
    emit_copy_string_to_memory(&separator_local, &out_ptr, &out_idx, module);
    module.body().close("end");
    emit_implode_mixed_assoc_cell_at_index(array_name, &index, &entry, &cell, module);
    emit_implode_mixed_assoc_load_tag(&cell, &tag, module);
    emit_implode_mixed_assoc_copy_dynamic_cell(&cell, &tag, &part, &true_local, &out_ptr, &out_idx, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", copy_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
}

fn emit_implode_mixed_assoc_cell_at_index(
    array_name: &str,
    index: &str,
    entry: &str,
    cell: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", array_name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_assoc_entry");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("local.get {}", entry));
    module.body().line("call $__rt_assoc_value_cell");
    module.body().line(&format!("local.set {}", cell));
}

fn emit_implode_mixed_assoc_load_tag(cell: &str, tag: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", tag));
}

fn emit_implode_mixed_assoc_add_dynamic_cell_len(
    cell: &str,
    tag: &str,
    out_len: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_implode_mixed_assoc_add_len_const(20, out_len, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_implode_mixed_assoc_add_len_const(64, out_len, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().open("if");
    emit_implode_mixed_assoc_add_len_const(1, out_len, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

fn emit_implode_mixed_assoc_add_len_const(len: i32, out_len: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", out_len));
    module.body().line(&format!("i32.const {}", len));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_len));
}

fn emit_implode_mixed_assoc_copy_dynamic_cell(
    cell: &str,
    tag: &str,
    part: &str,
    true_local: &str,
    out_ptr: &str,
    out_idx: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    emit_i64_stack_string_cast_value_to_stack("implode_mixed_assoc_int_part", module);
    module.body().line(&format!("local.set ${}_len", part));
    module.body().line(&format!("local.set ${}_ptr", part));
    emit_copy_string_to_memory(part, out_ptr, out_idx, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("f64.load");
    emit_f64_stack_string_cast_value_to_stack("implode_mixed_assoc_float_part", module);
    module.body().line(&format!("local.set ${}_len", part));
    module.body().line(&format!("local.set ${}_ptr", part));
    emit_copy_string_to_memory(part, out_ptr, out_idx, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().open("if");
    emit_copy_string_to_memory(true_local, out_ptr, out_idx, module);
    module.body().close("end");
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set ${}_ptr", part));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set ${}_len", part));
    emit_copy_string_to_memory(part, out_ptr, out_idx, module);
    module.body().line("else");
    module.body().line(&format!("local.get {}", tag));
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

fn emit_implode_compact_int_array_value_to_stack(
    array_name: &str,
    separator: &StringValueImplodeSeparator,
    module: &mut WasmModule,
) {
    let out_ptr = module.next_label("implode_int_out_ptr");
    let out_idx = module.next_label("implode_int_out_idx");
    let alloc_len = module.next_label("implode_int_alloc_len");
    let index = module.next_label("implode_int_index");
    let part = module
        .next_label("implode_int_part")
        .trim_start_matches('$')
        .to_string();
    let separator_local = module
        .next_label("implode_int_separator")
        .trim_start_matches('$')
        .to_string();
    let loop_label = module.next_label("implode_int_loop");
    let done_label = module.next_label("implode_int_done");
    for local in [&out_ptr, &out_idx, &alloc_len, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i32_local(format!("{}_ptr", part));
    module.declare_i32_local(format!("{}_len", part));
    module.declare_i32_local(format!("{}_ptr", separator_local));
    module.declare_i32_local(format!("{}_len", separator_local));
    match separator {
        StringValueImplodeSeparator::Static(separator) => {
            let (separator_ptr, separator_len) = module.intern_string(separator);
            module.body().line(&format!("i32.const {}", separator_ptr));
            module.body().line(&format!("local.set ${}_ptr", separator_local));
            module.body().line(&format!("i32.const {}", separator_len));
            module.body().line(&format!("local.set ${}_len", separator_local));
        }
        StringValueImplodeSeparator::Local(separator) => {
            module.body().line(&format!("local.get ${}_ptr", separator));
            module.body().line(&format!("local.set ${}_ptr", separator_local));
            module.body().line(&format!("local.get ${}_len", separator));
            module.body().line(&format!("local.set ${}_len", separator_local));
        }
    }
    module.body().line(&format!("local.get ${}_len", array_name));
    module.body().line("i32.const 20");
    module.body().line(&format!("local.get ${}_len", separator_local));
    module.body().line("i32.add");
    module.body().line("i32.mul");
    module.body().line(&format!("local.set {}", alloc_len));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", alloc_len));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", array_name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 0");
    module.body().line("i32.gt_u");
    module.body().open("if");
    emit_copy_string_to_memory(&separator_local, &out_ptr, &out_idx, module);
    module.body().close("end");
    module.body().line(&format!("local.get ${}_ptr", array_name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    emit_i64_stack_string_cast_value_to_stack("implode_int_part", module);
    module.body().line(&format!("local.set ${}_len", part));
    module.body().line(&format!("local.set ${}_ptr", part));
    emit_copy_string_to_memory(&part, &out_ptr, &out_idx, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
}

fn emit_implode_assigned_scalar_array_value_to_stack(
    array_name: &str,
    len: usize,
    separator: &StringValueImplodeSeparator,
    kinds: &[ValueCellKind],
    module: &mut WasmModule,
) {
    let out_ptr = module.next_label("implode_out_ptr");
    let out_len = module.next_label("implode_out_len");
    let out_idx = module.next_label("implode_out_idx");
    let part = module.next_label("implode_part").trim_start_matches('$').to_string();
    let separator_local = module.next_label("implode_separator").trim_start_matches('$').to_string();
    for local in [&out_ptr, &out_len, &out_idx] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i32_local(format!("{}_ptr", part));
    module.declare_i32_local(format!("{}_len", part));
    module.declare_i32_local(format!("{}_ptr", separator_local));
    module.declare_i32_local(format!("{}_len", separator_local));
    match separator {
        StringValueImplodeSeparator::Static(separator) => {
            let (separator_ptr, separator_len) = module.intern_string(separator);
            module.body().line(&format!("i32.const {}", separator_ptr));
            module.body().line(&format!("local.set ${}_ptr", separator_local));
            module.body().line(&format!("i32.const {}", separator_len));
            module.body().line(&format!("local.set ${}_len", separator_local));
        }
        StringValueImplodeSeparator::Local(separator) => {
            module.body().line(&format!("local.get ${}_ptr", separator));
            module.body().line(&format!("local.set ${}_ptr", separator_local));
            module.body().line(&format!("local.get ${}_len", separator));
            module.body().line(&format!("local.set ${}_len", separator_local));
        }
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_len));
    if len > 1 {
        module.body().line(&format!("local.get {}", out_len));
        module.body().line(&format!("local.get ${}_len", separator_local));
        module.body().line(&format!("i32.const {}", len - 1));
        module.body().line("i32.mul");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", out_len));
    }
    for (index, kind) in kinds.iter().copied().enumerate().take(len) {
        module.body().line(&format!("local.get {}", out_len));
        match kind {
            ValueCellKind::Int => module.body().line("i32.const 20"),
            ValueCellKind::Float => module.body().line("i32.const 64"),
            ValueCellKind::Bool => module.body().line("i32.const 1"),
            ValueCellKind::Null => module.body().line("i32.const 0"),
            ValueCellKind::Str => {
                emit_value_array_static_payload_addr(array_name, index, module);
                module.body().line("i32.const 4");
                module.body().line("i32.add");
                module.body().line("i32.load");
            }
            ValueCellKind::Array => {
                unreachable!("unsupported implode value-cell kind rejected before emission")
            }
        }
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", out_len));
    }
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", out_len));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
    let true_local = module.next_label("implode_true").trim_start_matches('$').to_string();
    module.declare_i32_local(format!("{}_ptr", true_local));
    module.declare_i32_local(format!("{}_len", true_local));
    let (true_ptr, true_len) = module.intern_string("1");
    module.body().line(&format!("i32.const {}", true_ptr));
    module.body().line(&format!("local.set ${}_ptr", true_local));
    module.body().line(&format!("i32.const {}", true_len));
    module.body().line(&format!("local.set ${}_len", true_local));
    for (index, kind) in kinds.iter().copied().enumerate().take(len) {
        if index > 0 {
            emit_copy_string_to_memory(&separator_local, &out_ptr, &out_idx, module);
        }
        match kind {
            ValueCellKind::Int => {
                emit_value_array_static_payload_addr(array_name, index, module);
                module.body().line("i64.load");
                emit_i64_stack_string_cast_value_to_stack("implode_value_int_part", module);
                module.body().line(&format!("local.set ${}_len", part));
                module.body().line(&format!("local.set ${}_ptr", part));
                emit_copy_string_to_memory(&part, &out_ptr, &out_idx, module);
            }
            ValueCellKind::Float => {
                emit_value_array_static_payload_addr(array_name, index, module);
                module.body().line("f64.load");
                emit_f64_stack_string_cast_value_to_stack("implode_value_float_part", module);
                module.body().line(&format!("local.set ${}_len", part));
                module.body().line(&format!("local.set ${}_ptr", part));
                emit_copy_string_to_memory(&part, &out_ptr, &out_idx, module);
            }
            ValueCellKind::Bool => {
                emit_value_array_static_payload_addr(array_name, index, module);
                module.body().line("i64.load");
                module.body().line("i64.const 0");
                module.body().line("i64.ne");
                module.body().open("if");
                emit_copy_string_to_memory(&true_local, &out_ptr, &out_idx, module);
                module.body().close("end");
            }
            ValueCellKind::Null => {}
            ValueCellKind::Str => {
                emit_value_array_static_payload_addr(array_name, index, module);
                module.body().line("i32.load");
                module.body().line(&format!("local.set ${}_ptr", part));
                emit_value_array_static_payload_addr(array_name, index, module);
                module.body().line("i32.const 4");
                module.body().line("i32.add");
                module.body().line("i32.load");
                module.body().line(&format!("local.set ${}_len", part));
                emit_copy_string_to_memory(&part, &out_ptr, &out_idx, module);
            }
            ValueCellKind::Array => {
                unreachable!("unsupported implode value-cell kind rejected before emission")
            }
        }
    }
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_idx));
}

pub(super) use super::implode_output::*;
