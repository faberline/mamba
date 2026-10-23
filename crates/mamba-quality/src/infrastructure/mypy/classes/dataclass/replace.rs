//! Python 3.13 and 3.14 generate a replacement method for constructor fields.
use super::*;

impl Context {
    pub(super) fn dataclass_replace_method(
        &self,
        class: &mut Class,
        attributes: &[Attribute],
        variables: &Variables,
    ) {
        let instance = self.applied_class(
            self.class_id(class.instance).unwrap(),
            class.generic.clone(),
            false,
        );
        let mut signature = Signature::empty("__replace__".into(), instance);
        signature.constructor = false;
        signature.owner = Some(class.name.clone());
        signature.generic = class.generic.clone().into();
        signature.variables = variables.clone();
        let mut parameters = vec![Parameter {
            name: "self".into(),
            ty: Some(instance),
            kind: Kind::PositionalOrKeyword,
            required: true,
        }];
        for field in attributes.iter().filter(|field| field.in_init) {
            parameters.push(Parameter {
                name: field.name.clone(),
                ty: Some(field.ty),
                kind: Kind::KeywordOnly,
                required: field.init_var && field.required,
            });
        }
        signature.parameters = parameters.into();
        class.methods.insert(
            "__replace__".into(),
            Method {
                signature: Arc::new(signature),
                kind: MethodKind::Instance,
                setter: None,
                final_method: false,
                abstractness: Abstractness::Concrete,
            },
        );
    }
}
