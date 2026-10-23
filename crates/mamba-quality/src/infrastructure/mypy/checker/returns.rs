//! Missing-return checks use the final reachable path and declared function type.
use super::*;
use quality_syntax::python_ast::StmtFunctionDef;

impl Checker<'_> {
    pub(super) fn check_return_paths(
        &mut self,
        function: &StmtFunctionDef,
        returns: Option<types::Type>,
    ) {
        let Some(expected) = returns else { return };
        if !self.flow.reachable
            || self.source.is_stub
            || matches!(
                expected,
                types::Type::Any | types::Type::AnySpecial | types::Type::None
            )
        {
            return;
        }
        let trivial = function.body.iter().all(|statement| match statement {
            Stmt::Pass(_) => true,
            Stmt::Expr(expression) => matches!(
                expression.value.as_ref(),
                Expr::EllipsisLiteral(_) | Expr::StringLiteral(_)
            ),
            _ => false,
        });
        if trivial
            && (self.context.accepts(expected, types::Type::None)
                || self
                    .context
                    .abstract_method(function, &self.scopes.variables()))
        {
            return;
        }
        let (rule, message) = if expected == types::Type::Never {
            (
                if trivial { "empty-body" } else { "misc" },
                "Implicit return in function which does not return",
            )
        } else {
            (
                if trivial { "empty-body" } else { "return" },
                "Missing return statement",
            )
        };
        let hint = (trivial && self.class_types.last().copied().flatten().is_some())
            .then(|| "If the method is meant to be abstract, use @abc.abstractmethod".into());
        self.emit_with_hint(rule, message.into(), function, hint);
    }
}
