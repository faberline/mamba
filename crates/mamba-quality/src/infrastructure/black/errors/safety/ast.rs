//! Adapt the existing parser and protected type-comment ownership to CPython fields.
use super::tree::{self, Value};
use mamba_language::application::quality_syntax::SyntaxDocument;
use ruff_python_ast::{
    visitor::{self, Visitor},
    Expr, Stmt,
};
use ruff_text_size::Ranged;
use std::collections::BTreeMap;

pub(super) type Result = std::result::Result<Value, String>;
pub(super) struct Builder<'a> {
    pub document: &'a SyntaxDocument<'a>,
    pub statements: BTreeMap<u32, usize>,
    pub arguments: BTreeMap<u32, String>,
    pub comments: BTreeMap<String, String>,
}
struct Indices {
    statements: BTreeMap<u32, usize>,
    arguments: BTreeMap<u32, String>,
}
impl<'a> Visitor<'a> for Indices {
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        let index = self.statements.len();
        self.statements.insert(statement.start().to_u32(), index);
        if let Stmt::FunctionDef(function) = statement {
            for (argument, parameter) in function.parameters.iter().enumerate() {
                self.arguments.insert(
                    parameter.start().to_u32(),
                    format!("argument:{index}:{argument}"),
                );
            }
        }
        visitor::walk_stmt(self, statement);
    }
}
impl Builder<'_> {
    pub fn expr_optional(&self, value: Option<&Expr>) -> Result {
        value.map_or_else(|| Ok(tree::none()), |value| self.expr(value))
    }
    pub fn exprs<'a>(&self, values: impl IntoIterator<Item = &'a Expr>) -> Result {
        values
            .into_iter()
            .map(|value| self.expr(value))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map(Value::List)
    }
    pub fn body(&self, values: &[Stmt]) -> Result {
        values
            .iter()
            .map(|value| self.statement(value))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map(Value::List)
    }
    pub fn comment(&self, statement: &Stmt) -> Value {
        self.statements
            .get(&statement.start().to_u32())
            .and_then(|index| self.comments.get(&format!("statement:{index}")))
            .map_or_else(tree::none, |value| tree::string(value.clone()))
    }
}
pub(super) fn module<'a>(document: &'a SyntaxDocument<'a>) -> Result {
    let mut indices = Indices {
        statements: BTreeMap::new(),
        arguments: BTreeMap::new(),
    };
    for statement in &document.parsed.syntax().body {
        indices.visit_stmt(statement);
    }
    let builder = Builder {
        document,
        statements: indices.statements,
        arguments: indices.arguments,
        comments: crate::infrastructure::black::type_comments::fingerprint(document)
            .into_iter()
            .collect(),
    };
    let ignores = builder
        .comments
        .get("type_ignores")
        .and_then(|count| count.parse::<usize>().ok())
        .unwrap_or(0);
    Ok(tree::node(
        "Module",
        [
            ("body", builder.body(&document.parsed.syntax().body)?),
            (
                "type_ignores",
                Value::List((0..ignores).map(|_| tree::leaf("TypeIgnore")).collect()),
            ),
        ],
    ))
}
