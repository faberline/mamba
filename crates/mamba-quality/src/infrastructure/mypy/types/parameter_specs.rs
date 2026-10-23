//! Parameter specifications preserve a callable's complete argument domain.
use super::super::signatures::{Kind, Parameter, Signature};
use super::*;
use mamba_language::application::quality_syntax::python_ast::ExprAttribute;

impl Context {
    pub(in crate::infrastructure::mypy) fn parameter_spec_callable_input(
        &self,
        input: Type,
        variable: Type,
    ) -> bool {
        match self.node(input).as_deref() {
            Some(Node::Callable(_)) => {
                let mut variables = Vec::new();
                self.collect_variables(input, &mut variables);
                variables.contains(&variable)
            }
            Some(Node::Container(_, items) | Node::Tuple(items) | Node::Union(items)) => items
                .iter()
                .any(|&item| self.parameter_spec_callable_input(item, variable)),
            _ => false,
        }
    }

    pub(super) fn accepts_parameters(
        &self,
        expected: &[Parameter],
        expected_ellipsis: bool,
        actual: &[Parameter],
        actual_ellipsis: bool,
    ) -> bool {
        if expected_ellipsis || actual_ellipsis {
            return true;
        }
        expected.len() == actual.len()
            && expected.iter().zip(actual).all(|(expected, actual)| {
                expected.kind == actual.kind
                    && expected.required == actual.required
                    && (expected.kind == Kind::PositionalOnly || expected.name == actual.name)
                    && self.accepts(
                        expected.ty.unwrap_or(Type::Any),
                        actual.ty.unwrap_or(Type::Any),
                    )
            })
    }

    pub(in crate::infrastructure::mypy) fn parameter_spec(
        &self,
        name: String,
        default: Option<Type>,
    ) -> Type {
        self.intern(Node::Variable(variables::Variable {
            id: self.variable_id(),
            name,
            bound: Type::Object,
            values: Vec::new(),
            default,
            variance: ParameterVariance::Invariant,
            parameter_spec: true,
            type_var_tuple: false,
        }))
    }

    pub(in crate::infrastructure::mypy) fn is_parameter_spec(&self, ty: Type) -> bool {
        self.node(ty).is_some_and(
            |node| matches!(node.as_ref(), Node::Variable(variable) if variable.parameter_spec),
        )
    }

    pub(in crate::infrastructure::mypy) fn parameter_spec_default(
        &self,
        expression: &Expr,
        variables: &VariableMap,
    ) -> Option<Type> {
        if let Some(variable) = self
            .annotation_with_variables(expression, variables)
            .filter(|&ty| self.is_parameter_spec(ty))
        {
            return Some(variable);
        }
        Some(
            self.callable_parameters(expression, variables)
                .map(|(parameters, ellipsis)| self.intern(Node::Parameters(parameters, ellipsis)))
                .unwrap_or(Type::Any),
        )
    }

    pub(super) fn parameter_spec_projection(
        &self,
        attribute: &ExprAttribute,
        variables: &VariableMap,
    ) -> Option<Type> {
        let variable = self.annotation_with_variables(&attribute.value, variables)?;
        if !self.is_parameter_spec(variable) {
            return None;
        }
        Some(self.intern(match attribute.attr.as_str() {
            "args" => Node::ParamSpecArgs(variable),
            "kwargs" => Node::ParamSpecKwargs(variable),
            _ => return None,
        }))
    }

    pub(super) fn callable_parameters(
        &self,
        expression: &Expr,
        variables: &VariableMap,
    ) -> Option<(Vec<Parameter>, bool)> {
        if expression.is_ellipsis_literal_expr() {
            return Some((
                [Kind::VariadicPositional, Kind::VariadicKeyword]
                    .into_iter()
                    .map(|kind| Parameter {
                        name: String::new(),
                        ty: Some(Type::Any),
                        kind,
                        required: false,
                    })
                    .collect(),
                true,
            ));
        }
        if let Expr::List(list) = expression {
            let parameters = list
                .elts
                .iter()
                .map(|parameter| {
                    let mut parameter = Parameter {
                        name: String::new(),
                        ty: Some(self.annotation_with_variables(parameter, variables)?),
                        kind: Kind::PositionalOnly,
                        required: true,
                    };
                    if let Some(target) = parameter.ty.and_then(|ty| self.unpack_target(ty)) {
                        parameter.kind = Kind::VariadicPositional;
                        parameter.required = false;
                        if let Some(Node::VariadicTuple(item)) = self.node(target).as_deref() {
                            parameter.ty = Some(*item);
                        } else {
                            parameter.name = "args".into();
                        }
                    }
                    Some(parameter)
                })
                .collect::<Option<Vec<_>>>()?;
            return Some((parameters, false));
        }
        let (mut prefix, variable) = if let Expr::Subscript(subscript) = expression {
            if self.typing_form(&subscript.value, variables).as_deref() != Some("Concatenate") {
                return None;
            }
            let Expr::Tuple(tuple) = subscript.slice.as_ref() else {
                return None;
            };
            let (last, prefix) = tuple.elts.split_last()?;
            let prefix = prefix
                .iter()
                .map(|parameter| {
                    Some(Parameter {
                        name: String::new(),
                        ty: Some(self.annotation_with_variables(parameter, variables)?),
                        kind: Kind::PositionalOnly,
                        required: true,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            if last.is_ellipsis_literal_expr() {
                let mut parameters = prefix;
                parameters.extend(self.callable_parameters(last, variables)?.0);
                return Some((parameters, false));
            }
            (prefix, self.annotation_with_variables(last, variables)?)
        } else {
            (
                Vec::new(),
                self.annotation_with_variables(expression, variables)?,
            )
        };
        if !self.is_parameter_spec(variable) {
            return None;
        }
        prefix.extend([
            Parameter {
                name: "args".into(),
                ty: Some(self.intern(Node::ParamSpecArgs(variable))),
                kind: Kind::VariadicPositional,
                required: false,
            },
            Parameter {
                name: "kwargs".into(),
                ty: Some(self.intern(Node::ParamSpecKwargs(variable))),
                kind: Kind::VariadicKeyword,
                required: false,
            },
        ]);
        Some((prefix, false))
    }

    pub(in crate::infrastructure::mypy) fn signature_parameter_spec(
        &self,
        signature: &Signature,
    ) -> Option<(usize, Type)> {
        signature
            .parameters
            .iter()
            .enumerate()
            .find_map(|(index, parameter)| {
                let node = self.node(parameter.ty?)?;
                match node.as_ref() {
                    Node::ParamSpecArgs(variable) if self.is_parameter_spec(*variable) => {
                        Some((index, *variable))
                    }
                    _ => None,
                }
            })
    }

    pub(super) fn substitute_parameters(
        &self,
        signature: &Signature,
        solutions: &BTreeMap<Type, Type>,
    ) -> (Vec<Parameter>, bool) {
        let mut parameters = Vec::new();
        let mut expanded = Vec::new();
        let mut ellipsis = signature.ellipsis;
        for parameter in signature.parameters.iter() {
            let projection = parameter
                .ty
                .and_then(|ty| self.node(ty))
                .and_then(|node| match node.as_ref() {
                    Node::ParamSpecArgs(variable) | Node::ParamSpecKwargs(variable) => {
                        Some(*variable)
                    }
                    _ => None,
                });
            if let Some(variable) = projection {
                if let Some(node) = solutions.get(&variable).and_then(|&ty| self.node(ty)) {
                    if let Node::Parameters(captured, captured_ellipsis) = node.as_ref() {
                        if !expanded.contains(&variable) {
                            parameters.extend(captured.iter().map(|parameter| Parameter {
                                ty: parameter.ty.map(|ty| self.substitute(ty, solutions)),
                                ..parameter.clone()
                            }));
                            expanded.push(variable);
                            ellipsis = *captured_ellipsis;
                        }
                        continue;
                    }
                }
                if solutions.get(&variable).is_some_and(|ty| ty.is_any()) {
                    parameters.push(Parameter {
                        name: String::new(),
                        ty: solutions.get(&variable).copied(),
                        ..parameter.clone()
                    });
                    ellipsis = true;
                    continue;
                }
            }
            parameters.push(Parameter {
                ty: parameter.ty.map(|ty| self.substitute(ty, solutions)),
                ..parameter.clone()
            });
        }
        (
            self.expand_tuple_parameters(parameters, signature.name.is_none()),
            ellipsis,
        )
    }
}
