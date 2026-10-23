//! Return consistency follows the original structural walk and termination checks.
use super::{callables::Target, values::Value, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, Visitor},
    ExceptHandler, Expr, Number, Stmt,
};
use ruff_text_size::Ranged;
use std::collections::BTreeSet;

pub(super) fn has_break(body: &[Stmt]) -> bool {
    body.iter().any(|statement| match statement {
        Stmt::Break(_) => true,
        Stmt::If(conditional) => {
            has_break(&conditional.body)
                || conditional
                    .elif_else_clauses
                    .iter()
                    .any(|clause| has_break(&clause.body))
        }
        Stmt::With(with) => has_break(&with.body),
        Stmt::Try(attempt) => {
            has_break(&attempt.body)
                || has_break(&attempt.orelse)
                || has_break(&attempt.finalbody)
                || attempt.handlers.iter().any(|handler| {
                    let ExceptHandler::ExceptHandler(handler) = handler;
                    has_break(&handler.body)
                })
        }
        Stmt::For(loop_) => has_break(&loop_.orelse),
        Stmt::While(loop_) => has_break(&loop_.orelse),
        _ => false,
    })
}

struct Calls<'a, 'b> {
    checker: &'a Checker<'b>,
    ended: bool,
}
impl<'a> Visitor<'a> for Calls<'_, '_> {
    fn visit_expr(&mut self, value: &'a Expr) {
        if let Expr::Call(call) = value {
            self.ended |= self.checker.terminating_call(call, false);
            return;
        }
        if !self.ended {
            walk_expr(self, value);
        }
    }
}

impl Checker<'_> {
    pub(super) fn terminating_call(
        &self,
        call: &mamba_language::application::quality_syntax::python_ast::ExprCall,
        unreachable: bool,
    ) -> bool {
        let context = self.flow.calls.get(&call.start().to_u32());
        if unreachable && context.is_some_and(|value| value.lambda) {
            return false;
        }
        let builtin = !context.is_some_and(|value| value.lambda);
        match call.func.as_ref() {
            Expr::Name(name)
                if matches!(name.id.as_str(), "exit" | "quit")
                    && !self.scopes.declared(name.id.as_str())
                    && builtin =>
            {
                return true;
            }
            Expr::Attribute(attribute)
                if self
                    .module_expression(&attribute.value)
                    .is_some_and(|module| {
                        (module == "sys" && attribute.attr.as_str() == "exit")
                            || (module == "os" && attribute.attr.as_str() == "_exit")
                    })
                    && builtin =>
            {
                return true;
            }
            _ => {}
        }
        if builtin && super::values::path(&call.func).is_some_and(|path| self.terminal_path(path)) {
            return true;
        }
        matches!(self.callable_target(&call.func), Some(Target::Function(signature)) if signature.never_returns && (!unreachable || !signature.is_async || context.is_some_and(|value| value.awaited)))
    }

    fn terminal_path(&self, mut path: Vec<String>) -> bool {
        let mut seen_bindings = BTreeSet::new();
        loop {
            let Some(first) = path.first() else {
                return false;
            };
            let Some((scope, target)) = self
                .scopes
                .call_target_in(first, self.scopes.current_index())
            else {
                return false;
            };
            // A reference may append members on every visit, so the full path
            // need not repeat even when the same binding forms a cycle.
            if !seen_bindings.insert((scope, first.clone())) {
                return false;
            }
            let mut qualified = match target {
                Target::Imported(import) => {
                    let mut names = vec![import.module];
                    if let Some(member) = import.member {
                        names.push(member);
                    }
                    names
                }
                Target::Reference(mut names) => {
                    names.extend(path.into_iter().skip(1));
                    path = names;
                    continue;
                }
                Target::Module(module) => vec![module],
                _ => return false,
            };
            qualified.extend(path.into_iter().skip(1));
            return matches!(
                qualified
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .as_slice(),
                ["sys", "exit"]
                    | ["os", "_exit"]
                    | ["posix", "_exit"]
                    | ["nt", "_exit"]
                    | ["builtins", "exit"]
                    | ["builtins", "quit"]
            );
        }
    }

    fn expression_ended(&self, value: &Expr) -> bool {
        let mut calls = Calls {
            checker: self,
            ended: false,
        };
        calls.visit_expr(value);
        calls.ended
    }

    pub(super) fn body_return_ended(&self, body: &[Stmt]) -> bool {
        self.body_return_ended_in(body, false)
    }

    fn body_return_ended_in(&self, body: &[Stmt], branch: bool) -> bool {
        body.iter().enumerate().any(|(index, statement)| match statement {
            Stmt::Return(_) | Stmt::Raise(_) => true,
            Stmt::Expr(value) => self.expression_ended(&value.value),
            Stmt::Assign(value) => self.expression_ended(&value.value),
            Stmt::AnnAssign(value) => value.value.as_deref().is_some_and(|value| self.expression_ended(value)),
            Stmt::AugAssign(value) => self.expression_ended(&value.value),
            Stmt::If(conditional) => {
                let body_ended = self.branch_return_ended(&conditional.body);
                if conditional.elif_else_clauses.is_empty() {
                    body_ended && body[index+1..].iter().any(|statement| matches!(statement, Stmt::Return(_)))
                } else {
                    body_ended && self.clauses_return_ended(&conditional.elif_else_clauses)
                }
            }
            Stmt::For(loop_) => self.body_return_ended(&loop_.body) || self.body_return_ended(&loop_.orelse),
            Stmt::While(loop_) => {
                let truthy = match self.static_value(&loop_.test) {
                    Value::Boolean(value) => value,
                    Value::Number(Number::Int(value)) => value.to_string() != "0",
                    Value::Number(Number::Float(value)) => value != 0.0,
                    Value::String(value) => !value.is_empty(),
                    Value::Bytes(value) => !value.is_empty(),
                    Value::Sequence(value, _) => !value.is_empty(),
                    Value::Mapping(value) => !value.is_empty(),
                    _ => false,
                };
                (truthy && !has_break(&loop_.body)) || self.body_return_ended(&loop_.orelse)
            }
            Stmt::With(with) => self.body_return_ended(&with.body),
            Stmt::Match(value) => value.cases.iter().any(|case| self.body_return_ended(&case.body)),
            Stmt::Try(attempt) => (self.body_return_ended(&attempt.body) || self.body_return_ended(&attempt.orelse) || self.body_return_ended(&attempt.finalbody)) && attempt.handlers.iter().all(|handler| { let ExceptHandler::ExceptHandler(handler) = handler; self.body_return_ended(&handler.body) }),
            Stmt::Assert(assertion) => matches!(assertion.test.as_ref(), Expr::BooleanLiteral(value) if !value.value) || matches!(assertion.test.as_ref(), Expr::NumberLiteral(value) if matches!(&value.value, Number::Int(value) if value.to_string() == "0")),
            Stmt::FunctionDef(function) if !branch => self.body_return_ended(&function.body),
            Stmt::ClassDef(class) => self.body_return_ended(&class.body),
            _ => false,
        })
    }

    fn branch_return_ended(&self, body: &[Stmt]) -> bool {
        self.body_return_ended_in(body, true)
    }

    fn clauses_return_ended(
        &self,
        clauses: &[mamba_language::application::quality_syntax::python_ast::ElifElseClause],
    ) -> bool {
        let Some((first, remaining)) = clauses.split_first() else {
            return false;
        };
        let ended = self.branch_return_ended(&first.body);
        if first.test.is_none() {
            ended
        } else {
            ended && self.clauses_return_ended(remaining)
        }
    }
}
