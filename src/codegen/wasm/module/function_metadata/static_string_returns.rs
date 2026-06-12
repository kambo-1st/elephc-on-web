//! Purpose:
//! Collects wasm32-web metadata for functions that can return a static string.
//! Keeps string-return walking separate from array and mixed return metadata.
//!
//! Called from:
//! - `super` re-export consumed by `crate::codegen::wasm::module::WasmModule::new`.
//!
//! Key details:
//! - Only functions declared with a string return type are considered.
//! - Branches must resolve to one exact constant string for the relevant call shape.

use super::*;

pub(in crate::codegen::wasm::module) fn collect_function_static_string_returns(
    program: &Program,
    constants: &HashMap<String, ConstantValue>,
) -> HashMap<String, String> {
    let mut returns = HashMap::new();
    for stmt in program {
        let StmtKind::FunctionDecl {
            name,
            params,
            return_type,
            body,
            ..
        } = &stmt.kind else {
            continue;
        };
        if value_kind_from_return_type(return_type.as_ref()) != ValueKind::Str {
            continue;
        }
        if let Some(value) = consistent_static_string_return(body, constants, &HashMap::new()) {
            returns.insert(function_key(name), value);
        }
        let param_names = params
            .iter()
            .map(|(param_name, _, _, _)| param_name.clone())
            .collect::<Vec<_>>();
        for args in static_arg_combinations(params, body, constants) {
            let env = param_names
                .iter()
                .cloned()
                .zip(args.iter().cloned())
                .collect::<HashMap<_, _>>();
            if let Some(value) = consistent_static_string_return(body, constants, &env) {
                returns.insert(static_string_return_call_key(name, &args), value);
            }
        }
    }
    returns
}

pub(in crate::codegen::wasm::module) fn collect_function_possible_static_string_returns(
    program: &Program,
    constants: &HashMap<String, ConstantValue>,
) -> HashMap<String, Vec<String>> {
    let mut returns = HashMap::new();
    for stmt in program {
        let StmtKind::FunctionDecl {
            name,
            return_type,
            body,
            ..
        } = &stmt.kind else {
            continue;
        };
        if value_kind_from_return_type(return_type.as_ref()) != ValueKind::Str {
            continue;
        }
        let mut values = Vec::new();
        if collect_possible_static_string_returns(body, constants, &HashMap::new(), &mut values).is_some()
            && values.len() > 1
        {
            returns.insert(function_key(name), values);
        }
    }
    returns
}

pub(in crate::codegen::wasm::module) fn static_string_return_call_key_for_expr(
    name: &Name,
    args: &[Expr],
    constants: &HashMap<String, ConstantValue>,
) -> Option<String> {
    let args = args
        .iter()
        .map(|arg| constant_value_from_expr(arg, constants))
        .collect::<Option<Vec<_>>>()?;
    Some(static_string_return_call_key(name.as_str(), &args))
}

fn static_arg_combinations(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    body: &[Stmt],
    constants: &HashMap<String, ConstantValue>,
) -> Vec<Vec<ConstantValue>> {
    if params.is_empty() || params.len() > 4 {
        return Vec::new();
    }
    let mut values = Vec::new();
    collect_static_arg_candidates_from_stmts(body, constants, &mut values);
    let mut domains = params
        .iter()
        .map(|(_, ty, _, _)| static_arg_domain_for_type(ty.as_ref(), &values))
        .collect::<Vec<_>>();
    if domains.iter().any(Vec::is_empty) {
        return Vec::new();
    }
    for domain in &mut domains {
        domain.truncate(8);
    }
    let mut out = Vec::new();
    let mut current = Vec::new();
    build_static_arg_combinations(&domains, 0, &mut current, &mut out);
    out
}

fn static_arg_domain_for_type(ty: Option<&TypeExpr>, values: &[ConstantValue]) -> Vec<ConstantValue> {
    let mut out = Vec::new();
    if type_accepts_bool(ty) {
        push_static_arg_candidate(&mut out, ConstantValue::Bool(false));
        push_static_arg_candidate(&mut out, ConstantValue::Bool(true));
    }
    for value in values {
        if type_accepts_static_arg(ty, value) {
            push_static_arg_candidate(&mut out, value.clone());
        }
    }
    out
}

fn build_static_arg_combinations(
    domains: &[Vec<ConstantValue>],
    index: usize,
    current: &mut Vec<ConstantValue>,
    out: &mut Vec<Vec<ConstantValue>>,
) {
    if out.len() >= 64 {
        return;
    }
    if index == domains.len() {
        out.push(current.clone());
        return;
    }
    for value in &domains[index] {
        current.push(value.clone());
        build_static_arg_combinations(domains, index + 1, current, out);
        current.pop();
        if out.len() >= 64 {
            break;
        }
    }
}

fn collect_static_arg_candidates_from_stmts(
    stmts: &[Stmt],
    constants: &HashMap<String, ConstantValue>,
    out: &mut Vec<ConstantValue>,
) {
    for stmt in stmts {
        collect_static_arg_candidates_from_stmt(stmt, constants, out);
    }
}

fn collect_static_arg_candidates_from_stmt(
    stmt: &Stmt,
    constants: &HashMap<String, ConstantValue>,
    out: &mut Vec<ConstantValue>,
) {
    match &stmt.kind {
        StmtKind::Echo(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::Throw(expr)
        | StmtKind::Return(Some(expr))
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::Assign { value: expr, .. }
        | StmtKind::TypedAssign { value: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::ArrayPush { value: expr, .. }
        | StmtKind::StaticPropertyArrayPush { value: expr, .. }
        | StmtKind::PropertyArrayPush { value: expr, .. } => {
            collect_static_arg_candidates_from_expr(expr, constants, out);
        }
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. } => {
            collect_static_arg_candidates_from_expr(index, constants, out);
            collect_static_arg_candidates_from_expr(value, constants, out);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            collect_static_arg_candidates_from_expr(condition, constants, out);
            collect_static_arg_candidates_from_stmts(then_body, constants, out);
            for (condition, body) in elseif_clauses {
                collect_static_arg_candidates_from_expr(condition, constants, out);
                collect_static_arg_candidates_from_stmts(body, constants, out);
            }
            if let Some(body) = else_body {
                collect_static_arg_candidates_from_stmts(body, constants, out);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            collect_static_arg_candidates_from_expr(condition, constants, out);
            collect_static_arg_candidates_from_stmts(body, constants, out);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
            ..
        } => {
            if let Some(init) = init {
                collect_static_arg_candidates_from_stmt(init, constants, out);
            }
            if let Some(condition) = condition {
                collect_static_arg_candidates_from_expr(condition, constants, out);
            }
            if let Some(update) = update {
                collect_static_arg_candidates_from_stmt(update, constants, out);
            }
            collect_static_arg_candidates_from_stmts(body, constants, out);
        }
        StmtKind::Foreach { array, body, .. } => {
            collect_static_arg_candidates_from_expr(array, constants, out);
            collect_static_arg_candidates_from_stmts(body, constants, out);
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            collect_static_arg_candidates_from_stmts(stmts, constants, out);
        }
        _ => {}
    }
}

fn collect_static_arg_candidates_from_expr(
    expr: &Expr,
    constants: &HashMap<String, ConstantValue>,
    out: &mut Vec<ConstantValue>,
) {
    if let Some(value) = constant_value_from_expr(expr, constants) {
        push_static_arg_candidate(out, value);
        return;
    }
    match &expr.kind {
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_static_arg_candidates_from_expr(condition, constants, out);
            collect_static_arg_candidates_from_expr(then_expr, constants, out);
            collect_static_arg_candidates_from_expr(else_expr, constants, out);
        }
        ExprKind::ShortTernary { value, default } => {
            collect_static_arg_candidates_from_expr(value, constants, out);
            collect_static_arg_candidates_from_expr(default, constants, out);
        }
        ExprKind::Not(inner) | ExprKind::Negate(inner) => {
            collect_static_arg_candidates_from_expr(inner, constants, out);
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_static_arg_candidates_from_expr(left, constants, out);
            collect_static_arg_candidates_from_expr(right, constants, out);
        }
        ExprKind::FunctionCall { args, .. } => {
            for arg in args {
                collect_static_arg_candidates_from_expr(arg, constants, out);
            }
        }
        _ => {}
    }
}

fn push_static_arg_candidate(out: &mut Vec<ConstantValue>, value: ConstantValue) {
    if !matches!(
        value,
        ConstantValue::Int(_)
            | ConstantValue::Float(_)
            | ConstantValue::Bool(_)
            | ConstantValue::Str(_)
            | ConstantValue::Null
    ) {
        return;
    }
    let key = static_arg_key_part(&value);
    if !out.iter().any(|existing| static_arg_key_part(existing) == key) {
        out.push(value);
    }
}

fn type_accepts_static_arg(ty: Option<&TypeExpr>, value: &ConstantValue) -> bool {
    match ty {
        Some(TypeExpr::Int) => matches!(value, ConstantValue::Int(_)),
        Some(TypeExpr::Float) => matches!(value, ConstantValue::Float(_) | ConstantValue::Int(_)),
        Some(TypeExpr::Bool) => matches!(value, ConstantValue::Bool(_)),
        Some(TypeExpr::Str) => matches!(value, ConstantValue::Str(_)),
        Some(TypeExpr::Nullable(inner)) => {
            matches!(value, ConstantValue::Null) || type_accepts_static_arg(Some(inner), value)
        }
        Some(TypeExpr::Union(types)) => types
            .iter()
            .any(|inner| type_accepts_static_arg(Some(inner), value)),
        Some(_) => false,
        None => matches!(
            value,
            ConstantValue::Int(_)
                | ConstantValue::Float(_)
                | ConstantValue::Bool(_)
                | ConstantValue::Str(_)
                | ConstantValue::Null
        ),
    }
}

fn type_accepts_bool(ty: Option<&TypeExpr>) -> bool {
    match ty {
        Some(TypeExpr::Bool) | None => true,
        Some(TypeExpr::Nullable(inner)) => type_accepts_bool(Some(inner)),
        Some(TypeExpr::Union(types)) => types.iter().any(|inner| type_accepts_bool(Some(inner))),
        _ => false,
    }
}

fn static_string_return_call_key(name: &str, args: &[ConstantValue]) -> String {
    let mut key = function_key(name);
    key.push('#');
    for (index, arg) in args.iter().enumerate() {
        if index > 0 {
            key.push(',');
        }
        key.push_str(&static_arg_key_part(arg));
    }
    key
}

fn static_arg_key_part(value: &ConstantValue) -> String {
    match value {
        ConstantValue::Int(value) => format!("i{value}"),
        ConstantValue::Float(value) => format!("f{:016x}", value.to_bits()),
        ConstantValue::Bool(value) => format!("b{}", if *value { 1 } else { 0 }),
        ConstantValue::Str(value) => format!("s{}:{value}", value.len()),
        ConstantValue::Null => "n".to_string(),
    }
}

fn consistent_static_string_return(
    stmts: &[Stmt],
    constants: &HashMap<String, ConstantValue>,
    env: &HashMap<String, ConstantValue>,
) -> Option<String> {
    let mut value: Option<String> = None;
    for stmt in stmts {
        collect_static_string_returns(stmt, constants, env, &mut value)?;
    }
    value
}

fn collect_possible_static_string_returns(
    stmts: &[Stmt],
    constants: &HashMap<String, ConstantValue>,
    env: &HashMap<String, ConstantValue>,
    values: &mut Vec<String>,
) -> Option<()> {
    for stmt in stmts {
        collect_possible_static_string_returns_from_stmt(stmt, constants, env, values)?;
    }
    Some(())
}

fn collect_possible_static_string_returns_from_stmt(
    stmt: &Stmt,
    constants: &HashMap<String, ConstantValue>,
    env: &HashMap<String, ConstantValue>,
    values: &mut Vec<String>,
) -> Option<()> {
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => collect_possible_static_string_returns_from_expr(
            expr,
            constants,
            env,
            values,
        ),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            if let Some(condition) = constant_value_from_expr_with_env(condition, constants, env) {
                if constant_truthiness(&condition) {
                    return collect_possible_static_string_returns(then_body, constants, env, values);
                }
                for (elseif_condition, body) in elseif_clauses {
                    if constant_truthiness(&constant_value_from_expr_with_env(
                        elseif_condition,
                        constants,
                        env,
                    )?) {
                        return collect_possible_static_string_returns(body, constants, env, values);
                    }
                }
                if let Some(body) = else_body {
                    return collect_possible_static_string_returns(body, constants, env, values);
                }
                return Some(());
            }
            collect_possible_static_string_returns(then_body, constants, env, values)?;
            for (_, body) in elseif_clauses {
                collect_possible_static_string_returns(body, constants, env, values)?;
            }
            if let Some(body) = else_body {
                collect_possible_static_string_returns(body, constants, env, values)?;
            }
            Some(())
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            collect_possible_static_string_returns(stmts, constants, env, values)
        }
        _ => Some(()),
    }
}

fn collect_possible_static_string_returns_from_expr(
    expr: &Expr,
    constants: &HashMap<String, ConstantValue>,
    env: &HashMap<String, ConstantValue>,
    values: &mut Vec<String>,
) -> Option<()> {
    match &expr.kind {
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            if let Some(condition) = constant_value_from_expr_with_env(condition, constants, env) {
                return if constant_truthiness(&condition) {
                    collect_possible_static_string_returns_from_expr(then_expr, constants, env, values)
                } else {
                    collect_possible_static_string_returns_from_expr(else_expr, constants, env, values)
                };
            }
            collect_possible_static_string_returns_from_expr(then_expr, constants, env, values)?;
            collect_possible_static_string_returns_from_expr(else_expr, constants, env, values)
        }
        ExprKind::ShortTernary { value, default } => {
            collect_possible_static_string_returns_from_expr(value, constants, env, values)?;
            collect_possible_static_string_returns_from_expr(default, constants, env, values)
        }
        _ => {
            let value = constant_string_value(constant_value_from_expr_with_env(expr, constants, env)?)?;
            if !values.iter().any(|existing| existing == &value) {
                values.push(value);
            }
            Some(())
        }
    }
}

fn collect_static_string_returns(
    stmt: &Stmt,
    constants: &HashMap<String, ConstantValue>,
    env: &HashMap<String, ConstantValue>,
    value: &mut Option<String>,
) -> Option<()> {
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => {
            let next = constant_string_value(constant_value_from_expr_with_env(expr, constants, env)?)?;
            match value {
                Some(existing) if existing != &next => None,
                Some(_) => Some(()),
                slot @ None => {
                    *slot = Some(next);
                    Some(())
                }
            }
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            if let Some(condition) = constant_value_from_expr_with_env(condition, constants, env) {
                if constant_truthiness(&condition) {
                    for stmt in then_body {
                        collect_static_string_returns(stmt, constants, env, value)?;
                    }
                    return Some(());
                }
                for (elseif_condition, body) in elseif_clauses {
                    if constant_truthiness(&constant_value_from_expr_with_env(
                        elseif_condition,
                        constants,
                        env,
                    )?) {
                        for stmt in body {
                            collect_static_string_returns(stmt, constants, env, value)?;
                        }
                        return Some(());
                    }
                }
                if let Some(body) = else_body {
                    for stmt in body {
                        collect_static_string_returns(stmt, constants, env, value)?;
                    }
                }
                return Some(());
            }
            for stmt in then_body {
                collect_static_string_returns(stmt, constants, env, value)?;
            }
            for (_, body) in elseif_clauses {
                for stmt in body {
                    collect_static_string_returns(stmt, constants, env, value)?;
                }
            }
            if let Some(body) = else_body {
                for stmt in body {
                    collect_static_string_returns(stmt, constants, env, value)?;
                }
            }
            Some(())
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for stmt in stmts {
                collect_static_string_returns(stmt, constants, env, value)?;
            }
            Some(())
        }
        _ => Some(()),
    }
}

fn constant_value_from_expr_with_env(
    expr: &Expr,
    constants: &HashMap<String, ConstantValue>,
    env: &HashMap<String, ConstantValue>,
) -> Option<ConstantValue> {
    match &expr.kind {
        ExprKind::Variable(name) => env.get(name).cloned(),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            if constant_truthiness(&constant_value_from_expr_with_env(condition, constants, env)?) {
                constant_value_from_expr_with_env(then_expr, constants, env)
            } else {
                constant_value_from_expr_with_env(else_expr, constants, env)
            }
        }
        ExprKind::ShortTernary { value, default } => {
            let value = constant_value_from_expr_with_env(value, constants, env)?;
            if constant_truthiness(&value) {
                Some(value)
            } else {
                constant_value_from_expr_with_env(default, constants, env)
            }
        }
        ExprKind::Not(inner) => {
            let value = constant_value_from_expr_with_env(inner, constants, env)?;
            Some(ConstantValue::Bool(!constant_truthiness(&value)))
        }
        ExprKind::BinaryOp { left, op, right } => {
            let left = constant_value_from_expr_with_env(left, constants, env)?;
            let right = constant_value_from_expr_with_env(right, constants, env)?;
            eval_static_string_return_binary(left, op, right)
        }
        ExprKind::ClassConstant { receiver: StaticReceiver::Named(class_name) } => {
            Some(ConstantValue::Str(class_name.as_str().to_string()))
        }
        ExprKind::BoolLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::Negate(_) => constant_value_from_expr(expr, constants),
        _ => None,
    }
}

fn eval_static_string_return_binary(
    left: ConstantValue,
    op: &BinOp,
    right: ConstantValue,
) -> Option<ConstantValue> {
    match op {
        BinOp::StrictEq => Some(ConstantValue::Bool(static_arg_key_part(&left) == static_arg_key_part(&right))),
        BinOp::StrictNotEq => Some(ConstantValue::Bool(static_arg_key_part(&left) != static_arg_key_part(&right))),
        BinOp::Lt => compare_static_selector_values(left, right, |ordering| ordering.is_lt()),
        BinOp::Gt => compare_static_selector_values(left, right, |ordering| ordering.is_gt()),
        BinOp::LtEq => compare_static_selector_values(left, right, |ordering| !ordering.is_gt()),
        BinOp::GtEq => compare_static_selector_values(left, right, |ordering| !ordering.is_lt()),
        BinOp::And => Some(ConstantValue::Bool(
            constant_truthiness(&left) && constant_truthiness(&right),
        )),
        BinOp::Or => Some(ConstantValue::Bool(
            constant_truthiness(&left) || constant_truthiness(&right),
        )),
        BinOp::Xor => Some(ConstantValue::Bool(
            constant_truthiness(&left) ^ constant_truthiness(&right),
        )),
        _ => None,
    }
}

fn compare_static_selector_values(
    left: ConstantValue,
    right: ConstantValue,
    check: impl FnOnce(std::cmp::Ordering) -> bool,
) -> Option<ConstantValue> {
    let ordering = match (left, right) {
        (ConstantValue::Int(left), ConstantValue::Int(right)) => left.cmp(&right),
        (ConstantValue::Float(left), ConstantValue::Float(right)) => left.partial_cmp(&right)?,
        (ConstantValue::Int(left), ConstantValue::Float(right)) => (left as f64).partial_cmp(&right)?,
        (ConstantValue::Float(left), ConstantValue::Int(right)) => left.partial_cmp(&(right as f64))?,
        (ConstantValue::Str(left), ConstantValue::Str(right)) => left.cmp(&right),
        _ => return None,
    };
    Some(ConstantValue::Bool(check(ordering)))
}
