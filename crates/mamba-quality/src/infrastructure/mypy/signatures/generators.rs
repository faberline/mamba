//! Only yields evaluated in the function's own frame make it a generator.
use super::*;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    ExprLambda, Parameters, Stmt,
};

#[derive(Default)]
struct Yields(bool);

impl Yields {
    fn defaults(&mut self, parameters: &Parameters) {
        for parameter in parameters.iter_non_variadic_params() {
            if let Some(default) = parameter.default.as_deref() {
                self.visit_expr(default);
            }
        }
    }
}

impl<'a> Visitor<'a> for Yields {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::FunctionDef(function) => {
                for decorator in &function.decorator_list {
                    self.visit_decorator(decorator);
                }
                self.defaults(&function.parameters);
            }
            Stmt::ClassDef(class) => {
                for decorator in &class.decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(arguments) = &class.arguments {
                    self.visit_arguments(arguments);
                }
            }
            _ => walk_stmt(self, statement),
        }
    }

    fn visit_expr(&mut self, expression: &'a Expr) {
        match expression {
            Expr::Yield(_) | Expr::YieldFrom(_) => self.0 = true,
            Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    self.defaults(parameters);
                }
            }
            _ => walk_expr(self, expression),
        }
    }
}

impl Signature {
    pub(in crate::infrastructure::mypy) fn is_generator(function: &StmtFunctionDef) -> bool {
        let mut yields = Yields::default();
        yields.visit_body(&function.body);
        yields.0
    }

    pub(in crate::infrastructure::mypy) fn generator_lambda(lambda: &ExprLambda) -> Option<Self> {
        let mut yields = Yields::default();
        yields.visit_expr(&lambda.body);
        if !yields.0 {
            return None;
        }
        let mut signature = Self::empty(String::new(), Type::Any);
        signature.name = None;
        signature.implicit = true;
        signature.constructor = false;
        signature.return_explicit = false;
        if let Some(parameters) = &lambda.parameters {
            let mut values = Vec::new();
            for (parameters, kind) in [
                (&parameters.posonlyargs, Kind::PositionalOnly),
                (&parameters.args, Kind::PositionalOrKeyword),
                (&parameters.kwonlyargs, Kind::KeywordOnly),
            ] {
                values.extend(parameters.iter().map(|parameter| Parameter {
                    name: parameter.parameter.name.to_string(),
                    ty: Some(Type::Any),
                    kind,
                    required: parameter.default.is_none(),
                }));
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
            signature.parameters = values.into();
        }
        Some(signature)
    }
}
