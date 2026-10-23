use super::*;

pub(super) fn mangle_expr(expr: &mut Spanned<Expr>, class: Option<&str>) {
    match &mut expr.node {
        Expr::Ident(name) => mangle_in_place(name, class),
        Expr::Attr { object, attr } => {
            mangle_expr(object, class);
            mangle_in_place(attr, class);
        }
        Expr::BinOp { lhs, rhs, .. } => {
            mangle_expr(lhs, class);
            mangle_expr(rhs, class);
        }
        Expr::UnaryOp { operand, .. } => mangle_expr(operand, class),
        Expr::Call { func, args } => {
            mangle_expr(func, class);
            for arg in args.iter_mut() {
                match arg {
                    // Keyword argument *names* are not mangled (CPython binds
                    // them by literal name); only the value expression is.
                    CallArg::Positional(e)
                    | CallArg::StarArg(e)
                    | CallArg::DoubleStarArg(e)
                    | CallArg::Keyword { value: e, .. } => mangle_expr(e, class),
                }
            }
        }
        Expr::Index { object, index } => {
            mangle_expr(object, class);
            mangle_expr(index, class);
        }
        Expr::Slice { start, stop, step } => {
            if let Some(s) = start {
                mangle_expr(s, class);
            }
            if let Some(s) = stop {
                mangle_expr(s, class);
            }
            if let Some(s) = step {
                mangle_expr(s, class);
            }
        }
        Expr::ListLit(elems) | Expr::SetLit(elems) | Expr::TupleLit(elems) => {
            for e in elems.iter_mut() {
                mangle_expr(e, class);
            }
        }
        Expr::DictLit(entries) => {
            for (k, v) in entries.iter_mut() {
                if let Some(k) = k {
                    mangle_expr(k, class);
                }
                mangle_expr(v, class);
            }
        }
        Expr::IfExpr {
            body,
            condition,
            else_body,
        } => {
            mangle_expr(body, class);
            mangle_expr(condition, class);
            mangle_expr(else_body, class);
        }
        Expr::Lambda { params, body } => {
            for p in params.iter_mut() {
                mangle_param(p, class);
            }
            mangle_expr(body, class);
        }
        Expr::ListComp {
            element,
            generators,
        }
        | Expr::SetComp {
            element,
            generators,
        }
        | Expr::GeneratorExpr {
            element,
            generators,
        } => {
            mangle_comprehensions(generators, class);
            mangle_expr(element, class);
        }
        Expr::DictComp {
            key,
            value,
            generators,
        } => {
            mangle_comprehensions(generators, class);
            mangle_expr(key, class);
            mangle_expr(value, class);
        }
        Expr::Walrus { target, value } => {
            mangle_in_place(target, class);
            mangle_expr(value, class);
        }
        Expr::FString(parts) => mangle_fstring(parts, class),
        Expr::Yield(Some(e)) | Expr::YieldFrom(e) | Expr::Await(e) | Expr::Starred(e) => {
            mangle_expr(e, class)
        }
        Expr::UnpackTarget(elems) => {
            for e in elems.iter_mut() {
                mangle_expr(e, class);
            }
        }
        Expr::ChainedCompare { operands, .. } => {
            for e in operands.iter_mut() {
                mangle_expr(e, class);
            }
        }
        Expr::Yield(None)
        | Expr::IntLit(_)
        | Expr::BigIntLit(_)
        | Expr::FloatLit(_)
        | Expr::ComplexLit(_)
        | Expr::StrLit(_)
        | Expr::BytesLit(_)
        | Expr::BoolLit(_)
        | Expr::NoneLit
        | Expr::Ellipsis => {}
    }
}
