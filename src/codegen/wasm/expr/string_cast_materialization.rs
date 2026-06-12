//! Purpose:
//! Lowers wasm32-web string casts and scalar-to-string stack materialization.
//! Keeps cast-specific conversion code separate from string assignment and concat lowering.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::string_materialization`
//! - `crate::codegen::wasm::expr` sibling modules that need scalar string coercion.
//!
//! Key details:
//! - Cast helpers preserve PHP scalar coercion rules and return ptr/len on the wasm stack.

use super::*;

pub(in crate::codegen::wasm) fn emit_string_cast_value_to_stack(
    cast: &Expr,
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(err) = unsupported_string_coercion_expr(expr) {
        return Err(err);
    }
    if let Some(err) = unsupported_object_string_coercion_expr(expr, module) {
        return Err(err);
    }
    if let Some(value) = static_scalar_cast_string(expr, module) {
        let (ptr, len) = module.intern_string(&value);
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        return Ok(());
    }
    if emit_object_tostring_value_to_stack(expr, module)? {
        return Ok(());
    }
    if let Some(err) = unsupported_object_string_coercion_expr(expr, module) {
        return Err(err);
    }
    if dynamic_scalar_mixed_string_coercion_supported(expr, module)
        || variable_index_scalar_mixed_string_coercion_candidate(expr, module)
        || unknown_mixed_string_coercion_candidate(expr, module)
    {
        let cell = if let Some(cell) = materialize_dynamic_string_coercion_value_cell(expr, module)? {
            cell
        } else if let Some(cell) = materialize_mixed_value_cell(expr, module)? {
            cell
        } else {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web dynamic mixed string coercion could not materialize this value cell",
            ));
        };
        emit_dynamic_mixed_string_cast_value_to_stack(&cell, module);
        return Ok(());
    }
    if let Some(kind) = known_mixed_string_coercion_kind(expr, module)? {
        let Some(cell) = materialize_mixed_value_cell(expr, module)? else {
            return Err(CompileError::new(
                expr.span,
                "wasm32-web mixed string coercion could not materialize this value cell",
            ));
        };
        return emit_known_mixed_string_cast_value_to_stack(&cell, kind, expr.span, module);
    }
    if expression_is_stringy(expr, module) {
        return emit_string_value_to_stack(expr, module);
    }
    if expression_is_booly(expr, module) {
        emit_condition(expr, module)?;
        module.body().open("if (result i32 i32)");
        let (ptr, len) = module.intern_string("1");
        module.body().line(&format!("i32.const {}", ptr));
        module.body().line(&format!("i32.const {}", len));
        module.body().line("else");
        module.body().line("i32.const 0");
        module.body().line("i32.const 0");
        module.body().close("end");
        return Ok(());
    }
    if expression_is_inty(expr, module) {
        return emit_int_string_cast_value_to_stack(expr, module);
    }
    if expression_is_floaty(expr, module) {
        require_float(expr, module)?;
        emit_f64_stack_string_cast_value_to_stack("string_cast_float", module);
        return Ok(());
    }
    Err(CompileError::new(
        cast.span,
        "wasm32-web string casts currently support static scalars and runtime int/float/bool/string values",
    ))
}

pub(in crate::codegen::wasm) fn emit_int_string_cast_value_to_stack(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    require_int(expr, module)?;
    emit_i64_stack_string_cast_value_to_stack("string_cast", module);
    Ok(())
}

pub(in crate::codegen::wasm) fn emit_i64_stack_string_cast_value_to_stack(prefix: &str, module: &mut WasmModule) {
    let value = module.next_label(&format!("{prefix}_value"));
    let digit = module.next_label(&format!("{prefix}_digit"));
    let negative = module.next_label(&format!("{prefix}_negative"));
    let out_ptr = module.next_label(&format!("{prefix}_ptr"));
    let write = module.next_label(&format!("{prefix}_write"));
    let len = module.next_label(&format!("{prefix}_len"));
    let loop_label = module.next_label(&format!("{prefix}_loop"));
    let done_label = module.next_label(&format!("{prefix}_done"));
    module.declare_i64_local(value.trim_start_matches('$').to_string());
    module.declare_i64_local(digit.trim_start_matches('$').to_string());
    module.declare_i32_local(negative.trim_start_matches('$').to_string());
    for local in [&out_ptr, &write, &len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.set {}", value));
    module.body().line(&format!("local.get {}", value));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().line(&format!("local.set {}", negative));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line("i32.const 20");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("i32.const 20");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", write));
    module.body().line(&format!("local.get {}", value));
    module.body().line("i64.eqz");
    module.body().open("if");
    module.body().line(&format!("local.get {}", write));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", write));
    module.body().line(&format!("local.get {}", write));
    module.body().line("i32.const 48");
    module.body().line("i32.store8");
    module.body().line("else");
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", value));
    module.body().line("i64.eqz");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", value));
    module.body().line("i64.const 10");
    module.body().line("i64.rem_s");
    module.body().line(&format!("local.set {}", digit));
    module.body().line(&format!("local.get {}", digit));
    module.body().line("i64.const 0");
    module.body().line("i64.lt_s");
    module.body().open("if");
    module.body().line("i64.const 0");
    module.body().line(&format!("local.get {}", digit));
    module.body().line("i64.sub");
    module.body().line(&format!("local.set {}", digit));
    module.body().close("end");
    module.body().line(&format!("local.get {}", write));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", write));
    module.body().line(&format!("local.get {}", write));
    module.body().line(&format!("local.get {}", digit));
    module.body().line("i32.wrap_i64");
    module.body().line("i32.const 48");
    module.body().line("i32.add");
    module.body().line("i32.store8");
    module.body().line(&format!("local.get {}", value));
    module.body().line("i64.const 10");
    module.body().line("i64.div_s");
    module.body().line(&format!("local.set {}", value));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", negative));
    module.body().open("if");
    module.body().line(&format!("local.get {}", write));
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", write));
    module.body().line(&format!("local.get {}", write));
    module.body().line("i32.const 45");
    module.body().line("i32.store8");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("i32.const 20");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", write));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.get {}", write));
    module.body().line(&format!("local.get {}", len));
}

pub(in crate::codegen::wasm) fn emit_f64_stack_string_cast_value_to_stack(prefix: &str, module: &mut WasmModule) {
    let value = module.next_label(&format!("{prefix}_value"));
    let out_ptr = module.next_label(&format!("{prefix}_ptr"));
    let out_len = module.next_label(&format!("{prefix}_len"));
    module.declare_f64_local(value.trim_start_matches('$').to_string());
    module.declare_i32_local(out_ptr.trim_start_matches('$').to_string());
    module.declare_i32_local(out_len.trim_start_matches('$').to_string());
    module.body().line(&format!("local.set {}", value));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line("i32.const 64");
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line(&format!("local.get {}", value));
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line("call $host_float_to_string");
    module.body().line(&format!("local.set {}", out_len));
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.get {}", out_len));
}

pub(in crate::codegen::wasm) fn string_cast_value_supported(expr: &Expr, module: &WasmModule) -> bool {
    static_scalar_cast_string(expr, module).is_some()
        || object_tostring_supported(expr, module)
        || expression_is_stringy(expr, module)
        || expression_is_booly(expr, module)
        || expression_is_inty(expr, module)
        || expression_is_floaty(expr, module)
        || matches!(
            known_mixed_string_coercion_kind(expr, module),
            Ok(Some(
                ValueCellKind::Int
                    | ValueCellKind::Float
                    | ValueCellKind::Bool
                    | ValueCellKind::Str
                    | ValueCellKind::Null
                    | ValueCellKind::Array
            ))
        )
        || dynamic_scalar_mixed_string_coercion_supported(expr, module)
        || variable_index_scalar_mixed_string_coercion_candidate(expr, module)
        || unknown_mixed_string_coercion_candidate(expr, module)
}
