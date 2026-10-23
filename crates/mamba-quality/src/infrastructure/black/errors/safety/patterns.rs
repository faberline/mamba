//! CPython match pattern node fields.
use super::{
    ast::{Builder, Result},
    tree::{self, Value},
};
use ruff_python_ast::{Pattern, Singleton};
impl Builder<'_> {
    pub fn pattern(&self, pattern: &Pattern) -> Result {
        use tree::{node, optional_string, string};
        let patterns = |values: &[Pattern]| {
            values
                .iter()
                .map(|v| self.pattern(v))
                .collect::<std::result::Result<Vec<_>, String>>()
                .map(Value::List)
        };
        Ok(match pattern {
            Pattern::MatchValue(v) => node("MatchValue", [("value", self.expr(&v.value)?)]),
            Pattern::MatchSingleton(v) => node(
                "MatchSingleton",
                [(
                    "value",
                    match v.value {
                        Singleton::None => tree::none(),
                        Singleton::True => Value::Scalar("True".into(), "bool"),
                        Singleton::False => Value::Scalar("False".into(), "bool"),
                    },
                )],
            ),
            Pattern::MatchSequence(v) => {
                node("MatchSequence", [("patterns", patterns(&v.patterns)?)])
            }
            Pattern::MatchMapping(v) => node(
                "MatchMapping",
                [
                    ("keys", self.exprs(&v.keys)?),
                    ("patterns", patterns(&v.patterns)?),
                    (
                        "rest",
                        optional_string(v.rest.as_ref().map(|name| name.as_str())),
                    ),
                ],
            ),
            Pattern::MatchClass(v) => node(
                "MatchClass",
                [
                    ("cls", self.expr(&v.cls)?),
                    ("patterns", patterns(&v.arguments.patterns)?),
                    (
                        "kwd_attrs",
                        Value::List(
                            v.arguments
                                .keywords
                                .iter()
                                .map(|keyword| string(keyword.attr.to_string()))
                                .collect(),
                        ),
                    ),
                    (
                        "kwd_patterns",
                        Value::List(
                            v.arguments
                                .keywords
                                .iter()
                                .map(|keyword| self.pattern(&keyword.pattern))
                                .collect::<std::result::Result<Vec<_>, _>>()?,
                        ),
                    ),
                ],
            ),
            Pattern::MatchStar(v) => node(
                "MatchStar",
                [(
                    "name",
                    optional_string(v.name.as_ref().map(|name| name.as_str())),
                )],
            ),
            Pattern::MatchAs(v) => node(
                "MatchAs",
                [
                    (
                        "pattern",
                        v.pattern
                            .as_deref()
                            .map_or_else(|| Ok(tree::none()), |v| self.pattern(v))?,
                    ),
                    (
                        "name",
                        optional_string(v.name.as_ref().map(|name| name.as_str())),
                    ),
                ],
            ),
            Pattern::MatchOr(v) => node("MatchOr", [("patterns", patterns(&v.patterns)?)]),
        })
    }
}
