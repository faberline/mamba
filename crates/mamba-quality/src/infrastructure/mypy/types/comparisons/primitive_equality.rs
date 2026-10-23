//! Direct builtin primitives have locked (object) -> bool equality methods.
use super::{CmpOp, Comparison, Type};

pub(in crate::infrastructure::mypy) fn comparison(
    left: Type,
    right: Type,
    operator: CmpOp,
) -> Option<Comparison> {
    let symbol = match operator {
        CmpOp::Eq => "==",
        CmpOp::NotEq => "!=",
        _ => return None,
    };
    if !direct_primitive(left) || !direct_primitive(right) {
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

fn direct_primitive(ty: Type) -> bool {
    matches!(
        ty,
        Type::Bool
            | Type::Int
            | Type::Float
            | Type::Complex
            | Type::Str
            | Type::Bytes
            | Type::None
            | Type::Object
    )
}
