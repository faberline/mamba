//! Standard conversion overloads retain the factory's generic result type.
use super::super::super::types::{Container, Node};
use super::*;

impl Context {
    pub(super) fn dataclass_conversion_function(&self, name: &str) -> Arc<Signature> {
        if let Some(signature) = self
            .classes
            .borrow()
            .dataclass_converters
            .get(name)
            .cloned()
        {
            return signature;
        }
        let instance = self.dataclass_instance_type();
        let dictionary = name == "asdict";
        let result = if dictionary {
            self.intern(Node::Container(Container::Dict, vec![Type::Str, Type::Any]))
        } else {
            self.intern(Node::VariadicTuple(Type::Any))
        };
        let mut plain = Signature::empty(name.into(), result);
        plain.constructor = false;
        plain.definition_module = Some("dataclasses".into());
        plain.parameters = vec![Parameter {
            name: "obj".into(),
            ty: Some(instance),
            kind: Kind::PositionalOrKeyword,
            required: true,
        }]
        .into();
        let variable = self.variable("_T".into(), Type::Object, Vec::new(), None);
        let item = if dictionary {
            self.intern(Node::Tuple(vec![Type::Str, Type::Any]))
        } else {
            Type::Any
        };
        let input = self.intern(Node::Container(Container::List, vec![item]));
        let mut factory = Signature::empty(String::new(), variable);
        factory.name = None;
        factory.constructor = false;
        factory.parameters = vec![Parameter {
            name: String::new(),
            ty: Some(input),
            kind: Kind::PositionalOnly,
            required: true,
        }]
        .into();
        let mut custom = plain.clone();
        custom.generic = vec![variable].into();
        custom.returns = Some(variable);
        custom.parameters = plain
            .parameters
            .iter()
            .cloned()
            .chain([Parameter {
                name: if dictionary {
                    "dict_factory"
                } else {
                    "tuple_factory"
                }
                .into(),
                ty: Some(self.callable_type(Arc::new(factory))),
                kind: Kind::KeywordOnly,
                required: true,
            }])
            .collect::<Vec<_>>()
            .into();
        let mut signature = plain.clone();
        signature.overloads = vec![Arc::new(plain), Arc::new(custom)].into();
        let signature = Arc::new(signature);
        self.classes
            .borrow_mut()
            .dataclass_converters
            .insert(name.into(), signature.clone());
        signature
    }
}
