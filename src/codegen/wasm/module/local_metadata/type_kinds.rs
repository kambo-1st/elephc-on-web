//! Purpose:
//! Maps frontend type annotations onto wasm32-web local, return, and value-cell kinds.
//! Keeps type-shape classification separate from local collection and foreach inference.
//!
//! Called from:
//! - `crate::codegen::wasm::module::local_metadata`
//! - `crate::codegen::wasm::module::function_metadata`
//!
//! Key details:
//! - Unknown PHP types conservatively fall back to integer locals/returns unless array or mixed is explicit.

use super::*;

pub(in crate::codegen::wasm::module) fn local_kind_from_type(ty: Option<&TypeExpr>) -> LocalKind {
    match ty {
        Some(TypeExpr::Nullable(inner)) => local_kind_from_type(Some(inner)),
        Some(TypeExpr::Float) => LocalKind::F64,
        Some(TypeExpr::Bool) => LocalKind::I32,
        Some(TypeExpr::Str) => LocalKind::Str,
        Some(TypeExpr::Named(name)) if name.is_unqualified() && name.as_str().eq_ignore_ascii_case("callable") => {
            LocalKind::Callable
        }
        Some(TypeExpr::Named(name)) if name.is_unqualified() && name.as_str().eq_ignore_ascii_case("mixed") => {
            LocalKind::Mixed
        }
        Some(ty) if is_array_type(ty) => {
            LocalKind::Array
        }
        Some(TypeExpr::Named(_)) => LocalKind::Object,
        _ => LocalKind::I64,
    }
}

pub(in crate::codegen::wasm::module) fn value_kind_from_return_type(ty: Option<&TypeExpr>) -> ValueKind {
    match ty {
        Some(TypeExpr::Nullable(inner)) => value_kind_from_return_type(Some(inner)),
        Some(TypeExpr::Never) => ValueKind::Never,
        Some(TypeExpr::Float) => ValueKind::Float,
        Some(TypeExpr::Bool) => ValueKind::Bool,
        Some(TypeExpr::Str) => ValueKind::Str,
        Some(TypeExpr::Named(name)) if name.is_unqualified() && name.as_str().eq_ignore_ascii_case("mixed") => {
            ValueKind::Mixed
        }
        Some(ty) if is_array_type(ty) => ValueKind::Array,
        Some(TypeExpr::Named(_)) => ValueKind::Object,
        _ => ValueKind::Int,
    }
}

pub(in crate::codegen::wasm::module) fn value_cell_kind_from_type(ty: Option<&TypeExpr>) -> Option<ValueCellKind> {
    match ty {
        Some(TypeExpr::Nullable(inner)) => value_cell_kind_from_type(Some(inner)),
        Some(TypeExpr::Int) => Some(ValueCellKind::Int),
        Some(TypeExpr::Float) => Some(ValueCellKind::Float),
        Some(TypeExpr::Bool) => Some(ValueCellKind::Bool),
        Some(TypeExpr::Str) => Some(ValueCellKind::Str),
        Some(ty) if is_array_type(ty) => Some(ValueCellKind::Array),
        _ => None,
    }
}

pub(in crate::codegen::wasm::module) fn local_kind_for_value_cell(kind: ValueCellKind) -> LocalKind {
    match kind {
        ValueCellKind::Int => LocalKind::I64,
        ValueCellKind::Null => LocalKind::Mixed,
        ValueCellKind::Float => LocalKind::F64,
        ValueCellKind::Bool => LocalKind::I32,
        ValueCellKind::Str => LocalKind::Str,
        ValueCellKind::Array => LocalKind::Array,
    }
}

pub(in crate::codegen::wasm::module) fn local_kind_for_value(kind: ValueKind) -> LocalKind {
    match kind {
        ValueKind::Int | ValueKind::Null => LocalKind::I64,
        ValueKind::Float => LocalKind::F64,
        ValueKind::Bool => LocalKind::I32,
        ValueKind::Str => LocalKind::Str,
        ValueKind::Array => LocalKind::Array,
        ValueKind::Object => LocalKind::Object,
        ValueKind::Mixed => LocalKind::Mixed,
        ValueKind::Never => LocalKind::I64,
    }
}

pub(in crate::codegen::wasm::module) fn local_kind_for_constant(value: &ConstantValue) -> LocalKind {
    match value {
        ConstantValue::Int(_) | ConstantValue::Null => LocalKind::I64,
        ConstantValue::Float(_) => LocalKind::F64,
        ConstantValue::Bool(_) => LocalKind::I32,
        ConstantValue::Str(_) => LocalKind::Str,
    }
}

pub(in crate::codegen::wasm) fn value_kind_for_local(kind: LocalKind) -> ValueKind {
    match kind {
        LocalKind::I64 => ValueKind::Int,
        LocalKind::F64 => ValueKind::Float,
        LocalKind::I32 => ValueKind::Bool,
        LocalKind::Str => ValueKind::Str,
        LocalKind::Array => ValueKind::Array,
        LocalKind::Object => ValueKind::Object,
        LocalKind::Mixed => ValueKind::Mixed,
        LocalKind::Callable => ValueKind::Mixed,
    }
}

fn is_array_type(ty: &TypeExpr) -> bool {
    matches!(ty, TypeExpr::Iterable)
        || matches!(ty, TypeExpr::Named(name) if name.is_unqualified() && name.as_str().eq_ignore_ascii_case("array"))
}
