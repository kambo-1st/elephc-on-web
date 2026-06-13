//! Purpose:
//! Collects wasm32-web user-function signature, return, and array-parameter metadata.
//! Keeps metadata discovery separate from the `WasmModule` state container.
//!
//! Called from:
//! - `crate::codegen::wasm::module::WasmModule::new`.
//!
//! Key details:
//! - Reads PHP function declarations and delegates deeper expression analysis to module-level collectors.

use super::*;

mod signature_collection;
mod array_param_assoc;
mod array_param_layout_conflicts;
mod array_param_lengths;
mod array_param_nested;
mod array_param_values;
mod callable_param_targets;
mod array_return_layouts;
mod array_return_values;
mod array_return_params;
mod callable_return_targets;
mod mixed_return_kinds;
mod static_array_return_lengths;
mod static_string_returns;

pub(in crate::codegen::wasm::module::function_metadata) fn object_array_return_methods<'a>(
    object_classes: &'a HashMap<String, object_metadata::ObjectClassInfo>,
) -> Vec<&'a object_metadata::ObjectMethodInfo> {
    object_classes
        .values()
        .flat_map(|class_info| {
            class_info
                .constructor
                .iter()
                .chain(class_info.methods.iter())
                .chain(class_info.static_methods.iter())
        })
        .filter(|method| method.return_kind == ValueKind::Array)
        .collect()
}

pub(super) use self::array_return_params::collect_function_array_return_param_indices;
pub(super) use self::array_param_assoc::collect_function_array_param_assoc_metadata;
pub(super) use self::array_param_layout_conflicts::collect_function_array_param_layout_conflicts;
pub(super) use self::array_param_lengths::collect_function_array_param_lengths;
pub(super) use self::array_param_nested::collect_function_array_param_runtime_nested_values;
pub(super) use self::array_param_values::collect_function_array_param_value_kinds;
pub(super) use self::callable_param_targets::collect_function_callable_param_targets;
pub(super) use self::callable_return_targets::collect_function_callable_return_targets;
pub(super) use self::array_return_layouts::{
    callable_target_for_return_layout, collect_function_array_return_layouts,
    static_array_fill_return_len, static_callback_name_for_return_layout,
    static_string_value_for_return_layout,
};
pub(super) use self::array_return_values::{
    array_return_expr_runtime_value_kind, collect_function_array_return_key_kinds,
    collect_function_array_return_key_values, collect_function_array_return_nested_values,
    collect_function_array_return_runtime_value_kinds, collect_function_array_return_value_constants,
    collect_function_array_return_value_kinds,
    static_split_string_array_len,
};
pub(super) use self::mixed_return_kinds::collect_function_mixed_return_kinds;
pub(super) use self::signature_collection::{
    collect_function_defaults, collect_function_param_kinds,
    collect_function_params, collect_function_return_kinds, collect_function_return_object_classes,
    collect_nullable_function_returns,
};
pub(super) use self::static_array_return_lengths::collect_function_array_return_lengths;
pub(super) use self::static_string_returns::{
    collect_function_possible_static_string_returns, collect_function_static_string_returns,
    static_string_return_call_key_for_expr,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::codegen::wasm::module) enum ParamMetadata<T> {
    Unseen,
    Known(T),
    Unknown,
}

pub(in crate::codegen::wasm::module) fn finalize_param_metadata<T>(
    states: HashMap<String, Vec<ParamMetadata<T>>>,
) -> HashMap<String, Vec<Option<T>>> {
    states
        .into_iter()
        .map(|(name, values)| {
            let values = values
                .into_iter()
                .map(|value| match value {
                    ParamMetadata::Known(value) => Some(value),
                    ParamMetadata::Unseen | ParamMetadata::Unknown => None,
                })
                .collect();
            (name, values)
        })
        .collect()
}

pub(in crate::codegen::wasm::module) fn merge_param_metadata<T: Clone + PartialEq>(
    states: &mut HashMap<String, Vec<ParamMetadata<T>>>,
    function_name: &str,
    param_index: usize,
    param_count: usize,
    value: Option<T>,
) {
    let slots = states
        .entry(function_name.to_string())
        .or_insert_with(|| vec![ParamMetadata::Unseen; param_count]);
    let Some(slot) = slots.get_mut(param_index) else {
        return;
    };
    match (slot, value) {
        (slot @ ParamMetadata::Unseen, Some(value)) => *slot = ParamMetadata::Known(value),
        (ParamMetadata::Known(existing), Some(value)) if *existing == value => {}
        (slot, _) => *slot = ParamMetadata::Unknown,
    }
}

pub(in crate::codegen::wasm::module) fn collect_array_param_calls_in_stmt(
    stmt: &Stmt,
    visit_call: &mut impl FnMut(&Name, &[Expr]),
) {
    match &stmt.kind {
        StmtKind::Echo(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::Throw(expr)
        | StmtKind::Return(Some(expr))
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. } => {
            collect_array_param_calls_in_expr(expr, visit_call);
        }
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::ListUnpack { value, .. } => {
            collect_array_param_calls_in_expr(value, visit_call);
        }
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. } => {
            collect_array_param_calls_in_expr(index, visit_call);
            collect_array_param_calls_in_expr(value, visit_call);
        }
        StmtKind::ArrayPush { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. }
        | StmtKind::PropertyArrayPush { value, .. } => {
            collect_array_param_calls_in_expr(value, visit_call);
        }
        StmtKind::NestedArrayAssign { target, value }
        | StmtKind::NestedArrayPush { target, value } => {
            collect_array_param_calls_in_expr(target, visit_call);
            collect_array_param_calls_in_expr(value, visit_call);
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            collect_array_param_calls_in_expr(object, visit_call);
            collect_array_param_calls_in_expr(value, visit_call);
        }
        StmtKind::StaticPropertyAssign { value, .. } => {
            collect_array_param_calls_in_expr(value, visit_call);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_array_param_calls_in_expr(condition, visit_call);
            collect_array_param_calls_in_stmts(then_body, visit_call);
            for (condition, body) in elseif_clauses {
                collect_array_param_calls_in_expr(condition, visit_call);
                collect_array_param_calls_in_stmts(body, visit_call);
            }
            if let Some(body) = else_body {
                collect_array_param_calls_in_stmts(body, visit_call);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            collect_array_param_calls_in_expr(condition, visit_call);
            collect_array_param_calls_in_stmts(body, visit_call);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_array_param_calls_in_stmt(init, visit_call);
            }
            if let Some(condition) = condition {
                collect_array_param_calls_in_expr(condition, visit_call);
            }
            if let Some(update) = update {
                collect_array_param_calls_in_stmt(update, visit_call);
            }
            collect_array_param_calls_in_stmts(body, visit_call);
        }
        StmtKind::Foreach { array, body, .. } => {
            collect_array_param_calls_in_expr(array, visit_call);
            collect_array_param_calls_in_stmts(body, visit_call);
        }
        StmtKind::Switch { subject, cases, default } => {
            collect_array_param_calls_in_expr(subject, visit_call);
            for (conditions, body) in cases {
                for condition in conditions {
                    collect_array_param_calls_in_expr(condition, visit_call);
                }
                collect_array_param_calls_in_stmts(body, visit_call);
            }
            if let Some(default) = default {
                collect_array_param_calls_in_stmts(default, visit_call);
            }
        }
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => {
            collect_array_param_calls_in_stmts(body, visit_call);
        }
        StmtKind::FunctionDecl { body, .. } => collect_array_param_calls_in_stmts(body, visit_call),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            collect_array_param_calls_in_stmts(try_body, visit_call);
            for catch in catches {
                collect_array_param_calls_in_stmts(&catch.body, visit_call);
            }
            if let Some(body) = finally_body {
                collect_array_param_calls_in_stmts(body, visit_call);
            }
        }
        StmtKind::Include { path, .. } => collect_array_param_calls_in_expr(path, visit_call),
        _ => {}
    }
}

fn collect_array_param_calls_in_stmts(stmts: &[Stmt], visit_call: &mut impl FnMut(&Name, &[Expr])) {
    for stmt in stmts {
        collect_array_param_calls_in_stmt(stmt, visit_call);
    }
}

fn collect_array_param_calls_in_expr(expr: &Expr, visit_call: &mut impl FnMut(&Name, &[Expr])) {
    match &expr.kind {
        ExprKind::FunctionCall { name, args } => {
            visit_call(name, args);
            for arg in args {
                collect_array_param_calls_in_expr(arg, visit_call);
            }
        }
        ExprKind::BinaryOp { left, right, .. }
        | ExprKind::InstanceOf { value: left, target: InstanceOfTarget::Expr(right) }
        | ExprKind::NullCoalesce { value: left, default: right }
        | ExprKind::Pipe { value: left, callable: right }
        | ExprKind::ArrayAccess { array: left, index: right } => {
            collect_array_param_calls_in_expr(left, visit_call);
            collect_array_param_calls_in_expr(right, visit_call);
        }
        ExprKind::Negate(expr)
        | ExprKind::Not(expr)
        | ExprKind::BitNot(expr)
        | ExprKind::Throw(expr)
        | ExprKind::ErrorSuppress(expr)
        | ExprKind::Print(expr)
        | ExprKind::Cast { expr, .. }
        | ExprKind::Spread(expr)
        | ExprKind::PtrCast { expr, .. } => collect_array_param_calls_in_expr(expr, visit_call),
        ExprKind::Assignment { target, value, prelude, result_target, .. } => {
            collect_array_param_calls_in_expr(target, visit_call);
            collect_array_param_calls_in_expr(value, visit_call);
            collect_array_param_calls_in_stmts(prelude, visit_call);
            if let Some(result_target) = result_target {
                collect_array_param_calls_in_expr(result_target, visit_call);
            }
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            collect_array_param_calls_in_expr(condition, visit_call);
            collect_array_param_calls_in_expr(then_expr, visit_call);
            collect_array_param_calls_in_expr(else_expr, visit_call);
        }
        ExprKind::ShortTernary { value, default } => {
            collect_array_param_calls_in_expr(value, visit_call);
            collect_array_param_calls_in_expr(default, visit_call);
        }
        ExprKind::Match { subject, arms, default } => {
            collect_array_param_calls_in_expr(subject, visit_call);
            for (conditions, value) in arms {
                for condition in conditions {
                    collect_array_param_calls_in_expr(condition, visit_call);
                }
                collect_array_param_calls_in_expr(value, visit_call);
            }
            if let Some(default) = default {
                collect_array_param_calls_in_expr(default, visit_call);
            }
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_array_param_calls_in_expr(item, visit_call);
            }
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                collect_array_param_calls_in_expr(key, visit_call);
                collect_array_param_calls_in_expr(value, visit_call);
            }
        }
        ExprKind::Closure { params, body, .. } => {
            for (_, _, default, _) in params {
                if let Some(default) = default {
                    collect_array_param_calls_in_expr(default, visit_call);
                }
            }
            collect_array_param_calls_in_stmts(body, visit_call);
        }
        ExprKind::NamedArg { value, .. } => collect_array_param_calls_in_expr(value, visit_call),
        ExprKind::ClosureCall { args, .. } => {
            for arg in args {
                collect_array_param_calls_in_expr(arg, visit_call);
            }
        }
        ExprKind::ExprCall { callee, args } => {
            collect_array_param_calls_in_expr(callee, visit_call);
            for arg in args {
                collect_array_param_calls_in_expr(arg, visit_call);
            }
        }
        ExprKind::NewObject { args, .. }
        | ExprKind::MethodCall { args, .. }
        | ExprKind::NullsafeMethodCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. } => {
            for arg in args {
                collect_array_param_calls_in_expr(arg, visit_call);
            }
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            collect_array_param_calls_in_expr(object, visit_call);
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            collect_array_param_calls_in_expr(object, visit_call);
            collect_array_param_calls_in_expr(property, visit_call);
        }
        ExprKind::BufferNew { len, .. } => collect_array_param_calls_in_expr(len, visit_call),
        _ => {}
    }
}

pub(in crate::codegen::wasm::module) fn arg_for_param<'a>(
    args: &'a [Expr],
    param_names: &[String],
    param_index: usize,
) -> Option<&'a Expr> {
    let param_name = param_names.get(param_index)?;
    for arg in args {
        if let ExprKind::NamedArg { name, value } = &arg.kind {
            if name.eq_ignore_ascii_case(param_name) {
                return Some(value);
            }
        }
    }
    let mut positional_index = 0;
    for arg in args {
        match &arg.kind {
            ExprKind::NamedArg { .. } => {}
            ExprKind::Spread(_) => return None,
            _ if positional_index == param_index => return Some(arg),
            _ => positional_index += 1,
        }
    }
    None
}

pub(super) fn static_string_for_metadata(
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
            static_string_for_metadata(left, constants)?,
            static_string_for_metadata(right, constants)?
        )),
        _ => None,
    }
}

pub(super) fn static_or_const_int_value_for_metadata(
    expr: &Expr,
    constants: &HashMap<String, ConstantValue>,
) -> Option<i64> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::ConstRef(name) => match constants.get(name.as_str())? {
            ConstantValue::Int(value) => Some(*value),
            _ => None,
        },
        ExprKind::Negate(inner) => static_or_const_int_value_for_metadata(inner, constants)?.checked_neg(),
        _ => None,
    }
}

pub(super) fn static_nested_array_metadata_for_expr(expr: &Expr) -> Option<NestedArrayMetadata> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => {
            let value_kinds = static_value_cell_kinds_for_items(items);
            Some(NestedArrayMetadata {
                layout: ArrayLayout::Value,
                len: items.len(),
                value_kinds,
                key_values: None,
                nested_values: static_nested_array_metadata_for_items(items),
            })
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            let normalized = normalized_static_assoc_items(items);
            let items = normalized.as_deref().unwrap_or(items);
            Some(NestedArrayMetadata {
                layout: ArrayLayout::Assoc,
                len: items.len(),
                value_kinds: static_value_cell_kinds_for_assoc_items(items),
                key_values: static_assoc_key_values_for_items(items),
                nested_values: static_nested_array_metadata_for_assoc_items(items),
            })
        }
        _ => None,
    }
}

pub(super) fn static_nested_array_metadata_for_items(
    items: &[Expr],
) -> Option<Vec<Option<NestedArrayMetadata>>> {
    let metadata = items
        .iter()
        .map(static_nested_array_metadata_for_expr)
        .collect::<Vec<_>>();
    metadata.iter().any(Option::is_some).then_some(metadata)
}

pub(super) fn static_nested_array_metadata_for_assoc_items(
    items: &[(Expr, Expr)],
) -> Option<Vec<Option<NestedArrayMetadata>>> {
    let normalized = normalized_static_assoc_items(items);
    let items = normalized.as_deref().unwrap_or(items);
    let metadata = items
        .iter()
        .map(|(_, value)| static_nested_array_metadata_for_expr(value))
        .collect::<Vec<_>>();
    metadata.iter().any(Option::is_some).then_some(metadata)
}
