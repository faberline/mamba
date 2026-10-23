//! Generator cleanup retains original yield and context-manager boundaries.
use super::{callables::Target, resolution::Context, scopes::Kind, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    ExceptHandler, Expr, Stmt, StmtFunctionDef, StmtTry, StmtWith,
};
use ruff_text_size::Ranged;

#[derive(Default)]
struct Nodes<'a> {
    contexts: Vec<&'a StmtWith>,
    yields: Vec<Option<&'a Expr>>,
    tries: Vec<&'a StmtTry>,
    following: bool,
    yield_following: Vec<bool>,
}
impl<'a> Visitor<'a> for Nodes<'a> {
    fn visit_body(&mut self, body: &'a [Stmt]) {
        let previous = self.following;
        for (index, statement) in body.iter().enumerate() {
            self.following = previous || index + 1 < body.len();
            self.visit_stmt(statement);
        }
        self.following = previous;
    }
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if let Stmt::With(value) = statement {
            self.contexts.push(value);
        }
        if let Stmt::Try(value) = statement {
            self.tries.push(value);
        }
        walk_stmt(self, statement);
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        match expression {
            Expr::Yield(value) => {
                self.yields.push(value.value.as_deref());
                self.yield_following.push(self.following);
            }
            Expr::YieldFrom(value) => {
                self.yields.push(Some(&value.value));
                self.yield_following.push(self.following);
            }
            _ => {}
        }
        walk_expr(self, expression);
    }
}
fn constant(expression: &Expr) -> bool {
    matches!(
        expression,
        Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
            | Expr::NumberLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::NoneLiteral(_)
            | Expr::EllipsisLiteral(_)
    )
}
struct Scan<'a, 'b, 's> {
    checker: &'b mut Checker<'s>,
    name: &'a str,
    warn: bool,
}
impl<'a> Visitor<'a> for Scan<'a, '_, '_> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::With(value) => self
                .checker
                .check_generator_context(value, self.name, self.warn),
            Stmt::FunctionDef(function) => {
                self.checker.scopes.enter(
                    Kind::Function,
                    &function.body,
                    Some(&function.parameters),
                );
                self.visit_body(&function.body);
                self.checker.scopes.leave();
                return;
            }
            Stmt::ClassDef(class) => {
                self.checker.scopes.enter(Kind::Class, &class.body, None);
                self.visit_body(&class.body);
                self.checker.scopes.leave();
                return;
            }
            _ => {}
        }
        walk_stmt(self, statement);
    }
}
impl Checker<'_> {
    pub(super) fn check_generator_cleanup(&mut self, function: &StmtFunctionDef) {
        let mut nodes = Nodes::default();
        nodes.visit_body(&function.body);
        let mut yielded = Vec::new();
        for context in nodes.contexts {
            let mut nodes = Nodes::default();
            nodes.visit_body(&context.body);
            yielded.extend(nodes.yields);
        }
        if yielded.is_empty() {
            return;
        }
        let warn = !yielded.iter().any(|value| value.is_none_or(constant));
        Scan {
            checker: self,
            name: function.name.as_str(),
            warn,
        }
        .visit_body(&function.body);
    }
    fn generator_context_function(&self, expression: &Expr) -> Option<(&StmtFunctionDef, Context)> {
        let inferred = self.infer_context(expression, Context::Scope(self.scopes.current_index()));
        let super::context_values::Inferred::Generator {
            signature, origin, ..
        } = inferred
        else {
            return None;
        };
        Some((self.source_function(&signature, &origin)?, origin))
    }
    fn generator_handles_exit(&self, attempt: &StmtTry, origin: &Context) -> bool {
        attempt.handlers.iter().any(|handler| {
            let ExceptHandler::ExceptHandler(handler) = handler;
            let Some(value) = &handler.type_ else {
                return true;
            };
            self.expression_target_in(value, origin.clone()).is_some_and(|resolved| {
                matches!(resolved.target, Target::Class(class) if class.identity.0 == "builtins" && matches!(class.name.as_str(), "GeneratorExit" | "Exception"))
            })
        })
    }
    fn generator_cleanup_fails(&self, function: &StmtFunctionDef, origin: &Context) -> bool {
        let mut nodes = Nodes::default();
        nodes.visit_body(&function.body);
        if nodes.yields.len() == 1 && !nodes.yield_following[0] {
            return false;
        }
        let attempts: Vec<_> = nodes
            .tries
            .into_iter()
            .filter(|attempt| {
                let mut nodes = Nodes::default();
                nodes.visit_body(&attempt.body);
                for handler in &attempt.handlers {
                    let ExceptHandler::ExceptHandler(handler) = handler;
                    nodes.visit_body(&handler.body);
                }
                nodes.visit_body(&attempt.orelse);
                nodes.visit_body(&attempt.finalbody);
                !nodes.yields.is_empty()
            })
            .collect();
        if attempts.is_empty() {
            return true;
        }
        !attempts.iter().all(|attempt| !attempt.finalbody.is_empty())
            && !attempts
                .iter()
                .all(|attempt| self.generator_handles_exit(attempt, origin))
    }
    fn check_generator_context(&mut self, context: &StmtWith, name: &str, warn: bool) {
        for item in &context.items {
            if item.optional_vars.is_none() {
                continue;
            }
            self.cleanup_inferred
                .insert(item.context_expr.start().to_u32());
            let fails = self
                .generator_context_function(&item.context_expr)
                .is_some_and(|(function, origin)| self.generator_cleanup_fails(function, &origin));
            if fails && warn {
                self.emit(
                    "W0135",
                    "contextmanager-generator-missing-cleanup",
                    &format!("The context used in function '{name}' will not be exited."),
                    context.range(),
                );
            }
        }
    }
}
