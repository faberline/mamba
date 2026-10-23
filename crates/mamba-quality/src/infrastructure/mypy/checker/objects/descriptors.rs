//! Descriptor calls use temporary typed arguments and the common call binder.
use super::*;
use quality_syntax::python_ast::{Arguments, ExprCall, ExprContext, ExprName};
use signatures::{Kind, Signature};

const INSTANCE: &str = "\0descriptor-instance";
const OWNER: &str = "\0descriptor-owner";

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn check_descriptor_assignment(
        &mut self,
        target: &Expr,
        value: &Expr,
    ) -> bool {
        let Expr::Attribute(attribute) = target else {
            return false;
        };
        let Some(receiver) = self.infer(&attribute.value) else {
            return false;
        };
        if self.context.is_class_value(receiver) {
            return false;
        }
        let Some(descriptor) = self
            .context
            .descriptor_member(receiver, attribute.attr.as_str())
        else {
            return false;
        };
        let Some(signature) = self.context.class_method(descriptor, "__set__") else {
            return false;
        };
        let previous = self.descriptor_values([receiver, types::Type::Any]);
        let mut call = Self::descriptor_call(attribute);
        let key = scopes::Scopes::attribute_key(target);
        if signature.overloads.is_empty() {
            let signature = self.descriptor_positional_signature(descriptor, "__set__", &signature);
            let mut inferred_call = call.clone();
            inferred_call.arguments.args[1] = value.clone();
            let solution =
                self.scopes
                    .call_solution(&inferred_call, &signature, &self.environment(), None);
            self.bind_call(&call, &signature);
            let expected = signature.parameters.get(1).map(|parameter| {
                self.context.substitute_inferred_alias(
                    parameter.ty.unwrap_or(types::Type::Any),
                    &solution.types,
                )
            });
            let actual = expected.and_then(|expected| self.infer_expected(value, expected));
            if let Some((expected, actual)) = expected.zip(actual) {
                self.check_assignment_type(expected, actual, value);
            }
            if let Some(key) = key {
                self.flow.narrowed.remove(&key);
                if expected.is_none() {
                    self.flow.narrowed.insert(key, types::Type::Any);
                } else if let Some((expected, actual, read)) =
                    expected.zip(actual).and_then(|(expected, actual)| {
                        Some((
                            expected,
                            actual,
                            self.context.descriptor_read_type(descriptor, receiver)?,
                        ))
                    })
                {
                    if self.context.accepts(expected, read) && self.context.accepts(read, actual) {
                        self.flow.narrowed.insert(key, actual);
                    }
                }
            }
        } else {
            call.arguments.args[1] = value.clone();
            let resolution =
                self.scopes
                    .resolve_overload(&call, &signature, &self.environment(), None);
            self.check_overload_call(&call, &signature);
            if let Some(key) = key {
                self.flow.narrowed.remove(&key);
                if !resolution.matched && resolution.fallback.is_none() {
                    self.flow.narrowed.insert(key, types::Type::Any);
                }
            }
        }
        self.restore_descriptor_values(previous);
        true
    }

    pub(super) fn check_descriptor_access(
        &mut self,
        attribute: &ExprAttribute,
        receiver: types::Type,
    ) {
        let Some(descriptor) = self
            .context
            .descriptor_member(receiver, attribute.attr.as_str())
        else {
            return;
        };
        let Some(signature) = self.context.descriptor_get_signature(descriptor, receiver) else {
            return;
        };
        let signature = self.descriptor_positional_signature(descriptor, "__get__", &signature);
        let previous = self.descriptor_values(self.context.descriptor_arguments(receiver));
        self.bind_call(&Self::descriptor_call(attribute), &signature);
        self.restore_descriptor_values(previous);
    }

    fn descriptor_call(attribute: &ExprAttribute) -> ExprCall {
        let point =
            ruff_text_size::TextRange::at(attribute.start(), ruff_text_size::TextSize::new(1));
        ExprCall {
            node_index: Default::default(),
            range_start: attribute.start(),
            func: attribute.value.clone(),
            arguments: Arguments {
                range: attribute.range(),
                node_index: Default::default(),
                args: [INSTANCE, OWNER]
                    .into_iter()
                    .map(|name| {
                        Expr::Name(ExprName {
                            node_index: Default::default(),
                            range: point,
                            id: name.into(),
                            ctx: ExprContext::Load,
                        })
                    })
                    .collect(),
                keywords: Default::default(),
            },
        }
    }

    fn descriptor_values(&mut self, types: [types::Type; 2]) -> [Option<types::Type>; 2] {
        std::array::from_fn(|index| {
            self.flow
                .narrowed
                .insert([INSTANCE, OWNER][index].into(), types[index])
        })
    }

    fn restore_descriptor_values(&mut self, previous: [Option<types::Type>; 2]) {
        for (name, ty) in [INSTANCE, OWNER].into_iter().zip(previous) {
            if let Some(ty) = ty {
                self.flow.narrowed.insert(name.into(), ty);
            } else {
                self.flow.narrowed.remove(name);
            }
        }
    }

    fn descriptor_positional_signature(
        &self,
        descriptor: types::Type,
        name: &str,
        signature: &Signature,
    ) -> Signature {
        let mut signature = signature.clone();
        if let Some(owner) = self.context.class_member_owner(descriptor, name) {
            let class = self.context.class(owner);
            let namespace = &class.namespace;
            let module = namespace
                .split_once(':')
                .map_or(namespace.as_str(), |(_, module)| module);
            if format!("{module}.") != self.scopes.qualified_name("") {
                signature.definition_module = Some(module.into());
            }
        }
        signature.parameters = signature
            .parameters
            .iter()
            .map(|parameter| {
                let mut parameter = parameter.clone();
                if parameter.kind == Kind::PositionalOrKeyword {
                    parameter.kind = Kind::PositionalOnly;
                }
                parameter
            })
            .collect();
        signature
    }
    pub(in crate::infrastructure::mypy::checker) fn assigned_object_type(
        &self,
        expression: &Expr,
    ) -> Option<types::Type> {
        let Expr::Attribute(attribute) = expression else {
            return None;
        };
        self.context
            .class_write_type(self.infer(&attribute.value)?, attribute.attr.as_str())
            .or_else(|| {
                self.context
                    .class_method(self.infer(&attribute.value)?, attribute.attr.as_str())
                    .map(|signature| self.context.callable_type(signature))
            })
    }
}
