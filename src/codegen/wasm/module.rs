//! Purpose:
//! Tracks browser WASM module state while lowering PHP AST nodes.
//! Owns locals, function bodies, string data allocation, and output assembly.
//!
//! Called from:
//! - `crate::codegen::wasm::wat`
//! - `crate::codegen::wasm::expr` and `crate::codegen::wasm::stmt`
//!
//! Key details:
//! - Strings live in linear memory data segments; string variables use paired ptr/len locals.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::Name;
use crate::parser::ast::{
    BinOp, CallableTarget, CastType, Expr, ExprKind, InstanceOfTarget, Program, StaticReceiver, Stmt,
    StmtKind, TraitUse, TypeExpr, Visibility,
};
use crate::types::{builtin_call_sig, call_args};

use super::emitter::{escape_wat_bytes, WatEmitter};

mod array_metadata;
mod array_access_metadata;
mod array_filter_metadata;
mod array_rand_metadata;
mod array_state;
mod array_static_metadata;
mod array_transform_metadata;
mod constant_metadata;
mod function_metadata;
mod function_state;
mod local_state;
mod local_metadata;
mod object_metadata;

use array_static_metadata::{
    normalized_static_assoc_items, php_array_int_key, static_assoc_key_kind_for_expr,
    static_assoc_key_kinds_for_items, static_assoc_key_value_for_expr,
    static_assoc_key_values_for_items, static_or_const_int_value_for_locals,
    static_value_cell_kind_for_expr, static_value_cell_kinds_for_assoc_items,
    static_value_cell_kinds_for_items, static_value_cell_truthiness_for_filter,
};
use constant_metadata::*;
use function_metadata::*;
use local_metadata::*;
pub(super) use local_metadata::value_kind_for_local;
pub(in crate::codegen::wasm) use object_metadata::{
    ObjectClassInfo, ObjectPropertyInfo, ObjectPropertyKind, ObjectStaticPropertyInfo,
};

const DATA_START: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ValueKind {
    Int,
    Float,
    Bool,
    Str,
    Array,
    Object,
    Mixed,
    Null,
    Never,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LocalKind {
    I64,
    F64,
    I32,
    Str,
    Array,
    Object,
    Mixed,
    Callable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ArrayLayout {
    CompactInt,
    Value,
    Assoc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ValueCellKind {
    Int,
    Float,
    Bool,
    Str,
    Array,
    Null,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AssocKeyKind {
    Int,
    Str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NestedArrayMetadata {
    pub(super) layout: ArrayLayout,
    pub(super) len: usize,
    pub(super) value_kinds: Option<Vec<ValueCellKind>>,
    pub(super) key_values: Option<Vec<AssocKeyValue>>,
    pub(super) nested_values: Option<Vec<Option<NestedArrayMetadata>>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AssocKeyValue {
    Int(i64),
    Str(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum ConstantValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Null,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum ConstantArrayValue {
    Indexed(Vec<Expr>),
    Assoc(Vec<(Expr, Expr)>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EnumCaseMetadata {
    pub(super) name: String,
    pub(super) value: Option<EnumCaseBackingValue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum EnumCaseBackingValue {
    Int(i64),
    Str(String),
}

#[derive(Clone, Debug)]
struct DataSegment {
    offset: usize,
    bytes: Vec<u8>,
}

pub(super) struct WasmModule {
    current: FunctionBody,
    functions: Vec<FunctionBody>,
    function_params: HashMap<String, Vec<String>>,
    function_param_kinds: HashMap<String, Vec<LocalKind>>,
    function_defaults: HashMap<String, Vec<Option<Expr>>>,
    function_return_kinds: HashMap<String, ValueKind>,
    function_return_object_classes: HashMap<String, String>,
    nullable_function_returns: HashSet<String>,
    function_static_string_returns: HashMap<String, String>,
    function_possible_static_string_returns: HashMap<String, Vec<String>>,
    function_mixed_return_kinds: HashMap<String, ValueCellKind>,
    function_array_return_lengths: HashMap<String, usize>,
    function_array_return_layouts: HashMap<String, ArrayLayout>,
    function_array_return_value_kinds: HashMap<String, Vec<ValueCellKind>>,
    function_array_return_value_constants: HashMap<String, Vec<ConstantValue>>,
    function_array_return_runtime_value_kinds: HashMap<String, ValueCellKind>,
    function_array_return_nested_values: HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    function_array_return_key_kinds: HashMap<String, Vec<AssocKeyKind>>,
    function_array_return_key_values: HashMap<String, Vec<AssocKeyValue>>,
    function_array_return_param_indices: HashMap<String, usize>,
    function_array_param_lengths: HashMap<String, Vec<Option<usize>>>,
    function_array_param_value_kinds: HashMap<String, Vec<Option<Vec<ValueCellKind>>>>,
    function_array_param_runtime_nested_values: HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    function_array_param_layout_conflicts: Vec<(String, usize)>,
    function_array_param_key_kinds: HashMap<String, Vec<Option<Vec<AssocKeyKind>>>>,
    function_array_param_key_values: HashMap<String, Vec<Option<Vec<AssocKeyValue>>>>,
    function_callable_param_targets: HashMap<String, Vec<Option<String>>>,
    constants: HashMap<String, ConstantValue>,
    array_constants: HashMap<String, ConstantArrayValue>,
    class_names: HashSet<String>,
    object_classes: HashMap<String, object_metadata::ObjectClassInfo>,
    interface_names: HashSet<String>,
    interface_parents: HashMap<String, Vec<String>>,
    trait_names: HashSet<String>,
    enum_names: HashSet<String>,
    enum_cases: HashMap<String, Vec<EnumCaseMetadata>>,
    class_constants: HashMap<String, ConstantValue>,
    static_property_nested_values: HashMap<String, NestedArrayMetadata>,
    loop_stack: Vec<LoopLabels>,
    break_stack: Vec<String>,
    next_label_id: usize,
    strings: Vec<DataSegment>,
    next_data_offset: usize,
    scratch_byte: Option<usize>,
}

impl WasmModule {
    pub(super) fn new(program: &Program) -> Self {
        let class_constants = collect_class_constants(program);
        let constants = collect_constants_with_class_constants(program, &class_constants);
        let object_classes = object_metadata::collect_object_classes(program);
        let function_params = collect_function_params(program);
        let function_param_kinds = collect_function_param_kinds(program);
        let function_defaults = collect_function_defaults(program);
        let function_return_kinds = collect_function_return_kinds(program);
        let function_return_object_classes = collect_function_return_object_classes(program);
        let nullable_function_returns = collect_nullable_function_returns(program);
        let (
            function_params,
            function_param_kinds,
            function_defaults,
            function_return_kinds,
            function_return_object_classes,
            nullable_function_returns,
        ) =
            add_object_method_function_metadata(
                &object_classes,
                function_params,
                function_param_kinds,
                function_defaults,
                function_return_kinds,
                function_return_object_classes,
                nullable_function_returns,
            );
        let mut function_array_return_lengths = collect_function_array_return_lengths(
            program,
            &constants,
            &object_classes,
        );
        let mut function_array_return_layouts = collect_function_array_return_layouts(
            program,
            &constants,
            &function_return_kinds,
            &object_classes,
        );
        let mut function_array_return_value_kinds = collect_function_array_return_value_kinds(
            program,
            &constants,
            &object_classes,
        );
        let mut function_array_return_value_constants =
            collect_function_array_return_value_constants(program, &constants, &object_classes);
        let mut function_array_return_runtime_value_kinds =
            collect_function_array_return_runtime_value_kinds(
                program,
                &function_return_kinds,
                &object_classes,
            );
        let mut function_array_return_nested_values =
            collect_function_array_return_nested_values(program, &object_classes);
        let mut function_array_return_key_kinds =
            collect_function_array_return_key_kinds(program, &object_classes);
        let mut function_array_return_key_values =
            collect_function_array_return_key_values(program, &object_classes);
        add_object_method_array_return_metadata_aliases(
            &object_classes,
            &mut function_array_return_lengths,
        );
        add_object_method_array_return_metadata_aliases(
            &object_classes,
            &mut function_array_return_layouts,
        );
        add_object_method_array_return_metadata_aliases(
            &object_classes,
            &mut function_array_return_value_kinds,
        );
        add_object_method_array_return_metadata_aliases(
            &object_classes,
            &mut function_array_return_value_constants,
        );
        add_object_method_array_return_metadata_aliases(
            &object_classes,
            &mut function_array_return_runtime_value_kinds,
        );
        add_object_method_array_return_metadata_aliases(
            &object_classes,
            &mut function_array_return_nested_values,
        );
        add_object_method_array_return_metadata_aliases(
            &object_classes,
            &mut function_array_return_key_kinds,
        );
        add_object_method_array_return_metadata_aliases(
            &object_classes,
            &mut function_array_return_key_values,
        );
        let function_array_return_param_indices =
            collect_function_array_return_param_indices(program, &function_param_kinds);
        let function_array_param_lengths = collect_function_array_param_lengths(
            program,
            &function_params,
            &function_param_kinds,
            &function_defaults,
            &function_array_return_lengths,
            &function_array_return_layouts,
            &function_array_return_param_indices,
        );
        let function_array_param_value_kinds = collect_function_array_param_value_kinds(
            program,
            &function_params,
            &function_param_kinds,
            &function_defaults,
            &function_array_return_value_kinds,
            &function_array_return_param_indices,
        );
        let function_array_param_runtime_nested_values =
            collect_function_array_param_runtime_nested_values(
                program,
                &function_params,
                &function_param_kinds,
                &function_defaults,
                &function_array_return_value_kinds,
                &function_array_return_runtime_value_kinds,
                &function_array_return_param_indices,
            );
        let function_array_param_layout_conflicts = collect_function_array_param_layout_conflicts(
            program,
            &function_params,
            &function_param_kinds,
            &function_defaults,
            &function_array_return_layouts,
            &function_array_return_param_indices,
        );
        let (function_array_param_key_kinds, function_array_param_key_values) =
            collect_function_array_param_assoc_metadata(
                program,
                &function_params,
                &function_param_kinds,
                &function_defaults,
                &function_array_return_key_kinds,
                &function_array_return_key_values,
                &function_array_return_param_indices,
            );
        let function_callable_param_targets = collect_function_callable_param_targets(
            program,
            &function_params,
            &function_param_kinds,
            &function_defaults,
        );
        for (function, param_index) in &function_array_return_param_indices {
            if function_array_param_key_kinds
                .get(function)
                .and_then(|items| items.get(*param_index))
                .and_then(Option::as_ref)
                .is_some()
            {
                function_array_return_layouts.insert(function.clone(), ArrayLayout::Assoc);
            }
            if let Some(len) = function_array_param_lengths
                .get(function)
                .and_then(|items| items.get(*param_index))
                .and_then(|item| *item)
            {
                function_array_return_lengths.insert(function.clone(), len);
            }
            if let Some(kinds) = function_array_param_value_kinds
                .get(function)
                .and_then(|items| items.get(*param_index))
                .and_then(Option::as_ref)
            {
                function_array_return_value_kinds.insert(function.clone(), kinds.clone());
            }
            if let Some(metadata) = function_array_param_runtime_nested_values
                .get(function)
                .and_then(|items| items.get(*param_index))
                .and_then(Option::as_ref)
            {
                function_array_return_runtime_value_kinds.insert(function.clone(), ValueCellKind::Array);
                function_array_return_nested_values.insert(function.clone(), vec![Some(metadata.clone())]);
            }
            if let Some(kinds) = function_array_param_key_kinds
                .get(function)
                .and_then(|items| items.get(*param_index))
                .and_then(Option::as_ref)
            {
                function_array_return_key_kinds.insert(function.clone(), kinds.clone());
            }
            if let Some(values) = function_array_param_key_values
                .get(function)
                .and_then(|items| items.get(*param_index))
                .and_then(Option::as_ref)
            {
                function_array_return_key_values.insert(function.clone(), values.clone());
            }
        }
        let mut module = Self {
            current: FunctionBody::main(),
            functions: Vec::new(),
            function_params,
            function_param_kinds,
            function_defaults,
            function_return_kinds,
            function_return_object_classes,
            nullable_function_returns,
            function_static_string_returns: collect_function_static_string_returns(program, &constants),
            function_possible_static_string_returns: collect_function_possible_static_string_returns(program, &constants),
            function_mixed_return_kinds: collect_function_mixed_return_kinds(program),
            function_array_return_lengths,
            function_array_return_layouts,
            function_array_return_value_kinds,
            function_array_return_value_constants,
            function_array_return_runtime_value_kinds,
            function_array_return_nested_values,
            function_array_return_key_kinds,
            function_array_return_key_values,
            function_array_return_param_indices,
            function_array_param_lengths,
            function_array_param_value_kinds,
            function_array_param_runtime_nested_values,
            function_array_param_layout_conflicts,
            function_array_param_key_kinds,
            function_array_param_key_values,
            function_callable_param_targets,
            constants,
            array_constants: collect_array_constants(program),
            class_names: collect_decl_names(program, DeclKind::Class),
            object_classes,
            interface_names: collect_decl_names(program, DeclKind::Interface),
            interface_parents: collect_interface_parents(program),
            trait_names: collect_decl_names(program, DeclKind::Trait),
            enum_names: collect_decl_names(program, DeclKind::Enum),
            enum_cases: collect_enum_cases(program),
            class_constants,
            static_property_nested_values: HashMap::new(),
            loop_stack: Vec::new(),
            break_stack: Vec::new(),
            next_label_id: 0,
            strings: Vec::new(),
            next_data_offset: DATA_START,
            scratch_byte: None,
        };
        module.collect_locals(program);
        module
    }

    pub(super) fn validate_static_metadata(&self) -> Result<(), CompileError> {
        if let Some((function, param_index)) = self.function_array_param_layout_conflicts.first() {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!(
                    "wasm32-web array parameter ${} of function {} cannot mix indexed and associative layouts",
                    self.function_params
                        .get(function)
                        .and_then(|params| params.get(*param_index))
                        .map_or("?", String::as_str),
                    function
                ),
            ));
        }
        Ok(())
    }

    pub(super) fn constant_value(&self, name: &Name) -> Option<ConstantValue> {
        self.constants.get(name.as_str()).cloned()
    }

    pub(super) fn array_constant_value(&self, name: &Name) -> Option<ConstantArrayValue> {
        self.array_constants.get(name.as_str()).cloned()
    }

    pub(super) fn class_constant_value(
        &self,
        receiver: &StaticReceiver,
        name: &str,
    ) -> Option<ConstantValue> {
        let class_name = self.class_name_for_receiver(receiver)?;
        self.class_constants
            .get(&class_const_key(&class_name, name))
            .cloned()
    }

    pub(super) fn class_name_for_receiver(&self, receiver: &StaticReceiver) -> Option<String> {
        match receiver {
            StaticReceiver::Named(name) => Some(name.as_str().to_string()),
            StaticReceiver::Self_ => self.current.current_class.clone(),
            StaticReceiver::Parent => {
                let current_class = self.current.current_class.as_ref()?;
                let parent_key = self.object_class(current_class)?.parent.clone()?;
                Some(self.object_class(&parent_key)?.name.clone())
            }
            StaticReceiver::Static => self
                .current
                .late_static_class
                .clone()
                .or_else(|| self.static_class_name_for_current_context()),
        }
    }

    fn static_class_name_for_current_context(&self) -> Option<String> {
        let current_class = self.current.current_class.as_ref()?;
        let current_key = function_key(current_class);
        let current_info = self.object_class(current_class)?;
        if current_info.parent.is_some()
            || self
                .object_classes
                .values()
                .any(|class_info| class_info.parent.as_deref() == Some(current_key.as_str()))
        {
            return None;
        }
        Some(current_info.name.clone())
    }

    pub(super) fn declared_type_exists(&self, kind: &str, name: &str) -> bool {
        let key = function_key(name);
        match kind {
            "class_exists" => self.class_names.contains(&key),
            "interface_exists" => self.interface_names.contains(&key),
            "trait_exists" => self.trait_names.contains(&key),
            "enum_exists" => self.enum_names.contains(&key),
            _ => false,
        }
    }

    pub(super) fn declared_type_names(&self, kind: &str) -> Vec<String> {
        let mut names = match kind {
            "class_exists" => self
                .object_classes
                .values()
                .map(|class_info| function_key(&class_info.name))
                .collect::<Vec<_>>(),
            "interface_exists" => self.interface_names.iter().cloned().collect(),
            "trait_exists" => self.trait_names.iter().cloned().collect(),
            "enum_exists" => self.enum_names.iter().cloned().collect(),
            _ => Vec::new(),
        };
        names.sort();
        names
    }

    pub(super) fn object_class(&self, name: &str) -> Option<&object_metadata::ObjectClassInfo> {
        self.object_classes.get(&function_key(name))
    }

    pub(super) fn enum_case_class(
        &self,
        receiver: &StaticReceiver,
        case_name: &str,
    ) -> Option<String> {
        let class_name = self.class_name_for_receiver(receiver)?;
        let cases = self.enum_cases.get(&function_key(&class_name))?;
        cases
            .iter()
            .any(|case| case.name.eq_ignore_ascii_case(case_name))
            .then_some(class_name)
    }

    pub(super) fn enum_case_names(&self, class_name: &str) -> Option<Vec<String>> {
        Some(
            self.enum_cases
                .get(&function_key(class_name))?
                .iter()
                .map(|case| case.name.clone())
                .collect(),
        )
    }

    pub(super) fn enum_case_backing_value(
        &self,
        receiver: &StaticReceiver,
        case_name: &str,
    ) -> Option<EnumCaseBackingValue> {
        let class_name = self.class_name_for_receiver(receiver)?;
        self.enum_cases
            .get(&function_key(&class_name))?
            .iter()
            .find(|case| case.name.eq_ignore_ascii_case(case_name))?
            .value
            .clone()
    }

    pub(super) fn enum_case_name_for_backing_value(
        &self,
        class_name: &str,
        value: &EnumCaseBackingValue,
    ) -> Option<String> {
        self.enum_cases
            .get(&function_key(class_name))?
            .iter()
            .find(|case| case.value.as_ref() == Some(value))
            .map(|case| case.name.clone())
    }

    pub(super) fn enum_case_global(&self, class_name: &str, case_name: &str) -> String {
        format!(
            "__wasm_enum_case_{}_{}",
            crate::names::mangle_fqn(&function_key(class_name)),
            crate::names::mangle_fqn(&function_key(case_name))
        )
    }

    pub(super) fn object_class_names_by_id(&self) -> Vec<(u64, String)> {
        let mut classes = self
            .object_classes
            .values()
            .map(|class_info| (class_info.class_id, class_info.name.clone()))
            .collect::<Vec<_>>();
        classes.sort_by_key(|(class_id, _)| *class_id);
        classes
    }

    pub(super) fn current_class(&self) -> Option<&str> {
        self.current.current_class.as_deref()
    }

    pub(super) fn object_member_is_accessible(
        &self,
        owner_class: &str,
        visibility: &Visibility,
    ) -> bool {
        match visibility {
            Visibility::Public => true,
            Visibility::Private => self
                .current_class()
                .is_some_and(|current| function_key(current) == function_key(owner_class)),
            Visibility::Protected => self.current_class().is_some_and(|current| {
                function_key(current) == function_key(owner_class)
                    || self.object_class_extends(current, owner_class)
                    || self.object_class_extends(owner_class, current)
            }),
        }
    }

    fn object_class_extends(&self, class_name: &str, ancestor_name: &str) -> bool {
        let ancestor_key = function_key(ancestor_name);
        let mut current = self
            .object_class(class_name)
            .and_then(|class_info| class_info.parent.clone());
        while let Some(class_key) = current {
            if class_key == ancestor_key {
                return true;
            }
            current = self
                .object_class(&class_key)
                .and_then(|class_info| class_info.parent.clone());
        }
        false
    }

    pub(super) fn object_static_property(
        &self,
        receiver: &StaticReceiver,
        property: &str,
    ) -> Option<object_metadata::ObjectStaticPropertyInfo> {
        let class_name = self.class_name_for_receiver(receiver)?;
        self.object_class(&class_name)?
            .static_properties
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(property))
            .cloned()
    }

    pub(super) fn static_property_nested_value_metadata(
        &self,
        property: &object_metadata::ObjectStaticPropertyInfo,
    ) -> Option<NestedArrayMetadata> {
        self.static_property_nested_values
            .get(&property.global)
            .cloned()
    }

    pub(super) fn set_static_property_nested_value_metadata(
        &mut self,
        property: &object_metadata::ObjectStaticPropertyInfo,
        metadata: NestedArrayMetadata,
    ) {
        self.static_property_nested_values
            .insert(property.global.clone(), metadata);
    }

    pub(super) fn interface_extends(&self, interface: &str, target: &str) -> bool {
        let target_key = function_key(target);
        let mut stack = vec![function_key(interface)];
        let mut seen = HashSet::new();
        while let Some(current) = stack.pop() {
            if !seen.insert(current.clone()) {
                continue;
            }
            let Some(parents) = self.interface_parents.get(&current) else {
                continue;
            };
            for parent in parents {
                if parent == &target_key {
                    return true;
                }
                stack.push(parent.clone());
            }
        }
        false
    }

    pub(super) fn object_methods(&self) -> Vec<(String, object_metadata::ObjectMethodInfo)> {
        self.object_classes
            .iter()
            .flat_map(|(_, class_info)| {
                class_info
                    .constructor
                    .iter()
                    .chain(class_info.methods.iter())
                    .chain(class_info.static_methods.iter())
                    .cloned()
                    .map(|method| (class_info.name.clone(), method))
            })
            .collect()
    }

    pub(super) fn object_constructor(
        &self,
        class_name: &str,
    ) -> Option<object_metadata::ObjectMethodInfo> {
        let mut current = Some(function_key(class_name));
        while let Some(class_key) = current {
            let class_info = self.object_class(&class_key)?;
            if class_info.has_constructor {
                return class_info.constructor.clone();
            }
            current = class_info.parent.clone();
        }
        None
    }

    pub(super) fn object_has_constructor_in_hierarchy(&self, class_name: &str) -> bool {
        let mut current = Some(function_key(class_name));
        while let Some(class_key) = current {
            let Some(class_info) = self.object_class(&class_key) else {
                return false;
            };
            if class_info.has_constructor {
                return true;
            }
            current = class_info.parent.clone();
        }
        false
    }

    pub(super) fn object_method_in_hierarchy(
        &self,
        class_name: &str,
        method_name: &str,
    ) -> Option<(String, object_metadata::ObjectMethodInfo)> {
        let mut current = Some(function_key(class_name));
        while let Some(class_key) = current {
            let class_info = self.object_class(&class_key)?;
            if let Some(method) = class_info
                .methods
                .iter()
                .find(|method| method.name.eq_ignore_ascii_case(method_name))
                .cloned()
            {
                return Some((class_info.name.clone(), method));
            }
            current = class_info.parent.clone();
        }
        None
    }

    pub(super) fn object_static_method_in_hierarchy(
        &self,
        class_name: &str,
        method_name: &str,
    ) -> Option<object_metadata::ObjectMethodInfo> {
        let mut current = Some(function_key(class_name));
        while let Some(class_key) = current {
            let class_info = self.object_class(&class_key)?;
            if let Some(method) = class_info
                .static_methods
                .iter()
                .find(|method| method.name.eq_ignore_ascii_case(method_name))
                .cloned()
            {
                return Some(method);
            }
            current = class_info.parent.clone();
        }
        None
    }

    pub(super) fn object_class_for_local(&self, name: &str) -> Option<String> {
        self.current.object_local_classes.get(name).cloned()
    }

    pub(super) fn declared_object_type_for_local(&self, name: &str) -> Option<String> {
        self.current.object_local_declared_types.get(name).cloned()
    }

    pub(super) fn set_declared_object_type_for_local(&mut self, name: &str, type_name: Option<String>) {
        if let Some(type_name) = type_name {
            self.current.object_local_declared_types.insert(name.to_string(), type_name);
        } else {
            self.current.object_local_declared_types.remove(name);
        }
    }

    pub(super) fn set_object_class_for_local(&mut self, name: &str, class_name: Option<String>) {
        if let Some(class_name) = class_name {
            self.current.object_local_classes.insert(name.to_string(), class_name);
        } else {
            self.current.object_local_classes.remove(name);
        }
    }

    pub(super) fn is_interface_name(&self, name: &str) -> bool {
        self.interface_names.contains(&function_key(name))
    }

    pub(super) fn object_class_implements_interface_or_parent(
        &self,
        class_name: &str,
        interface_name: &str,
    ) -> bool {
        metadata_class_implements_interface_or_parent(
            class_name,
            &function_key(interface_name),
            &self.object_classes,
            &self.interface_parents,
        )
    }

    pub(super) fn next_label(&mut self, prefix: &str) -> String {
        let id = self.next_label_id;
        self.next_label_id += 1;
        format!("${}_{}", prefix, id)
    }

    pub(super) fn push_loop(&mut self, break_label: String, continue_label: String) {
        self.break_stack.push(break_label.clone());
        self.loop_stack.push(LoopLabels {
            continue_label,
        });
    }

    pub(super) fn pop_loop(&mut self) {
        self.loop_stack.pop();
        self.break_stack.pop();
    }

    pub(super) fn break_label(&self, levels: usize) -> Option<&str> {
        if levels == 0 || levels > self.break_stack.len() {
            return None;
        }
        let index = self.break_stack.len() - levels;
        self.break_stack.get(index).map(String::as_str)
    }

    pub(super) fn continue_label(&self, levels: usize) -> Option<&str> {
        self.loop_branch_label(levels, |labels| labels.continue_label.as_str())
    }

    pub(super) fn intern_string(&mut self, value: &str) -> (usize, usize) {
        self.intern_bytes(value.as_bytes())
    }

    pub(super) fn intern_bytes(&mut self, bytes: &[u8]) -> (usize, usize) {
        let offset = self.next_data_offset;
        self.next_data_offset += bytes.len().max(1);
        let len = bytes.len();
        self.strings.push(DataSegment {
            offset,
            bytes: bytes.to_vec(),
        });
        (offset, len)
    }

    pub(super) fn scratch_byte_offset(&mut self) -> usize {
        if let Some(offset) = self.scratch_byte {
            return offset;
        }
        let offset = self.next_data_offset;
        self.next_data_offset += 1;
        self.scratch_byte = Some(offset);
        offset
    }

    pub(super) fn push_switch(&mut self, break_label: String) {
        self.break_stack.push(break_label);
    }

    pub(super) fn pop_switch(&mut self) {
        self.break_stack.pop();
    }

    pub(super) fn enter_function(
        &mut self,
        name: String,
        params: Vec<(String, Option<TypeExpr>)>,
        body: &[Stmt],
    ) -> Result<FunctionBody, CompileError> {
        let param_names = params.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>();
        let param_kinds = self.function_param_kinds(&name).ok_or_else(|| {
            CompileError::new(crate::span::Span::dummy(), "wasm32-web function metadata is missing")
        })?;
        let return_kind = self.function_return_kind(&name).ok_or_else(|| {
            CompileError::new(crate::span::Span::dummy(), "wasm32-web function metadata is missing")
        })?;
        if return_kind == ValueKind::Mixed && !body_guarantees_return(body) {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                "wasm32-web mixed-returning functions must return an explicit value",
            ));
        }
        let function_key = function_key(&name);
        let mut function = FunctionBody::function(name, param_names.clone(), param_kinds, return_kind);
        if let Some(lengths) = self.function_array_param_lengths.get(&function_key) {
            for (param, len) in param_names.iter().zip(lengths.iter()) {
                if let Some(len) = len {
                    function.array_lengths.insert(param.clone(), *len);
                }
            }
        }
        if let Some(value_kinds) = self.function_array_param_value_kinds.get(&function_key) {
            for (param, kinds) in param_names.iter().zip(value_kinds.iter()) {
                if let Some(kinds) = kinds {
                    function.array_value_kinds.insert(param.clone(), kinds.clone());
                }
            }
        }
        if let Some(metadata) = self.function_array_param_runtime_nested_values.get(&function_key) {
            for (param, metadata) in param_names.iter().zip(metadata.iter()) {
                if let Some(metadata) = metadata {
                    function.array_runtime_nested_value.insert(param.clone(), metadata.clone());
                }
            }
        }
        if let Some(key_kinds) = self.function_array_param_key_kinds.get(&function_key) {
            for (param, kinds) in param_names.iter().zip(key_kinds.iter()) {
                if let Some(kinds) = kinds {
                    function.array_layouts.insert(param.clone(), ArrayLayout::Assoc);
                    function.array_key_kinds.insert(param.clone(), kinds.clone());
                }
            }
        }
        if let Some(key_values) = self.function_array_param_key_values.get(&function_key) {
            for (param, values) in param_names.iter().zip(key_values.iter()) {
                if let Some(values) = values {
                    function.array_key_values.insert(param.clone(), values.clone());
                }
            }
        }
        if let Some(targets) = self.function_callable_param_targets.get(&function_key) {
            for (param, target) in param_names.iter().zip(targets.iter()) {
                if let Some(target) = target {
                    function.callable_targets.insert(param.clone(), target.clone());
                }
            }
        }
        let mut inferred = function.locals.iter().map(|(k, v)| (k.clone(), *v)).collect();
        let mut array_value_kinds = function.array_value_kinds.clone();
        let mut array_runtime_value_kinds = function.array_runtime_value_kind.clone();
        let mut array_nested_values = HashMap::new();
        let mut array_runtime_nested_values = HashMap::new();
        let mut array_key_kinds = function.array_key_kinds.clone();
        let mut array_key_values = function.array_key_values.clone();
        let mut php_normalized_key_arrays = HashSet::new();
        let mut callable_targets = HashMap::new();
        let mut string_static_values = HashMap::new();
        for stmt in body {
            collect_stmt_locals(
                stmt,
                &mut inferred,
                &mut array_value_kinds,
                &mut array_runtime_value_kinds,
                &mut array_nested_values,
                &mut array_runtime_nested_values,
                &mut array_key_kinds,
                &mut array_key_values,
                &mut php_normalized_key_arrays,
                &mut callable_targets,
                &mut string_static_values,
                &self.function_return_kinds,
                &self.function_static_string_returns,
                &self.function_possible_static_string_returns,
                &self.function_array_return_value_kinds,
                &self.function_array_return_runtime_value_kinds,
                &self.function_array_return_layouts,
                &self.function_array_return_nested_values,
                &self.function_array_return_key_kinds,
                &self.function_array_return_key_values,
                &self.function_array_return_param_indices,
                &self.constants,
                &self.class_constants,
                &self.array_constants,
            );
        }
        function.locals.extend(inferred);
        let initial_object_local_classes = object_param_classes(&params, &self.object_classes);
        let initial_object_local_declared_types = object_param_declared_types(&params);
        let object_local_classes = collect_object_local_classes(
            body,
            &self.object_classes,
            &self.enum_cases,
            &self.function_return_object_classes,
            &initial_object_local_classes,
        );
        let object_local_declared_types =
            collect_object_declared_local_types(body, &initial_object_local_declared_types);
        let object_method_return_locals = collect_object_method_return_locals(
            body,
            &self.object_classes,
            &self.interface_names,
            &self.interface_parents,
            &self.enum_cases,
            &self.function_return_object_classes,
            &self.function_return_kinds,
            &object_local_classes,
            &object_local_declared_types,
            &string_static_values,
            None,
        );
        function.locals.extend(object_method_return_locals);
        for name in object_local_classes.keys() {
            function.locals.insert(name.clone(), LocalKind::Object);
        }
        function.object_local_classes.extend(object_local_classes);
        function
            .object_local_declared_types
            .extend(object_local_declared_types);
        function.callable_targets.extend(callable_targets);
        function.string_static_values.extend(string_static_values);
        self.loop_stack.clear();
        self.break_stack.clear();
        Ok(std::mem::replace(&mut self.current, function))
    }

    pub(super) fn enter_object_method(
        &mut self,
        class_name: String,
        method: &object_metadata::ObjectMethodInfo,
    ) -> Result<FunctionBody, CompileError> {
        let mut params = if method.has_this {
            vec![("this".to_string(), None)]
        } else {
            Vec::new()
        };
        params.extend(method.params.iter().cloned());
        let previous = self.enter_function(method.symbol.clone(), params, &method.body)?;
        self.current.current_class = Some(method.owner_class.clone());
        self.current.late_static_class = Some(class_name.clone());
        if method.has_this {
            self.current
                .object_local_classes
                .insert("this".to_string(), class_name.clone());
            self.current
                .object_local_declared_types
                .insert("this".to_string(), class_name.clone());
            let mut initial_object_local_classes = HashMap::new();
            initial_object_local_classes.insert("this".to_string(), class_name.clone());
            let mut initial_object_local_declared_types = object_param_declared_types(&method.params);
            initial_object_local_declared_types.insert("this".to_string(), class_name.clone());
            let object_local_classes = collect_object_local_classes(
                &method.body,
                &self.object_classes,
                &self.enum_cases,
                &self.function_return_object_classes,
                &initial_object_local_classes,
            );
            let object_local_declared_types =
                collect_object_declared_local_types(&method.body, &initial_object_local_declared_types);
            let object_method_return_locals = collect_object_method_return_locals(
                &method.body,
                &self.object_classes,
                &self.interface_names,
                &self.interface_parents,
                &self.enum_cases,
                &self.function_return_object_classes,
                &self.function_return_kinds,
                &object_local_classes,
                &object_local_declared_types,
                &self.current.string_static_values,
                Some(&class_name),
            );
            self.current.locals.extend(object_method_return_locals);
            let scoped_constant_locals = collect_scoped_constant_locals(
                &method.body,
                &self.object_classes,
                &self.class_constants,
                &class_name,
            );
            self.current.locals.extend(scoped_constant_locals);
            for name in object_local_classes.keys() {
                self.current.locals.insert(name.clone(), LocalKind::Object);
            }
            self.current.object_local_classes.extend(object_local_classes);
            self.current
                .object_local_declared_types
                .extend(object_local_declared_types);
        } else {
            let initial_object_local_declared_types = object_param_declared_types(&method.params);
            let object_local_declared_types =
                collect_object_declared_local_types(&method.body, &initial_object_local_declared_types);
            let object_method_return_locals = collect_object_method_return_locals(
                &method.body,
                &self.object_classes,
                &self.interface_names,
                &self.interface_parents,
                &self.enum_cases,
                &self.function_return_object_classes,
                &self.function_return_kinds,
                &HashMap::new(),
                &object_local_declared_types,
                &self.current.string_static_values,
                Some(&class_name),
            );
            self.current.locals.extend(object_method_return_locals);
            let scoped_constant_locals = collect_scoped_constant_locals(
                &method.body,
                &self.object_classes,
                &self.class_constants,
                &class_name,
            );
            self.current.locals.extend(scoped_constant_locals);
            self.current
                .object_local_declared_types
                .extend(object_local_declared_types);
        }
        Ok(previous)
    }

    pub(super) fn leave_function(&mut self, previous: FunctionBody) {
        let function = std::mem::replace(&mut self.current, previous);
        self.functions.push(function);
        self.loop_stack.clear();
        self.break_stack.clear();
    }

    pub(super) fn finish(mut self) -> String {
        let static_string_defaults = intern_static_property_string_defaults(&mut self);
        let heap_start = align_to(self.next_data_offset, 16);
        let mut out = WatEmitter::new();
        out.open("(module");
        out.line("(import \"elephc\" \"write\" (func $host_write (param i32 i32)))");
        out.line("(import \"elephc\" \"writeInt\" (func $host_write_int (param i64)))");
        out.line("(import \"elephc\" \"writeFloat\" (func $host_write_float (param f64)))");
        out.line("(import \"elephc\" \"floatToString\" (func $host_float_to_string (param f64 i32) (result i32)))");
        out.line("(import \"elephc\" \"formatFloatFixed\" (func $host_format_float_fixed (param f64 i32 i32) (result i32)))");
        out.line("(import \"elephc\" \"formatFloatScientific\" (func $host_format_float_scientific (param f64 i32 i32 i32) (result i32)))");
        out.line("(import \"elephc\" \"formatFloatGeneral\" (func $host_format_float_general (param f64 i32 i32 i32) (result i32)))");
        out.line("(import \"elephc\" \"numberFormatFloat\" (func $host_number_format_float (param f64 i32 i32 i32 i32 i32 i32) (result i32)))");
        out.line("(import \"elephc\" \"numericStringValue\" (func $host_numeric_string_value (param i32 i32 i32) (result i32)))");
        out.line("(import \"elephc\" \"leadingNumericStringValue\" (func $host_leading_numeric_string_value (param i32 i32 i32) (result i32)))");
        out.line("(import \"elephc\" \"pow\" (func $host_pow (param f64 f64) (result f64)))");
        out.line("(import \"elephc\" \"sin\" (func $host_sin (param f64) (result f64)))");
        out.line("(import \"elephc\" \"cos\" (func $host_cos (param f64) (result f64)))");
        out.line("(import \"elephc\" \"tan\" (func $host_tan (param f64) (result f64)))");
        out.line("(import \"elephc\" \"asin\" (func $host_asin (param f64) (result f64)))");
        out.line("(import \"elephc\" \"acos\" (func $host_acos (param f64) (result f64)))");
        out.line("(import \"elephc\" \"atan\" (func $host_atan (param f64) (result f64)))");
        out.line("(import \"elephc\" \"sinh\" (func $host_sinh (param f64) (result f64)))");
        out.line("(import \"elephc\" \"cosh\" (func $host_cosh (param f64) (result f64)))");
        out.line("(import \"elephc\" \"tanh\" (func $host_tanh (param f64) (result f64)))");
        out.line("(import \"elephc\" \"log\" (func $host_log (param f64) (result f64)))");
        out.line("(import \"elephc\" \"log10\" (func $host_log10 (param f64) (result f64)))");
        out.line("(import \"elephc\" \"exp\" (func $host_exp (param f64) (result f64)))");
        out.line("(import \"elephc\" \"fmod\" (func $host_fmod (param f64 f64) (result f64)))");
        out.line("(import \"elephc\" \"atan2\" (func $host_atan2 (param f64 f64) (result f64)))");
        out.line("(import \"elephc\" \"hypot\" (func $host_hypot (param f64 f64) (result f64)))");
        out.line("(import \"elephc\" \"hashHex\" (func $host_hash_hex (param i32 i32 i32 i32) (result i32)))");
        out.line("(import \"elephc\" \"hashRaw\" (func $host_hash_raw (param i32 i32 i32 i32) (result i32)))");
        out.line("(import \"elephc\" \"hashHexName\" (func $host_hash_hex_name (param i32 i32 i32 i32 i32) (result i32)))");
        out.line("(import \"elephc\" \"hashRawName\" (func $host_hash_raw_name (param i32 i32 i32 i32 i32) (result i32)))");
        out.line("(import \"elephc\" \"jsonValidate\" (func $host_json_validate (param i32 i32 i32 i32) (result i32)))");
        out.line("(import \"elephc\" \"randomU32\" (func $host_random_u32 (result i32)))");
        out.line("(memory (export \"memory\") 1)");
        out.line(&format!(
            "(global $heap (mut i32) (i32.const {}))",
            heap_start
        ));
        out.line("(global $json_last_error (mut i32) (i32.const 0))");
        emit_static_property_globals(&mut out, &static_string_defaults, &self);
        emit_enum_case_globals(&mut out, &self);
        super::runtime::emit(&mut out);
        for segment in self.strings {
            let escaped = escape_wat_bytes(&segment.bytes);
            out.line(&format!("(data (i32.const {}) \"{}\")", segment.offset, escaped));
        }
        for function in self.functions {
            emit_function(&mut out, function);
        }
        emit_function(&mut out, self.current);
        out.close(")");
        out.finish()
    }

    fn collect_locals(&mut self, program: &Program) {
        let mut inferred = HashMap::new();
        let mut array_value_kinds = HashMap::new();
        let mut array_runtime_value_kinds = HashMap::new();
        let mut array_nested_values = HashMap::new();
        let mut array_runtime_nested_values = HashMap::new();
        let mut array_key_kinds = HashMap::new();
        let mut array_key_values = HashMap::new();
        let mut php_normalized_key_arrays = HashSet::new();
        let mut callable_targets = HashMap::new();
        let mut string_static_values = HashMap::new();
        for stmt in program {
            collect_stmt_locals(
                stmt,
                &mut inferred,
                &mut array_value_kinds,
                &mut array_runtime_value_kinds,
                &mut array_nested_values,
                &mut array_runtime_nested_values,
                &mut array_key_kinds,
                &mut array_key_values,
                &mut php_normalized_key_arrays,
                &mut callable_targets,
                &mut string_static_values,
                &self.function_return_kinds,
                &self.function_static_string_returns,
                &self.function_possible_static_string_returns,
                &self.function_array_return_value_kinds,
                &self.function_array_return_runtime_value_kinds,
                &self.function_array_return_layouts,
                &self.function_array_return_nested_values,
                &self.function_array_return_key_kinds,
                &self.function_array_return_key_values,
                &self.function_array_return_param_indices,
                &self.constants,
                &self.class_constants,
                &self.array_constants,
            );
        }
        self.current.locals.extend(inferred);
        self.current.callable_targets.extend(callable_targets);
        self.current.string_static_values.extend(string_static_values);
        let object_local_classes = collect_object_local_classes(
            program,
            &self.object_classes,
            &self.enum_cases,
            &self.function_return_object_classes,
            &HashMap::new(),
        );
        let object_local_declared_types =
            collect_object_declared_local_types(program, &HashMap::new());
        let object_method_return_locals = collect_object_method_return_locals(
            program,
            &self.object_classes,
            &self.interface_names,
            &self.interface_parents,
            &self.enum_cases,
            &self.function_return_object_classes,
            &self.function_return_kinds,
            &object_local_classes,
            &object_local_declared_types,
            &self.current.string_static_values,
            None,
        );
        self.current.locals.extend(object_method_return_locals);
        for name in object_local_classes.keys() {
            self.current.locals.insert(name.clone(), LocalKind::Object);
        }
        self.current.object_local_classes.extend(object_local_classes);
        self.current
            .object_local_declared_types
            .extend(object_local_declared_types);
    }

    fn loop_branch_label(
        &self,
        levels: usize,
        select: impl FnOnce(&LoopLabels) -> &str,
    ) -> Option<&str> {
        if levels == 0 || levels > self.loop_stack.len() {
            return None;
        }
        let index = self.loop_stack.len() - levels;
        Some(select(&self.loop_stack[index]))
    }
}

fn intern_static_property_string_defaults(module: &mut WasmModule) -> HashMap<String, (usize, usize)> {
    let static_properties = module
        .object_classes
        .values()
        .flat_map(|class_info| class_info.static_properties.clone())
        .collect::<Vec<_>>();
    let mut strings = HashMap::new();
    for property in static_properties {
        if property.kind != ObjectPropertyKind::Str {
            continue;
        }
        let value = match property.default.as_ref().map(|expr| &expr.kind) {
            Some(ExprKind::StringLiteral(value)) => value.as_str(),
            Some(_) | None => "",
        };
        strings.insert(property.global.clone(), module.intern_string(value));
    }
    strings
}

fn emit_static_property_globals(
    out: &mut WatEmitter,
    static_string_defaults: &HashMap<String, (usize, usize)>,
    module: &WasmModule,
) {
    let static_properties = module
        .object_classes
        .values()
        .flat_map(|class_info| class_info.static_properties.clone())
        .collect::<Vec<_>>();
    let mut emitted = HashSet::new();
    for property in static_properties {
        if !emitted.insert(property.global.clone()) {
            continue;
        }
        match property.kind {
            ObjectPropertyKind::Int => {
                let value = match property.default.as_ref().map(|expr| &expr.kind) {
                    Some(ExprKind::IntLiteral(value)) => *value,
                    Some(_) | None => 0,
                };
                out.line(&format!(
                    "(global ${} (mut i64) (i64.const {}))",
                    property.global, value
                ));
            }
            ObjectPropertyKind::Float => {
                let value = match property.default.as_ref().map(|expr| &expr.kind) {
                    Some(ExprKind::FloatLiteral(value)) => *value,
                    Some(ExprKind::IntLiteral(value)) => *value as f64,
                    Some(_) | None => 0.0,
                };
                out.line(&format!(
                    "(global ${} (mut f64) (f64.const {}))",
                    property.global, value
                ));
            }
            ObjectPropertyKind::Bool => {
                let value = match property.default.as_ref().map(|expr| &expr.kind) {
                    Some(ExprKind::BoolLiteral(value)) => i32::from(*value),
                    Some(_) | None => 0,
                };
                out.line(&format!(
                    "(global ${} (mut i32) (i32.const {}))",
                    property.global, value
                ));
            }
            ObjectPropertyKind::Str => {
                let (ptr, len) = static_string_defaults
                    .get(&property.global)
                    .copied()
                    .unwrap_or((0, 0));
                out.line(&format!(
                    "(global ${}_ptr (mut i32) (i32.const {}))",
                    property.global, ptr
                ));
                out.line(&format!(
                    "(global ${}_len (mut i32) (i32.const {}))",
                    property.global, len
                ));
            }
            ObjectPropertyKind::Mixed => {
                out.line(&format!(
                    "(global ${} (mut i32) (i32.const 0))",
                    property.global
                ));
            }
            ObjectPropertyKind::Object => {
                out.line(&format!(
                    "(global ${} (mut i32) (i32.const 0))",
                    property.global
                ));
            }
        }
        if property.needs_initialized_guard {
            out.line(&format!(
                "(global ${}_initialized (mut i32) (i32.const 0))",
                property.global
            ));
        }
    }
}

fn emit_enum_case_globals(out: &mut WatEmitter, module: &WasmModule) {
    for (class_key, cases) in &module.enum_cases {
        let Some(class_info) = module.object_classes.get(class_key) else {
            continue;
        };
        for case in cases {
            out.line(&format!(
                "(global ${} (mut i32) (i32.const 0))",
                module.enum_case_global(&class_info.name, &case.name)
            ));
        }
    }
}

fn collect_object_local_classes(
    stmts: &[Stmt],
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
    enum_cases: &HashMap<String, Vec<EnumCaseMetadata>>,
    function_return_object_classes: &HashMap<String, String>,
    initial_locals: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut locals = initial_locals.clone();
    collect_object_local_classes_in_stmts(
        stmts,
        object_classes,
        enum_cases,
        function_return_object_classes,
        &mut locals,
    );
    locals
}

fn object_param_classes(
    params: &[(String, Option<TypeExpr>)],
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
) -> HashMap<String, String> {
    let mut locals = HashMap::new();
    for (name, type_expr) in params {
        let Some(type_expr) = type_expr else {
            continue;
        };
        if let Some(class_name) = object_class_name_for_type_expr(type_expr, object_classes) {
            locals.insert(name.clone(), class_name);
        }
    }
    locals
}

fn collect_object_declared_local_types(
    stmts: &[Stmt],
    initial_locals: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut locals = initial_locals.clone();
    collect_object_declared_local_types_in_stmts(stmts, &mut locals);
    locals
}

fn object_param_declared_types(params: &[(String, Option<TypeExpr>)]) -> HashMap<String, String> {
    let mut locals = HashMap::new();
    for (name, type_expr) in params {
        let Some(type_expr) = type_expr else {
            continue;
        };
        if let Some(type_name) = object_declared_type_name_for_type_expr(type_expr) {
            locals.insert(name.clone(), type_name);
        }
    }
    locals
}

fn collect_object_declared_local_types_in_stmts(stmts: &[Stmt], locals: &mut HashMap<String, String>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Assign { name, value } => {
                if let ExprKind::Variable(source) = &value.kind {
                    if let Some(type_name) = locals.get(source).cloned() {
                        locals.insert(name.clone(), type_name);
                    }
                }
            }
            StmtKind::TypedAssign { type_expr, name, value } => {
                if let Some(type_name) = object_declared_type_name_for_type_expr(type_expr) {
                    locals.insert(name.clone(), type_name);
                } else if let ExprKind::Variable(source) = &value.kind {
                    if let Some(type_name) = locals.get(source).cloned() {
                        locals.insert(name.clone(), type_name);
                    }
                }
            }
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                collect_object_declared_local_types_in_stmts(then_body, locals);
                for (_, body) in elseif_clauses {
                    collect_object_declared_local_types_in_stmts(body, locals);
                }
                if let Some(else_body) = else_body {
                    collect_object_declared_local_types_in_stmts(else_body, locals);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                collect_object_declared_local_types_in_stmts(body, locals);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_object_declared_local_types_in_stmts(body, locals);
                }
                if let Some(default) = default {
                    collect_object_declared_local_types_in_stmts(default, locals);
                }
            }
            _ => {}
        }
    }
}

fn collect_object_local_classes_in_stmts(
    stmts: &[Stmt],
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
    enum_cases: &HashMap<String, Vec<EnumCaseMetadata>>,
    function_return_object_classes: &HashMap<String, String>,
    locals: &mut HashMap<String, String>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Assign { name, value } => {
                if object_expr_may_be_null(value) {
                    locals.remove(name);
                } else if let Some(class_name) =
                    object_class_name_for_metadata_expr(value, object_classes, enum_cases, function_return_object_classes, locals, None)
                {
                    locals.insert(name.clone(), class_name);
                }
            }
            StmtKind::TypedAssign { type_expr, name, value } => {
                if let Some(class_name) = object_class_name_for_type_expr(type_expr, object_classes)
                    .or_else(|| object_class_name_for_metadata_expr(
                        value,
                        object_classes,
                        enum_cases,
                        function_return_object_classes,
                        locals,
                        None,
                    ))
                {
                    locals.insert(name.clone(), class_name);
                }
            }
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                collect_object_local_classes_in_stmts(
                    then_body,
                    object_classes,
                    enum_cases,
                    function_return_object_classes,
                    locals,
                );
                for (_, body) in elseif_clauses {
                    collect_object_local_classes_in_stmts(
                        body,
                        object_classes,
                        enum_cases,
                        function_return_object_classes,
                        locals,
                    );
                }
                if let Some(body) = else_body {
                    collect_object_local_classes_in_stmts(
                        body,
                        object_classes,
                        enum_cases,
                        function_return_object_classes,
                        locals,
                    );
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::NamespaceBlock { body, .. }
            | StmtKind::IncludeOnceGuard { body, .. } => collect_object_local_classes_in_stmts(
                body,
                object_classes,
                enum_cases,
                function_return_object_classes,
                locals,
            ),
            StmtKind::For { init, update, body, .. } => {
                if let Some(init) = init {
                    collect_object_local_classes_in_stmts(
                        std::slice::from_ref(init),
                        object_classes,
                        enum_cases,
                        function_return_object_classes,
                        locals,
                    );
                }
                collect_object_local_classes_in_stmts(
                    body,
                    object_classes,
                    enum_cases,
                    function_return_object_classes,
                    locals,
                );
                if let Some(update) = update {
                    collect_object_local_classes_in_stmts(
                        std::slice::from_ref(update),
                        object_classes,
                        enum_cases,
                        function_return_object_classes,
                        locals,
                    );
                }
            }
            StmtKind::Foreach {
                array,
                value_var,
                body,
                ..
            } => {
                if let Some(class_name) = enum_cases_class_name_for_metadata_expr(
                    array,
                    object_classes,
                    enum_cases,
                    locals,
                    None,
                )
                .or_else(|| {
                    homogeneous_object_array_class_for_metadata_expr(
                        array,
                        object_classes,
                        enum_cases,
                        function_return_object_classes,
                        locals,
                        None,
                    )
                }) {
                    insert_object_local_class(locals, value_var, class_name);
                }
                collect_object_local_classes_in_stmts(
                    body,
                    object_classes,
                    enum_cases,
                    function_return_object_classes,
                    locals,
                );
            }
            _ => {}
        }
    }
}

fn homogeneous_object_array_class_for_metadata_expr(
    expr: &Expr,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
    enum_cases: &HashMap<String, Vec<EnumCaseMetadata>>,
    function_return_object_classes: &HashMap<String, String>,
    locals: &HashMap<String, String>,
    current_class: Option<&str>,
) -> Option<String> {
    let classes = match &expr.kind {
        ExprKind::ArrayLiteral(items) => items
            .iter()
            .map(|item| {
                object_class_name_for_metadata_expr(
                    item,
                    object_classes,
                    enum_cases,
                    function_return_object_classes,
                    locals,
                    current_class,
                )
            })
            .collect::<Option<Vec<_>>>()?,
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .map(|(_, value)| {
                object_class_name_for_metadata_expr(
                    value,
                    object_classes,
                    enum_cases,
                    function_return_object_classes,
                    locals,
                    current_class,
                )
            })
            .collect::<Option<Vec<_>>>()?,
        _ => return None,
    };
    let first = classes.first()?.clone();
    classes
        .iter()
        .all(|class_name| class_name.eq_ignore_ascii_case(&first))
        .then_some(first)
}

fn insert_object_local_class(locals: &mut HashMap<String, String>, name: &str, class_name: String) {
    match locals.get(name) {
        Some(existing) if !existing.eq_ignore_ascii_case(&class_name) => {
            locals.remove(name);
        }
        _ => {
            locals.insert(name.to_string(), class_name);
        }
    }
}

fn enum_cases_class_name_for_metadata_expr(
    expr: &Expr,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
    enum_cases: &HashMap<String, Vec<EnumCaseMetadata>>,
    locals: &HashMap<String, String>,
    current_class: Option<&str>,
) -> Option<String> {
    let ExprKind::StaticMethodCall {
        receiver,
        method,
        args,
    } = &expr.kind
    else {
        return None;
    };
    if !method.eq_ignore_ascii_case("cases") || !args.is_empty() {
        return None;
    }
    let class_name =
        static_receiver_class_name_for_metadata(receiver, object_classes, locals, current_class)?;
    enum_cases
        .contains_key(&function_key(&class_name))
        .then_some(class_name)
}

fn object_expr_may_be_null(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::NullsafeMethodCall { .. }
            | ExprKind::NullsafePropertyAccess { .. }
            | ExprKind::NullsafeDynamicPropertyAccess { .. }
    )
}

fn collect_object_method_return_locals(
    stmts: &[Stmt],
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
    interface_names: &HashSet<String>,
    interface_parents: &HashMap<String, Vec<String>>,
    enum_cases: &HashMap<String, Vec<EnumCaseMetadata>>,
    function_return_object_classes: &HashMap<String, String>,
    function_return_kinds: &HashMap<String, ValueKind>,
    object_local_classes: &HashMap<String, String>,
    object_local_declared_types: &HashMap<String, String>,
    string_static_values: &HashMap<String, String>,
    current_class: Option<&str>,
) -> HashMap<String, LocalKind> {
    let mut locals = HashMap::new();
    collect_object_method_return_locals_in_stmts(
        stmts,
        object_classes,
        interface_names,
        interface_parents,
        enum_cases,
        function_return_object_classes,
        function_return_kinds,
        object_local_classes,
        object_local_declared_types,
        string_static_values,
        current_class,
        &mut locals,
    );
    locals
}

fn collect_scoped_constant_locals(
    stmts: &[Stmt],
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
    class_constants: &HashMap<String, ConstantValue>,
    current_class: &str,
) -> HashMap<String, LocalKind> {
    let mut locals = HashMap::new();
    collect_scoped_constant_locals_in_stmts(
        stmts,
        object_classes,
        class_constants,
        current_class,
        &mut locals,
    );
    locals
}

fn collect_scoped_constant_locals_in_stmts(
    stmts: &[Stmt],
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
    class_constants: &HashMap<String, ConstantValue>,
    current_class: &str,
    locals: &mut HashMap<String, LocalKind>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
                if let Some(kind) =
                    scoped_constant_local_kind(value, object_classes, class_constants, current_class)
                {
                    locals.insert(name.clone(), kind);
                }
            }
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                collect_scoped_constant_locals_in_stmts(
                    then_body,
                    object_classes,
                    class_constants,
                    current_class,
                    locals,
                );
                for (_, body) in elseif_clauses {
                    collect_scoped_constant_locals_in_stmts(
                        body,
                        object_classes,
                        class_constants,
                        current_class,
                        locals,
                    );
                }
                if let Some(body) = else_body {
                    collect_scoped_constant_locals_in_stmts(
                        body,
                        object_classes,
                        class_constants,
                        current_class,
                        locals,
                    );
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::NamespaceBlock { body, .. }
            | StmtKind::IncludeOnceGuard { body, .. } => collect_scoped_constant_locals_in_stmts(
                body,
                object_classes,
                class_constants,
                current_class,
                locals,
            ),
            StmtKind::For { init, update, body, .. } => {
                if let Some(init) = init {
                    collect_scoped_constant_locals_in_stmts(
                        std::slice::from_ref(init),
                        object_classes,
                        class_constants,
                        current_class,
                        locals,
                    );
                }
                collect_scoped_constant_locals_in_stmts(
                    body,
                    object_classes,
                    class_constants,
                    current_class,
                    locals,
                );
                if let Some(update) = update {
                    collect_scoped_constant_locals_in_stmts(
                        std::slice::from_ref(update),
                        object_classes,
                        class_constants,
                        current_class,
                        locals,
                    );
                }
            }
            _ => {}
        }
    }
}

fn scoped_constant_local_kind(
    expr: &Expr,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
    class_constants: &HashMap<String, ConstantValue>,
    current_class: &str,
) -> Option<LocalKind> {
    let ExprKind::ScopedConstantAccess { receiver, name } = &expr.kind else {
        return None;
    };
    let class_name = static_receiver_class_name_for_metadata(
        receiver,
        object_classes,
        &HashMap::new(),
        Some(current_class),
    )?;
    class_constants
        .get(&class_const_key(&class_name, name))
        .map(local_kind_for_metadata_constant)
}

fn local_kind_for_metadata_constant(value: &ConstantValue) -> LocalKind {
    match value {
        ConstantValue::Int(_) | ConstantValue::Null => LocalKind::I64,
        ConstantValue::Float(_) => LocalKind::F64,
        ConstantValue::Bool(_) => LocalKind::I32,
        ConstantValue::Str(_) => LocalKind::Str,
    }
}

fn collect_object_method_return_locals_in_stmts(
    stmts: &[Stmt],
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
    interface_names: &HashSet<String>,
    interface_parents: &HashMap<String, Vec<String>>,
    enum_cases: &HashMap<String, Vec<EnumCaseMetadata>>,
    function_return_object_classes: &HashMap<String, String>,
    function_return_kinds: &HashMap<String, ValueKind>,
    object_local_classes: &HashMap<String, String>,
    object_local_declared_types: &HashMap<String, String>,
    string_static_values: &HashMap<String, String>,
    current_class: Option<&str>,
    locals: &mut HashMap<String, LocalKind>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
                if let Some(kind) = object_access_local_kind(
                    value,
                    object_classes,
                    interface_names,
                    interface_parents,
                    enum_cases,
                    function_return_object_classes,
                    function_return_kinds,
                    object_local_classes,
                    object_local_declared_types,
                    string_static_values,
                    current_class,
                ) {
                    locals.insert(name.clone(), kind);
                }
            }
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                collect_object_method_return_locals_in_stmts(
                    then_body,
                    object_classes,
                    interface_names,
                    interface_parents,
                    enum_cases,
                    function_return_object_classes,
                    function_return_kinds,
                    object_local_classes,
                    object_local_declared_types,
                    string_static_values,
                    current_class,
                    locals,
                );
                for (_, body) in elseif_clauses {
                    collect_object_method_return_locals_in_stmts(
                        body,
                        object_classes,
                        interface_names,
                        interface_parents,
                        enum_cases,
                        function_return_object_classes,
                        function_return_kinds,
                        object_local_classes,
                        object_local_declared_types,
                        string_static_values,
                        current_class,
                        locals,
                    );
                }
                if let Some(body) = else_body {
                    collect_object_method_return_locals_in_stmts(
                        body,
                        object_classes,
                        interface_names,
                        interface_parents,
                        enum_cases,
                        function_return_object_classes,
                        function_return_kinds,
                        object_local_classes,
                        object_local_declared_types,
                        string_static_values,
                        current_class,
                        locals,
                    );
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::NamespaceBlock { body, .. }
            | StmtKind::IncludeOnceGuard { body, .. } => collect_object_method_return_locals_in_stmts(
                body,
                object_classes,
                interface_names,
                interface_parents,
                enum_cases,
                function_return_object_classes,
                function_return_kinds,
                object_local_classes,
                object_local_declared_types,
                string_static_values,
                current_class,
                locals,
            ),
            StmtKind::For { init, update, body, .. } => {
                if let Some(init) = init {
                    collect_object_method_return_locals_in_stmts(
                        std::slice::from_ref(init),
                        object_classes,
                        interface_names,
                        interface_parents,
                        enum_cases,
                        function_return_object_classes,
                        function_return_kinds,
                        object_local_classes,
                        object_local_declared_types,
                        string_static_values,
                        current_class,
                        locals,
                    );
                }
                collect_object_method_return_locals_in_stmts(
                    body,
                    object_classes,
                    interface_names,
                    interface_parents,
                    enum_cases,
                    function_return_object_classes,
                    function_return_kinds,
                    object_local_classes,
                    object_local_declared_types,
                    string_static_values,
                    current_class,
                    locals,
                );
                if let Some(update) = update {
                    collect_object_method_return_locals_in_stmts(
                        std::slice::from_ref(update),
                        object_classes,
                        interface_names,
                        interface_parents,
                        enum_cases,
                        function_return_object_classes,
                        function_return_kinds,
                        object_local_classes,
                        object_local_declared_types,
                        string_static_values,
                        current_class,
                        locals,
                    );
                }
            }
            _ => {}
        }
    }
}

fn object_access_local_kind(
    expr: &Expr,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
    interface_names: &HashSet<String>,
    interface_parents: &HashMap<String, Vec<String>>,
    enum_cases: &HashMap<String, Vec<EnumCaseMetadata>>,
    function_return_object_classes: &HashMap<String, String>,
    function_return_kinds: &HashMap<String, ValueKind>,
    object_local_classes: &HashMap<String, String>,
    object_local_declared_types: &HashMap<String, String>,
    string_static_values: &HashMap<String, String>,
    current_class: Option<&str>,
) -> Option<LocalKind> {
    match &expr.kind {
        ExprKind::NullsafeMethodCall { .. }
        | ExprKind::NullsafePropertyAccess { .. }
        | ExprKind::NullsafeDynamicPropertyAccess { .. } => Some(LocalKind::Mixed),
        ExprKind::MethodCall { object, method, .. } => {
            let class_name = object_class_name_for_metadata_expr(
                object,
                object_classes,
                enum_cases,
                function_return_object_classes,
                object_local_classes,
                current_class,
            )?;
            function_return_kinds
                .get(&method_call_return_key(&class_name, method))
                .copied()
                .map(local_kind_for_value)
        }
        ExprKind::ExprCall { callee, .. } => {
            let class_name = object_class_name_for_metadata_expr(
                callee,
                object_classes,
                enum_cases,
                function_return_object_classes,
                object_local_classes,
                current_class,
            )?;
            function_return_kinds
                .get(&method_call_return_key(&class_name, "__invoke"))
                .copied()
                .map(local_kind_for_value)
        }
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("call_user_func")
                || name.eq_ignore_ascii_case("call_user_func_array") =>
        {
            let callback = args.first()?;
            let class_name = object_class_name_for_metadata_expr(
                callback,
                object_classes,
                enum_cases,
                function_return_object_classes,
                object_local_classes,
                current_class,
            )?;
            function_return_kinds
                .get(&method_call_return_key(&class_name, "__invoke"))
                .copied()
                .map(local_kind_for_value)
        }
        ExprKind::StaticMethodCall {
            receiver, method, ..
        } => {
            let class_name =
                static_receiver_class_name_for_metadata(receiver, object_classes, object_local_classes, current_class)?;
            function_return_kinds
                .get(&static_method_call_return_key(&class_name, method))
                .copied()
                .map(local_kind_for_value)
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            let class_name =
                static_receiver_class_name_for_metadata(receiver, object_classes, object_local_classes, current_class)?;
            object_classes
                .get(&function_key(&class_name))?
                .static_properties
                .iter()
                .find(|candidate| candidate.name.eq_ignore_ascii_case(property))
                .map(|property_info| local_kind_for_object_property(property_info.kind))
        }
        ExprKind::PropertyAccess { object, property } => {
            let class_name = object_class_name_for_metadata_expr(
                object,
                object_classes,
                enum_cases,
                function_return_object_classes,
                object_local_classes,
                current_class,
            )?;
            object_classes
                .get(&function_key(&class_name))?
                .properties
                .iter()
                .find(|candidate| candidate.name == *property)
                .map(|property_info| local_kind_for_object_property(property_info.kind))
        }
        ExprKind::DynamicPropertyAccess { object, property } => {
            dynamic_object_property_local_kind_for_metadata(
                object,
                object_dynamic_property_name_for_metadata(property, string_static_values).as_deref(),
                object_classes,
                interface_names,
                interface_parents,
                object_local_declared_types,
            )
        }
        _ => None,
    }
}

fn object_dynamic_property_name_for_metadata(
    expr: &Expr,
    string_static_values: &HashMap<String, String>,
) -> Option<String> {
    match &expr.kind {
        ExprKind::StringLiteral(value) => Some(value.clone()),
        ExprKind::Variable(name) => string_static_values.get(name).cloned(),
        _ => None,
    }
}

fn dynamic_object_property_local_kind_for_metadata(
    object: &Expr,
    property: Option<&str>,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
    interface_names: &HashSet<String>,
    interface_parents: &HashMap<String, Vec<String>>,
    object_local_declared_types: &HashMap<String, String>,
) -> Option<LocalKind> {
    let receiver_type = object_declared_type_for_metadata_expr(object, object_local_declared_types)?;
    let mut kind = None;
    let mut found = false;
    for class_info in object_classes.values() {
        if !metadata_class_matches_declared_receiver(
            &class_info.name,
            &receiver_type,
            object_classes,
            interface_names,
            interface_parents,
        ) {
            continue;
        }
        for property_info in &class_info.properties {
            if property.is_some_and(|name| !property_info.name.eq_ignore_ascii_case(name)) {
                continue;
            }
            if kind.is_some_and(|existing| existing != property_info.kind) {
                return None;
            }
            kind = Some(property_info.kind);
            found = true;
        }
    }
    found.then(|| local_kind_for_object_property(kind.expect("found property must set kind")))
}

fn object_declared_type_for_metadata_expr(
    expr: &Expr,
    object_local_declared_types: &HashMap<String, String>,
) -> Option<String> {
    match &expr.kind {
        ExprKind::This => object_local_declared_types.get("this").cloned(),
        ExprKind::Variable(name) => object_local_declared_types.get(name).cloned(),
        _ => None,
    }
}

fn metadata_class_matches_declared_receiver(
    class_name: &str,
    receiver_type: &str,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
    interface_names: &HashSet<String>,
    interface_parents: &HashMap<String, Vec<String>>,
) -> bool {
    let receiver_key = function_key(receiver_type);
    if interface_names.contains(&receiver_key) {
        return metadata_class_implements_interface_or_parent(
            class_name,
            &receiver_key,
            object_classes,
            interface_parents,
        );
    }
    metadata_class_extends_or_equals(class_name, &receiver_key, object_classes)
}

fn metadata_class_implements_interface_or_parent(
    class_name: &str,
    target_key: &str,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
    interface_parents: &HashMap<String, Vec<String>>,
) -> bool {
    let mut current = Some(function_key(class_name));
    while let Some(class_key) = current {
        let Some(class_info) = object_classes.get(&class_key) else {
            return false;
        };
        if class_info.interfaces.iter().any(|interface| {
            interface == target_key || metadata_interface_extends(interface, target_key, interface_parents)
        }) {
            return true;
        }
        current = class_info.parent.clone();
    }
    false
}

fn metadata_class_extends_or_equals(
    class_name: &str,
    target_key: &str,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
) -> bool {
    let mut current = Some(function_key(class_name));
    while let Some(class_key) = current {
        let Some(class_info) = object_classes.get(&class_key) else {
            return false;
        };
        if function_key(&class_info.name) == target_key {
            return true;
        }
        current = class_info.parent.clone();
    }
    false
}

fn metadata_interface_extends(
    interface: &str,
    target_key: &str,
    interface_parents: &HashMap<String, Vec<String>>,
) -> bool {
    let mut stack = vec![function_key(interface)];
    let mut seen = HashSet::new();
    while let Some(current) = stack.pop() {
        if !seen.insert(current.clone()) {
            continue;
        }
        let Some(parents) = interface_parents.get(&current) else {
            continue;
        };
        for parent in parents {
            if parent == target_key {
                return true;
            }
            stack.push(parent.clone());
        }
    }
    false
}

fn local_kind_for_object_property(kind: ObjectPropertyKind) -> LocalKind {
    match kind {
        ObjectPropertyKind::Int => LocalKind::I64,
        ObjectPropertyKind::Float => LocalKind::F64,
        ObjectPropertyKind::Bool => LocalKind::I32,
        ObjectPropertyKind::Str => LocalKind::Str,
        ObjectPropertyKind::Object => LocalKind::Object,
        ObjectPropertyKind::Mixed => LocalKind::Mixed,
    }
}

fn object_class_name_for_metadata_expr(
    expr: &Expr,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
    enum_cases: &HashMap<String, Vec<EnumCaseMetadata>>,
    function_return_object_classes: &HashMap<String, String>,
    locals: &HashMap<String, String>,
    current_class: Option<&str>,
) -> Option<String> {
    let class_name = match &expr.kind {
        ExprKind::NewObject { class_name, .. } => Some(class_name.as_str().to_string()),
        ExprKind::This => locals.get("this").cloned(),
        ExprKind::Variable(name) => locals.get(name).cloned(),
        ExprKind::FunctionCall { name, .. } => {
            function_return_object_classes.get(&function_key(name)).cloned()
        }
        ExprKind::MethodCall { object, method, .. }
        | ExprKind::NullsafeMethodCall { object, method, .. } => {
            let receiver_class = object_class_name_for_metadata_expr(
                object,
                object_classes,
                enum_cases,
                function_return_object_classes,
                locals,
                current_class,
            )?;
            function_return_object_classes
                .get(&method_call_return_key(&receiver_class, method))
                .cloned()
        }
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => {
            let class_name =
                static_receiver_class_name_for_metadata(receiver, object_classes, locals, current_class)?;
            if metadata_expr_is_matching_backed_enum_lookup(method, args, &class_name, enum_cases) {
                return Some(class_name);
            }
            function_return_object_classes
                .get(&static_method_call_return_key(&class_name, method))
                .cloned()
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            let class_name =
                static_receiver_class_name_for_metadata(receiver, object_classes, locals, current_class)?;
            object_classes
                .get(&function_key(&class_name))?
                .static_properties
                .iter()
                .find(|candidate| candidate.name.eq_ignore_ascii_case(property))
                .and_then(|property_info| property_info.declared_class.clone())
        }
        ExprKind::ScopedConstantAccess { receiver, name } => {
            let class_name =
                static_receiver_class_name_for_metadata(receiver, object_classes, locals, current_class)?;
        let cases = enum_cases.get(&function_key(&class_name))?;
        cases
            .iter()
            .any(|case| case.name.eq_ignore_ascii_case(name))
            .then_some(class_name)
    }
        ExprKind::PropertyAccess { object, property }
        | ExprKind::NullsafePropertyAccess { object, property } => {
            let class_name = object_class_name_for_metadata_expr(
                object,
                object_classes,
                enum_cases,
                function_return_object_classes,
                locals,
                current_class,
            )?;
            object_classes
                .get(&function_key(&class_name))?
                .properties
                .iter()
                .find(|candidate| candidate.name == *property)
                .and_then(|property_info| property_info.declared_class.clone())
        }
        _ => None,
    }?;
    object_classes
        .get(&function_key(&class_name))
        .map(|class_info| class_info.name.clone())
}

fn metadata_expr_is_matching_backed_enum_lookup(
    method: &str,
    args: &[Expr],
    class_name: &str,
    enum_cases: &HashMap<String, Vec<EnumCaseMetadata>>,
) -> bool {
    if !method.eq_ignore_ascii_case("from") && !method.eq_ignore_ascii_case("tryFrom") {
        return false;
    }
    let Some(value) = args.first().and_then(metadata_enum_lookup_value) else {
        return false;
    };
    enum_cases
        .get(&function_key(class_name))
        .is_some_and(|cases| cases.iter().any(|case| case.value.as_ref() == Some(&value)))
}

fn metadata_enum_lookup_value(expr: &Expr) -> Option<EnumCaseBackingValue> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(EnumCaseBackingValue::Int(*value)),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => Some(EnumCaseBackingValue::Int(-*value)),
            _ => None,
        },
        ExprKind::StringLiteral(value) => Some(EnumCaseBackingValue::Str(value.clone())),
        _ => None,
    }
}

fn static_receiver_class_name_for_metadata(
    receiver: &StaticReceiver,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
    locals: &HashMap<String, String>,
    current_class: Option<&str>,
) -> Option<String> {
    match receiver {
        StaticReceiver::Named(class_name) => object_classes
            .get(&function_key(class_name.as_str()))
            .map(|class_info| class_info.name.clone()),
        StaticReceiver::Self_ | StaticReceiver::Static => {
            let class_name = locals.get("this").map(String::as_str).or(current_class)?;
            object_classes
                .get(&function_key(class_name))
                .map(|class_info| class_info.name.clone())
        }
        StaticReceiver::Parent => {
            let class_name = locals.get("this").map(String::as_str).or(current_class)?;
            let parent_key = object_classes.get(&function_key(class_name))?.parent.clone()?;
            object_classes
                .get(&parent_key)
                .map(|class_info| class_info.name.clone())
        }
    }
}

fn object_class_name_for_type_expr(
    type_expr: &TypeExpr,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
) -> Option<String> {
    match type_expr {
        TypeExpr::Named(name) => object_classes
            .get(&function_key(name.as_str()))
            .map(|class_info| class_info.name.clone()),
        TypeExpr::Nullable(inner) => object_class_name_for_type_expr(inner, object_classes),
        _ => None,
    }
}

fn object_declared_type_name_for_type_expr(type_expr: &TypeExpr) -> Option<String> {
    match type_expr {
        TypeExpr::Named(name) => Some(name.as_str().to_string()),
        TypeExpr::Nullable(inner) => object_declared_type_name_for_type_expr(inner),
        _ => None,
    }
}

fn align_to(value: usize, alignment: usize) -> usize {
    debug_assert!(alignment.is_power_of_two());
    (value + alignment - 1) & !(alignment - 1)
}

fn add_object_method_function_metadata(
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
    mut function_params: HashMap<String, Vec<String>>,
    mut function_param_kinds: HashMap<String, Vec<LocalKind>>,
    mut function_defaults: HashMap<String, Vec<Option<Expr>>>,
    mut function_return_kinds: HashMap<String, ValueKind>,
    mut function_return_object_classes: HashMap<String, String>,
    mut nullable_function_returns: HashSet<String>,
) -> (
    HashMap<String, Vec<String>>,
    HashMap<String, Vec<LocalKind>>,
    HashMap<String, Vec<Option<Expr>>>,
    HashMap<String, ValueKind>,
    HashMap<String, String>,
    HashSet<String>,
) {
    for class_info in object_classes.values() {
        for method in class_info
            .constructor
            .iter()
            .chain(class_info.methods.iter())
            .chain(class_info.static_methods.iter())
        {
            let key = function_key(&method.symbol);
            let mut params = if method.has_this {
                vec!["this".to_string()]
            } else {
                Vec::new()
            };
            params.extend(method.params.iter().map(|(name, _)| name.clone()));
            let mut param_kinds = if method.has_this {
                vec![LocalKind::Object]
            } else {
                Vec::new()
            };
            param_kinds.extend(method.param_kinds.iter().copied());
            let mut defaults = if method.has_this {
                vec![None]
            } else {
                Vec::new()
            };
            defaults.extend(method.defaults.iter().cloned());
            function_params.insert(key.clone(), params);
            function_param_kinds.insert(key.clone(), param_kinds);
            function_defaults.insert(key.clone(), defaults);
            if let Some(class_name) = &method.return_object_class {
                function_return_object_classes.insert(key.clone(), class_name.clone());
            }
            nullable_function_returns.remove(&key);
            function_return_kinds.insert(key, method.return_kind);
        }
    }
    for class_info in object_classes.values() {
        let mut current = Some(function_key(&class_info.name));
        while let Some(class_key) = current {
            let Some(source_class) = object_classes.get(&class_key) else {
                break;
            };
            for method in &source_class.methods {
                function_return_kinds.insert(
                    method_call_return_key(&class_info.name, &method.name),
                    method.return_kind,
                );
                if let Some(class_name) = &method.return_object_class {
                    function_return_object_classes
                        .entry(method_call_return_key(&class_info.name, &method.name))
                        .or_insert_with(|| class_name.clone());
                }
            }
            for method in &source_class.static_methods {
                function_return_kinds.insert(
                    static_method_call_return_key(&class_info.name, &method.name),
                    method.return_kind,
                );
                if let Some(class_name) = &method.return_object_class {
                    function_return_object_classes
                        .entry(static_method_call_return_key(&class_info.name, &method.name))
                        .or_insert_with(|| class_name.clone());
                }
            }
            current = source_class.parent.clone();
        }
    }
    (
        function_params,
        function_param_kinds,
        function_defaults,
        function_return_kinds,
        function_return_object_classes,
        nullable_function_returns,
    )
}

fn add_object_method_array_return_metadata_aliases<T: Clone>(
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
    metadata: &mut HashMap<String, T>,
) {
    for class_info in object_classes.values() {
        let mut current = Some(function_key(&class_info.name));
        while let Some(class_key) = current {
            let Some(source_class) = object_classes.get(&class_key) else {
                break;
            };
            for method in &source_class.methods {
                let source_key = function_key(&method.symbol);
                if let Some(value) = metadata.get(&source_key).cloned() {
                    metadata.insert(method_call_return_key(&class_info.name, &method.name), value);
                }
            }
            for method in &source_class.static_methods {
                let source_key = function_key(&method.symbol);
                if let Some(value) = metadata.get(&source_key).cloned() {
                    metadata.insert(
                        static_method_call_return_key(&class_info.name, &method.name),
                        value,
                    );
                }
            }
            current = source_class.parent.clone();
        }
    }
}

struct LoopLabels {
    continue_label: String,
}

pub(super) struct FunctionBody {
    name: Option<String>,
    params: Vec<String>,
    param_kinds: Vec<LocalKind>,
    return_kind: ValueKind,
    locals: BTreeMap<String, LocalKind>,
    body: WatEmitter,
    array_lengths: HashMap<String, usize>,
    array_layouts: HashMap<String, ArrayLayout>,
    array_value_kinds: HashMap<String, Vec<ValueCellKind>>,
    array_object_classes: HashMap<String, Vec<Option<String>>>,
    array_value_constants: HashMap<String, Vec<ConstantValue>>,
    array_runtime_value_kind: HashMap<String, ValueCellKind>,
    array_nested_values: HashMap<String, Vec<Option<NestedArrayMetadata>>>,
    array_runtime_nested_value: HashMap<String, NestedArrayMetadata>,
    array_key_kinds: HashMap<String, Vec<AssocKeyKind>>,
    array_key_values: HashMap<String, Vec<AssocKeyValue>>,
    array_runtime_key_kind: HashMap<String, AssocKeyKind>,
    array_php_normalized_runtime_keys: HashSet<String>,
    mixed_value_kinds: HashMap<String, ValueCellKind>,
    i64_static_values: HashMap<String, i64>,
    f64_static_values: HashMap<String, f64>,
    bool_static_values: HashMap<String, bool>,
    string_static_values: HashMap<String, String>,
    possible_static_string_values: HashMap<String, Vec<String>>,
    callable_targets: HashMap<String, String>,
    callable_instance_targets: HashMap<String, (String, String)>,
    object_local_classes: HashMap<String, String>,
    object_local_declared_types: HashMap<String, String>,
    current_class: Option<String>,
    late_static_class: Option<String>,
    i64_ref_aliases: HashMap<String, String>,
    i64_ref_alias_refreshes: HashSet<String>,
    f64_ref_aliases: HashMap<String, String>,
    f64_ref_alias_refreshes: HashSet<String>,
    i32_ref_aliases: HashMap<String, String>,
    i32_ref_alias_refreshes: HashSet<String>,
    str_ref_aliases: HashMap<String, String>,
    str_ref_alias_refreshes: HashSet<String>,
    array_ref_aliases: HashMap<String, String>,
    array_ref_alias_sources: HashMap<String, String>,
    array_ref_alias_refreshes: HashSet<String>,
    mixed_ref_aliases: HashMap<String, String>,
    mixed_ref_alias_refreshes: HashSet<String>,
}

impl FunctionBody {
    fn main() -> Self {
        Self {
            name: None,
            params: Vec::new(),
            param_kinds: Vec::new(),
            return_kind: ValueKind::Int,
            locals: BTreeMap::new(),
            body: WatEmitter::new(),
            array_lengths: HashMap::new(),
            array_layouts: HashMap::new(),
            array_value_kinds: HashMap::new(),
            array_object_classes: HashMap::new(),
            array_value_constants: HashMap::new(),
            array_runtime_value_kind: HashMap::new(),
            array_nested_values: HashMap::new(),
            array_runtime_nested_value: HashMap::new(),
            array_key_kinds: HashMap::new(),
            array_key_values: HashMap::new(),
            array_runtime_key_kind: HashMap::new(),
            array_php_normalized_runtime_keys: HashSet::new(),
            mixed_value_kinds: HashMap::new(),
            i64_static_values: HashMap::new(),
            f64_static_values: HashMap::new(),
            bool_static_values: HashMap::new(),
            string_static_values: HashMap::new(),
            possible_static_string_values: HashMap::new(),
            callable_targets: HashMap::new(),
            callable_instance_targets: HashMap::new(),
            object_local_classes: HashMap::new(),
            object_local_declared_types: HashMap::new(),
            current_class: None,
            late_static_class: None,
            i64_ref_aliases: HashMap::new(),
            i64_ref_alias_refreshes: HashSet::new(),
            f64_ref_aliases: HashMap::new(),
            f64_ref_alias_refreshes: HashSet::new(),
            i32_ref_aliases: HashMap::new(),
            i32_ref_alias_refreshes: HashSet::new(),
            str_ref_aliases: HashMap::new(),
            str_ref_alias_refreshes: HashSet::new(),
            array_ref_aliases: HashMap::new(),
            array_ref_alias_sources: HashMap::new(),
            array_ref_alias_refreshes: HashSet::new(),
            mixed_ref_aliases: HashMap::new(),
            mixed_ref_alias_refreshes: HashSet::new(),
        }
    }

    fn function(
        name: String,
        params: Vec<String>,
        param_kinds: Vec<LocalKind>,
        return_kind: ValueKind,
    ) -> Self {
        let mut locals = BTreeMap::new();
        let mut array_layouts = HashMap::new();
        for (param, kind) in params.iter().zip(param_kinds.iter().copied()) {
            locals.insert(param.clone(), kind);
            if kind == LocalKind::Array {
                array_layouts.insert(param.clone(), ArrayLayout::Value);
            }
        }
        Self {
            name: Some(name),
            params,
            param_kinds,
            return_kind,
            locals,
            body: WatEmitter::new(),
            array_lengths: HashMap::new(),
            array_layouts,
            array_value_kinds: HashMap::new(),
            array_object_classes: HashMap::new(),
            array_value_constants: HashMap::new(),
            array_runtime_value_kind: HashMap::new(),
            array_nested_values: HashMap::new(),
            array_runtime_nested_value: HashMap::new(),
            array_key_kinds: HashMap::new(),
            array_key_values: HashMap::new(),
            array_runtime_key_kind: HashMap::new(),
            array_php_normalized_runtime_keys: HashSet::new(),
            mixed_value_kinds: HashMap::new(),
            i64_static_values: HashMap::new(),
            f64_static_values: HashMap::new(),
            bool_static_values: HashMap::new(),
            string_static_values: HashMap::new(),
            possible_static_string_values: HashMap::new(),
            callable_targets: HashMap::new(),
            callable_instance_targets: HashMap::new(),
            object_local_classes: HashMap::new(),
            object_local_declared_types: HashMap::new(),
            current_class: None,
            late_static_class: None,
            i64_ref_aliases: HashMap::new(),
            i64_ref_alias_refreshes: HashSet::new(),
            f64_ref_aliases: HashMap::new(),
            f64_ref_alias_refreshes: HashSet::new(),
            i32_ref_aliases: HashMap::new(),
            i32_ref_alias_refreshes: HashSet::new(),
            str_ref_aliases: HashMap::new(),
            str_ref_alias_refreshes: HashSet::new(),
            array_ref_aliases: HashMap::new(),
            array_ref_alias_sources: HashMap::new(),
            array_ref_alias_refreshes: HashSet::new(),
            mixed_ref_aliases: HashMap::new(),
            mixed_ref_alias_refreshes: HashSet::new(),
        }
    }
}

fn emit_function(out: &mut WatEmitter, function: FunctionBody) {
    match &function.name {
        Some(name) => {
            let params = function
                .params
                .iter()
                .zip(function.param_kinds.iter())
                .map(|(param, kind)| match kind {
                    LocalKind::Str | LocalKind::Array => {
                        format!(" (param ${}_ptr i32) (param ${}_len i32)", param, param)
                    }
                    _ => format!(" (param ${} {})", param, wasm_local_type(*kind)),
                })
                .collect::<String>();
            if matches!(function.return_kind, ValueKind::Null | ValueKind::Never) {
                out.open(&format!("(func ${}{}", wasm_function_name(name), params));
            } else {
                out.open(&format!(
                    "(func ${}{} (result {})",
                    wasm_function_name(name),
                    params,
                    wasm_value_type(function.return_kind)
                ));
            }
        }
        None => out.open("(func (export \"main\")"),
    }
    let mut mixed_locals = Vec::new();
    let param_kind_by_name = function
        .params
        .iter()
        .cloned()
        .zip(function.param_kinds.iter().copied())
        .collect::<HashMap<_, _>>();
    for (name, kind) in function.locals {
        if let Some(param_kind) = param_kind_by_name.get(&name) {
            if *param_kind == kind
                || (!matches!(param_kind, LocalKind::Str | LocalKind::Array)
                    && !matches!(kind, LocalKind::Str | LocalKind::Array))
            {
                continue;
            }
        }
        match kind {
            LocalKind::I64 => out.line(&format!("(local ${} i64)", name)),
            LocalKind::F64 => out.line(&format!("(local ${} f64)", name)),
            LocalKind::I32 => out.line(&format!("(local ${} i32)", name)),
            LocalKind::Str => {
                out.line(&format!("(local ${}_ptr i32)", name));
                out.line(&format!("(local ${}_len i32)", name));
            }
            LocalKind::Array => {
                out.line(&format!("(local ${}_ptr i32)", name));
                out.line(&format!("(local ${}_len i32)", name));
            }
            LocalKind::Object => out.line(&format!("(local ${} i32)", name)),
            LocalKind::Mixed => {
                out.line(&format!("(local ${} i32)", name));
                mixed_locals.push(name);
            }
            LocalKind::Callable => {}
        }
    }
    for name in mixed_locals {
        out.line("call $__rt_alloc_null_mixed_cell");
        out.line(&format!("local.set ${}", name));
    }
    for line in function.body.finish().lines() {
        out.line(line);
    }
    if function.name.is_some() {
        emit_default_result(out, function.return_kind);
    }
    out.close(")");
}

fn body_guarantees_return(body: &[Stmt]) -> bool {
    body.last().is_some_and(stmt_guarantees_return)
}

fn stmt_guarantees_return(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Return(Some(_)) => true,
        StmtKind::Return(None) => false,
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            body_guarantees_return(then_body)
                && elseif_clauses
                    .iter()
                    .all(|(_, body)| body_guarantees_return(body))
                && else_body.as_deref().is_some_and(body_guarantees_return)
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            body_guarantees_return(stmts)
        }
        _ => false,
    }
}

pub(super) fn wasm_value_type(kind: ValueKind) -> &'static str {
    match kind {
        ValueKind::Int => "i64",
        ValueKind::Float => "f64",
        ValueKind::Bool => "i32",
        ValueKind::Mixed => "i32",
        ValueKind::Object => "i32",
        ValueKind::Str | ValueKind::Array => "i32 i32",
        ValueKind::Null | ValueKind::Never => unreachable!("unsupported function result kind"),
    }
}

fn wasm_local_type(kind: LocalKind) -> &'static str {
    match kind {
        LocalKind::I64 => "i64",
        LocalKind::F64 => "f64",
        LocalKind::I32 => "i32",
        LocalKind::Mixed => "i32",
        LocalKind::Object => "i32",
        LocalKind::Callable => "i32",
        LocalKind::Str | LocalKind::Array => {
            unreachable!("aggregate params need specialized ABI")
        }
    }
}

fn emit_default_result(out: &mut WatEmitter, kind: ValueKind) {
    match kind {
        ValueKind::Int => out.line("i64.const 0"),
        ValueKind::Float => out.line("f64.const 0"),
        ValueKind::Bool => out.line("i32.const 0"),
        ValueKind::Mixed => {
            out.line("call $__rt_alloc_null_mixed_cell");
            out.line("return");
        }
        ValueKind::Object => out.line("i32.const 0"),
        ValueKind::Str | ValueKind::Array => {
            out.line("i32.const 0");
            out.line("i32.const 0");
        }
        ValueKind::Null => {}
        ValueKind::Never => out.line("unreachable"),
    }
}

pub(super) fn wasm_function_name(name: &str) -> String {
    format!("fn_{}", crate::names::mangle_fqn(&function_key(name)))
}

pub(super) fn static_method_call_return_key(class_name: &str, method_name: &str) -> String {
    format!("{}::{}", function_key(class_name), function_key(method_name))
}

pub(super) fn method_call_return_key(class_name: &str, method_name: &str) -> String {
    format!("{}->{}", function_key(class_name), function_key(method_name))
}

fn function_key(name: &str) -> String {
    name.to_ascii_lowercase()
}
