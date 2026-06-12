//! Purpose:
//! Emits wasm32-web helper paths for `array_unshift` over associative and value arrays.
//! Keeps unshift-specific reindexing and metadata updates separate from other mutators.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_mutators`
//!
//! Key details:
//! - Preserves PHP reindex behavior, value-cell copies, and associative key metadata updates.

use super::*;
use super::array_value_cells::reject_value_array_mutator;

pub(super) fn emit_array_unshift_call(
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if args.len() < 2 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_unshift() expects an array and at least one value",
        ));
    }
    let ExprKind::Variable(name) = &args[0].kind else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_unshift() requires an assigned indexed array variable",
        ));
    };
    if module.local_kind(&name) == Some(LocalKind::Mixed) && module.mixed_value_cell_kind(&name).is_none() {
        for value in args[1..].iter().rev() {
            super::super::stmt::emit_unknown_mixed_array_unshift(name, value, module)?;
        }
        module.body().line(&format!("local.get ${name}"));
        module.body().line("i32.const 12");
        module.body().line("i32.add");
        module.body().line("i32.load");
        module.body().line("i64.extend_i32_u");
        return Ok(ValueKind::Int);
    }
    if module.local_kind(name) != Some(LocalKind::Array) {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_unshift() first argument must be an indexed array",
        ));
    }
    if module.array_layout(name) == ArrayLayout::Assoc {
        return emit_assoc_array_unshift_call(expr, args, module, name);
    }
    if module.array_layout(name) == ArrayLayout::Value {
        return emit_value_array_unshift_call(expr, args, module, name);
    }
    reject_value_array_mutator(args[0].span, name, "array_unshift", module)?;
    let Some(len) = module.array_length(name) else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_unshift() requires a known indexed array length",
        ));
    };
    let mut value_temps = Vec::new();
    for (index, value) in args[1..].iter().enumerate() {
        let temp = module.next_label(&format!("array_unshift_value_{index}"));
        module.declare_i64_local(temp.trim_start_matches('$').to_string());
        require_int(value, module)?;
        module.body().line(&format!("local.set {}", temp));
        value_temps.push(temp);
    }
    let source_ptr = preserve_array_ptr(name, "array_unshift", module);
    let out_len = len + value_temps.len();
    emit_array_alloc_prelude(name, out_len, module);
    for (index, temp) in value_temps.iter().enumerate() {
        module.body().line(&format!("local.get ${name}_ptr"));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line(&format!("local.get {}", temp));
        module.body().line("i64.store");
    }
    for source_index in 0..len {
        emit_array_store_load(
            &name,
            value_temps.len() + source_index,
            &source_ptr,
            source_index,
            module,
        );
    }
    module.body().line(&format!("i64.const {}", out_len));
    Ok(ValueKind::Int)
}

fn emit_assoc_array_unshift_call(
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
    name: &str,
) -> Result<ValueKind, CompileError> {
    if args.len() < 2 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_unshift() expects an array and at least one value",
        ));
    }
    let Some(len) = module.array_length(name) else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_unshift() requires a known associative array length",
        ));
    };
    let inserted = args.len() - 1;
    let out_len = len + inserted;
    let old_ptr = preserve_array_ptr(name, "assoc_array_unshift", module);
    let temp_ptr = module.next_label("assoc_array_unshift_temp_ptr");
    let source_index = module.next_label("assoc_array_unshift_source_index");
    let source_entry = module.next_label("assoc_array_unshift_source_entry");
    let target_entry = module.next_label("assoc_array_unshift_target_entry");
    let target_index = module.next_label("assoc_array_unshift_target_index");
    let next_int_key = module.next_label("assoc_array_unshift_next_int_key");
    let key_kind = module.next_label("assoc_array_unshift_key_kind");
    let value_cell = module.next_label("assoc_array_unshift_value_cell");
    let temp_cell = module.next_label("assoc_array_unshift_temp_cell");
    let done_label = module.next_label("assoc_array_unshift_done");
    let loop_label = module.next_label("assoc_array_unshift_loop");
    for local in [
        &temp_ptr,
        &source_index,
        &source_entry,
        &target_entry,
        &target_index,
        &key_kind,
        &value_cell,
        &temp_cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(next_int_key.trim_start_matches('$').to_string());

    module.body().line(&format!("i32.const {}", inserted));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", temp_ptr));
    for (index, value) in args[1..].iter().enumerate() {
        module.body().line(&format!("local.get {}", temp_ptr));
        module
            .body()
            .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", temp_cell));
        emit_store_value_cell(&temp_cell, value, module)?;
    }

    module.body().line(&format!("i32.const {}", out_len));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    for index in 0..inserted {
        emit_assoc_entry_address_const_index(&format!("${}_ptr", name), index, &target_entry, module);
        module.body().line(&format!("local.get {}", target_entry));
        module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
        module.body().line("i32.store");
        module.body().line(&format!("local.get {}", target_entry));
        module.body().line("i32.const 8");
        module.body().line("i32.add");
        module.body().line(&format!("i64.const {}", index));
        module.body().line("i64.store");
        emit_assoc_value_cell_address(&target_entry, &value_cell, module);
        module.body().line(&format!("local.get {}", temp_ptr));
        module
            .body()
            .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", temp_cell));
        module.body().line(&format!("local.get {}", value_cell));
        module.body().line(&format!("local.get {}", temp_cell));
        module.body().line("call $__rt_value_copy");
    }

    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line(&format!("i32.const {}", inserted));
    module.body().line(&format!("local.set {}", target_index));
    module.body().line(&format!("i64.const {}", inserted));
    module.body().line(&format!("local.set {}", next_int_key));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", source_index));
    module.body().line(&format!("i32.const {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_assoc_entry_address(&old_ptr, &source_index, &source_entry, module);
    emit_assoc_entry_address(&format!("${}_ptr", name), &target_index, &target_entry, module);
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.load");
    module.body().line(&format!("local.set {}", key_kind));
    module.body().line(&format!("local.get {}", key_kind));
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.eq");
    module.body().open("if");
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
    module.body().line("else");
    copy_assoc_entry_key(&target_entry, &source_entry, module);
    module.body().close("end");
    copy_assoc_entry_value(&target_entry, &source_entry, module);
    module.body().line(&format!("local.get {}", source_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", source_index));
    module.body().line(&format!("local.get {}", target_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", target_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("i32.const {}", out_len));
    module.body().line(&format!("local.set ${}_len", name));

    let inserted_kinds: Vec<_> = args[1..]
        .iter()
        .map(|value| value_cell_kind_for_expr(value, module))
        .collect::<Option<_>>()
        .unwrap_or_default();
    let value_kinds = module.array_value_cell_kinds(name).and_then(|kinds| {
        if inserted_kinds.len() != inserted {
            return None;
        }
        let mut out = inserted_kinds.clone();
        out.extend_from_slice(kinds);
        Some(out)
    });
    let key_kinds = module.array_key_kinds(name).map(|kinds| {
        let mut out = vec![AssocKeyKind::Int; inserted];
        out.extend_from_slice(kinds);
        out
    });
    let key_values = key_kinds
        .as_ref()
        .zip(module.array_key_values(name))
        .map(|(kinds, values)| {
            let mut raw_values = (0..inserted)
                .map(|index| AssocKeyValue::Int(index as i64))
                .collect::<Vec<_>>();
            raw_values.extend_from_slice(values);
            reindexed_assoc_key_values(kinds, &raw_values)
        });
    module.set_array_length(name, out_len);
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_key_kinds(name, key_kinds);
    module.set_array_key_values(name, key_values);
    module.set_array_value_cell_kinds(name, value_kinds);
    module.set_array_value_constants(name, None);
    module.body().line(&format!("i64.const {}", out_len));
    Ok(ValueKind::Int)
}

fn emit_value_array_unshift_call(
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
    name: &str,
) -> Result<ValueKind, CompileError> {
    if args.len() < 2 {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web array_unshift() expects an array and at least one value",
        ));
    }
    let Some(len) = module.array_length(name) else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web array_unshift() requires a known indexed array length",
        ));
    };
    let inserted = args.len() - 1;
    let old_ptr = preserve_array_ptr(name, "value_array_unshift", module);
    let temp_ptr = module.next_label("value_array_unshift_temp_ptr");
    let temp_cell = module.next_label("value_array_unshift_temp_cell");
    let target_cell = module.next_label("value_array_unshift_target_cell");
    for local in [&temp_ptr, &temp_cell, &target_cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("i32.const {}", inserted));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set {}", temp_ptr));
    for (index, value) in args[1..].iter().enumerate() {
        module.body().line(&format!("local.get {}", temp_ptr));
        module
            .body()
            .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", temp_cell));
        emit_store_value_cell(&temp_cell, value, module)?;
    }
    let inserted_kinds: Vec<_> = args[1..]
        .iter()
        .map(|value| value_cell_kind_for_expr(value, module))
        .collect::<Option<_>>()
        .unwrap_or_default();
    let value_kinds = module.array_value_cell_kinds(name).and_then(|kinds| {
        if inserted_kinds.len() != inserted {
            return None;
        }
        let mut out = inserted_kinds.clone();
        out.extend_from_slice(kinds);
        Some(out)
    });
    let out_len = len + inserted;
    module.body().line(&format!("i32.const {}", out_len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("i32.const {}", out_len));
    module.body().line(&format!("local.set ${}_len", name));
    for index in 0..inserted {
        module.body().line(&format!("local.get ${}_ptr", name));
        module.body().line(&format!("i32.const {}", index));
        module.body().line("call $__rt_value_cell");
        module.body().line(&format!("local.set {}", target_cell));
        module.body().line(&format!("local.get {}", temp_ptr));
        module
            .body()
            .line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE));
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", temp_cell));
        module.body().line(&format!("local.get {}", target_cell));
        module.body().line(&format!("local.get {}", temp_cell));
        module.body().line("call $__rt_value_copy");
    }
    for source_index in 0..len {
        emit_copy_static_value_cell(
            name,
            inserted + source_index,
            &old_ptr,
            source_index,
            module,
        );
    }
    module.set_array_length(name, out_len);
    module.set_array_layout(name, ArrayLayout::Value);
    module.set_array_value_cell_kinds(name, value_kinds);
    module.set_array_value_constants(name, None);
    module.body().line(&format!("i64.const {}", out_len));
    Ok(ValueKind::Int)
}
