//! The locked plugin's construction method uses field names and typed returns.
use super::Model;
use crate::infrastructure::mypy::{
    signatures::{Kind, Parameter, Signature},
    types::{Container, Context, Node, Type},
};

impl Context {
    pub(in crate::infrastructure::mypy) fn pydantic_construct_method(
        &self,
        receiver: Type,
        model: &Model,
    ) -> Signature {
        let id = self.class_id(receiver).expect("model has a class identity");
        let returns = self
            .instance_type(receiver)
            .expect("model has an instance type");
        let solutions = self.class_solutions(receiver, id);
        let mut signature = Signature::empty("model_construct".into(), returns);
        signature.owner = Some(model.name.clone());
        if matches!(self.node(receiver).as_deref(), Some(Node::Class(_))) {
            signature.generic = self.class(id).generic.clone().into();
        }
        let mut parameters = Vec::new();
        if !self.is_class_value(receiver) {
            parameters.push(Parameter {
                name: "_cls".into(),
                ty: Some(self.class_value_type(receiver)),
                kind: Kind::PositionalOrKeyword,
                required: true,
            });
        }
        let fields_set = self.intern(Node::Container(Container::Set, vec![Type::Str]));
        parameters.push(Parameter {
            name: "_fields_set".into(),
            ty: Some(self.union(vec![fields_set, Type::None])),
            kind: Kind::PositionalOrKeyword,
            required: false,
        });
        parameters.extend(model.fields.iter().map(|field| Parameter {
            name: field.name.clone(),
            ty: field.ty.map(|ty| self.substitute(ty, &solutions)),
            kind: Kind::KeywordOnly,
            required: field.required,
        }));
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
