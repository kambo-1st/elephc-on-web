//! Purpose:
//! Wraps wasm32-web array_map lowering for nine-source mixed array assignments.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_map_multi_emitters`.
//!
//! Key details:
//! - Keeps nine-source mixed callback support isolated from native codegen.
//! - Delegates the shared boxed-cell loop to the high-arity mixed emitter.

use super::*;

pub(in crate::codegen::wasm::expr) fn emit_array_map_nine_mixed_locals_assign(
    name: &str,
    first: &str,
    second: &str,
    third: &str,
    fourth: &str,
    fifth: &str,
    sixth: &str,
    seventh: &str,
    eighth: &str,
    ninth: &str,
    source_span: crate::span::Span,
    callback: &str,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    emit_array_map_many_mixed_locals_assign(
        name,
        &[
            first, second, third, fourth, fifth, sixth, seventh, eighth, ninth,
        ],
        "nine",
        source_span,
        callback,
        module,
    )
}
