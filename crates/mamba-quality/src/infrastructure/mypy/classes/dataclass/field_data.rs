//! Generated field dictionaries and replacement values use native standard types.
use super::super::super::types::{Container, Node};
use super::*;

impl Context {
    pub(in crate::infrastructure::mypy) fn dataclass_fields_type(&self) -> Type {
        self.intern(Node::Container(
            Container::Dict,
            vec![Type::Str, self.dataclass_field_type()],
        ))
    }

    pub(in crate::infrastructure::mypy) fn dataclass_field_type(&self) -> Type {
        let id = self.declare_class("dataclasses", "Field", 0);
        if self.class(id).generic.is_empty() {
            let mut class = (*self.class(id)).clone();
            class.generic = vec![self.variable("_T".into(), Type::Object, Vec::new(), None)];
            for (name, ty) in [
                ("name", Type::Str),
                ("repr", Type::Bool),
                ("hash", self.union(vec![Type::Bool, Type::None])),
                ("init", Type::Bool),
                ("compare", Type::Bool),
            ] {
                class.fields.insert(
                    name.into(),
                    Field {
                        ty: Some(ty),
                        needs_annotation: false,
                        class_access: false,
                        readonly: false,
                        final_attribute: false,
                        class_var: false,
                        abstract_field: false,
                    },
                );
            }
            self.replace_class(id, class);
        }
        self.applied_class(id, vec![Type::Any], false)
    }

    pub(in crate::infrastructure::mypy) fn dataclass_replace_function(&self) -> Arc<Signature> {
        if let Some(signature) = self.classes.borrow().dataclass_replace_function.clone() {
            return signature;
        }
        let bound = self.dataclass_instance_type();
        let variable = self.variable("_DataclassT".into(), bound, Vec::new(), None);
        let mut signature = Signature::empty("replace".into(), variable);
        signature.constructor = false;
        signature.generic = vec![variable].into();
        signature.parameters = vec![
            Parameter {
                name: String::new(),
                ty: Some(variable),
                kind: Kind::PositionalOnly,
                required: true,
            },
            Parameter {
                name: "changes".into(),
                ty: Some(Type::Any),
                kind: Kind::VariadicKeyword,
                required: false,
            },
        ]
        .into();
        let signature = Arc::new(signature);
        self.classes.borrow_mut().dataclass_replace_function = Some(signature.clone());
        signature
    }

    pub(super) fn dataclass_instance_type(&self) -> Type {
        let id = self.declare_class("_typeshed", "DataclassInstance", 0);
        let mut class = (*self.class(id)).clone();
        class.protocol = true;
        class.fields.insert(
            "__dataclass_fields__".into(),
            Field {
                ty: Some(self.dataclass_fields_type()),
                needs_annotation: false,
                class_access: true,
                readonly: false,
                final_attribute: false,
                class_var: true,
                abstract_field: false,
            },
        );
        self.replace_class(id, class);
        self.class(id).instance
    }

    pub(in crate::infrastructure::mypy) fn dataclass_function(
        &self,
        form: &str,
    ) -> Option<Arc<Signature>> {
        match form {
            "replace" => Some(self.dataclass_replace_function()),
            "fields" => Some(self.dataclass_fields_function()),
            "asdict" | "astuple" => Some(self.dataclass_conversion_function(form)),
            _ => None,
        }
    }

    fn dataclass_fields_function(&self) -> Arc<Signature> {
        if let Some(signature) = self.classes.borrow().dataclass_fields_function.clone() {
            return signature;
        }
        let instance = self.dataclass_instance_type();
        let class = self.intern(Node::Container(Container::TypeObject, vec![instance]));
        let parameter = self.union(vec![instance, class]);
        let returns = self.intern(Node::VariadicTuple(self.dataclass_field_type()));
        let mut signature = Signature::empty("fields".into(), returns);
        signature.constructor = false;
        signature.definition_module = Some("dataclasses".into());
        signature.parameters = vec![Parameter {
            name: "class_or_instance".into(),
            ty: Some(parameter),
            kind: Kind::PositionalOrKeyword,
            required: true,
        }]
        .into();
        let signature = Arc::new(signature);
        self.classes.borrow_mut().dataclass_fields_function = Some(signature.clone());
        signature
    }
}
