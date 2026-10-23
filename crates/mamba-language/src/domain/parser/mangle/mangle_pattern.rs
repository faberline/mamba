use super::*;

pub(super) fn mangle_pattern(pat: &mut Spanned<Pattern>, class: Option<&str>) {
    match &mut pat.node {
        Pattern::Binding(name) => mangle_in_place(name, class),
        Pattern::Literal(e) => {
            let mut tmp = Spanned {
                node: std::mem::replace(e, Expr::NoneLit),
                span: pat.span,
            };
            mangle_expr(&mut tmp, class);
            *e = tmp.node;
        }
        Pattern::Or(pats) | Pattern::Sequence(pats) => {
            for p in pats.iter_mut() {
                mangle_pattern(p, class);
            }
        }
        Pattern::Mapping { pairs, rest } => {
            for (k, p) in pairs.iter_mut() {
                mangle_expr(k, class);
                mangle_pattern(p, class);
            }
            if let Some(r) = rest {
                mangle_in_place(r, class);
            }
        }
        Pattern::ClassPattern { patterns, .. } => {
            for (_, p) in patterns.iter_mut() {
                mangle_pattern(p, class);
            }
        }
        Pattern::Star(Some(name)) => mangle_in_place(name, class),
        Pattern::As { pattern, name } => {
            mangle_pattern(pattern, class);
            mangle_in_place(name, class);
        }
        Pattern::Constructor { fields, .. } => {
            for f in fields.iter_mut() {
                mangle_in_place(f, class);
            }
        }
        Pattern::Wildcard | Pattern::Star(None) => {}
    }
}
