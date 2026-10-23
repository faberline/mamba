use super::*;

/// Build `binding_name = __mb_pep695_typevar__("name", kind, bound, constraints, default)`.
pub(super) fn typevar_assign(param: &TypeParam, binding_name: &str, span: Span) -> Spanned<Stmt> {
    let kind = match param.kind {
        TypeParamKind::TypeVar => 0,
        TypeParamKind::TypeVarTuple => 1,
        TypeParamKind::ParamSpec => 2,
    };
    let bound_arg = match &param.bound {
        Some(b) => sp(
            Expr::Lambda {
                params: vec![],
                body: Box::new(b.clone()),
            },
            b.span,
        ),
        None => sp(Expr::NoneLit, span),
    };
    let constraints_arg = match &param.constraints {
        Some(items) => sp(
            Expr::Lambda {
                params: vec![],
                body: Box::new(sp(Expr::TupleLit(items.clone()), span)),
            },
            span,
        ),
        None => sp(Expr::NoneLit, span),
    };
    let default_arg = match &param.default {
        Some(default) => sp(
            Expr::Lambda {
                params: vec![],
                body: Box::new(default.clone()),
            },
            default.span,
        ),
        None => sp(Expr::NoneLit, span),
    };
    let call = Expr::Call {
        func: Box::new(sp(Expr::Ident(TYPEVAR_INTRINSIC.to_string()), span)),
        args: vec![
            CallArg::Positional(sp(Expr::StrLit(param.name.clone()), span)),
            CallArg::Positional(sp(Expr::IntLit(kind), span)),
            CallArg::Positional(bound_arg),
            CallArg::Positional(constraints_arg),
            CallArg::Positional(default_arg),
        ],
    };
    Spanned::new(
        Stmt::Assign {
            target: sp(Expr::Ident(binding_name.to_string()), span),
            value: sp(call, span),
        },
        span,
    )
}
