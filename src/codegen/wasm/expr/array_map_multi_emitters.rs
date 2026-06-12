//! Purpose:
//! Wires wasm32-web array_map lowering for multi-source array assignments.
//! Keeps arity-specific callback loops in focused child modules.
//!
//! Called from:
//! - `super::array_map_assign` while lowering array_map assignments with multiple sources.
//!
//! Key details:
//! - Child modules preserve compact-int/value-string layout metadata and runtime-length loop behavior.

mod eight;
mod four;
mod five;
mod many;
mod nine;
mod six;
mod seven;
mod three;
mod two;

use super::*;

pub(super) use eight::emit_array_map_eight_mixed_locals_assign;
pub(super) use four::{
    emit_array_map_four_compact_int_locals_assign, emit_array_map_four_mixed_locals_assign,
    emit_array_map_four_value_string_locals_assign,
};
pub(super) use five::{
    emit_array_map_five_compact_int_locals_assign, emit_array_map_five_mixed_locals_assign,
    emit_array_map_five_value_string_locals_assign,
};
pub(in crate::codegen::wasm::expr) use many::emit_array_map_many_mixed_locals_assign;
pub(super) use nine::emit_array_map_nine_mixed_locals_assign;
pub(super) use six::{
    emit_array_map_six_compact_int_locals_assign, emit_array_map_six_mixed_locals_assign,
    emit_array_map_six_value_string_locals_assign,
};
pub(super) use seven::emit_array_map_seven_mixed_locals_assign;
pub(super) use three::{
    emit_array_map_three_compact_int_locals_assign, emit_array_map_three_mixed_locals_assign,
    emit_array_map_three_value_string_locals_assign,
};
pub(super) use two::{
    emit_array_map_two_compact_int_locals_assign, emit_array_map_two_mixed_arg_cell,
    emit_array_map_two_mixed_locals_assign, emit_array_map_two_value_string_locals_assign,
};
