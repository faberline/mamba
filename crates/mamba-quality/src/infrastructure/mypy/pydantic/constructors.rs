//! Model constructors share the ordinary generic inference and class identity.
use super::Model;
use crate::infrastructure::mypy::{
    signatures::{Kind, Parameter, Signature},
    types::{Context, Type},
};

impl Context {
    pub(in crate::infrastructure::mypy) fn pydantic_declared_initializer(
        &self,
        receiver: Type,
    ) -> bool {
        self.class_id(receiver).is_some_and(|id| {
            let class = self.class(id);
            class.namespace.contains(':') && class.methods.contains_key("__init__")
        })
    }

    pub(in crate::infrastructure::mypy) fn pydantic_constructor(
        &self,
        receiver: Type,
        instance: Type,
        model: &Model,
    ) -> Signature {
        let id = self.class_id(receiver).expect("model has a class identity");
        let solutions = self.class_solutions(receiver, id);
        let mut signature = Signature::empty(model.name.clone(), instance);
        let mut parameters: Vec<_> = model
            .init_fields()
            .map(|field| Parameter {
                name: field.argument_name(!model.uses_aliases()).into(),
                ty: field.ty.map(|ty| self.substitute(ty, &solutions)),
                kind: if model.is_root_model && field.name == "root" {
                    Kind::PositionalOrKeyword
                } else {
                    Kind::KeywordOnly
                },
                required: field.required && !model.optional_init_fields(),
            })
            .collect();
        if model.allows_extra() {
            parameters.push(Parameter {
                name: "kwargs".into(),
                ty: Some(Type::Any),
                kind: Kind::VariadicKeyword,
                required: false,
            });
        }
        signature.parameters = parameters.into();
        signature
    }
}
