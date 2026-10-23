//! Pylint call metadata is independent of static annotation types.
mod collector;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Expr, ExprCall, ExprLambda, Parameters, Stmt, StmtFunctionDef,
};
use std::{collections::BTreeMap, rc::Rc};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    PositionalOnly,
    Positional,
    KeywordOnly,
}

#[derive(Clone)]
pub(super) struct Parameter {
    pub name: String,
    pub required: bool,
    pub kind: Kind,
    pub default: Option<super::values::Value>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Returns {
    NoReturn,
    None,
    Value,
    Unknown,
}

#[derive(Clone)]
pub(super) struct Signature {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub variadic: bool,
    pub keywords: Option<String>,
    pub returns: Returns,
    pub never_returns: bool,
    pub is_async: bool,
    pub direct_raise: bool,
    pub category: &'static str,
    pub result: super::values::Value,
    pub owner: Option<(String, String)>,
    pub result_class: Option<(String, String)>,
    pub decorators: Vec<Expr>,
    pub abstract_body: bool,
    pub pass_body: bool,
    pub parameter_ast: Option<Parameters>,
    pub return_ast: Option<Expr>,
    pub result_expression: Option<Expr>,
    pub result_expressions: Vec<Expr>,
    pub result_bindings: BTreeMap<String, Expr>,
    pub generator_expression_returns: bool,
    pub generates: bool,
    pub definition_range: ruff_text_size::TextRange,
}

#[derive(Clone)]
pub(super) enum Target {
    Function(Rc<Signature>),
    Reference(Vec<String>),
    Value(Rc<super::values::Value>),
    Imported(crate::infrastructure::project::Import),
    Module(String),
    Class(Rc<super::classes::Class>),
    Instance(Rc<super::classes::Class>, super::resolution::Context),
    Super(Rc<super::super_calls::Proxy>),
    Construct(Rc<ExprCall>),
    Contextual(Box<Target>, super::resolution::Context),
}

#[derive(Default)]
struct ReturnValues {
    values: Vec<Returns>,
    generator: bool,
    expressions: Vec<super::values::Value>,
    syntax: Vec<Expr>,
}
impl<'a> Visitor<'a> for ReturnValues {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        match statement {
            Stmt::Return(statement) => {
                self.syntax.extend(statement.value.as_deref().cloned());
                self.expressions.push(
                    statement
                        .value
                        .as_deref()
                        .map(super::values::Value::from_expression)
                        .unwrap_or(super::values::Value::Scalar("NoneType")),
                );
                self.values.push(
                    if statement
                        .value
                        .as_deref()
                        .is_none_or(|value| matches!(value, Expr::NoneLiteral(_)))
                    {
                        Returns::None
                    } else {
                        Returns::Value
                    },
                );
            }
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            _ => walk_stmt(self, statement),
        }
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        if matches!(expression, Expr::Yield(_) | Expr::YieldFrom(_)) {
            self.generator = true;
        }
        if !matches!(expression, Expr::Lambda(_)) {
            walk_expr(self, expression);
        }
    }
}

impl Signature {
    fn parameters(parameters: &Parameters) -> Vec<Parameter> {
        let mut result = Vec::new();
        for (group, kind) in [
            (&parameters.posonlyargs, Kind::PositionalOnly),
            (&parameters.args, Kind::Positional),
            (&parameters.kwonlyargs, Kind::KeywordOnly),
        ] {
            result.extend(group.iter().map(|parameter| {
                Parameter {
                    name: parameter.parameter.name.to_string(),
                    required: parameter.default.is_none(),
                    kind,
                    default: parameter
                        .default
                        .as_deref()
                        .map(super::values::Value::from_expression),
                }
            }));
        }
        result
    }

    pub(super) fn from_function(function: &StmtFunctionDef) -> Self {
        let parameters = Self::parameters(&function.parameters);
        let mut values = ReturnValues::default();
        values.visit_body(&function.body);
        let body = &function.body[usize::from(super::docstring(&function.body))..];
        let returns = if function.is_async || values.generator {
            Returns::Value
        } else if !function.decorator_list.is_empty()
            || (!body.is_empty()
                && body
                    .iter()
                    .all(|statement| matches!(statement, Stmt::Raise(_))))
        {
            Returns::Unknown
        } else if values.values.is_empty() {
            Returns::NoReturn
        } else if values.values.iter().all(|value| *value == Returns::None) {
            Returns::None
        } else {
            Returns::Value
        };
        let result = if function.is_async || values.generator || returns == Returns::Unknown {
            super::values::Value::Unknown
        } else if values.expressions.len() == 1 {
            values.expressions.pop().unwrap()
        } else if returns == Returns::NoReturn || returns == Returns::None {
            super::values::Value::Scalar("NoneType")
        } else {
            super::values::Value::Unknown
        };
        Self {
            name: function.name.to_string(),
            definition_range: function.name.range,
            result_expression: (!function.is_async && !values.generator && values.syntax.len() == 1).then(|| values.syntax[0].clone()),
            result_expressions: values.syntax.clone(),
            result_bindings: super::source_bindings::collect(&function.body),
            generator_expression_returns: super::condition_inference::only_generator_returns(function),
            generates: values.generator,
            parameter_ast: Some((*function.parameters).clone()),
            return_ast: function.returns.as_deref().cloned(),
            parameters,
            variadic: function.parameters.vararg.is_some(),
            keywords: function
                .parameters
                .kwarg
                .as_ref()
                .map(|parameter| parameter.name.to_string()),
            returns,
            is_async: function.is_async,
            direct_raise: function.body.iter().any(|value| matches!(value, Stmt::Raise(_))),
            never_returns: function.returns.as_deref().is_some_and(|value| {
                matches!(value, Expr::Name(name) if matches!(name.id.as_str(), "Never" | "NoReturn"))
                    || matches!(value, Expr::Attribute(attribute) if matches!(attribute.attr.as_str(), "Never" | "NoReturn"))
            }),
            category: "function",
            result,
            owner: None,
            result_class: None,
            decorators: function.decorator_list.iter().map(|value| value.expression.clone()).collect(),
            pass_body: body.first().is_none_or(|value| matches!(value, Stmt::Pass(_))),
            abstract_body: matches!(body.first(), Some(Stmt::Raise(value)) if value.exc.as_deref().is_some_and(|value| match value {
                Expr::Name(value) => value.id.as_str() == "NotImplementedError",
                Expr::Call(value) => matches!(value.func.as_ref(), Expr::Name(value) if value.id.as_str() == "NotImplementedError"),
                _ => false,
            })),
        }
    }

    pub(super) fn from_lambda(lambda: &ExprLambda) -> Self {
        Self {
            name: "<lambda>".into(),
            definition_range: lambda.range,
            result_expression: Some((*lambda.body).clone()),
            result_expressions: vec![(*lambda.body).clone()],
            result_bindings: BTreeMap::new(),
            generator_expression_returns: false,
            generates: false,
            parameter_ast: lambda.parameters.as_deref().cloned(),
            return_ast: None,
            parameters: lambda
                .parameters
                .as_deref()
                .map(Self::parameters)
                .unwrap_or_default(),
            variadic: lambda
                .parameters
                .as_ref()
                .is_some_and(|parameters| parameters.vararg.is_some()),
            keywords: lambda
                .parameters
                .as_ref()
                .and_then(|parameters| parameters.kwarg.as_ref())
                .map(|parameter| parameter.name.to_string()),
            returns: Returns::Unknown,
            never_returns: false,
            is_async: false,
            direct_raise: false,
            category: "lambda",
            result: super::values::Value::from_expression(&lambda.body),
            owner: None,
            result_class: None,
            decorators: Vec::new(),
            abstract_body: false,
            pass_body: false,
        }
    }

    pub(super) fn bound(&self, implicit: usize, category: &'static str) -> Self {
        Self {
            parameters: self.parameters.iter().skip(implicit).cloned().collect(),
            category,
            ..self.clone()
        }
    }
}

#[derive(Default)]
struct Collector {
    targets: BTreeMap<String, Target>,
    current: String,
    is_package: bool,
    first_targets: BTreeMap<String, Target>,
    record_first: bool,
}

pub(super) fn collect(body: &[Stmt], current: &str, is_package: bool) -> BTreeMap<String, Target> {
    let mut collector = Collector {
        current: current.into(),
        is_package,
        ..Collector::default()
    };
    collector.visit_body(body);
    collector.targets
}

pub(super) fn collect_with_first(
    body: &[Stmt],
    current: &str,
    is_package: bool,
) -> (BTreeMap<String, Target>, BTreeMap<String, Target>) {
    let mut collector = Collector {
        current: current.into(),
        is_package,
        record_first: true,
        ..Collector::default()
    };
    collector.visit_body(body);
    (collector.targets, collector.first_targets)
}
