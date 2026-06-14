//! Purpose:
//! Emits associative array key-set operations for wasm32-web array helpers.
//! Keeps array_diff_key()/array_intersect_key() lowering separate from merge emitters.
//!
//! Called from:
//! - `super::array_assign` and `super::array_merge` helper paths.
//!
//! Key details:
//! - Handles static and runtime key comparison without changing native codegen behavior.

use super::*;
use super::array_transform_sets::emit_assoc_entry_address;

pub(super) fn emit_assoc_array_key_set_assign(
    name: &str,
    expr: &Expr,
    function_name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    if args.len() < 2 {
        return Err(CompileError::new(
            expr.span,
            &format!("wasm32-web {function_name}() expects at least two arguments"),
        ));
    }
    if let ExprKind::Variable(source) = &args[0].kind {
        if module.local_kind(source) == Some(LocalKind::Array)
            && module.array_layout(source) == ArrayLayout::Assoc
        {
            return emit_assoc_array_local_key_set_assign(name, expr, source, function_name, args, module);
        }
    }
    if let ExprKind::FunctionCall { name: callee_name, args: call_args } = &args[0].kind {
        if module.has_function(callee_name)
            && module.function_return_kind(callee_name) == Some(ValueKind::Array)
            && module.function_array_return_layout(callee_name) == ArrayLayout::Assoc
        {
            let temp = materialize_function_array_return_local(
                &args[0],
                callee_name,
                call_args,
                "assoc_key_set_source",
                module,
            )?;
            return emit_assoc_array_local_key_set_assign(name, expr, &temp, function_name, args, module);
        }
    }
    if !matches!(args[0].kind, ExprKind::ArrayLiteralAssoc(_))
        && expression_has_array_type(&args[0], module)
    {
        let temp = materialize_array_map_multi_source(&args[0], "assoc_key_set_source", module)?;
        if module.array_layout(&temp) == ArrayLayout::Assoc {
            return emit_assoc_array_local_key_set_assign(name, expr, &temp, function_name, args, module);
        }
        return Err(CompileError::new(
            args[0].span,
            &format!("wasm32-web {function_name}() currently requires an associative source"),
        ));
    }
    if let ExprKind::ArrayLiteralAssoc(items) = &args[0].kind {
        if assoc_key_set_needs_runtime_compare(&args[1..], module) {
            let temp = module
                .next_label("assoc_key_set_source")
                .trim_start_matches('$')
                .to_string();
            module.declare_array_local(temp.clone());
            emit_assoc_array_items_assign(&temp, items, module)?;
            return emit_assoc_array_local_key_set_assign(name, expr, &temp, function_name, args, module);
        }
    }
    let items = static_assoc_array_key_set_items(expr, function_name, args, module)?;
    emit_assoc_array_items_assign(name, &items, module)
}

pub(super) fn assoc_key_set_needs_runtime_compare(args: &[Expr], module: &WasmModule) -> bool {
    args.iter().any(|arg| {
        matches!(
            &arg.kind,
            ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array)
        ) || matches!(
            &arg.kind,
            ExprKind::FunctionCall { .. } | ExprKind::ExprCall { .. }
                if expression_has_array_type(arg, module)
        )
    })
}

pub(super) fn emit_assoc_array_local_key_set_assign(
    name: &str,
    expr: &Expr,
    source: &str,
    function_name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<(), CompileError> {
    let compare_sets = prepare_assoc_key_compare_sources(expr, function_name, &args[1..], module)?;
    let source_len = module.array_length(source);
    let metadata = assoc_array_key_set_metadata(source, function_name, &args[1..], module)?;
    if let Some(metadata) = metadata.as_ref() {
        module.set_array_length(name, metadata.keys.len());
    } else if let Some(source_len) = source_len {
        module.set_array_length(name, source_len);
    } else {
        module.clear_array_length(name);
    }
    module.set_array_layout(name, ArrayLayout::Assoc);
    module.set_array_key_kinds(
        name,
        metadata
            .as_ref()
            .map(|metadata| metadata.keys.iter().map(assoc_key_kind_for_value).collect()),
    );
    module.set_array_key_values(name, metadata.as_ref().map(|metadata| metadata.keys.clone()));
    module.set_array_value_cell_kinds(name, metadata.as_ref().map(|metadata| metadata.value_kinds.clone()));
    module.set_array_nested_value_metadata(
        name,
        metadata
            .as_ref()
            .map(|metadata| metadata.nested_values.clone()),
    );
    if metadata.is_none() {
        module.set_array_runtime_key_kind(name, module.array_runtime_key_kind(source));
        module.set_array_php_normalized_runtime_keys(
            name,
            module.array_has_php_normalized_runtime_keys(source),
        );
        module.set_array_runtime_value_cell_kind(name, module.array_runtime_value_cell_kind(source));
    }
    let source_ptr = preserve_array_ptr(source, function_name, module);
    let source_len_local = module.next_label("assoc_key_set_source_len");
    let index = module.next_label("assoc_key_set_index");
    let out_index = module.next_label("assoc_key_set_out_index");
    let source_entry = module.next_label("assoc_key_set_source_entry");
    let target_entry = module.next_label("assoc_key_set_target_entry");
    let found = module.next_label("assoc_key_set_found");
    let set_found = module.next_label("assoc_key_set_arg_found");
    let key_found = module.next_label("assoc_key_set_key_found");
    let compare_index = module.next_label("assoc_key_set_compare_index");
    let compare_entry = module.next_label("assoc_key_set_compare_entry");
    let should_copy = module.next_label("assoc_key_set_should_copy");
    let done_label = module.next_label("assoc_key_set_done");
    let loop_label = module.next_label("assoc_key_set_loop");
    for local in [
        &source_len_local,
        &index,
        &out_index,
        &source_entry,
        &target_entry,
        &found,
        &set_found,
        &key_found,
        &compare_index,
        &compare_entry,
        &should_copy,
    ] {
        module.declare_i32_local(local.trim_start_matches('$').to_string());
    }
    module.body().line(&format!("local.get ${}_len", source));
    module.body().line(&format!("local.set {}", source_len_local));
    module.body().line(&format!("local.get {}", source_len_local));
    module.body().line("call $__rt_alloc_assoc_entries");
    module.body().line(&format!("local.set ${}_ptr", name));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", index));
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", out_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", index));
    module.body().line(&format!("local.get {}", source_len_local));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_assoc_entry_address(&source_ptr, &index, &source_entry, module);
    emit_assoc_key_set_match_aggregate(
        &source_entry,
        &compare_sets,
        function_name.eq_ignore_ascii_case("array_intersect_key"),
        &found,
        &set_found,
        &key_found,
        &compare_index,
        &compare_entry,
        module,
    );
    if function_name.eq_ignore_ascii_case("array_intersect_key") {
        module.body().line(&format!("local.get {}", found));
        module.body().line(&format!("local.set {}", should_copy));
    } else {
        module.body().line(&format!("local.get {}", found));
        module.body().line("i32.eqz");
        module.body().line(&format!("local.set {}", should_copy));
    }
    module.body().line(&format!("local.get {}", should_copy));
    module.body().open("if");
    emit_assoc_entry_address(&format!("${}_ptr", name), &out_index, &target_entry, module);
    copy_assoc_entry(&target_entry, &source_entry, module);
    module.body().line(&format!("local.get {}", out_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", out_index));
    module.body().close("end");
    module.body().line(&format!("local.get {}", index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
    module.body().line(&format!("local.get {}", out_index));
    module.body().line(&format!("local.set ${}_len", name));
    Ok(())
}

pub(super) struct AssocArrayKeySetMetadata {
    keys: Vec<AssocKeyValue>,
    value_kinds: Vec<ValueCellKind>,
    nested_values: Vec<Option<NestedArrayMetadata>>,
}

pub(super) fn assoc_array_key_set_metadata(
    source: &str,
    function_name: &str,
    args: &[Expr],
    module: &WasmModule,
) -> Result<Option<AssocArrayKeySetMetadata>, CompileError> {
    let Some(source_keys) = module.array_key_values(source) else {
        return Ok(None);
    };
    let Some(source_values) = module.array_value_cell_kinds(source) else {
        return Ok(None);
    };
    let Some(compare_sets) = assoc_key_compare_static_values(args, module)? else {
        return Ok(None);
    };
    let keep_matching = function_name.eq_ignore_ascii_case("array_intersect_key");
    let mut keys = Vec::new();
    let mut value_kinds = Vec::new();
    let mut nested_values = Vec::new();
    let source_nested_values = module.array_nested_value_metadata_items(source);
    for (index, (key, value_kind)) in source_keys.iter().zip(source_values.iter()).enumerate() {
        let matches = if keep_matching {
            compare_sets.iter().all(|set| set.contains(key))
        } else {
            compare_sets.iter().any(|set| set.contains(key))
        };
        if matches == keep_matching {
            keys.push(key.clone());
            value_kinds.push(*value_kind);
            nested_values.push(
                source_nested_values
                    .and_then(|values| values.get(index))
                    .cloned()
                    .flatten(),
            );
        }
    }
    Ok(Some(AssocArrayKeySetMetadata {
        keys,
        value_kinds,
        nested_values,
    }))
}

pub(super) fn assoc_key_compare_static_values(
    args: &[Expr],
    module: &WasmModule,
) -> Result<Option<Vec<Vec<AssocKeyValue>>>, CompileError> {
    let mut compare_sets = Vec::new();
    for arg in args {
        match &arg.kind {
            ExprKind::ArrayLiteralAssoc(items) => {
                let mut keys = Vec::new();
                for (key, _) in items {
                    keys.push(static_assoc_key_value_to_value(static_assoc_key_value(key, module)?));
                }
                compare_sets.push(keys);
            }
            ExprKind::ArrayLiteral(items) => {
                compare_sets.push(
                    (0..items.len())
                        .map(|index| AssocKeyValue::Int(index as i64))
                        .collect(),
                );
            }
            ExprKind::Variable(source)
                if module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::Assoc =>
            {
                let Some(keys) = module.array_key_values(source) else {
                    return Ok(None);
                };
                compare_sets.push(keys.to_vec());
            }
            ExprKind::FunctionCall { name, .. }
                if module.has_function(name)
                    && module.function_return_kind(name) == Some(ValueKind::Array)
                    && module.function_array_return_layout(name) == ArrayLayout::Assoc =>
            {
                let Some(keys) = module.function_array_return_key_values(name) else {
                    return Ok(None);
                };
                compare_sets.push(keys.to_vec());
            }
            ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
                let Some(len) = module.array_length(source) else {
                    return Ok(None);
                };
                compare_sets.push((0..len).map(|index| AssocKeyValue::Int(index as i64)).collect());
            }
            _ => return Ok(None),
        }
    }
    Ok(Some(compare_sets))
}

pub(super) fn static_assoc_key_value_to_value(key: StaticAssocKey) -> AssocKeyValue {
    match key {
        StaticAssocKey::Int(value) => AssocKeyValue::Int(value),
        StaticAssocKey::Str(value) => AssocKeyValue::Str(value),
    }
}

pub(super) enum AssocKeyCompareSource {
    Static(Vec<StaticAssocKey>),
    Runtime { ptr: String, len: String },
    RuntimeIndexed { len: String },
}

pub(super) fn prepare_assoc_key_compare_sources(
    expr: &Expr,
    function_name: &str,
    args: &[Expr],
    module: &mut WasmModule,
) -> Result<Vec<AssocKeyCompareSource>, CompileError> {
    let mut compare_sets = Vec::new();
    for arg in args {
        match &arg.kind {
            ExprKind::ArrayLiteralAssoc(items) => {
                let mut keys = Vec::new();
                for (key, _) in items {
                    keys.push(static_assoc_key_value(key, module)?);
                }
                compare_sets.push(AssocKeyCompareSource::Static(keys));
            }
            ExprKind::ArrayLiteral(items) => {
                let mut keys = Vec::new();
                keys.extend((0..items.len()).map(|index| StaticAssocKey::Int(index as i64)));
                compare_sets.push(AssocKeyCompareSource::Static(keys));
            }
            ExprKind::Variable(source)
                if module.local_kind(source) == Some(LocalKind::Array)
                    && module.array_layout(source) == ArrayLayout::Assoc =>
            {
                let ptr = preserve_array_ptr(source, function_name, module);
                let len = module.next_label("assoc_key_set_compare_len");
                module.declare_i32_local(len.trim_start_matches('$').to_string());
                module.body().line(&format!("local.get ${}_len", source));
                module.body().line(&format!("local.set {}", len));
                compare_sets.push(AssocKeyCompareSource::Runtime { ptr, len });
            }
            ExprKind::FunctionCall { name: callee_name, args: call_args }
                if module.has_function(callee_name)
                    && module.function_return_kind(callee_name) == Some(ValueKind::Array)
                    && module.function_array_return_layout(callee_name) == ArrayLayout::Assoc =>
            {
                let temp = materialize_function_array_return_local(
                    arg,
                    callee_name,
                    call_args,
                    "assoc_key_set_compare",
                    module,
                )?;
                let ptr = preserve_array_ptr(&temp, function_name, module);
                let len = module.next_label("assoc_key_set_compare_len");
                module.declare_i32_local(len.trim_start_matches('$').to_string());
                module.body().line(&format!("local.get ${}_len", temp));
                module.body().line(&format!("local.set {}", len));
                compare_sets.push(AssocKeyCompareSource::Runtime { ptr, len });
            }
            ExprKind::FunctionCall { .. } | ExprKind::ExprCall { .. }
                if expression_has_array_type(arg, module) =>
            {
                let temp = materialize_array_map_multi_source(arg, "assoc_key_set_compare", module)?;
                if module.array_layout(&temp) == ArrayLayout::Assoc {
                    let ptr = preserve_array_ptr(&temp, function_name, module);
                    let len = module.next_label("assoc_key_set_compare_len");
                    module.declare_i32_local(len.trim_start_matches('$').to_string());
                    module.body().line(&format!("local.get ${}_len", temp));
                    module.body().line(&format!("local.set {}", len));
                    compare_sets.push(AssocKeyCompareSource::Runtime { ptr, len });
                } else {
                    let len = module.next_label("assoc_key_set_indexed_compare_len");
                    module.declare_i32_local(len.trim_start_matches('$').to_string());
                    module.body().line(&format!("local.get ${}_len", temp));
                    module.body().line(&format!("local.set {}", len));
                    compare_sets.push(AssocKeyCompareSource::RuntimeIndexed { len });
                }
            }
            ExprKind::Variable(source) if module.local_kind(source) == Some(LocalKind::Array) => {
                let len = module.next_label("assoc_key_set_indexed_compare_len");
                module.declare_i32_local(len.trim_start_matches('$').to_string());
                module.body().line(&format!("local.get ${}_len", source));
                module.body().line(&format!("local.set {}", len));
                compare_sets.push(AssocKeyCompareSource::RuntimeIndexed { len });
            }
            _ => {
                return Err(CompileError::new(
                    arg.span,
                    &format!(
                        "wasm32-web {function_name}() currently requires direct array literals or assigned associative locals"
                    ),
                ));
            }
        }
    }
    let _ = expr;
    Ok(compare_sets)
}

pub(super) fn emit_assoc_key_set_match_aggregate(
    source_entry: &str,
    compare_sets: &[AssocKeyCompareSource],
    require_all_sets: bool,
    found: &str,
    set_found: &str,
    key_found: &str,
    compare_index: &str,
    compare_entry: &str,
    module: &mut WasmModule,
) {
    module
        .body()
        .line(&format!("i32.const {}", i32::from(require_all_sets)));
    module.body().line(&format!("local.set {}", found));
    for set in compare_sets {
        module.body().line("i32.const 0");
        module.body().line(&format!("local.set {}", set_found));
        match set {
            AssocKeyCompareSource::Static(keys) => {
                for key in keys {
                    emit_assoc_entry_matches_static_key(source_entry, key, key_found, module);
                    module.body().line(&format!("local.get {}", set_found));
                    module.body().line(&format!("local.get {}", key_found));
                    module.body().line("i32.or");
                    module.body().line(&format!("local.set {}", set_found));
                }
            }
            AssocKeyCompareSource::Runtime { ptr, len } => {
                emit_assoc_key_set_runtime_source_match(
                    source_entry,
                    ptr,
                    len,
                    set_found,
                    key_found,
                    compare_index,
                    compare_entry,
                    module,
                );
            }
            AssocKeyCompareSource::RuntimeIndexed { len } => {
                emit_assoc_key_set_runtime_indexed_match(source_entry, len, set_found, module);
            }
        }
        module.body().line(&format!("local.get {}", found));
        module.body().line(&format!("local.get {}", set_found));
        if require_all_sets {
            module.body().line("i32.and");
        } else {
            module.body().line("i32.or");
        }
        module.body().line(&format!("local.set {}", found));
    }
}

pub(super) fn emit_assoc_key_set_runtime_source_match(
    source_entry: &str,
    compare_ptr: &str,
    compare_len: &str,
    set_found: &str,
    key_found: &str,
    compare_index: &str,
    compare_entry: &str,
    module: &mut WasmModule,
) {
    let done_label = module.next_label("assoc_key_set_runtime_done");
    let loop_label = module.next_label("assoc_key_set_runtime_loop");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", compare_index));
    module.body().open(&format!("block {}", done_label));
    module.body().open(&format!("loop {}", loop_label));
    module.body().line(&format!("local.get {}", compare_index));
    module.body().line(&format!("local.get {}", compare_len));
    module.body().line("i32.ge_u");
    module.body().line(&format!("br_if {}", done_label));
    emit_assoc_entry_address(compare_ptr, compare_index, compare_entry, module);
    emit_assoc_entries_have_same_key(source_entry, compare_entry, key_found, module);
    module.body().line(&format!("local.get {}", set_found));
    module.body().line(&format!("local.get {}", key_found));
    module.body().line("i32.or");
    module.body().line(&format!("local.set {}", set_found));
    module.body().line(&format!("local.get {}", set_found));
    module.body().line(&format!("br_if {}", done_label));
    module.body().line(&format!("local.get {}", compare_index));
    module.body().line("i32.const 1");
    module.body().line("i32.add");
    module.body().line(&format!("local.set {}", compare_index));
    module.body().line(&format!("br {}", loop_label));
    module.body().close("end");
    module.body().close("end");
}

pub(super) fn emit_assoc_key_set_runtime_indexed_match(
    source_entry: &str,
    compare_len: &str,
    set_found: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.load");
    module.body().line(&format!("i32.const {}", WASM_ASSOC_KEY_INT));
    module.body().line("i32.eq");
    module.body().open("if");
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line("i64.const 0");
    module.body().line("i64.ge_s");
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line("i32.const 8");
    module.body().line("i32.add");
    module.body().line("i64.load");
    module.body().line(&format!("local.get {}", compare_len));
    module.body().line("i64.extend_i32_u");
    module.body().line("i64.lt_s");
    module.body().line("i32.and");
    module.body().line(&format!("local.set {}", set_found));
    module.body().line("else");
    module.body().line("i32.const 0");
    module.body().line(&format!("local.set {}", set_found));
    module.body().close("end");
}

pub(super) fn emit_assoc_entry_matches_static_key(
    source_entry: &str,
    key: &StaticAssocKey,
    key_found: &str,
    module: &mut WasmModule,
) {
    match key {
        StaticAssocKey::Int(value) => {
            module.body().line(&format!("local.get {}", source_entry));
            module.body().line(&format!("i64.const {}", value));
            module.body().line("call $__rt_assoc_key_eq_int");
            module.body().line(&format!("local.set {}", key_found));
        }
        StaticAssocKey::Str(value) => {
            emit_assoc_entry_matches_static_string_key(source_entry, value, key_found, module);
        }
    }
}

pub(super) fn emit_assoc_entries_have_same_key(
    left_entry: &str,
    right_entry: &str,
    key_found: &str,
    module: &mut WasmModule,
) {
    module.body().line(&format!("local.get {}", left_entry));
    module.body().line(&format!("local.get {}", right_entry));
    module.body().line("call $__rt_assoc_keys_equal");
    module.body().line(&format!("local.set {}", key_found));
}

pub(super) fn emit_assoc_entry_matches_static_string_key(
    source_entry: &str,
    value: &str,
    key_found: &str,
    module: &mut WasmModule,
) {
    let (ptr, len) = module.intern_string(value);
    module.body().line(&format!("local.get {}", source_entry));
    module.body().line(&format!("i32.const {}", ptr));
    module.body().line(&format!("i32.const {}", len));
    module.body().line("call $__rt_assoc_key_eq_php_string");
    module.body().line(&format!("local.set {}", key_found));
}

pub(super) fn static_assoc_array_key_set_items(
    expr: &Expr,
    function_name: &str,
    args: &[Expr],
    module: &WasmModule,
) -> Result<Vec<(Expr, Expr)>, CompileError> {
    let ExprKind::ArrayLiteralAssoc(source_items) = &args[0].kind else {
        return Err(CompileError::new(
            args[0].span,
            &format!("wasm32-web {function_name}() currently requires an associative literal source"),
        ));
    };
    let mut compare_sets = Vec::new();
    for arg in &args[1..] {
        let mut keys = Vec::new();
        match &arg.kind {
            ExprKind::ArrayLiteralAssoc(items) => {
                for (key, _) in items {
                    keys.push(static_assoc_key_value(key, module)?);
                }
            }
            ExprKind::ArrayLiteral(items) => {
                keys.extend((0..items.len()).map(|index| StaticAssocKey::Int(index as i64)));
            }
            _ => {
                return Err(CompileError::new(
                    arg.span,
                    &format!(
                        "wasm32-web {function_name}() currently requires direct array literals"
                    ),
                ));
            }
        }
        compare_sets.push(keys);
    }
    let keep_matching = function_name.eq_ignore_ascii_case("array_intersect_key");
    let mut out = Vec::new();
    for (key, value) in source_items {
        let key_value = static_assoc_key_value(key, module)?;
        let matches = if keep_matching {
            compare_sets.iter().all(|keys| keys.contains(&key_value))
        } else {
            compare_sets.iter().any(|keys| keys.contains(&key_value))
        };
        if matches == keep_matching {
            out.push((key.clone(), value.clone()));
        }
    }
    let _ = expr;
    Ok(out)
}

#[derive(Clone, PartialEq)]
pub(super) enum StaticAssocKey {
    Int(i64),
    Str(String),
}

pub(super) fn static_assoc_key_value(
    key: &Expr,
    module: &WasmModule,
) -> Result<StaticAssocKey, CompileError> {
    if let Some(value) = static_int_value(key) {
        return Ok(StaticAssocKey::Int(value));
    }
    if let Some(value) = static_string_value(key, module) {
        if let Some(value) = literal_php_array_int_key(&value) {
            return Ok(StaticAssocKey::Int(value));
        }
        return Ok(StaticAssocKey::Str(value));
    }
    Err(CompileError::new(
        key.span,
        "wasm32-web associative key set operations require static integer or string keys",
    ))
}
