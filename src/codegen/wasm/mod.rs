//! Purpose:
//! Owns the experimental WebAssembly backend entry point and output format selection.
//! Keeps browser-target emission isolated from native assembly codegen.
//!
//! Called from:
//! - `crate::pipeline::compile()` when `--target wasm32-web` is selected.
//!
//! Key details:
//! - WAT is implemented first; binary `.wasm` is reserved behind the same format enum.

mod binary;
mod emitter;
mod expr;
mod module;
mod runtime;
mod stmt;
mod wat;

use crate::errors::CompileError;
use crate::parser::ast::Program;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmOutputFormat {
    Wat,
    Wasm,
}

pub fn generate(program: &Program, format: WasmOutputFormat) -> Result<Vec<u8>, CompileError> {
    match format {
        WasmOutputFormat::Wat => wat::generate(program).map(|wat| wat.into_bytes()),
        WasmOutputFormat::Wasm => {
            let wat = wat::generate(program)?;
            binary::wat_to_wasm(&wat)
        }
    }
}
