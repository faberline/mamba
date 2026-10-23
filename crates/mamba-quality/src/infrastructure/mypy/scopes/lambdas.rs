//! Lambda arguments are local to the expression's callable signature.
use super::*;
use crate::infrastructure::mypy::signatures::{Kind, Parameter};
use mamba_language::application::quality_syntax::python_ast::ExprLambda;

impl Scopes {
    pub(in crate::infrastructure::mypy) fn lambda_type(
        &self,
        lambda: &ExprLambda,
        environment: &BTreeMap<String, Type>,
    ) -> Option<Type> {
        self.lambda_type_context(lambda, environment, None)
    }

    pub(in crate::infrastructure::mypy) fn lambda_type_expected(
        &self,
        lambda: &ExprLambda,
        environment: &BTreeMap<String, Type>,
        expected: Type,
    ) -> Option<Type> {
        let signature = self.context.callable_signature(expected)?;
        self.lambda_type_context(lambda, environment, Some(&signature))
    }

    // Select only one direct Callable; aliases and opaque arms keep the old route.
    pub(in crate::infrastructure::mypy) fn lambda_expected_context(
        &self,
        lambda: &ExprLambda,
        expected: Type,
    ) -> Type {
        if lambda
            .parameters
            .as_ref()
            .is_some_and(|parameters| parameters.vararg.is_some() || parameters.kwarg.is_some())
        {
            return expected;
        }
        let Some(node) = self.context.node(expected) else {
            return expected;
        };
        let types::Node::Union(items) = node.as_ref() else {
            return expected;
        };
        let mut selected = None;
        for &item in items {
            match self.context.node(item).as_deref() {
                Some(types::Node::Callable(signature)) => {
                    if selected.is_some() || !self.flat_lambda_context(signature) {
                        return expected;
                    }
                    selected = Some(item);
                }
                None if !matches!(item, Type::Any | Type::AnySpecial | Type::Never) => {}
                _ => return expected,
            }
        }
        selected.unwrap_or(expected)
    }

    pub(in crate::infrastructure::mypy) fn lambda_context_mismatch(
        &self,
        lambda: &ExprLambda,
        expected: Type,
        signature: &Signature,
    ) -> bool {
        if !matches!(
            self.context.node(expected).as_deref(),
            Some(types::Node::Callable(_))
        ) || !self.flat_lambda_context(signature)
            || lambda
                .parameters
                .as_ref()
                .is_some_and(|parameters| parameters.vararg.is_some() || parameters.kwarg.is_some())
        {
            return false;
        }
        let mut actual = Vec::new();
        if let Some(parameters) = &lambda.parameters {
            for (parameters, keyword) in [
                (&parameters.posonlyargs, false),
                (&parameters.args, false),
                (&parameters.kwonlyargs, true),
            ] {
                actual.extend(
                    parameters
                        .iter()
                        .map(|parameter| (keyword, parameter.default.is_none())),
                );
            }
        }
        let target: Vec<_> = signature
            .parameters
            .iter()
            .map(|parameter| (parameter.kind == Kind::KeywordOnly, parameter.required))
            .collect();
        actual != target
    }

    fn flat_lambda_context(&self, signature: &Signature) -> bool {
        !signature.implicit
            && !signature.ignored
            && !signature.constructor
            && !signature.ellipsis
            && signature.narrowing.is_none()
            && signature.generic.is_empty()
            && signature.overloads.is_empty()
            && signature
                .returns
                .is_some_and(|ty| self.lambda_context_type_known(ty))
            && signature.parameters.iter().all(|parameter| {
                matches!(
                    parameter.kind,
                    Kind::PositionalOnly | Kind::PositionalOrKeyword | Kind::KeywordOnly
                ) && parameter
                    .ty
                    .is_some_and(|ty| self.lambda_context_type_known(ty))
            })
    }

    fn lambda_context_type_known(&self, ty: Type) -> bool {
        match self.context.node(ty).as_deref() {
            None => !matches!(ty, Type::Any | Type::AnySpecial | Type::Never),
            Some(types::Node::Literal(_) | types::Node::LiteralValue(_)) => true,
            Some(types::Node::AppliedClass {
                arguments,
                value: false,
                ..
            }) => {
                !arguments.is_empty()
                    && arguments.iter().all(|&argument| {
                        self.context.node(argument).is_none()
                            && !matches!(argument, Type::Any | Type::AnySpecial | Type::Never)
                    })
            }
            _ => false,
        }
    }

    fn lambda_type_context(
        &self,
        lambda: &ExprLambda,
        environment: &BTreeMap<String, Type>,
        expected: Option<&Signature>,
    ) -> Option<Type> {
        if let Some(signature) = Signature::generator_lambda(lambda) {
            return Some(self.context.callable_type(Arc::new(signature)));
        }
        let mut local = environment.clone();
        let mut values = Vec::new();
        if let Some(parameters) = &lambda.parameters {
            for (parameters, kind) in [
                (&parameters.posonlyargs, Kind::PositionalOnly),
                (&parameters.args, Kind::PositionalOrKeyword),
                (&parameters.kwonlyargs, Kind::KeywordOnly),
            ] {
                for parameter in parameters {
                    let name = parameter.parameter.name.to_string();
                    values.push(Parameter {
                        name,
                        ty: Some(Type::Any),
                        kind,
                        required: parameter.default.is_none(),
                    });
                }
            }
            for (parameter, kind) in [
                (&parameters.vararg, Kind::VariadicPositional),
                (&parameters.kwarg, Kind::VariadicKeyword),
            ] {
                if let Some(parameter) = parameter {
                    values.push(Parameter {
                        name: parameter.name.to_string(),
                        ty: Some(Type::Any),
                        kind,
                        required: false,
                    });
                }
            }
        }
        if let Some(expected) =
            expected.filter(|expected| expected.parameters.len() == values.len())
        {
            for (value, expected) in values.iter_mut().zip(expected.parameters.iter()) {
                value.ty = expected.ty;
            }
        }
        for value in &values {
            local.insert(value.name.clone(), value.ty.unwrap_or(Type::Any));
        }
        let returns = self.context.without_observed(|| {
            self.context
                .infer_with_calls(&lambda.body, &local, &|expression| {
                    self.expression_type(expression, &local)
                })
        })?;
        let mut signature = Signature::empty(String::new(), returns);
        signature.name = None;
        signature.constructor = false;
        signature.implicit = expected.is_none();
        signature.parameters = values.into();
        Some(self.context.callable_type(Arc::new(signature)))
    }
}
