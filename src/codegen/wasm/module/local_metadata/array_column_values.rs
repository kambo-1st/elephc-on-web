//! Purpose:
//! Infers static value-cell kinds for `array_column` results in wasm32-web local metadata.
//! Keeps column-specific row scanning separate from the main local metadata collector.
//!
//! Called from:
//! - `super::value_kinds_for_foreach_source()`.
//!
//! Key details:
//! - Only fully static rows with static associative keys are classified here.
//! - Unknown dynamic rows return `None` so existing conservative inference remains in charge.

use super::*;

pub(super) fn array_column_value_kinds(args: &[Expr]) -> Option<Vec<ValueCellKind>> {
    let ExprKind::ArrayLiteral(rows) = &args.first()?.kind else {
        return None;
    };
    let column = static_assoc_key_value_for_expr(args.get(1)?)?;
    let mut kinds = Vec::new();
    for row in rows {
        let ExprKind::ArrayLiteralAssoc(items) = &row.kind else {
            return None;
        };
        let mut found = None;
        for (key, value) in items {
            if static_assoc_key_value_for_expr(key)? == column {
                found = Some(static_value_cell_kind_for_expr(value)?);
            }
        }
        if let Some(kind) = found {
            kinds.push(kind);
        }
    }
    Some(kinds)
}
