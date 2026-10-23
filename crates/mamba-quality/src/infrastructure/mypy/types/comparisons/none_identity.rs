//! Direct identity checks with None have the builtin bool result.
use super::{CmpOp, Comparison, Type};

pub(in crate::infrastructure::mypy) fn comparison(
    left: Type,
    right: Type,
    operator: CmpOp,
) -> Option<Comparison> {
    let symbol = match operator {
        CmpOp::Is => "is",
        CmpOp::IsNot => "is not",
        _ => return None,
    };
    if left != Type::None && right != Type::None {
        return None;
    }
    Some(Comparison {
        returns: Type::Bool,
        failure: None,
        symbol,
        left,
        right,
    })
}
