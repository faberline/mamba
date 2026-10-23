//! CPython arguments, keyword, arg, and version-specific type parameter fields.
use super::{
    ast::{Builder, Result},
    tree::{self, Value},
};
use ruff_python_ast::{Arguments, Parameter, Parameters, TypeParam, TypeParams};
use ruff_text_size::Ranged;

impl Builder<'_> {
    pub fn arguments(
        &self,
        arguments: Option<&Arguments>,
    ) -> std::result::Result<(Value, Value), String> {
        let Some(arguments) = arguments else {
            return Ok((Value::List(Vec::new()), Value::List(Vec::new())));
        };
        Ok((
            self.exprs(arguments.args.iter())?,
            Value::List(
                arguments
                    .keywords
                    .iter()
                    .map(|v| {
                        Ok(tree::node(
                            "keyword",
                            [
                                (
                                    "arg",
                                    tree::optional_string(v.arg.as_ref().map(|name| name.as_str())),
                                ),
                                ("value", self.expr(&v.value)?),
                            ],
                        ))
                    })
                    .collect::<std::result::Result<Vec<_>, String>>()?,
            ),
        ))
    }
    fn parameter(&self, parameter: &Parameter) -> Result {
        let comment = self
            .arguments
            .get(&parameter.start().to_u32())
            .and_then(|key| self.comments.get(key));
        Ok(tree::node(
            "arg",
            [
                ("arg", tree::string(parameter.name.to_string())),
                (
                    "annotation",
                    self.expr_optional(parameter.annotation.as_deref())?,
                ),
                (
                    "type_comment",
                    comment.map_or_else(tree::none, |value| tree::string(value.clone())),
                ),
            ],
        ))
    }
    pub fn parameters(&self, parameters: Option<&Parameters>) -> Result {
        let Some(p) = parameters else {
            return Ok(tree::node(
                "arguments",
                [
                    ("posonlyargs", Value::List(Vec::new())),
                    ("args", Value::List(Vec::new())),
                    ("vararg", tree::none()),
                    ("kwonlyargs", Value::List(Vec::new())),
                    ("kw_defaults", Value::List(Vec::new())),
                    ("kwarg", tree::none()),
                    ("defaults", Value::List(Vec::new())),
                ],
            ));
        };
        let args = |values: &[ruff_python_ast::ParameterWithDefault]| {
            values
                .iter()
                .map(|p| self.parameter(&p.parameter))
                .collect::<std::result::Result<Vec<_>, String>>()
                .map(Value::List)
        };
        Ok(tree::node(
            "arguments",
            [
                ("posonlyargs", args(&p.posonlyargs)?),
                ("args", args(&p.args)?),
                (
                    "vararg",
                    p.vararg
                        .as_deref()
                        .map_or_else(|| Ok(tree::none()), |p| self.parameter(p))?,
                ),
                ("kwonlyargs", args(&p.kwonlyargs)?),
                (
                    "kw_defaults",
                    Value::List(
                        p.kwonlyargs
                            .iter()
                            .map(|p| self.expr_optional(p.default.as_deref()))
                            .collect::<std::result::Result<Vec<_>, _>>()?,
                    ),
                ),
                (
                    "kwarg",
                    p.kwarg
                        .as_deref()
                        .map_or_else(|| Ok(tree::none()), |p| self.parameter(p))?,
                ),
                (
                    "defaults",
                    self.exprs(
                        p.posonlyargs
                            .iter()
                            .chain(&p.args)
                            .filter_map(|p| p.default.as_deref()),
                    )?,
                ),
            ],
        ))
    }
    pub fn type_params(&self, parameters: Option<&TypeParams>) -> Result {
        let Some(parameters) = parameters else {
            return Ok(Value::List(Vec::new()));
        };
        let mut values = Vec::new();
        for parameter in &parameters.type_params {
            let (name, fields, default) = match parameter {
                TypeParam::TypeVar(v) => (
                    "TypeVar",
                    vec![
                        ("name", tree::string(v.name.to_string())),
                        ("bound", self.expr_optional(v.bound.as_deref())?),
                    ],
                    v.default.as_deref(),
                ),
                TypeParam::TypeVarTuple(v) => (
                    "TypeVarTuple",
                    vec![("name", tree::string(v.name.to_string()))],
                    v.default.as_deref(),
                ),
                TypeParam::ParamSpec(v) => (
                    "ParamSpec",
                    vec![("name", tree::string(v.name.to_string()))],
                    v.default.as_deref(),
                ),
            };
            let mut fields: std::collections::BTreeMap<_, _> = fields.into_iter().collect();
            if self.document.target
                != mamba_language::application::quality_syntax::PythonTarget::Py312
            {
                fields.insert("default_value", self.expr_optional(default)?);
            }
            values.push(Value::Node(tree::Node {
                name: name.into(),
                fields,
            }));
        }
        Ok(Value::List(values))
    }
}
