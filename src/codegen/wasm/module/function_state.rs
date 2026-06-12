//! Purpose:
//! Owns wasm32-web function metadata accessors for parameters and return shapes.
//!
//! Called from:
//! - `crate::codegen::wasm::module::WasmModule` method calls in wasm lowering.
//!
//! Key details:
//! - Accessors normalize function names through the same PHP case-insensitive key.
//! - Array return metadata is exposed without cloning where callers only need slices.

use super::{
    function_key, ArrayLayout, AssocKeyKind, AssocKeyValue, ConstantValue, Expr, LocalKind,
    NestedArrayMetadata, ValueCellKind, ValueKind, WasmModule, static_string_return_call_key_for_expr,
};

impl WasmModule {
    pub(in crate::codegen::wasm) fn has_function(&self, name: &str) -> bool {
        self.function_params.contains_key(&function_key(name))
    }

    pub(in crate::codegen::wasm) fn function_param_defaults(&self, name: &str) -> Option<Vec<Option<Expr>>> {
        self.function_defaults.get(&function_key(name)).cloned()
    }

    pub(in crate::codegen::wasm) fn function_param_names(&self, name: &str) -> Option<Vec<String>> {
        self.function_params.get(&function_key(name)).cloned()
    }

    pub(in crate::codegen::wasm) fn function_param_kinds(&self, name: &str) -> Option<Vec<LocalKind>> {
        self.function_param_kinds.get(&function_key(name)).cloned()
    }

    pub(in crate::codegen::wasm) fn function_return_kind(&self, name: &str) -> Option<ValueKind> {
        self.function_return_kinds.get(&function_key(name)).copied()
    }

    pub(in crate::codegen::wasm) fn function_return_object_class(&self, name: &str) -> Option<String> {
        self.function_return_object_classes
            .get(&function_key(name))
            .cloned()
    }

    pub(in crate::codegen::wasm) fn function_return_is_nullable(&self, name: &str) -> bool {
        self.nullable_function_returns.contains(&function_key(name))
    }

    pub(in crate::codegen::wasm) fn function_static_string_return(&self, name: &str) -> Option<String> {
        self.function_static_string_returns
            .get(&function_key(name))
            .cloned()
    }

    pub(in crate::codegen::wasm) fn function_static_string_return_for_call(
        &self,
        name: &crate::names::Name,
        args: &[Expr],
    ) -> Option<String> {
        let key = static_string_return_call_key_for_expr(name, args, &self.constants)?;
        self.function_static_string_returns.get(&key).cloned()
    }

    pub(in crate::codegen::wasm) fn function_possible_static_string_returns(&self, name: &str) -> Option<&[String]> {
        self.function_possible_static_string_returns
            .get(&function_key(name))
            .map(Vec::as_slice)
    }

    pub(in crate::codegen::wasm) fn function_mixed_return_kind(&self, name: &str) -> Option<ValueCellKind> {
        self.function_mixed_return_kinds.get(&function_key(name)).copied()
    }

    pub(in crate::codegen::wasm) fn function_array_return_length(&self, name: &str) -> Option<usize> {
        self.function_array_return_lengths.get(&function_key(name)).copied()
    }

    pub(in crate::codegen::wasm) fn function_array_return_layout(&self, name: &str) -> ArrayLayout {
        self.function_array_return_layouts
            .get(&function_key(name))
            .copied()
            .unwrap_or(ArrayLayout::CompactInt)
    }

    pub(in crate::codegen::wasm) fn function_array_return_value_kinds(&self, name: &str) -> Option<&[ValueCellKind]> {
        self.function_array_return_value_kinds
            .get(&function_key(name))
            .map(Vec::as_slice)
    }

    pub(in crate::codegen::wasm) fn function_array_return_value_constants(&self, name: &str) -> Option<&[ConstantValue]> {
        self.function_array_return_value_constants
            .get(&function_key(name))
            .map(Vec::as_slice)
    }

    pub(in crate::codegen::wasm) fn function_array_return_runtime_value_kind(&self, name: &str) -> Option<ValueCellKind> {
        self.function_array_return_runtime_value_kinds
            .get(&function_key(name))
            .copied()
    }

    pub(in crate::codegen::wasm) fn function_array_return_nested_values(
        &self,
        name: &str,
    ) -> Option<&[Option<NestedArrayMetadata>]> {
        self.function_array_return_nested_values
            .get(&function_key(name))
            .map(Vec::as_slice)
    }

    pub(in crate::codegen::wasm) fn function_array_return_key_kinds(&self, name: &str) -> Option<&[AssocKeyKind]> {
        self.function_array_return_key_kinds
            .get(&function_key(name))
            .map(Vec::as_slice)
    }

    pub(in crate::codegen::wasm) fn function_array_return_key_values(&self, name: &str) -> Option<&[AssocKeyValue]> {
        self.function_array_return_key_values
            .get(&function_key(name))
            .map(Vec::as_slice)
    }

    pub(in crate::codegen::wasm) fn function_array_return_param_index(&self, name: &str) -> Option<usize> {
        self.function_array_return_param_indices
            .get(&function_key(name))
            .copied()
    }

    pub(in crate::codegen::wasm) fn current_array_return_layout(&self) -> ArrayLayout {
        self.current
            .name
            .as_ref()
            .map(|name| self.function_array_return_layout(name))
            .unwrap_or(ArrayLayout::CompactInt)
    }

    pub(in crate::codegen::wasm) fn current_return_kind(&self) -> ValueKind {
        self.current.return_kind
    }
}
