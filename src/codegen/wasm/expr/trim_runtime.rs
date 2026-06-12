//! Purpose:
//! Emits wasm32-web runtime lowering for PHP trim/ltrim/rtrim on heap strings.
//! Keeps trim byte matching and charlist parsing out of the main expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::string_builtins`
//! - `crate::codegen::wasm::expr::string_values`
//!
//! Key details:
//! - Supports static and runtime charlists, including PHP-style byte ranges.
//! - Emits pointer/length pairs for value contexts and host writes for output contexts.

use super::*;

enum TrimCharlistAtom {
    Byte(u8),
    Range(u8, u8),
}



pub(super) fn emit_runtime_trim(
    var: &str,
    side: TrimSide,
    charlist: Option<&str>,
    module: &mut WasmModule,
) {
    let start = module.next_label("str_start");
    let end = module.next_label("str_end");
    let byte = module.next_label("str_byte");
    let left_loop = module.next_label("trim_left_loop");
    let left_done = module.next_label("trim_left_done");
    let right_loop = module.next_label("trim_right_loop");
    let right_done = module.next_label("trim_right_done");
    module.declare_i32_local(start.trim_start_matches('$').to_string());
    module.declare_i32_local(end.trim_start_matches('$').to_string());
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", start));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.set {}", end));

    if matches!(side, TrimSide::Left | TrimSide::Both) {
        module.body().open(&format!("block {}", left_done));
        module.body().open(&format!("loop {}", left_loop));
        module.body().line(&format!("local.get {}", start));
        module.body().line(&format!("local.get {}", end));
        module.body().line("i32.ge_u");
        module.body().line(&format!("br_if {}", left_done));
        emit_load_string_byte(var, &start, module);
        module.body().line(&format!("local.set {}", byte));
        emit_trim_byte_condition(&byte, charlist, module);
        module.body().line("i32.eqz");
        module.body().line(&format!("br_if {}", left_done));
        module.body().line(&format!("local.get {}", start));
        module.body().line("i32.const 1");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", start));
        module.body().line(&format!("br {}", left_loop));
        module.body().close("end");
        module.body().close("end");
    }

    if matches!(side, TrimSide::Right | TrimSide::Both) {
        module.body().open(&format!("block {}", right_done));
        module.body().open(&format!("loop {}", right_loop));
        module.body().line(&format!("local.get {}", end));
        module.body().line(&format!("local.get {}", start));
        module.body().line("i32.le_u");
        module.body().line(&format!("br_if {}", right_done));
        module.body().line(&format!("local.get {}", end));
        module.body().line("i32.const 1");
        module.body().line("i32.sub");
        module.body().line(&format!("local.set {}", end));
        emit_load_string_byte(var, &end, module);
        module.body().line(&format!("local.set {}", byte));
        emit_trim_byte_condition(&byte, charlist, module);
        module.body().line("i32.eqz");
        module.body().open("if");
        module.body().line(&format!("local.get {}", end));
        module.body().line("i32.const 1");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", end));
        module.body().line(&format!("br {}", right_done));
        module.body().close("end");
        module.body().line(&format!("br {}", right_loop));
        module.body().close("end");
        module.body().close("end");
    }

    emit_write_string_range(var, &start, &end, module);
}

pub(super) fn emit_runtime_trim_var_charlist(
    var: &str,
    side: TrimSide,
    charlist_var: &str,
    module: &mut WasmModule,
) {
    let start = module.next_label("str_start");
    let end = module.next_label("str_end");
    let byte = module.next_label("str_byte");
    let left_loop = module.next_label("trim_left_loop");
    let left_done = module.next_label("trim_left_done");
    let right_loop = module.next_label("trim_right_loop");
    let right_done = module.next_label("trim_right_done");
    module.declare_i32_local(start.trim_start_matches('$').to_string());
    module.declare_i32_local(end.trim_start_matches('$').to_string());
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", start));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.set {}", end));

    if matches!(side, TrimSide::Left | TrimSide::Both) {
        module.body().open(&format!("block {}", left_done));
        module.body().open(&format!("loop {}", left_loop));
        module.body().line(&format!("local.get {}", start));
        module.body().line(&format!("local.get {}", end));
        module.body().line("i32.ge_u");
        module.body().line(&format!("br_if {}", left_done));
        emit_load_string_byte(var, &start, module);
        module.body().line(&format!("local.set {}", byte));
        emit_trim_byte_condition_var(&byte, charlist_var, module);
        module.body().line("i32.eqz");
        module.body().line(&format!("br_if {}", left_done));
        module.body().line(&format!("local.get {}", start));
        module.body().line("i32.const 1");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", start));
        module.body().line(&format!("br {}", left_loop));
        module.body().close("end");
        module.body().close("end");
    }

    if matches!(side, TrimSide::Right | TrimSide::Both) {
        module.body().open(&format!("block {}", right_done));
        module.body().open(&format!("loop {}", right_loop));
        module.body().line(&format!("local.get {}", end));
        module.body().line(&format!("local.get {}", start));
        module.body().line("i32.le_u");
        module.body().line(&format!("br_if {}", right_done));
        module.body().line(&format!("local.get {}", end));
        module.body().line("i32.const 1");
        module.body().line("i32.sub");
        module.body().line(&format!("local.set {}", end));
        emit_load_string_byte(var, &end, module);
        module.body().line(&format!("local.set {}", byte));
        emit_trim_byte_condition_var(&byte, charlist_var, module);
        module.body().line("i32.eqz");
        module.body().open("if");
        module.body().line(&format!("local.get {}", end));
        module.body().line("i32.const 1");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", end));
        module.body().line(&format!("br {}", right_done));
        module.body().close("end");
        module.body().line(&format!("br {}", right_loop));
        module.body().close("end");
        module.body().close("end");
    }

    emit_write_string_range(var, &start, &end, module);
}

pub(super) fn emit_runtime_trim_value_to_stack(
    var: &str,
    side: TrimSide,
    charlist: Option<&str>,
    charlist_var: Option<&str>,
    module: &mut WasmModule,
) {
    let start = module.next_label("trim_value_start");
    let end = module.next_label("trim_value_end");
    let byte = module.next_label("trim_value_byte");
    let left_loop = module.next_label("trim_value_left_loop");
    let left_done = module.next_label("trim_value_left_done");
    let right_loop = module.next_label("trim_value_right_loop");
    let right_done = module.next_label("trim_value_right_done");
    module.declare_i32_local(start.trim_start_matches('$').to_string());
    module.declare_i32_local(end.trim_start_matches('$').to_string());
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", start));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("local.set {}", end));

    if matches!(side, TrimSide::Left | TrimSide::Both) {
        module.body().open(&format!("block {}", left_done));
        module.body().open(&format!("loop {}", left_loop));
        module.body().line(&format!("local.get {}", start));
        module.body().line(&format!("local.get {}", end));
        module.body().line("i32.ge_u");
        module.body().line(&format!("br_if {}", left_done));
        emit_load_string_byte(var, &start, module);
        module.body().line(&format!("local.set {}", byte));
        emit_runtime_trim_value_condition(&byte, charlist, charlist_var, module);
        module.body().line("i32.eqz");
        module.body().line(&format!("br_if {}", left_done));
        module.body().line(&format!("local.get {}", start));
        module.body().line("i32.const 1");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", start));
        module.body().line(&format!("br {}", left_loop));
        module.body().close("end");
        module.body().close("end");
    }

    if matches!(side, TrimSide::Right | TrimSide::Both) {
        module.body().open(&format!("block {}", right_done));
        module.body().open(&format!("loop {}", right_loop));
        module.body().line(&format!("local.get {}", end));
        module.body().line(&format!("local.get {}", start));
        module.body().line("i32.le_u");
        module.body().line(&format!("br_if {}", right_done));
        module.body().line(&format!("local.get {}", end));
        module.body().line("i32.const 1");
        module.body().line("i32.sub");
        module.body().line(&format!("local.set {}", end));
        emit_load_string_byte(var, &end, module);
        module.body().line(&format!("local.set {}", byte));
        emit_runtime_trim_value_condition(&byte, charlist, charlist_var, module);
        module.body().line("i32.eqz");
        module.body().open("if");
        module.body().line(&format!("local.get {}", end));
        module.body().line("i32.const 1");
        module.body().line("i32.add");
        module.body().line(&format!("local.set {}", end));
        module.body().line(&format!("br {}", right_done));
        module.body().close("end");
        module.body().line(&format!("br {}", right_loop));
        module.body().close("end");
        module.body().close("end");
    }

    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.add");
    module.body().line(&format!("local.get {}", end));
    module.body().line(&format!("local.get {}", start));
    module.body().line("i32.sub");
}

fn emit_runtime_trim_value_condition(
    byte: &str,
    charlist: Option<&str>,
    charlist_var: Option<&str>,
    module: &mut WasmModule,
) {
    if let Some(charlist_var) = charlist_var {
        emit_trim_byte_condition_var(byte, charlist_var, module);
    } else {
        emit_trim_byte_condition(byte, charlist, module);
    }
}

fn emit_default_trim_byte_condition(byte_local: &str, module: &mut WasmModule) {
    for byte in [0, 9, 10, 11, 13, 32] {
        module.body().line(&format!("local.get {}", byte_local));
        module.body().line(&format!("i32.const {}", byte));
        module.body().line("i32.eq");
    }
    for _ in 1..6 {
        module.body().line("i32.or");
    }
}

fn emit_trim_byte_condition(
    byte_local: &str,
    charlist: Option<&str>,
    module: &mut WasmModule,
) {
    let Some(charlist) = charlist else {
        emit_default_trim_byte_condition(byte_local, module);
        return;
    };
    if charlist.is_empty() {
        module.body().line("i32.const 0");
        return;
    }
    let atoms = trim_charlist_atoms(charlist);
    if atoms.is_empty() {
        module.body().line("i32.const 0");
        return;
    }
    for atom in &atoms {
        match atom {
            TrimCharlistAtom::Byte(byte) => {
                module.body().line(&format!("local.get {}", byte_local));
                module.body().line(&format!("i32.const {}", byte));
                module.body().line("i32.eq");
            }
            TrimCharlistAtom::Range(start, end) => {
                module.body().line(&format!("local.get {}", byte_local));
                module.body().line(&format!("i32.const {}", start));
                module.body().line("i32.ge_u");
                module.body().line(&format!("local.get {}", byte_local));
                module.body().line(&format!("i32.const {}", end));
                module.body().line("i32.le_u");
                module.body().line("i32.and");
            }
        }
    }
    for _ in 1..atoms.len() {
        module.body().line("i32.or");
    }
}

fn trim_charlist_atoms(charlist: &str) -> Vec<TrimCharlistAtom> {
    let bytes = charlist.as_bytes();
    let mut atoms = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if index + 3 < bytes.len()
            && bytes[index + 1] == b'.'
            && bytes[index + 2] == b'.'
            && bytes[index] <= bytes[index + 3]
        {
            atoms.push(TrimCharlistAtom::Range(bytes[index], bytes[index + 3]));
            index += 4;
        } else {
            atoms.push(TrimCharlistAtom::Byte(bytes[index]));
            index += 1;
        }
    }
    atoms
}

fn emit_trim_byte_condition_var(byte_local: &str, charlist_var: &str, module: &mut WasmModule) {
    let idx = module.next_label("trim_chars_idx");
    let found = module.next_label("trim_chars_found");
    let current = module.next_label("trim_chars_current");
    let range_end = module.next_label("trim_chars_range_end");
    let loop_label = module.next_label("trim_chars_loop");
    let done_label = module.next_label("trim_chars_done");
    module.declare_i32_local(idx.trim_start_matches('$').to_string());
    module.declare_i32_local(found.trim_start_matches('$').to_string());
    module.declare_i32_local(current.trim_start_matches('$').to_string());
    module.declare_i32_local(range_end.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", idx));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", found));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", idx));
    module.body().line(&format!("local.get ${}_len", charlist_var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", charlist_var));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", current));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 3");
    module.body().line("i32.add");
    module.body().line(&format!("local.get ${}_len", charlist_var));
    module.body().line("i32.lt_u");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", charlist_var));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.const 46");
    module.body().line("i32.eq");
    module.body().line(&format!("local.get ${}_ptr", charlist_var));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 2");
    module.body().line("i32.add");
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line("i32.const 46");
    module.body().line("i32.eq");
    module.body().line("i32.and");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", charlist_var));
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 3");
    module.body().line("i32.add");
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", range_end));
    module.body().line(&format!("local.get {}", current));
    module.body().line(&format!("local.get {}", range_end));
    module.body().line("i32.le_u");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line(&format!("local.get {}", current));
    module.body().line("i32.ge_u");
    module.body().line("i32.and");
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line(&format!("local.get {}", range_end));
    module.body().line("i32.le_u");
    module.body().line("i32.and");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 4");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", current));
    module.body().line(&format!("local.get {}", byte_local));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", found));
    module.body().line(&format!("br {}", done_label));
    module.body().close("end");
    module.body().line(&format!("local.get {}", idx));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", idx));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", found));
}
