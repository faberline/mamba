//! Branch paths retain Astroid's mutual-exclusion decisions.
use mamba_language::application::quality_syntax::python_ast::{CmpOp, Expr, UnaryOp};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Branch {
    If(usize),
    TryBody,
    TryHandler(usize),
    TryElse,
    TryFinally,
}
pub(super) type Path = Vec<(u32, Branch)>;

pub(super) fn exclusive(first: &Path, second: &Path) -> bool {
    for ((left_id, left), (right_id, right)) in first.iter().zip(second) {
        if left_id != right_id {
            return false;
        }
        match (left, right) {
            (Branch::If(left), Branch::If(right)) if left != right => return true,
            (Branch::TryHandler(left), Branch::TryHandler(right)) if left != right => return true,
            (Branch::TryBody | Branch::TryElse, Branch::TryHandler(_))
            | (Branch::TryHandler(_), Branch::TryBody | Branch::TryElse) => return true,
            _ => {}
        }
    }
    false
}

pub(super) fn permits_redefinition(value: &Expr, name: &str) -> bool {
    match value {
        Expr::UnaryOp(value) if value.op == UnaryOp::Not => {
            matches!(value.operand.as_ref(), Expr::Name(value) if value.id.as_str() == name)
        }
        Expr::Compare(value) if value.ops.as_ref() == [CmpOp::Is] => {
            matches!(value.operands.as_ref(), [Expr::Name(value), Expr::NoneLiteral(_)] if value.id.as_str() == name)
        }
        _ => false,
    }
}
