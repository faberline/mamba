use super::*;

/// Type expression for annotations.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    /// Simple type name: `int`, `float`, `MyClass`
    Named(Name),
    /// Static type-pack expansion: `*Ts` or `Unpack[Ts]`.
    Unpack(Box<Spanned<TypeExpr>>),
    /// Generic type: `list[int]`, `dict[str, int]`
    Generic {
        name: Name,
        args: Vec<Spanned<TypeExpr>>,
    },
    /// Optional shorthand: `T?` => `T | None`
    Optional(Box<Spanned<TypeExpr>>),
    /// Union type: `int | str`
    Union(Vec<Spanned<TypeExpr>>),
    /// Function type: `(int, int) -> int`
    Fn {
        params: Vec<Spanned<TypeExpr>>,
        ret: Box<Spanned<TypeExpr>>,
    },
    /// Tuple type: `tuple[int, str]`
    Tuple(Vec<Spanned<TypeExpr>>),
}

pub const FORWARD_REF_PREFIX: &str = "__mamba_forward_ref__:";

pub fn forward_ref_name(name: &str) -> String {
    format!("{FORWARD_REF_PREFIX}{name}")
}

pub fn strip_forward_ref_name(name: &str) -> Option<&str> {
    name.strip_prefix(FORWARD_REF_PREFIX)
}
