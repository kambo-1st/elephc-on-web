//! Purpose:
//! Coordinates browser WebAssembly text generation for supported PHP programs.
//! Builds backend-local module state, lowers statements, and returns complete WAT.
//!
//! Called from:
//! - `crate::codegen::wasm::generate()` for `WasmOutputFormat::Wat`.
//!
//! Key details:
//! - WAT output imports `elephc.write(ptr, len)`, `elephc.writeInt(value)`, and host helpers.

use crate::errors::CompileError;
use crate::parser::ast::Program;

use super::module::WasmModule;

pub fn generate(program: &Program) -> Result<String, CompileError> {
    let mut module = WasmModule::new(program);
    module.validate_static_metadata()?;
    for stmt in program {
        if let crate::parser::ast::StmtKind::FunctionDecl {
            name, params, body, ..
        } = &stmt.kind
        {
            let params = params
                .iter()
                .map(|(name, ty, _, _)| (name.clone(), ty.clone()))
                .collect();
            let previous = module.enter_function(name.clone(), params, body)?;
            super::stmt::emit_function_body(body, &mut module)?;
            module.leave_function(previous);
        }
    }
    for (class_name, method) in module.object_methods() {
        let previous = module.enter_object_method(class_name, &method)?;
        super::stmt::emit_function_body(&method.body, &mut module)?;
        module.leave_function(previous);
    }
    super::stmt::emit_program(program, &mut module)?;
    Ok(module.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wat_for(source: &str) -> String {
        let tokens = crate::lexer::tokenize(source).expect("tokenize failed");
        let program = crate::parser::parse(&tokens).expect("parse failed");
        generate(&program).expect("WAT generation failed")
    }

    #[test]
    fn emits_browser_host_imports() {
        let wat = wat_for("<?php echo \"Hello, web!\\n\";");
        assert!(wat.contains("(import \"elephc\" \"write\""));
        assert!(wat.contains("(import \"elephc\" \"writeInt\""));
        assert!(wat.contains("(import \"elephc\" \"jsonValidate\""));
        assert!(wat.contains("(import \"elephc\" \"randomU32\""));
        assert!(wat.contains("(memory (export \"memory\") 1)"));
        assert!(wat.contains("(global $json_last_error (mut i32)"));
        assert!(wat.contains("Hello, web!\\0a"));
    }

    #[test]
    fn emits_runtime_integer_assignment_and_output() {
        let wat = wat_for("<?php $x = 40 + 2; echo \"value=\" . $x;");
        assert!(wat.contains("(local $x i64)"));
        assert!(wat.contains("i64.add"));
        assert!(wat.contains("local.set $x"));
        assert!(wat.contains("call $host_write_int"));
    }

    #[test]
    fn emits_runtime_while_loop() {
        let wat = wat_for("<?php $i = 0; while ($i < 3) { echo $i; $i = $i + 1; }");
        assert!(wat.contains("loop"));
        assert!(wat.contains("br_if $break_"));
        assert!(wat.contains("br $continue_"));
    }

    #[test]
    fn emits_scalar_user_function_and_call() {
        let wat = wat_for("<?php function add(int $a, int $b): int { return $a + $b; } echo add(2, 3);");
        assert!(wat.contains("(func $fn_add (param $a i64) (param $b i64) (result i64)"));
        assert!(wat.contains("local.get $a"));
        assert!(wat.contains("local.get $b"));
        assert!(wat.contains("call $fn_add"));
        assert!(wat.contains("call $host_write_int"));
    }

    #[test]
    fn emits_strlen_for_string_variable() {
        let wat = wat_for("<?php $name = \"Ada\"; echo strlen($name);");
        assert!(wat.contains("(local $name_ptr i32)"));
        assert!(wat.contains("(local $name_len i32)"));
        assert!(wat.contains("local.get $name_len"));
        assert!(wat.contains("i64.extend_i32_u"));
    }
}
