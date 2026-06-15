//! Purpose:
//! Infers generic expression result local kinds for wasm32-web metadata.
//! Keeps broad scalar, builtin, callable, and array result classification out of the statement collector.
//!
//! Called from:
//! - `super::collect_stmt_locals()` and sibling metadata helpers through `super::infer_local_kind()`.
//!
//! Key details:
//! - This is metadata-only inference; unsupported or unknown expressions stay conservative.
//! - Recursive branch and callable inference must preserve the existing boxed/mixed local-kind contract.

use super::*;
use crate::span::Span;

pub(in crate::codegen::wasm::module) fn infer_local_kind(
    expr: &Expr,
    locals: &HashMap<String, LocalKind>,
    function_return_kinds: &HashMap<String, ValueKind>,
    constants: &HashMap<String, ConstantValue>,
    class_constants: &HashMap<String, ConstantValue>,
) -> LocalKind {
    match &expr.kind {
        ExprKind::StringLiteral(_) => LocalKind::Str,
        ExprKind::NullsafePropertyAccess { .. } | ExprKind::NullsafeDynamicPropertyAccess { .. } => {
            LocalKind::Mixed
        }
        ExprKind::NullsafeMethodCall { object, .. } if matches!(object.kind, ExprKind::Null) =>
        {
            LocalKind::Mixed
        }
        ExprKind::NullsafeDynamicMethodCall { object, .. } if matches!(object.kind, ExprKind::Null) =>
        {
            LocalKind::Mixed
        }
        ExprKind::NewObject { .. } | ExprKind::NewScopedObject { .. } => LocalKind::Object,
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => LocalKind::Array,
        ExprKind::FunctionCall { name, .. }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "explode" | "str_split" | "array_fill" | "array_fill_keys" | "array_combine"
                    | "array_column" | "class_parents" | "class_implements" | "class_uses"
                    | "class_attribute_names"
            ) =>
        {
            LocalKind::Array
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("shuffle") => {
            LocalKind::I32
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("array_reduce") => {
            array_reduce_static_local_kind(
                args,
                locals,
                function_return_kinds,
                constants,
                class_constants,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("call_user_func") => {
            call_user_func_local_kind(
                args,
                locals,
                &HashMap::new(),
                &HashMap::new(),
                &HashMap::new(),
                function_return_kinds,
                constants,
                class_constants,
            )
            .unwrap_or(LocalKind::I64)
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("call_user_func_array") => {
            call_user_func_array_local_kind(
                args,
                locals,
                &HashMap::new(),
                &HashMap::new(),
                &HashMap::new(),
                &HashMap::new(),
                function_return_kinds,
                constants,
                class_constants,
            )
            .unwrap_or(LocalKind::I64)
        }
        ExprKind::FloatLiteral(_) => LocalKind::F64,
        ExprKind::BoolLiteral(_) | ExprKind::Not(_) => LocalKind::I32,
        ExprKind::BitNot(_) => LocalKind::I64,
        ExprKind::Print(_) => LocalKind::I64,
        ExprKind::FirstClassCallable(_)
        | ExprKind::Closure { .. }
        | ExprKind::ClosureCall { .. } => LocalKind::Callable,
        ExprKind::Pipe { value, callable } => infer_pipe_local_kind(
            value,
            callable,
            expr.span,
            locals,
            function_return_kinds,
            constants,
            class_constants,
        ),
        ExprKind::Negate(inner) => {
            infer_local_kind(
                inner,
                locals,
                function_return_kinds,
                constants,
                class_constants,
            )
        }
        ExprKind::Cast { target, .. } => match target {
            CastType::Int => LocalKind::I64,
            CastType::Float => LocalKind::F64,
            CastType::Bool => LocalKind::I32,
            CastType::String => LocalKind::Str,
            CastType::Array => LocalKind::I64,
        },
        ExprKind::Variable(name) => locals.get(name).copied().unwrap_or(LocalKind::I64),
        ExprKind::ArrayAccess { array, index }
            if pathinfo_direct_array_access_value_kind(array, index) == Some(ValueCellKind::Str) =>
        {
            LocalKind::Str
        }
        ExprKind::ArrayAccess { array, .. }
            if infer_local_kind(
                array,
                locals,
                function_return_kinds,
                constants,
                class_constants,
            ) == LocalKind::Str =>
        {
            LocalKind::Str
        }
        ExprKind::ConstRef(name) => constants
            .get(name.as_str())
            .map(local_kind_for_constant)
            .unwrap_or(LocalKind::I64),
        ExprKind::ClassConstant { .. } => LocalKind::Str,
        ExprKind::ScopedConstantAccess { receiver, name } => {
            let Some(class_name) = (match receiver {
                StaticReceiver::Named(class_name) => Some(class_name.as_str()),
                StaticReceiver::Self_ | StaticReceiver::Static | StaticReceiver::Parent => {
                    None
                }
            }) else {
                return LocalKind::I64;
            };
            class_constants
                .get(&class_const_key(class_name, name))
                .map(local_kind_for_constant)
                .unwrap_or(LocalKind::I64)
        }
        ExprKind::StaticMethodCall {
            receiver: StaticReceiver::Named(class_name),
            method,
            ..
        } => {
            if let Some(kind) = function_return_kinds
                .get(&static_method_call_return_key(class_name.as_str(), method))
                .copied()
                .map(local_kind_for_value)
            {
                return kind;
            }
            if method.eq_ignore_ascii_case("cases") {
                return LocalKind::Array;
            }
            if method.eq_ignore_ascii_case("from") || method.eq_ignore_ascii_case("tryFrom") {
                return LocalKind::Object;
            }
            LocalKind::I64
        }
        ExprKind::MethodCall { object, method, .. } => {
            let Some(class_name) = (match &object.kind {
                ExprKind::NewObject { class_name, .. } => Some(class_name.as_str()),
                _ => None,
            }) else {
                return unknown_receiver_method_local_kind(method, function_return_kinds);
            };
            function_return_kinds
                .get(&method_call_return_key(class_name, method))
                .copied()
                .map(local_kind_for_value)
                .unwrap_or(LocalKind::I64)
        }
        ExprKind::DynamicMethodCall { object, method, .. } => {
            let Some(method) = static_dynamic_method_name(method, locals) else {
                return LocalKind::Mixed;
            };
            let Some(class_name) = (match &object.kind {
                ExprKind::NewObject { class_name, .. } => Some(class_name.as_str()),
                _ => None,
            }) else {
                return unknown_receiver_method_local_kind(&method, function_return_kinds);
            };
            function_return_kinds
                .get(&method_call_return_key(class_name, &method))
                .copied()
                .map(local_kind_for_value)
                .unwrap_or(LocalKind::I64)
        }
        ExprKind::NullsafeMethodCall { object, method, .. } => {
            let Some(class_name) = (match &object.kind {
                ExprKind::NewObject { class_name, .. } => Some(class_name.as_str()),
                _ => None,
            }) else {
                return LocalKind::Mixed;
            };
            function_return_kinds
                .get(&method_call_return_key(class_name, method))
                .copied()
                .map(local_kind_for_value)
                .unwrap_or(LocalKind::Mixed)
        }
        ExprKind::NullsafeDynamicMethodCall { object, method, .. } => {
            let Some(method) = static_dynamic_method_name(method, locals) else {
                return LocalKind::Mixed;
            };
            let Some(class_name) = (match &object.kind {
                ExprKind::NewObject { class_name, .. } => Some(class_name.as_str()),
                _ => None,
            }) else {
                return LocalKind::Mixed;
            };
            function_return_kinds
                .get(&method_call_return_key(class_name, &method))
                .copied()
                .map(local_kind_for_value)
                .unwrap_or(LocalKind::Mixed)
        }
        ExprKind::ExprCall { callee, .. } => {
            let Some(class_name) = (match &callee.kind {
                ExprKind::NewObject { class_name, .. } => Some(class_name.as_str()),
                _ => None,
            }) else {
                return unknown_receiver_method_local_kind("__invoke", function_return_kinds);
            };
            function_return_kinds
                .get(&method_call_return_key(class_name, "__invoke"))
                .copied()
                .map(local_kind_for_value)
                .unwrap_or(LocalKind::I64)
        }
        ExprKind::BinaryOp { op, left, right } => match op {
            crate::parser::ast::BinOp::Concat => LocalKind::Str,
            crate::parser::ast::BinOp::Div | crate::parser::ast::BinOp::Pow => LocalKind::F64,
            crate::parser::ast::BinOp::And
            | crate::parser::ast::BinOp::Or
            | crate::parser::ast::BinOp::Xor
            | crate::parser::ast::BinOp::Eq
            | crate::parser::ast::BinOp::NotEq
            | crate::parser::ast::BinOp::StrictEq
            | crate::parser::ast::BinOp::StrictNotEq
            | crate::parser::ast::BinOp::Lt
            | crate::parser::ast::BinOp::Gt
            | crate::parser::ast::BinOp::LtEq
            | crate::parser::ast::BinOp::GtEq => LocalKind::I32,
            _ => match (
                infer_local_kind(
                    left,
                    locals,
                    function_return_kinds,
                    constants,
                    class_constants,
                ),
                infer_local_kind(
                    right,
                    locals,
                    function_return_kinds,
                    constants,
                    class_constants,
                ),
            ) {
                (LocalKind::F64, _) | (_, LocalKind::F64) => LocalKind::F64,
                (LocalKind::I32, LocalKind::I32) => LocalKind::I32,
                _ => LocalKind::I64,
            },
        },
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => infer_branch_local_kind(
            then_expr,
            else_expr,
            locals,
            function_return_kinds,
            constants,
            class_constants,
        ),
        ExprKind::ShortTernary { value, default } => {
            infer_branch_local_kind(
                value,
                default,
                locals,
                function_return_kinds,
                constants,
                class_constants,
            )
        }
        ExprKind::NullCoalesce { value, default } => {
            if matches!(value.kind, ExprKind::Null) {
                infer_local_kind(
                    default,
                    locals,
                    function_return_kinds,
                    constants,
                    class_constants,
                )
            } else {
                infer_local_kind(value, locals, function_return_kinds, constants, class_constants)
            }
        }
        ExprKind::Match { arms, default, .. } => {
            let values = arms
                .iter()
                .map(|(_, value)| value)
                .chain(default.as_deref())
                .collect::<Vec<_>>();
            infer_many_local_kind(
                &values,
                locals,
                function_return_kinds,
                constants,
                class_constants,
            )
        }
        ExprKind::FunctionCall { name, args } if name.eq_ignore_ascii_case("abs") => args
            .first()
            .map(|arg| {
                infer_local_kind(arg, locals, function_return_kinds, constants, class_constants)
            })
            .unwrap_or(LocalKind::I64),
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("min") || name.eq_ignore_ascii_case("max") =>
        {
            if args
                .iter()
                .any(|arg| {
                    infer_local_kind(
                        arg,
                        locals,
                        function_return_kinds,
                        constants,
                        class_constants,
                    )
                        == LocalKind::F64
                })
            {
                LocalKind::F64
            } else {
                LocalKind::I64
            }
        }
        ExprKind::FunctionCall { name, .. }
            if name.eq_ignore_ascii_case("strlen") || name.eq_ignore_ascii_case("intdiv") =>
        {
            LocalKind::I64
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("intval") => {
            LocalKind::I64
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("floatval") => {
            LocalKind::F64
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("fdiv") => {
            LocalKind::F64
        }
        ExprKind::FunctionCall { name, args }
            if name.eq_ignore_ascii_case("pathinfo") && pathinfo_call_returns_array(args) =>
        {
            LocalKind::Array
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("class_parents") => {
            LocalKind::Array
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("class_implements") => {
            LocalKind::Array
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("class_uses") => {
            LocalKind::Array
        }
        ExprKind::FunctionCall { name, .. }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "floor"
                    | "ceil"
                    | "sqrt"
                    | "pi"
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
                    | "pow"
                    | "fmod"
                    | "atan2"
                    | "hypot"
                    | "round"
            ) =>
        {
            LocalKind::F64
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("boolval") => {
            LocalKind::I32
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("empty") => {
            LocalKind::I32
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("is_numeric") => {
            LocalKind::I32
        }
        ExprKind::FunctionCall { name, .. }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "is_nan" | "is_finite" | "is_infinite"
            ) =>
        {
            LocalKind::I32
        }
        ExprKind::FunctionCall { name, .. }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "is_int"
                    | "is_float"
                    | "is_bool"
                    | "is_null"
                    | "is_string"
                    | "is_array"
                    | "is_iterable"
                    | "is_callable"
                    | "is_a"
                    | "is_subclass_of"
                    | "isset"
                    | "json_validate"
                    | "function_exists"
                    | "class_exists"
                    | "interface_exists"
                    | "trait_exists"
                    | "enum_exists"
                    | "str_contains"
                    | "str_starts_with"
                    | "str_ends_with"
                    | "array_walk"
                    | "usort"
                    | "uasort"
                    | "uksort"
                    | "sort"
                    | "rsort"
                    | "ctype_alpha"
                    | "ctype_digit"
                    | "ctype_alnum"
                    | "ctype_space"
            ) =>
        {
            LocalKind::I32
        }
        ExprKind::FunctionCall { name, .. }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "range"
                    | "array_values"
                    | "array_reverse"
                    | "array_keys"
                    | "array_unique"
                    | "array_flip"
                    | "array_diff"
                    | "array_intersect"
                    | "array_diff_key"
                    | "array_intersect_key"
                    | "array_merge"
                    | "array_slice"
                    | "array_splice"
                    | "array_chunk"
                    | "array_pad"
                    | "array_map"
                    | "array_filter"
                    | "array_fill"
            ) =>
        {
            LocalKind::Array
        }
        ExprKind::FunctionCall { name, .. }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "strtolower"
                    | "strtoupper"
                    | "lcfirst"
                    | "ucfirst"
                    | "ucwords"
                    | "strrev"
                    | "addslashes"
                    | "stripslashes"
                    | "bin2hex"
                    | "hex2bin"
                    | "nl2br"
                    | "trim"
                    | "ltrim"
                    | "rtrim"
                    | "str_repeat"
                    | "chr"
                    | "str_pad"
                    | "substr"
                    | "substr_replace"
                    | "str_replace"
                    | "str_ireplace"
                    | "strstr"
                    | "wordwrap"
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
                    | "number_format"
                    | "sprintf"
                    | "basename"
                    | "dirname"
                    | "json_encode"
                    | "json_last_error_msg"
                    | "get_parent_class"
            ) =>
        {
            LocalKind::Str
        }
        ExprKind::FunctionCall { name, .. }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "ord" | "strcmp" | "strcasecmp"
            ) =>
        {
            LocalKind::I64
        }
        ExprKind::FunctionCall { name, .. }
            if matches!(name.to_ascii_lowercase().as_str(), "strpos" | "strrpos") =>
        {
            LocalKind::Mixed
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("get_class") => {
            LocalKind::Str
        }
        ExprKind::FunctionCall { name, .. } if name.eq_ignore_ascii_case("array_search") => {
            LocalKind::Mixed
        }
        ExprKind::FunctionCall { name, .. } => function_return_kinds
            .get(&function_key(name.as_str()))
            .copied()
            .map(local_kind_for_value)
            .unwrap_or(LocalKind::I64),
        _ => LocalKind::I64,
    }
}

fn infer_pipe_local_kind(
    value: &Expr,
    callable: &Expr,
    span: Span,
    locals: &HashMap<String, LocalKind>,
    function_return_kinds: &HashMap<String, ValueKind>,
    constants: &HashMap<String, ConstantValue>,
    class_constants: &HashMap<String, ConstantValue>,
) -> LocalKind {
    let synth_args = vec![value.clone()];
    let synthetic = match &callable.kind {
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => Expr::new(
            ExprKind::FunctionCall {
                name: name.clone(),
                args: synth_args,
            },
            span,
        ),
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            Expr::new(
                ExprKind::StaticMethodCall {
                    receiver: receiver.clone(),
                    method: method.clone(),
                    args: synth_args,
                },
                span,
            )
        }
        ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) => Expr::new(
            ExprKind::MethodCall {
                object: object.clone(),
                method: method.clone(),
                args: synth_args,
            },
            span,
        ),
        ExprKind::Variable(var) => Expr::new(
            ExprKind::ClosureCall {
                var: var.clone(),
                args: synth_args,
            },
            span,
        ),
        _ => Expr::new(
            ExprKind::ExprCall {
                callee: Box::new(callable.clone()),
                args: synth_args,
            },
            span,
        ),
    };
    infer_local_kind(
        &synthetic,
        locals,
        function_return_kinds,
        constants,
        class_constants,
    )
}

fn static_dynamic_method_name(
    method: &Expr,
    locals: &HashMap<String, LocalKind>,
) -> Option<String> {
    match &method.kind {
        ExprKind::StringLiteral(value) => Some(value.clone()),
        ExprKind::Variable(name) if locals.get(name) == Some(&LocalKind::Str) => {
            // The exact string is tracked by codegen state at emission time; metadata can
            // only safely use literal names here.
            None
        }
        _ => None,
    }
}

pub(in crate::codegen::wasm::module) fn unknown_receiver_method_local_kind(
    method: &str,
    function_return_kinds: &HashMap<String, ValueKind>,
) -> LocalKind {
    let suffix = format!("->{}", function_key(method));
    let mut kind = None;
    for (key, value_kind) in function_return_kinds {
        if !key.ends_with(&suffix) {
            continue;
        }
        let candidate = local_kind_for_value(*value_kind);
        if kind.is_some_and(|existing| existing != candidate) {
            return LocalKind::I64;
        }
        kind = Some(candidate);
    }
    kind.unwrap_or(LocalKind::I64)
}

fn array_reduce_static_local_kind(
    args: &[Expr],
    locals: &HashMap<String, LocalKind>,
    function_return_kinds: &HashMap<String, ValueKind>,
    constants: &HashMap<String, ConstantValue>,
    class_constants: &HashMap<String, ConstantValue>,
) -> LocalKind {
    if let Some(callback) = args.get(1).and_then(static_callback_name_for_locals) {
        if let Some(kind) = array_map_callback_return_kind(&callback, function_return_kinds) {
            return local_kind_for_value(kind);
        }
    }
    args.get(2)
        .map(|initial| {
            infer_local_kind(
                initial,
                locals,
                function_return_kinds,
                constants,
                class_constants,
            )
        })
        .filter(|kind| matches!(kind, LocalKind::Str | LocalKind::I32 | LocalKind::F64))
        .unwrap_or(LocalKind::I64)
}
