//! Purpose:
//! Evaluates literal string comparison and search builtins for wasm32-web lowering.
//! Keeps static `strcmp`/`strcasecmp`/`strpos`/`strrpos` behavior separate from runtime search loops.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::string_search`
//! - `crate::codegen::wasm::expr::string_int_predicates`
//!
//! Key details:
//! - Preserves PHP offset bounds and `false` optional-result semantics for literal searches.

use super::*;

pub(super) fn eval_literal_string_int_call(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &WasmModule,
) -> Result<Option<i64>, CompileError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(CompileError::new(
            call.span,
            &format!("wasm32-web {}() expects two or three arguments", name),
        ));
    }
    let left = static_ascii_string_arg(call, &args[0], module)?;
    let right = static_ascii_string_arg(call, &args[1], module)?;
    let offset = args.get(2).map(literal_int_arg).transpose()?.unwrap_or(0);
    let value = match name.to_ascii_lowercase().as_str() {
        "strcmp" => Some(ordering_to_php_int(left.as_bytes().cmp(right.as_bytes()))),
        "strcasecmp" => Some(ordering_to_php_int(
            ascii_lower(&left).as_bytes().cmp(ascii_lower(&right).as_bytes()),
        )),
        "strpos" => {
            let offset = eval_literal_strpos_offset(call, &left, offset)?;
            left.get(offset..)
                .and_then(|tail| tail.find(&right).map(|index| (offset + index) as i64))
        }
        "strrpos" => eval_literal_strrpos(call, &left, &right, offset)?,
        _ => unreachable!(),
    };
    Ok(value)
}

fn eval_literal_strpos_offset(call: &Expr, haystack: &str, offset: i64) -> Result<usize, CompileError> {
    let hay_len = haystack.len() as i64;
    if offset > hay_len || offset < -hay_len {
        return Err(CompileError::new(
            call.span,
            "wasm32-web strpos() offset must be contained in the haystack",
        ));
    }
    Ok(if offset < 0 { hay_len + offset } else { offset } as usize)
}

fn eval_literal_strrpos(
    call: &Expr,
    haystack: &str,
    needle: &str,
    offset: i64,
) -> Result<Option<i64>, CompileError> {
    let hay_len = haystack.len() as i64;
    if offset > hay_len || offset < -hay_len {
        return Err(CompileError::new(
            call.span,
            "wasm32-web strrpos() offset must be contained in the haystack",
        ));
    }
    if needle.is_empty() {
        return Ok(Some(if offset < 0 { hay_len + offset } else { hay_len }));
    }
    if offset >= 0 {
        let start = offset as usize;
        return Ok(haystack
            .get(start..)
            .and_then(|tail| tail.rfind(needle).map(|index| (start + index) as i64)));
    }
    let max_start = (hay_len + offset) as usize;
    Ok(haystack
        .get(..)
        .and_then(|whole| whole.match_indices(needle).take_while(|(index, _)| *index <= max_start).last())
        .map(|(index, _)| index as i64))
}

fn ordering_to_php_int(ordering: std::cmp::Ordering) -> i64 {
    match ordering {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}
