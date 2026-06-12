//! Purpose:
//! Evaluates literal/static PHP basename(), dirname(), and pathinfo() cases for wasm32-web.
//! Keeps constant path decomposition separate from runtime path emitter loops.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::scalar_builtins`.
//! - `crate::codegen::wasm::expr::array_assign`.
//!
//! Key details:
//! - Static pathinfo array materialization mirrors PHP path component behavior for known paths.

use super::*;
use super::pathinfo_flags::{pathinfo_scalar_component, PathinfoScalarComponent};

pub(super) fn eval_literal_basename(call: &Expr, args: &[Expr]) -> Result<String, CompileError> {
    if args.is_empty() || args.len() > 2 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web basename() expects one or two literal arguments",
        ));
    }
    let path = literal_string_arg(&args[0])?.trim_end_matches('/');
    let name = path.rsplit('/').next().unwrap_or(path);
    let suffix = args.get(1).map(literal_string_arg).transpose()?;
    if let Some(suffix) = suffix {
        if !suffix.is_empty() && name.ends_with(suffix) {
            return Ok(name[..name.len() - suffix.len()].to_string());
        }
    }
    Ok(name.to_string())
}

pub(super) fn eval_literal_dirname(call: &Expr, args: &[Expr]) -> Result<String, CompileError> {
    if args.is_empty() || args.len() > 2 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web dirname() expects one or two literal arguments",
        ));
    }
    let mut path = literal_string_arg(&args[0])?.trim_end_matches('/').to_string();
    let levels = args.get(1).map(literal_int_arg).transpose()?.unwrap_or(1);
    if levels < 1 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web dirname() literal levels must be at least one",
        ));
    }
    for _ in 0..levels {
        path = dirname_once(&path);
    }
    Ok(path)
}

fn dirname_once(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return "/".to_string();
    }
    match trimmed.rfind('/') {
        Some(0) => "/".to_string(),
        Some(index) => trimmed[..index].to_string(),
        None => ".".to_string(),
    }
}

fn basename_without_suffix(path: &str, suffix: Option<&str>) -> String {
    let path = path.trim_end_matches('/');
    let name = path.rsplit('/').next().unwrap_or(path);
    if let Some(suffix) = suffix {
        if !suffix.is_empty() && name.ends_with(suffix) {
            return name[..name.len() - suffix.len()].to_string();
        }
    }
    name.to_string()
}

pub(super) fn static_pathinfo_assoc_items(
    call: &Expr,
    args: &[Expr],
    module: &WasmModule,
) -> Result<Vec<(Expr, Expr)>, CompileError> {
    if args.is_empty() || args.len() > 2 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web pathinfo() expects one or two arguments",
        ));
    }
    let path = static_pathinfo_path_value(&args[0], module).ok_or_else(|| {
        CompileError::new(
            args[0].span,
            "wasm32-web array-shaped pathinfo() currently requires a static string path",
        )
    })?;
    if let Some(flag_arg) = args.get(1) {
        let Some(flag) = static_or_const_int_value(flag_arg) else {
            return Err(CompileError::new(
                flag_arg.span,
                "wasm32-web array-shaped pathinfo() currently requires PATHINFO_ALL or no flag",
            ));
        };
        if flag != 15 {
            return Err(CompileError::new(
                flag_arg.span,
                "wasm32-web array-shaped pathinfo() currently requires PATHINFO_ALL or no flag",
            ));
        }
    }
    let basename = basename_without_suffix(&path, None);
    let extension = basename
        .rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty())
        .map(|(_, ext)| ext)
        .unwrap_or("")
        .to_string();
    let filename = if extension.is_empty() {
        basename.clone()
    } else {
        basename[..basename.len() - extension.len() - 1].to_string()
    };
    let mut items = vec![
        pathinfo_assoc_item(call, "dirname", dirname_once(&path)),
        pathinfo_assoc_item(call, "basename", basename),
    ];
    if !extension.is_empty() {
        items.push(pathinfo_assoc_item(call, "extension", extension));
    }
    items.push(pathinfo_assoc_item(call, "filename", filename));
    Ok(items)
}

pub(super) fn static_pathinfo_path_value(expr: &Expr, module: &WasmModule) -> Option<String> {
    match &expr.kind {
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Str) => {
            module.string_static_value(name)
        }
        _ => static_string_value(expr, module),
    }
}

fn pathinfo_assoc_item(call: &Expr, key: &str, value: String) -> (Expr, Expr) {
    (
        Expr::new(ExprKind::StringLiteral(key.to_string()), call.span),
        Expr::new(ExprKind::StringLiteral(value), call.span),
    )
}

pub(super) fn eval_literal_pathinfo(call: &Expr, args: &[Expr]) -> Result<String, CompileError> {
    if args.len() != 2 {
        return Err(CompileError::new(
            call.span,
            "wasm32-web pathinfo() currently requires a literal flag argument",
        ));
    }
    let path = literal_string_arg(&args[0])?;
    let flag = static_or_const_int_value(&args[1]).ok_or_else(|| {
        CompileError::new(
            args[1].span,
            "wasm32-web pathinfo() currently requires a literal or constant flag argument",
        )
    })?;
    let basename = basename_without_suffix(path, None);
    let extension = basename
        .rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty())
        .map(|(_, ext)| ext)
        .unwrap_or("");
    let filename = if extension.is_empty() {
        basename.as_str()
    } else {
        &basename[..basename.len() - extension.len() - 1]
    };
    match pathinfo_scalar_component(flag) {
        Some(PathinfoScalarComponent::Dirname) => Ok(dirname_once(path)),
        Some(PathinfoScalarComponent::Basename) => Ok(basename),
        Some(PathinfoScalarComponent::Extension) => Ok(extension.to_string()),
        Some(PathinfoScalarComponent::Filename) => Ok(filename.to_string()),
        Some(PathinfoScalarComponent::Empty) => Ok(String::new()),
        None => Err(CompileError::new(
            call.span,
            "wasm32-web pathinfo() literal support requires a scalar PATHINFO_* flag",
        )),
    }
}
