//! Purpose:
//! Emits wasm32-web numeric array folds for `array_sum()` and `array_product()`.
//!
//! Called from:
//! - `super::array_aggregates` through its public re-export.
//!
//! Key details:
//! - Static constants use PHP-compatible scalar numeric coercions.
//! - Runtime paths preserve the boxed value-cell contract for value and assoc arrays.

use super::*;

pub(super) fn emit_numeric_array_fold_call(
    expr: &Expr,
    name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if args.len() != 1 {
        return Err(CompileError::new(
            expr.span,
            &format!("wasm32-web {name}() expects exactly one argument"),
        ));
    }
    let product = name.eq_ignore_ascii_case("array_product");
    match &args[0].kind {
        ExprKind::ArrayLiteral(items) => {
            if let Some(kind) = emit_static_array_fold(items, product, name, expr, module)? {
                return Ok(kind);
            }
            module.body().line(&format!("i64.const {}", if product { 1 } else { 0 }));
            for item in items {
                require_int(item, module)?;
                module
                    .body()
                    .line(if product { "i64.mul" } else { "i64.add" });
            }
            Ok(ValueKind::Int)
        }
        ExprKind::ConstRef(const_name) => match module.array_constant_value(const_name) {
            Some(ConstantArrayValue::Indexed(items)) => {
                if let Some(kind) = emit_static_array_fold(&items, product, name, expr, module)? {
                    return Ok(kind);
                }
                module.body().line(&format!("i64.const {}", if product { 1 } else { 0 }));
                for item in items {
                    require_int(&item, module)?;
                    module
                        .body()
                        .line(if product { "i64.mul" } else { "i64.add" });
                }
                Ok(ValueKind::Int)
            }
            Some(ConstantArrayValue::Assoc(items)) => {
                let values = normalize_assoc_items(&items)
                    .unwrap_or(items)
                    .into_iter()
                    .map(|(_, value)| value)
                    .collect::<Vec<_>>();
                if let Some(kind) = emit_static_array_fold(&values, product, name, expr, module)? {
                    return Ok(kind);
                }
                module.body().line(&format!("i64.const {}", if product { 1 } else { 0 }));
                for item in values {
                    require_int(&item, module)?;
                    module
                        .body()
                        .line(if product { "i64.mul" } else { "i64.add" });
                }
                Ok(ValueKind::Int)
            }
            None => Err(CompileError::new(
                args[0].span,
                &format!("wasm32-web {name}() requires a known array constant"),
            )),
        },
        ExprKind::Variable(var) if module.local_kind(var) == Some(LocalKind::Array) => {
            emit_numeric_array_fold_from_local(expr, name, var, product, module)
        }
        ExprKind::Variable(var)
            if module.local_kind(var) == Some(LocalKind::Mixed)
                && module.mixed_value_cell_kind(var).is_none() =>
        {
            emit_unknown_mixed_numeric_array_fold(var, product, module)
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            let values = items
                .iter()
                .map(|(_, value)| value.clone())
                .collect::<Vec<_>>();
            if let Some(kind) = emit_static_array_fold(&values, product, name, expr, module)? {
                return Ok(kind);
            }
            let temp = module
                .next_label("assoc_array_fold_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_assoc_array_items_assign(&temp, items, module)?;
            reject_known_unsupported_array_fold_values(expr, &temp, name, module)?;
            if value_array_fold_needs_float(&temp, module) {
                return emit_runtime_assoc_array_float_fold(&temp, product, module);
            }
            emit_runtime_assoc_array_fold(&temp, product, module)
        }
        ExprKind::FunctionCall { name: function_name, .. }
            if numeric_fold_direct_array_call_is_supported(function_name) =>
        {
            let temp = module
                .next_label("array_fold_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, &args[0], module)?;
            if numeric_fold_direct_array_call_needs_unknown_scalar_float(function_name, &temp, module) {
                reject_known_unsupported_array_fold_values(expr, &temp, name, module)?;
                if module.array_layout(&temp) == ArrayLayout::Assoc {
                    return emit_runtime_assoc_array_float_fold(&temp, product, module);
                }
                return emit_runtime_value_array_float_fold(&temp, product, module);
            }
            emit_numeric_array_fold_from_local(expr, name, &temp, product, module)
        }
        ExprKind::MethodCall { object, method, .. }
            if method_call_array_return_metadata(object, method, module).is_some() =>
        {
            let temp = module
                .next_label("array_fold_method_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, &args[0], module)?;
            emit_numeric_array_fold_from_local(expr, name, &temp, product, module)
        }
        ExprKind::NullsafeMethodCall { object, method, .. }
            if object_expr_is_known_non_null(object, module)
                && method_call_array_return_metadata(object, method, module).is_some() =>
        {
            let temp = module
                .next_label("array_fold_nullsafe_method_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, &args[0], module)?;
            emit_numeric_array_fold_from_local(expr, name, &temp, product, module)
        }
        ExprKind::StaticMethodCall { receiver, method, .. }
            if static_method_call_array_return_metadata(receiver, method, module).is_some() =>
        {
            let temp = module
                .next_label("array_fold_static_method_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_array_assign(&temp, &args[0], module)?;
            emit_numeric_array_fold_from_local(expr, name, &temp, product, module)
        }
        ExprKind::ArrayAccess { .. } if nested_array_metadata_for_access_expr(&args[0], module).is_some() => {
            let temp = materialize_nested_array_fold_source(&args[0], name, module)?;
            emit_numeric_array_fold_from_local(expr, name, &temp, product, module)
        }
        _ => Err(CompileError::new(
            args[0].span,
            &format!("wasm32-web {name}() currently supports indexed array values only"),
        )),
    }
}

fn emit_unknown_mixed_numeric_array_fold(
    var: &str,
    product: bool,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let temp = module
        .next_label("unknown_mixed_array_fold_source")
        .trim_start_matches('$')
        .to_string();
    let heap_kind = module.next_label("unknown_mixed_array_fold_heap_kind");
    module.declare_array_local(temp.clone());
    module.declare_i32_local(heap_kind.trim_start_matches('$').to_string());
    module.body().line(&format!("local.get ${}", var));
    module.body().line("call $__rt_mixed_array_ptr");
    module.body().line(&format!("local.set ${}_ptr", temp));
    module.body().line(&format!("local.get ${}", var));
    module.body().line("call $__rt_mixed_array_len_i32");
    module.body().line(&format!("local.set ${}_len", temp));
    module.body().line(&format!("local.get ${}", var));
    module.body().line("call $__rt_mixed_array_heap_kind");
    module.body().line(&format!("local.set {}", heap_kind));
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_INDEXED_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if (result f64)");
    module.set_array_layout(&temp, ArrayLayout::Value);
    module.set_array_value_cell_kinds(&temp, None);
    emit_runtime_value_array_float_fold(&temp, product, module)?;
    module.body().line("else");
    module.body().line(&format!("local.get {}", heap_kind));
    module.body().line(&format!("i32.const {}", WASM_HEAP_KIND_ASSOC_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if (result f64)");
    module.set_array_layout(&temp, ArrayLayout::Assoc);
    module.set_array_value_cell_kinds(&temp, None);
    module.set_array_key_kinds(&temp, None);
    emit_runtime_assoc_array_float_fold(&temp, product, module)?;
    module.body().line("else");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    Ok(ValueKind::Float)
}

fn emit_numeric_array_fold_from_local(
    expr: &Expr,
    name: &str,
    var: &str,
    product: bool,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if let Some(values) = module.array_value_constants(var).map(|values| values.to_vec()) {
        return emit_static_array_fold_values(values, product, module);
    }
    if module.array_layout(var) == ArrayLayout::Assoc {
        reject_known_unsupported_array_fold_values(expr, var, name, module)?;
        if value_array_fold_needs_float(var, module) {
            return emit_runtime_assoc_array_float_fold(var, product, module);
        }
        return emit_runtime_assoc_array_fold(var, product, module);
    }
    if module.array_layout(var) == ArrayLayout::Value {
        reject_known_unsupported_array_fold_values(expr, var, name, module)?;
        if value_array_fold_needs_float(var, module) {
            return emit_runtime_value_array_float_fold(var, product, module);
        }
        return emit_runtime_value_array_fold(var, product, module);
    }
    let Some(len) = module.array_length(var) else {
        return emit_runtime_numeric_array_fold(var, product, module);
    };
    module.body().line(&format!("i64.const {}", if product { 1 } else { 0 }));
    for index in 0..len {
        module.body().line(&format!("local.get ${}_ptr", var));
        module.body().line(&format!("i32.const {}", index * 8));
        module.body().line("i32.add");
        module.body().line("i64.load");
        module
            .body()
            .line(if product { "i64.mul" } else { "i64.add" });
    }
    Ok(ValueKind::Int)
}

fn numeric_fold_direct_array_call_is_supported(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "range"
            | "array_fill"
            | "array_fill_keys"
            | "array_combine"
            | "array_column"
            | "array_values"
            | "array_keys"
            | "array_reverse"
            | "array_unique"
            | "array_merge"
            | "array_slice"
            | "array_pad"
            | "array_diff"
            | "array_intersect"
            | "array_diff_key"
            | "array_intersect_key"
            | "array_filter"
    )
}

fn numeric_fold_direct_array_call_needs_unknown_scalar_float(
    name: &str,
    var: &str,
    module: &WasmModule,
) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "array_diff" | "array_intersect"
    ) && module.array_value_cell_kinds(var).is_none()
        && module.array_runtime_value_cell_kind(var).is_none()
        && matches!(module.array_layout(var), ArrayLayout::Value | ArrayLayout::Assoc)
}

fn materialize_nested_array_fold_source(
    source: &Expr,
    name: &str,
    module: &mut WasmModule,
) -> Result<String, CompileError> {
    let Some(metadata) = nested_array_metadata_for_access_expr(source, module) else {
        return Err(CompileError::new(
            source.span,
            &format!("wasm32-web {name}() requires nested array metadata"),
        ));
    };
    let temp = module
        .next_label("array_fold_nested_source")
        .trim_start_matches('$')
        .to_string();
    module.declare_array_local(temp.clone());
    match emit_expr(source, module)? {
        ValueKind::Array => {
            module.body().line(&format!("local.set ${}_len", temp));
            module.body().line(&format!("local.set ${}_ptr", temp));
        }
        _ => {
            return Err(CompileError::new(
                source.span,
                &format!("wasm32-web {name}() expected a nested array value"),
            ));
        }
    }
    let key_values = metadata.key_values.clone();
    module.set_array_layout(&temp, metadata.layout);
    module.set_array_length(&temp, metadata.len);
    module.set_array_value_cell_kinds(&temp, metadata.value_kinds);
    module.set_array_key_values(&temp, key_values.clone());
    module.set_array_key_kinds(
        &temp,
        key_values.map(|keys| {
            keys.into_iter()
                .map(|key| match key {
                    AssocKeyValue::Int(_) => AssocKeyKind::Int,
                    AssocKeyValue::Str(_) => AssocKeyKind::Str,
                })
                .collect()
        }),
    );
    module.set_array_php_normalized_runtime_keys(
        &temp,
        metadata.layout == ArrayLayout::Assoc && module.array_key_kinds(&temp).is_none(),
    );
    module.set_array_nested_value_metadata(&temp, metadata.nested_values);
    Ok(temp)
}

fn emit_static_array_fold(
    items: &[Expr],
    product: bool,
    name: &str,
    expr: &Expr,
    module: &mut WasmModule,
) -> Result<Option<ValueKind>, CompileError> {
    let Some(values) = items
        .iter()
        .map(|item| static_scalar_value(item, module))
        .collect::<Option<Vec<_>>>()
    else {
        if items
            .iter()
            .any(|item| matches!(item.kind, ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)))
        {
            return Err(CompileError::new(
                expr.span,
                &format!(
                    "wasm32-web {name}() does not yet support string or nested-array value cells"
                ),
            ));
        }
        return Ok(None);
    };
    Ok(Some(emit_static_array_fold_values(values, product, module)?))
}

fn emit_static_array_fold_values(
    values: Vec<ConstantValue>,
    product: bool,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    if values.iter().any(static_array_fold_value_needs_float) {
        module.body().line(&format!("f64.const {}", if product { 1 } else { 0 }));
        for value in values {
            module
                .body()
                .line(&format!("f64.const {}", static_array_fold_value_as_float(&value)));
            module.body().line(if product { "f64.mul" } else { "f64.add" });
        }
        return Ok(ValueKind::Float);
    }
    module.body().line(&format!("i64.const {}", if product { 1 } else { 0 }));
    for value in values {
        module
            .body()
            .line(&format!("i64.const {}", static_array_fold_value_as_int(&value)));
        module.body().line(if product { "i64.mul" } else { "i64.add" });
    }
    Ok(ValueKind::Int)
}

fn static_array_fold_value_needs_float(value: &ConstantValue) -> bool {
    match value {
        ConstantValue::Float(_) => true,
        ConstantValue::Str(value) => php_leading_numeric_string_value(value).is_some_and(
            |(number, has_float_marker)| {
                has_float_marker || number.fract() != 0.0 || number < i64::MIN as f64 || number > i64::MAX as f64
            },
        ),
        ConstantValue::Int(_) | ConstantValue::Bool(_) | ConstantValue::Null => false,
    }
}

fn static_array_fold_value_as_float(value: &ConstantValue) -> f64 {
    match value {
        ConstantValue::Int(value) => *value as f64,
        ConstantValue::Float(value) => *value,
        ConstantValue::Bool(value) => i32::from(*value) as f64,
        ConstantValue::Null => 0.0,
        ConstantValue::Str(value) => php_leading_numeric_string_value(value)
            .map(|(number, _)| number)
            .unwrap_or(0.0),
    }
}

fn static_array_fold_value_as_int(value: &ConstantValue) -> i64 {
    match value {
        ConstantValue::Int(value) => *value,
        ConstantValue::Bool(value) => i64::from(*value),
        ConstantValue::Null => 0,
        ConstantValue::Str(value) => php_leading_numeric_string_value(value)
            .map(|(number, _)| number as i64)
            .unwrap_or(0),
        ConstantValue::Float(_) => {
            unreachable!("float and string values are handled before integer folding")
        }
    }
}

fn reject_known_unsupported_array_fold_values(
    expr: &Expr,
    var: &str,
    name: &str,
    module: &WasmModule,
) -> Result<(), CompileError> {
    if module.array_value_cell_kinds(var).is_some_and(|kinds| {
        kinds.iter().any(|kind| matches!(kind, ValueCellKind::Array))
    }) {
        return Err(CompileError::new(
            expr.span,
            &format!(
                "wasm32-web {name}() does not yet support nested-array value cells"
            ),
        ));
    }
    Ok(())
}

fn value_array_fold_needs_float(var: &str, module: &WasmModule) -> bool {
    if module.array_layout(var) == ArrayLayout::Assoc
        && module.array_value_cell_kinds(var).is_none()
        && module.array_runtime_value_cell_kind(var).is_none()
    {
        return true;
    }
    module
        .array_value_cell_kinds(var)
        .is_some_and(|kinds| kinds.iter().any(|kind| matches!(kind, ValueCellKind::Float | ValueCellKind::Str)))
        || matches!(
            module.array_runtime_value_cell_kind(var),
            Some(ValueCellKind::Float | ValueCellKind::Str)
        )
}

fn emit_runtime_numeric_array_fold(
    var: &str,
    product: bool,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let acc = module.next_label("array_fold_acc");
    let index = module.next_label("array_fold_index");
    let done_label = module.next_label("array_fold_done");
    let loop_label = module.next_label("array_fold_loop");
    module.declare_i64_local(acc.trim_start_matches('$').to_string());
    module.declare_i32_local(index.trim_start_matches('$').to_string());
    module.body().line(&format!("i64.const {}", if product { 1 } else { 0 }));
    module.body().line(&format!("local.set {}", acc));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", acc));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 8");
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module
        .body()
        .line(if product { "i64.mul" } else { "i64.add" });
    module.body().line(&format!("local.set {}", acc));
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", acc));
    Ok(ValueKind::Int)
}

fn emit_runtime_value_array_fold(
    var: &str,
    product: bool,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let acc = module.next_label("value_array_fold_acc");
    let index = module.next_label("value_array_fold_index");
    let cell = module.next_label("value_array_fold_cell");
    let done_label = module.next_label("value_array_fold_done");
    let loop_label = module.next_label("value_array_fold_loop");
    module.declare_i64_local(acc.trim_start_matches('$').to_string());
    for local in [&index, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("i64.const {}", if product { 1 } else { 0 }));
    module.body().line(&format!("local.set {}", acc));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(var, &index, &cell, module);
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", acc));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module
        .body()
        .line(if product { "i64.mul" } else { "i64.add" });
    module.body().line(&format!("local.set {}", acc));
    module.body().line("else");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", acc));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module
        .body()
        .line(if product { "i64.mul" } else { "i64.add" });
    module.body().line(&format!("local.set {}", acc));
    module.body().line("else");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.eq");
    module.body().open("if");
    if product {
        module.body().line("i64.const 0");
        module.body().line(&format!("local.set {}", acc));
    }
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", acc));
    Ok(ValueKind::Int)
}

fn emit_runtime_value_array_float_fold(
    var: &str,
    product: bool,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let acc = module.next_label("value_array_float_fold_acc");
    let index = module.next_label("value_array_float_fold_index");
    let cell = module.next_label("value_array_float_fold_cell");
    let done_label = module.next_label("value_array_float_fold_done");
    let loop_label = module.next_label("value_array_float_fold_loop");
    module.declare_f64_local(acc.trim_start_matches('$').to_string());
    for local in [&index, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("f64.const {}", if product { 1 } else { 0 }));
    module.body().line(&format!("local.set {}", acc));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_value_cell_address_for_local(var, &index, &cell, module);
    emit_runtime_value_cell_float_fold_step(&acc, &cell, product, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", acc));
    Ok(ValueKind::Float)
}

fn emit_runtime_value_cell_float_fold_step(
    acc: &str,
    cell: &str,
    product: bool,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", acc));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("f64.convert_i64_s");
    module.body().line(if product { "f64.mul" } else { "f64.add" });
    module.body().line(&format!("local.set {}", acc));
    module.body().line("else");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_FLOAT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", acc));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("f64.load");
    module.body().line(if product { "f64.mul" } else { "f64.add" });
    module.body().line(&format!("local.set {}", acc));
    module.body().line("else");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", acc));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("f64.convert_i64_s");
    module.body().line(if product { "f64.mul" } else { "f64.add" });
    module.body().line(&format!("local.set {}", acc));
    module.body().line("else");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_STRING));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", acc));
    let number = module.next_label("value_array_fold_string_number");
    module.declare_f64_local(number.trim_start_matches('$').to_string());
    emit_value_cell_string_leading_numeric_value(cell, &number, module);
    module.body().line(&format!("local.get {}", number));
    module.body().line(if product { "f64.mul" } else { "f64.add" });
    module.body().line(&format!("local.set {}", acc));
    module.body().line("else");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.eq");
    module.body().open("if");
    if product {
        module.body().line("f64.const 0");
        module.body().line(&format!("local.set {}", acc));
    }
    module.body().line("else");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_ARRAY));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line("unreachable");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
}

fn emit_runtime_assoc_array_fold(
    var: &str,
    product: bool,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let acc = module.next_label("assoc_array_fold_acc");
    let index = module.next_label("assoc_array_fold_index");
    let cell = module.next_label("assoc_array_fold_cell");
    let done_label = module.next_label("assoc_array_fold_done");
    let loop_label = module.next_label("assoc_array_fold_loop");
    module.declare_i64_local(acc.trim_start_matches('$').to_string());
    for local in [&index, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("i64.const {}", if product { 1 } else { 0 }));
    module.body().line(&format!("local.set {}", acc));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("i32.const {}", WASM_ASSOC_ENTRY_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i32.const 16");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", acc));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module
        .body()
        .line(if product { "i64.mul" } else { "i64.add" });
    module.body().line(&format!("local.set {}", acc));
    module.body().line("else");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_BOOL));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", acc));
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module
        .body()
        .line(if product { "i64.mul" } else { "i64.add" });
    module.body().line(&format!("local.set {}", acc));
    module.body().line("else");
    module.body().line(&format!("local.get {}", cell));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_VALUE_TAG_NULL));
    module.body().line("i32.eq");
    module.body().open("if");
    if product {
        module.body().line("i64.const 0");
        module.body().line(&format!("local.set {}", acc));
    }
    module.body().close("end");
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", acc));
    Ok(ValueKind::Int)
}

fn emit_runtime_assoc_array_float_fold(
    var: &str,
    product: bool,
    module: &mut WasmModule,
) -> Result<ValueKind, CompileError> {
    let acc = module.next_label("assoc_array_float_fold_acc");
    let index = module.next_label("assoc_array_float_fold_index");
    let cell = module.next_label("assoc_array_float_fold_cell");
    let done_label = module.next_label("assoc_array_float_fold_done");
    let loop_label = module.next_label("assoc_array_float_fold_loop");
    module.declare_f64_local(acc.trim_start_matches('$').to_string());
    for local in [&index, &cell] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("f64.const {}", if product { 1 } else { 0 }));
    module.body().line(&format!("local.set {}", acc));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get ${}_len", var));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get ${}_ptr", var));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("i32.const {}", WASM_ASSOC_ENTRY_SIZE));
    module.body().line("i32.mul");
    module.body().line("i32.add");
    module.body().line("i32.const 16");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", cell));
    emit_runtime_value_cell_float_fold_step(&acc, &cell, product, module);
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", acc));
    Ok(ValueKind::Float)
}
