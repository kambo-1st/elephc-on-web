//! Purpose:
//! Lowers scalar PHP expressions into browser WebAssembly text instructions.
//! Handles runtime integer/boolean values plus direct string literal/local output.
//!
//! Called from:
//! - `crate::codegen::wasm::stmt`
//!
//! Key details:
//! - String concatenation writes output directly or materializes runtime string values on the heap.

use crate::errors::CompileError;
use crate::names::Name;
use crate::parser::ast::{BinOp, CallableTarget, CastType, Expr, ExprKind, StaticReceiver, Stmt, StmtKind};
use crate::span::Span;
use std::borrow::Cow;

mod array_aggregates;
mod array_assign;
mod assign_value;
mod array_builders;
mod array_chunk;
mod array_chunk_static;
mod array_assignment;
mod array_assignment_assoc_key;
mod array_assignment_sparse;
mod array_map_assign;
mod array_filter_assign;
mod array_filter_assign_default;
mod array_filter_assign_modes;
mod array_filter_assoc_args;
mod array_filter_callback_modes;
mod array_filter_default;
mod array_filter_helpers;
mod array_filter_literal;
mod array_filter_runtime_scalars;
mod array_filter_truthiness;
mod array_map_assoc_emitters;
mod array_map_filter;
mod array_map_filter_assoc;
mod array_map_filter_value_strings;
mod array_map_emitters;
mod array_map_multi_emitters;
mod array_map_null_assign;
mod array_map_strlen_emitters;
mod array_map_value_emitters;
mod array_map_filter_shapes;
mod array_merge;
mod array_merge_assoc_emitters;
mod array_merge_key_set;
mod array_merge_metadata;
mod array_merge_value;
mod array_pad;
mod array_pad_assoc;
mod array_pad_dynamic;
mod array_primitives;
mod array_range;
mod array_rand;
mod array_materialization;
mod array_args;
mod array_contains;
mod array_contains_assoc;
mod array_contains_values;
mod array_indexing;
mod array_indexing_assoc;
mod array_indexing_chunks;
mod array_indexing_metadata;
mod array_indexing_nested;
mod array_indexing_value_cells;
mod array_membership;
mod array_metadata;
mod array_nested_assignment;
mod array_reduce;
mod array_reduce_assoc;
mod array_reduce_values;
mod array_reverse;
mod array_search;
mod array_search_assoc;
mod array_search_output;
mod array_search_static;
mod array_search_value;
mod array_slice;
mod array_slice_dynamic;
mod array_slice_preserve;
mod array_splice;
mod array_splice_metadata;
mod array_mutators;
mod array_numeric_folds;
mod array_mutator_assoc_entries;
mod array_mutator_assoc_value_sort;
mod array_mutator_push;
mod array_mutator_shuffle;
mod array_mutator_sorts;
mod array_mutator_unshift;
mod array_output;
mod array_sort;
mod array_sort_usort;
mod array_sort_uasort;
mod array_sort_uksort;
mod array_string_split;
mod array_transform_sets;
mod array_transform_static;
mod array_transforms;
mod array_value_cells;
mod array_value_cell_sort;
mod array_value_cell_assoc_reads;
mod array_walk;
mod array_walk_value;
mod chr_runtime;
mod dispatch;
mod enums;
mod existence;
mod expression_kinds;
mod float_math;
mod functions;
mod hash_builtins;
mod html_entities;
mod implode;
mod implode_output;
mod implode_runtime;
mod literal_encoding;
mod literal_strings;
mod string_builtin_materialization;
mod string_builtins;
mod string_cast_materialization;
mod string_comparison;
mod string_control_values;
mod string_int_predicates;
mod string_materialization;
mod string_offset_assignment;
mod string_bytes;
mod string_byte_classes;
mod string_memory;
mod string_predicates;
mod string_search;
mod string_search_find;
mod string_search_literals;
mod string_strstr;
mod string_runtime;
mod string_runtime_base64;
mod string_runtime_entity_decode;
mod string_runtime_entity_numeric;
mod string_runtime_html_escape;
mod string_runtime_hex;
mod string_runtime_slashes;
mod string_runtime_nl2br;
mod string_runtime_url;
mod substring_runtime;
mod trim_runtime;
mod string_arg_materialization;
mod string_values;
mod string_values_sprintf;
mod sprintf;
mod sprintf_output;
mod sprintf_segments;
mod sprintf_runtime_helpers;
mod sprintf_writers;
mod sprintf_literal;
mod static_args;
mod truthiness;
mod wordwrap;
mod path_builtins;
mod path_builtins_literal;
mod pathinfo_flags;
mod string_repeat;
mod string_replace;
mod str_pad;
mod json_encode;
mod json_encode_output_arrays;
mod json_encode_runtime_arrays;
mod json_literal;
mod json_memory_string;
mod json_output_metadata;
mod json_runtime_string;
mod json_state;
mod mixed_assign;
mod mixed_values;
mod number_format;
mod numeric_predicates;
mod output;
mod output_helpers;
mod objects;
mod runtime_args;
mod scalar_constants;
mod type_predicates;
mod value_flow;
mod scalar_builtins;
mod scalar_ops;
mod scalar_mixed_comparisons;
mod scalar_mixed_numeric;
mod scalar_numeric_strings;
mod scalar_string_coercion;
mod scalar_casts;
mod scalar_static;
mod sscanf;

pub(super) use self::assign_value::emit_assign_value;
pub(super) use self::dispatch::{
    array_unsupported, emit_expr, emit_mixed_f64_payload, emit_mixed_i32_payload,
    emit_mixed_i64_payload,
    is_array_value_expr, require_float, require_int,
};
use self::array_aggregates::{
    emit_array_rand_assign, emit_array_rand_call, emit_count_call, emit_numeric_array_fold_call,
    known_indexed_array_expr_len,
};
pub(super) use self::array_assign::{callable_expr_array_return_metadata, emit_array_assign};
use self::array_builders::*;
use self::array_chunk::*;
use self::array_chunk_static::*;
pub(super) use self::array_map_assign::{
    emit_array_map_assign, materialize_array_map_multi_source,
};
use self::array_map_null_assign::emit_array_map_null_two_array_assign;
pub(super) use self::array_filter_assign::emit_array_filter_assign;
use self::array_filter_callback_modes::*;
use self::array_filter_helpers::*;
use self::array_filter_literal::*;
use self::array_filter_truthiness::*;
use self::array_map_assoc_emitters::*;
use self::array_map_filter::*;
use self::array_map_filter_assoc::*;
use self::array_map_emitters::*;
use self::array_map_multi_emitters::*;
use self::array_map_strlen_emitters::*;
use self::array_map_value_emitters::*;
use self::array_map_filter_shapes::*;
use self::array_merge::*;
use self::array_merge_assoc_emitters::*;
use self::array_merge_key_set::*;
use self::array_merge_metadata::*;
use self::array_pad::*;
use self::array_primitives::*;
use self::array_range::*;
pub(super) use self::array_materialization::{
    array_access_array_metadata, emit_array_value_to_stack, emit_assoc_array_copy_assign,
    emit_assoc_array_match_to_stack, emit_indexed_array_copy_assign,
    emit_return_array_value_to_stack, emit_value_array_copy_assign, match_result_is_assoc_array,
};
use self::array_slice::*;
use self::array_splice::*;
use self::array_splice_metadata::*;
use self::array_transform_sets::*;
pub(super) use self::string_materialization::*;
pub(super) use self::string_control_values::*;
pub(super) use self::string_offset_assignment::emit_string_offset_assign;
pub(super) use self::string_arg_materialization::*;
pub(super) use self::string_cast_materialization::*;
pub(in crate::codegen::wasm) use self::string_comparison::evaluated_static_callback_function_name;
use self::string_comparison::*;
use self::scalar_numeric_strings::*;
use self::scalar_static::*;
use self::array_transforms::*;
use self::enums::*;
use self::array_assignment::{
    assoc_assign_key, copy_assoc_entry, copy_assoc_entry_key, copy_assoc_entry_value,
    emit_assoc_entry_matches_assign_key, emit_assoc_entry_matches_string_parts,
    emit_store_assoc_assign_key_at_entry, static_assoc_assign_key_metadata, AssocAssignKey,
};
pub(super) use self::array_assignment::{
    emit_array_int_assign, emit_assoc_array_assign, emit_assoc_array_items_assign,
    emit_assoc_array_copy_value_cell_from_indexed, emit_assoc_array_store_int_key,
    emit_assoc_array_store_int_slot_value, emit_assoc_array_store_key,
    emit_assoc_array_store_static_string_value, emit_assoc_array_store_string_range_value,
    emit_release_current_assoc_array, normalize_assoc_items, assoc_key_value_for_expr,
    key_values_for_assoc_items,
};
pub(super) use self::array_args::single_array_variable_arg;
use self::array_contains::emit_array_contains_int;
use self::expression_kinds::*;
pub(super) use self::expression_kinds::expression_is_stringy;
pub(super) use self::array_indexing_value_cells::{
    emit_copy_static_assoc_value_cell, emit_copy_static_value_cell, emit_copy_value_cell,
    emit_output_array_local_index, emit_output_array_value_index, emit_output_value_array_local_index,
    emit_output_value_cell, emit_static_int_slot_as_value_cell, emit_static_null_value_cell,
    emit_value_array_static_cell_addr, emit_value_array_static_payload_addr,
    emit_value_cell_address_for_local,
};
pub(super) use self::array_indexing_metadata::{
    nested_array_access_requires_layout, nested_array_access_unsupported,
    nested_array_index_metadata, nested_array_metadata_for_access_expr,
    nested_array_static_access_kind, static_assoc_access_key,
};
use self::array_indexing::{
    assoc_runtime_scan_value_kind, assoc_value_kind_for_static_key,
    dynamic_nested_array_value_metadata, dynamic_outer_nested_assoc_metadata,
    dynamic_outer_nested_assoc_static_access_kind, dynamic_parent_nested_assoc_static_access_kind,
    emit_array_index_expr, emit_dynamic_outer_assoc_nested_array_parts, emit_nested_array_index_expr,
    emit_output_nested_array_index, homogeneous_nested_assoc_value_kind,
};
use self::array_indexing_chunks::{
    direct_array_chunk_homogeneous_access, direct_array_chunk_return_homogeneous_access,
};
use self::array_membership::{emit_array_key_exists_call, emit_in_array_call};
use self::array_metadata::{
    array_filter_callback_assoc_nested_metadata_for_local,
    array_filter_default_assoc_nested_metadata_for_local,
    array_filter_default_int_key_values_for_items, array_filter_default_int_key_values_for_local,
    array_filter_default_nested_metadata_for_assoc_items,
    array_filter_default_nested_metadata_for_items, array_filter_default_nested_metadata_for_local,
    array_value_constants_after_pop_shift, key_kinds_for_assoc_items,
    nested_array_metadata_for_assoc_items, nested_array_metadata_for_expr,
    nested_array_metadata_for_items, value_cell_constants_for_assoc_items,
    value_cell_constants_for_items, value_cell_kind_for_expr, value_cell_kinds_for_assoc_items,
    value_cell_kinds_for_items,
};
pub(super) use self::array_nested_assignment::{emit_nested_array_assign, emit_nested_array_push};
use self::array_output::{emit_array_truthiness, emit_output_array_index, emit_output_array_marker};
use self::array_reduce::{
    array_reduce_call_is_string, array_reduce_static_callback_return_kind, emit_array_reduce_call,
    is_callback_array_builtin,
};
use self::array_reverse::{
    emit_assoc_array_reverse_assign, emit_indexed_array_reverse_preserve_keys_assign,
};
use self::array_search::{
    array_search_false_comparison, emit_array_search_false_comparison_bool,
    emit_array_search_index, emit_array_search_index_from_args,
};
use self::array_search_output::emit_output_array_search;
use self::array_search_assoc::{
    assoc_array_has_negative_int_key, emit_assoc_array_search_float,
    emit_assoc_array_search_loose_scalar_packed_key,
    emit_assoc_array_search_mixed_key_with_found, emit_assoc_array_search_negative_key_found,
    emit_assoc_array_search_packed_key, mixed_key_search_needle_is_supported,
    negative_key_array_search_needle_is_supported,
    reject_negative_int_key_array_search,
};
use self::array_search_static::{
    assoc_array_value_exprs, static_loose_array_constant_search_index,
    static_loose_assoc_string_key_search_result, static_loose_value_array_search_index,
};
pub(super) use self::array_mutators::{emit_array_push, emit_assoc_array_pop_shift_discard};
use self::array_mutators::{
    emit_array_key_sort_call, emit_array_pop_call, emit_array_push_call, emit_array_shift_call,
    emit_array_shuffle_call, emit_array_sort_call, emit_array_unshift_call,
    emit_assoc_array_value_sort_call, is_assoc_array_single_arg, is_value_array_single_arg,
    emit_output_assoc_array_pop_shift, emit_output_value_array_pop_shift,
};
use self::array_sort::{emit_uasort_call, emit_uksort_call, emit_usort_call};
use self::array_string_split::{emit_runtime_explode_assign, emit_runtime_str_split_assign};
use self::array_transform_static::{
    array_reverse_preserve_keys_arg, assoc_key_constants_for_source,
    array_unique_sort_mode, emit_static_array_items_assign,
    transformed_static_array_items, validate_array_reverse_preserve_keys_arg,
    validate_array_unique_sort_flag, ArrayUniqueSortMode,
};
pub(super) use self::array_value_cells::{
    array_literal_needs_value_cells, emit_copy_assoc_access_to_value_cell_by_mixed_key,
    emit_release_current_value_array, emit_store_value_cell, emit_value_array_items_assign,
    emit_value_array_store_expr, expr_needs_value_cell,
};
use self::array_value_cells::is_resource_returning_builtin;
use self::array_value_cells::emit_ensure_unique_array_payload;
use self::array_walk::emit_array_walk_call;
use self::chr_runtime::{emit_runtime_chr_output, emit_runtime_chr_value_to_stack};
use self::existence::{
    emit_existence_call, emit_is_callable_call, emit_isset_call, emit_member_exists_call,
    wasm_known_builtin_exists,
};
use self::implode::{
    emit_implode_string_builtin_value_to_stack, emit_output_implode_assigned_string_array,
    emit_output_value_cell_stack,
};
use self::float_math::{
    emit_float_math_call, emit_literal_float_math_call, is_literal_float_math_builtin,
    literal_numeric_arg, wasm_float_literal,
};
use self::string_builtins::emit_output_string_builtin;
use self::string_byte_classes::{
    emit_ascii_alpha_condition, emit_ascii_digit_condition, emit_byte_class_condition, ByteClass,
};
use self::string_bytes::*;
use self::string_int_predicates::emit_literal_string_int_call;
use self::string_memory::{
    emit_copy_pad_to_memory, emit_copy_spaces_to_memory, emit_copy_static_range_to_memory,
    emit_copy_string_range_to_memory, emit_copy_string_to_memory, emit_copy_zeros_to_memory,
    WasmPadSource,
};
use self::string_predicates::{
    emit_literal_string_bool_call, emit_runtime_literal_match_at,
    emit_runtime_literal_match_at_case_insensitive, emit_runtime_var_match_at,
    emit_runtime_var_match_at_case_insensitive,
};
use self::string_search::{
    emit_output_optional_int_builtin, emit_string_search_index_from_args,
};
use self::string_strstr::{
    emit_runtime_strstr, emit_runtime_strstr_value_to_stack, emit_runtime_strstr_var,
    emit_runtime_strstr_var_value_to_stack,
};
use self::string_runtime::*;
use self::substring_runtime::{
    emit_runtime_substr, emit_runtime_substr_dynamic, emit_runtime_substr_replace,
    emit_runtime_substr_replace_dynamic, emit_runtime_substr_replace_value_to_stack,
    emit_runtime_substr_replace_var_replacement, emit_runtime_substring_bounds,
    emit_runtime_substring_dynamic_bounds,
};
use self::trim_runtime::{
    emit_runtime_trim, emit_runtime_trim_value_to_stack, emit_runtime_trim_var_charlist,
};
use self::string_values::{
    emit_base64_decode_string_builtin_value_to_stack,
    emit_base64_encode_string_builtin_value_to_stack,
    emit_bin2hex_string_builtin_value_to_stack,
    emit_chr_string_builtin_value_to_stack,
    emit_escape_string_builtin_value_to_stack,
    emit_hex2bin_string_builtin_value_to_stack,
    emit_html_escape_string_builtin_value_to_stack,
    emit_same_len_string_builtin_value_to_stack,
    emit_str_repeat_string_builtin_value_to_stack,
    emit_str_replace_string_builtin_value_to_stack, emit_strstr_string_builtin_value_to_stack,
    emit_substr_replace_string_builtin_value_to_stack, emit_substr_string_builtin_value_to_stack,
    emit_trim_string_builtin_value_to_stack, emit_urldecode_string_builtin_value_to_stack,
    emit_urlencode_string_builtin_value_to_stack,
};
use self::string_values_sprintf::{
    emit_sprintf_segment_len, emit_sprintf_string_builtin_value_to_stack,
    emit_sprintf_width_pad_len,
};
use self::static_args::{
    const_int_value, const_or_literal_int_arg, literal_bool_arg, literal_format_int_arg,
    literal_format_string_arg, literal_int_arg, literal_php_array_int_key, literal_string_arg,
    literal_unsigned_int_arg, normalize_base64_input, normalize_static_string_args,
    static_ascii_string_arg, static_bool_value_cell_needle,
    static_format_string_arg, static_int_value, static_or_const_int_value,
    static_or_const_or_f64_local_value, static_or_const_or_i32_bool_value,
    static_or_const_or_i64_local_value, static_or_module_const_int_value,
    static_or_tracked_ascii_string_arg, SpaceEncoding,
};
pub(super) use self::truthiness::emit_condition;
use self::truthiness::{
    emit_mixed_local_truthiness, emit_string_local_truthiness, string_is_truthy,
};
pub(super) use self::value_flow::*;
use self::sprintf::{
    emit_printf_call, emit_runtime_sprintf, sprintf_string_value_segments, SprintfStringSegment,
};
use self::sprintf_literal::{eval_literal_sprintf, format_php_general};
use self::wordwrap::{
    emit_runtime_wordwrap, emit_runtime_wordwrap_dynamic,
    emit_wordwrap_string_builtin_value_to_stack, WasmBreakText, WasmWordwrapCut,
};
use self::path_builtins::{
    emit_runtime_basename, emit_runtime_basename_var_suffix, emit_runtime_dirname,
    emit_runtime_dirname_dynamic, emit_runtime_pathinfo_array_assign,
    emit_runtime_pathinfo_dynamic_flag, emit_runtime_pathinfo_static_flag,
};
use self::path_builtins_literal::{
    eval_literal_basename, eval_literal_dirname, eval_literal_pathinfo,
    static_pathinfo_assoc_items, static_pathinfo_path_value,
};
use self::string_repeat::{
    emit_non_negative_i32_count, emit_runtime_str_repeat, emit_runtime_str_repeat_dynamic,
    emit_runtime_str_repeat_value_to_stack,
};
use self::string_replace::{
    emit_runtime_str_replace, emit_runtime_str_replace_array_value_to_stack,
    emit_runtime_str_replace_dynamic_array_value_to_stack, emit_runtime_str_replace_value_to_stack,
    emit_runtime_str_replace_var_replacement, emit_runtime_str_replace_var_search,
    emit_set_i64_local_const, materialize_string_value_array_arg,
    runtime_string_value_array, str_replace_count_local, value_string_array_len, WasmReplacement,
    WasmSearch,
};
use self::str_pad::{
    emit_runtime_str_pad, emit_runtime_str_pad_dynamic, emit_runtime_str_pad_dynamic_var_pad,
    emit_runtime_str_pad_var_pad, emit_str_pad_string_builtin_value_to_stack,
};
use self::json_encode::{
    emit_json_encode_array_value_to_stack, emit_output_json_encode_array_local,
};
use self::json_literal::{eval_literal_json_encode, json_quote};
use self::json_runtime_string::emit_runtime_json_encode_string;
use self::json_state::{
    emit_json_last_error_call, emit_json_last_error_msg_value_to_stack, emit_json_last_error_none,
    emit_json_validate_call,
};
use self::mixed_assign::{
    emit_mixed_array_search_assign, emit_mixed_value_array_pop_shift_assign,
    emit_unknown_mixed_array_pop_assign, emit_unknown_mixed_array_shift_assign,
};
pub(in crate::codegen::wasm) use self::mixed_assign::emit_mixed_assign;
pub(super) use self::mixed_values::{
    emit_alloc_mixed_cell, emit_copy_value_cell_from_addr_to_addr,
    emit_null_mixed_value_to_stack, emit_release_value_cell, emit_store_null_value_cell,
};
use self::number_format::{
    emit_number_format_string_builtin_value_to_stack, emit_runtime_number_format_int,
    eval_literal_number_format, runtime_number_format_decimals,
    runtime_number_format_int_arg_local, runtime_number_format_separator,
};
use self::numeric_predicates::{
    emit_float_predicate_call, emit_is_numeric_call, emit_runtime_is_numeric,
};
pub(super) use self::output::emit_output_expr;
use self::output_helpers::{
    emit_output_class_name, emit_output_constant, emit_output_constant_value, emit_output_match,
    emit_output_scoped_constant, emit_output_short_ternary, emit_output_ternary,
};
pub(super) use self::objects::{
    emit_dynamic_object_property_assign, emit_dynamic_object_property_assignment_expr,
    emit_dynamic_object_property_empty_expr, emit_dynamic_object_property_isset_expr,
    emit_get_class_value_to_stack, emit_get_parent_class_value_to_stack,
    emit_nullsafe_mixed_object_dynamic_property_access_expr,
    emit_nullsafe_mixed_object_dynamic_property_isset_expr, emit_object_property_assign,
    emit_object_property_assignment_expr, emit_object_property_isset_expr,
    dynamic_static_method_call_array_return_metadata,
    method_call_array_return_metadata, object_class_name_for_expr,
    static_method_call_array_return_metadata,
    emit_static_property_access_expr, emit_static_property_assign, emit_static_property_isset_expr,
};
use self::objects::*;
use self::runtime_args::{
    runtime_bool_arg, runtime_bool_variable_arg, runtime_float_variable_arg,
    runtime_int_variable_arg, runtime_str_pad_type, runtime_string_variable_arg, RuntimeBoolArg,
    RuntimePadType,
};
use self::scalar_builtins::*;
pub(super) use self::scalar_mixed_numeric::{
    dynamic_numeric_operand_may_materialize, emit_known_mixed_numeric_operand,
};
use self::scalar_mixed_numeric::*;
use self::scalar_ops::*;
use self::scalar_string_coercion::*;
use self::scalar_constants::{emit_constant_expr, emit_constant_value, emit_scoped_constant_expr};
use self::type_predicates::emit_type_predicate_call;
use self::scalar_casts::{
    emit_boolval_call, emit_cast_expr, emit_empty_call, emit_floatval_call, emit_intval_call,
};
use self::hash_builtins::{
    emit_hash_string_builtin_value_to_stack, eval_literal_hash, eval_literal_md5, eval_literal_sha1,
};
use self::html_entities::{
    emit_html_entity_decode_string_builtin_value_to_stack,
    html_decodes_apos, html_encoding_is_ascii_compatible, html_entity_decode, html_escape,
    html_single_quote_entity, html_utf8_entity_decodes, html_utf8_entity_encodes,
    validate_html_encoding,
};
use self::literal_encoding::{base64_decode, base64_encode, url_decode, url_encode};
use self::literal_strings::{
    addslashes, ascii_lcfirst, ascii_lower, ascii_ucfirst, ascii_ucwords, ascii_upper,
    bin2hex, bytes_to_ascii_string, digit_allowed_for_radix, eval_literal_implode,
    eval_literal_str_pad, eval_literal_str_replace, eval_literal_str_replace_with_count,
    eval_literal_strstr, eval_literal_substr,
    eval_literal_substr_replace, eval_literal_trim, eval_literal_wordwrap,
    eval_static_explode, eval_static_str_replace_args, eval_static_str_split, hex2bin, hex_nibble,
    nl2br, stripslashes,
    TrimSide,
};
pub(in crate::codegen::wasm) use self::functions::callable_expr_return_kind;
use self::functions::{
    call_user_func_array_return_kind, call_user_func_return_kind, call_user_func_target,
    callable_return_kind, callable_return_expr_targets, callable_variable_return_kind,
    callable_variable_target, emit_call_user_func_array_call, emit_call_user_func_call,
    emit_callable_assign, emit_callable_expr_call,
    emit_callable_variable_call, emit_user_function_args, object_expr_is_invokable,
};

use super::module::{
    value_kind_for_local, wasm_function_name, wasm_value_type, ArrayLayout, AssocKeyKind, AssocKeyValue,
    ConstantArrayValue, ConstantValue, LocalKind, NestedArrayMetadata, ValueCellKind, ValueKind,
    WasmModule,
};

const WASM_VALUE_TAG_NULL: i32 = 0;
const WASM_VALUE_TAG_INT: i32 = 1;
const WASM_VALUE_TAG_BOOL: i32 = 2;
const WASM_VALUE_TAG_STRING: i32 = 3;
const WASM_VALUE_TAG_ARRAY: i32 = 4;
const WASM_VALUE_TAG_FLOAT: i32 = 5;
const WASM_VALUE_TAG_OBJECT: i32 = 6;
const WASM_VALUE_CELL_SIZE: usize = 16;
const WASM_HEAP_KIND_INDEXED_ARRAY: i32 = 2;
const WASM_HEAP_KIND_ASSOC_ARRAY: i32 = 3;
pub(super) const WASM_ASSOC_KEY_INT: i32 = 0;
pub(super) const WASM_ASSOC_KEY_STRING: i32 = 1;
const WASM_ASSOC_ENTRY_SIZE: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArrayMapCallbackShape {
    IntToInt,
    IntToBool,
    IntIntToInt,
    IntIntIntToInt,
    IntIntIntIntToInt,
    IntIntIntIntIntToInt,
    IntIntIntIntIntIntToInt,
    StrToInt,
    StrToBool,
    StrToStr,
    BoolToBool,
    FloatToFloat,
    FloatToBool,
    NullToBool,
    NumericToBool,
    ArrayToBool,
    ObjectToBool,
    ObjectToStr,
    ObjectToParentStr,
    ObjectToTypeStr,
    StrStrToStr,
    StrStrStrToStr,
    StrStrStrStrToStr,
    StrStrStrStrStrToStr,
    StrStrStrStrStrStrToStr,
    MixedMixedToInt,
    MixedMixedToStr,
    MixedMixedToBool,
    MixedMixedToFloat,
    MixedMixedMixedToInt,
    MixedMixedMixedToStr,
    MixedMixedMixedToBool,
    MixedMixedMixedToFloat,
    MixedMixedMixedMixedToInt,
    MixedMixedMixedMixedToStr,
    MixedMixedMixedMixedToBool,
    MixedMixedMixedMixedToFloat,
    MixedMixedMixedMixedMixedToInt,
    MixedMixedMixedMixedMixedToStr,
    MixedMixedMixedMixedMixedToBool,
    MixedMixedMixedMixedMixedToFloat,
    MixedMixedMixedMixedMixedMixedToInt,
    MixedMixedMixedMixedMixedMixedToStr,
    MixedMixedMixedMixedMixedMixedToBool,
    MixedMixedMixedMixedMixedMixedToFloat,
    MixedMixedMixedMixedMixedMixedMixedToInt,
    MixedMixedMixedMixedMixedMixedMixedToStr,
    MixedMixedMixedMixedMixedMixedMixedToBool,
    MixedMixedMixedMixedMixedMixedMixedToFloat,
    MixedMixedMixedMixedMixedMixedMixedMixedToInt,
    MixedMixedMixedMixedMixedMixedMixedMixedToStr,
    MixedMixedMixedMixedMixedMixedMixedMixedToBool,
    MixedMixedMixedMixedMixedMixedMixedMixedToFloat,
    MixedMixedMixedMixedMixedMixedMixedMixedMixedToInt,
    MixedMixedMixedMixedMixedMixedMixedMixedMixedToStr,
    MixedMixedMixedMixedMixedMixedMixedMixedMixedToBool,
    MixedMixedMixedMixedMixedMixedMixedMixedMixedToFloat,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArrayFilterCallbackShape {
    Int,
    Str,
    Bool,
    Float,
    Null,
    Numeric,
    Array,
    Object,
    Mixed,
}

#[derive(Clone, Copy)]
struct ArrayFilterUseBothCallback {
    value_kind: Option<ValueCellKind>,
    key_shape: ArrayFilterCallbackShape,
}
