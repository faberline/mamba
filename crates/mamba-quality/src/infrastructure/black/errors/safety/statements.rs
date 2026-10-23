//! CPython statement fields; positions are attributes and are not stringified.
use super::{
    ast::{Builder, Result},
    tree::{self, Value},
};
use ruff_python_ast::{Expr, Stmt};

impl Builder<'_> {
    pub fn statement(&self, statement: &Stmt) -> Result {
        use tree::{integer, leaf, node, optional_string, string};
        Ok(match statement {
            Stmt::FunctionDef(_)
            | Stmt::ClassDef(_)
            | Stmt::For(_)
            | Stmt::While(_)
            | Stmt::If(_)
            | Stmt::With(_)
            | Stmt::Match(_)
            | Stmt::Try(_) => self.compound_statement(statement)?,
            Stmt::Return(value) => node(
                "Return",
                [("value", self.expr_optional(value.value.as_deref())?)],
            ),
            Stmt::Delete(value) => {
                let mut targets = Vec::new();
                fn flatten<'a>(value: &'a Expr, targets: &mut Vec<&'a Expr>) {
                    if let Expr::Tuple(tuple) = value {
                        for item in &tuple.elts {
                            flatten(item, targets);
                        }
                    } else {
                        targets.push(value);
                    }
                }
                for target in &value.targets {
                    flatten(target, &mut targets);
                }
                node("Delete", [("targets", self.exprs(targets)?)])
            }
            Stmt::TypeAlias(value) => node(
                "TypeAlias",
                [
                    ("name", self.expr(&value.name)?),
                    (
                        "type_params",
                        self.type_params(value.type_params.as_deref())?,
                    ),
                    ("value", self.expr(&value.value)?),
                ],
            ),
            Stmt::Assign(value) => node(
                "Assign",
                [
                    ("targets", self.exprs(&value.targets)?),
                    ("value", self.expr(&value.value)?),
                    ("type_comment", self.comment(statement)),
                ],
            ),
            Stmt::AugAssign(value) => node(
                "AugAssign",
                [
                    ("target", self.expr(&value.target)?),
                    ("op", leaf(format!("{:?}", value.op))),
                    ("value", self.expr(&value.value)?),
                ],
            ),
            Stmt::AnnAssign(value) => node(
                "AnnAssign",
                [
                    ("target", self.expr(&value.target)?),
                    ("annotation", self.expr(&value.annotation)?),
                    ("value", self.expr_optional(value.value.as_deref())?),
                    ("simple", integer(u8::from(value.simple))),
                ],
            ),
            Stmt::Raise(value) => node(
                "Raise",
                [
                    ("exc", self.expr_optional(value.exc.as_deref())?),
                    ("cause", self.expr_optional(value.cause.as_deref())?),
                ],
            ),
            Stmt::Assert(value) => node(
                "Assert",
                [
                    ("test", self.expr(&value.test)?),
                    ("msg", self.expr_optional(value.msg.as_deref())?),
                ],
            ),
            Stmt::Import(value) => {
                if value.is_lazy {
                    return Err("Lazy imports have no pinned CPython AST schema".into());
                }
                node("Import", [("names", self.import_aliases(&value.names))])
            }
            Stmt::ImportFrom(value) => {
                if value.is_lazy {
                    return Err("Lazy imports have no pinned CPython AST schema".into());
                }
                node(
                    "ImportFrom",
                    [
                        (
                            "module",
                            optional_string(value.module.as_ref().map(|name| name.as_str())),
                        ),
                        ("names", self.import_aliases(&value.names)),
                        ("level", integer(value.level)),
                    ],
                )
            }
            Stmt::Global(value) => node(
                "Global",
                [(
                    "names",
                    Value::List(
                        value
                            .names
                            .iter()
                            .map(|name| string(name.to_string()))
                            .collect(),
                    ),
                )],
            ),
            Stmt::Nonlocal(value) => node(
                "Nonlocal",
                [(
                    "names",
                    Value::List(
                        value
                            .names
                            .iter()
                            .map(|name| string(name.to_string()))
                            .collect(),
                    ),
                )],
            ),
            Stmt::Expr(value) => node("Expr", [("value", self.expr(&value.value)?)]),
            Stmt::Pass(_) => leaf("Pass"),
            Stmt::Break(_) => leaf("Break"),
            Stmt::Continue(_) => leaf("Continue"),
            Stmt::IpyEscapeCommand(_) => {
                return Err("IPython escape statements have no CPython AST".into())
            }
        })
    }
    fn import_aliases(&self, aliases: &[ruff_python_ast::Alias]) -> Value {
        Value::List(
            aliases
                .iter()
                .map(|alias| {
                    tree::node(
                        "alias",
                        [
                            ("name", tree::string(alias.name.to_string())),
                            (
                                "asname",
                                tree::optional_string(
                                    alias.asname.as_ref().map(|name| name.as_str()),
                                ),
                            ),
                        ],
                    )
                })
                .collect(),
        )
    }
}
