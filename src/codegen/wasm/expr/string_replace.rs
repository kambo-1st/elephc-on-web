//! Purpose:
//! Lowers str_repeat() and str_replace() families for the wasm32-web backend.
//! Owns direct-output and heap-materialized string replacement/repetition paths.
//!
//! Called from:
//! - `crate::codegen::wasm::expr`
//! - `crate::codegen::wasm::expr::string_builtins`
//!
//! Key details:
//! - Handles count locals, runtime search/replacement strings, and string-array replacement loops without touching native codegen.

use super::*;

pub(super) fn str_replace_count_local(
    call: &Expr,
    args: &[Expr],
    module: &WasmModule,
) -> Result<Option<String>, CompileError> {
    let Some(count_arg) = args.get(3) else {
        return Ok(None);
    };
    let ExprKind::Variable(name) = &count_arg.kind else {
        return Err(CompileError::new(
            count_arg.span,
            "wasm32-web str_replace() count argument currently requires an integer local variable",
        ));
    };
    if module.local_kind(name) != Some(LocalKind::I64) {
        return Err(CompileError::new(
            call.span,
            "wasm32-web str_replace() count argument currently requires an integer local variable",
        ));
    }
    Ok(Some(name.clone()))
}

pub(super) fn emit_set_i64_local_const(local: &str, value: i64, module: &mut WasmModule) {
    module.body().line(&format!("i64.const {}", value));
    module.body().line(&format!("local.set ${}", local));
}

fn emit_zero_i64_count_local(local: Option<&str>, module: &mut WasmModule) {
    if let Some(local) = local {
        emit_set_i64_local_const(local, 0, module);
    }
}

fn emit_increment_i64_count_local(local: Option<&str>, module: &mut WasmModule) {
    if let Some(local) = local {
        module.body().line(&format!("local.get ${}", local));
        module.body().line("i64.const 1");
        module.body().line("i64.add");
        module.body().line(&format!("local.set ${}", local));
    }
}

pub(super) fn emit_runtime_str_replace(
    var: &str,
    search: &str,
    replace: &str,
    case_insensitive: bool,
    count_local: Option<&str>,
    module: &mut WasmModule,
) {
    emit_zero_i64_count_local(count_local, module);
    if search.is_empty() {
        module.body().line(&format!("local.get ${}_ptr", var));
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line("call $host_write");
        return;
    }
    let idx = module.next_label("replace_idx");
    let loop_label = module.next_label("replace_loop");
    let done_label = module.next_label("replace_done");
    let (replace_ptr, replace_len) = module.intern_string(replace);
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", search.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.le_u");
    module.body().open("if");
    if case_insensitive {
        emit_runtime_literal_match_at_case_insensitive(var, &idx, search, module);
    } else {
        emit_runtime_literal_match_at(var, &idx, search, module);
    }
    module.body().open("if");
    emit_write_static_string(replace_ptr, replace_len, module);
    emit_increment_i64_count_local(count_local, module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", search.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    emit_load_string_byte(var, &idx, module);
    emit_write_stack_byte(module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_runtime_str_replace_var_replacement(
    var: &str,
    search: &str,
    replace_var: &str,
    case_insensitive: bool,
    count_local: Option<&str>,
    module: &mut WasmModule,
) {
    emit_zero_i64_count_local(count_local, module);
    if search.is_empty() {
        module.body().line(&format!("local.get ${}_ptr", var));
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line("call $host_write");
        return;
    }
    let idx = module.next_label("replace_idx");
    let loop_label = module.next_label("replace_loop");
    let done_label = module.next_label("replace_done");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", search.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.le_u");
    module.body().open("if");
    if case_insensitive {
        emit_runtime_literal_match_at_case_insensitive(var, &idx, search, module);
    } else {
        emit_runtime_literal_match_at(var, &idx, search, module);
    }
    module.body().open("if");
    emit_write_string_var(replace_var, module);
    emit_increment_i64_count_local(count_local, module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("i32.const {}", search.len()));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    emit_load_string_byte(var, &idx, module);
    emit_write_stack_byte(module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

#[derive(Clone, Copy)]
pub(super) enum WasmReplacement<'a> {
    Literal(&'a str),
    Variable(&'a str),
}

#[derive(Clone, Copy)]
pub(super) enum WasmSearch<'a> {
    Literal(&'a str),
    Variable(&'a str),
}

pub(super) fn emit_runtime_str_replace_value_to_stack(
    var: &str,
    search: WasmSearch<'_>,
    replace: WasmReplacement<'_>,
    case_insensitive: bool,
    count_local: Option<&str>,
    reset_count: bool,
    module: &mut WasmModule,
) {
    if reset_count {
        emit_zero_i64_count_local(count_local, module);
    }
    if matches!(search, WasmSearch::Literal("")) {
        module.body().line(&format!("local.get ${}_ptr", var));
        module.body().line(&format!("local.get ${}_len", var));
        return;
    }
    let idx = module.next_label("replace_value_idx");
    let out_ptr = module.next_label("replace_value_ptr");
    let out_idx = module.next_label("replace_value_out_idx");
    let out_cap = module.next_label("replace_value_cap");
    let result_ptr = module.next_label("replace_value_result_ptr");
    let result_len = module.next_label("replace_value_result_len");
    let loop_label = module.next_label("replace_value_loop");
    let done_label = module.next_label("replace_value_done");
    let literal_replace = match replace {
        WasmReplacement::Literal(value) => Some(module.intern_string(value)),
        WasmReplacement::Variable(_) => None,
    };
    for local in [&idx, &out_ptr, &out_idx, &out_cap, &result_ptr, &result_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }

    if let WasmSearch::Variable(search_var) = search {
        module.body().line(&format!("local.get ${}_len", search_var));
        module.body().line("i32.eqz");
        module.body().open("if");
        module.body().line(&format!("local.get ${}_ptr", var));
        module.body().line(&format!("local.set {}", result_ptr));
        module.body().line(&format!("local.get ${}_len", var));
        module.body().line(&format!("local.set {}", result_len));
        module.body().line("else");
        emit_runtime_str_replace_value_body(
            var,
            search,
            replace,
            literal_replace,
            case_insensitive,
            &idx,
            &out_ptr,
            &out_idx,
            &out_cap,
            &result_ptr,
            &result_len,
            &loop_label,
            &done_label,
            count_local,
            module,
        );
        module.body().close("end");
    } else {
        emit_runtime_str_replace_value_body(
            var,
            search,
            replace,
            literal_replace,
            case_insensitive,
            &idx,
            &out_ptr,
            &out_idx,
            &out_cap,
            &result_ptr,
            &result_len,
            &loop_label,
            &done_label,
            count_local,
            module,
        );
    }
    module.body().line(&format!("local.get {}", result_ptr));
    module.body().line(&format!("local.get {}", result_len));
}

pub(super) fn value_string_array_len(name: &str, module: &WasmModule) -> Option<usize> {
    if module.array_layout(name) != ArrayLayout::Value {
        return None;
    }
    let len = module.array_length(name)?;
    let kinds = module.array_value_cell_kinds(name)?;
    (kinds.len() == len && kinds.iter().all(|kind| matches!(kind, ValueCellKind::Str))).then_some(len)
}

pub(super) fn runtime_string_value_array(name: &str, module: &WasmModule) -> bool {
    module.array_layout(name) == ArrayLayout::Value
        && module.array_runtime_value_cell_kind(name) == Some(ValueCellKind::Str)
}

pub(super) fn materialize_string_value_array_arg(
    expr: &Expr,
    prefix: &str,
    module: &mut WasmModule,
) -> Result<Option<String>, CompileError> {
    if let ExprKind::Variable(name) = &expr.kind {
        if value_string_array_len(name, module).is_some() || runtime_string_value_array(name, module) {
            return Ok(Some(name.clone()));
        }
        return Ok(None);
    }
    let can_materialize = match &expr.kind {
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => true,
        ExprKind::FunctionCall { name, .. } => {
            name.eq_ignore_ascii_case("explode")
                || name.eq_ignore_ascii_case("str_split")
                || (module.has_function(name)
                    && module.function_return_kind(name) == Some(ValueKind::Array)
                    && module.function_array_return_layout(name) == ArrayLayout::Value
                    && (module
                        .function_array_return_value_kinds(name)
                        .is_some_and(|kinds| kinds.iter().all(|kind| *kind == ValueCellKind::Str))
                        || module.function_array_return_runtime_value_kind(name)
                            == Some(ValueCellKind::Str)))
        }
        _ => false,
    };
    if !can_materialize {
        return Ok(None);
    }
    let temp = module.next_label(prefix).trim_start_matches('$').to_string();
    module.declare_array_local(temp.clone());
    emit_array_assign(&temp, expr, module)?;
    if value_string_array_len(&temp, module).is_some() || runtime_string_value_array(&temp, module) {
        return Ok(Some(temp));
    }
    Ok(None)
}

pub(super) fn emit_runtime_str_replace_array_value_to_stack(
    subject_var: &str,
    search_array: &str,
    replacement_array: Option<&str>,
    scalar_replacement: Option<WasmReplacement<'_>>,
    case_insensitive: bool,
    count_local: Option<&str>,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let search_len = value_string_array_len(search_array, module).ok_or_else(|| {
        CompileError::new(
            crate::span::Span::dummy(),
            "wasm32-web str_replace() array search currently requires a known string array",
        )
    })?;
    let replacement_len = replacement_array.and_then(|name| value_string_array_len(name, module));
    if replacement_array.is_some() && replacement_len.is_none() {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            "wasm32-web str_replace() array replacement currently requires a known string array",
        ));
    }
    let current = module
        .next_label("str_replace_array_current")
        .trim_start_matches('$')
        .to_string();
    let search_part = module
        .next_label("str_replace_array_search")
        .trim_start_matches('$')
        .to_string();
    let replacement_part = module
        .next_label("str_replace_array_replacement")
        .trim_start_matches('$')
        .to_string();
    for local in [&current, &search_part, &replacement_part] {
        module.declare_i32_local(format!("{}_ptr", local));
        module.declare_i32_local(format!("{}_len", local));
    }
    module.body().line(&format!("local.get ${}_ptr", subject_var));
    module.body().line(&format!("local.set ${}_ptr", current));
    module.body().line(&format!("local.get ${}_len", subject_var));
    module.body().line(&format!("local.set ${}_len", current));
    emit_zero_i64_count_local(count_local, module);
    for index in 0..search_len {
        emit_load_value_array_string_element_to_local(search_array, index, &search_part, module);
        let replacement = if let Some(replacement_array) = replacement_array {
            if index < replacement_len.expect("replacement array length was validated") {
                emit_load_value_array_string_element_to_local(
                    replacement_array,
                    index,
                    &replacement_part,
                    module,
                );
                WasmReplacement::Variable(&replacement_part)
            } else {
                WasmReplacement::Literal("")
            }
        } else {
            scalar_replacement.expect("scalar replacement is required when replacement array is absent")
        };
        emit_runtime_str_replace_value_to_stack(
            &current,
            WasmSearch::Variable(&search_part),
            replacement,
            case_insensitive,
            count_local,
            false,
            module,
        );
        module.body().line(&format!("local.set ${}_len", current));
        module.body().line(&format!("local.set ${}_ptr", current));
    }
    module.body().line(&format!("local.get ${}_ptr", current));
    module.body().line(&format!("local.get ${}_len", current));
    Ok(())
}

pub(super) fn emit_runtime_str_replace_dynamic_array_value_to_stack(
    subject_var: &str,
    search_array: &str,
    replacement_array: Option<&str>,
    scalar_replacement: Option<WasmReplacement<'_>>,
    case_insensitive: bool,
    count_local: Option<&str>,
    module: &mut WasmModule,
) {
    let current = module
        .next_label("str_replace_runtime_array_current")
        .trim_start_matches('$')
        .to_string();
    let search_part = module
        .next_label("str_replace_runtime_array_search")
        .trim_start_matches('$')
        .to_string();
    let replacement_part = module
        .next_label("str_replace_runtime_array_replacement")
        .trim_start_matches('$')
        .to_string();
    let index = module.next_label("str_replace_runtime_array_index");
    let loop_label = module.next_label("str_replace_runtime_array_loop");
    let done_label = module.next_label("str_replace_runtime_array_done");
    for local in [&current, &search_part, &replacement_part] {
        module.declare_i32_local(format!("{}_ptr", local));
        module.declare_i32_local(format!("{}_len", local));
    }
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_ptr", subject_var));
    module.body().line(&format!("local.set ${}_ptr", current));
    module.body().line(&format!("local.get ${}_len", subject_var));
    module.body().line(&format!("local.set ${}_len", current));
    emit_zero_i64_count_local(count_local, module);
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", search_array));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_load_runtime_value_array_string_element_to_local(search_array, &index, &search_part, module);
    if let Some(replacement_array) = replacement_array {
        module.body().line(&format!("local.get {}", index));
        module.body().line(&format!("local.get ${}_len", replacement_array));
        module.body().line("i32.lt_u");
        module.body().open("if");
        emit_load_runtime_value_array_string_element_to_local(
            replacement_array,
            &index,
            &replacement_part,
            module,
        );
        emit_runtime_str_replace_value_to_stack(
            &current,
            WasmSearch::Variable(&search_part),
            WasmReplacement::Variable(&replacement_part),
            case_insensitive,
            count_local,
            false,
            module,
        );
        module.body().line(&format!("local.set ${}_len", current));
        module.body().line(&format!("local.set ${}_ptr", current));
        module.body().line("else");
        emit_runtime_str_replace_value_to_stack(
            &current,
            WasmSearch::Variable(&search_part),
            WasmReplacement::Literal(""),
            case_insensitive,
            count_local,
            false,
            module,
        );
        module.body().line(&format!("local.set ${}_len", current));
        module.body().line(&format!("local.set ${}_ptr", current));
        module.body().close("end");
    } else if let Some(replacement) = scalar_replacement {
        emit_runtime_str_replace_value_to_stack(
            &current,
            WasmSearch::Variable(&search_part),
            replacement,
            case_insensitive,
            count_local,
            false,
            module,
        );
        module.body().line(&format!("local.set ${}_len", current));
        module.body().line(&format!("local.set ${}_ptr", current));
    }
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get ${}_ptr", current));
    module.body().line(&format!("local.get ${}_len", current));
}

fn emit_load_value_array_string_element_to_local(
    array_name: &str,
    index: usize,
    target: &str,
    module: &mut WasmModule,
) {
    emit_value_array_static_payload_addr(array_name, index, module);
    module.body().line("i32.load");
    module.body().line(&format!("local.set ${}_ptr", target));
    emit_value_array_static_payload_addr(array_name, index, module);
    module.body().line("i32.const 4");
    module.body().line("i32.add");
    module.body().line("i32.load");
    module.body().line(&format!("local.set ${}_len", target));
}

fn emit_load_runtime_value_array_string_element_to_local(
    array_name: &str,
    index: &str,
    target: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", array_name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set ${}_ptr", target));
    module.body().line(&format!("local.get ${}_ptr", array_name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set ${}_len", target));
}

fn emit_runtime_str_replace_value_body(
    var: &str,
    search: WasmSearch<'_>,
    replace: WasmReplacement<'_>,
    literal_replace: Option<(usize, usize)>,
    case_insensitive: bool,
    idx: &str,
    out_ptr: &str,
    out_idx: &str,
    out_cap: &str,
    result_ptr: &str,
    result_len: &str,
    loop_label: &str,
    done_label: &str,
    count_local: Option<&str>,
    module: &mut WasmModule,
) {
    emit_runtime_str_replace_value_capacity(var, replace, out_cap, module);
    module.body().line("global.get $heap");
    module.body().line(&format!("local.set {}", out_ptr));
    module.body().line("global.get $heap");
    module.body().line(&format!("local.get {}", out_cap));
    module.body().line("i32.add");
    module.body().line("global.set $heap");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", idx));
    emit_wasm_search_len(search, module);
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.le_u");
    module.body().open("if");
    match search {
        WasmSearch::Literal(search) => {
            if case_insensitive {
                emit_runtime_literal_match_at_case_insensitive(var, idx, search, module);
            } else {
                emit_runtime_literal_match_at(var, idx, search, module);
            }
        }
        WasmSearch::Variable(search_var) => {
            if case_insensitive {
                emit_runtime_var_match_at_case_insensitive(var, idx, search_var, module);
            } else {
                emit_runtime_var_match_at(var, idx, search_var, module);
            }
        }
    }
    module.body().open("if");
    match replace {
        WasmReplacement::Literal(_) => {
            let (replace_ptr, replace_len) = literal_replace.unwrap();
            emit_copy_static_range_to_memory(replace_ptr, replace_len, out_ptr, out_idx, module);
        }
        WasmReplacement::Variable(replace_var) => emit_copy_string_to_memory(replace_var, out_ptr, out_idx, module),
    }
    emit_increment_i64_count_local(count_local, module);
    module.body().line(&format!("local.get {}", idx));
    emit_wasm_search_len(search, module);
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    emit_load_string_byte(var, idx, module);
    let byte = module.next_label("replace_value_byte");
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    module.body().line(&format!("local.set {}", byte));
    emit_store_byte_local(&byte, out_ptr, out_idx, module);
    emit_increment_local(idx, module);
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_ptr));
    module.body().line(&format!("local.set {}", result_ptr));
    module.body().line(&format!("local.get {}", out_idx));
    module.body().line(&format!("local.set {}", result_len));
}

fn emit_runtime_str_replace_value_capacity(
    var: &str,
    replace: WasmReplacement<'_>,
    out_cap: &str,
    module: &mut WasmModule,
) {
    match replace {
        WasmReplacement::Literal(value) => {
            module.body().line(&format!("local.get ${}_len", var));
            module.body().line(&format!("i32.const {}", value.len().max(1)));
            module.body().line("i32.mul");
            module.body().line(&format!("local.set {}", out_cap));
        }
        WasmReplacement::Variable(replace_var) => {
            module.body().line(&format!("local.get ${}_len", replace_var));
            module.body().line("i32.eqz");
            module.body().open("if (result i32)");
            module.body().line("i32.const 1");
            module.body().line("else");
            module.body().line(&format!("local.get ${}_len", replace_var));
            module.body().close("end");
            module.body().line(&format!("local.get ${}_len", var));
            module.body().line("i32.mul");
            module.body().line(&format!("local.set {}", out_cap));
        }
    }
}

fn emit_wasm_search_len(search: WasmSearch<'_>, module: &mut WasmModule) {
    match search {
        WasmSearch::Literal(search) => module.body().line(&format!("i32.const {}", search.len())),
        WasmSearch::Variable(search_var) => module.body().line(&format!("local.get ${}_len", search_var)),
    }
}

pub(super) fn emit_runtime_str_replace_var_search(
    var: &str,
    search_var: &str,
    replace: WasmReplacement<'_>,
    case_insensitive: bool,
    count_local: Option<&str>,
    module: &mut WasmModule,
) {
    emit_zero_i64_count_local(count_local, module);
    let idx = module.next_label("replace_idx");
    let loop_label = module.next_label("replace_loop");
    let done_label = module.next_label("replace_done");
    let literal_replace = match replace {
        WasmReplacement::Literal(value) => Some(module.intern_string(value)),
        WasmReplacement::Variable(_) => None,
    };
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_len", search_var));
    module.body().line("i32.eqz");
    module.body().open("if");
    emit_write_string_var(var, module);
    module.body().line("else");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", search_var));
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.le_u");
    module.body().open("if");
    if case_insensitive {
        emit_runtime_var_match_at_case_insensitive(var, &idx, search_var, module);
    } else {
        emit_runtime_var_match_at(var, &idx, search_var, module);
    }
    module.body().open("if");
    match replace {
        WasmReplacement::Literal(_) => {
            let (replace_ptr, replace_len) = literal_replace.unwrap();
            emit_write_static_string(replace_ptr, replace_len, module);
        }
        WasmReplacement::Variable(replace_var) => emit_write_string_var(replace_var, module),
    }
    emit_increment_i64_count_local(count_local, module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", search_var));
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    emit_load_string_byte(var, &idx, module);
    emit_write_stack_byte(module);
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}
