//! Purpose:
//! Provides a small WAT text builder for the browser WebAssembly backend.
//! Owns indentation, raw line emission, and byte escaping for data segments.
//!
//! Called from:
//! - `crate::codegen::wasm::wat` and focused WASM lowering modules.
//!
//! Key details:
//! - This is backend-local text emission; native assembly `Emitter` remains untouched.

pub(super) struct WatEmitter {
    buf: String,
    indent: usize,
}

impl WatEmitter {
    pub(super) fn new() -> Self {
        Self {
            buf: String::with_capacity(4096),
            indent: 0,
        }
    }

    pub(super) fn line(&mut self, text: &str) {
        for _ in 0..self.indent {
            self.buf.push_str("  ");
        }
        self.buf.push_str(text);
        self.buf.push('\n');
    }

    pub(super) fn open(&mut self, text: &str) {
        self.line(text);
        self.indent += 1;
    }

    pub(super) fn close(&mut self, text: &str) {
        self.indent = self.indent.saturating_sub(1);
        self.line(text);
    }

    pub(super) fn finish(self) -> String {
        self.buf
    }
}

pub(super) fn escape_wat_bytes(bytes: &[u8]) -> String {
    let mut escaped = String::new();
    for byte in bytes {
        match byte {
            b'"' => escaped.push_str("\\22"),
            b'\\' => escaped.push_str("\\5c"),
            0x20..=0x7e => escaped.push(*byte as char),
            _ => escaped.push_str(&format!("\\{:02x}", byte)),
        }
    }
    escaped
}
