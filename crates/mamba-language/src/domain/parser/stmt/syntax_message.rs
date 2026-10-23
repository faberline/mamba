use super::*;

impl Pep695ExprContext {
    pub(super) fn syntax_message(self, expr: &Expr) -> &'static str {
        match expr {
            Expr::Yield(_) | Expr::YieldFrom(_) => match self {
                Self::TypeAlias => "yield expression cannot be used within a type alias",
                Self::TypeParam => "yield expression cannot be used within a TypeVar bound",
                Self::GenericDefinition => {
                    "yield expression cannot be used within the definition of a generic"
                }
            },
            Expr::Await(_) => match self {
                Self::TypeAlias => "await expression cannot be used within a type alias",
                Self::TypeParam => "await expression cannot be used within a TypeVar bound",
                Self::GenericDefinition => {
                    "await expression cannot be used within the definition of a generic"
                }
            },
            Expr::Walrus { .. } => match self {
                Self::TypeAlias => "named expression cannot be used within a type alias",
                Self::TypeParam => "named expression cannot be used within a TypeVar bound",
                Self::GenericDefinition => {
                    "named expression cannot be used within the definition of a generic"
                }
            },
            _ => unreachable!("unexpected PEP 695 validation error source"),
        }
    }
}
