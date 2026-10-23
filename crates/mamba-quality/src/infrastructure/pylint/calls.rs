//! Call arguments use Pylint inference and its diagnostic ordering.
use super::{
    arguments::Arguments,
    callables::{Kind, Returns, Signature, Target},
    values::Value,
    Checker,
};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprCall, StmtAssign};
use ruff_text_size::Ranged;
use std::{collections::BTreeSet, rc::Rc};

impl Checker<'_> {
    fn signature(&self, expression: &Expr) -> Option<Rc<Signature>> {
        match self.callable_target(expression)? {
            Target::Function(signature) => Some(signature),
            _ => None,
        }
    }

    pub(super) fn check_call_assignment(&mut self, statement: &StmtAssign) {
        let Expr::Call(call) = statement.value.as_ref() else {
            return;
        };
        if self.check_side_effect_assignment(statement, call) {
            return;
        }
        if self.expression_target(&call.func).is_some_and(|value| {
            matches!(value.context, super::resolution::Context::Module(ref module) if module == "builtins")
        }) {
            return;
        }
        let Some(signature) = self.signature(&call.func) else {
            return;
        };
        match signature.returns {
            Returns::NoReturn => self.emit(
                "E1111",
                "assignment-from-no-return",
                &format!(
                    "Assigning result of a function call, but '{}' returns None",
                    signature.name
                ),
                statement.range(),
            ),
            Returns::None => self.emit(
                "E1128",
                "assignment-from-none",
                "Assigning result of a function call, where the function returns None",
                statement.range(),
            ),
            _ => {}
        }
    }

    pub(super) fn check_call(&mut self, call: &ExprCall) {
        self.check_super_arguments_style(call);
        if let Expr::Lambda(_) = call.func.as_ref() {
            self.emit(
                "C3002",
                "unnecessary-direct-lambda-call",
                "Lambda expression called directly. Execute the expression inline instead.",
                call.range(),
            );
        }
        if let Expr::Name(name) = call.func.as_ref() {
            if self
                .scopes
                .before_definition(name.id.as_str(), name.start())
            {
                return;
            }
        }
        self.check_isinstance(call);
        let Some(resolved) = self.expression_target(&call.func) else {
            return;
        };
        let builtin_callable = matches!(&resolved.context, super::resolution::Context::Module(module) if module == "builtins");
        if let Target::Function(signature) = &resolved.target {
            if matches!(signature.category, "method" | "classmethod")
                && self
                    .method_context
                    .as_deref()
                    .is_some_and(|name| super::defaults::fixed().defining_method(name))
            {
                if let Some((class, _)) = self.class_stack.last() {
                    self.initialized_methods
                        .entry(class.identity.clone())
                        .or_default()
                        .insert(signature.name.clone());
                }
            }
        }
        let signature = match resolved.target {
            Target::Class(class) => {
                if class.identity.0 == "builtins" {
                    return;
                }
                let Some(signature) = self.constructor(class, resolved.context) else {
                    return;
                };
                signature
            }
            Target::Instance(class, origin) => {
                if self.class_member(class, origin, "__call__", true).is_none() {
                    let expression = &self.source.text[call.func.range()];
                    self.emit(
                        "E1102",
                        "not-callable",
                        &format!("{expression} is not callable"),
                        call.range(),
                    );
                }
                return;
            }
            Target::Value(value)
                if !matches!(value.as_ref(), Value::Unknown | Value::StringMethod(_, _)) =>
            {
                let expression = &self.source.text[call.func.range()];
                self.emit(
                    "E1102",
                    "not-callable",
                    &format!("{expression} is not callable"),
                    call.range(),
                );
                return;
            }
            Target::Function(signature) => signature,
            _ => return,
        };
        if builtin_callable {
            return;
        }
        if signature.keywords.is_none() {
            let positional_only: BTreeSet<_> = signature
                .parameters
                .iter()
                .filter(|parameter| parameter.kind == Kind::PositionalOnly)
                .map(|parameter| parameter.name.as_str())
                .collect();
            let keywords: Vec<_> = call
                .arguments
                .keywords
                .iter()
                .filter_map(|keyword| keyword.arg.as_ref())
                .filter(|name| positional_only.contains(name.as_str()))
                .map(|name| format!("'{name}'"))
                .collect();
            if !keywords.is_empty() {
                let expression = &self.source.text[call.func.range()];
                self.emit("E3102", "positional-only-arguments-expected", &format!("`{expression}()` got some positional-only arguments passed as keyword arguments: {}", keywords.join(", ")), call.range());
            }
        }
        let arguments = Arguments::from_call(self, call);
        for keyword in &arguments.duplicates {
            self.emit(
                "E1132",
                "repeated-keyword",
                &format!(
                    "Got multiple values for keyword argument '{keyword}' in {} call",
                    signature.category
                ),
                call.range(),
            );
        }
        if arguments.invalid {
            return;
        }
        self.check_argument_order(call, &signature);
        let positional: Vec<_> = signature
            .parameters
            .iter()
            .filter(|parameter| parameter.kind != Kind::KeywordOnly)
            .collect();
        if arguments.positional > positional.len() && !signature.variadic {
            self.emit(
                "E1121",
                "too-many-function-args",
                &format!(
                    "Too many positional arguments for {} call",
                    signature.category
                ),
                call.range(),
            );
        }
        let mut filled: BTreeSet<_> = positional
            .iter()
            .take(arguments.positional)
            .map(|parameter| parameter.name.clone())
            .collect();
        for keyword in arguments.keywords {
            if let Some(variadic) = &signature.keywords {
                if signature.parameters.iter().any(|parameter| {
                    parameter.kind == Kind::PositionalOnly && parameter.name == keyword
                }) {
                    self.emit("W1117", "kwarg-superseded-by-positional-arg", &format!("'{keyword}' will be included in '**{variadic}' since a positional-only parameter with this name already exists"), call.range());
                    continue;
                }
            }
            if signature
                .parameters
                .iter()
                .any(|parameter| parameter.name == keyword)
            {
                if !filled.insert(keyword.clone()) {
                    self.emit(
                        "E1124",
                        "redundant-keyword-arg",
                        &format!(
                            "Argument '{keyword}' passed by position and keyword in {} call",
                            signature.category
                        ),
                        call.range(),
                    );
                }
            } else if signature.keywords.is_none() {
                self.emit(
                    "E1123",
                    "unexpected-keyword-arg",
                    &format!(
                        "Unexpected keyword argument '{keyword}' in {} call",
                        signature.category
                    ),
                    call.range(),
                );
            }
        }
        if !arguments.might_supply_keywords {
            for parameter in &signature.parameters {
                if !parameter.required || filled.contains(&parameter.name) {
                    continue;
                }
                if parameter.kind == Kind::KeywordOnly {
                    self.emit(
                        "E1125",
                        "missing-kwoa",
                        &format!(
                            "Missing mandatory keyword argument '{}' in {} call",
                            parameter.name, signature.category
                        ),
                        call.range(),
                    );
                } else {
                    self.emit(
                        "E1120",
                        "no-value-for-parameter",
                        &format!(
                            "No value for argument '{}' in {} call",
                            parameter.name, signature.category
                        ),
                        call.range(),
                    );
                }
            }
        }
    }
}
