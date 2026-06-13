//! Purpose:
//! Lowers JSON validation and JSON error-state builtins for wasm32-web.
//! Owns json_validate(), json_last_error(), json_last_error_msg(), and last-error reset emission.
//!
//! Called from:
//! - `crate::codegen::wasm::expr`
//! - `crate::codegen::wasm::expr::json_encode`
//!
//! Key details:
//! - Keeps JSON error-state globals and literal validation helpers separate from json_encode lowering.

use super::*;

pub(super) fn emit_json_validate_call(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if args.is_empty() || args.len() > 3 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web json_validate() expects one to three arguments",
        ));
    }
    let depth = json_validate_depth_arg(args.get(1), module)?;
    let flags = json_validate_flags_arg(call, args.get(2))?;
    let static_depth = match &depth {
        JsonValidateDepth::Static(depth) => Some(*depth),
        JsonValidateDepth::Dynamic(_) => None,
    };
    if let Some(depth) = static_depth {
        if let Ok(value) = static_ascii_string_arg(call, &args[0], module) {
            let error = json_validate_literal_error(&value, depth);
            module.body().line(&format!("i32.const {}", error));
            module.body().line("global.set $json_last_error");
            module.body().line(&format!("i32.const {}", i32::from(error == 0)));
            return Ok(ValueKind::Bool);
        }
    }
    let Some(var) = string_arg_or_materialize(&args[0], "json_validate_arg", module)? else {
        return Err(CompileError::new(
            args[0].span,
            "wasm32-web json_validate() currently requires a string value",
        ));
    };
    let depth_local = match depth {
        JsonValidateDepth::Static(_) => None,
        JsonValidateDepth::Dynamic(local) => Some(local),
    };
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get ${}_len", var));
    match &depth_local {
        Some(local) => module.body().line(&format!("local.get ${}", local)),
        None => {
            module
                .body()
                .line(&format!("i32.const {}", static_depth.expect("static depth")));
        }
    }
    module.body().line(&format!("i32.const {}", flags));
    module.body().line("call $host_json_validate");
    module.body().line("global.set $json_last_error");
    module.body().line("global.get $json_last_error");
    module.body().line("i32.eqz");
    Ok(ValueKind::Bool)
}

enum JsonValidateDepth {
    Static(usize),
    Dynamic(String),
}

fn json_validate_depth_arg(
    arg: Option<&Expr>,
    module: &mut WasmModule,
) -> Result<JsonValidateDepth, CompileError> {
    let Some(arg) = arg else {
        return Ok(JsonValidateDepth::Static(512));
    };
    if let Some(depth) = static_or_const_int_value(arg) {
        if depth < 1 {
            return Err(CompileError::new(
                arg.span,
                "wasm32-web json_validate() depth must be greater than zero",
            ));
        }
        return Ok(JsonValidateDepth::Static(depth as usize));
    }
    let local = module
        .next_label("json_validate_depth")
        .trim_start_matches('$')
        .to_string();
    module.declare_i32_local(local.clone());
    require_int(arg, module)?;
    module.body().line("i32.wrap_i64");
    module.body().line(&format!("local.set ${}", local));
    module.body().line(&format!("local.get ${}", local));
    module.body().line("i32.const 1");
    module.body().line("i32.lt_s");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    Ok(JsonValidateDepth::Dynamic(local))
}

pub(super) fn emit_json_last_error_call(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if !args.is_empty() {
        return Err(CompileError::new(
            call.span,
            "wasm32-web json_last_error() takes no arguments",
        ));
    }
    module.body().line("global.get $json_last_error");
    module.body().line("i64.extend_i32_s");
    Ok(ValueKind::Int)
}

pub(super) fn emit_json_last_error_none(module: &mut WasmModule) {
    module.body().line("i32.const 0");
    module.body().line("global.set $json_last_error");
}

pub(super) fn emit_json_last_error_msg_value_to_stack(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if !args.is_empty() {
        return Err(CompileError::new(
            call.span,
            "wasm32-web json_last_error_msg() takes no arguments",
        ));
    }
    let (no_error_ptr, no_error_len) = module.intern_string("No error");
    let (depth_ptr, depth_len) = module.intern_string("Maximum stack depth exceeded");
    let (syntax_ptr, syntax_len) = module.intern_string("Syntax error");
    let ptr_local = module.next_label("json_error_msg_ptr").trim_start_matches('$').to_string();
    let len_local = module.next_label("json_error_msg_len").trim_start_matches('$').to_string();
    module.declare_i32_local(ptr_local.clone());
    module.declare_i32_local(len_local.clone());
    module.body().line(&format!("i32.const {}", no_error_ptr));
    module.body().line(&format!("local.set ${}", ptr_local));
    module.body().line(&format!("i32.const {}", no_error_len));
    module.body().line(&format!("local.set ${}", len_local));
    module.body().line("global.get $json_last_error");
    module.body().line("i32.const 1");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("i32.const {}", depth_ptr));
    module.body().line(&format!("local.set ${}", ptr_local));
    module.body().line(&format!("i32.const {}", depth_len));
    module.body().line(&format!("local.set ${}", len_local));
    module.body().close("end");
    module.body().line("global.get $json_last_error");
    module.body().line("i32.const 4");
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("i32.const {}", syntax_ptr));
    module.body().line(&format!("local.set ${}", ptr_local));
    module.body().line(&format!("i32.const {}", syntax_len));
    module.body().line(&format!("local.set ${}", len_local));
    module.body().close("end");
    module.body().line(&format!("local.get ${}", ptr_local));
    module.body().line(&format!("local.get ${}", len_local));
    Ok(())
}

fn json_validate_flags_arg(call: &Expr, arg: Option<&Expr>) -> Result<i64, CompileError> {
    let flags = arg.map(const_or_literal_int_arg).transpose()?.unwrap_or(0);
    if flags & !1_048_576 != 0 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web json_validate() flags currently support only JSON_INVALID_UTF8_IGNORE",
        ));
    }
    Ok(flags)
}

fn json_validate_literal_error(value: &str, max_depth: usize) -> i32 {
    if json_validate_literal(value, max_depth) {
        return 0;
    }
    if json_validate_literal(value, usize::MAX) {
        return 1;
    }
    4
}

fn json_validate_literal(value: &str, max_depth: usize) -> bool {
    let mut parser = JsonLiteralParser::new(value, max_depth);
    parser.skip_ws();
    parser.parse_value(1) && {
        parser.skip_ws();
        parser.is_done()
    }
}

struct JsonLiteralParser<'a> {
    bytes: &'a [u8],
    pos: usize,
    max_depth: usize,
}

impl<'a> JsonLiteralParser<'a> {
    fn new(value: &'a str, max_depth: usize) -> Self {
        Self {
            bytes: value.as_bytes(),
            pos: 0,
            max_depth,
        }
    }

    fn is_done(&self) -> bool {
        self.pos == self.bytes.len()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn consume(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn consume_bytes(&mut self, bytes: &[u8]) -> bool {
        if self.bytes[self.pos..].starts_with(bytes) {
            self.pos += bytes.len();
            true
        } else {
            false
        }
    }

    fn parse_value(&mut self, depth: usize) -> bool {
        match self.peek() {
            Some(b'n') => self.consume_bytes(b"null"),
            Some(b't') => self.consume_bytes(b"true"),
            Some(b'f') => self.consume_bytes(b"false"),
            Some(b'"') => self.parse_string(),
            Some(b'[') => self.parse_array(depth),
            Some(b'{') => self.parse_object(depth),
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            _ => false,
        }
    }

    fn parse_string(&mut self) -> bool {
        if !self.consume(b'"') {
            return false;
        }
        while let Some(byte) = self.peek() {
            match byte {
                b'"' => {
                    self.pos += 1;
                    return true;
                }
                b'\\' => {
                    self.pos += 1;
                    if !self.parse_escape() {
                        return false;
                    }
                }
                0x00..=0x1f => return false,
                _ => self.pos += 1,
            }
        }
        false
    }

    fn parse_escape(&mut self) -> bool {
        match self.peek() {
            Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                self.pos += 1;
                true
            }
            Some(b'u') => {
                self.pos += 1;
                let end = self.pos + 4;
                if end > self.bytes.len() {
                    return false;
                }
                if self.bytes[self.pos..end]
                    .iter()
                    .all(|byte| byte.is_ascii_hexdigit())
                {
                    self.pos = end;
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn parse_array(&mut self, depth: usize) -> bool {
        if depth >= self.max_depth {
            return false;
        }
        if !self.consume(b'[') {
            return false;
        }
        self.skip_ws();
        if self.consume(b']') {
            return true;
        }
        loop {
            self.skip_ws();
            if !self.parse_value(depth + 1) {
                return false;
            }
            self.skip_ws();
            if self.consume(b']') {
                return true;
            }
            if !self.consume(b',') {
                return false;
            }
        }
    }

    fn parse_object(&mut self, depth: usize) -> bool {
        if depth >= self.max_depth {
            return false;
        }
        if !self.consume(b'{') {
            return false;
        }
        self.skip_ws();
        if self.consume(b'}') {
            return true;
        }
        loop {
            self.skip_ws();
            if !self.parse_string() {
                return false;
            }
            self.skip_ws();
            if !self.consume(b':') {
                return false;
            }
            self.skip_ws();
            if !self.parse_value(depth + 1) {
                return false;
            }
            self.skip_ws();
            if self.consume(b'}') {
                return true;
            }
            if !self.consume(b',') {
                return false;
            }
        }
    }

    fn parse_number(&mut self) -> bool {
        if self.consume(b'-') && self.is_done() {
            return false;
        }
        match self.peek() {
            Some(b'0') => {
                self.pos += 1;
                if matches!(self.peek(), Some(b'0'..=b'9')) {
                    return false;
                }
            }
            Some(b'1'..=b'9') => {
                self.pos += 1;
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            }
            _ => return false,
        }
        if self.consume(b'.') {
            let start = self.pos;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
            if self.pos == start {
                return false;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let start = self.pos;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
            if self.pos == start {
                return false;
            }
        }
        true
    }
}
