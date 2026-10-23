//! Guard call refinements use the same parameter binding and generic solution as calls.
use super::*;
use quality_syntax::python_ast::{ExprCall, StmtFunctionDef};
use signatures::{Kind, NarrowingKind, Signature};

impl Checker<'_> {
    pub(super) fn runtime_predicate_name(&self, expression: &Expr) -> Option<&'static str> {
        self.scopes
            .runtime_predicate_name(expression, &self.environment())
    }

    fn refine_runtime_predicate(&mut self, call: &ExprCall, positive: bool) -> bool {
        let environment = self.environment();
        let (recognized, refinement) =
            self.scopes
                .runtime_predicate_refinement(call, positive, &environment, &|expression| {
                    self.infer(expression)
                });
        if let Some((key, narrowed)) = refinement {
            self.flow.narrowed.insert(key, narrowed);
            self.flow.reachable &= narrowed != types::Type::Never;
        }
        recognized
    }

    pub(super) fn check_predicate_declaration(
        &mut self,
        function: &StmtFunctionDef,
        signature: &Signature,
        implicit_receiver: bool,
    ) {
        let Some((kind, target)) = signature.narrowing else {
            return;
        };
        let input = signature
            .parameters
            .iter()
            .skip(usize::from(implicit_receiver))
            .find(|p| matches!(p.kind, Kind::PositionalOnly | Kind::PositionalOrKeyword));
        let Some(input) = input else {
            let name = match kind {
                NarrowingKind::TypeGuard => "TypeGuard",
                NarrowingKind::TypeIs => "\"TypeIs\"",
            };
            self.emit(
                "valid-type",
                format!("{name} functions must have a positional argument"),
                &ruff_text_size::TextRange::new(
                    function.start(),
                    function.start() + ruff_text_size::TextSize::new(1),
                ),
            );
            return;
        };
        if kind == NarrowingKind::TypeIs {
            if let Some(input) = input.ty {
                if !self.context.accepts(input, target) {
                    self.emit(
                        "narrowed-type-not-subtype",
                        format!(
                            "Narrowed type \"{}\" is not a subtype of input type \"{}\"",
                            self.context.name(target),
                            self.context.name(input)
                        ),
                        &self.function_range(function),
                    );
                }
            }
        }
    }

    pub(super) fn refine_predicate_call(&mut self, call: &ExprCall, positive: bool) -> bool {
        if self.refine_runtime_predicate(call, positive) {
            return true;
        }
        let environment = self.environment();
        let Some(mut signature) = self.scopes.callable_expression_in(&call.func, &environment)
        else {
            return false;
        };
        if !signature.overloads.is_empty() {
            let resolution = self
                .scopes
                .resolve_overload(call, &signature, &environment, None);
            if let Some(selected) = resolution.candidate.filter(|_| resolution.matched) {
                signature = selected;
            }
        }
        if signature.definition_module.as_deref() == Some("builtins")
            && signature.name.as_deref() == Some("callable")
        {
            if let Some(Expr::Name(name)) = call.arguments.args.first() {
                if let Some(&input) = environment.get(name.id.as_str()) {
                    let module = self.scopes.qualified_name("");
                    let narrowed = self.context.callable_partition(
                        input,
                        module.trim_end_matches('.'),
                        positive,
                    );
                    self.flow.narrowed.insert(name.id.to_string(), narrowed);
                    self.flow.reachable &= narrowed != types::Type::Never;
                    return true;
                }
            }
        }
        let Some((kind, target)) = signature.narrowing else {
            return false;
        };
        let Some(parameter) = signature
            .parameters
            .iter()
            .find(|p| matches!(p.kind, Kind::PositionalOnly | Kind::PositionalOrKeyword))
        else {
            return false;
        };
        let argument = call
            .arguments
            .args
            .first()
            .filter(|argument| !argument.is_starred_expr())
            .or_else(|| {
                call.arguments.keywords.iter().find_map(|keyword| {
                    (keyword
                        .arg
                        .as_ref()
                        .is_some_and(|name| name.as_str() == parameter.name))
                    .then_some(&keyword.value)
                })
            });
        let Some(argument) = argument else {
            if !call.arguments.args.is_empty() || !call.arguments.keywords.is_empty() {
                let kind = match kind {
                    NarrowingKind::TypeGuard => "guard",
                    NarrowingKind::TypeIs => "narrower",
                };
                self.emit(
                    "misc",
                    format!("Type {kind} requires positional argument"),
                    call,
                );
                return true;
            }
            return false;
        };
        let argument = if let Expr::Named(named) = argument {
            named.target.as_ref()
        } else {
            argument
        };
        let Some(key) = scopes::Scopes::attribute_key(argument) else {
            return false;
        };
        let Some(input) = self.infer(argument) else {
            return false;
        };
        let solution = self
            .scopes
            .call_solution(call, &signature, &environment, None);
        let target = self.context.substitute(target, &solution.types);
        let narrowed = match kind {
            NarrowingKind::TypeGuard if positive => target,
            NarrowingKind::TypeGuard => input,
            NarrowingKind::TypeIs => {
                let module = self.scopes.qualified_name("");
                self.context.predicate_intersection(
                    input,
                    target,
                    positive,
                    module.trim_end_matches('.'),
                )
            }
        };
        self.flow.narrowed.insert(key, narrowed);
        self.flow.reachable &= narrowed != types::Type::Never;
        true
    }
}
