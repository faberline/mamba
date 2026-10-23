//! Lambda forwarding retains original parameter groups and later name mutations.
use super::Checker;
use mamba_language::application::quality_syntax::python_ast::{
    visitor::{walk_expr, walk_stmt, Visitor},
    Expr, ExprContext, ExprLambda, Stmt,
};
use ruff_source_file::LineIndex;
use ruff_text_size::Ranged;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Mutations {
    scopes: BTreeMap<u32, u32>,
    changes: BTreeMap<(u32, String), Vec<u32>>,
    lines: BTreeMap<u32, u32>,
}
struct Collector<'a> {
    source: &'a str,
    index: LineIndex,
    scope: u32,
    data: Mutations,
}
impl Collector<'_> {
    fn line(&self, position: ruff_text_size::TextSize) -> u32 {
        self.index.line_column(position, self.source).line.get() as u32
    }
    fn mutation(&mut self, name: String, position: ruff_text_size::TextSize) {
        let line = self.line(position);
        self.data
            .changes
            .entry((self.scope, name))
            .or_default()
            .push(line);
    }
}
impl<'a> Visitor<'a> for Collector<'_> {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        let name = match statement {
            Stmt::FunctionDef(function) => Some(&function.name),
            Stmt::ClassDef(class) => Some(&class.name),
            _ => None,
        };
        let previous = self.scope;
        if let Some(name) = name {
            self.mutation(name.to_string(), name.start());
            self.scope = statement.start().to_u32();
        }
        walk_stmt(self, statement);
        self.scope = previous;
    }
    fn visit_expr(&mut self, value: &'a Expr) {
        if let Expr::Name(name) = value {
            if matches!(name.ctx, ExprContext::Store | ExprContext::Del) {
                self.mutation(name.id.to_string(), name.start());
            }
        }
        if let Expr::Lambda(lambda) = value {
            self.data.scopes.insert(lambda.start().to_u32(), self.scope);
            self.data
                .lines
                .insert(lambda.start().to_u32(), self.line(lambda.start()));
            let previous = self.scope;
            self.scope = lambda.start().to_u32();
            walk_expr(self, value);
            self.scope = previous;
        } else {
            walk_expr(self, value);
        }
    }
}
impl Mutations {
    pub(super) fn new(body: &[Stmt], source: &str) -> Self {
        let mut collector = Collector {
            source,
            index: LineIndex::from_source_text(source),
            scope: 0,
            data: Self::default(),
        };
        collector.visit_body(body);
        collector.data
    }
    fn after(&self, lambda: &ExprLambda, name: &str) -> bool {
        let key = lambda.start().to_u32();
        let Some(scope) = self.scopes.get(&key) else {
            return false;
        };
        self.changes
            .get(&(*scope, name.into()))
            .is_some_and(|lines| lines.iter().any(|line| *line > self.lines[&key]))
    }
}

#[derive(Default)]
struct Names(Vec<String>);
impl<'a> Visitor<'a> for Names {
    fn visit_expr(&mut self, value: &'a Expr) {
        if let Expr::Name(name) = value {
            self.0.push(name.id.to_string());
        }
        walk_expr(self, value);
    }
}

impl Checker<'_> {
    pub(super) fn check_lambda_effect(&mut self, lambda: &ExprLambda) {
        let Expr::Call(call) = lambda.body.as_ref() else {
            return;
        };
        if matches!(call.func.as_ref(), Expr::Attribute(value) if matches!(value.value.as_ref(), Expr::Call(_)))
        {
            return;
        }
        let parameters = lambda.parameters.as_deref();
        if parameters.is_some_and(|parameters| {
            parameters
                .posonlyargs
                .iter()
                .chain(&parameters.args)
                .any(|parameter| parameter.default.is_some())
        }) {
            return;
        }
        let ordinary = parameters
            .map(|parameters| parameters.args.as_slice())
            .unwrap_or_default();
        let vararg = parameters
            .and_then(|parameters| parameters.vararg.as_ref())
            .map(|parameter| parameter.name.as_str());
        let kwarg = parameters
            .and_then(|parameters| parameters.kwarg.as_ref())
            .map(|parameter| parameter.name.as_str());
        let starred: Vec<_> = call
            .arguments
            .args
            .iter()
            .filter_map(|value| {
                if let Expr::Starred(starred) = value {
                    Some(starred.value.as_ref())
                } else {
                    None
                }
            })
            .collect();
        let variadic_bad = |values: &[&Expr], name: &str| {
            values.is_empty()
                || values
                    .iter()
                    .any(|value| !matches!(value, Expr::Name(value) if value.id.as_str() == name))
        };
        if vararg.map_or(!starred.is_empty(), |name| variadic_bad(&starred, name)) {
            return;
        }
        let keywords: Vec<_> = call
            .arguments
            .keywords
            .iter()
            .map(|keyword| &keyword.value)
            .collect();
        if kwarg.map_or(!keywords.is_empty(), |name| variadic_bad(&keywords, name)) {
            return;
        }
        let arguments: Vec<_> = call.arguments.args.iter().filter(|value| match value {
            Expr::Starred(starred) => matches!(starred.value.as_ref(), Expr::Name(name) if Some(name.id.as_str()) != vararg),
            _ => true,
        }).collect();
        if ordinary.len() != arguments.len() || ordinary.iter().zip(arguments).any(|(parameter, argument)| !matches!(argument, Expr::Name(name) if name.id.as_str() == parameter.parameter.name.as_str())) { return; }
        let mut names = Names::default();
        names.visit_expr(&call.func);
        if names.0.iter().any(|name| {
            parameters.is_some_and(|parameters| {
                parameters
                    .iter()
                    .any(|parameter| parameter.name().as_str() == name)
            }) || self.lambda_mutations.after(lambda, name)
        }) {
            return;
        }
        self.emit(
            "W0108",
            "unnecessary-lambda",
            "Lambda may not be necessary",
            lambda.range(),
        );
    }
}
