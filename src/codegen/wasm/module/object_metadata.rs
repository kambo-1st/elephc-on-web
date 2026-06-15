//! Purpose:
//! Collects minimal class/object metadata needed by wasm32-web object lowering.
//! Mirrors the native fixed object payload layout without depending on native codegen state.
//!
//! Called from:
//! - `crate::codegen::wasm::module::WasmModule::new()`.
//!
//! Key details:
//! - Object payloads start with a 64-bit class id followed by fixed-width property slots.
//! - Constructors, inheritance flattening, static property globals, dynamic properties, and dynamic
//!   methods stay explicit metadata decisions so unsupported paths fail before code emission.

use std::collections::{HashMap, HashSet};

use crate::names::Name;
use crate::parser::ast::{
    AttributeGroup, ClassMethod, ClassProperty, Expr, ExprKind, Program, StaticReceiver, Stmt,
    StmtKind, TraitAdaptation, TraitUse, TypeExpr, Visibility,
};
use crate::types::AttrArgValue;

use super::{
    function_key, local_kind_from_type, value_kind_from_return_type, LocalKind, ValueCellKind,
    ValueKind,
};
use super::function_metadata::consistent_mixed_return_value_kind;

#[derive(Clone, Debug)]
pub(in crate::codegen::wasm) struct ObjectClassInfo {
    pub(in crate::codegen::wasm) name: String,
    pub(in crate::codegen::wasm) class_id: u64,
    pub(in crate::codegen::wasm) parent: Option<String>,
    pub(in crate::codegen::wasm) interfaces: Vec<String>,
    pub(in crate::codegen::wasm) used_traits: Vec<String>,
    pub(in crate::codegen::wasm) attribute_names: Vec<String>,
    pub(in crate::codegen::wasm) attribute_args: Vec<Option<Vec<AttrArgValue>>>,
    pub(in crate::codegen::wasm) has_constructor: bool,
    pub(in crate::codegen::wasm) properties: Vec<ObjectPropertyInfo>,
    pub(in crate::codegen::wasm) static_properties: Vec<ObjectStaticPropertyInfo>,
    pub(in crate::codegen::wasm) constructor: Option<ObjectMethodInfo>,
    pub(in crate::codegen::wasm) methods: Vec<ObjectMethodInfo>,
    pub(in crate::codegen::wasm) static_methods: Vec<ObjectMethodInfo>,
}

#[derive(Clone, Debug)]
pub(in crate::codegen::wasm) struct ObjectPropertyInfo {
    pub(in crate::codegen::wasm) name: String,
    pub(in crate::codegen::wasm) offset: usize,
    pub(in crate::codegen::wasm) initialized_offset: Option<usize>,
    pub(in crate::codegen::wasm) kind: ObjectPropertyKind,
    pub(in crate::codegen::wasm) visibility: Visibility,
    pub(in crate::codegen::wasm) owner_class: String,
    pub(in crate::codegen::wasm) declared_class: Option<String>,
    pub(in crate::codegen::wasm) default: Option<Expr>,
}

#[derive(Clone, Debug)]
pub(in crate::codegen::wasm) struct ObjectStaticPropertyInfo {
    pub(in crate::codegen::wasm) name: String,
    pub(in crate::codegen::wasm) kind: ObjectPropertyKind,
    pub(in crate::codegen::wasm) visibility: Visibility,
    pub(in crate::codegen::wasm) owner_class: String,
    pub(in crate::codegen::wasm) declared_class: Option<String>,
    pub(in crate::codegen::wasm) global: String,
    pub(in crate::codegen::wasm) needs_initialized_guard: bool,
    pub(in crate::codegen::wasm) default: Option<Expr>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::codegen::wasm) enum ObjectPropertyKind {
    Int,
    Float,
    Bool,
    Str,
    Object,
    Mixed,
}

#[derive(Clone, Debug)]
pub(in crate::codegen::wasm) struct ObjectMethodInfo {
    pub(in crate::codegen::wasm) name: String,
    pub(in crate::codegen::wasm) symbol: String,
    pub(in crate::codegen::wasm) visibility: Visibility,
    pub(in crate::codegen::wasm) owner_class: String,
    pub(in crate::codegen::wasm) has_this: bool,
    pub(in crate::codegen::wasm) params: Vec<(String, Option<TypeExpr>)>,
    pub(in crate::codegen::wasm) param_kinds: Vec<LocalKind>,
    pub(in crate::codegen::wasm) defaults: Vec<Option<Expr>>,
    pub(in crate::codegen::wasm) return_kind: ValueKind,
    pub(in crate::codegen::wasm) mixed_return_kind: Option<ValueCellKind>,
    pub(in crate::codegen::wasm) return_object_class: Option<String>,
    pub(in crate::codegen::wasm) body: Vec<Stmt>,
}

pub(super) fn collect_object_classes(program: &Program) -> HashMap<String, ObjectClassInfo> {
    let traits = collect_simple_traits(program);
    let mut classes = HashMap::new();
    let mut next_class_id = 1u64;
    for stmt in program {
        let StmtKind::ClassDecl {
            name,
            extends,
            implements,
            trait_uses,
            properties,
            methods,
            ..
        } = &stmt.kind else {
            continue;
        };
        let simple_trait_members = simple_trait_members(trait_uses, &traits);
        let has_supported_traits = simple_trait_members.is_some();
        let trait_methods = simple_trait_members
            .as_ref()
            .map(|(_, trait_methods)| trait_methods.as_slice())
            .unwrap_or(&[]);
        let has_constructor = methods
            .iter()
            .chain(trait_methods.iter())
            .any(|method| method.name.eq_ignore_ascii_case("__construct"));
        let inherited_properties = inherited_properties(extends, has_supported_traits, &classes);
        let mut wasm_properties = inherited_properties.clone().unwrap_or_default();
        let mut wasm_static_properties =
            inherited_static_properties(extends, has_supported_traits, &classes).unwrap_or_default();
        if has_supported_traits && (extends.is_none() || inherited_properties.is_some()) {
            if let Some((trait_properties, _)) = &simple_trait_members {
                append_own_properties(name, &mut wasm_properties, trait_properties);
                append_static_properties(name, &mut wasm_static_properties, trait_properties);
            }
            append_own_properties(name, &mut wasm_properties, properties);
            append_static_properties(name, &mut wasm_static_properties, properties);
        }
        let constructor = if has_supported_traits && (extends.is_none() || inherited_properties.is_some()) {
            methods
                .iter()
                .chain(trait_methods.iter())
                .find(|method| method.name.eq_ignore_ascii_case("__construct"))
                .filter(|method| supported_object_method(method))
                .map(|method| object_method_info(name, method, true, method_symbol))
        } else {
            None
        };
        let mut wasm_methods = if has_supported_traits {
            methods
                .iter()
                .chain(
                    simple_trait_members
                        .as_ref()
                        .into_iter()
                        .flat_map(|(_, trait_methods)| trait_methods.iter()),
                )
                .filter(|method| supported_object_method(method))
                .filter(|method| !method.name.eq_ignore_ascii_case("__construct"))
                .map(|method| object_method_info(name, method, true, method_symbol))
                .collect()
        } else {
            Vec::new()
        };
        append_inherited_methods(name, extends, &mut wasm_methods, &classes);
        let mut wasm_static_methods = if has_supported_traits {
            methods
                .iter()
                .chain(
                    simple_trait_members
                        .as_ref()
                        .into_iter()
                        .flat_map(|(_, trait_methods)| trait_methods.iter()),
                )
                .filter(|method| supported_static_object_method(method))
                .map(|method| object_method_info(name, method, false, static_method_symbol))
                .collect()
        } else {
            Vec::new()
        };
        append_inherited_static_methods(name, extends, &mut wasm_static_methods, &classes);
        classes.insert(
            name.to_ascii_lowercase(),
            ObjectClassInfo {
                name: name.as_str().to_string(),
                class_id: next_class_id,
                parent: extends.as_ref().map(|name| name.as_str().to_ascii_lowercase()),
                interfaces: implements
                    .iter()
                    .map(|name| name.as_str().to_ascii_lowercase())
                    .collect(),
                used_traits: trait_uses
                    .iter()
                    .flat_map(|use_decl| {
                        use_decl
                            .trait_names
                            .iter()
                            .map(|name| name.as_str().to_string())
                    })
                    .collect(),
                attribute_names: collect_attribute_names(&stmt.attributes),
                attribute_args: collect_attribute_args(&stmt.attributes),
                has_constructor,
                properties: wasm_properties,
                static_properties: wasm_static_properties,
                constructor,
                methods: wasm_methods,
                static_methods: wasm_static_methods,
            },
        );
        next_class_id += 1;
    }
    for stmt in program {
        let StmtKind::EnumDecl {
            name,
            backing_type,
            ..
        } = &stmt.kind else {
            continue;
        };
        let properties = backing_type
            .as_ref()
            .and_then(kind_for_type_expr)
            .map(|kind| {
                vec![ObjectPropertyInfo {
                    name: "value".to_string(),
                    offset: 8,
                    initialized_offset: None,
                    kind,
                    visibility: Visibility::Public,
                    owner_class: name.clone(),
                    declared_class: None,
                    default: None,
                }]
            })
            .unwrap_or_default();
        classes.insert(
            name.to_ascii_lowercase(),
            ObjectClassInfo {
                name: name.clone(),
                class_id: next_class_id,
                parent: None,
                interfaces: Vec::new(),
                used_traits: Vec::new(),
                attribute_names: Vec::new(),
                attribute_args: Vec::new(),
                has_constructor: false,
                properties,
                static_properties: Vec::new(),
                constructor: None,
                methods: Vec::new(),
                static_methods: Vec::new(),
            },
        );
        next_class_id += 1;
    }
    classes
}

fn collect_attribute_names(groups: &[AttributeGroup]) -> Vec<String> {
    groups
        .iter()
        .flat_map(|group| group.attributes.iter())
        .map(|attr| attr.name.as_str().to_string())
        .collect()
}

fn collect_attribute_args(groups: &[AttributeGroup]) -> Vec<Option<Vec<AttrArgValue>>> {
    groups
        .iter()
        .flat_map(|group| group.attributes.iter())
        .map(|attr| {
            let mut args = Vec::new();
            for arg in &attr.args {
                args.push(attr_arg_value(arg)?);
            }
            Some(args)
        })
        .collect()
}

fn attr_arg_value(arg: &Expr) -> Option<AttrArgValue> {
    match &arg.kind {
        ExprKind::Null => Some(AttrArgValue::Null),
        ExprKind::IntLiteral(value) => Some(AttrArgValue::Int(*value)),
        ExprKind::BoolLiteral(value) => Some(AttrArgValue::Bool(*value)),
        ExprKind::StringLiteral(value) => Some(AttrArgValue::Str(value.clone())),
        ExprKind::Negate(inner) => {
            if let ExprKind::IntLiteral(value) = &inner.kind {
                Some(AttrArgValue::Int(value.wrapping_neg()))
            } else {
                None
            }
        }
        _ => None,
    }
}

#[derive(Clone, Debug)]
struct SimpleTraitMembers {
    properties: Vec<ClassProperty>,
    methods: Vec<SimpleTraitMethod>,
}

#[derive(Clone, Debug)]
struct SimpleTraitMethod {
    source_trait: String,
    method: ClassMethod,
}

#[derive(Clone, Debug)]
struct RawTraitMembers {
    trait_uses: Vec<TraitUse>,
    properties: Vec<ClassProperty>,
    methods: Vec<ClassMethod>,
}

fn collect_simple_traits(program: &Program) -> HashMap<String, SimpleTraitMembers> {
    let mut raw_traits = HashMap::new();
    for stmt in program {
        let StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
            ..
        } = &stmt.kind else {
            continue;
        };
        raw_traits.insert(
            name.as_str().to_ascii_lowercase(),
            RawTraitMembers {
                trait_uses: trait_uses.clone(),
                properties: properties.clone(),
                methods: methods.clone(),
            },
        );
    }
    let mut traits = HashMap::new();
    for name in raw_traits.keys() {
        let mut stack = Vec::new();
        if let Some(members) = expand_simple_trait(name, &raw_traits, &mut traits, &mut stack) {
            traits.insert(name.clone(), members);
        }
    }
    traits
}

fn expand_simple_trait(
    name: &str,
    raw_traits: &HashMap<String, RawTraitMembers>,
    cache: &mut HashMap<String, SimpleTraitMembers>,
    stack: &mut Vec<String>,
) -> Option<SimpleTraitMembers> {
    if let Some(members) = cache.get(name) {
        return Some(members.clone());
    }
    if stack.iter().any(|entry| entry == name) {
        return None;
    }
    let raw = raw_traits.get(name)?;
    stack.push(name.to_string());
    let mut properties = Vec::new();
    let mut methods = Vec::new();
    for trait_use in &raw.trait_uses {
        if !trait_use.adaptations.is_empty() {
            stack.pop();
            return None;
        }
        for nested_name in &trait_use.trait_names {
            let nested_key = nested_name.as_str().to_ascii_lowercase();
            let nested = expand_simple_trait(&nested_key, raw_traits, cache, stack)?;
            properties.extend(nested.properties);
            methods.extend(nested.methods);
        }
    }
    stack.pop();
    properties.extend(raw.properties.clone());
    methods.extend(raw.methods.iter().cloned().map(|method| SimpleTraitMethod {
        source_trait: name.to_string(),
        method,
    }));
    let members = SimpleTraitMembers {
        properties,
        methods,
    };
    cache.insert(name.to_string(), members.clone());
    Some(members)
}

fn simple_trait_members(
    trait_uses: &[TraitUse],
    traits: &HashMap<String, SimpleTraitMembers>,
) -> Option<(Vec<ClassProperty>, Vec<ClassMethod>)> {
    let mut properties = Vec::new();
    let mut methods = Vec::new();
    for trait_use in trait_uses {
        let listed_trait_names: HashSet<String> = trait_use
            .trait_names
            .iter()
            .map(|name| name.as_str().to_ascii_lowercase())
            .collect();
        let mut method_order = Vec::new();
        let mut candidates: HashMap<String, Vec<SimpleTraitMethod>> = HashMap::new();
        for name in &trait_use.trait_names {
            let trait_members = traits.get(&name.as_str().to_ascii_lowercase())?;
            properties.extend(trait_members.properties.clone());
            for method in &trait_members.methods {
                let method_key = function_key(&method.method.name);
                if !candidates.contains_key(&method_key) {
                    method_order.push(method_key.clone());
                }
                candidates
                    .entry(method_key)
                    .or_default()
                    .push(method.clone());
            }
        }
        let selected_methods =
            select_simple_trait_methods(trait_use, listed_trait_names, method_order, candidates)?;
        methods.extend(selected_methods);
    }
    Some((properties, methods))
}

fn select_simple_trait_methods(
    trait_use: &TraitUse,
    listed_trait_names: HashSet<String>,
    method_order: Vec<String>,
    candidates: HashMap<String, Vec<SimpleTraitMethod>>,
) -> Option<Vec<ClassMethod>> {
    let mut suppressed: HashMap<String, HashSet<String>> = HashMap::new();
    let mut visibility_overrides: HashMap<(String, String), Visibility> = HashMap::new();
    let mut alias_methods = Vec::new();
    for adaptation in &trait_use.adaptations {
        match adaptation {
            TraitAdaptation::InsteadOf {
                trait_name,
                method,
                instead_of,
            } => {
                let method_key = function_key(method);
                let selected_trait = simple_trait_adaptation_source(
                    trait_name.as_ref().map(|name| name.as_str()),
                    &method_key,
                    &candidates,
                )?;
                for loser in instead_of {
                    let loser_key = loser.as_str().to_ascii_lowercase();
                    if !listed_trait_names.contains(&loser_key) || loser_key == selected_trait {
                        return None;
                    }
                    suppressed
                        .entry(method_key.clone())
                        .or_default()
                        .insert(loser_key);
                }
            }
            TraitAdaptation::Alias {
                trait_name,
                method,
                alias,
                visibility,
            } => {
                let method_key = function_key(method);
                let selected_trait = simple_trait_adaptation_source(
                    trait_name.as_ref().map(|name| name.as_str()),
                    &method_key,
                    &candidates,
                )?;
                let imported = candidates
                    .get(&method_key)
                    .and_then(|methods| {
                        methods
                            .iter()
                            .find(|method| method.source_trait == selected_trait)
                    })?
                    .clone();
                if let Some(alias_name) = alias {
                    let mut alias_method = imported.method;
                    alias_method.name = alias_name.clone();
                    if let Some(visibility) = visibility {
                        alias_method.visibility = visibility.clone();
                    }
                    alias_methods.push(alias_method);
                } else if let Some(visibility) = visibility {
                    visibility_overrides.insert((selected_trait, method_key), visibility.clone());
                }
            }
        }
    }

    let mut selected = Vec::new();
    for method_key in method_order {
        let remaining: Vec<_> = candidates
            .get(&method_key)?
            .iter()
            .filter(|method| {
                !suppressed
                    .get(&method_key)
                    .is_some_and(|suppressed| suppressed.contains(&method.source_trait))
            })
            .cloned()
            .collect();
        let mut method = match remaining.as_slice() {
            [method] => method.clone(),
            _ => return None,
        };
        if let Some(visibility) =
            visibility_overrides.get(&(method.source_trait.clone(), method_key.clone()))
        {
            method.method.visibility = visibility.clone();
        }
        selected.push(method.method);
    }
    for alias_method in alias_methods {
        if selected
            .iter()
            .any(|method| method.name.eq_ignore_ascii_case(&alias_method.name))
        {
            return None;
        }
        selected.push(alias_method);
    }
    Some(selected)
}

fn simple_trait_adaptation_source(
    explicit_trait: Option<&str>,
    method_key: &str,
    candidates: &HashMap<String, Vec<SimpleTraitMethod>>,
) -> Option<String> {
    let methods = candidates.get(method_key)?;
    if let Some(trait_name) = explicit_trait {
        let trait_key = trait_name.to_ascii_lowercase();
        return methods
            .iter()
            .any(|method| method.source_trait == trait_key)
            .then_some(trait_key);
    }
    match methods.as_slice() {
        [method] => Some(method.source_trait.clone()),
        _ => None,
    }
}

fn inherited_properties(
    extends: &Option<Name>,
    has_no_traits: bool,
    classes: &HashMap<String, ObjectClassInfo>,
) -> Option<Vec<ObjectPropertyInfo>> {
    if !has_no_traits {
        return None;
    }
    let parent_name = extends.as_ref()?;
    let parent_info = classes.get(&parent_name.as_str().to_ascii_lowercase())?;
    Some(parent_info.properties.clone())
}

fn inherited_static_properties(
    extends: &Option<Name>,
    has_no_traits: bool,
    classes: &HashMap<String, ObjectClassInfo>,
) -> Option<Vec<ObjectStaticPropertyInfo>> {
    if !has_no_traits {
        return None;
    }
    let parent_name = extends.as_ref()?;
    let parent_info = classes.get(&parent_name.as_str().to_ascii_lowercase())?;
    Some(parent_info.static_properties.clone())
}

fn append_own_properties(
    class_name: &str,
    wasm_properties: &mut Vec<ObjectPropertyInfo>,
    properties: &[crate::parser::ast::ClassProperty],
) {
    for property in properties {
        if property.is_static || property.hooks.any() {
            continue;
        }
        if let Some(existing) = wasm_properties
            .iter()
            .position(|existing| existing.name.eq_ignore_ascii_case(&property.name))
        {
            let offset = wasm_properties[existing].offset;
            wasm_properties[existing] = object_property_info(class_name, property, offset);
            continue;
        }
        let slot_index = wasm_properties.len();
        let offset = 8 + slot_index * 24;
        wasm_properties.push(object_property_info(class_name, property, offset));
    }
}

fn object_property_info(
    class_name: &str,
    property: &crate::parser::ast::ClassProperty,
    offset: usize,
) -> ObjectPropertyInfo {
    let initialized_offset =
        (property.type_expr.is_some() && property.default.is_none()).then_some(offset + 16);
    let kind = property
        .type_expr
        .as_ref()
        .and_then(kind_for_type_expr)
        .or_else(|| property.default.as_ref().and_then(kind_for_default))
        .unwrap_or(ObjectPropertyKind::Mixed);
    ObjectPropertyInfo {
        name: property.name.clone(),
        offset,
        initialized_offset,
        kind,
        visibility: property.visibility.clone(),
        owner_class: class_name.to_string(),
        declared_class: property.type_expr.as_ref().and_then(object_class_for_type_expr),
        default: property.default.clone(),
    }
}

fn append_static_properties(
    class_name: &str,
    wasm_properties: &mut Vec<ObjectStaticPropertyInfo>,
    properties: &[crate::parser::ast::ClassProperty],
) {
    for property in properties {
        if !property.is_static || property.hooks.any() {
            continue;
        }
        let Some(property_info) = object_static_property_info(class_name, property) else {
            continue;
        };
        if let Some(existing) = wasm_properties
            .iter()
            .position(|existing| existing.name.eq_ignore_ascii_case(&property.name))
        {
            wasm_properties[existing] = property_info;
        } else {
            wasm_properties.push(property_info);
        }
    }
}

fn object_static_property_info(
    class_name: &str,
    property: &crate::parser::ast::ClassProperty,
) -> Option<ObjectStaticPropertyInfo> {
    let kind = property
        .type_expr
        .as_ref()
        .and_then(kind_for_type_expr)
        .or_else(|| property.default.as_ref().and_then(kind_for_default))
        .unwrap_or(ObjectPropertyKind::Mixed);
    if property.default.is_none() && property.type_expr.is_none() {
        return None;
    }
    if !matches!(
        kind,
        ObjectPropertyKind::Int
            | ObjectPropertyKind::Float
            | ObjectPropertyKind::Bool
            | ObjectPropertyKind::Str
            | ObjectPropertyKind::Object
            | ObjectPropertyKind::Mixed
    ) {
        return None;
    }
    Some(ObjectStaticPropertyInfo {
        name: property.name.clone(),
        kind,
        visibility: property.visibility.clone(),
        owner_class: class_name.to_string(),
        declared_class: property.type_expr.as_ref().and_then(object_class_for_type_expr),
        global: static_property_global(class_name, &property.name, kind),
        needs_initialized_guard: property.type_expr.is_some() && property.default.is_none(),
        default: property.default.clone(),
    })
}

fn static_property_global(class_name: &str, property_name: &str, kind: ObjectPropertyKind) -> String {
    let suffix = match kind {
        ObjectPropertyKind::Int => "i64",
        ObjectPropertyKind::Float => "f64",
        ObjectPropertyKind::Bool => "i32",
        ObjectPropertyKind::Str => "str",
        ObjectPropertyKind::Object | ObjectPropertyKind::Mixed => "value",
    };
    format!(
        "__wasm_static_prop_{}_{}_{}",
        function_key(class_name),
        function_key(property_name),
        suffix
    )
}

fn supported_object_method(method: &ClassMethod) -> bool {
    !method.is_static
        && !method.is_abstract
        && method.has_body
        && method.variadic.is_none()
}

fn supported_static_object_method(method: &ClassMethod) -> bool {
    method.is_static
        && !method.is_abstract
        && method.has_body
        && method.variadic.is_none()
        && !method.name.eq_ignore_ascii_case("__construct")
}

fn object_method_info(
    class_name: &str,
    method: &ClassMethod,
    has_this: bool,
    symbol: fn(&str, &str) -> String,
) -> ObjectMethodInfo {
    ObjectMethodInfo {
        name: method.name.clone(),
        symbol: symbol(class_name, &method.name),
        visibility: method.visibility.clone(),
        owner_class: class_name.to_string(),
        has_this,
        params: method
            .params
            .iter()
            .map(|(param_name, ty, _, _)| (param_name.clone(), ty.clone()))
            .collect(),
        param_kinds: method
            .params
            .iter()
            .map(|(_, ty, _, _)| local_kind_from_type(ty.as_ref()))
            .collect(),
        defaults: method
            .params
            .iter()
            .map(|(_, _, default, _)| default.clone())
            .collect(),
        return_kind: value_kind_from_return_type(method.return_type.as_ref()),
        mixed_return_kind: (value_kind_from_return_type(method.return_type.as_ref()) == ValueKind::Mixed)
            .then(|| {
                consistent_mixed_return_value_kind(
                    &method.params,
                    &method.body,
                )
            })
            .flatten(),
        return_object_class: method
            .return_type
            .as_ref()
            .and_then(object_class_for_type_expr),
        body: method.body.clone(),
    }
}

fn append_inherited_methods(
    class_name: &str,
    extends: &Option<Name>,
    methods: &mut Vec<ObjectMethodInfo>,
    classes: &HashMap<String, ObjectClassInfo>,
) {
    append_inherited_method_symbols(class_name, extends, methods, classes, false, method_symbol);
}

fn append_inherited_static_methods(
    class_name: &str,
    extends: &Option<Name>,
    methods: &mut Vec<ObjectMethodInfo>,
    classes: &HashMap<String, ObjectClassInfo>,
) {
    append_inherited_method_symbols(class_name, extends, methods, classes, true, static_method_symbol);
}

fn append_inherited_method_symbols(
    class_name: &str,
    extends: &Option<Name>,
    methods: &mut Vec<ObjectMethodInfo>,
    classes: &HashMap<String, ObjectClassInfo>,
    is_static: bool,
    symbol: fn(&str, &str) -> String,
) {
    let own_names = methods
        .iter()
        .map(|method| method.name.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let Some(parent_name) = extends else {
        return;
    };
    let mut current = Some(parent_name.as_str().to_ascii_lowercase());
    while let Some(class_key) = current {
        let Some(parent) = classes.get(&class_key) else {
            break;
        };
        let inherited_methods = if is_static {
            &parent.static_methods
        } else {
            &parent.methods
        };
        for method in inherited_methods {
            if method.visibility == Visibility::Private
                || own_names.contains(&method.name.to_ascii_lowercase())
                || methods
                    .iter()
                    .any(|existing| existing.name.eq_ignore_ascii_case(&method.name))
            {
                continue;
            }
            let mut inherited = method.clone();
            inherited.symbol = symbol(class_name, &method.name);
            if method_returns_new_static(&inherited.body) {
                inherited.return_object_class = Some(class_name.to_string());
            }
            methods.push(inherited);
        }
        current = parent.parent.clone();
    }
}

fn method_returns_new_static(body: &[Stmt]) -> bool {
    body.iter().any(stmt_returns_new_static)
}

fn stmt_returns_new_static(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => matches!(
            expr.kind,
            ExprKind::NewScopedObject {
                receiver: StaticReceiver::Static,
                ..
            }
        ),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            method_returns_new_static(then_body)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| method_returns_new_static(body))
                || else_body
                    .as_deref()
                    .is_some_and(method_returns_new_static)
        }
        StmtKind::IfDef { then_body, else_body, .. } => {
            method_returns_new_static(then_body)
                || else_body
                    .as_deref()
                    .is_some_and(method_returns_new_static)
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => method_returns_new_static(body),
        StmtKind::Switch {
            cases, default, ..
        } => {
            cases
                .iter()
                .any(|(_, body)| method_returns_new_static(body))
                || default
                    .as_deref()
                    .is_some_and(method_returns_new_static)
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            method_returns_new_static(try_body)
                || catches
                    .iter()
                    .any(|catch| method_returns_new_static(&catch.body))
                || finally_body
                    .as_deref()
                    .is_some_and(method_returns_new_static)
        }
        _ => false,
    }
}

fn method_symbol(class_name: &str, method_name: &str) -> String {
    format!(
        "__wasm_method_{}_{}",
        function_key(class_name),
        function_key(method_name)
    )
}

fn static_method_symbol(class_name: &str, method_name: &str) -> String {
    format!(
        "__wasm_static_method_{}_{}",
        function_key(class_name),
        function_key(method_name)
    )
}

fn kind_for_type_expr(type_expr: &TypeExpr) -> Option<ObjectPropertyKind> {
    match type_expr {
        TypeExpr::Int => Some(ObjectPropertyKind::Int),
        TypeExpr::Float => Some(ObjectPropertyKind::Float),
        TypeExpr::Bool => Some(ObjectPropertyKind::Bool),
        TypeExpr::Str => Some(ObjectPropertyKind::Str),
        TypeExpr::Named(name)
            if name.as_str().eq_ignore_ascii_case("mixed") || name.as_str().eq_ignore_ascii_case("array") =>
        {
            Some(ObjectPropertyKind::Mixed)
        }
        TypeExpr::Named(_) => Some(ObjectPropertyKind::Object),
        TypeExpr::Nullable(_) | TypeExpr::Union(_) => Some(ObjectPropertyKind::Mixed),
        TypeExpr::Void
        | TypeExpr::Never
        | TypeExpr::Iterable
        | TypeExpr::Array(_)
        | TypeExpr::Ptr(_)
        | TypeExpr::Buffer(_) => None,
    }
}

fn object_class_for_type_expr(type_expr: &TypeExpr) -> Option<String> {
    match type_expr {
        TypeExpr::Named(name)
            if !name.as_str().eq_ignore_ascii_case("mixed") && !name.as_str().eq_ignore_ascii_case("array") =>
        {
            Some(name.as_str().to_string())
        }
        _ => None,
    }
}

fn kind_for_default(expr: &Expr) -> Option<ObjectPropertyKind> {
    match &expr.kind {
        crate::parser::ast::ExprKind::IntLiteral(_) => Some(ObjectPropertyKind::Int),
        crate::parser::ast::ExprKind::FloatLiteral(_) => Some(ObjectPropertyKind::Float),
        crate::parser::ast::ExprKind::BoolLiteral(_) => Some(ObjectPropertyKind::Bool),
        crate::parser::ast::ExprKind::StringLiteral(_) => Some(ObjectPropertyKind::Str),
        crate::parser::ast::ExprKind::NewObject { .. } => Some(ObjectPropertyKind::Object),
        crate::parser::ast::ExprKind::Null => Some(ObjectPropertyKind::Mixed),
        _ => None,
    }
}
