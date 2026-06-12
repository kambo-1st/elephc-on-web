//! Purpose:
//! Lowers wasm32-web array transform and set-operation helpers.
//! Covers array_unique/diff/intersect/flip support plus metadata helpers for transformed arrays.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` array assignment lowering.
//!
//! Key details:
//! - Helpers preserve PHP key/value normalization, value-cell string comparisons, and exact metadata.

use super::*;

mod dispatch;
mod assoc_entry;
mod assoc_flip;
mod assoc;
mod static_items;
mod value_string;
mod value_string_cells;
mod value_cell_access;
mod value_cell_loose;
mod value_set_runtime_match;
mod value_compare_sources;
mod scalar_string_repr;
mod scalar_string_repr_predicates;
mod value_compare;
mod exact_metadata;
mod indexed_int;
mod indexed_int_static;
mod materialize_return;

pub(super) use assoc::*;
pub(super) use assoc_entry::*;
pub(super) use assoc_flip::*;
pub(super) use value_cell_access::*;
pub(super) use value_cell_loose::*;
pub(super) use value_set_runtime_match::*;
pub(super) use value_compare_sources::*;
pub(super) use scalar_string_repr::*;
pub(super) use scalar_string_repr_predicates::*;
pub(super) use value_compare::*;
pub(super) use value_string::*;
pub(super) use value_string_cells::*;
pub(super) use dispatch::*;
use exact_metadata::*;
use indexed_int::*;
use indexed_int_static::*;
pub(super) use materialize_return::*;
use static_items::*;
