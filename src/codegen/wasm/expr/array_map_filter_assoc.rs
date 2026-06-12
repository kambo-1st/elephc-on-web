//! Purpose:
//! Emits wasm32-web array_filter lowering for associative arrays.
//! Keeps key/value/both callback modes separate from value-array filter emitters.
//!
//! Called from:
//! - `crate::codegen::wasm::expr` array_filter dispatch and assignment lowering.
//!
//! Key details:
//! - Preserves associative keys, runtime key metadata, callback argument modes, and PHP truthiness behavior.

mod both;
mod key;
mod strlen;
mod value;

use super::*;
use super::array_filter_assoc_args::*;
use super::array_filter_truthiness::emit_value_cell_pointer_strlen_truthiness;

pub(super) use both::emit_array_filter_assoc_both_local_assign;
pub(super) use key::emit_array_filter_assoc_key_local_assign;
pub(super) use strlen::{
    emit_array_filter_assoc_strlen_local_assign, emit_array_filter_runtime_assoc_strlen_local_assign,
};
pub(super) use value::{
    emit_array_filter_assoc_local_assign, emit_array_filter_assoc_local_instance_assign,
    emit_array_filter_runtime_assoc_local_assign, emit_array_filter_runtime_assoc_local_instance_assign,
};
