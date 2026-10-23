//! CPython compound statement fields and nested control-flow nodes.
use super::{
    ast::{Builder, Result},
    tree::{self, Value},
};
use ruff_python_ast::Stmt;
impl Builder<'_> {
    pub fn compound_statement(&self, statement: &Stmt) -> Result {
        use tree::{node, string};
        Ok(match statement {
            Stmt::FunctionDef(value) => node(
                if value.is_async {
                    "AsyncFunctionDef"
                } else {
                    "FunctionDef"
                },
                [
                    ("name", string(value.name.to_string())),
                    ("args", self.parameters(Some(&value.parameters))?),
                    ("body", self.body(&value.body)?),
                    (
                        "decorator_list",
                        self.exprs(value.decorator_list.iter().map(|d| &d.expression))?,
                    ),
                    ("returns", self.expr_optional(value.returns.as_deref())?),
                    ("type_comment", self.comment(statement)),
                    (
                        "type_params",
                        self.type_params(value.type_params.as_deref())?,
                    ),
                ],
            ),
            Stmt::ClassDef(value) => {
                let (bases, keywords) = self.arguments(value.arguments.as_deref())?;
                node(
                    "ClassDef",
                    [
                        ("name", string(value.name.to_string())),
                        ("bases", bases),
                        ("keywords", keywords),
                        ("body", self.body(&value.body)?),
                        (
                            "decorator_list",
                            self.exprs(value.decorator_list.iter().map(|d| &d.expression))?,
                        ),
                        (
                            "type_params",
                            self.type_params(value.type_params.as_deref())?,
                        ),
                    ],
                )
            }
            Stmt::For(value) => node(
                if value.is_async { "AsyncFor" } else { "For" },
                [
                    ("target", self.expr(&value.target)?),
                    ("iter", self.expr(&value.iter)?),
                    ("body", self.body(&value.body)?),
                    ("orelse", self.body(&value.orelse)?),
                    ("type_comment", self.comment(statement)),
                ],
            ),
            Stmt::While(value) => node(
                "While",
                [
                    ("test", self.expr(&value.test)?),
                    ("body", self.body(&value.body)?),
                    ("orelse", self.body(&value.orelse)?),
                ],
            ),
            Stmt::If(value) => node(
                "If",
                [
                    ("test", self.expr(&value.test)?),
                    ("body", self.body(&value.body)?),
                    ("orelse", self.if_tail(&value.elif_else_clauses)?),
                ],
            ),
            Stmt::With(value) => node(
                if value.is_async { "AsyncWith" } else { "With" },
                [
                    (
                        "items",
                        Value::List(
                            value
                                .items
                                .iter()
                                .map(|item| {
                                    Ok(node(
                                        "withitem",
                                        [
                                            ("context_expr", self.expr(&item.context_expr)?),
                                            (
                                                "optional_vars",
                                                self.expr_optional(item.optional_vars.as_deref())?,
                                            ),
                                        ],
                                    ))
                                })
                                .collect::<std::result::Result<Vec<_>, String>>()?,
                        ),
                    ),
                    ("body", self.body(&value.body)?),
                    ("type_comment", self.comment(statement)),
                ],
            ),
            Stmt::Match(value) => node(
                "Match",
                [
                    ("subject", self.expr(&value.subject)?),
                    (
                        "cases",
                        Value::List(
                            value
                                .cases
                                .iter()
                                .map(|case| {
                                    Ok(node(
                                        "match_case",
                                        [
                                            ("pattern", self.pattern(&case.pattern)?),
                                            ("guard", self.expr_optional(case.guard.as_deref())?),
                                            ("body", self.body(&case.body)?),
                                        ],
                                    ))
                                })
                                .collect::<std::result::Result<Vec<_>, String>>()?,
                        ),
                    ),
                ],
            ),
            Stmt::Try(value) => self.try_statement(value)?,
            _ => return Err("Expected a compound statement".into()),
        })
    }
    fn if_tail(&self, clauses: &[ruff_python_ast::ElifElseClause]) -> Result {
        let Some((first, rest)) = clauses.split_first() else {
            return Ok(Value::List(Vec::new()));
        };
        if let Some(test) = &first.test {
            Ok(Value::List(vec![tree::node(
                "If",
                [
                    ("test", self.expr(test)?),
                    ("body", self.body(&first.body)?),
                    ("orelse", self.if_tail(rest)?),
                ],
            )]))
        } else {
            self.body(&first.body)
        }
    }
    fn try_statement(&self, value: &ruff_python_ast::StmtTry) -> Result {
        let handlers = value
            .handlers
            .iter()
            .map(|handler| {
                let ruff_python_ast::ExceptHandler::ExceptHandler(handler) = handler;
                Ok(tree::node(
                    "ExceptHandler",
                    [
                        ("type", self.expr_optional(handler.type_.as_deref())?),
                        (
                            "name",
                            tree::optional_string(handler.name.as_ref().map(|name| name.as_str())),
                        ),
                        ("body", self.body(&handler.body)?),
                    ],
                ))
            })
            .collect::<std::result::Result<Vec<_>, String>>()?;
        Ok(tree::node(
            if value.is_star { "TryStar" } else { "Try" },
            [
                ("body", self.body(&value.body)?),
                ("handlers", Value::List(handlers)),
                ("orelse", self.body(&value.orelse)?),
                ("finalbody", self.body(&value.finalbody)?),
            ],
        ))
    }
}
