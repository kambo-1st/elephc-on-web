//! Purpose:
//! Emits wasm32-web `usort()` helper loops and compare-swap paths.
//! Keeps compact/value-cell value sorting separate from assoc-preserving sort modes.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_sort`
//!
//! Key details:
//! - Sorts after copy-on-write uniqueness and preserves value-cell ownership when swapping cells.

use super::*;
use super::array_value_cells::{
    emit_copy_value_cell_slot, emit_ensure_unique_array_payload, emit_load_value_cell_half,
    emit_store_value_cell_half,
};

pub(super) fn emit_usort_compact_int_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_usort_runtime_compact_int_local(source, source_span, callback, module);
    };
    emit_ensure_unique_array_payload(source, module);
    let left = module.next_label("usort_left");
    let right = module.next_label("usort_right");
    module.declare_i64_local(left.trim_start_matches('$').to_string());
    module.declare_i64_local(right.trim_start_matches('$').to_string());
    for pass in 0..len {
        for index in 0..len.saturating_sub(1 + pass) {
            emit_usort_compact_int_compare_swap(source, index, callback, &left, &right, module);
        }
    }
    Ok(())
}

pub(super) fn emit_usort_compact_int_local_instance(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_usort_runtime_compact_int_local_instance(
            source,
            source_span,
            callback,
            capture_local,
            module,
        );
    };
    emit_ensure_unique_array_payload(source, module);
    let left = module.next_label("usort_instance_left");
    let right = module.next_label("usort_instance_right");
    module.declare_i64_local(left.trim_start_matches('$').to_string());
    module.declare_i64_local(right.trim_start_matches('$').to_string());
    for pass in 0..len {
        for index in 0..len.saturating_sub(1 + pass) {
            emit_usort_compact_int_compare_swap_instance(
                source,
                index,
                callback,
                capture_local,
                &left,
                &right,
                module,
            );
        }
    }
    Ok(())
}

fn emit_usort_runtime_compact_int_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::CompactInt {
        return Err(CompileError::new(
            source_span,
            "wasm32-web runtime-length usort() currently requires compact integer arrays",
        ));
    }
    emit_ensure_unique_array_payload(source, module);
    let pass = module.next_label("usort_pass");
    let index = module.next_label("usort_index");
    let left = module.next_label("usort_runtime_left");
    let right = module.next_label("usort_runtime_right");
    let outer_done = module.next_label("usort_outer_done");
    let outer_loop = module.next_label("usort_outer_loop");
    let inner_done = module.next_label("usort_inner_done");
    let inner_loop = module.next_label("usort_inner_loop");
    module.declare_i32_local(pass.trim_start_matches('$').to_string());
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.declare_i64_local(left.trim_start_matches('$').to_string());
    module.declare_i64_local(right.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pass));
    module.body().open(&format!("block {}", outer_done));
    module.body().open(&format!("loop {}", outer_loop));
    module.body().line(&format!("local.get {}", pass));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", outer_done));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", inner_done));
    module.body().open(&format!("loop {}", inner_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.sub");
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", inner_done));
    emit_usort_runtime_compact_int_compare_swap(source, &index, callback, &left, &right, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", inner_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", pass));
    module.body().line(&format!("br {}", outer_loop));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_usort_runtime_compact_int_local_instance(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::CompactInt {
        return Err(CompileError::new(
            source_span,
            "wasm32-web runtime-length usort() currently requires compact integer arrays",
        ));
    }
    emit_ensure_unique_array_payload(source, module);
    let pass = module.next_label("usort_instance_pass");
    let index = module.next_label("usort_instance_index");
    let left = module.next_label("usort_instance_runtime_left");
    let right = module.next_label("usort_instance_runtime_right");
    let outer_done = module.next_label("usort_instance_outer_done");
    let outer_loop = module.next_label("usort_instance_outer_loop");
    let inner_done = module.next_label("usort_instance_inner_done");
    let inner_loop = module.next_label("usort_instance_inner_loop");
    module.declare_i32_local(pass.trim_start_matches('$').to_string());
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.declare_i64_local(left.trim_start_matches('$').to_string());
    module.declare_i64_local(right.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pass));
    module.body().open(&format!("block {}", outer_done));
    module.body().open(&format!("loop {}", outer_loop));
    module.body().line(&format!("local.get {}", pass));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", outer_done));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", inner_done));
    module.body().open(&format!("loop {}", inner_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.sub");
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", inner_done));
    emit_usort_runtime_compact_int_compare_swap_instance(
        source,
        &index,
        callback,
        capture_local,
        &left,
        &right,
        module,
    );
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", inner_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", pass));
    module.body().line(&format!("br {}", outer_loop));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_usort_runtime_compact_int_compare_swap(
    source: &str,
    index: &str,
    callback: &str,
    left: &str,
    right: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", right));
    module.body().line("i64.store");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", left));
    module.body().line("i64.store");
    module.body().close("end");
}

fn emit_usort_runtime_compact_int_compare_swap_instance(
    source: &str,
    index: &str,
    callback: &str,
    capture_local: &str,
    left: &str,
    right: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", right));
    module.body().line("i64.store");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", left));
    module.body().line("i64.store");
    module.body().close("end");
}

fn emit_usort_compact_int_compare_swap(
    source: &str,
    index: usize,
    callback: &str,
    left: &str,
    right: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", index * 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", (index + 1) * 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", index * 8));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", right));
    module.body().line("i64.store");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", (index + 1) * 8));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", left));
    module.body().line("i64.store");
    module.body().close("end");
}

fn emit_usort_compact_int_compare_swap_instance(
    source: &str,
    index: usize,
    callback: &str,
    capture_local: &str,
    left: &str,
    right: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", index * 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", (index + 1) * 8));
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", index * 8));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", right));
    module.body().line("i64.store");
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", (index + 1) * 8));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", left));
    module.body().line("i64.store");
    module.body().close("end");
}

pub(super) fn emit_usort_value_int_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_usort_runtime_value_int_local(source, source_span, callback, module);
    };
    emit_ensure_unique_array_payload(source, module);
    let left = module.next_label("usort_value_left");
    let right = module.next_label("usort_value_right");
    let low = module.next_label("usort_value_low");
    let high = module.next_label("usort_value_high");
    for local in [&left, &right, &low, &high] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    for pass in 0..len {
        for index in 0..len.saturating_sub(1 + pass) {
            emit_usort_value_int_compare_swap(source, index, callback, &left, &right, &low, &high, module);
        }
    }
    Ok(())
}

fn emit_usort_runtime_value_int_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value || !array_map_value_cells_are_ints(source, module) {
        return Err(CompileError::new(
            source_span,
            "wasm32-web runtime-length usort() currently requires integer value-cell arrays",
        ));
    }
    emit_ensure_unique_array_payload(source, module);
    let pass = module.next_label("usort_value_pass");
    let index = module.next_label("usort_value_index");
    let left = module.next_label("usort_value_runtime_left");
    let right = module.next_label("usort_value_runtime_right");
    let low = module.next_label("usort_value_runtime_low");
    let high = module.next_label("usort_value_runtime_high");
    let outer_done = module.next_label("usort_value_outer_done");
    let outer_loop = module.next_label("usort_value_outer_loop");
    let inner_done = module.next_label("usort_value_inner_done");
    let inner_loop = module.next_label("usort_value_inner_loop");
    for local in [&pass, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left, &right, &low, &high] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pass));
    module.body().open(&format!("block {}", outer_done));
    module.body().open(&format!("loop {}", outer_loop));
    module.body().line(&format!("local.get {}", pass));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", outer_done));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", inner_done));
    module.body().open(&format!("loop {}", inner_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.sub");
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", inner_done));
    emit_usort_runtime_value_int_compare_swap(source, &index, callback, &left, &right, &low, &high, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", inner_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", pass));
    module.body().line(&format!("br {}", outer_loop));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_dynamic_value_cell_address(name: &str, index: &str, offset: usize, module: &mut WasmModule) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    if offset != 0 {
        module.body().line(&format!("i32.const {}", offset));
        module.body().line("i32.add");
    }
    module.body().line("i32.add");
}

fn emit_dynamic_next_value_cell_address(name: &str, index: &str, offset: usize, module: &mut WasmModule) {
    module.body().line(&format!("local.get ${}_ptr", name));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    if offset != 0 {
        module.body().line(&format!("i32.const {}", offset));
        module.body().line("i32.add");
    }
    module.body().line("i32.add");
}

fn emit_usort_runtime_value_int_compare_swap(
    source: &str,
    index: &str,
    callback: &str,
    left: &str,
    right: &str,
    low: &str,
    high: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_i64");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("call $__rt_value_payload_i64");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_dynamic_value_cell_address(source, index, 0, module);
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", low));
    emit_dynamic_value_cell_address(source, index, 8, module);
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", high));
    emit_dynamic_value_cell_address(source, index, 0, module);
    emit_dynamic_next_value_cell_address(source, index, 0, module);
    module.body().line("call $__rt_value_copy");
    emit_dynamic_next_value_cell_address(source, index, 0, module);
    module.body().line(&format!("local.get {}", low));
    module.body().line("i64.store");
    emit_dynamic_next_value_cell_address(source, index, 8, module);
    module.body().line(&format!("local.get {}", high));
    module.body().line("i64.store");
    module.body().close("end");
}

fn emit_usort_value_int_compare_swap(
    source: &str,
    index: usize,
    callback: &str,
    left: &str,
    right: &str,
    low: &str,
    high: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", index));
    module.body().line("call $__rt_value_payload_i64");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", index + 1));
    module.body().line("call $__rt_value_payload_i64");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_load_value_cell_half(source, index, 0, low, module);
    emit_load_value_cell_half(source, index, 8, high, module);
    emit_copy_value_cell_slot(source, index, index + 1, module);
    emit_store_value_cell_half(source, index + 1, 0, low, module);
    emit_store_value_cell_half(source, index + 1, 8, high, module);
    module.body().close("end");
}

pub(super) fn emit_usort_value_bool_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_usort_runtime_value_bool_local(source, source_span, callback, module);
    };
    emit_ensure_unique_array_payload(source, module);
    let left = module.next_label("usort_bool_left");
    let right = module.next_label("usort_bool_right");
    let low = module.next_label("usort_bool_low");
    let high = module.next_label("usort_bool_high");
    for local in [&left, &right] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&low, &high] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    for pass in 0..len {
        for index in 0..len.saturating_sub(1 + pass) {
            emit_usort_value_bool_compare_swap(source, index, callback, &left, &right, &low, &high, module);
        }
    }
    Ok(())
}

fn emit_usort_runtime_value_bool_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Bool)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web runtime-length usort() currently requires bool value-cell arrays",
        ));
    }
    emit_ensure_unique_array_payload(source, module);
    let pass = module.next_label("usort_bool_pass");
    let index = module.next_label("usort_bool_index");
    let left = module.next_label("usort_bool_runtime_left");
    let right = module.next_label("usort_bool_runtime_right");
    let low = module.next_label("usort_bool_runtime_low");
    let high = module.next_label("usort_bool_runtime_high");
    let outer_done = module.next_label("usort_bool_outer_done");
    let outer_loop = module.next_label("usort_bool_outer_loop");
    let inner_done = module.next_label("usort_bool_inner_done");
    let inner_loop = module.next_label("usort_bool_inner_loop");
    for local in [&pass, &index, &left, &right] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&low, &high] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pass));
    module.body().open(&format!("block {}", outer_done));
    module.body().open(&format!("loop {}", outer_loop));
    module.body().line(&format!("local.get {}", pass));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", outer_done));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", inner_done));
    module.body().open(&format!("loop {}", inner_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.sub");
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", inner_done));
    emit_usort_runtime_value_bool_compare_swap(source, &index, callback, &left, &right, &low, &high, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", inner_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", pass));
    module.body().line(&format!("br {}", outer_loop));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_usort_runtime_value_bool_compare_swap(
    source: &str,
    index: &str,
    callback: &str,
    left: &str,
    right: &str,
    low: &str,
    high: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_i64");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("call $__rt_value_payload_i64");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_dynamic_value_cell_address(source, index, 0, module);
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", low));
    emit_dynamic_value_cell_address(source, index, 8, module);
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", high));
    emit_dynamic_value_cell_address(source, index, 0, module);
    emit_dynamic_next_value_cell_address(source, index, 0, module);
    module.body().line("call $__rt_value_copy");
    emit_dynamic_next_value_cell_address(source, index, 0, module);
    module.body().line(&format!("local.get {}", low));
    module.body().line("i64.store");
    emit_dynamic_next_value_cell_address(source, index, 8, module);
    module.body().line(&format!("local.get {}", high));
    module.body().line("i64.store");
    module.body().close("end");
}

fn emit_usort_value_bool_compare_swap(
    source: &str,
    index: usize,
    callback: &str,
    left: &str,
    right: &str,
    low: &str,
    high: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", index));
    module.body().line("call $__rt_value_payload_i64");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", index + 1));
    module.body().line("call $__rt_value_payload_i64");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_load_value_cell_half(source, index, 0, low, module);
    emit_load_value_cell_half(source, index, 8, high, module);
    emit_copy_value_cell_slot(source, index, index + 1, module);
    emit_store_value_cell_half(source, index + 1, 0, low, module);
    emit_store_value_cell_half(source, index + 1, 8, high, module);
    module.body().close("end");
}

pub(super) fn emit_usort_value_bool_local_instance(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_usort_runtime_value_bool_local_instance(
            source,
            source_span,
            callback,
            capture_local,
            module,
        );
    };
    emit_ensure_unique_array_payload(source, module);
    let left = module.next_label("usort_bool_instance_left");
    let right = module.next_label("usort_bool_instance_right");
    let low = module.next_label("usort_bool_instance_low");
    let high = module.next_label("usort_bool_instance_high");
    for local in [&left, &right] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&low, &high] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    for pass in 0..len {
        for index in 0..len.saturating_sub(1 + pass) {
            emit_usort_value_bool_compare_swap_instance(
                source,
                index,
                callback,
                capture_local,
                &left,
                &right,
                &low,
                &high,
                module,
            );
        }
    }
    Ok(())
}

fn emit_usort_runtime_value_bool_local_instance(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value
        || module.array_runtime_value_cell_kind(source) != Some(ValueCellKind::Bool)
    {
        return Err(CompileError::new(
            source_span,
            "wasm32-web runtime-length usort() currently requires bool value-cell arrays",
        ));
    }
    emit_ensure_unique_array_payload(source, module);
    let pass = module.next_label("usort_bool_instance_pass");
    let index = module.next_label("usort_bool_instance_index");
    let left = module.next_label("usort_bool_instance_runtime_left");
    let right = module.next_label("usort_bool_instance_runtime_right");
    let low = module.next_label("usort_bool_instance_runtime_low");
    let high = module.next_label("usort_bool_instance_runtime_high");
    let outer_done = module.next_label("usort_bool_instance_outer_done");
    let outer_loop = module.next_label("usort_bool_instance_outer_loop");
    let inner_done = module.next_label("usort_bool_instance_inner_done");
    let inner_loop = module.next_label("usort_bool_instance_inner_loop");
    for local in [&pass, &index, &left, &right] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&low, &high] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pass));
    module.body().open(&format!("block {}", outer_done));
    module.body().open(&format!("loop {}", outer_loop));
    module.body().line(&format!("local.get {}", pass));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", outer_done));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", inner_done));
    module.body().open(&format!("loop {}", inner_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.sub");
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", inner_done));
    emit_usort_runtime_value_bool_compare_swap_instance(
        source,
        &index,
        callback,
        capture_local,
        &left,
        &right,
        &low,
        &high,
        module,
    );
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", inner_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", pass));
    module.body().line(&format!("br {}", outer_loop));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_usort_runtime_value_bool_compare_swap_instance(
    source: &str,
    index: &str,
    callback: &str,
    capture_local: &str,
    left: &str,
    right: &str,
    low: &str,
    high: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("call $__rt_value_payload_i64");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("call $__rt_value_payload_i64");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_dynamic_value_cell_address(source, index, 0, module);
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", low));
    emit_dynamic_value_cell_address(source, index, 8, module);
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", high));
    emit_dynamic_value_cell_address(source, index, 0, module);
    emit_dynamic_next_value_cell_address(source, index, 0, module);
    module.body().line("call $__rt_value_copy");
    emit_dynamic_next_value_cell_address(source, index, 0, module);
    module.body().line(&format!("local.get {}", low));
    module.body().line("i64.store");
    emit_dynamic_next_value_cell_address(source, index, 8, module);
    module.body().line(&format!("local.get {}", high));
    module.body().line("i64.store");
    module.body().close("end");
}

fn emit_usort_value_bool_compare_swap_instance(
    source: &str,
    index: usize,
    callback: &str,
    capture_local: &str,
    left: &str,
    right: &str,
    low: &str,
    high: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", index));
    module.body().line("call $__rt_value_payload_i64");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", index + 1));
    module.body().line("call $__rt_value_payload_i64");
    module.body().line("i64.const 0");
    module.body().line("i64.ne");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_load_value_cell_half(source, index, 0, low, module);
    emit_load_value_cell_half(source, index, 8, high, module);
    emit_copy_value_cell_slot(source, index, index + 1, module);
    emit_store_value_cell_half(source, index + 1, 0, low, module);
    emit_store_value_cell_half(source, index + 1, 8, high, module);
    module.body().close("end");
}

pub(super) fn emit_usort_value_float_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_usort_runtime_value_float_local(source, source_span, callback, module);
    };
    emit_ensure_unique_array_payload(source, module);
    let left = module.next_label("usort_float_left");
    let right = module.next_label("usort_float_right");
    let low = module.next_label("usort_float_low");
    let high = module.next_label("usort_float_high");
    for local in [&left, &right] {
        module.declare_f64_local(local.trim_start_matches('$').to_string());
    }
    for local in [&low, &high] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    for pass in 0..len {
        for index in 0..len.saturating_sub(1 + pass) {
            emit_usort_value_float_compare_swap(source, index, callback, &left, &right, &low, &high, module);
        }
    }
    Ok(())
}

pub(super) fn emit_usort_value_float_local_instance(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_usort_runtime_value_float_local_instance(
            source,
            source_span,
            callback,
            capture_local,
            module,
        );
    };
    emit_ensure_unique_array_payload(source, module);
    let left = module.next_label("usort_float_instance_left");
    let right = module.next_label("usort_float_instance_right");
    let low = module.next_label("usort_float_instance_low");
    let high = module.next_label("usort_float_instance_high");
    for local in [&left, &right] {
        module.declare_f64_local(local.trim_start_matches('$').to_string());
    }
    for local in [&low, &high] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    for pass in 0..len {
        for index in 0..len.saturating_sub(1 + pass) {
            emit_usort_value_float_compare_swap_instance(
                source,
                index,
                callback,
                capture_local,
                &left,
                &right,
                &low,
                &high,
                module,
            );
        }
    }
    Ok(())
}

fn emit_usort_runtime_value_float_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value || !array_filter_value_cells_are_floats(source, module) {
        return Err(CompileError::new(
            source_span,
            "wasm32-web runtime-length usort() currently requires float value-cell arrays",
        ));
    }
    emit_ensure_unique_array_payload(source, module);
    let pass = module.next_label("usort_float_pass");
    let index = module.next_label("usort_float_index");
    let left = module.next_label("usort_float_runtime_left");
    let right = module.next_label("usort_float_runtime_right");
    let low = module.next_label("usort_float_runtime_low");
    let high = module.next_label("usort_float_runtime_high");
    let outer_done = module.next_label("usort_float_outer_done");
    let outer_loop = module.next_label("usort_float_outer_loop");
    let inner_done = module.next_label("usort_float_inner_done");
    let inner_loop = module.next_label("usort_float_inner_loop");
    for local in [&pass, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left, &right] {
        module.declare_f64_local(local.trim_start_matches('$').to_string());
    }
    for local in [&low, &high] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pass));
    module.body().open(&format!("block {}", outer_done));
    module.body().open(&format!("loop {}", outer_loop));
    module.body().line(&format!("local.get {}", pass));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", outer_done));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", inner_done));
    module.body().open(&format!("loop {}", inner_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.sub");
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", inner_done));
    emit_usort_runtime_value_float_compare_swap(source, &index, callback, &left, &right, &low, &high, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", inner_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", pass));
    module.body().line(&format!("br {}", outer_loop));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_usort_runtime_value_float_local_instance(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value || !array_filter_value_cells_are_floats(source, module) {
        return Err(CompileError::new(
            source_span,
            "wasm32-web runtime-length usort() currently requires float value-cell arrays",
        ));
    }
    emit_ensure_unique_array_payload(source, module);
    let pass = module.next_label("usort_float_instance_pass");
    let index = module.next_label("usort_float_instance_index");
    let left = module.next_label("usort_float_instance_runtime_left");
    let right = module.next_label("usort_float_instance_runtime_right");
    let low = module.next_label("usort_float_instance_runtime_low");
    let high = module.next_label("usort_float_instance_runtime_high");
    let outer_done = module.next_label("usort_float_instance_outer_done");
    let outer_loop = module.next_label("usort_float_instance_outer_loop");
    let inner_done = module.next_label("usort_float_instance_inner_done");
    let inner_loop = module.next_label("usort_float_instance_inner_loop");
    for local in [&pass, &index] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&left, &right] {
        module.declare_f64_local(local.trim_start_matches('$').to_string());
    }
    for local in [&low, &high] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pass));
    module.body().open(&format!("block {}", outer_done));
    module.body().open(&format!("loop {}", outer_loop));
    module.body().line(&format!("local.get {}", pass));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", outer_done));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", inner_done));
    module.body().open(&format!("loop {}", inner_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.sub");
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", inner_done));
    emit_usort_runtime_value_float_compare_swap_instance(
        source,
        &index,
        callback,
        capture_local,
        &left,
        &right,
        &low,
        &high,
        module,
    );
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", inner_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", pass));
    module.body().line(&format!("br {}", outer_loop));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_usort_runtime_value_float_compare_swap(
    source: &str,
    index: &str,
    callback: &str,
    left: &str,
    right: &str,
    low: &str,
    high: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_dynamic_value_cell_address(source, index, 0, module);
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", low));
    emit_dynamic_value_cell_address(source, index, 8, module);
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", high));
    emit_dynamic_value_cell_address(source, index, 0, module);
    emit_dynamic_next_value_cell_address(source, index, 0, module);
    module.body().line("call $__rt_value_copy");
    emit_dynamic_next_value_cell_address(source, index, 0, module);
    module.body().line(&format!("local.get {}", low));
    module.body().line("i64.store");
    emit_dynamic_next_value_cell_address(source, index, 8, module);
    module.body().line(&format!("local.get {}", high));
    module.body().line("i64.store");
    module.body().close("end");
}

fn emit_usort_runtime_value_float_compare_swap_instance(
    source: &str,
    index: &str,
    callback: &str,
    capture_local: &str,
    left: &str,
    right: &str,
    low: &str,
    high: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("i32.const {}", WASM_VALUE_CELL_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_dynamic_value_cell_address(source, index, 0, module);
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", low));
    emit_dynamic_value_cell_address(source, index, 8, module);
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", high));
    emit_dynamic_value_cell_address(source, index, 0, module);
    emit_dynamic_next_value_cell_address(source, index, 0, module);
    module.body().line("call $__rt_value_copy");
    emit_dynamic_next_value_cell_address(source, index, 0, module);
    module.body().line(&format!("local.get {}", low));
    module.body().line("i64.store");
    emit_dynamic_next_value_cell_address(source, index, 8, module);
    module.body().line(&format!("local.get {}", high));
    module.body().line("i64.store");
    module.body().close("end");
}

fn emit_usort_value_float_compare_swap(
    source: &str,
    index: usize,
    callback: &str,
    left: &str,
    right: &str,
    low: &str,
    high: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE + 8));
    module.body().line("i32.add");
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", (index + 1) * WASM_VALUE_CELL_SIZE + 8));
    module.body().line("i32.add");
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_load_value_cell_half(source, index, 0, low, module);
    emit_load_value_cell_half(source, index, 8, high, module);
    emit_copy_value_cell_slot(source, index, index + 1, module);
    emit_store_value_cell_half(source, index + 1, 0, low, module);
    emit_store_value_cell_half(source, index + 1, 8, high, module);
    module.body().close("end");
}

fn emit_usort_value_float_compare_swap_instance(
    source: &str,
    index: usize,
    callback: &str,
    capture_local: &str,
    left: &str,
    right: &str,
    low: &str,
    high: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", index * WASM_VALUE_CELL_SIZE + 8));
    module.body().line("i32.add");
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", left));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", (index + 1) * WASM_VALUE_CELL_SIZE + 8));
    module.body().line("i32.add");
    module.body().line("f64.load");
    module.body().line(&format!("local.set {}", right));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get {}", left));
    module.body().line(&format!("local.get {}", right));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_load_value_cell_half(source, index, 0, low, module);
    emit_load_value_cell_half(source, index, 8, high, module);
    emit_copy_value_cell_slot(source, index, index + 1, module);
    emit_store_value_cell_half(source, index + 1, 0, low, module);
    emit_store_value_cell_half(source, index + 1, 8, high, module);
    module.body().close("end");
}

pub(super) fn emit_usort_value_string_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_usort_runtime_value_string_local(source, source_span, callback, module);
    };
    emit_ensure_unique_array_payload(source, module);
    let left_ptr = module.next_label("usort_string_left_ptr");
    let left_len = module.next_label("usort_string_left_len");
    let right_ptr = module.next_label("usort_string_right_ptr");
    let right_len = module.next_label("usort_string_right_len");
    for local in [&left_ptr, &left_len, &right_ptr, &right_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    let low = module.next_label("usort_string_low");
    let high = module.next_label("usort_string_high");
    for local in [&low, &high] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    for pass in 0..len {
        for index in 0..len.saturating_sub(1 + pass) {
            emit_usort_value_string_compare_swap(
                source, index, callback, &left_ptr, &left_len, &right_ptr, &right_len, &low, &high, module,
            );
        }
    }
    Ok(())
}

pub(super) fn emit_usort_value_string_local_instance(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let Some(len) = module.array_length(source) else {
        return emit_usort_runtime_value_string_local_instance(
            source,
            source_span,
            callback,
            capture_local,
            module,
        );
    };
    emit_ensure_unique_array_payload(source, module);
    let left_ptr = module.next_label("usort_string_instance_left_ptr");
    let left_len = module.next_label("usort_string_instance_left_len");
    let right_ptr = module.next_label("usort_string_instance_right_ptr");
    let right_len = module.next_label("usort_string_instance_right_len");
    for local in [&left_ptr, &left_len, &right_ptr, &right_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    let low = module.next_label("usort_string_instance_low");
    let high = module.next_label("usort_string_instance_high");
    for local in [&low, &high] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    for pass in 0..len {
        for index in 0..len.saturating_sub(1 + pass) {
            emit_usort_value_string_compare_swap_instance(
                source,
                index,
                callback,
                capture_local,
                &left_ptr,
                &left_len,
                &right_ptr,
                &right_len,
                &low,
                &high,
                module,
            );
        }
    }
    Ok(())
}

fn emit_usort_runtime_value_string_local(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value || !array_map_value_cells_are_strings(source, module) {
        return Err(CompileError::new(
            source_span,
            "wasm32-web runtime-length usort() currently requires string value-cell arrays",
        ));
    }
    emit_ensure_unique_array_payload(source, module);
    let pass = module.next_label("usort_string_pass");
    let index = module.next_label("usort_string_index");
    let left_ptr = module.next_label("usort_string_runtime_left_ptr");
    let left_len = module.next_label("usort_string_runtime_left_len");
    let right_ptr = module.next_label("usort_string_runtime_right_ptr");
    let right_len = module.next_label("usort_string_runtime_right_len");
    let low = module.next_label("usort_string_runtime_low");
    let high = module.next_label("usort_string_runtime_high");
    let outer_done = module.next_label("usort_string_outer_done");
    let outer_loop = module.next_label("usort_string_outer_loop");
    let inner_done = module.next_label("usort_string_inner_done");
    let inner_loop = module.next_label("usort_string_inner_loop");
    for local in [&pass, &index, &left_ptr, &left_len, &right_ptr, &right_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&low, &high] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pass));
    module.body().open(&format!("block {}", outer_done));
    module.body().open(&format!("loop {}", outer_loop));
    module.body().line(&format!("local.get {}", pass));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", outer_done));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", inner_done));
    module.body().open(&format!("loop {}", inner_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.sub");
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", inner_done));
    emit_usort_runtime_value_string_compare_swap(
        source, &index, callback, &left_ptr, &left_len, &right_ptr, &right_len, &low, &high, module,
    );
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", inner_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", pass));
    module.body().line(&format!("br {}", outer_loop));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_usort_runtime_value_string_local_instance(
    source: &str,
    source_span: crate::span::Span,
    callback: &str,
    capture_local: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if module.array_layout(source) != ArrayLayout::Value || !array_map_value_cells_are_strings(source, module) {
        return Err(CompileError::new(
            source_span,
            "wasm32-web runtime-length usort() currently requires string value-cell arrays",
        ));
    }
    emit_ensure_unique_array_payload(source, module);
    let pass = module.next_label("usort_string_instance_pass");
    let index = module.next_label("usort_string_instance_index");
    let left_ptr = module.next_label("usort_string_instance_runtime_left_ptr");
    let left_len = module.next_label("usort_string_instance_runtime_left_len");
    let right_ptr = module.next_label("usort_string_instance_runtime_right_ptr");
    let right_len = module.next_label("usort_string_instance_runtime_right_len");
    let low = module.next_label("usort_string_instance_runtime_low");
    let high = module.next_label("usort_string_instance_runtime_high");
    let outer_done = module.next_label("usort_string_instance_outer_done");
    let outer_loop = module.next_label("usort_string_instance_outer_loop");
    let inner_done = module.next_label("usort_string_instance_inner_done");
    let inner_loop = module.next_label("usort_string_instance_inner_loop");
    for local in [&pass, &index, &left_ptr, &left_len, &right_ptr, &right_len] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    for local in [&low, &high] {
        module.declare_i64_local(local.trim_start_matches('$').to_string());
    }
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", pass));
    module.body().open(&format!("block {}", outer_done));
    module.body().open(&format!("loop {}", outer_loop));
    module.body().line(&format!("local.get {}", pass));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", outer_done));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", inner_done));
    module.body().open(&format!("loop {}", inner_loop));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.sub");
    module.body().line("i32.const 1");
    module.body().line("i32.sub");
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", inner_done));
    emit_usort_runtime_value_string_compare_swap_instance(
        source,
        &index,
        callback,
        capture_local,
        &left_ptr,
        &left_len,
        &right_ptr,
        &right_len,
        &low,
        &high,
        module,
    );
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", inner_loop));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", pass));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", pass));
    module.body().line(&format!("br {}", outer_loop));
    module.body().close("end");
    module.body().close("end");
    Ok(())
}

fn emit_usort_runtime_value_string_compare_swap(
    source: &str,
    index: &str,
    callback: &str,
    left_ptr: &str,
    left_len: &str,
    right_ptr: &str,
    right_len: &str,
    low: &str,
    high: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", left_ptr));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", left_len));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", right_ptr));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", right_len));
    module.body().line(&format!("local.get {}", left_ptr));
    module.body().line(&format!("local.get {}", left_len));
    module.body().line(&format!("local.get {}", right_ptr));
    module.body().line(&format!("local.get {}", right_len));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_dynamic_value_cell_address(source, index, 0, module);
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", low));
    emit_dynamic_value_cell_address(source, index, 8, module);
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", high));
    emit_dynamic_value_cell_address(source, index, 0, module);
    emit_dynamic_next_value_cell_address(source, index, 0, module);
    module.body().line("call $__rt_value_copy");
    emit_dynamic_next_value_cell_address(source, index, 0, module);
    module.body().line(&format!("local.get {}", low));
    module.body().line("i64.store");
    emit_dynamic_next_value_cell_address(source, index, 8, module);
    module.body().line(&format!("local.get {}", high));
    module.body().line("i64.store");
    module.body().close("end");
}

fn emit_usort_runtime_value_string_compare_swap_instance(
    source: &str,
    index: &str,
    callback: &str,
    capture_local: &str,
    left_ptr: &str,
    left_len: &str,
    right_ptr: &str,
    right_len: &str,
    low: &str,
    high: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", left_ptr));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", left_len));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", right_ptr));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", right_len));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get {}", left_ptr));
    module.body().line(&format!("local.get {}", left_len));
    module.body().line(&format!("local.get {}", right_ptr));
    module.body().line(&format!("local.get {}", right_len));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_dynamic_value_cell_address(source, index, 0, module);
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", low));
    emit_dynamic_value_cell_address(source, index, 8, module);
    module.body().line("i64.load");
    module.body().line(&format!("local.set {}", high));
    emit_dynamic_value_cell_address(source, index, 0, module);
    emit_dynamic_next_value_cell_address(source, index, 0, module);
    module.body().line("call $__rt_value_copy");
    module.body().line("call $__rt_value_copy");
    emit_dynamic_next_value_cell_address(source, index, 0, module);
    module.body().line(&format!("local.get {}", low));
    module.body().line("i64.store");
    emit_dynamic_next_value_cell_address(source, index, 8, module);
    module.body().line(&format!("local.get {}", high));
    module.body().line("i64.store");
    module.body().close("end");
}

fn emit_usort_value_string_compare_swap(
    source: &str,
    index: usize,
    callback: &str,
    left_ptr: &str,
    left_len: &str,
    right_ptr: &str,
    right_len: &str,
    low: &str,
    high: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", index));
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", left_ptr));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", left_len));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", index + 1));
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", right_ptr));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", index + 1));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", right_len));
    module.body().line(&format!("local.get {}", left_ptr));
    module.body().line(&format!("local.get {}", left_len));
    module.body().line(&format!("local.get {}", right_ptr));
    module.body().line(&format!("local.get {}", right_len));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_load_value_cell_half(source, index, 0, low, module);
    emit_load_value_cell_half(source, index, 8, high, module);
    emit_copy_value_cell_slot(source, index, index + 1, module);
    emit_store_value_cell_half(source, index + 1, 0, low, module);
    emit_store_value_cell_half(source, index + 1, 8, high, module);
    module.body().close("end");
}

fn emit_usort_value_string_compare_swap_instance(
    source: &str,
    index: usize,
    callback: &str,
    capture_local: &str,
    left_ptr: &str,
    left_len: &str,
    right_ptr: &str,
    right_len: &str,
    low: &str,
    high: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", index));
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", left_ptr));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", index));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", left_len));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", index + 1));
    module.body().line("i32.const 8");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", right_ptr));
    module.body().line(&format!("local.get ${}_ptr", source));
    module.body().line(&format!("i32.const {}", index + 1));
    module.body().line("i32.const 12");
    module.body().line("call $__rt_value_payload_i32");
    module.body().line(&format!("local.set {}", right_len));
    module.body().line(&format!("local.get ${}", capture_local));
    module.body().line(&format!("local.get {}", left_ptr));
    module.body().line(&format!("local.get {}", left_len));
    module.body().line(&format!("local.get {}", right_ptr));
    module.body().line(&format!("local.get {}", right_len));
    module
        .body()
        .line(&format!("call ${}", wasm_function_name(callback)));
    module.body().line("i64.const 0");
    module.body().line("i64.gt_s");
    module.body().open("if");
    emit_load_value_cell_half(source, index, 0, low, module);
    emit_load_value_cell_half(source, index, 8, high, module);
    emit_copy_value_cell_slot(source, index, index + 1, module);
    emit_store_value_cell_half(source, index + 1, 0, low, module);
    emit_store_value_cell_half(source, index + 1, 8, high, module);
    module.body().close("end");
}
