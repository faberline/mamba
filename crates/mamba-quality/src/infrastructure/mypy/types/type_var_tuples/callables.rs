//! Variadic tuple parameters expand into fixed or homogeneous call arguments.
use super::super::super::signatures::{Kind, Parameter, Signature};
use super::*;

impl Context {
    pub(in crate::infrastructure::mypy) fn callable_needs_unpack(
        &self,
        signature: &Signature,
    ) -> bool {
        self.signature_parameter_spec(signature).is_some()
            || signature.parameters.iter().any(|parameter| {
                parameter
                    .ty
                    .is_some_and(|ty| self.unpack_target(ty).is_some())
            })
    }

    pub(in crate::infrastructure::mypy) fn expand_tuple_parameters(
        &self,
        parameters: Vec<Parameter>,
        anonymous: bool,
    ) -> Vec<Parameter> {
        let mut expanded = Vec::new();
        for parameter in parameters {
            let Some(target) = parameter.ty.and_then(|ty| self.unpack_target(ty)) else {
                expanded.push(parameter);
                continue;
            };
            match self.node(target).as_deref() {
                Some(Node::Tuple(items)) => {
                    expanded.extend(
                        self.expand_tuple_parameters(
                            items
                                .iter()
                                .map(|&ty| Parameter {
                                    name: String::new(),
                                    ty: Some(ty),
                                    kind: Kind::PositionalOnly,
                                    required: true,
                                })
                                .collect(),
                            anonymous,
                        ),
                    );
                }
                Some(Node::VariadicTuple(item)) => expanded.push(Parameter {
                    name: if anonymous {
                        String::new()
                    } else {
                        parameter.name.clone()
                    },
                    ty: Some(*item),
                    kind: Kind::VariadicPositional,
                    required: false,
                }),
                _ if target.is_any() => expanded.push(Parameter {
                    name: if anonymous {
                        String::new()
                    } else {
                        parameter.name.clone()
                    },
                    ty: Some(target),
                    kind: Kind::VariadicPositional,
                    required: false,
                }),
                _ => expanded.push(parameter),
            }
        }
        expanded
    }
}
