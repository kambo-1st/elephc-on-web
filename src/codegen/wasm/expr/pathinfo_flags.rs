//! Purpose:
//! Decodes PHP PATHINFO_* bit flags for wasm32-web pathinfo() lowering.
//! Keeps scalar pathinfo flag semantics shared by runtime and literal helpers.
//!
//! Called from:
//! - `crate::codegen::wasm::expr::path_builtins`.
//! - `crate::codegen::wasm::expr::path_builtins_literal`.
//!
//! Key details:
//! - PATHINFO_ALL remains array-shaped; scalar helpers accept only one concrete component.

#[derive(Clone, Copy)]
pub(super) enum PathinfoScalarComponent {
    Empty,
    Dirname,
    Basename,
    Extension,
    Filename,
}

pub(super) fn pathinfo_scalar_component(flag: i64) -> Option<PathinfoScalarComponent> {
    if flag == 15 {
        return None;
    }
    if flag & 1 != 0 {
        Some(PathinfoScalarComponent::Dirname)
    } else if flag & 2 != 0 {
        Some(PathinfoScalarComponent::Basename)
    } else if flag & 4 != 0 {
        Some(PathinfoScalarComponent::Extension)
    } else if flag & 8 != 0 {
        Some(PathinfoScalarComponent::Filename)
    } else if flag == 0 {
        Some(PathinfoScalarComponent::Empty)
    } else {
        None
    }
}
