//! Purpose:
//! Collects wasm32-web function declaration metadata from PHP user functions.
//! Keeps simple signature/default/return-kind scans separate from array return analysis.
//!
//! Called from:
//! - `super` re-exports consumed by `crate::codegen::wasm::module::WasmModule::new`.
//!
//! Key details:
//! - These collectors only read declaration headers and default expressions.
//! - Deeper body-derived return and array metadata stays in the parent module.

use super::*;

pub(in crate::codegen::wasm::module) fn collect_function_defaults(
    program: &Program,
) -> HashMap<String, Vec<Option<Expr>>> {
    program
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl { name, params, .. } => Some((
                function_key(name),
                params
                    .iter()
                    .map(|(_, _, default, _)| default.clone())
                    .collect(),
            )),
            _ => None,
        })
        .collect()
}

pub(in crate::codegen::wasm::module) fn collect_function_params(
    program: &Program,
) -> HashMap<String, Vec<String>> {
    program
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl { name, params, .. } => Some((
                function_key(name),
                params
                    .iter()
                    .map(|(param_name, _, _, _)| param_name.clone())
                    .collect(),
            )),
            _ => None,
        })
        .collect()
}

pub(in crate::codegen::wasm::module) fn collect_function_param_kinds(
    program: &Program,
) -> HashMap<String, Vec<LocalKind>> {
    program
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl { name, params, .. } => Some((
                function_key(name),
                params
                    .iter()
                    .map(|(_, ty, _, _)| local_kind_from_type(ty.as_ref()))
                    .collect(),
            )),
            _ => None,
        })
        .collect()
}

pub(in crate::codegen::wasm::module) fn collect_function_return_kinds(
    program: &Program,
) -> HashMap<String, ValueKind> {
    program
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl {
                name, return_type, ..
            } => Some((function_key(name), value_kind_from_return_type(return_type.as_ref()))),
            _ => None,
        })
        .collect()
}

pub(in crate::codegen::wasm::module) fn collect_function_return_object_classes(
    program: &Program,
) -> HashMap<String, String> {
    program
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl {
                name,
                return_type,
                ..
            } => object_class_name_from_return_type(return_type.as_ref())
                .map(|class_name| (function_key(name), class_name)),
            _ => None,
        })
        .collect()
}

pub(in crate::codegen::wasm::module) fn collect_nullable_function_returns(
    program: &Program,
) -> HashSet<String> {
    program
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl {
                name,
                return_type: Some(TypeExpr::Nullable(_)),
                ..
            } => Some(function_key(name)),
            _ => None,
        })
        .collect()
}

fn object_class_name_from_return_type(return_type: Option<&TypeExpr>) -> Option<String> {
    match return_type {
        Some(TypeExpr::Named(class_name)) => Some(class_name.as_str().to_string()),
        Some(TypeExpr::Nullable(inner)) => object_class_name_from_return_type(Some(inner)),
        _ => None,
    }
}
