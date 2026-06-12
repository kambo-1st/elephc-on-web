//! Purpose:
//! Owns wasm32-web per-local array metadata accessors for layout, value cells,
//! nested array shapes, and associative key state.
//!
//! Called from:
//! - `crate::codegen::wasm::module::WasmModule` method calls in wasm lowering.
//!
//! Key details:
//! - Keeps static metadata and runtime metadata mutually exclusive where required.
//! - Clearing layout/key/value metadata must preserve the same invalidation behavior as before.

use super::{
    ArrayLayout, AssocKeyKind, AssocKeyValue, ConstantValue, NestedArrayMetadata, ValueCellKind,
    WasmModule,
};

impl WasmModule {
    pub(in crate::codegen::wasm) fn array_length(&self, name: &str) -> Option<usize> {
        self.current.array_lengths.get(name).copied()
    }

    pub(in crate::codegen::wasm) fn array_layout(&self, name: &str) -> ArrayLayout {
        self.current
            .array_layouts
            .get(name)
            .copied()
            .unwrap_or(ArrayLayout::CompactInt)
    }

    pub(in crate::codegen::wasm) fn set_array_layout(&mut self, name: &str, layout: ArrayLayout) {
        self.current.array_layouts.insert(name.to_string(), layout);
        if layout != ArrayLayout::Assoc {
            self.current.array_key_kinds.remove(name);
            self.current.array_key_values.remove(name);
            self.current.array_runtime_key_kind.remove(name);
            self.current.array_php_normalized_runtime_keys.remove(name);
        }
        if layout != ArrayLayout::Value {
            self.current.array_runtime_nested_value.remove(name);
            self.current.array_object_classes.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn set_array_length(&mut self, name: &str, len: usize) {
        self.current.array_lengths.insert(name.to_string(), len);
    }

    pub(in crate::codegen::wasm) fn clear_array_length(&mut self, name: &str) {
        self.current.array_lengths.remove(name);
    }

    pub(in crate::codegen::wasm) fn array_value_cell_kind(&self, name: &str, index: usize) -> Option<ValueCellKind> {
        self.current
            .array_value_kinds
            .get(name)
            .and_then(|items| items.get(index).copied())
    }

    pub(in crate::codegen::wasm) fn array_value_cell_kinds(&self, name: &str) -> Option<&[ValueCellKind]> {
        self.current.array_value_kinds.get(name).map(Vec::as_slice)
    }

    pub(in crate::codegen::wasm) fn set_array_value_cell_kinds(&mut self, name: &str, kinds: Option<Vec<ValueCellKind>>) {
        self.current.array_value_constants.remove(name);
        if let Some(kinds) = kinds {
            self.current.array_runtime_value_kind.remove(name);
            self.current.array_runtime_nested_value.remove(name);
            self.current.array_value_kinds.insert(name.to_string(), kinds);
        } else {
            self.current.array_value_kinds.remove(name);
            self.current.array_nested_values.remove(name);
            self.current.array_object_classes.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn array_object_class(
        &self,
        name: &str,
        index: usize,
    ) -> Option<String> {
        self.current
            .array_object_classes
            .get(name)
            .and_then(|items| items.get(index))
            .cloned()
            .flatten()
    }

    pub(in crate::codegen::wasm) fn array_object_classes(
        &self,
        name: &str,
    ) -> Option<&[Option<String>]> {
        self.current.array_object_classes.get(name).map(Vec::as_slice)
    }

    pub(in crate::codegen::wasm) fn set_array_object_classes(
        &mut self,
        name: &str,
        classes: Option<Vec<Option<String>>>,
    ) {
        if let Some(classes) = classes {
            self.current.array_object_classes.insert(name.to_string(), classes);
        } else {
            self.current.array_object_classes.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn array_value_constants(&self, name: &str) -> Option<&[ConstantValue]> {
        self.current.array_value_constants.get(name).map(Vec::as_slice)
    }

    pub(in crate::codegen::wasm) fn set_array_value_constants(
        &mut self,
        name: &str,
        values: Option<Vec<ConstantValue>>,
    ) {
        if let Some(values) = values {
            self.current.array_value_constants.insert(name.to_string(), values);
        } else {
            self.current.array_value_constants.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn array_runtime_value_cell_kind(&self, name: &str) -> Option<ValueCellKind> {
        self.current.array_runtime_value_kind.get(name).copied()
    }

    pub(in crate::codegen::wasm) fn set_array_runtime_value_cell_kind(&mut self, name: &str, kind: Option<ValueCellKind>) {
        self.current.array_value_constants.remove(name);
        if let Some(kind) = kind {
            self.current.array_runtime_value_kind.insert(name.to_string(), kind);
        } else {
            self.current.array_runtime_value_kind.remove(name);
            self.current.array_runtime_nested_value.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn array_nested_value_metadata(
        &self,
        name: &str,
        index: usize,
    ) -> Option<NestedArrayMetadata> {
        self.current
            .array_nested_values
            .get(name)
            .and_then(|items| items.get(index))
            .cloned()
            .flatten()
    }

    pub(in crate::codegen::wasm) fn array_runtime_nested_value_metadata(&self, name: &str) -> Option<NestedArrayMetadata> {
        self.current.array_runtime_nested_value.get(name).cloned()
    }

    pub(in crate::codegen::wasm) fn array_nested_value_metadata_items(
        &self,
        name: &str,
    ) -> Option<&[Option<NestedArrayMetadata>]> {
        self.current.array_nested_values.get(name).map(Vec::as_slice)
    }

    pub(in crate::codegen::wasm) fn set_array_nested_value_metadata(
        &mut self,
        name: &str,
        metadata: Option<Vec<Option<NestedArrayMetadata>>>,
    ) {
        if let Some(metadata) = metadata {
            self.current.array_runtime_nested_value.remove(name);
            self.current.array_nested_values.insert(name.to_string(), metadata);
        } else {
            self.current.array_nested_values.remove(name);
            self.current.array_runtime_nested_value.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn set_array_runtime_nested_value_metadata(
        &mut self,
        name: &str,
        metadata: Option<NestedArrayMetadata>,
    ) {
        if let Some(metadata) = metadata {
            self.current.array_nested_values.remove(name);
            self.current.array_runtime_nested_value.insert(name.to_string(), metadata);
        } else {
            self.current.array_runtime_nested_value.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn set_array_nested_value_metadata_at(
        &mut self,
        name: &str,
        index: usize,
        metadata: Option<NestedArrayMetadata>,
    ) {
        if let Some(items) = self.current.array_nested_values.get_mut(name) {
            if let Some(slot) = items.get_mut(index) {
                *slot = metadata;
            } else {
                self.current.array_nested_values.remove(name);
            }
            return;
        }
        let Some(metadata) = metadata else {
            return;
        };
        let Some(len) = self.current.array_lengths.get(name).copied() else {
            return;
        };
        if index >= len {
            return;
        }
        let mut items = vec![None; len];
        items[index] = Some(metadata);
        self.current.array_nested_values.insert(name.to_string(), items);
    }

    pub(in crate::codegen::wasm) fn array_key_kinds(&self, name: &str) -> Option<&[AssocKeyKind]> {
        self.current.array_key_kinds.get(name).map(Vec::as_slice)
    }

    pub(in crate::codegen::wasm) fn set_array_key_kinds(&mut self, name: &str, kinds: Option<Vec<AssocKeyKind>>) {
        self.current.array_key_values.remove(name);
        self.current.array_runtime_key_kind.remove(name);
        self.current.array_php_normalized_runtime_keys.remove(name);
        if let Some(kinds) = kinds {
            self.current.array_key_kinds.insert(name.to_string(), kinds);
        } else {
            self.current.array_key_kinds.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn array_runtime_key_kind(&self, name: &str) -> Option<AssocKeyKind> {
        self.current.array_runtime_key_kind.get(name).copied()
    }

    pub(in crate::codegen::wasm) fn set_array_runtime_key_kind(&mut self, name: &str, kind: Option<AssocKeyKind>) {
        self.current.array_php_normalized_runtime_keys.remove(name);
        if let Some(kind) = kind {
            self.current.array_key_kinds.remove(name);
            self.current.array_key_values.remove(name);
            self.current.array_runtime_key_kind.insert(name.to_string(), kind);
        } else {
            self.current.array_runtime_key_kind.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn array_key_values(&self, name: &str) -> Option<&[AssocKeyValue]> {
        self.current.array_key_values.get(name).map(Vec::as_slice)
    }

    pub(in crate::codegen::wasm) fn set_array_key_values(&mut self, name: &str, values: Option<Vec<AssocKeyValue>>) {
        self.current.array_runtime_key_kind.remove(name);
        self.current.array_php_normalized_runtime_keys.remove(name);
        if let Some(values) = values {
            self.current.array_key_values.insert(name.to_string(), values);
        } else {
            self.current.array_key_values.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn array_has_php_normalized_runtime_keys(&self, name: &str) -> bool {
        self.current.array_php_normalized_runtime_keys.contains(name)
    }

    pub(in crate::codegen::wasm) fn set_array_php_normalized_runtime_keys(&mut self, name: &str, enabled: bool) {
        if enabled {
            self.current
                .array_php_normalized_runtime_keys
                .insert(name.to_string());
        } else {
            self.current.array_php_normalized_runtime_keys.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn set_array_value_cell_kind(&mut self, name: &str, index: usize, kind: Option<ValueCellKind>) {
        self.current.array_runtime_value_kind.remove(name);
        let Some(kind) = kind else {
            self.current.array_value_kinds.remove(name);
            self.current.array_object_classes.remove(name);
            return;
        };
        let Some(kinds) = self.current.array_value_kinds.get_mut(name) else {
            return;
        };
        if let Some(slot) = kinds.get_mut(index) {
            *slot = kind;
        } else {
            self.current.array_value_kinds.remove(name);
            self.current.array_object_classes.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn push_array_value_cell_kind(&mut self, name: &str, kind: Option<ValueCellKind>) {
        self.current.array_runtime_value_kind.remove(name);
        let Some(kind) = kind else {
            self.current.array_value_kinds.remove(name);
            self.current.array_object_classes.remove(name);
            return;
        };
        if let Some(kinds) = self.current.array_value_kinds.get_mut(name) {
            kinds.push(kind);
        }
    }
}
