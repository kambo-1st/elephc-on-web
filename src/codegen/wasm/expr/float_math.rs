//! Purpose:
//! Lowers wasm32-web floating-point math builtins and literal math folding.
//! Keeps host math calls and literal numeric parsing outside the main expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` scalar builtin dispatch.
//! - sibling wasm formatting modules that need literal numeric coercion.
//!
//! Key details:
//! - Runtime math uses imported host helpers where WebAssembly has no direct opcode.

use super::*;

pub(super) fn is_literal_float_math_builtin(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "sinh"
            | "cosh"
            | "tanh"
            | "log"
            | "log10"
            | "exp"
            | "deg2rad"
            | "rad2deg"
            | "pow"
            | "fmod"
            | "atan2"
            | "hypot"
            | "round"
    )
}

pub(super) fn emit_literal_float_math_call(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let result = eval_literal_float_math_call(call, name, args)?;
    module.body().line(&format!("f64.const {}", wasm_float_literal(result)));
    Ok(ValueKind::Float)
}

fn eval_literal_float_math_call(
    call: &Expr,
    name: &str,
    args: &[Expr],
) -> Result<f64, CompileError> {
    let lower = name.to_ascii_lowercase();
    let values = args
        .iter()
        .map(literal_numeric_arg)
        .collect::<Result<Vec<_>, _>>()?;
    let require_len = |expected: usize| {
        if values.len() == expected {
            Ok(())
        } else {
            Err(CompileError::new(
                call.span,
                &format!("wasm32-web {}() expects literal numeric arguments", name),
            ))
        }
    };
    let result = match lower.as_str() {
        "sin" => {
            require_len(1)?;
            values[0].sin()
        }
        "cos" => {
            require_len(1)?;
            values[0].cos()
        }
        "tan" => {
            require_len(1)?;
            values[0].tan()
        }
        "asin" => {
            require_len(1)?;
            values[0].asin()
        }
        "acos" => {
            require_len(1)?;
            values[0].acos()
        }
        "atan" => {
            require_len(1)?;
            values[0].atan()
        }
        "sinh" => {
            require_len(1)?;
            values[0].sinh()
        }
        "cosh" => {
            require_len(1)?;
            values[0].cosh()
        }
        "tanh" => {
            require_len(1)?;
            values[0].tanh()
        }
        "log" if values.len() == 1 => values[0].ln(),
        "log" if values.len() == 2 => values[0].log(values[1]),
        "log10" => {
            require_len(1)?;
            values[0].log10()
        }
        "exp" => {
            require_len(1)?;
            values[0].exp()
        }
        "deg2rad" => {
            require_len(1)?;
            values[0].to_radians()
        }
        "rad2deg" => {
            require_len(1)?;
            values[0].to_degrees()
        }
        "pow" => {
            require_len(2)?;
            values[0].powf(values[1])
        }
        "fmod" => {
            require_len(2)?;
            values[0] % values[1]
        }
        "atan2" => {
            require_len(2)?;
            values[0].atan2(values[1])
        }
        "hypot" => {
            require_len(2)?;
            values[0].hypot(values[1])
        }
        "round" if values.len() == 1 => values[0].round(),
        "round" if values.len() == 2 => {
            let scale = 10_f64.powf(values[1]);
            (values[0] * scale).round() / scale
        }
        _ => {
            return Err(CompileError::new(
                call.span,
                &format!("wasm32-web {}() expects literal numeric arguments", name),
            ));
        }
    };
    Ok(result)
}

pub(super) fn literal_numeric_arg(expr: &Expr) -> Result<f64, CompileError> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Ok(*value as f64),
        ExprKind::FloatLiteral(value) => Ok(*value),
        ExprKind::Negate(inner) => literal_numeric_arg(inner).map(|value| -value),
        _ => Err(CompileError::new(
            expr.span,
            "wasm32-web literal math builtin currently requires literal numeric arguments",
        )),
    }
}

pub(super) fn wasm_float_literal(value: f64) -> String {
    if value.is_nan() {
        "nan".to_string()
    } else if value == f64::INFINITY {
        "inf".to_string()
    } else if value == f64::NEG_INFINITY {
        "-inf".to_string()
    } else {
        value.to_string()
    }
}

pub(super) fn emit_float_math_call(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    match name.to_ascii_lowercase().as_str() {
        "pi" => {
            if !args.is_empty() {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web pi() expects no arguments",
                ));
            }
            module.body().line("f64.const 3.141592653589793");
        }
        "floor" | "ceil" | "sqrt" => {
            let [arg] = args else {
                return Err(CompileError::new(
                    call.span,
                    &format!("wasm32-web {}() expects exactly one argument", name),
                ));
            };
            require_float(arg, module)?;
            let instr = match name.to_ascii_lowercase().as_str() {
                "floor" => "f64.floor",
                "ceil" => "f64.ceil",
                "sqrt" => "f64.sqrt",
                _ => unreachable!(),
            };
            module.body().line(instr);
        }
        "pow" => {
            let [base, exponent] = args else {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web pow() expects exactly two arguments",
                ));
            };
            require_float(base, module)?;
            require_float(exponent, module)?;
            module.body().line("call $host_pow");
        }
        "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh" | "tanh"
        | "log10" | "exp" => {
            let [arg] = args else {
                return Err(CompileError::new(
                    call.span,
                    &format!("wasm32-web {}() expects exactly one argument", name),
                ));
            };
            require_float(arg, module)?;
            module
                .body()
                .line(&format!("call $host_{}", name.to_ascii_lowercase()));
        }
        "log" => match args {
            [arg] => {
                require_float(arg, module)?;
                module.body().line("call $host_log");
            }
            [arg, base] => {
                require_float(arg, module)?;
                module.body().line("call $host_log");
                require_float(base, module)?;
                module.body().line("call $host_log");
                module.body().line("f64.div");
            }
            _ => {
                return Err(CompileError::new(
                    call.span,
                    "wasm32-web log() expects one or two arguments",
                ));
            }
        },
        "deg2rad" | "rad2deg" => {
            let [arg] = args else {
                return Err(CompileError::new(
                    call.span,
                    &format!("wasm32-web {}() expects exactly one argument", name),
                ));
            };
            require_float(arg, module)?;
            if name.eq_ignore_ascii_case("deg2rad") {
                module.body().line("f64.const 0.017453292519943295");
                module.body().line("f64.mul");
            } else {
                module.body().line("f64.const 57.29577951308232");
                module.body().line("f64.mul");
            }
        }
        "fmod" | "atan2" | "hypot" => {
            let [left, right] = args else {
                return Err(CompileError::new(
                    call.span,
                    &format!("wasm32-web {}() expects exactly two arguments", name),
                ));
            };
            require_float(left, module)?;
            require_float(right, module)?;
            module
                .body()
                .line(&format!("call $host_{}", name.to_ascii_lowercase()));
        }
        _ => unreachable!(),
    }
    Ok(ValueKind::Float)
}
