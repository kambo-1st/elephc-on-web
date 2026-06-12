//! Purpose:
//! Collects wasm32-web declaration and constant metadata from the PHP AST.
//! Keeps compile-time constant evaluation separate from module state and emission.
//!
//! Called from:
//! - `crate::codegen::wasm::module::WasmModule::new`.
//! - Function/local metadata helpers that need static constant values.
//!
//! Key details:
//! - Constant folding here is metadata-only and must preserve PHP-compatible results.

use super::*;

pub(super) enum DeclKind {
    Class,
    Interface,
    Trait,
    Enum,
}

pub(super) fn collect_decl_names(program: &Program, kind: DeclKind) -> HashSet<String> {
    let mut names = HashSet::new();
    for stmt in program {
        collect_stmt_decl_names(stmt, &kind, &mut names);
    }
    names
}

pub(super) fn collect_interface_parents(program: &Program) -> HashMap<String, Vec<String>> {
    let mut parents = HashMap::new();
    for stmt in program {
        collect_stmt_interface_parents(stmt, &mut parents);
    }
    parents
}

pub(super) fn collect_enum_cases(program: &Program) -> HashMap<String, Vec<EnumCaseMetadata>> {
    let mut cases = HashMap::new();
    for stmt in program {
        collect_stmt_enum_cases(stmt, &mut cases);
    }
    cases
}

fn collect_stmt_enum_cases(stmt: &Stmt, cases: &mut HashMap<String, Vec<EnumCaseMetadata>>) {
    match &stmt.kind {
        StmtKind::EnumDecl {
            name,
            cases: enum_cases,
            ..
        } => {
            cases.insert(
                function_key(name),
                enum_cases
                    .iter()
                    .map(|case| EnumCaseMetadata {
                        name: case.name.clone(),
                        value: case.value.as_ref().and_then(enum_case_backing_value),
                    })
                    .collect(),
            );
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for stmt in stmts {
                collect_stmt_enum_cases(stmt, cases);
            }
        }
        _ => {}
    }
}

fn enum_case_backing_value(expr: &Expr) -> Option<EnumCaseBackingValue> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(EnumCaseBackingValue::Int(*value)),
        ExprKind::StringLiteral(value) => Some(EnumCaseBackingValue::Str(value.clone())),
        _ => None,
    }
}

fn collect_stmt_decl_names(stmt: &Stmt, kind: &DeclKind, names: &mut HashSet<String>) {
    match &stmt.kind {
        StmtKind::ClassDecl { name, .. } if matches!(kind, DeclKind::Class) => {
            names.insert(function_key(name));
        }
        StmtKind::InterfaceDecl { name, .. } if matches!(kind, DeclKind::Interface) => {
            names.insert(function_key(name));
        }
        StmtKind::TraitDecl { name, .. } if matches!(kind, DeclKind::Trait) => {
            names.insert(function_key(name));
        }
        StmtKind::EnumDecl { name, .. } if matches!(kind, DeclKind::Enum) => {
            names.insert(function_key(name));
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for stmt in stmts {
                collect_stmt_decl_names(stmt, kind, names);
            }
        }
        _ => {}
    }
}

fn collect_stmt_interface_parents(stmt: &Stmt, parents: &mut HashMap<String, Vec<String>>) {
    match &stmt.kind {
        StmtKind::InterfaceDecl { name, extends, .. } => {
            parents.insert(
                function_key(name),
                extends
                    .iter()
                    .map(|parent| parent.as_str().to_ascii_lowercase())
                    .collect(),
            );
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for stmt in stmts {
                collect_stmt_interface_parents(stmt, parents);
            }
        }
        _ => {}
    }
}

pub(super) fn collect_constants(program: &Program) -> HashMap<String, ConstantValue> {
    let mut constants = HashMap::new();
    for stmt in program {
        collect_stmt_constants(stmt, &mut constants, None);
    }
    constants
}

pub(super) fn collect_constants_with_class_constants(
    program: &Program,
    class_constants: &HashMap<String, ConstantValue>,
) -> HashMap<String, ConstantValue> {
    let mut constants = HashMap::new();
    for stmt in program {
        collect_stmt_constants(stmt, &mut constants, Some(class_constants));
    }
    constants
}

pub(super) fn collect_array_constants(program: &Program) -> HashMap<String, ConstantArrayValue> {
    let mut constants = HashMap::new();
    for stmt in program {
        collect_stmt_array_constants(stmt, &mut constants);
    }
    constants
}

pub(super) fn collect_class_constants(program: &Program) -> HashMap<String, ConstantValue> {
    let top_level_constants = collect_constants(program);
    let mut constants = HashMap::new();
    let mut class_parents = HashMap::new();
    let trait_constants = collect_simple_trait_constants(program, &top_level_constants);
    for stmt in program {
        collect_stmt_class_constants(stmt, &top_level_constants, None, &mut constants);
        collect_stmt_trait_class_constants(stmt, &trait_constants, &mut constants);
        collect_stmt_class_parents(stmt, &mut class_parents);
    }
    collect_dependent_class_constants(program, &top_level_constants, &mut constants);
    add_inherited_class_constants(&class_parents, &mut constants);
    constants
}

fn collect_stmt_array_constants(
    stmt: &Stmt,
    constants: &mut HashMap<String, ConstantArrayValue>,
) {
    match &stmt.kind {
        StmtKind::ConstDecl { name, value } => {
            match &value.kind {
                ExprKind::ArrayLiteral(items) => {
                    constants.insert(name.clone(), ConstantArrayValue::Indexed(items.clone()));
                }
                ExprKind::ArrayLiteralAssoc(items) => {
                    constants.insert(name.clone(), ConstantArrayValue::Assoc(items.clone()));
                }
                _ => {}
            }
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for stmt in stmts {
                collect_stmt_array_constants(stmt, constants);
            }
        }
        _ => {}
    }
}

fn collect_simple_trait_constants(
    program: &Program,
    top_level_constants: &HashMap<String, ConstantValue>,
) -> HashMap<String, HashMap<String, ConstantValue>> {
    let mut raw_traits = HashMap::new();
    for stmt in program {
        let StmtKind::TraitDecl {
            name,
            trait_uses,
            constants,
            ..
        } = &stmt.kind
        else {
            continue;
        };
        let mut trait_constants = HashMap::new();
        for class_const in constants {
            if let Some(value) = constant_value_from_expr(&class_const.value, top_level_constants)
            {
                trait_constants.insert(class_const.name.clone(), value);
            }
        }
        raw_traits.insert(function_key(name), (trait_uses.clone(), trait_constants));
    }
    let mut traits = HashMap::new();
    for name in raw_traits.keys() {
        let mut stack = Vec::new();
        if let Some(constants) = expand_simple_trait_constants(name, &raw_traits, &mut traits, &mut stack)
        {
            traits.insert(name.clone(), constants);
        }
    }
    traits
}

fn expand_simple_trait_constants(
    name: &str,
    raw_traits: &HashMap<String, (Vec<TraitUse>, HashMap<String, ConstantValue>)>,
    cache: &mut HashMap<String, HashMap<String, ConstantValue>>,
    stack: &mut Vec<String>,
) -> Option<HashMap<String, ConstantValue>> {
    if let Some(constants) = cache.get(name) {
        return Some(constants.clone());
    }
    if stack.iter().any(|entry| entry == name) {
        return None;
    }
    let (trait_uses, own_constants) = raw_traits.get(name)?;
    stack.push(name.to_string());
    let mut constants = HashMap::new();
    for trait_use in trait_uses {
        if !trait_use.adaptations.is_empty() {
            stack.pop();
            return None;
        }
        for nested_name in &trait_use.trait_names {
            let nested_key = function_key(nested_name);
            let nested = expand_simple_trait_constants(&nested_key, raw_traits, cache, stack)?;
            merge_trait_constants(&mut constants, nested)?;
        }
    }
    stack.pop();
    merge_trait_constants(&mut constants, own_constants.clone())?;
    cache.insert(name.to_string(), constants.clone());
    Some(constants)
}

fn merge_trait_constants(
    target: &mut HashMap<String, ConstantValue>,
    incoming: HashMap<String, ConstantValue>,
) -> Option<()> {
    for (name, value) in incoming {
        match target.get(&name) {
            Some(existing) if existing == &value => {}
            Some(_) => return None,
            None => {
                target.insert(name, value);
            }
        }
    }
    Some(())
}

fn collect_stmt_trait_class_constants(
    stmt: &Stmt,
    trait_constants: &HashMap<String, HashMap<String, ConstantValue>>,
    constants: &mut HashMap<String, ConstantValue>,
) {
    match &stmt.kind {
        StmtKind::ClassDecl {
            name, trait_uses, ..
        } => {
            for trait_use in trait_uses {
                if !trait_use.adaptations.is_empty() {
                    continue;
                }
                for trait_name in &trait_use.trait_names {
                    let Some(imported) = trait_constants.get(&function_key(trait_name)) else {
                        continue;
                    };
                    for (const_name, value) in imported {
                        let key = class_const_key(name, const_name);
                        match constants.get(&key) {
                            Some(existing) if existing == value => {}
                            Some(_) => {
                                constants.remove(&key);
                            }
                            None => {
                                constants.insert(key, value.clone());
                            }
                        }
                    }
                }
            }
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for stmt in stmts {
                collect_stmt_trait_class_constants(stmt, trait_constants, constants);
            }
        }
        _ => {}
    }
}

fn collect_stmt_class_constants(
    stmt: &Stmt,
    top_level_constants: &HashMap<String, ConstantValue>,
    known_class_constants: Option<&HashMap<String, ConstantValue>>,
    constants: &mut HashMap<String, ConstantValue>,
) {
    match &stmt.kind {
        StmtKind::ClassDecl {
            name,
            constants: class_constants,
            ..
        }
        | StmtKind::InterfaceDecl {
            name,
            constants: class_constants,
            ..
        }
        | StmtKind::TraitDecl {
            name,
            constants: class_constants,
            ..
        } => {
            for class_const in class_constants {
                if let Some(value) = constant_value_from_expr_with_class_constants(
                    &class_const.value,
                    top_level_constants,
                    known_class_constants,
                )
                {
                    constants.insert(class_const_key(name, &class_const.name), value);
                }
            }
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for stmt in stmts {
                collect_stmt_class_constants(
                    stmt,
                    top_level_constants,
                    known_class_constants,
                    constants,
                );
            }
        }
        _ => {}
    }
}

fn collect_dependent_class_constants(
    program: &Program,
    top_level_constants: &HashMap<String, ConstantValue>,
    constants: &mut HashMap<String, ConstantValue>,
) {
    let mut changed = true;
    while changed {
        changed = false;
        let snapshot = constants.clone();
        let before = constants.len();
        for stmt in program {
            collect_stmt_class_constants(
                stmt,
                top_level_constants,
                Some(&snapshot),
                constants,
            );
        }
        if constants.len() != before {
            changed = true;
        }
    }
}

fn collect_stmt_class_parents(stmt: &Stmt, class_parents: &mut HashMap<String, String>) {
    match &stmt.kind {
        StmtKind::ClassDecl {
            name,
            extends: Some(parent),
            ..
        } => {
            class_parents.insert(function_key(name), function_key(parent));
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for stmt in stmts {
                collect_stmt_class_parents(stmt, class_parents);
            }
        }
        _ => {}
    }
}

fn add_inherited_class_constants(
    class_parents: &HashMap<String, String>,
    constants: &mut HashMap<String, ConstantValue>,
) {
    let mut changed = true;
    while changed {
        changed = false;
        let snapshot = constants.clone();
        for (class_key, parent_key) in class_parents {
            let inherited: Vec<_> = snapshot
                .iter()
                .filter_map(|(key, value)| {
                    let (owner, const_name) = key.split_once('\0')?;
                    (owner == parent_key.as_str()).then(|| (const_name.to_string(), value.clone()))
                })
                .collect();
            for (const_name, value) in inherited {
                let child_key = class_const_key(class_key, &const_name);
                if let std::collections::hash_map::Entry::Vacant(entry) =
                    constants.entry(child_key)
                {
                    entry.insert(value);
                    changed = true;
                }
            }
        }
    }
}

pub(super) fn class_const_key(class_name: &str, const_name: &str) -> String {
    format!("{}\0{}", function_key(class_name), const_name)
}

fn collect_stmt_constants(
    stmt: &Stmt,
    constants: &mut HashMap<String, ConstantValue>,
    class_constants: Option<&HashMap<String, ConstantValue>>,
) {
    match &stmt.kind {
        StmtKind::ConstDecl { name, value } => {
            if let Some(value) =
                constant_value_from_expr_with_class_constants(value, constants, class_constants)
            {
                constants.insert(name.clone(), value);
            }
        }
        StmtKind::Synthetic(stmts) | StmtKind::NamespaceBlock { body: stmts, .. } => {
            for stmt in stmts {
                collect_stmt_constants(stmt, constants, class_constants);
            }
        }
        _ => {}
    }
}

pub(super) fn constant_value_from_expr(
    expr: &Expr,
    constants: &HashMap<String, ConstantValue>,
) -> Option<ConstantValue> {
    constant_value_from_expr_with_class_constants(expr, constants, None)
}

fn constant_value_from_expr_with_class_constants(
    expr: &Expr,
    constants: &HashMap<String, ConstantValue>,
    class_constants: Option<&HashMap<String, ConstantValue>>,
) -> Option<ConstantValue> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(ConstantValue::Int(*value)),
        ExprKind::FloatLiteral(value) => Some(ConstantValue::Float(*value)),
        ExprKind::BoolLiteral(value) => Some(ConstantValue::Bool(*value)),
        ExprKind::StringLiteral(value) => Some(ConstantValue::Str(value.clone())),
        ExprKind::Null => Some(ConstantValue::Null),
        ExprKind::ConstRef(name) => constants.get(name.as_str()).cloned(),
        ExprKind::ClassConstant { receiver: StaticReceiver::Named(class_name) } => {
            Some(ConstantValue::Str(class_name.as_str().to_string()))
        }
        ExprKind::ScopedConstantAccess {
            receiver: StaticReceiver::Named(class_name),
            name,
        } => class_constants
            .and_then(|values| values.get(&class_const_key(class_name, name)))
            .cloned(),
        ExprKind::Negate(inner) => negate_constant(constant_value_from_expr_with_class_constants(
            inner,
            constants,
            class_constants,
        )?),
        ExprKind::Not(inner) => {
            let value = constant_value_from_expr_with_class_constants(
                inner,
                constants,
                class_constants,
            )?;
            Some(ConstantValue::Bool(!constant_truthiness(&value)))
        }
        ExprKind::BinaryOp { left, op, right } => {
            let left = constant_value_from_expr_with_class_constants(
                left,
                constants,
                class_constants,
            )?;
            let right = constant_value_from_expr_with_class_constants(
                right,
                constants,
                class_constants,
            )?;
            eval_constant_binary(left, op, right)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            let condition = constant_value_from_expr_with_class_constants(
                condition,
                constants,
                class_constants,
            )?;
            if constant_truthiness(&condition) {
                constant_value_from_expr_with_class_constants(
                    then_expr,
                    constants,
                    class_constants,
                )
            } else {
                constant_value_from_expr_with_class_constants(
                    else_expr,
                    constants,
                    class_constants,
                )
            }
        }
        ExprKind::ShortTernary { value, default } => {
            let value = constant_value_from_expr_with_class_constants(
                value,
                constants,
                class_constants,
            )?;
            if constant_truthiness(&value) {
                Some(value)
            } else {
                constant_value_from_expr_with_class_constants(
                    default,
                    constants,
                    class_constants,
                )
            }
        }
        ExprKind::NullCoalesce { value, default } => {
            let value = constant_value_from_expr_with_class_constants(
                value,
                constants,
                class_constants,
            )?;
            if matches!(value, ConstantValue::Null) {
                constant_value_from_expr_with_class_constants(
                    default,
                    constants,
                    class_constants,
                )
            } else {
                Some(value)
            }
        }
        _ => None,
    }
}

fn negate_constant(value: ConstantValue) -> Option<ConstantValue> {
    match value {
        ConstantValue::Int(value) => Some(ConstantValue::Int(value.checked_neg()?)),
        ConstantValue::Float(value) => Some(ConstantValue::Float(-value)),
        _ => None,
    }
}

fn eval_constant_binary(
    left: ConstantValue,
    op: &BinOp,
    right: ConstantValue,
) -> Option<ConstantValue> {
    match op {
        BinOp::Add => eval_constant_int_float(left, right, i64::checked_add, |a, b| a + b),
        BinOp::Sub => eval_constant_int_float(left, right, i64::checked_sub, |a, b| a - b),
        BinOp::Mul => eval_constant_int_float(left, right, i64::checked_mul, |a, b| a * b),
        BinOp::Div => {
            let left = constant_numeric_as_float(left)?;
            let right = constant_numeric_as_float(right)?;
            Some(ConstantValue::Float(left / right))
        }
        BinOp::Mod => {
            let (left, right) = constant_int_pair(left, right)?;
            if right == 0 {
                return None;
            }
            Some(ConstantValue::Int(left % right))
        }
        BinOp::Pow => {
            let left = constant_numeric_as_float(left)?;
            let right = constant_numeric_as_float(right)?;
            Some(ConstantValue::Float(left.powf(right)))
        }
        BinOp::BitAnd => {
            let (left, right) = constant_int_pair(left, right)?;
            Some(ConstantValue::Int(left & right))
        }
        BinOp::BitOr => {
            let (left, right) = constant_int_pair(left, right)?;
            Some(ConstantValue::Int(left | right))
        }
        BinOp::BitXor => {
            let (left, right) = constant_int_pair(left, right)?;
            Some(ConstantValue::Int(left ^ right))
        }
        BinOp::ShiftLeft => {
            let (left, right) = constant_int_pair(left, right)?;
            Some(ConstantValue::Int(left.checked_shl(u32::try_from(right).ok()?)?))
        }
        BinOp::ShiftRight => {
            let (left, right) = constant_int_pair(left, right)?;
            Some(ConstantValue::Int(left.checked_shr(u32::try_from(right).ok()?)?))
        }
        BinOp::Concat => {
            let left = constant_string_value(left)?;
            let right = constant_string_value(right)?;
            Some(ConstantValue::Str(format!("{}{}", left, right)))
        }
        BinOp::And => Some(ConstantValue::Bool(
            constant_truthiness(&left) && constant_truthiness(&right),
        )),
        BinOp::Or => Some(ConstantValue::Bool(
            constant_truthiness(&left) || constant_truthiness(&right),
        )),
        BinOp::Xor => Some(ConstantValue::Bool(
            constant_truthiness(&left) ^ constant_truthiness(&right),
        )),
        BinOp::Eq
        | BinOp::NotEq
        | BinOp::StrictEq
        | BinOp::StrictNotEq
        | BinOp::Lt
        | BinOp::Gt
        | BinOp::LtEq
        | BinOp::GtEq => Some(ConstantValue::Bool(compare_constant_scalars(&left, op, &right))),
        BinOp::Spaceship => Some(ConstantValue::Int(constant_spaceship(&left, &right))),
        _ => None,
    }
}

fn eval_constant_int_float(
    left: ConstantValue,
    right: ConstantValue,
    int_op: fn(i64, i64) -> Option<i64>,
    float_op: fn(f64, f64) -> f64,
) -> Option<ConstantValue> {
    match (left, right) {
        (ConstantValue::Int(left), ConstantValue::Int(right)) => {
            Some(ConstantValue::Int(int_op(left, right)?))
        }
        (left, right) => Some(ConstantValue::Float(float_op(
            constant_numeric_as_float(left)?,
            constant_numeric_as_float(right)?,
        ))),
    }
}

fn constant_int_pair(left: ConstantValue, right: ConstantValue) -> Option<(i64, i64)> {
    match (left, right) {
        (ConstantValue::Int(left), ConstantValue::Int(right)) => Some((left, right)),
        _ => None,
    }
}

fn constant_numeric_as_float(value: ConstantValue) -> Option<f64> {
    match value {
        ConstantValue::Int(value) => Some(value as f64),
        ConstantValue::Float(value) => Some(value),
        _ => None,
    }
}

pub(super) fn constant_string_value(value: ConstantValue) -> Option<String> {
    match value {
        ConstantValue::Str(value) => Some(value),
        _ => None,
    }
}

fn constant_spaceship(left: &ConstantValue, right: &ConstantValue) -> i64 {
    if compare_loose_constant_scalars(left, &BinOp::Lt, right) {
        -1
    } else if compare_loose_constant_scalars(left, &BinOp::Gt, right) {
        1
    } else {
        0
    }
}

fn compare_constant_scalars(left: &ConstantValue, op: &BinOp, right: &ConstantValue) -> bool {
    match op {
        BinOp::StrictEq => constant_same_type(left, right) && compare_same_constant_type(left, op, right),
        BinOp::StrictNotEq => {
            !constant_same_type(left, right) || compare_same_constant_type(left, op, right)
        }
        BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq => {
            compare_loose_constant_scalars(left, op, right)
        }
        _ => unreachable!(),
    }
}

fn constant_same_type(left: &ConstantValue, right: &ConstantValue) -> bool {
    matches!(
        (left, right),
        (ConstantValue::Int(_), ConstantValue::Int(_))
            | (ConstantValue::Float(_), ConstantValue::Float(_))
            | (ConstantValue::Bool(_), ConstantValue::Bool(_))
            | (ConstantValue::Str(_), ConstantValue::Str(_))
            | (ConstantValue::Null, ConstantValue::Null)
    )
}

fn compare_same_constant_type(left: &ConstantValue, op: &BinOp, right: &ConstantValue) -> bool {
    match (left, right) {
        (ConstantValue::Int(left), ConstantValue::Int(right)) => compare_i64(*left, op, *right),
        (ConstantValue::Float(left), ConstantValue::Float(right)) => compare_f64(*left, op, *right),
        (ConstantValue::Bool(left), ConstantValue::Bool(right)) => {
            compare_i64(i64::from(*left), op, i64::from(*right))
        }
        (ConstantValue::Str(left), ConstantValue::Str(right)) => {
            compare_ordering(left.as_bytes().cmp(right.as_bytes()), op)
        }
        (ConstantValue::Null, ConstantValue::Null) => compare_i64(0, op, 0),
        _ => false,
    }
}

fn compare_loose_constant_scalars(left: &ConstantValue, op: &BinOp, right: &ConstantValue) -> bool {
    if matches!(left, ConstantValue::Bool(_)) || matches!(right, ConstantValue::Bool(_)) {
        return compare_i64(
            i64::from(constant_truthiness(left)),
            op,
            i64::from(constant_truthiness(right)),
        );
    }
    if matches!(left, ConstantValue::Null) || matches!(right, ConstantValue::Null) {
        if matches!(op, BinOp::Eq | BinOp::NotEq) {
            let equal = constant_null_loose_equal(left, right);
            return if matches!(op, BinOp::Eq) { equal } else { !equal };
        }
        return compare_i64(
            i64::from(constant_truthiness(left)),
            op,
            i64::from(constant_truthiness(right)),
        );
    }

    match (left, right) {
        (ConstantValue::Str(left), ConstantValue::Str(right)) => {
            match (php_numeric_string_value(left), php_numeric_string_value(right)) {
                (Some(left), Some(right)) => compare_f64(left, op, right),
                _ => compare_ordering(left.as_bytes().cmp(right.as_bytes()), op),
            }
        }
        (ConstantValue::Str(left), right) => compare_string_and_non_bool_constant(left, op, right, false),
        (left, ConstantValue::Str(right)) => compare_string_and_non_bool_constant(right, op, left, true),
        _ => compare_numeric_constants(left, op, right),
    }
}

fn constant_null_loose_equal(left: &ConstantValue, right: &ConstantValue) -> bool {
    match (left, right) {
        (ConstantValue::Null, ConstantValue::Null) => true,
        (ConstantValue::Null, value) | (value, ConstantValue::Null) => match value {
            ConstantValue::Int(value) => *value == 0,
            ConstantValue::Float(value) => *value == 0.0,
            ConstantValue::Bool(value) => !*value,
            ConstantValue::Str(value) => value.is_empty(),
            ConstantValue::Null => true,
        },
        _ => false,
    }
}

fn compare_string_and_non_bool_constant(
    string: &str,
    op: &BinOp,
    scalar: &ConstantValue,
    scalar_is_left: bool,
) -> bool {
    if let Some(string_number) = php_numeric_string_value(string) {
        let scalar_number = constant_numeric_as_float_ref(scalar).unwrap_or(0.0);
        return if scalar_is_left {
            compare_f64(scalar_number, op, string_number)
        } else {
            compare_f64(string_number, op, scalar_number)
        };
    }

    let scalar_string = constant_string_for_compare(scalar);
    let ordering = if scalar_is_left {
        scalar_string.as_bytes().cmp(string.as_bytes())
    } else {
        string.as_bytes().cmp(scalar_string.as_bytes())
    };
    compare_ordering(ordering, op)
}

fn compare_numeric_constants(left: &ConstantValue, op: &BinOp, right: &ConstantValue) -> bool {
    match (left, right) {
        (ConstantValue::Int(left), ConstantValue::Int(right)) => compare_i64(*left, op, *right),
        _ => compare_f64(
            constant_numeric_as_float_ref(left).unwrap_or(0.0),
            op,
            constant_numeric_as_float_ref(right).unwrap_or(0.0),
        ),
    }
}

fn constant_numeric_as_float_ref(value: &ConstantValue) -> Option<f64> {
    match value {
        ConstantValue::Int(value) => Some(*value as f64),
        ConstantValue::Float(value) => Some(*value),
        _ => None,
    }
}

fn constant_string_for_compare(value: &ConstantValue) -> String {
    match value {
        ConstantValue::Int(value) => value.to_string(),
        ConstantValue::Float(value) => value.to_string(),
        ConstantValue::Bool(true) => "1".to_string(),
        ConstantValue::Bool(false) | ConstantValue::Null => String::new(),
        ConstantValue::Str(value) => value.clone(),
    }
}

fn compare_i64(left: i64, op: &BinOp, right: i64) -> bool {
    match op {
        BinOp::Eq | BinOp::StrictEq => left == right,
        BinOp::NotEq | BinOp::StrictNotEq => left != right,
        BinOp::Lt => left < right,
        BinOp::Gt => left > right,
        BinOp::LtEq => left <= right,
        BinOp::GtEq => left >= right,
        _ => unreachable!(),
    }
}

fn compare_f64(left: f64, op: &BinOp, right: f64) -> bool {
    match op {
        BinOp::Eq | BinOp::StrictEq => left == right,
        BinOp::NotEq | BinOp::StrictNotEq => left != right,
        BinOp::Lt => left < right,
        BinOp::Gt => left > right,
        BinOp::LtEq => left <= right,
        BinOp::GtEq => left >= right,
        _ => unreachable!(),
    }
}

fn compare_ordering(ordering: std::cmp::Ordering, op: &BinOp) -> bool {
    match op {
        BinOp::Eq | BinOp::StrictEq => ordering.is_eq(),
        BinOp::NotEq | BinOp::StrictNotEq => !ordering.is_eq(),
        BinOp::Lt => ordering.is_lt(),
        BinOp::Gt => ordering.is_gt(),
        BinOp::LtEq => !ordering.is_gt(),
        BinOp::GtEq => !ordering.is_lt(),
        _ => unreachable!(),
    }
}

fn php_numeric_string_value(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<f64>().ok()
}

pub(super) fn constant_truthiness(value: &ConstantValue) -> bool {
    match value {
        ConstantValue::Int(value) => *value != 0,
        ConstantValue::Float(value) => *value != 0.0,
        ConstantValue::Bool(value) => *value,
        ConstantValue::Str(value) => !value.is_empty() && value != "0",
        ConstantValue::Null => false,
    }
}
