//! Purpose:
//! Lowers wasm32-web PHP range() array construction.
//! Keeps static/runtime integer and string range metadata and loops out of expr.rs.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_assign`
//!
//! Key details:
//! - Helpers preserve PHP string range edge cases, value-cell layout, and runtime length metadata.

use super::*;

pub(super) fn static_range_items_if_possible(
    expr: &Expr,
    args: &[Expr],
    module: &WasmModule,
) -> Result<Option<Vec<Expr>>, CompileError> {
    if !(2..=3).contains(&args.len()) {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web range() expects two or three arguments",
        ));
    }
    if let (Some(start), Some(end)) = (
        static_range_string_value(&args[0], module),
        static_range_string_value(&args[1], module),
    ) {
        return static_string_range_items(expr, args, &start, &end, module);
    }
    let Some(start) = static_or_const_int_value(&args[0]) else {
        return Ok(None);
    };
    let Some(end) = static_or_const_int_value(&args[1]) else {
        return Ok(None);
    };
    let step = if let Some(step_arg) = args.get(2) {
        let Some(step) = static_or_const_or_i64_local_value(step_arg, module) else {
            return Ok(None);
        };
        if step == 0 {
            return Err(CompileError::new(
                step_arg.span,
                "wasm32-web range() step must not be zero",
            ));
        }
        step.abs()
    } else {
        1
    };
    let mut values = Vec::new();
    let mut current = start;
    if start <= end {
        while current <= end {
            values.push(Expr::new(ExprKind::IntLiteral(current), expr.span));
            let next = current.saturating_add(step);
            if next <= current {
                break;
            }
            current = next;
        }
    } else {
        while current >= end {
            values.push(Expr::new(ExprKind::IntLiteral(current), expr.span));
            let next = current.saturating_sub(step);
            if next >= current {
                break;
            }
            current = next;
        }
    }
    Ok(Some(values))
}

fn static_string_range_items(
    expr: &Expr,
    args: &[Expr],
    start: &str,
    end: &str,
    module: &WasmModule,
) -> Result<Option<Vec<Expr>>, CompileError> {
    if start.is_empty() || end.is_empty() {
        return static_empty_string_range_items(expr, args, start, end, module);
    }
    let step = if let Some(step_arg) = args.get(2) {
        let Some(step) = static_or_const_or_i64_local_value(step_arg, module) else {
            return Ok(None);
        };
        if step == 0 {
            return Err(CompileError::new(
                step_arg.span,
                "wasm32-web range() step must not be zero",
            ));
        }
        u8::try_from(step.abs()).map_err(|_| {
            CompileError::new(step_arg.span, "wasm32-web range() string step is too large")
        })?
    } else {
        1
    };
    let start = start.as_bytes()[0];
    let end = end.as_bytes()[0];
    let mut values = Vec::new();
    let mut current = start;
    if start <= end {
        while current <= end {
            values.push(Expr::new(
                ExprKind::StringLiteral(char::from(current).to_string()),
                expr.span,
            ));
            let Some(next) = current.checked_add(step) else {
                break;
            };
            if next <= current {
                break;
            }
            current = next;
        }
    } else {
        while current >= end {
            values.push(Expr::new(
                ExprKind::StringLiteral(char::from(current).to_string()),
                expr.span,
            ));
            let Some(next) = current.checked_sub(step) else {
                break;
            };
            if next >= current {
                break;
            }
            current = next;
        }
    }
    Ok(Some(values))
}

fn static_empty_string_range_items(
    expr: &Expr,
    args: &[Expr],
    start: &str,
    end: &str,
    module: &WasmModule,
) -> Result<Option<Vec<Expr>>, CompileError> {
    let step = if let Some(step_arg) = args.get(2) {
        let Some(step) = static_or_const_or_i64_local_value(step_arg, module) else {
            return Ok(None);
        };
        if step == 0 {
            return Err(CompileError::new(
                step_arg.span,
                "wasm32-web range() step must not be zero",
            ));
        }
        step.abs()
    } else {
        1
    };
    let start = string_range_empty_cast_int(start);
    let end = string_range_empty_cast_int(end);
    let mut values = Vec::new();
    let mut current = start;
    if start <= end {
        while current <= end {
            values.push(Expr::new(ExprKind::IntLiteral(current), expr.span));
            let next = current.saturating_add(step);
            if next <= current {
                break;
            }
            current = next;
        }
    } else {
        while current >= end {
            values.push(Expr::new(ExprKind::IntLiteral(current), expr.span));
            let next = current.saturating_sub(step);
            if next >= current {
                break;
            }
            current = next;
        }
    }
    Ok(Some(values))
}

fn string_range_empty_cast_int(value: &str) -> i64 {
    if value.is_empty() {
        0
    } else {
        value.parse::<i64>().unwrap_or(0)
    }
}

fn static_range_string_value(expr: &Expr, module: &WasmModule) -> Option<String> {
    match &expr.kind {
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Str) => {
            module.string_static_value(name)
        }
        _ => static_string_value(expr, module),
    }
}

pub(super) fn range_args_are_stringy(args: &[Expr], module: &WasmModule) -> bool {
    args.len() >= 2 && expression_is_stringy(&args[0], module) && expression_is_stringy(&args[1], module)
}

pub(super) fn emit_runtime_string_range_assign(
    name: &str,
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if !(2..=3).contains(&args.len()) {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web range() expects two or three arguments",
        ));
    }
    let start_ptr = module.next_label("string_range_start_ptr");
    let start_len = module.next_label("string_range_start_len");
    let end_ptr = module.next_label("string_range_end_ptr");
    let end_len = module.next_label("string_range_end_len");
    let start = module.next_label("string_range_start");
    let end = module.next_label("string_range_end");
    let step64 = module.next_label("string_range_step64");
    let step = module.next_label("string_range_step");
    let current = module.next_label("string_range_current");
    let len = module.next_label("string_range_len");
    let index = module.next_label("string_range_index");
    let ascending = module.next_label("string_range_ascending");
    let part_ptr = module.next_label("string_range_part_ptr");
    let cell = module.next_label("string_range_cell");
    let done_label = module.next_label("string_range_done");
    let loop_label = module.next_label("string_range_loop");
    for local in [
        &start_ptr,
        &start_len,
        &end_ptr,
        &end_len,
        &start,
        &end,
        &step,
        &current,
        &len,
        &index,
        &ascending,
        &part_ptr,
        &cell,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.declare_i64_local(step64.trim_start_matches('$').to_string());
    emit_release_current_value_array(name, module);
    emit_string_value_to_stack(&args[0], module)?;
    module.body().line(&format!("local.set {}", start_len));
    module.body().line(&format!("local.set {}", start_ptr));
    emit_string_value_to_stack(&args[1], module)?;
    module.body().line(&format!("local.set {}", end_len));
    module.body().line(&format!("local.set {}", end_ptr));
    if let Some(step_arg) = args.get(2) {
        require_int(step_arg, module)?;
        module.body().line(&format!("local.set {}", step64));
        module.body().line(&format!("local.get {}", step64));
        module.body().line("i64.eqz");
        module.body().open("if");
        module.body().line("unreachable");
        module.body().close("end");
        module.body().line(&format!("local.get {}", step64));
        module.body().line("i64.const 0");
        module.body().line("i64.lt_s");
        module.body().open("if");
        module.body().line("i64.const 0");
        module.body().line(&format!("local.get {}", step64));
        module.body().line("i64.sub");
        module.body().line(&format!("local.set {}", step64));
        module.body().close("end");
        module.body().line(&format!("local.get {}", step64));
        module.body().line("i64.const 255");
        module.body().line("i64.gt_u");
        module.body().open("if");
        module.body().line("i32.const 256");
        module.body().line(&format!("local.set {}", step));
        module.body().line("else");
        module.body().line(&format!("local.get {}", step64));
        module.body().line("i32.wrap_i64");
        module.body().line(&format!("local.set {}", step));
        module.body().close("end");
    } else {
        module.body().line("i64.const 1");
        module.body().line(&format!("local.set {}", step64));
        module.body().line("i32.const 1");
        module.body().line(&format!("local.set {}", step));
    }
    module.body().line(&format!("local.get {}", start_len));
    module.body().line("i32.eqz");
    module.body().line(&format!("local.get {}", end_len));
    module.body().line("i32.eqz");
    module.body().line("i32.or");
    module.body().open("if");
    emit_empty_string_range_int_assign(
        name,
        &start_ptr,
        &start_len,
        &end_ptr,
        &end_len,
        &step64,
        &len,
        &index,
        &ascending,
        &cell,
        module,
    );
    module.body().line("else");
    module.body().line(&format!("local.get {}", start_ptr));
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", start));
    module.body().line(&format!("local.get {}", end_ptr));
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", end));
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.le_u");
    module.body().line(&format!("local.set {}", ascending));
    module.body().line(&format!("local.get {}", ascending));
    module.body().open("if");
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.sub");
    module.body().line(&format!("local.get {}", step));
    module.body().line("i32.div_u");
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.set {}", current));
    module.body().line("else");
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i32.sub");
    module.body().line(&format!("local.get {}", step));
    module.body().line("i32.div_u");
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.set {}", current));
    module.body().close("end");
    module.body().line(&format!("local.get {}", len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line("i32.const 1");
    module.body().line("call $__rt_alloc_bytes");
    module.body().line(&format!("local.set {}", part_ptr));
    module.body().line(&format!("local.get {}", part_ptr));
    module.body().line(&format!("local.get {}", current));
    module.body().line("i32.store8");
    emit_value_cell_address_for_local(name, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("local.get {}", part_ptr));
    module.body().line("i32.const 1");
    module.body().line("call $__rt_value_store_string");
    module.body().line(&format!("local.get {}", ascending));
    module.body().open("if");
    module.body().line(&format!("local.get {}", current));
    module.body().line(&format!("local.get {}", step));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", current));
    module.body().line("else");
    module.body().line(&format!("local.get {}", current));
    module.body().line(&format!("local.get {}", step));
    module.body().line("i32.sub");
    module.body().line(&format!("local.set {}", current));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.set_array_layout(name, ArrayLayout::Value);
    module.clear_array_length(name);
    module.set_array_value_cell_kinds(name, None);
    module.set_array_runtime_value_cell_kind(name, Some(ValueCellKind::Str));
    module.set_array_nested_value_metadata(name, None);
    Ok(())
}

fn emit_empty_string_range_int_assign(
    name: &str,
    start_ptr: &str,
    start_len: &str,
    end_ptr: &str,
    end_len: &str,
    step: &str,
    len: &str,
    index: &str,
    ascending: &str,
    cell: &str,
    module: &mut WasmModule,
) {
    let start = module.next_label("empty_string_range_start");
    let end = module.next_label("empty_string_range_end");
    let current = module.next_label("empty_string_range_current");
    let len64 = module.next_label("empty_string_range_len64");
    let done_label = module.next_label("empty_string_range_done");
    let loop_label = module.next_label("empty_string_range_loop");
    for local in [&start, &end, &current, &len64] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get {}", start_ptr));
    module.body().line(&format!("local.get {}", start_len));
    module.body().line("call $__rt_string_range_empty_cast_int");
    module.body().line(&format!("local.set {}", start));
    module.body().line(&format!("local.get {}", end_ptr));
    module.body().line(&format!("local.get {}", end_len));
    module.body().line("call $__rt_string_range_empty_cast_int");
    module.body().line(&format!("local.set {}", end));
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i64.le_s");
    module.body().line(&format!("local.set {}", ascending));
    module.body().line(&format!("local.get {}", ascending));
    module.body().open("if");
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.get {}", start));
    module.body().line("i64.sub");
    module.body().line(&format!("local.get {}", step));
    module.body().line("i64.div_u");
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", len64));
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.set {}", current));
    module.body().line("else");
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i64.sub");
    module.body().line(&format!("local.get {}", step));
    module.body().line("i64.div_u");
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", len64));
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.set {}", current));
    module.body().close("end");
    module.body().line(&format!("local.get {}", len64));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", len));
    module.body().line(&format!("local.get {}", len));
    module.body().line("call $__rt_alloc_value_cells");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", len));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(name, index, cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line(&format!("local.get {}", current));
    module.body().line("call $__rt_value_store_int");
    module.body().line(&format!("local.get {}", ascending));
    module.body().open("if");
    module.body().line(&format!("local.get {}", current));
    module.body().line(&format!("local.get {}", step));
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", current));
    module.body().line("else");
    module.body().line(&format!("local.get {}", current));
    module.body().line(&format!("local.get {}", step));
    module.body().line("i64.sub");
    module.body().line(&format!("local.set {}", current));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_range_assign(
    name: &str,
    expr: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if !(2..=3).contains(&args.len()) {
        return Err(CompileError::new(
            expr.span,
            "wasm32-web range() expects two or three arguments",
        ));
    }
    let start = module.next_label("range_start");
    let end = module.next_label("range_end");
    let step = module.next_label("range_step");
    let current = module.next_label("range_current");
    let len64 = module.next_label("range_len64");
    let len32 = module.next_label("range_len32");
    let index = module.next_label("range_index");
    let ascending = module.next_label("range_ascending");
    let done_label = module.next_label("range_done");
    let loop_label = module.next_label("range_loop");
    for local in [&start, &end, &step, &current, &len64] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    for local in [&len32, &index, &ascending] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    require_int(&args[0], module)?;
    module.body().line(&format!("local.set {}", start));
    require_int(&args[1], module)?;
    module.body().line(&format!("local.set {}", end));
    if let Some(step_arg) = args.get(2) {
        require_int(step_arg, module)?;
        module.body().line(&format!("local.set {}", step));
        module.body().line(&format!("local.get {}", step));
        module.body().line("i64.eqz");
        module.body().open("if");
        module.body().line("unreachable");
        module.body().close("end");
        module.body().line(&format!("local.get {}", step));
        module.body().line("i64.const 0");
        module.body().line("i64.lt_s");
        module.body().open("if");
        module.body().line("i64.const 0");
        module.body().line(&format!("local.get {}", step));
        module.body().line("i64.sub");
        module.body().line(&format!("local.set {}", step));
        module.body().close("end");
    } else {
        module.body().line("i64.const 1");
        module.body().line(&format!("local.set {}", step));
    }
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i64.le_s");
    module.body().line(&format!("local.set {}", ascending));
    module.body().line(&format!("local.get {}", ascending));
    module.body().open("if");
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.get {}", start));
    module.body().line("i64.sub");
    module.body().line(&format!("local.get {}", step));
    module.body().line("i64.div_u");
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", len64));
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.set {}", current));
    module.body().line("else");
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.get {}", end));
    module.body().line("i64.sub");
    module.body().line(&format!("local.get {}", step));
    module.body().line("i64.div_u");
    module.body().line("i64.const 1");
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", len64));
    module.body().line(&format!("local.get {}", start));
    module.body().line(&format!("local.set {}", current));
    module.body().close("end");
    module.body().line(&format!("local.get {}", len64));
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set {}", len32));
    module.body().line(&format!("local.get {}", len32));
    module.body().line("call $__rt_alloc_indexed_slots");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line(&format!("local.get {}", len32));
    module.body().line(&format!("local.set ${}_len", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", len32));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", current));
    module.body().line("i64.store");
    module.body().line(&format!("local.get {}", ascending));
    module.body().open("if");
    module.body().line(&format!("local.get {}", current));
    module.body().line(&format!("local.get {}", step));
    module.body().line("i64.add");
    module.body().line(&format!("local.set {}", current));
    module.body().line("else");
    module.body().line(&format!("local.get {}", current));
    module.body().line(&format!("local.get {}", step));
    module.body().line("i64.sub");
    module.body().line(&format!("local.set {}", current));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}
