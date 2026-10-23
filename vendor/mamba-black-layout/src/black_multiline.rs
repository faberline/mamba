//! Triple-quoted literals that affect Black's comma and conditional split rules.
use ruff_python_ast::{
    Expr, StringFlags, StringLike,
    visitor::{self, Visitor},
};
use ruff_text_size::Ranged;

pub(crate) fn contains(expression: &Expr, source: &str) -> bool {
    struct Find<'a> {
        source: &'a str,
        found: bool,
    }
    impl<'a> Visitor<'a> for Find<'_> {
        fn visit_expr(&mut self, expression: &'a Expr) {
            if StringLike::try_from(expression).is_ok_and(|string| {
                string.parts().any(|part| {
                    part.flags().is_triple_quoted() && self.source[part.range()].contains('\n')
                })
            }) {
                self.found = true;
            } else if !self.found {
                visitor::walk_expr(self, expression);
            }
        }
    }
    let mut find = Find {
        source,
        found: false,
    };
    find.visit_expr(expression);
    find.found
}
