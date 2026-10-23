//! Basic statement warnings use body membership rather than source spelling.
use super::{statements::Kind, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, Visitor},
    Expr, Stmt,
};
use ruff_text_size::Ranged;

#[derive(Default)]
struct Calls(bool);
impl<'a> Visitor<'a> for Calls {
    fn visit_expr(&mut self, expression: &'a Expr) {
        self.0 |= matches!(expression, Expr::Call(_));
        walk_expr(self, expression);
    }
}

impl Checker<'_> {
    pub(super) fn check_basic_statement(&mut self, statement: &Stmt) {
        let Some(context) = self
            .statements
            .contexts
            .get(&statement.start().to_u32())
            .copied()
        else {
            return;
        };
        let redundant = context.siblings > 1
            || (context.doc && matches!(context.parent, Kind::Function | Kind::Class));
        if let Stmt::Pass(pass) = statement {
            if redundant {
                self.emit(
                    "W0107",
                    "unnecessary-pass",
                    "Unnecessary pass statement",
                    pass.range(),
                );
            }
            return;
        }
        let Stmt::Expr(statement) = statement else {
            return;
        };
        let expression = statement.value.as_ref();
        match expression {
            Expr::EllipsisLiteral(_) => {
                if redundant {
                    self.emit(
                        "W2301",
                        "unnecessary-ellipsis",
                        "Unnecessary ellipsis constant",
                        expression.range(),
                    );
                }
                return;
            }
            Expr::StringLiteral(_) => {
                if !context.attribute_doc {
                    self.emit(
                        "W0105",
                        "pointless-string-statement",
                        "String statement has no effect",
                        statement.range(),
                    );
                }
                return;
            }
            Expr::Call(call) => {
                self.check_exception_statement(call, statement.range());
                return;
            }
            Expr::Yield(_) | Expr::YieldFrom(_) | Expr::Await(_) => return,
            _ if context.parent == Kind::Try && context.siblings == 1 => return,
            Expr::Named(_) => {
                self.emit(
                    "W0131",
                    "named-expr-without-context",
                    "Named expression used without context",
                    statement.range(),
                );
                return;
            }
            _ => {}
        }
        let mut calls = Calls::default();
        calls.visit_expr(expression);
        if calls.0 {
            let source = super::display::expression(expression, &self.source.text);
            self.emit(
                "W0106",
                "expression-not-assigned",
                &format!("Expression \"{source}\" is assigned to nothing"),
                statement.range(),
            );
        } else {
            self.emit(
                "W0104",
                "pointless-statement",
                "Statement seems to have no effect",
                statement.range(),
            );
        }
    }

    pub(super) fn check_unreachable(&mut self, statement: &Stmt) {
        if let Some(next) = self
            .statements
            .contexts
            .get(&statement.start().to_u32())
            .and_then(|context| context.next)
        {
            self.emit("W0101", "unreachable", "Unreachable code", next);
        }
    }
}
