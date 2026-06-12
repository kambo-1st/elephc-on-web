//! Purpose:
//! Converts browser WAT output into binary `.wasm` without coupling the first backend to a crate.
//! Uses the external WABT `wat2wasm` tool as a narrow, replaceable bridge.
//!
//! Called from:
//! - `crate::codegen::wasm::generate()` for `WasmOutputFormat::Wasm`.
//!
//! Key details:
//! - This file is the only place that shells out; future direct binary emission can replace it.

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::errors::CompileError;
use crate::span::Span;

pub(super) fn wat_to_wasm(wat: &str) -> Result<Vec<u8>, CompileError> {
    let dir = std::env::temp_dir();
    let unique = format!(
        "elephc-wasm-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let wat_path = dir.join(format!("{unique}.wat"));
    let wasm_path = dir.join(format!("{unique}.wasm"));

    fs::write(&wat_path, wat).map_err(|err| {
        CompileError::new(
            Span::dummy(),
            &format!("failed to write temporary WAT for wasm32-web: {}", err),
        )
    })?;

    let output = Command::new("wat2wasm")
        .arg(&wat_path)
        .arg("-o")
        .arg(&wasm_path)
        .output();

    let _ = fs::remove_file(&wat_path);

    match output {
        Ok(output) if output.status.success() => {
            let bytes = fs::read(&wasm_path).map_err(|err| {
                CompileError::new(
                    Span::dummy(),
                    &format!("failed to read generated wasm32-web output: {}", err),
                )
            });
            let _ = fs::remove_file(&wasm_path);
            bytes
        }
        Ok(output) => {
            let _ = fs::remove_file(&wasm_path);
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(CompileError::new(
                Span::dummy(),
                &format!("wat2wasm failed for wasm32-web output: {}", stderr.trim()),
            ))
        }
        Err(err) => {
            let _ = fs::remove_file(&wasm_path);
            Err(CompileError::new(
                Span::dummy(),
                &format!(
                    "wat2wasm is required for binary wasm32-web output: {}. Use --emit-asm to emit WAT instead",
                    err
                ),
            ))
        }
    }
}
