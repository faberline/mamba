//! Return refactoring retains the original body traversal and branch decisions.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_stmt, Visitor},
    ExceptHandler, Expr, Stmt, StmtFunctionDef, StmtIf, StmtTry,
};
use ruff_text_size::Ranged;

#[derive(Default)]
struct Returns {
    values: usize,
    empty: usize,
}
impl<'a> Visitor<'a> for Returns {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Return(returned) => {
                if returned.value.is_some() {
                    self.values += 1;
                } else {
                    self.empty += 1;
                }
            }
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            _ => walk_stmt(self, statement),
        }
    }
}

impl Checker<'_> {
    pub(super) fn check_useless_return(&mut self, function: &StmtFunctionDef) {
        let body = &function.body[usize::from(super::docstring(&function.body))..];
        let mut returns = Returns::default();
        returns.visit_body(body);
        if returns.empty + returns.values != 1 {
            return;
        }
        let Some(mut last) = body.last() else {
            return;
        };
        if body.len() == 1 && matches!(last, Stmt::Return(_)) {
            return;
        }
        loop {
            let next = match last {
                Stmt::If(value) => value
                    .elif_else_clauses
                    .last()
                    .map_or(value.body.as_slice(), |clause| clause.body.as_slice())
                    .last(),
                Stmt::Try(value) => value
                    .finalbody
                    .last()
                    .or_else(|| value.orelse.last())
                    .or_else(|| {
                        value.handlers.last().and_then(|handler| {
                            let ExceptHandler::ExceptHandler(handler) = handler;
                            handler.body.last()
                        })
                    })
                    .or_else(|| value.body.last()),
                _ => None,
            };
            let Some(next) = next else {
                break;
            };
            last = next;
        }
        if matches!(last, Stmt::Return(value) if value.value.as_deref().is_none_or(|value| matches!(value, Expr::NoneLiteral(_))))
        {
            self.emit(
                "R1711",
                "useless-return",
                "Useless return at end of function or method",
                self.declaration_range(function.name.range()),
            );
        }
    }
    pub(super) fn check_consistent_returns(&mut self, function: &StmtFunctionDef) {
        if function.is_async {
            return;
        }
        let mut returns = Returns::default();
        returns.visit_body(&function.body);
        if returns.values > 0 && (returns.empty > 0 || !self.body_return_ended(&function.body)) {
            self.emit("R1710", "inconsistent-return-statements", "Either all return statements in a function should return an expression, or none of them should.", self.declaration_range(function.name.range()));
        }
    }

    fn emit_else(&mut self, body: &[Stmt], keyword: &str, range: ruff_text_size::TextRange) {
        let suggestion = if keyword == "elif" {
            "replace only that \"elif\" with \"if\""
        } else {
            "remove the \"else\" and de-indent the code inside it"
        };
        for (rule, name, terminator) in [
            ("R1705", "no-else-return", "return"),
            ("R1720", "no-else-raise", "raise"),
            ("R1723", "no-else-break", "break"),
            ("R1724", "no-else-continue", "continue"),
        ] {
            if body.iter().any(|statement| {
                matches!(
                    (statement, terminator),
                    (Stmt::Return(_), "return")
                        | (Stmt::Raise(_), "raise")
                        | (Stmt::Break(_), "break")
                        | (Stmt::Continue(_), "continue")
                )
            }) {
                self.emit(
                    rule,
                    name,
                    &format!("Unnecessary \"{keyword}\" after \"{terminator}\", {suggestion}"),
                    range,
                );
            }
        }
    }

    pub(super) fn check_else_return(&mut self, conditional: &StmtIf) {
        let Some(first) = conditional.elif_else_clauses.first() else {
            return;
        };
        self.emit_else(
            &conditional.body,
            if first.test.is_some() { "elif" } else { "else" },
            conditional.range(),
        );
    }

    pub(super) fn check_try_else(&mut self, attempt: &StmtTry) {
        if attempt.orelse.is_empty() || !attempt.finalbody.is_empty() {
            return;
        }
        for (rule, name, terminator) in [
            ("R1705", "no-else-return", "return"),
            ("R1720", "no-else-raise", "raise"),
        ] {
            if attempt.handlers.iter().all(|handler| {
                let ExceptHandler::ExceptHandler(handler) = handler;
                handler.body.iter().any(|statement| {
                    matches!(
                        (statement, terminator),
                        (Stmt::Return(_), "return") | (Stmt::Raise(_), "raise")
                    )
                })
            }) {
                self.emit(rule, name, &format!("Unnecessary \"else\" after \"{terminator}\", remove the \"else\" and de-indent the code inside it"), attempt.range());
            }
        }
    }

    pub(super) fn check_loop_else(&mut self, body: &[Stmt], orelse: &[Stmt]) {
        if orelse.is_empty() || super::return_ended::has_break(body) {
            return;
        }
        let first = orelse[0].start();
        let before = body
            .last()
            .map_or(0, |statement| statement.end().to_usize());
        let prefix = &self.source.text[before..first.to_usize()];
        let Some(keyword) = prefix.rfind("else") else {
            return;
        };
        let range = ruff_text_size::TextRange::new(
            ruff_text_size::TextSize::try_from(before + keyword).expect("source range"),
            orelse.last().unwrap().end(),
        );
        self.emit("W0120", "useless-else-on-loop", "Else clause on loop without a break statement, remove the else and de-indent all the code inside it", range);
    }
}
