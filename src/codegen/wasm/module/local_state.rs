//! Purpose:
//! Owns wasm32-web per-function local state accessors for scalar constants,
//! callable targets, mixed value kinds, and local declarations.
//!
//! Called from:
//! - `crate::codegen::wasm::module::WasmModule` method calls in wasm lowering.
//!
//! Key details:
//! - Methods keep current function-local bookkeeping centralized.
//! - Visibility remains wasm-wide because sibling emitters call these accessors directly.

use super::{LocalKind, ValueCellKind, WasmModule};
use super::super::emitter::WatEmitter;

impl WasmModule {
    pub(in crate::codegen::wasm) fn body(&mut self) -> &mut WatEmitter {
        &mut self.current.body
    }

    pub(in crate::codegen::wasm) fn local_kind(&self, name: &str) -> Option<LocalKind> {
        self.current.locals.get(name).copied()
    }

    pub(in crate::codegen::wasm) fn is_current_param(&self, name: &str) -> bool {
        self.current.params.iter().any(|param| param == name)
    }

    pub(in crate::codegen::wasm) fn i64_static_value(&self, name: &str) -> Option<i64> {
        self.current.i64_static_values.get(name).copied()
    }

    pub(in crate::codegen::wasm) fn set_i64_static_value(&mut self, name: &str, value: Option<i64>) {
        if let Some(value) = value {
            self.current.i64_static_values.insert(name.to_string(), value);
        } else {
            self.current.i64_static_values.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn i64_ref_alias(&self, name: &str) -> Option<String> {
        self.current.i64_ref_aliases.get(name).cloned()
    }

    pub(in crate::codegen::wasm) fn set_i64_ref_alias(&mut self, name: &str, address_local: &str) {
        self.current
            .i64_ref_aliases
            .insert(name.to_string(), address_local.to_string());
    }

    pub(in crate::codegen::wasm) fn i64_ref_alias_refreshes(&self, name: &str) -> bool {
        self.current.i64_ref_alias_refreshes.contains(name)
    }

    pub(in crate::codegen::wasm) fn set_i64_ref_alias_refresh(&mut self, name: &str, refresh: bool) {
        if refresh {
            self.current.i64_ref_alias_refreshes.insert(name.to_string());
        } else {
            self.current.i64_ref_alias_refreshes.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn f64_ref_alias(&self, name: &str) -> Option<String> {
        self.current.f64_ref_aliases.get(name).cloned()
    }

    pub(in crate::codegen::wasm) fn set_f64_ref_alias(&mut self, name: &str, address_local: &str) {
        self.current
            .f64_ref_aliases
            .insert(name.to_string(), address_local.to_string());
    }

    pub(in crate::codegen::wasm) fn f64_ref_alias_refreshes(&self, name: &str) -> bool {
        self.current.f64_ref_alias_refreshes.contains(name)
    }

    pub(in crate::codegen::wasm) fn set_f64_ref_alias_refresh(&mut self, name: &str, refresh: bool) {
        if refresh {
            self.current.f64_ref_alias_refreshes.insert(name.to_string());
        } else {
            self.current.f64_ref_alias_refreshes.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn i32_ref_alias(&self, name: &str) -> Option<String> {
        self.current.i32_ref_aliases.get(name).cloned()
    }

    pub(in crate::codegen::wasm) fn set_i32_ref_alias(&mut self, name: &str, address_local: &str) {
        self.current
            .i32_ref_aliases
            .insert(name.to_string(), address_local.to_string());
    }

    pub(in crate::codegen::wasm) fn i32_ref_alias_refreshes(&self, name: &str) -> bool {
        self.current.i32_ref_alias_refreshes.contains(name)
    }

    pub(in crate::codegen::wasm) fn set_i32_ref_alias_refresh(&mut self, name: &str, refresh: bool) {
        if refresh {
            self.current.i32_ref_alias_refreshes.insert(name.to_string());
        } else {
            self.current.i32_ref_alias_refreshes.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn str_ref_alias(&self, name: &str) -> Option<String> {
        self.current.str_ref_aliases.get(name).cloned()
    }

    pub(in crate::codegen::wasm) fn set_str_ref_alias(&mut self, name: &str, address_local: &str) {
        self.current
            .str_ref_aliases
            .insert(name.to_string(), address_local.to_string());
    }

    pub(in crate::codegen::wasm) fn str_ref_alias_refreshes(&self, name: &str) -> bool {
        self.current.str_ref_alias_refreshes.contains(name)
    }

    pub(in crate::codegen::wasm) fn set_str_ref_alias_refresh(&mut self, name: &str, refresh: bool) {
        if refresh {
            self.current.str_ref_alias_refreshes.insert(name.to_string());
        } else {
            self.current.str_ref_alias_refreshes.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn array_ref_alias(&self, name: &str) -> Option<String> {
        self.current.array_ref_aliases.get(name).cloned()
    }

    pub(in crate::codegen::wasm) fn set_array_ref_alias(&mut self, name: &str, address_local: &str) {
        self.current
            .array_ref_aliases
            .insert(name.to_string(), address_local.to_string());
    }

    pub(in crate::codegen::wasm) fn array_ref_alias_source(&self, name: &str) -> Option<String> {
        self.current.array_ref_alias_sources.get(name).cloned()
    }

    pub(in crate::codegen::wasm) fn set_array_ref_alias_source(&mut self, name: &str, source: &str) {
        self.current
            .array_ref_alias_sources
            .insert(name.to_string(), source.to_string());
    }

    pub(in crate::codegen::wasm) fn array_ref_alias_refreshes(&self, name: &str) -> bool {
        self.current.array_ref_alias_refreshes.contains(name)
    }

    pub(in crate::codegen::wasm) fn set_array_ref_alias_refresh(&mut self, name: &str, refresh: bool) {
        if refresh {
            self.current.array_ref_alias_refreshes.insert(name.to_string());
        } else {
            self.current.array_ref_alias_refreshes.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn mixed_ref_alias(&self, name: &str) -> Option<String> {
        self.current.mixed_ref_aliases.get(name).cloned()
    }

    pub(in crate::codegen::wasm) fn set_mixed_ref_alias(&mut self, name: &str, address_local: &str) {
        self.current
            .mixed_ref_aliases
            .insert(name.to_string(), address_local.to_string());
    }

    pub(in crate::codegen::wasm) fn mixed_ref_alias_refreshes(&self, name: &str) -> bool {
        self.current.mixed_ref_alias_refreshes.contains(name)
    }

    pub(in crate::codegen::wasm) fn set_mixed_ref_alias_refresh(&mut self, name: &str, refresh: bool) {
        if refresh {
            self.current.mixed_ref_alias_refreshes.insert(name.to_string());
        } else {
            self.current.mixed_ref_alias_refreshes.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn f64_static_value(&self, name: &str) -> Option<f64> {
        self.current.f64_static_values.get(name).copied()
    }

    pub(in crate::codegen::wasm) fn set_f64_static_value(&mut self, name: &str, value: Option<f64>) {
        if let Some(value) = value {
            self.current.f64_static_values.insert(name.to_string(), value);
        } else {
            self.current.f64_static_values.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn bool_static_value(&self, name: &str) -> Option<bool> {
        self.current.bool_static_values.get(name).copied()
    }

    pub(in crate::codegen::wasm) fn set_bool_static_value(&mut self, name: &str, value: Option<bool>) {
        if let Some(value) = value {
            self.current.bool_static_values.insert(name.to_string(), value);
        } else {
            self.current.bool_static_values.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn string_static_value(&self, name: &str) -> Option<String> {
        self.current.string_static_values.get(name).cloned()
    }

    pub(in crate::codegen::wasm) fn set_string_static_value(&mut self, name: &str, value: Option<String>) {
        if let Some(value) = value {
            self.current.string_static_values.insert(name.to_string(), value);
        } else {
            self.current.string_static_values.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn callable_target(&self, name: &str) -> Option<String> {
        self.current.callable_targets.get(name).cloned()
    }

    pub(in crate::codegen::wasm) fn set_callable_target(&mut self, name: &str, target: Option<String>) {
        if let Some(target) = target {
            self.current.callable_targets.insert(name.to_string(), target);
            self.current.callable_instance_targets.remove(name);
        } else {
            self.current.callable_targets.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn callable_instance_target(&self, name: &str) -> Option<(String, String)> {
        self.current.callable_instance_targets.get(name).cloned()
    }

    pub(in crate::codegen::wasm) fn set_callable_instance_target(
        &mut self,
        name: &str,
        target: Option<(String, String)>,
    ) {
        if let Some(target) = target {
            self.current.callable_instance_targets.insert(name.to_string(), target);
            self.current.callable_targets.remove(name);
        } else {
            self.current.callable_instance_targets.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn mixed_value_cell_kind(&self, name: &str) -> Option<ValueCellKind> {
        self.current.mixed_value_kinds.get(name).copied()
    }

    pub(in crate::codegen::wasm) fn set_mixed_value_cell_kind(&mut self, name: &str, kind: Option<ValueCellKind>) {
        if let Some(kind) = kind {
            self.current.mixed_value_kinds.insert(name.to_string(), kind);
        } else {
            self.current.mixed_value_kinds.remove(name);
        }
    }

    pub(in crate::codegen::wasm) fn declare_i64_local(&mut self, name: String) {
        self.current.locals.insert(name, LocalKind::I64);
    }

    pub(in crate::codegen::wasm) fn declare_f64_local(&mut self, name: String) {
        self.current.locals.insert(name, LocalKind::F64);
    }

    pub(in crate::codegen::wasm) fn declare_i32_local(&mut self, name: String) {
        self.current.locals.insert(name, LocalKind::I32);
    }

    pub(in crate::codegen::wasm) fn declare_mixed_local(&mut self, name: String) {
        self.current.locals.insert(name, LocalKind::Mixed);
    }

    pub(in crate::codegen::wasm) fn declare_array_local(&mut self, name: String) {
        self.current.locals.insert(name, LocalKind::Array);
    }

    pub(in crate::codegen::wasm) fn declare_object_local(&mut self, name: String) {
        self.current.locals.insert(name, LocalKind::Object);
    }

}
