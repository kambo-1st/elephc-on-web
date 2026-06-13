//! Purpose:
//! Lowers PHP existence, callability, and isset-style predicates for wasm32-web.
//! Keeps builtin-name catalog checks out of the main expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` scalar builtin dispatch.
//! - `crate::codegen::wasm::expr::functions` callable helper lowering.
//!
//! Key details:
//! - Builtin existence stays in lockstep with the wasm-supported builtin surface.

use super::*;
use crate::parser::ast::Visibility;

pub(super) fn emit_existence_call(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let arg = match args {
        [arg] => arg,
        [arg, autoload]
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "class_exists" | "interface_exists" | "trait_exists" | "enum_exists"
            ) =>
        {
            arg
        }
        _ => {
            return Err(CompileError::new(
                call.span,
                &format!(
                    "wasm32-web {}() expects {}",
                    name,
                    if matches!(
                        name.to_ascii_lowercase().as_str(),
                        "class_exists" | "interface_exists" | "trait_exists" | "enum_exists"
                    ) {
                        "one or two arguments"
                    } else {
                        "exactly one argument"
                    }
                ),
            ));
        }
    };
    let lower_name = name.to_ascii_lowercase();
    let value = match &arg.kind {
        ExprKind::Variable(name) if module.string_static_value(name).is_some() => {
            let value = module.string_static_value(name).expect("checked by guard");
            if value.is_ascii() {
                Some(value)
            } else {
                return Err(CompileError::new(
                    arg.span,
                    "wasm32-web output string builtin currently requires ASCII string values",
                ));
            }
        }
        ExprKind::FunctionCall { .. } if static_callback_function_name(arg, module).is_some() => {
            let value = evaluated_static_callback_function_name(arg, module)?
                .expect("checked by guard");
            if value.is_ascii() {
                Some(value)
            } else {
                return Err(CompileError::new(
                    arg.span,
                    "wasm32-web output string builtin currently requires ASCII string values",
                ));
            }
        }
        _ if matches!(
            lower_name.as_str(),
            "class_exists" | "interface_exists" | "trait_exists" | "enum_exists"
        ) =>
        {
            if let Some(var) = runtime_string_arg_or_materialize(arg, "type_exists_name", module)? {
                if let Some(autoload) = args.get(1) {
                    let kind = emit_expr(autoload, module)?;
                    emit_drop_existence_arg(kind, module);
                }
                emit_runtime_declared_type_exists(&lower_name, &var, module);
                return Ok(ValueKind::Bool);
            }
            Some(static_ascii_string_arg(call, arg, module)?)
        }
        _ if lower_name == "function_exists" => {
            if let Some(var) =
                runtime_string_arg_or_materialize(arg, "function_exists_name", module)?
            {
                emit_runtime_string_matches_any(
                    "function_exists",
                    callable_function_names(module),
                    &var,
                    module,
                );
                return Ok(ValueKind::Bool);
            }
            Some(static_ascii_string_arg(call, arg, module)?)
        }
        _ => Some(static_ascii_string_arg(call, arg, module)?),
    };
    if let Some(autoload) = args.get(1) {
        let kind = emit_expr(autoload, module)?;
        emit_drop_existence_arg(kind, module);
    }
    let value = value.expect("static existence names are present after non-runtime path");
    let exists = match name.to_ascii_lowercase().as_str() {
        "function_exists" => module.has_function(&value) || wasm_known_builtin_exists(&value),
        "class_exists" | "interface_exists" | "trait_exists" | "enum_exists" => {
            module.declared_type_exists(name, &value)
        }
        _ => unreachable!(),
    };
    module.body().line(&format!("i32.const {}", i32::from(exists)));
    Ok(ValueKind::Bool)
}

fn emit_runtime_declared_type_exists(kind: &str, var: &str, module: &mut WasmModule) {
    emit_runtime_string_matches_any("type_exists", module.declared_type_names(kind), var, module);
}

pub(super) fn emit_runtime_string_matches_any(
    prefix: &str,
    candidates: Vec<String>,
    var: &str,
    module: &mut WasmModule,
) {
    let exists = module.next_label(&format!("{}_result", prefix));
    module.declare_i32_local(exists.trim_start_matches('$').to_string());
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", exists));
    let done = module.next_label(&format!("{}_done", prefix));
    module.body().open(&format!("block {}", done));
    for candidate in candidates {
        emit_runtime_string_match_candidate(prefix, &candidate, var, &exists, &done, module);
    }
    module.body().close("end");
    module.body().line(&format!("local.get {}", exists));
}

fn emit_runtime_string_match_candidate(
    prefix: &str,
    candidate: &str,
    var: &str,
    exists: &str,
    done: &str,
    module: &mut WasmModule,
) {
    if !candidate.is_ascii() {
        return;
    }
    let candidate = candidate.to_ascii_lowercase();
    let index = module.next_label(&format!("{}_index", prefix));
    let byte = module.next_label(&format!("{}_byte", prefix));
    let expected = module.next_label(&format!("{}_expected", prefix));
    let matched = module.next_label(&format!("{}_match", prefix));
    let compare_done = module.next_label(&format!("{}_compare_done", prefix));
    let loop_label = module.next_label(&format!("{}_compare_loop", prefix));
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.declare_i32_local(byte.trim_start_matches('$').to_string());
    module.declare_i32_local(expected.trim_start_matches('$').to_string());
    module.declare_i32_local(matched.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line(&format!("i32.const {}", candidate.len()));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", matched));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", compare_done));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("i32.const {}", candidate.len()));
    module.body().line("i32.lt_u");
    module.body().open("if");
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.add");
    module.body().line("i32.load8_u");
    module.body().line(&format!("local.set {}", byte));
    emit_ascii_lower_byte(&byte, module);
    emit_candidate_byte_select(&candidate, &index, &expected, module);
    module.body().line(&format!("local.get {}", byte));
    module.body().line(&format!("local.get {}", expected));
    module.body().line("i32.ne");
    module.body().open("if");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", matched));
    module.body().line(&format!("br {}", compare_done));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", matched));
    module.body().open("if");
    module.body().line("i32.const 1");
    module.body().line(&format!("local.set {}", exists));
    module.body().line(&format!("br {}", done));
    module.body().close("end");
    module.body().close("end");
}

fn emit_ascii_lower_byte(byte: &str, module: &mut WasmModule) {
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 65");
    module.body().line("i32.ge_u");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 90");
    module.body().line("i32.le_u");
    module.body().line("i32.and");
    module.body().open("if");
    module.body().line(&format!("local.get {}", byte));
    module.body().line("i32.const 32");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", byte));
    module.body().close("end");
}

fn emit_candidate_byte_select(
    candidate: &str,
    index: &str,
    expected: &str,
    module: &mut WasmModule,
) {
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", expected));
    for (candidate_index, candidate_byte) in candidate.bytes().enumerate() {
        module.body().line(&format!("local.get {}", index));
        module.body().line(&format!("i32.const {}", candidate_index));
        module.body().line("i32.eq");
        module.body().open("if");
        module.body().line(&format!("i32.const {}", candidate_byte));
        module.body().line(&format!("local.set {}", expected));
        module.body().close("end");
    }
}

fn emit_drop_existence_arg(kind: ValueKind, module: &mut WasmModule) {
    match kind {
        ValueKind::Str | ValueKind::Array => {
            module.body().line("drop");
            module.body().line("drop");
        }
        ValueKind::Int
        | ValueKind::Float
        | ValueKind::Bool
        | ValueKind::Object
        | ValueKind::Callable
        | ValueKind::Mixed
        | ValueKind::Null => {
            module.body().line("drop");
        }
        ValueKind::Never => {}
    }
}

pub(super) fn emit_member_exists_call(
    call: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let [target, member] = args else {
        return Err(CompileError::new(
            call.span,
            &format!("wasm32-web {}() expects exactly two arguments", name),
        ));
    };
    let target = member_exists_class_target(target, module)?.ok_or_else(|| {
        CompileError::new(
            target.span,
            &format!(
                "wasm32-web {}() currently requires an object with known class metadata or a static class-string target",
                name
            ),
        )
    })?;
    if let Some(var) = runtime_string_arg_or_materialize(member, "member_exists_name", module)? {
        let candidates = match name.to_ascii_lowercase().as_str() {
            "method_exists" => runtime_visible_method_names(
                &target.class_name,
                target.object_target,
                module,
            ),
            "property_exists" => runtime_visible_property_names(&target.class_name, module),
            _ => unreachable!(),
        };
        emit_runtime_string_matches_any("member_exists", candidates, &var, module);
        return Ok(ValueKind::Bool);
    }
    let member_name = member_exists_member_name(call, member, module)?;
    let exists = match name.to_ascii_lowercase().as_str() {
        "method_exists" => {
            class_declares_runtime_visible_method(&target.class_name, &member_name, target.object_target, module)
        }
        "property_exists" => {
            class_declares_runtime_visible_property(&target.class_name, &member_name, module)
        }
        _ => unreachable!(),
    };
    module.body().line(&format!("i32.const {}", i32::from(exists)));
    Ok(ValueKind::Bool)
}

fn runtime_visible_method_names(
    class_name: &str,
    object_target: bool,
    module: &WasmModule,
) -> Vec<String> {
    let mut names = Vec::new();
    let mut current = Some(class_name.to_string());
    while let Some(candidate) = current {
        let Some(class_info) = module.object_class(&candidate) else {
            break;
        };
        names.extend(
            class_info
                .constructor
                .iter()
                .chain(class_info.methods.iter())
                .chain(class_info.static_methods.iter())
                .filter(|method| {
                    object_target
                        || method.owner_class.eq_ignore_ascii_case(class_name)
                        || method.visibility != Visibility::Private
                })
                .map(|method| method.name.clone()),
        );
        current = class_info.parent.clone();
    }
    names.sort_by_key(|name| name.to_ascii_lowercase());
    names.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    names
}

fn runtime_visible_property_names(class_name: &str, module: &WasmModule) -> Vec<String> {
    let mut names = Vec::new();
    let mut current = Some(class_name.to_string());
    while let Some(candidate) = current {
        let Some(class_info) = module.object_class(&candidate) else {
            break;
        };
        names.extend(
            class_info
                .properties
                .iter()
                .filter(|property| {
                    property.owner_class.eq_ignore_ascii_case(class_name)
                        || property.visibility != Visibility::Private
                })
                .map(|property| property.name.clone()),
        );
        names.extend(
            class_info
                .static_properties
                .iter()
                .filter(|property| {
                    property.owner_class.eq_ignore_ascii_case(class_name)
                        || property.visibility != Visibility::Private
                })
                .map(|property| property.name.clone()),
        );
        current = class_info.parent.clone();
    }
    names.sort_by_key(|name| name.to_ascii_lowercase());
    names.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    names
}

struct MemberExistsTarget {
    class_name: String,
    object_target: bool,
}

fn member_exists_class_target(
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<Option<MemberExistsTarget>, CompileError> {
    if let Some(class_name) = object_class_name_for_expr(expr, module) {
        return Ok(Some(MemberExistsTarget {
            class_name,
            object_target: true,
        }));
    }
    let Some(class_name) = (match &expr.kind {
        ExprKind::Variable(name) => module.string_static_value(name),
        _ if static_callback_function_name(expr, module).is_some() => {
            evaluated_static_callback_function_name(expr, module)?
        }
        _ => static_string_value(expr, module),
    }) else {
        return Ok(None);
    };
    Ok(Some(MemberExistsTarget {
        class_name,
        object_target: false,
    }))
}

fn member_exists_member_name(
    call: &Expr,
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    match &expr.kind {
        ExprKind::Variable(name) if module.string_static_value(name).is_some() => {
            Ok(module.string_static_value(name).expect("checked by guard"))
        }
        _ if static_callback_function_name(expr, module).is_some() => {
            let value = evaluated_static_callback_function_name(expr, module)?
                .expect("checked by guard");
            if value.is_ascii() {
                Ok(value)
            } else {
                Err(CompileError::new(
                    expr.span,
                    "wasm32-web output string builtin currently requires ASCII string values",
                ))
            }
        }
        _ => static_ascii_string_arg(call, expr, module),
    }
}

fn class_declares_runtime_visible_method(
    class_name: &str,
    method_name: &str,
    object_target: bool,
    module: &WasmModule,
) -> bool {
    let mut current = Some(class_name.to_string());
    while let Some(candidate) = current {
        let Some(class_info) = module.object_class(&candidate) else {
            return false;
        };
        if class_info
            .constructor
            .iter()
            .chain(class_info.methods.iter())
            .chain(class_info.static_methods.iter())
            .any(|method| {
                method.name.eq_ignore_ascii_case(method_name)
                    && (object_target
                        || method.owner_class.eq_ignore_ascii_case(class_name)
                        || method.visibility != Visibility::Private)
            })
        {
            return true;
        }
        current = class_info.parent.clone();
    }
    false
}

fn class_declares_runtime_visible_property(
    class_name: &str,
    property_name: &str,
    module: &WasmModule,
) -> bool {
    let mut current = Some(class_name.to_string());
    while let Some(candidate) = current {
        let Some(class_info) = module.object_class(&candidate) else {
            return false;
        };
        let matches_instance_property = class_info.properties.iter().any(|property| {
            property.name.eq_ignore_ascii_case(property_name)
                && (property.owner_class.eq_ignore_ascii_case(class_name)
                    || property.visibility != Visibility::Private)
        });
        let matches_static_property = class_info.static_properties.iter().any(|property| {
            property.name.eq_ignore_ascii_case(property_name)
                && (property.owner_class.eq_ignore_ascii_case(class_name)
                    || property.visibility != Visibility::Private)
        });
        if matches_instance_property || matches_static_property {
            return true;
        }
        current = class_info.parent.clone();
    }
    false
}

pub(super) fn wasm_known_builtin_exists(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "strlen"
            | "ord"
            | "strtolower"
            | "strtoupper"
            | "lcfirst"
            | "ucfirst"
            | "ucwords"
            | "strrev"
            | "trim"
            | "ltrim"
            | "rtrim"
            | "str_repeat"
            | "substr"
            | "chr"
            | "sprintf"
            | "printf"
            | "json_encode"
            | "json_validate"
            | "number_format"
            | "basename"
            | "dirname"
            | "pathinfo"
            | "addslashes"
            | "stripslashes"
            | "bin2hex"
            | "hex2bin"
            | "nl2br"
            | "urlencode"
            | "urldecode"
            | "rawurlencode"
            | "rawurldecode"
            | "base64_encode"
            | "base64_decode"
            | "htmlspecialchars"
            | "htmlentities"
            | "html_entity_decode"
            | "md5"
            | "sha1"
            | "hash"
            | "implode"
            | "str_replace"
            | "str_ireplace"
            | "strstr"
            | "wordwrap"
            | "strpos"
            | "strrpos"
            | "strcmp"
            | "strcasecmp"
            | "str_contains"
            | "str_starts_with"
            | "str_ends_with"
            | "ctype_alpha"
            | "ctype_digit"
            | "ctype_alnum"
            | "ctype_space"
            | "abs"
            | "intdiv"
            | "fdiv"
            | "min"
            | "max"
            | "intval"
            | "floatval"
            | "boolval"
            | "empty"
            | "is_numeric"
            | "is_nan"
            | "is_finite"
            | "is_infinite"
            | "is_int"
            | "is_float"
            | "is_bool"
            | "is_null"
            | "is_string"
            | "is_array"
            | "is_iterable"
            | "pi"
            | "floor"
            | "ceil"
            | "sqrt"
            | "pow"
            | "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "sinh"
            | "cosh"
            | "tanh"
            | "log"
            | "log10"
            | "exp"
            | "deg2rad"
            | "rad2deg"
            | "fmod"
            | "atan2"
            | "hypot"
            | "round"
    )
}

fn wasm_known_builtin_names() -> Vec<&'static str> {
    vec![
        "strlen",
        "ord",
        "strtolower",
        "strtoupper",
        "lcfirst",
        "ucfirst",
        "ucwords",
        "strrev",
        "trim",
        "ltrim",
        "rtrim",
        "str_repeat",
        "substr",
        "chr",
        "sprintf",
        "printf",
        "json_encode",
        "json_validate",
        "number_format",
        "basename",
        "dirname",
        "pathinfo",
        "addslashes",
        "stripslashes",
        "bin2hex",
        "hex2bin",
        "nl2br",
        "urlencode",
        "urldecode",
        "rawurlencode",
        "rawurldecode",
        "base64_encode",
        "base64_decode",
        "htmlspecialchars",
        "htmlentities",
        "html_entity_decode",
        "md5",
        "sha1",
        "hash",
        "implode",
        "str_replace",
        "str_ireplace",
        "strstr",
        "wordwrap",
        "strpos",
        "strrpos",
        "strcmp",
        "strcasecmp",
        "str_contains",
        "str_starts_with",
        "str_ends_with",
        "ctype_alpha",
        "ctype_digit",
        "ctype_alnum",
        "ctype_space",
        "abs",
        "intdiv",
        "fdiv",
        "min",
        "max",
        "intval",
        "floatval",
        "boolval",
        "empty",
        "is_numeric",
        "is_nan",
        "is_finite",
        "is_infinite",
        "is_int",
        "is_float",
        "is_bool",
        "is_null",
        "is_string",
        "is_array",
        "is_iterable",
        "pi",
        "floor",
        "ceil",
        "sqrt",
        "pow",
        "sin",
        "cos",
        "tan",
        "asin",
        "acos",
        "atan",
        "sinh",
        "cosh",
        "tanh",
        "log",
        "log10",
        "exp",
        "deg2rad",
        "rad2deg",
        "fmod",
        "atan2",
        "hypot",
        "round",
    ]
}

pub(super) fn emit_is_callable_call(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let [arg] = args else {
        return Err(CompileError::new(
            call.span,
            "wasm32-web is_callable() expects exactly one argument",
        ));
    };
    let result = match &arg.kind {
        ExprKind::Variable(name)
            if module.callable_target(name).is_some()
                || module.callable_instance_target(name).is_some()
                || object_expr_is_invokable(module, arg) =>
        {
            true
        }
        ExprKind::ArrayLiteral(items) if fixed_callable_array_literal_exists(items, module).is_some() => {
            fixed_callable_array_literal_exists(items, module).expect("checked by guard")
        }
        ExprKind::ArrayLiteralAssoc(items)
            if fixed_callable_assoc_array_literal_exists(items, module).is_some() =>
        {
            fixed_callable_assoc_array_literal_exists(items, module).expect("checked by guard")
        }
        ExprKind::NewObject { .. } | ExprKind::NewScopedObject { .. }
            if object_expr_is_invokable(module, arg) =>
        {
            true
        }
        ExprKind::Variable(_) if object_class_name_for_expr(arg, module).is_some() => false,
        ExprKind::NewObject { .. } | ExprKind::NewScopedObject { .. } => false,
        ExprKind::FirstClassCallable(CallableTarget::Method { object, method })
            if direct_instance_method_callable_exists(object, method, module) =>
        {
            true
        }
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method })
            if direct_static_method_callable_exists(receiver, method, module) =>
        {
            true
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } if direct_static_method_callable_ternary_exists(then_expr, else_expr, module) => {
            emit_condition(condition, module)?;
            module.body().line("drop");
            true
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } if direct_instance_method_callable_ternary_exists(then_expr, else_expr, module)? => {
            emit_condition(condition, module)?;
            module.body().line("drop");
            true
        }
        ExprKind::Variable(name) if module.string_static_value(name).is_some() => {
            let value = module.string_static_value(name).expect("checked by guard");
            module.has_function(&value) || wasm_known_builtin_exists(&value)
        }
        _ if static_callback_function_name(arg, module).is_some() => {
            let value = evaluated_static_callback_function_name(arg, module)?
                .expect("checked by guard");
            if !value.is_ascii() {
                return Err(CompileError::new(
                    arg.span,
                    "wasm32-web output string builtin currently requires ASCII string values",
                ));
            }
            module.has_function(&value) || wasm_known_builtin_exists(&value)
        }
        _ if callable_return_expr_targets(arg, module).is_some() => {
            let kind = emit_expr(arg, module)?;
            if kind != ValueKind::Callable {
                return Err(CompileError::new(
                    arg.span,
                    "wasm32-web is_callable() expected a callable descriptor",
                ));
            }
            module.body().line("drop");
            true
        }
        ExprKind::StringLiteral(value) => module.has_function(value) || wasm_known_builtin_exists(value),
        ExprKind::BoolLiteral(_) | ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) | ExprKind::Null => false,
        _ => {
            if let Some(var) = runtime_string_arg_or_materialize(arg, "is_callable_name", module)? {
                emit_runtime_string_matches_any(
                    "is_callable",
                    callable_function_names(module),
                    &var,
                    module,
                );
                return Ok(ValueKind::Bool);
            }
            return Err(CompileError::new(
                arg.span,
                "wasm32-web is_callable() currently supports literal scalar arguments",
            ));
        }
    };
    module.body().line(&format!("i32.const {}", i32::from(result)));
    Ok(ValueKind::Bool)
}

fn callable_function_names(module: &WasmModule) -> Vec<String> {
    let mut names = module.declared_function_names();
    names.extend(wasm_known_builtin_names().into_iter().map(str::to_string));
    names.sort();
    names.dedup();
    names
}

fn fixed_callable_array_literal_exists(items: &[Expr], module: &WasmModule) -> Option<bool> {
    let [receiver, method] = items else {
        return Some(false);
    };
    fixed_callable_pair_exists(receiver, method, module)
}

fn fixed_callable_assoc_array_literal_exists(
    items: &[(Expr, Expr)],
    module: &WasmModule,
) -> Option<bool> {
    let receiver = fixed_callable_assoc_value(items, 0, module)?;
    let method = fixed_callable_assoc_value(items, 1, module)?;
    fixed_callable_pair_exists(receiver, method, module)
}

fn fixed_callable_assoc_value<'a>(
    items: &'a [(Expr, Expr)],
    needle: i64,
    module: &WasmModule,
) -> Option<&'a Expr> {
    items
        .iter()
        .rev()
        .find_map(|(key, value)| {
            matches!(
                static_assoc_access_key(key, module),
                Some(AssocKeyValue::Int(value)) if value == needle
            )
            .then_some(value)
        })
}

fn fixed_callable_pair_exists(
    receiver: &Expr,
    method: &Expr,
    module: &WasmModule,
) -> Option<bool> {
    let method = fixed_callable_static_string_value(method, module)?;
    if object_class_name_for_expr(receiver, module).is_some() {
        return Some(direct_instance_method_callable_exists(receiver, &method, module));
    }
    let class_name = fixed_callable_static_string_value(receiver, module)?;
    Some(fixed_static_method_callable_name_exists(&class_name, &method, module))
}

fn fixed_callable_static_string_value(expr: &Expr, module: &WasmModule) -> Option<String> {
    match &expr.kind {
        ExprKind::Variable(name) => module.string_static_value(name),
        _ => static_string_value(expr, module),
    }
}

fn fixed_static_method_callable_name_exists(
    class_name: &str,
    method: &str,
    module: &WasmModule,
) -> bool {
    let Some(method_info) = module.object_static_method_in_hierarchy(class_name, method) else {
        return false;
    };
    module.object_member_is_accessible(&method_info.owner_class, &method_info.visibility)
}

fn direct_instance_method_callable_ternary_exists(
    then_expr: &Expr,
    else_expr: &Expr,
    module: &WasmModule,
) -> Result<bool, CompileError> {
    let Some((then_object, then_method)) = direct_instance_method_callable_parts(then_expr) else {
        return Ok(false);
    };
    let Some((else_object, else_method)) = direct_instance_method_callable_parts(else_expr) else {
        return Ok(false);
    };
    Ok(direct_instance_method_callable_exists(then_object, then_method, module)
        && direct_instance_method_callable_exists(else_object, else_method, module))
}

fn direct_instance_method_callable_parts(expr: &Expr) -> Option<(&Expr, &str)> {
    match &expr.kind {
        ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) => {
            Some((object, method))
        }
        _ => None,
    }
}

fn direct_static_method_callable_ternary_exists(
    then_expr: &Expr,
    else_expr: &Expr,
    module: &WasmModule,
) -> bool {
    let Some((then_receiver, then_method)) = direct_static_method_callable_parts(then_expr) else {
        return false;
    };
    let Some((else_receiver, else_method)) = direct_static_method_callable_parts(else_expr) else {
        return false;
    };
    direct_static_method_callable_exists(then_receiver, then_method, module)
        && direct_static_method_callable_exists(else_receiver, else_method, module)
}

fn direct_static_method_callable_parts(expr: &Expr) -> Option<(&StaticReceiver, &str)> {
    match &expr.kind {
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            Some((receiver, method))
        }
        _ => None,
    }
}

fn direct_static_method_callable_exists(
    receiver: &StaticReceiver,
    method: &str,
    module: &WasmModule,
) -> bool {
    let StaticReceiver::Named(class_name) = receiver else {
        return false;
    };
    let Some(method_info) = module.object_static_method_in_hierarchy(class_name.as_str(), method)
    else {
        return false;
    };
    module.object_member_is_accessible(&method_info.owner_class, &method_info.visibility)
}

fn direct_instance_method_callable_exists(object: &Expr, method: &str, module: &WasmModule) -> bool {
    if !matches!(object.kind, ExprKind::Variable(_)) {
        return false;
    }
    let Some(class_name) = object_class_name_for_expr(object, module) else {
        return false;
    };
    let Some((_, method_info)) = module.object_method_in_hierarchy(&class_name, method) else {
        return false;
    };
    module.object_member_is_accessible(&method_info.owner_class, &method_info.visibility)
}

pub(super) fn emit_isset_call(
    call: &Expr,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if args.is_empty() {
        return Err(CompileError::new(
            call.span,
            "wasm32-web isset() expects at least one argument",
        ));
    }
    let mut first = true;
    for arg in args {
        emit_isset_arg(arg, module)?;
        if !first {
            module.body().line("i32.and");
        }
        first = false;
    }
    Ok(ValueKind::Bool)
}

fn emit_isset_arg(arg: &Expr, module: &mut WasmModule) -> Result<(), CompileError> {
    match &arg.kind {
        ExprKind::Variable(name) => {
            let exists = module.local_kind(name).is_some();
            module
                .body()
                .line(&format!("i32.const {}", i32::from(exists)));
            Ok(())
        }
        ExprKind::PropertyAccess { object, property } => {
            emit_object_property_isset_expr(arg, object, property, module)?;
            Ok(())
        }
        ExprKind::NullsafePropertyAccess { object, property }
            if object_expr_is_known_non_null(object, module) =>
        {
            emit_object_property_isset_expr(arg, object, property, module)?;
            Ok(())
        }
        ExprKind::NullsafePropertyAccess { object, .. } if matches!(object.kind, ExprKind::Null) => {
            module.body().line("i32.const 0");
            Ok(())
        }
        ExprKind::DynamicPropertyAccess { object, property } => {
            emit_dynamic_object_property_isset_expr(arg, object, property, module)?;
            Ok(())
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            emit_static_property_isset_expr(arg, receiver, property, module)?;
            Ok(())
        }
        ExprKind::ArrayAccess { array, index } => {
            emit_array_offset_isset_expr(arg, array, index, module)?;
            Ok(())
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property }
            if object_expr_is_known_non_null(object, module) =>
        {
            emit_dynamic_object_property_isset_expr(arg, object, property, module)?;
            Ok(())
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, .. } if matches!(object.kind, ExprKind::Null) => {
            module.body().line("i32.const 0");
            Ok(())
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            emit_nullsafe_mixed_object_dynamic_property_isset_expr(arg, object, property, module)?;
            Ok(())
        }
        ExprKind::NullsafePropertyAccess { .. } => {
            Err(CompileError::new(
                arg.span,
                "wasm32-web nullable nullsafe isset requires object/null runtime metadata",
            ))
        }
        _ => Err(CompileError::new(
            arg.span,
            "wasm32-web isset() currently supports scalar locals, fixed object properties, and supported array offsets only",
        )),
    }
}

fn emit_array_offset_isset_expr(
    arg: &Expr,
    array: &Expr,
    index: &Expr,
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if let Some(kind) = nested_array_static_access_kind(array, index, module) {
        emit_static_isset_result_for_value_cell_kind(kind, module);
        return Ok(());
    }
    if let Some(kind) = static_array_offset_isset_value_cell_kind(array, index, module) {
        emit_static_isset_result_for_value_cell_kind(kind, module);
        return Ok(());
    }
    emit_isset_value_expr(arg, module)
}

fn static_array_offset_isset_value_cell_kind(
    array: &Expr,
    index: &Expr,
    module: &WasmModule,
) -> Option<ValueCellKind> {
    match &array.kind {
        ExprKind::ConstRef(name) => match module.array_constant_value(name)? {
            ConstantArrayValue::Indexed(items) => {
                let offset = usize::try_from(static_or_const_or_i64_local_value(index, module)?).ok()?;
                Some(
                    items
                        .get(offset)
                        .and_then(|item| value_cell_kind_for_expr(item, module))
                        .unwrap_or(ValueCellKind::Null),
                )
            }
            ConstantArrayValue::Assoc(items) => {
                let key = static_isset_assoc_access_key(index, module)?;
                Some(
                    items
                        .iter()
                        .rev()
                        .find_map(|(candidate, value)| {
                            let candidate = static_isset_assoc_access_key(candidate, module)?;
                            (candidate == key).then(|| {
                                value_cell_kind_for_expr(value, module).unwrap_or(ValueCellKind::Null)
                            })
                        })
                        .unwrap_or(ValueCellKind::Null),
                )
            }
        },
        ExprKind::ArrayLiteral(items) => {
            let offset = usize::try_from(static_or_const_or_i64_local_value(index, module)?).ok()?;
            Some(
                items
                    .get(offset)
                    .and_then(|item| value_cell_kind_for_expr(item, module))
                    .unwrap_or(ValueCellKind::Null),
            )
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            let key = static_isset_assoc_access_key(index, module)?;
            Some(
                items
                    .iter()
                    .rev()
                    .find_map(|(candidate, value)| {
                        let candidate = static_isset_assoc_access_key(candidate, module)?;
                        (candidate == key).then(|| {
                            value_cell_kind_for_expr(value, module).unwrap_or(ValueCellKind::Null)
                        })
                    })
                    .unwrap_or(ValueCellKind::Null),
            )
        }
        ExprKind::Variable(name) if module.local_kind(name) == Some(LocalKind::Array) => {
            match module.array_layout(name) {
                ArrayLayout::Assoc => {
                    let key = static_isset_assoc_access_key(index, module)?;
                    assoc_value_kind_for_static_key(name, &key, module)
                }
                ArrayLayout::Value => {
                    let offset = usize::try_from(static_or_const_or_i64_local_value(index, module)?).ok()?;
                    Some(module.array_value_cell_kind(name, offset).unwrap_or(ValueCellKind::Null))
                }
                ArrayLayout::CompactInt => {
                    let offset = usize::try_from(static_or_const_or_i64_local_value(index, module)?).ok()?;
                    Some(
                        module
                            .array_length(name)
                            .is_some_and(|len| offset < len)
                            .then_some(ValueCellKind::Int)
                            .unwrap_or(ValueCellKind::Null),
                    )
                }
            }
        }
        _ => None,
    }
}

fn static_isset_assoc_access_key(index: &Expr, module: &WasmModule) -> Option<AssocKeyValue> {
    static_assoc_access_key(index, module).or_else(|| match &index.kind {
        ExprKind::Variable(name) => module.string_static_value(name).map(AssocKeyValue::Str),
        _ => None,
    })
}

fn emit_static_isset_result_for_value_cell_kind(kind: ValueCellKind, module: &mut WasmModule) {
    module
        .body()
        .line(&format!("i32.const {}", i32::from(kind != ValueCellKind::Null)));
}

fn emit_isset_value_expr(arg: &Expr, module: &mut WasmModule) -> Result<(), CompileError> {
    let kind = emit_expr(arg, module)?;
    match kind {
        ValueKind::Mixed => {
            module.body().line("call $__rt_mixed_tag");
            module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
            module.body().line("i32.ne");
        }
        ValueKind::Null => {
            module.body().line("drop");
            module.body().line("i32.const 0");
        }
        _ => {
            emit_drop_isset_value(kind, module);
            module.body().line("i32.const 1");
        }
    }
    Ok(())
}

fn emit_drop_isset_value(kind: ValueKind, module: &mut WasmModule) {
    match kind {
        ValueKind::Str | ValueKind::Array => {
            module.body().line("drop");
            module.body().line("drop");
        }
        ValueKind::Int
        | ValueKind::Float
        | ValueKind::Bool
        | ValueKind::Object
        | ValueKind::Callable
        | ValueKind::Mixed
        | ValueKind::Null => {
            module.body().line("drop");
        }
        ValueKind::Never => {}
    }
}
