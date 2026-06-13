//! Purpose:
//! Collects statically known callable targets for wasm user-function callable parameters.
//! Tracks both monomorphic and finite polymorphic callable descriptors.
//!
//! Called from:
//! - `crate::codegen::wasm::module::function_metadata`
//!
//! Key details:
//! - Monomorphic parameters seed direct callable metadata.
//! - Finite polymorphic parameters seed descriptor metadata; broader dynamic targets stay unknown.

use super::*;

pub(in crate::codegen::wasm::module) fn collect_function_callable_param_targets(
    program: &Program,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    constants: &HashMap<String, ConstantValue>,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
) -> HashMap<String, Vec<Option<String>>> {
    collect_function_possible_callable_param_targets(
        program,
        function_params,
        function_param_kinds,
        function_defaults,
        constants,
        object_classes,
    )
    .into_iter()
    .map(|(name, values)| {
        let values = values
            .into_iter()
            .map(|value| match value {
                Some(targets) if targets.len() == 1 => targets.into_iter().next(),
                _ => None,
            })
            .collect();
        (name, values)
    })
    .collect()
}

pub(in crate::codegen::wasm::module) fn collect_function_possible_callable_param_targets(
    program: &Program,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    constants: &HashMap<String, ConstantValue>,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
) -> HashMap<String, Vec<Option<Vec<String>>>> {
    let callable_return_targets = collect_function_callable_return_targets(program, constants);
    let possible_callable_return_targets =
        collect_function_possible_callable_return_targets(program, constants);
    let mut states = HashMap::new();
    let mut local_callable_targets = HashMap::new();
    for stmt in program {
        collect_possible_callable_param_targets_in_stmt(
            stmt,
            &mut states,
            &mut local_callable_targets,
            &callable_return_targets,
            &possible_callable_return_targets,
            function_params,
            function_param_kinds,
            function_defaults,
            constants,
            object_classes,
        );
    }
    finalize_param_metadata(states)
}

fn collect_possible_callable_param_targets_in_stmt(
    stmt: &Stmt,
    states: &mut HashMap<String, Vec<ParamMetadata<Vec<String>>>>,
    local_callable_targets: &mut HashMap<String, Vec<String>>,
    callable_return_targets: &HashMap<String, String>,
    possible_callable_return_targets: &HashMap<String, Vec<String>>,
    function_params: &HashMap<String, Vec<String>>,
    function_param_kinds: &HashMap<String, Vec<LocalKind>>,
    function_defaults: &HashMap<String, Vec<Option<Expr>>>,
    constants: &HashMap<String, ConstantValue>,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
) {
    if let StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } = &stmt.kind {
        if let Some(targets) = callable_targets_for_param_metadata(
            value,
            local_callable_targets,
            callable_return_targets,
            possible_callable_return_targets,
            constants,
            object_classes,
        ) {
            local_callable_targets.insert(name.clone(), targets);
        } else {
            local_callable_targets.remove(name);
        }
    }
    collect_array_param_calls_in_stmt(stmt, &mut |name, args| {
        let key = function_key(name);
        let Some(param_kinds) = function_param_kinds.get(&key) else {
            return;
        };
        let Some(param_names) = function_params.get(&key) else {
            return;
        };
        let defaults = function_defaults.get(&key);
        for (index, kind) in param_kinds.iter().enumerate() {
            if *kind != LocalKind::Callable {
                continue;
            }
            let default = defaults
                .and_then(|defaults| defaults.get(index))
                .and_then(Option::as_ref);
            let value = arg_for_param(args, param_names, index)
                .or(default)
                .and_then(|arg| {
                    callable_targets_for_param_metadata(
                        arg,
                        local_callable_targets,
                        callable_return_targets,
                        possible_callable_return_targets,
                        constants,
                        object_classes,
                    )
                });
            merge_callable_param_metadata(states, &key, index, param_kinds.len(), value);
        }
    });
}

fn merge_callable_param_metadata(
    states: &mut HashMap<String, Vec<ParamMetadata<Vec<String>>>>,
    function_name: &str,
    param_index: usize,
    param_count: usize,
    value: Option<Vec<String>>,
) {
    let slots = states
        .entry(function_name.to_string())
        .or_insert_with(|| vec![ParamMetadata::Unseen; param_count]);
    let Some(slot) = slots.get_mut(param_index) else {
        return;
    };
    match (slot, value) {
        (slot @ ParamMetadata::Unseen, Some(value)) => *slot = ParamMetadata::Known(value),
        (ParamMetadata::Known(existing), Some(values)) => {
            for value in values {
                push_unique_callable_param_target(existing, value);
            }
        }
        (slot, _) => *slot = ParamMetadata::Unknown,
    }
}

fn callable_targets_for_param_metadata(
    expr: &Expr,
    local_callable_targets: &HashMap<String, Vec<String>>,
    callable_return_targets: &HashMap<String, String>,
    possible_callable_return_targets: &HashMap<String, Vec<String>>,
    constants: &HashMap<String, ConstantValue>,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
) -> Option<Vec<String>> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => {
            let [receiver, method] = items.as_slice() else {
                return None;
            };
            static_callable_array_target_for_param_metadata(
                receiver,
                method,
                constants,
                object_classes,
            )
            .map(|target| vec![target])
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            let receiver = static_int_key_assoc_value(items, 0)?;
            let method = static_int_key_assoc_value(items, 1)?;
            static_callable_array_target_for_param_metadata(
                receiver,
                method,
                constants,
                object_classes,
            )
            .map(|target| vec![target])
        }
        ExprKind::StringLiteral(name) => Some(vec![name.clone()]),
        ExprKind::ConstRef(name) => match constants.get(name.as_str())? {
            ConstantValue::Str(value) => Some(vec![value.clone()]),
            _ => None,
        },
        ExprKind::BinaryOp {
            left,
            op: BinOp::Concat,
            right,
        } => {
            let prefixes = callable_targets_for_param_metadata(
                left,
                local_callable_targets,
                callable_return_targets,
                possible_callable_return_targets,
                constants,
                object_classes,
            )?;
            let suffixes = callable_targets_for_param_metadata(
                right,
                local_callable_targets,
                callable_return_targets,
                possible_callable_return_targets,
                constants,
                object_classes,
            )?;
            let mut targets = Vec::new();
            for prefix in &prefixes {
                for suffix in &suffixes {
                    push_unique_callable_param_target(
                        &mut targets,
                        format!("{}{}", prefix, suffix),
                    );
                }
            }
            Some(targets)
        }
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
            Some(vec![name.to_string()])
        }
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod {
            receiver: StaticReceiver::Named(class_name),
            method,
        }) => Some(vec![format!(
            "__wasm_static_method_{}_{}",
            function_key(class_name.as_str()),
            function_key(method)
        )]),
        ExprKind::Variable(name) => local_callable_targets.get(name).cloned(),
        ExprKind::FunctionCall { name, .. } => possible_callable_return_targets
            .get(&function_key(name.as_str()))
            .cloned()
            .or_else(|| {
                callable_return_targets
                    .get(&function_key(name.as_str()))
                    .map(|target| vec![target.clone()])
            }),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let mut targets = callable_targets_for_param_metadata(
                then_expr,
                local_callable_targets,
                callable_return_targets,
                possible_callable_return_targets,
                constants,
                object_classes,
            )?;
            for target in callable_targets_for_param_metadata(
                else_expr,
                local_callable_targets,
                callable_return_targets,
                possible_callable_return_targets,
                constants,
                object_classes,
            )? {
                push_unique_callable_param_target(&mut targets, target);
            }
            Some(targets)
        }
        _ => None,
    }
}

fn static_callable_array_target_for_param_metadata(
    receiver: &Expr,
    method: &Expr,
    constants: &HashMap<String, ConstantValue>,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
) -> Option<String> {
    let class_name = static_callable_string_value(receiver, constants)?;
    let method_name = static_callable_string_value(method, constants)?;
    let method = static_method_in_hierarchy(&class_name, &method_name, object_classes)?;
    matches!(method.visibility, Visibility::Public).then_some(method.symbol)
}

fn static_method_in_hierarchy(
    class_name: &str,
    method_name: &str,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
) -> Option<object_metadata::ObjectMethodInfo> {
    let mut current = Some(function_key(class_name));
    while let Some(class_key) = current {
        let class_info = object_classes.get(&class_key)?;
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

fn static_callable_string_value(
    expr: &Expr,
    constants: &HashMap<String, ConstantValue>,
) -> Option<String> {
    match &expr.kind {
        ExprKind::StringLiteral(value) => Some(value.clone()),
        ExprKind::ConstRef(name) => match constants.get(name.as_str())? {
            ConstantValue::Str(value) => Some(value.clone()),
            _ => None,
        },
        ExprKind::BinaryOp {
            left,
            op: BinOp::Concat,
            right,
        } => Some(format!(
            "{}{}",
            static_callable_string_value(left, constants)?,
            static_callable_string_value(right, constants)?
        )),
        _ => None,
    }
}

fn static_int_key_assoc_value(items: &[(Expr, Expr)], needle: i64) -> Option<&Expr> {
    items.iter().rev().find_map(|(key, value)| {
        matches!(key.kind, ExprKind::IntLiteral(key) if key == needle).then_some(value)
    })
}

fn push_unique_callable_param_target(values: &mut Vec<String>, value: String) {
    if !values
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&value))
    {
        values.push(value);
    }
}
