//! Purpose:
//! Collects wasm array-return value, nested-array, and associative-key metadata.
//! Keeps array return metadata traversal separate from the broader function metadata module.
//!
//! Called from:
//! - `crate::codegen::wasm::module::function_metadata`.
//! - wasm local metadata collectors that need runtime value-cell kinds for array builders.
//!
//! Key details:
//! - Preserves static and runtime metadata surfaces for array returns without changing native codegen.

use super::*;

pub(in crate::codegen::wasm::module) fn collect_function_array_return_value_kinds(
    program: &Program,
    constants: &HashMap<String, ConstantValue>,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
) -> HashMap<String, Vec<ValueCellKind>> {
    let mut value_kinds = program
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl { name, body, .. } => {
                consistent_array_return_value_kinds(body, constants)
                    .map(|kinds| (function_key(name), kinds))
            }
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    for method in object_array_return_methods(object_classes) {
        if let Some(kinds) = consistent_array_return_value_kinds(&method.body, constants) {
            value_kinds.insert(function_key(&method.symbol), kinds);
        }
    }
    value_kinds
}

pub(in crate::codegen::wasm::module) fn collect_function_array_return_value_constants(
    program: &Program,
    constants: &HashMap<String, ConstantValue>,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
) -> HashMap<String, Vec<ConstantValue>> {
    let mut value_constants = program
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl { name, body, .. } => {
                consistent_array_return_value_constants(body, constants)
                    .map(|values| (function_key(name), values))
            }
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    for method in object_array_return_methods(object_classes) {
        if let Some(values) = consistent_array_return_value_constants(&method.body, constants) {
            value_constants.insert(function_key(&method.symbol), values);
        }
    }
    value_constants
}

pub(in crate::codegen::wasm::module) fn collect_function_array_return_runtime_value_kinds(
    program: &Program,
    function_return_kinds: &HashMap<String, ValueKind>,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
) -> HashMap<String, ValueCellKind> {
    let mut runtime_kinds = program
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl { name, body, .. } => {
                consistent_array_return_runtime_value_kind(body, function_return_kinds)
                    .map(|kind| (function_key(name), kind))
            }
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    for method in object_array_return_methods(object_classes) {
        if let Some(kind) =
            consistent_array_return_runtime_value_kind(&method.body, function_return_kinds)
        {
            runtime_kinds.insert(function_key(&method.symbol), kind);
        }
    }
    runtime_kinds
}

pub(in crate::codegen::wasm::module) fn collect_function_array_return_nested_values(
    program: &Program,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
) -> HashMap<String, Vec<Option<NestedArrayMetadata>>> {
    let mut nested_values = program
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl { name, body, .. } => {
                consistent_array_return_nested_values(body)
                    .map(|metadata| (function_key(name), metadata))
            }
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    for method in object_array_return_methods(object_classes) {
        if let Some(metadata) = consistent_array_return_nested_values(&method.body) {
            nested_values.insert(function_key(&method.symbol), metadata);
        }
    }
    nested_values
}

pub(in crate::codegen::wasm::module) fn collect_function_array_return_key_kinds(
    program: &Program,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
) -> HashMap<String, Vec<AssocKeyKind>> {
    let mut key_kinds = program
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl { name, body, .. } => {
                consistent_array_return_key_kinds(body)
                    .map(|kinds| (function_key(name), kinds))
            }
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    for method in object_array_return_methods(object_classes) {
        if let Some(kinds) = consistent_array_return_key_kinds(&method.body) {
            key_kinds.insert(function_key(&method.symbol), kinds);
        }
    }
    key_kinds
}

pub(in crate::codegen::wasm::module) fn collect_function_array_return_key_values(
    program: &Program,
    object_classes: &HashMap<String, object_metadata::ObjectClassInfo>,
) -> HashMap<String, Vec<AssocKeyValue>> {
    let mut key_values = program
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl { name, body, .. } => {
                consistent_array_return_key_values(body)
                    .map(|values| (function_key(name), values))
            }
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    for method in object_array_return_methods(object_classes) {
        if let Some(values) = consistent_array_return_key_values(&method.body) {
            key_values.insert(function_key(&method.symbol), values);
        }
    }
    key_values
}

fn consistent_array_return_value_kinds(
    stmts: &[Stmt],
    constants: &HashMap<String, ConstantValue>,
) -> Option<Vec<ValueCellKind>> {
    let mut kinds = None;
    for stmt in stmts {
        collect_array_return_value_kinds(stmt, &mut kinds, constants)?;
    }
    kinds
}

fn consistent_array_return_value_constants(
    stmts: &[Stmt],
    constants: &HashMap<String, ConstantValue>,
) -> Option<Vec<ConstantValue>> {
    let mut values = None;
    for stmt in stmts {
        collect_array_return_value_constants(stmt, &mut values, constants)?;
    }
    values
}

fn consistent_array_return_nested_values(stmts: &[Stmt]) -> Option<Vec<Option<NestedArrayMetadata>>> {
    let mut metadata = None;
    for stmt in stmts {
        collect_array_return_nested_values(stmt, &mut metadata)?;
    }
    metadata
}

fn consistent_array_return_key_kinds(stmts: &[Stmt]) -> Option<Vec<AssocKeyKind>> {
    let mut kinds = None;
    for stmt in stmts {
        collect_array_return_key_kinds(stmt, &mut kinds)?;
    }
    kinds
}

fn consistent_array_return_key_values(stmts: &[Stmt]) -> Option<Vec<AssocKeyValue>> {
    let mut values = None;
    for stmt in stmts {
        collect_array_return_key_values(stmt, &mut values)?;
    }
    values
}

fn consistent_array_return_runtime_value_kind(
    stmts: &[Stmt],
    function_return_kinds: &HashMap<String, ValueKind>,
) -> Option<ValueCellKind> {
    let mut kind = None;
    let mut local_value_kinds = HashMap::new();
    let mut local_string_values = HashMap::new();
    let mut local_callable_targets = HashMap::new();
    for stmt in stmts {
        collect_array_return_runtime_value_kind(
            stmt,
            &mut kind,
            &mut local_value_kinds,
            &mut local_string_values,
            &mut local_callable_targets,
            function_return_kinds,
        )?;
    }
    kind
}

fn collect_array_return_value_kinds(
    stmt: &Stmt,
    kinds: &mut Option<Vec<ValueCellKind>>,
    constants: &HashMap<String, ConstantValue>,
) -> Option<()> {
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => {
            let next = array_return_expr_value_kinds(expr, constants)?;
            match kinds {
                Some(existing) if *existing != next => None,
                Some(_) => Some(()),
                None => {
                    *kinds = Some(next);
                    Some(())
                }
            }
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            for stmt in then_body {
                collect_array_return_value_kinds(stmt, kinds, constants)?;
            }
            for (_, body) in elseif_clauses {
                for stmt in body {
                    collect_array_return_value_kinds(stmt, kinds, constants)?;
                }
            }
            if let Some(body) = else_body {
                for stmt in body {
                    collect_array_return_value_kinds(stmt, kinds, constants)?;
                }
            }
            Some(())
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for stmt in stmts {
                collect_array_return_value_kinds(stmt, kinds, constants)?;
            }
            Some(())
        }
        _ => Some(()),
    }
}

fn collect_array_return_value_constants(
    stmt: &Stmt,
    values: &mut Option<Vec<ConstantValue>>,
    constants: &HashMap<String, ConstantValue>,
) -> Option<()> {
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => {
            let next = array_return_expr_value_constants(expr, constants)?;
            match values {
                Some(existing) if *existing != next => None,
                Some(_) => Some(()),
                None => {
                    *values = Some(next);
                    Some(())
                }
            }
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            for stmt in then_body {
                collect_array_return_value_constants(stmt, values, constants)?;
            }
            for (_, body) in elseif_clauses {
                for stmt in body {
                    collect_array_return_value_constants(stmt, values, constants)?;
                }
            }
            if let Some(body) = else_body {
                for stmt in body {
                    collect_array_return_value_constants(stmt, values, constants)?;
                }
            }
            Some(())
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for stmt in stmts {
                collect_array_return_value_constants(stmt, values, constants)?;
            }
            Some(())
        }
        _ => Some(()),
    }
}

fn collect_array_return_runtime_value_kind(
    stmt: &Stmt,
    kind: &mut Option<ValueCellKind>,
    local_value_kinds: &mut HashMap<String, ValueCellKind>,
    local_string_values: &mut HashMap<String, String>,
    local_callable_targets: &mut HashMap<String, String>,
    function_return_kinds: &HashMap<String, ValueKind>,
) -> Option<()> {
    match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            if let Some(target) = callable_target_for_return_layout(value, local_callable_targets) {
                local_callable_targets.insert(name.clone(), target);
            } else {
                local_callable_targets.remove(name);
            }
            if let Some(text) = static_string_value_for_return_layout(value, local_string_values) {
                local_string_values.insert(name.clone(), text);
            } else {
                local_string_values.remove(name);
            }
            if let Some(value_kind) = array_return_expr_runtime_value_kind_for_return(
                value,
                local_value_kinds,
                local_string_values,
                local_callable_targets,
                function_return_kinds,
            ) {
                local_value_kinds.insert(name.clone(), value_kind);
            } else {
                local_value_kinds.remove(name);
            }
            Some(())
        }
        StmtKind::Return(Some(expr)) => {
            let next = array_return_expr_runtime_value_kind_for_return(
                expr,
                local_value_kinds,
                local_string_values,
                local_callable_targets,
                function_return_kinds,
            )?;
            match kind {
                Some(existing) if *existing != next => None,
                Some(_) => Some(()),
                None => {
                    *kind = Some(next);
                    Some(())
                }
            }
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            for stmt in then_body {
                collect_array_return_runtime_value_kind(
                    stmt,
                    kind,
                    &mut local_value_kinds.clone(),
                    &mut local_string_values.clone(),
                    &mut local_callable_targets.clone(),
                    function_return_kinds,
                )?;
            }
            for (_, body) in elseif_clauses {
                for stmt in body {
                    collect_array_return_runtime_value_kind(
                        stmt,
                        kind,
                        &mut local_value_kinds.clone(),
                        &mut local_string_values.clone(),
                        &mut local_callable_targets.clone(),
                        function_return_kinds,
                    )?;
                }
            }
            if let Some(body) = else_body {
                for stmt in body {
                    collect_array_return_runtime_value_kind(
                        stmt,
                        kind,
                        &mut local_value_kinds.clone(),
                        &mut local_string_values.clone(),
                        &mut local_callable_targets.clone(),
                        function_return_kinds,
                    )?;
                }
            }
            Some(())
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for stmt in stmts {
                collect_array_return_runtime_value_kind(
                    stmt,
                    kind,
                    local_value_kinds,
                    local_string_values,
                    local_callable_targets,
                    function_return_kinds,
                )?;
            }
            Some(())
        }
        _ => Some(()),
    }
}

fn collect_array_return_nested_values(
    stmt: &Stmt,
    metadata: &mut Option<Vec<Option<NestedArrayMetadata>>>,
) -> Option<()> {
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => {
            let next = array_return_expr_nested_values(expr)?;
            match metadata {
                Some(existing) if *existing != next => None,
                Some(_) => Some(()),
                None => {
                    *metadata = Some(next);
                    Some(())
                }
            }
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            for stmt in then_body {
                collect_array_return_nested_values(stmt, metadata)?;
            }
            for (_, body) in elseif_clauses {
                for stmt in body {
                    collect_array_return_nested_values(stmt, metadata)?;
                }
            }
            if let Some(body) = else_body {
                for stmt in body {
                    collect_array_return_nested_values(stmt, metadata)?;
                }
            }
            Some(())
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for stmt in stmts {
                collect_array_return_nested_values(stmt, metadata)?;
            }
            Some(())
        }
        _ => Some(()),
    }
}

fn collect_array_return_key_kinds(
    stmt: &Stmt,
    kinds: &mut Option<Vec<AssocKeyKind>>,
) -> Option<()> {
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => {
            let next = array_return_expr_key_kinds(expr)?;
            match kinds {
                Some(existing) if *existing != next => None,
                Some(_) => Some(()),
                None => {
                    *kinds = Some(next);
                    Some(())
                }
            }
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            for stmt in then_body {
                collect_array_return_key_kinds(stmt, kinds)?;
            }
            for (_, body) in elseif_clauses {
                for stmt in body {
                    collect_array_return_key_kinds(stmt, kinds)?;
                }
            }
            if let Some(body) = else_body {
                for stmt in body {
                    collect_array_return_key_kinds(stmt, kinds)?;
                }
            }
            Some(())
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for stmt in stmts {
                collect_array_return_key_kinds(stmt, kinds)?;
            }
            Some(())
        }
        _ => Some(()),
    }
}

fn collect_array_return_key_values(
    stmt: &Stmt,
    values: &mut Option<Vec<AssocKeyValue>>,
) -> Option<()> {
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => {
            let next = array_return_expr_key_values(expr)?;
            match values {
                Some(existing) if *existing != next => None,
                Some(_) => Some(()),
                None => {
                    *values = Some(next);
                    Some(())
                }
            }
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            for stmt in then_body {
                collect_array_return_key_values(stmt, values)?;
            }
            for (_, body) in elseif_clauses {
                for stmt in body {
                    collect_array_return_key_values(stmt, values)?;
                }
            }
            if let Some(body) = else_body {
                for stmt in body {
                    collect_array_return_key_values(stmt, values)?;
                }
            }
            Some(())
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for stmt in stmts {
                collect_array_return_key_values(stmt, values)?;
            }
            Some(())
        }
        _ => Some(()),
    }
}

fn array_return_expr_value_kinds(
    expr: &Expr,
    constants: &HashMap<String, ConstantValue>,
) -> Option<Vec<ValueCellKind>> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => static_value_cell_kinds_for_items(items),
        ExprKind::ArrayLiteralAssoc(items) => static_value_cell_kinds_for_assoc_items(items),
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_fill") => {
            let count = static_array_fill_return_len(args, constants)?;
            let kind = static_value_cell_kind_for_expr(args.get(2)?)?;
            Some(vec![kind; count])
        }
        ExprKind::FunctionCall { .. } => {
            static_split_string_array_len(expr, constants).map(|len| vec![ValueCellKind::Str; len])
        }
        _ => None,
    }
}

fn array_return_expr_value_constants(
    expr: &Expr,
    constants: &HashMap<String, ConstantValue>,
) -> Option<Vec<ConstantValue>> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => items
            .iter()
            .map(|item| static_constant_value_for_array_return(item, constants))
            .collect(),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .map(|(_, value)| static_constant_value_for_array_return(value, constants))
            .collect(),
        _ => None,
    }
}

fn static_constant_value_for_array_return(
    expr: &Expr,
    constants: &HashMap<String, ConstantValue>,
) -> Option<ConstantValue> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(ConstantValue::Int(*value)),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => Some(ConstantValue::Int(value.checked_neg()?)),
            ExprKind::FloatLiteral(value) => Some(ConstantValue::Float(-value)),
            _ => None,
        },
        ExprKind::FloatLiteral(value) => Some(ConstantValue::Float(*value)),
        ExprKind::BoolLiteral(value) => Some(ConstantValue::Bool(*value)),
        ExprKind::StringLiteral(value) => Some(ConstantValue::Str(value.clone())),
        ExprKind::Null => Some(ConstantValue::Null),
        ExprKind::ConstRef(name) => constants.get(name.as_str()).cloned(),
        ExprKind::BinaryOp {
            op: BinOp::Concat, ..
        } => static_string_for_metadata(expr, constants).map(ConstantValue::Str),
        _ => None,
    }
}

pub(in crate::codegen::wasm::module) fn array_return_expr_runtime_value_kind(
    expr: &Expr,
    local_value_kinds: &HashMap<String, ValueCellKind>,
) -> Option<ValueCellKind> {
    match &expr.kind {
        ExprKind::Variable(name) => local_value_kinds.get(name).copied(),
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_fill") => {
            runtime_value_cell_kind_for_return_expr(args.get(2)?, local_value_kinds)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_pad") => {
            let source_kind = args
                .first()
                .and_then(|source| array_return_expr_runtime_value_kind(source, local_value_kinds))?;
            let pad_kind = args.get(2).and_then(static_value_cell_kind_for_expr)?;
            (source_kind == pad_kind).then_some(source_kind)
        }
        ExprKind::FunctionCall { name, .. }
            if name.eq_ignore_ascii_case("explode") || name.eq_ignore_ascii_case("str_split") =>
        {
            Some(ValueCellKind::Str)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_fill_keys") => {
            args.get(1).and_then(static_value_cell_kind_for_expr)
        }
        _ => None,
    }
}

fn array_return_expr_runtime_value_kind_for_return(
    expr: &Expr,
    local_value_kinds: &HashMap<String, ValueCellKind>,
    local_string_values: &HashMap<String, String>,
    local_callable_targets: &HashMap<String, String>,
    function_return_kinds: &HashMap<String, ValueKind>,
) -> Option<ValueCellKind> {
    match &expr.kind {
        ExprKind::Variable(name) => local_value_kinds.get(name).copied(),
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_map") => {
            array_map_runtime_value_kind_for_return(
                args,
                local_string_values,
                local_callable_targets,
                function_return_kinds,
            )
                .or_else(|| array_return_expr_runtime_value_kind(expr, local_value_kinds))
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_filter") => {
            array_filter_runtime_value_kind_for_return(
                args,
                local_value_kinds,
                local_string_values,
                local_callable_targets,
                function_return_kinds,
            )
                .or_else(|| array_return_expr_runtime_value_kind(expr, local_value_kinds))
        }
        _ => array_return_expr_runtime_value_kind(expr, local_value_kinds)
            .or_else(|| runtime_value_cell_kind_for_return_expr(expr, local_value_kinds)),
    }
}

fn array_filter_runtime_value_kind_for_return(
    args: &[Expr],
    local_value_kinds: &HashMap<String, ValueCellKind>,
    local_string_values: &HashMap<String, String>,
    local_callable_targets: &HashMap<String, String>,
    function_return_kinds: &HashMap<String, ValueKind>,
) -> Option<ValueCellKind> {
    if args.is_empty()
        || args.len() > 3
        || (args.len() == 3
            && !matches!(array_filter_metadata::mode_value_for_locals(&args[2]), Some(1 | 2)))
    {
        return None;
    }
    let source_kind = match &args[0].kind {
        ExprKind::Variable(source) => local_value_kinds.get(source).copied(),
        ExprKind::FunctionCall {
            name,
            args: source_args,
        } if name.eq_ignore_ascii_case("array_map") => {
            array_map_runtime_value_kind_for_return(
                source_args,
                local_string_values,
                local_callable_targets,
                function_return_kinds,
            )
        }
        ExprKind::FunctionCall { .. } => array_return_expr_runtime_value_kind(&args[0], local_value_kinds),
        _ => None,
    }?;
    matches!(
        source_kind,
        ValueCellKind::Int
            | ValueCellKind::Str
            | ValueCellKind::Bool
            | ValueCellKind::Float
            | ValueCellKind::Null
            | ValueCellKind::Array
    )
    .then_some(source_kind)
}

fn array_map_runtime_value_kind_for_return(
    args: &[Expr],
    local_string_values: &HashMap<String, String>,
    local_callable_targets: &HashMap<String, String>,
    function_return_kinds: &HashMap<String, ValueKind>,
) -> Option<ValueCellKind> {
    if args.len() < 2 {
        return None;
    }
    let callback = args.first().and_then(|callback| {
        static_callback_name_for_return_layout(
            callback,
            local_string_values,
            local_callable_targets,
        )
    })?;
    match array_map_callback_return_kind(&callback, function_return_kinds)? {
        ValueKind::Int => Some(ValueCellKind::Int),
        ValueKind::Str => Some(ValueCellKind::Str),
        ValueKind::Bool => Some(ValueCellKind::Bool),
        _ => None,
    }
}

fn runtime_value_cell_kind_for_return_expr(
    expr: &Expr,
    local_value_kinds: &HashMap<String, ValueCellKind>,
) -> Option<ValueCellKind> {
    match &expr.kind {
        ExprKind::Variable(name) => local_value_kinds.get(name).copied(),
        _ => static_value_cell_kind_for_expr(expr),
    }
}

fn array_return_expr_nested_values(expr: &Expr) -> Option<Vec<Option<NestedArrayMetadata>>> {
    match &expr.kind {
        ExprKind::ArrayLiteral(items) => static_nested_array_metadata_for_items(items),
        ExprKind::ArrayLiteralAssoc(items) => static_nested_array_metadata_for_assoc_items(items),
        _ => None,
    }
}

pub(in crate::codegen::wasm::module) fn static_split_string_array_len(
    expr: &Expr,
    constants: &HashMap<String, ConstantValue>,
) -> Option<usize> {
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return None;
    };
    if name.eq_ignore_ascii_case("explode") {
        return static_explode_len(args, constants);
    }
    if name.eq_ignore_ascii_case("str_split") {
        return static_str_split_len(args, constants);
    }
    None
}

fn static_explode_len(args: &[Expr], constants: &HashMap<String, ConstantValue>) -> Option<usize> {
    if args.len() < 2 || args.len() > 3 {
        return None;
    }
    let separator = static_string_for_metadata(&args[0], constants)?;
    if separator.is_empty() {
        return None;
    }
    let value = static_string_for_metadata(&args[1], constants)?;
    let mut parts = value.split(separator.as_str()).count();
    if args.get(2).is_some()
        && args
            .get(2)
            .and_then(|expr| static_or_const_int_value_for_metadata(expr, constants))
            .is_none()
    {
        return None;
    }
    if let Some(limit) = args
        .get(2)
        .and_then(|expr| static_or_const_int_value_for_metadata(expr, constants))
    {
        if limit == 0 {
            parts = parts.min(1);
        } else if limit > 0 {
            parts = parts.min(limit as usize);
        } else {
            let drop_count = usize::try_from(limit.unsigned_abs()).ok()?;
            parts = parts.saturating_sub(drop_count);
        }
    }
    Some(parts)
}

fn static_str_split_len(
    args: &[Expr],
    constants: &HashMap<String, ConstantValue>,
) -> Option<usize> {
    if args.is_empty() || args.len() > 2 {
        return None;
    }
    let value = static_string_for_metadata(&args[0], constants)?;
    if args.get(1).is_some()
        && args
            .get(1)
            .and_then(|expr| static_or_const_int_value_for_metadata(expr, constants))
            .is_none()
    {
        return None;
    }
    let chunk = args
        .get(1)
        .and_then(|expr| static_or_const_int_value_for_metadata(expr, constants))
        .unwrap_or(1);
    let chunk = usize::try_from(chunk).ok().filter(|chunk| *chunk > 0)?;
    if value.is_empty() {
        return Some(0);
    }
    Some(value.len().div_ceil(chunk))
}

fn array_return_expr_key_kinds(expr: &Expr) -> Option<Vec<AssocKeyKind>> {
    match &expr.kind {
        ExprKind::ArrayLiteralAssoc(items) => static_assoc_key_kinds_for_items(items),
        _ => None,
    }
}

fn array_return_expr_key_values(expr: &Expr) -> Option<Vec<AssocKeyValue>> {
    match &expr.kind {
        ExprKind::ArrayLiteralAssoc(items) => static_assoc_key_values_for_items(items),
        _ => None,
    }
}
