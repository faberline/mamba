use super::*;

pub(super) fn match_key_of(expr: &Expr) -> Option<MatchKey> {
    match expr {
        Expr::IntLit(i) => Some(MatchKey::Num(*i as f64)),
        Expr::BigIntLit(_) => None,
        Expr::FloatLit(f) => Some(MatchKey::Num(*f)),
        Expr::BoolLit(b) => Some(MatchKey::Num(if *b { 1.0 } else { 0.0 })),
        Expr::StrLit(s) => Some(MatchKey::Str(s.clone())),
        Expr::NoneLit => Some(MatchKey::NoneKey),
        Expr::UnaryOp {
            op: mamba_shared_kernel::ast::UnaryOp::Neg,
            operand,
        } => match match_key_of(&operand.node) {
            Some(MatchKey::Num(n)) => Some(MatchKey::Num(-n)),
            _ => None,
        },
        _ => None,
    }
}
