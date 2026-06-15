//! Purpose:
//! Classifies wasm32-web array_map/array_filter callback shapes and supported value kinds.
//! Keeps semantic support checks separate from the map/filter loop emitters.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::array_map_filter` and assignment lowering modules.
//!
//! Key details:
//! - Mirrors wasm runtime value-cell metadata so unsupported callbacks are rejected before lowering.

use super::*;

pub(super) fn array_filter_callback_shape(
    callback: &str,
    span: crate::span::Span,
    module: &WasmModule,
) -> Result<ArrayFilterCallbackShape, CompileError> {
    if callback.eq_ignore_ascii_case("is_object")
        || callback.eq_ignore_ascii_case("get_class")
        || callback.eq_ignore_ascii_case("gettype")
        || callback.eq_ignore_ascii_case("boolval")
    {
        return Ok(ArrayFilterCallbackShape::Object);
    }
    if callback.eq_ignore_ascii_case("get_parent_class") {
        return Ok(ArrayFilterCallbackShape::ObjectParent);
    }
    if callback.eq_ignore_ascii_case("is_int") {
        return Ok(ArrayFilterCallbackShape::Int);
    }
    if callback.eq_ignore_ascii_case("is_string") {
        return Ok(ArrayFilterCallbackShape::Str);
    }
    if callback.eq_ignore_ascii_case("is_bool") {
        return Ok(ArrayFilterCallbackShape::Bool);
    }
    if callback.eq_ignore_ascii_case("is_float") {
        return Ok(ArrayFilterCallbackShape::Float);
    }
    if callback.eq_ignore_ascii_case("is_null") {
        return Ok(ArrayFilterCallbackShape::Null);
    }
    if callback.eq_ignore_ascii_case("is_numeric") {
        return Ok(ArrayFilterCallbackShape::Numeric);
    }
    if callback.eq_ignore_ascii_case("is_array") || callback.eq_ignore_ascii_case("is_iterable") {
        return Ok(ArrayFilterCallbackShape::Array);
    }
    if callback.eq_ignore_ascii_case("strlen") {
        return Ok(ArrayFilterCallbackShape::Str);
    }
    let Some(param_kinds) = module.function_param_kinds(callback) else {
        return Err(CompileError::new(
            span,
            "wasm32-web array_filter() callback metadata is missing",
        ));
    };
    match (param_kinds.as_slice(), module.function_return_kind(callback)) {
        ([LocalKind::I64], Some(ValueKind::Bool | ValueKind::Int)) => Ok(ArrayFilterCallbackShape::Int),
        ([LocalKind::I32], Some(ValueKind::Bool | ValueKind::Int)) => Ok(ArrayFilterCallbackShape::Bool),
        ([LocalKind::F64], Some(ValueKind::Bool | ValueKind::Int)) => Ok(ArrayFilterCallbackShape::Float),
        ([LocalKind::Str], Some(ValueKind::Bool | ValueKind::Int)) => Ok(ArrayFilterCallbackShape::Str),
        ([LocalKind::Mixed], Some(ValueKind::Bool | ValueKind::Int)) => Ok(ArrayFilterCallbackShape::Mixed),
        _ => Err(CompileError::new(
            span,
            "wasm32-web array_filter() currently supports one-scalar-argument predicates",
        )),
    }
}

pub(super) fn array_filter_builtin_callback_is_supported(callback: &str) -> bool {
    matches!(
        callback.to_ascii_lowercase().as_str(),
        "strlen"
            | "is_int"
            | "is_string"
            | "is_bool"
            | "is_float"
            | "is_null"
            | "is_numeric"
            | "is_array"
            | "is_iterable"
            | "is_object"
            | "get_class"
            | "get_parent_class"
            | "gettype"
            | "boolval"
    )
}

pub(super) fn array_filter_type_predicate_callback(callback: &str) -> bool {
    matches!(
        callback.to_ascii_lowercase().as_str(),
        "is_int" | "is_string" | "is_bool" | "is_float" | "is_null" | "is_array" | "is_iterable"
            | "is_object"
    )
}

pub(super) fn array_filter_object_false_callback(callback: &str) -> bool {
    matches!(
        callback.to_ascii_lowercase().as_str(),
        "is_int" | "is_string" | "is_bool" | "is_float" | "is_null" | "is_numeric" | "is_array"
            | "is_iterable"
    )
}

pub(super) fn array_map_callback_shape(
    callback: &str,
    span: crate::span::Span,
    module: &WasmModule,
) -> Result<ArrayMapCallbackShape, CompileError> {
    if callback.eq_ignore_ascii_case("strlen") {
        return Ok(ArrayMapCallbackShape::StrToInt);
    }
    if callback.eq_ignore_ascii_case("is_int") {
        return Ok(ArrayMapCallbackShape::IntToBool);
    }
    if callback.eq_ignore_ascii_case("is_string") {
        return Ok(ArrayMapCallbackShape::StrToBool);
    }
    if callback.eq_ignore_ascii_case("is_bool") {
        return Ok(ArrayMapCallbackShape::BoolToBool);
    }
    if callback.eq_ignore_ascii_case("is_float") {
        return Ok(ArrayMapCallbackShape::FloatToBool);
    }
    if callback.eq_ignore_ascii_case("is_null") {
        return Ok(ArrayMapCallbackShape::NullToBool);
    }
    if callback.eq_ignore_ascii_case("is_numeric") {
        return Ok(ArrayMapCallbackShape::NumericToBool);
    }
    if callback.eq_ignore_ascii_case("is_array") || callback.eq_ignore_ascii_case("is_iterable") {
        return Ok(ArrayMapCallbackShape::ArrayToBool);
    }
    if callback.eq_ignore_ascii_case("is_object") || callback.eq_ignore_ascii_case("boolval") {
        return Ok(ArrayMapCallbackShape::ObjectToBool);
    }
    if callback.eq_ignore_ascii_case("get_class") {
        return Ok(ArrayMapCallbackShape::ObjectToStr);
    }
    if callback.eq_ignore_ascii_case("get_parent_class") {
        return Ok(ArrayMapCallbackShape::ObjectToParentStr);
    }
    if callback.eq_ignore_ascii_case("gettype") {
        return Ok(ArrayMapCallbackShape::ObjectToTypeStr);
    }
    let Some(param_kinds) = module.function_param_kinds(callback) else {
        return Err(CompileError::new(
            span,
            "wasm32-web array_map() callback metadata is missing",
        ));
    };
    match (param_kinds.as_slice(), module.function_return_kind(callback)) {
        ([LocalKind::I64], Some(ValueKind::Int)) => Ok(ArrayMapCallbackShape::IntToInt),
        ([LocalKind::I64], Some(ValueKind::Bool)) => Ok(ArrayMapCallbackShape::IntToBool),
        ([LocalKind::I64, LocalKind::I64], Some(ValueKind::Int)) => Ok(ArrayMapCallbackShape::IntIntToInt),
        ([LocalKind::I64, LocalKind::I64, LocalKind::I64], Some(ValueKind::Int)) => {
            Ok(ArrayMapCallbackShape::IntIntIntToInt)
        }
        ([LocalKind::I64, LocalKind::I64, LocalKind::I64, LocalKind::I64], Some(ValueKind::Int)) => {
            Ok(ArrayMapCallbackShape::IntIntIntIntToInt)
        }
        (
            [LocalKind::I64, LocalKind::I64, LocalKind::I64, LocalKind::I64, LocalKind::I64],
            Some(ValueKind::Int),
        ) => Ok(ArrayMapCallbackShape::IntIntIntIntIntToInt),
        (
            [
                LocalKind::I64,
                LocalKind::I64,
                LocalKind::I64,
                LocalKind::I64,
                LocalKind::I64,
                LocalKind::I64,
            ],
            Some(ValueKind::Int),
        ) => Ok(ArrayMapCallbackShape::IntIntIntIntIntIntToInt),
        ([LocalKind::Str], Some(ValueKind::Bool)) => Ok(ArrayMapCallbackShape::StrToBool),
        ([LocalKind::Str], Some(ValueKind::Str)) => Ok(ArrayMapCallbackShape::StrToStr),
        ([LocalKind::I32], Some(ValueKind::Bool)) => Ok(ArrayMapCallbackShape::BoolToBool),
        ([LocalKind::F64], Some(ValueKind::Float)) => Ok(ArrayMapCallbackShape::FloatToFloat),
        ([LocalKind::F64], Some(ValueKind::Bool)) => Ok(ArrayMapCallbackShape::FloatToBool),
        ([LocalKind::Str, LocalKind::Str], Some(ValueKind::Str)) => Ok(ArrayMapCallbackShape::StrStrToStr),
        ([LocalKind::Str, LocalKind::Str, LocalKind::Str], Some(ValueKind::Str)) => {
            Ok(ArrayMapCallbackShape::StrStrStrToStr)
        }
        ([LocalKind::Str, LocalKind::Str, LocalKind::Str, LocalKind::Str], Some(ValueKind::Str)) => {
            Ok(ArrayMapCallbackShape::StrStrStrStrToStr)
        }
        (
            [LocalKind::Str, LocalKind::Str, LocalKind::Str, LocalKind::Str, LocalKind::Str],
            Some(ValueKind::Str),
        ) => Ok(ArrayMapCallbackShape::StrStrStrStrStrToStr),
        (
            [
                LocalKind::Str,
                LocalKind::Str,
                LocalKind::Str,
                LocalKind::Str,
                LocalKind::Str,
                LocalKind::Str,
            ],
            Some(ValueKind::Str),
        ) => Ok(ArrayMapCallbackShape::StrStrStrStrStrStrToStr),
        ([LocalKind::Mixed, LocalKind::Mixed], Some(ValueKind::Int)) => Ok(ArrayMapCallbackShape::MixedMixedToInt),
        ([LocalKind::Mixed, LocalKind::Mixed], Some(ValueKind::Str)) => Ok(ArrayMapCallbackShape::MixedMixedToStr),
        ([LocalKind::Mixed, LocalKind::Mixed], Some(ValueKind::Bool)) => Ok(ArrayMapCallbackShape::MixedMixedToBool),
        ([LocalKind::Mixed, LocalKind::Mixed], Some(ValueKind::Float)) => {
            Ok(ArrayMapCallbackShape::MixedMixedToFloat)
        }
        ([LocalKind::Mixed, LocalKind::Mixed, LocalKind::Mixed], Some(ValueKind::Int)) => {
            Ok(ArrayMapCallbackShape::MixedMixedMixedToInt)
        }
        ([LocalKind::Mixed, LocalKind::Mixed, LocalKind::Mixed], Some(ValueKind::Str)) => {
            Ok(ArrayMapCallbackShape::MixedMixedMixedToStr)
        }
        ([LocalKind::Mixed, LocalKind::Mixed, LocalKind::Mixed], Some(ValueKind::Bool)) => {
            Ok(ArrayMapCallbackShape::MixedMixedMixedToBool)
        }
        ([LocalKind::Mixed, LocalKind::Mixed, LocalKind::Mixed], Some(ValueKind::Float)) => {
            Ok(ArrayMapCallbackShape::MixedMixedMixedToFloat)
        }
        ([LocalKind::Mixed, LocalKind::Mixed, LocalKind::Mixed, LocalKind::Mixed], Some(ValueKind::Int)) => {
            Ok(ArrayMapCallbackShape::MixedMixedMixedMixedToInt)
        }
        ([LocalKind::Mixed, LocalKind::Mixed, LocalKind::Mixed, LocalKind::Mixed], Some(ValueKind::Str)) => {
            Ok(ArrayMapCallbackShape::MixedMixedMixedMixedToStr)
        }
        ([LocalKind::Mixed, LocalKind::Mixed, LocalKind::Mixed, LocalKind::Mixed], Some(ValueKind::Bool)) => {
            Ok(ArrayMapCallbackShape::MixedMixedMixedMixedToBool)
        }
        ([LocalKind::Mixed, LocalKind::Mixed, LocalKind::Mixed, LocalKind::Mixed], Some(ValueKind::Float)) => {
            Ok(ArrayMapCallbackShape::MixedMixedMixedMixedToFloat)
        }
        (
            [
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
            ],
            Some(ValueKind::Int),
        ) => Ok(ArrayMapCallbackShape::MixedMixedMixedMixedMixedToInt),
        (
            [
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
            ],
            Some(ValueKind::Str),
        ) => Ok(ArrayMapCallbackShape::MixedMixedMixedMixedMixedToStr),
        (
            [
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
            ],
            Some(ValueKind::Bool),
        ) => Ok(ArrayMapCallbackShape::MixedMixedMixedMixedMixedToBool),
        (
            [
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
            ],
            Some(ValueKind::Float),
        ) => Ok(ArrayMapCallbackShape::MixedMixedMixedMixedMixedToFloat),
        (
            [
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
            ],
            Some(ValueKind::Int),
        ) => Ok(ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToInt),
        (
            [
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
            ],
            Some(ValueKind::Str),
        ) => Ok(ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToStr),
        (
            [
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
            ],
            Some(ValueKind::Bool),
        ) => Ok(ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToBool),
        (
            [
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
            ],
            Some(ValueKind::Float),
        ) => Ok(ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToFloat),
        (
            [
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
            ],
            Some(ValueKind::Int),
        ) => Ok(ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToInt),
        (
            [
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
            ],
            Some(ValueKind::Str),
        ) => Ok(ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToStr),
        (
            [
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
            ],
            Some(ValueKind::Bool),
        ) => Ok(ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToBool),
        (
            [
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
            ],
            Some(ValueKind::Float),
        ) => Ok(ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToFloat),
        (
            [
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
            ],
            Some(ValueKind::Int),
        ) => Ok(ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToInt),
        (
            [
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
            ],
            Some(ValueKind::Str),
        ) => Ok(ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToStr),
        (
            [
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
            ],
            Some(ValueKind::Bool),
        ) => Ok(ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToBool),
        (
            [
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
            ],
            Some(ValueKind::Float),
        ) => Ok(ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToFloat),
        (
            [
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
            ],
            Some(ValueKind::Int),
        ) => Ok(ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToInt),
        (
            [
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
            ],
            Some(ValueKind::Str),
        ) => Ok(ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToStr),
        (
            [
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
            ],
            Some(ValueKind::Bool),
        ) => Ok(ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToBool),
        (
            [
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
                LocalKind::Mixed,
            ],
            Some(ValueKind::Float),
        ) => Ok(ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToFloat),
        _ => Err(CompileError::new(
            span,
            "wasm32-web array_map() currently supports homogeneous int/string callbacks and selected mixed callback results up to nine arguments",
        )),
    }
}

pub(super) fn array_map_builtin_callback_is_supported(callback: &str) -> bool {
    matches!(
        callback.to_ascii_lowercase().as_str(),
        "strlen" | "is_int" | "is_string" | "is_bool" | "is_float" | "is_null"
            | "is_numeric"
            | "is_array"
            | "is_iterable"
            | "is_object"
            | "get_class"
            | "get_parent_class"
            | "gettype"
            | "boolval"
    )
}

pub(super) fn array_map_items_are_ints(items: &[Expr], module: &WasmModule) -> bool {
    value_cell_kinds_for_items(items, module)
        .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Int)))
}

pub(super) fn array_map_items_are_strings(items: &[Expr], module: &WasmModule) -> bool {
    value_cell_kinds_for_items(items, module)
        .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Str)))
}

pub(super) fn array_map_strlen_value_kind_is_supported(kind: ValueCellKind) -> bool {
    matches!(
        kind,
        ValueCellKind::Int | ValueCellKind::Str | ValueCellKind::Bool | ValueCellKind::Float | ValueCellKind::Null
    )
}

pub(super) fn array_reduce_string_callback_value_kind_is_supported(kind: ValueCellKind) -> bool {
    matches!(
        kind,
        ValueCellKind::Int | ValueCellKind::Str | ValueCellKind::Bool | ValueCellKind::Float
    )
}

pub(super) fn array_reduce_float_callback_value_kind_is_supported(kind: ValueCellKind) -> bool {
    matches!(kind, ValueCellKind::Int | ValueCellKind::Bool | ValueCellKind::Str)
}

pub(super) fn array_reduce_bool_callback_value_kind_is_supported(kind: ValueCellKind) -> bool {
    matches!(
        kind,
        ValueCellKind::Int | ValueCellKind::Bool | ValueCellKind::Float | ValueCellKind::Str
    )
}

pub(super) fn array_reduce_bool_callback_items_are_supported(
    items: &[Expr],
    module: &WasmModule,
) -> bool {
    value_cell_kinds_for_items(items, module).is_some_and(|kinds| {
        kinds
            .iter()
            .all(|kind| array_reduce_bool_callback_value_kind_is_supported(*kind))
    })
}

pub(super) fn array_reduce_bool_callback_assoc_items_are_supported(
    items: &[(Expr, Expr)],
    module: &WasmModule,
) -> bool {
    value_cell_kinds_for_assoc_items(items, module).is_some_and(|kinds| {
        kinds
            .iter()
            .all(|kind| array_reduce_bool_callback_value_kind_is_supported(*kind))
    })
}

pub(super) fn array_reduce_bool_callback_value_cells_are_supported(
    source: &str,
    module: &WasmModule,
) -> bool {
    module.array_value_cell_kinds(source).is_some_and(|kinds| {
        kinds
            .iter()
            .all(|kind| array_reduce_bool_callback_value_kind_is_supported(*kind))
    }) || module
        .array_runtime_value_cell_kind(source)
        .is_some_and(array_reduce_bool_callback_value_kind_is_supported)
}

pub(super) fn array_reduce_bool_callback_assoc_local_is_supported(
    source: &str,
    module: &WasmModule,
) -> bool {
    module.array_value_cell_kinds(source).is_some_and(|kinds| {
        kinds
            .iter()
            .all(|kind| array_reduce_bool_callback_value_kind_is_supported(*kind))
    }) || module
        .array_runtime_value_cell_kind(source)
        .is_some_and(array_reduce_bool_callback_value_kind_is_supported)
}

pub(super) fn array_reduce_float_callback_items_are_supported(
    items: &[Expr],
    module: &WasmModule,
) -> bool {
    value_cell_kinds_for_items(items, module).is_some_and(|kinds| {
        kinds
            .iter()
            .all(|kind| array_reduce_float_callback_value_kind_is_supported(*kind))
    })
}

pub(super) fn array_reduce_float_callback_assoc_items_are_supported(
    items: &[(Expr, Expr)],
    module: &WasmModule,
) -> bool {
    value_cell_kinds_for_assoc_items(items, module).is_some_and(|kinds| {
        kinds
            .iter()
            .all(|kind| array_reduce_float_callback_value_kind_is_supported(*kind))
    })
}

pub(super) fn array_reduce_float_callback_value_cells_are_supported(
    source: &str,
    module: &WasmModule,
) -> bool {
    module.array_value_cell_kinds(source).is_some_and(|kinds| {
        kinds
            .iter()
            .all(|kind| array_reduce_float_callback_value_kind_is_supported(*kind))
    }) || module
        .array_runtime_value_cell_kind(source)
        .is_some_and(array_reduce_float_callback_value_kind_is_supported)
}

pub(super) fn array_reduce_float_callback_assoc_local_is_supported(
    source: &str,
    module: &WasmModule,
) -> bool {
    module.array_value_cell_kinds(source).is_some_and(|kinds| {
        kinds
            .iter()
            .all(|kind| array_reduce_float_callback_value_kind_is_supported(*kind))
    })
}

pub(super) fn array_reduce_string_callback_items_are_supported(
    items: &[Expr],
    module: &WasmModule,
) -> bool {
    value_cell_kinds_for_items(items, module).is_some_and(|kinds| {
        kinds
            .iter()
            .all(|kind| array_reduce_string_callback_value_kind_is_supported(*kind))
    })
}

pub(super) fn array_reduce_string_callback_assoc_items_are_supported(
    items: &[(Expr, Expr)],
    module: &WasmModule,
) -> bool {
    value_cell_kinds_for_assoc_items(items, module).is_some_and(|kinds| {
        kinds
            .iter()
            .all(|kind| array_reduce_string_callback_value_kind_is_supported(*kind))
    })
}

pub(super) fn array_reduce_string_callback_value_cells_are_supported(
    source: &str,
    module: &WasmModule,
) -> bool {
    module
        .array_value_cell_kinds(source)
        .is_some_and(|kinds| {
            kinds
                .iter()
                .all(|kind| array_reduce_string_callback_value_kind_is_supported(*kind))
        })
        || module
            .array_runtime_value_cell_kind(source)
            .is_some_and(array_reduce_string_callback_value_kind_is_supported)
}

pub(super) fn array_reduce_string_callback_assoc_local_is_supported(
    source: &str,
    module: &WasmModule,
) -> bool {
    module.array_length(source).is_some()
        && module.array_value_cell_kinds(source).is_some_and(|kinds| {
            kinds
                .iter()
                .all(|kind| array_reduce_string_callback_value_kind_is_supported(*kind))
        })
}

pub(super) fn array_reduce_string_callback_runtime_assoc_local_is_supported(
    source: &str,
    module: &WasmModule,
) -> bool {
    module.array_length(source).is_none()
        && module
            .array_runtime_value_cell_kind(source)
            .is_some_and(array_reduce_string_callback_value_kind_is_supported)
}

pub(super) fn array_map_strlen_items_are_supported(items: &[Expr], module: &WasmModule) -> bool {
    value_cell_kinds_for_items(items, module)
        .is_some_and(|kinds| kinds.iter().all(|kind| array_map_strlen_value_kind_is_supported(*kind)))
}

pub(super) fn array_map_strlen_value_cells_are_supported(source: &str, module: &WasmModule) -> bool {
    module
        .array_value_cell_kinds(source)
        .is_some_and(|kinds| kinds.iter().all(|kind| array_map_strlen_value_kind_is_supported(*kind)))
        || module
            .array_runtime_value_cell_kind(source)
            .is_some_and(array_map_strlen_value_kind_is_supported)
}

pub(super) fn array_map_strlen_assoc_items_are_supported(
    items: &[(Expr, Expr)],
    module: &WasmModule,
) -> bool {
    value_cell_kinds_for_assoc_items(items, module)
        .is_some_and(|kinds| kinds.iter().all(|kind| array_map_strlen_value_kind_is_supported(*kind)))
}

pub(super) fn array_map_strlen_assoc_local_is_supported(source: &str, module: &WasmModule) -> bool {
    module.array_length(source).is_some()
        && module
            .array_value_cell_kinds(source)
            .is_some_and(|kinds| kinds.iter().all(|kind| array_map_strlen_value_kind_is_supported(*kind)))
}

pub(super) fn array_map_strlen_runtime_assoc_local_is_supported(source: &str, module: &WasmModule) -> bool {
    module.array_length(source).is_none()
        && module
            .array_runtime_value_cell_kind(source)
            .is_some_and(array_map_strlen_value_kind_is_supported)
}

pub(super) fn array_filter_items_are_bools(items: &[Expr], module: &WasmModule) -> bool {
    value_cell_kinds_for_items(items, module)
        .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Bool)))
}

pub(super) fn array_filter_items_are_floats(items: &[Expr], module: &WasmModule) -> bool {
    value_cell_kinds_for_items(items, module)
        .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Float)))
}

pub(super) fn array_filter_items_are_nulls(items: &[Expr], module: &WasmModule) -> bool {
    value_cell_kinds_for_items(items, module)
        .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Null)))
}

pub(super) fn array_filter_items_are_arrays(items: &[Expr], module: &WasmModule) -> bool {
    value_cell_kinds_for_items(items, module)
        .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Array)))
}

pub(super) fn array_map_value_cells_are_ints(source: &str, module: &WasmModule) -> bool {
    module
        .array_value_cell_kinds(source)
        .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Int)))
        || module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Int)
}

pub(super) fn array_map_value_cells_are_strings(source: &str, module: &WasmModule) -> bool {
    module
        .array_value_cell_kinds(source)
        .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Str)))
        || module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Str)
}

pub(super) fn array_filter_value_cells_are_bools(source: &str, module: &WasmModule) -> bool {
    module
        .array_value_cell_kinds(source)
        .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Bool)))
        || module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Bool)
}

pub(super) fn array_filter_value_cells_are_floats(source: &str, module: &WasmModule) -> bool {
    module
        .array_value_cell_kinds(source)
        .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Float)))
        || module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Float)
}

pub(super) fn array_filter_value_cells_are_nulls(source: &str, module: &WasmModule) -> bool {
    module
        .array_value_cell_kinds(source)
        .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Null)))
        || module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Null)
}

pub(super) fn array_filter_value_cells_are_arrays(source: &str, module: &WasmModule) -> bool {
    module
        .array_value_cell_kinds(source)
        .is_some_and(|kinds| kinds.iter().all(|kind| matches!(kind, ValueCellKind::Array)))
        || module.array_runtime_value_cell_kind(source) == Some(ValueCellKind::Array)
}

pub(super) fn array_filter_assoc_value_cells_are_supported(source: &str, module: &WasmModule) -> bool {
    module.array_length(source).is_some()
        && module.array_value_cell_kinds(source).is_some_and(|kinds| {
            kinds.iter().copied().all(array_filter_value_cell_kind_is_supported)
        })
}

pub(super) fn array_filter_runtime_assoc_value_cells_are_supported(source: &str, module: &WasmModule) -> bool {
    module.array_length(source).is_none()
        && module
            .array_runtime_value_cell_kind(source)
            .is_some_and(array_filter_value_cell_kind_is_supported)
}

pub(super) fn array_filter_value_cell_kind_is_supported(kind: ValueCellKind) -> bool {
    matches!(
        kind,
        ValueCellKind::Int
            | ValueCellKind::Str
            | ValueCellKind::Bool
            | ValueCellKind::Float
            | ValueCellKind::Null
            | ValueCellKind::Array
    )
}

pub(super) fn array_filter_assoc_items_are_supported(items: &[(Expr, Expr)], module: &WasmModule) -> bool {
    value_cell_kinds_for_assoc_items(items, module).is_some_and(|kinds| {
        kinds.iter().all(|kind| {
            matches!(
                kind,
                ValueCellKind::Int
                    | ValueCellKind::Str
                    | ValueCellKind::Bool
                    | ValueCellKind::Float
                    | ValueCellKind::Null
                    | ValueCellKind::Array
            )
        })
    })
}

pub(super) fn array_filter_assoc_items_match_shape(
    items: &[(Expr, Expr)],
    shape: ArrayFilterCallbackShape,
    module: &WasmModule,
) -> bool {
    value_cell_kinds_for_assoc_items(items, module).is_some_and(|kinds| {
        kinds.iter().all(|kind| {
            matches!(
                (shape, *kind),
                (ArrayFilterCallbackShape::Int, ValueCellKind::Int)
                    | (ArrayFilterCallbackShape::Str, ValueCellKind::Str)
                    | (ArrayFilterCallbackShape::Bool, ValueCellKind::Bool)
                    | (ArrayFilterCallbackShape::Float, ValueCellKind::Float)
                    | (ArrayFilterCallbackShape::Null, ValueCellKind::Null)
                    | (ArrayFilterCallbackShape::Numeric, ValueCellKind::Int)
                    | (ArrayFilterCallbackShape::Numeric, ValueCellKind::Float)
                    | (ArrayFilterCallbackShape::Numeric, ValueCellKind::Str)
                    | (ArrayFilterCallbackShape::Array, ValueCellKind::Array)
            )
        })
    })
}

pub(super) fn array_filter_strlen_value_kind_is_supported(kind: ValueCellKind) -> bool {
    matches!(
        kind,
        ValueCellKind::Int
            | ValueCellKind::Str
            | ValueCellKind::Bool
            | ValueCellKind::Float
            | ValueCellKind::Null
    )
}

pub(super) fn array_filter_strlen_assoc_items_are_supported(items: &[(Expr, Expr)], module: &WasmModule) -> bool {
    value_cell_kinds_for_assoc_items(items, module).is_some_and(|kinds| {
        kinds
            .iter()
            .all(|kind| array_filter_strlen_value_kind_is_supported(*kind))
    })
}

pub(super) fn array_filter_strlen_assoc_local_is_supported(source: &str, module: &WasmModule) -> bool {
    module.array_length(source).is_some()
        && module.array_value_cell_kinds(source).is_some_and(|kinds| {
            kinds
                .iter()
                .all(|kind| array_filter_strlen_value_kind_is_supported(*kind))
        })
}

pub(super) fn array_filter_strlen_runtime_assoc_local_is_supported(source: &str, module: &WasmModule) -> bool {
    module.array_length(source).is_none()
        && module
            .array_runtime_value_cell_kind(source)
            .is_some_and(array_filter_strlen_value_kind_is_supported)
}

pub(super) fn array_filter_assoc_local_matches_shape(
    source: &str,
    shape: ArrayFilterCallbackShape,
    module: &WasmModule,
) -> bool {
    module.array_length(source).is_some()
        && module.array_value_cell_kinds(source).is_some_and(|kinds| {
            kinds.iter().all(|kind| {
                matches!(
                    (shape, *kind),
                    (ArrayFilterCallbackShape::Int, ValueCellKind::Int)
                        | (ArrayFilterCallbackShape::Str, ValueCellKind::Str)
                        | (ArrayFilterCallbackShape::Bool, ValueCellKind::Bool)
                        | (ArrayFilterCallbackShape::Float, ValueCellKind::Float)
                        | (ArrayFilterCallbackShape::Null, ValueCellKind::Null)
                        | (ArrayFilterCallbackShape::Numeric, ValueCellKind::Int)
                        | (ArrayFilterCallbackShape::Numeric, ValueCellKind::Float)
                        | (ArrayFilterCallbackShape::Numeric, ValueCellKind::Str)
                        | (ArrayFilterCallbackShape::Array, ValueCellKind::Array)
                )
            })
        })
}

pub(super) fn array_filter_runtime_assoc_local_matches_shape(
    source: &str,
    shape: ArrayFilterCallbackShape,
    module: &WasmModule,
) -> bool {
    module.array_length(source).is_none()
        && module
            .array_runtime_value_cell_kind(source)
            .is_some_and(|kind| array_filter_value_kind_matches_shape(kind, shape))
}

pub(super) fn array_filter_value_kind_matches_shape(
    kind: ValueCellKind,
    shape: ArrayFilterCallbackShape,
) -> bool {
    matches!(
        (shape, kind),
        (ArrayFilterCallbackShape::Int, ValueCellKind::Int)
            | (ArrayFilterCallbackShape::Str, ValueCellKind::Str)
            | (ArrayFilterCallbackShape::Bool, ValueCellKind::Bool)
            | (ArrayFilterCallbackShape::Float, ValueCellKind::Float)
            | (ArrayFilterCallbackShape::Null, ValueCellKind::Null)
            | (ArrayFilterCallbackShape::Numeric, ValueCellKind::Int)
            | (ArrayFilterCallbackShape::Numeric, ValueCellKind::Float)
            | (ArrayFilterCallbackShape::Numeric, ValueCellKind::Str)
            | (ArrayFilterCallbackShape::Array, ValueCellKind::Array)
    )
}

pub(super) fn array_map_assoc_items_match_shape(
    items: &[(Expr, Expr)],
    shape: ArrayMapCallbackShape,
    module: &WasmModule,
) -> bool {
    value_cell_kinds_for_assoc_items(items, module).is_some_and(|kinds| {
        kinds.iter().all(|kind| {
            matches!(
                (shape, *kind),
                (ArrayMapCallbackShape::IntToInt, ValueCellKind::Int)
                    | (ArrayMapCallbackShape::IntToInt, ValueCellKind::Bool)
                    | (ArrayMapCallbackShape::IntToInt, ValueCellKind::Float)
                    | (ArrayMapCallbackShape::IntToInt, ValueCellKind::Str)
                    | (ArrayMapCallbackShape::IntToBool, ValueCellKind::Int)
                    | (ArrayMapCallbackShape::IntIntToInt, ValueCellKind::Int)
                    | (ArrayMapCallbackShape::IntIntIntToInt, ValueCellKind::Int)
                    | (ArrayMapCallbackShape::IntIntIntIntToInt, ValueCellKind::Int)
                    | (ArrayMapCallbackShape::IntIntIntIntIntToInt, ValueCellKind::Int)
                    | (ArrayMapCallbackShape::IntIntIntIntIntIntToInt, ValueCellKind::Int)
                    | (ArrayMapCallbackShape::StrToInt, ValueCellKind::Str)
                    | (ArrayMapCallbackShape::StrToBool, ValueCellKind::Str)
                    | (ArrayMapCallbackShape::StrToStr, ValueCellKind::Str)
                    | (ArrayMapCallbackShape::BoolToBool, ValueCellKind::Bool)
                    | (ArrayMapCallbackShape::FloatToFloat, ValueCellKind::Float)
                    | (ArrayMapCallbackShape::FloatToBool, ValueCellKind::Float)
                    | (ArrayMapCallbackShape::NullToBool, ValueCellKind::Null)
                    | (ArrayMapCallbackShape::NumericToBool, ValueCellKind::Int)
                    | (ArrayMapCallbackShape::NumericToBool, ValueCellKind::Float)
                    | (ArrayMapCallbackShape::NumericToBool, ValueCellKind::Str)
                    | (ArrayMapCallbackShape::ArrayToBool, ValueCellKind::Array)
                    | (ArrayMapCallbackShape::StrStrToStr, ValueCellKind::Str)
                    | (ArrayMapCallbackShape::StrStrStrToStr, ValueCellKind::Str)
                    | (ArrayMapCallbackShape::StrStrStrStrToStr, ValueCellKind::Str)
            )
        })
    })
}

pub(super) fn array_map_assoc_local_matches_shape(
    source: &str,
    shape: ArrayMapCallbackShape,
    module: &WasmModule,
) -> bool {
    module.array_length(source).is_some()
        && module.array_value_cell_kinds(source).is_some_and(|kinds| {
            kinds.iter().all(|kind| {
                matches!(
                    (shape, *kind),
                    (ArrayMapCallbackShape::IntToInt, ValueCellKind::Int)
                        | (ArrayMapCallbackShape::IntToInt, ValueCellKind::Bool)
                        | (ArrayMapCallbackShape::IntToInt, ValueCellKind::Float)
                        | (ArrayMapCallbackShape::IntToInt, ValueCellKind::Str)
                        | (ArrayMapCallbackShape::IntToBool, ValueCellKind::Int)
                        | (ArrayMapCallbackShape::IntIntToInt, ValueCellKind::Int)
                        | (ArrayMapCallbackShape::IntIntIntToInt, ValueCellKind::Int)
                        | (ArrayMapCallbackShape::IntIntIntIntToInt, ValueCellKind::Int)
                        | (ArrayMapCallbackShape::IntIntIntIntIntToInt, ValueCellKind::Int)
                        | (ArrayMapCallbackShape::IntIntIntIntIntIntToInt, ValueCellKind::Int)
                        | (ArrayMapCallbackShape::StrToInt, ValueCellKind::Str)
                        | (ArrayMapCallbackShape::StrToBool, ValueCellKind::Str)
                        | (ArrayMapCallbackShape::StrToStr, ValueCellKind::Str)
                        | (ArrayMapCallbackShape::BoolToBool, ValueCellKind::Bool)
                        | (ArrayMapCallbackShape::FloatToFloat, ValueCellKind::Float)
                        | (ArrayMapCallbackShape::FloatToBool, ValueCellKind::Float)
                        | (ArrayMapCallbackShape::NullToBool, ValueCellKind::Null)
                        | (ArrayMapCallbackShape::NumericToBool, ValueCellKind::Int)
                        | (ArrayMapCallbackShape::NumericToBool, ValueCellKind::Float)
                        | (ArrayMapCallbackShape::NumericToBool, ValueCellKind::Str)
                        | (ArrayMapCallbackShape::ArrayToBool, ValueCellKind::Array)
                        | (ArrayMapCallbackShape::StrStrToStr, ValueCellKind::Str)
                        | (ArrayMapCallbackShape::StrStrStrToStr, ValueCellKind::Str)
                        | (ArrayMapCallbackShape::StrStrStrStrToStr, ValueCellKind::Str)
                )
            })
        })
}

pub(super) fn array_map_runtime_assoc_local_matches_shape(
    source: &str,
    shape: ArrayMapCallbackShape,
    module: &WasmModule,
) -> bool {
    module.array_runtime_value_cell_kind(source).is_some_and(|kind| {
        matches!(
            (shape, kind),
            (ArrayMapCallbackShape::IntToInt, ValueCellKind::Int)
                | (ArrayMapCallbackShape::IntToInt, ValueCellKind::Bool)
                | (ArrayMapCallbackShape::IntToInt, ValueCellKind::Float)
                | (ArrayMapCallbackShape::IntToInt, ValueCellKind::Str)
                | (ArrayMapCallbackShape::IntToBool, ValueCellKind::Int)
                | (ArrayMapCallbackShape::StrToInt, ValueCellKind::Str)
                | (ArrayMapCallbackShape::StrToBool, ValueCellKind::Str)
                | (ArrayMapCallbackShape::StrToStr, ValueCellKind::Str)
                | (ArrayMapCallbackShape::BoolToBool, ValueCellKind::Bool)
                | (ArrayMapCallbackShape::FloatToFloat, ValueCellKind::Float)
                | (ArrayMapCallbackShape::FloatToBool, ValueCellKind::Float)
                | (ArrayMapCallbackShape::NullToBool, ValueCellKind::Null)
                | (ArrayMapCallbackShape::NumericToBool, ValueCellKind::Int)
                | (ArrayMapCallbackShape::NumericToBool, ValueCellKind::Float)
                | (ArrayMapCallbackShape::NumericToBool, ValueCellKind::Str)
                | (ArrayMapCallbackShape::ArrayToBool, ValueCellKind::Array)
        )
    })
}

pub(super) fn array_map_result_value_cell_kind(shape: ArrayMapCallbackShape) -> ValueCellKind {
    match shape {
        ArrayMapCallbackShape::IntToInt | ArrayMapCallbackShape::StrToInt => ValueCellKind::Int,
        ArrayMapCallbackShape::FloatToFloat => ValueCellKind::Float,
        ArrayMapCallbackShape::StrToStr
        | ArrayMapCallbackShape::ObjectToStr
        | ArrayMapCallbackShape::ObjectToParentStr
        | ArrayMapCallbackShape::ObjectToTypeStr => ValueCellKind::Str,
        ArrayMapCallbackShape::IntToBool
        | ArrayMapCallbackShape::StrToBool
        | ArrayMapCallbackShape::BoolToBool
        | ArrayMapCallbackShape::FloatToBool
        | ArrayMapCallbackShape::NullToBool
        | ArrayMapCallbackShape::NumericToBool
        | ArrayMapCallbackShape::ArrayToBool
        | ArrayMapCallbackShape::ObjectToBool => ValueCellKind::Bool,
        ArrayMapCallbackShape::IntIntToInt
        | ArrayMapCallbackShape::IntIntIntToInt
        | ArrayMapCallbackShape::IntIntIntIntToInt
        | ArrayMapCallbackShape::IntIntIntIntIntToInt
        | ArrayMapCallbackShape::IntIntIntIntIntIntToInt
        | ArrayMapCallbackShape::StrStrToStr
        | ArrayMapCallbackShape::StrStrStrToStr
        | ArrayMapCallbackShape::StrStrStrStrToStr
        | ArrayMapCallbackShape::StrStrStrStrStrToStr
        | ArrayMapCallbackShape::StrStrStrStrStrStrToStr
        | ArrayMapCallbackShape::MixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedToFloat
        | ArrayMapCallbackShape::MixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedToFloat
        | ArrayMapCallbackShape::MixedMixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedMixedToFloat
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedToFloat
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedToFloat
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedToFloat
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedToFloat
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToInt
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToStr
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToBool
        | ArrayMapCallbackShape::MixedMixedMixedMixedMixedMixedMixedMixedMixedToFloat => {
            unreachable!("multi-array map cannot preserve one assoc source")
        }
    }
}
