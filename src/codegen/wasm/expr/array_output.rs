//! Purpose:
//! Emits wasm32-web output paths for PHP arrays, including array truthiness,
//! `echo`-style array markers, and array-index output helpers.
//!
//! Called from:
//! - `super::emit_expr()` and `super::emit_output_expr()`.
//!
//! Key details:
//! - Array output preserves PHP's visible `Array` marker behavior while scalar
//!   index output delegates to the same value-cell access helpers used elsewhere.
//! - Missing or unsupported dynamic shapes still report compile errors instead of
//!   falling back to integer-only output.

use super::*;

pub(super) fn emit_array_truthiness(expr: &Expr, module: &mut WasmModule) -> Result<(), CompileError> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => {
            module.body().line(&format!("i32.const {}", i32::from(!items.is_empty())));
            Ok(())
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            module.body().line(&format!("i32.const {}", i32::from(!items.is_empty())));
            Ok(())
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Array) => {
            module.body().line(&format!("local.get ${}_len", name));
            module.body().line("i32.const 0");
            module.body().line("i32.ne");
            Ok(())
        }
        ExprKind::FunctionCall { .. } if expression_is_arrayy(expr, module) => {
            let len = module.next_label("array_truthy_len");
            module.declare_i32_local(len.trim_start_matches('$').to_string());
            emit_array_value_to_stack(expr, module)?;
            module.body().line(&format!("local.set {}", len));
            module.body().line("drop");
            module.body().line(&format!("local.get {}", len));
            module.body().line("i32.const 0");
            module.body().line("i32.ne");
            Ok(())
        }
        _ if expression_is_arrayy(expr, module) || expression_has_array_type(expr, module) => {
            let temp = module
                .next_label("array_truthy_expr")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, expr, module)?;
            module.body().line(&format!("local.get ${}_len", temp));
            module.body().line("i32.const 0");
            module.body().line("i32.ne");
            Ok(())
        }
        _ => Err(array_unsupported(expr)),
    }
}

pub(super) fn emit_output_array_marker(module: &mut WasmModule) {
    let (ptr, len) = module.intern_string("Array");
    module.body().line(&format!("i32.const {}", ptr));
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $host_write");
}

pub(super) fn emit_output_array_index(
    expr: &Expr,
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    match &array.kind {
        ExprKind::ArrayLiteral(items) => {
            let Some(index) = static_or_const_int_value(index) else {
                return Err(CompileError::new(
                    index.span,
                    "wasm32-web array literal access requires a static integer index",
                ));
            };
            let Ok(index) = usize::try_from(index) else {
                return Ok(());
            };
            let Some(item) = items.get(index) else {
                return Ok(());
            };
            require_int(item, module)?;
            module.body().line("call $host_write_int");
            Ok(())
        }
        ExprKind::ConstRef(name) => {
            let Some(index) = static_or_const_int_value(index) else {
                return Err(CompileError::new(
                    index.span,
                    "wasm32-web array constant access requires a static integer index",
                ));
            };
            let Ok(index) = usize::try_from(index) else {
                return Ok(());
            };
            let Some(ConstantArrayValue::Indexed(items)) = module.array_constant_value(name) else {
                return Err(array_unsupported(array));
            };
            let Some(item) = items.get(index) else {
                return Ok(());
            };
            require_int(item, module)?;
            module.body().line("call $host_write_int");
            Ok(())
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Array) => {
            if module.array_layout(name) == ArrayLayout::Assoc {
                return emit_output_assoc_array_local_index(expr, name, index, module);
            }
            if module.array_layout(name) == ArrayLayout::Value {
                return emit_output_value_array_local_index(expr, name, index, module);
            }
            emit_output_array_local_index(expr, name, index, module)
        }
        ExprKind::FunctionCall { name, .. }
            if module.has_function(name)
                && module.function_return_kind(name) == Some(ValueKind::Array)
                && matches!(
                    module.function_array_return_layout(name),
                    ArrayLayout::Assoc | ArrayLayout::Value
                ) =>
        {
            let temp = module
                .next_label("output_return_array")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                return emit_output_assoc_array_local_index(expr, &temp, index, module);
            }
            emit_output_value_array_local_index(expr, &temp, index, module)
        }
        _ if expression_is_arrayy(array, module) => {
            emit_output_array_value_index(expr, array, index, module)
        }
        _ if expression_has_array_type(array, module) => {
            let temp = module
                .next_label("output_direct_array_access")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, array, module)?;
            if module.array_layout(&temp) == ArrayLayout::Assoc {
                return emit_output_assoc_array_local_index(expr, &temp, index, module);
            }
            if module.array_layout(&temp) == ArrayLayout::Value {
                return emit_output_value_array_local_index(expr, &temp, index, module);
            }
            emit_output_array_local_index(expr, &temp, index, module)
        }
        _ => Err(array_unsupported(array)),
    }
}

fn emit_output_assoc_array_local_index(
    expr: &Expr,
    name: &str,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(key_value) = static_or_const_int_value(index) {
        return emit_output_assoc_array_local_int_key(expr, name, key_value, module);
    }
    if let Some(key_value) = static_string_value(index, module) {
        return emit_output_assoc_array_local_string_key(expr, name, &key_value, module);
    }
    if let ExprKind::Variable(key_name) = &index.kind {
        if module.local_kind(key_name) == Some(LocalKind::Mixed) {
            return emit_output_assoc_array_local_mixed_key(expr, name, key_name, module);
        }
    }
    if expression_is_stringy(index, module) {
        let key_ptr = module.next_label("assoc_array_key_ptr");
        let key_len = module.next_label("assoc_array_key_len");
        module.declare_i32_local(key_ptr.trim_start_matches('$').to_string());
        module.declare_i32_local(key_len.trim_start_matches('$').to_string());
        emit_string_value_to_stack(index, module)?;
        module.body().line(&format!("local.set {}", key_len));
        module.body().line(&format!("local.set {}", key_ptr));
        return emit_output_assoc_array_local_string_key_parts(
            expr,
            name,
            &key_ptr,
            &key_len,
            module,
        );
    }
    let key = module.next_label("assoc_array_key");
    module.declare_i64_local(key.trim_start_matches('$').to_string());
    require_int(index, module)?;
    module.body().line(&format!("local.set {}", key));
    emit_output_assoc_array_local_int_key_parts(expr, name, &key, module)
}

fn emit_output_assoc_array_local_mixed_key(
    expr: &Expr,
    name: &str,
    key_name: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let cell_local = module
        .next_label("assoc_array_mixed_key_output_cell")
        .trim_start_matches('$')
        .to_string();
    let cell = format!("${}", cell_local);
    module.declare_i32_local(cell_local.clone());
    emit_alloc_mixed_cell(&cell_local, module);
    emit_copy_assoc_access_to_value_cell_by_mixed_key(&cell, name, key_name, module);
    emit_output_value_cell(&cell, module);
    let _ = expr;
    Ok(())
}

fn emit_output_assoc_array_local_int_key(
    expr: &Expr,
    name: &str,
    key_value: i64,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let key = module.next_label("assoc_array_key");
    module.declare_i64_local(key.trim_start_matches('$').to_string());
    module.body().line(&format!("i64.const {}", key_value));
    module.body().line(&format!("local.set {}", key));
    emit_output_assoc_array_local_int_key_parts(expr, name, &key, module)
}

fn emit_output_assoc_array_local_int_key_parts(
    expr: &Expr,
    name: &str,
    key: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let entry = module.next_label("assoc_array_entry");
    let cell = module.next_label("assoc_array_cell");
    let done_label = module.next_label("assoc_array_access_done");
    let loop_label = module.next_label("assoc_array_access_loop");
    for local in [&entry, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", entry));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", entry));
    module
        .body()
        .line(&format!("i32.const {}", WASM_ASSOC_ENTRY_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", entry));
    module
        .body()
        .line(&format!("i32.const {}", WASM_ASSOC_ENTRY_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.get {}", key));
    module.body().line("i64.eq");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", entry));
    module
        .body()
        .line(&format!("i32.const {}", WASM_ASSOC_ENTRY_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i32.const 16");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
    emit_output_value_cell(&cell, module);
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    let _ = expr;
    Ok(())
}

fn emit_output_assoc_array_local_string_key(
    expr: &Expr,
    name: &str,
    key_value: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let (key_ptr, key_len) = module.intern_string(key_value);
    let key_ptr_local = module.next_label("assoc_array_key_ptr");
    let key_len_local = module.next_label("assoc_array_key_len");
    module.declare_i32_local(key_ptr_local.trim_start_matches('$').to_string());
    module.declare_i32_local(key_len_local.trim_start_matches('$').to_string());
    module.body().line(&format!("i32.const {}", key_ptr));
    module.body().line(&format!("local.set {}", key_ptr_local));
    module.body().line(&format!("i32.const {}", key_len));
    module.body().line(&format!("local.set {}", key_len_local));
    emit_output_assoc_array_local_string_key_parts(
        expr,
        name,
        &key_ptr_local,
        &key_len_local,
        module,
    )
}

fn emit_output_assoc_array_local_string_key_parts(
    expr: &Expr,
    name: &str,
    key_ptr: &str,
    key_len: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let entry = module.next_label("assoc_array_entry");
    let cell = module.next_label("assoc_array_cell");
    let entry_base = module.next_label("assoc_array_entry_base");
    let offset = module.next_label("assoc_array_key_offset");
    let matched = module.next_label("assoc_array_key_matched");
    let done_label = module.next_label("assoc_array_access_done");
    let loop_label = module.next_label("assoc_array_access_loop");
    let compare_done_label = module.next_label("assoc_array_key_compare_done");
    let compare_loop_label = module.next_label("assoc_array_key_compare_loop");
    for local in [&entry, &cell, &entry_base, &offset, &matched] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", entry));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", entry));
    module.body().line(&format!("local.get ${}_len", name));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", entry));
    module
        .body()
        .line(&format!("i32.const {}", WASM_ASSOC_ENTRY_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", entry_base));
    module.body().line(&format!("local.get {}", entry_base));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", entry_base));
    module.body().line("i32.const 12");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.get {}", key_len));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", matched));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", offset));
    module.body().open(&format!("block {}", compare_done_label));
    module.body().open(&format!("loop {}", compare_loop_label));
    module.body().line(&format!("local.get {}", offset));
    module.body().line(&format!("local.get {}", key_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", compare_done_label));
    module.body().line(&format!("local.get {}", entry_base));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.get {}", offset));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.get {}", key_ptr));
    module.body().line(&format!("local.get {}", offset));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("br {}", compare_done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", offset));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", offset));
    module.body().line(&format!("br {}", compare_loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line(&format!("local.get {}", entry_base));
    module.body().line("i32.const 16");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
    emit_output_value_cell(&cell, module);
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", entry));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", entry));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    let _ = expr;
    Ok(())
}
