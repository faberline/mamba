//! Generated constructors retain field names, defaults, and keyword-only order.
use super::*;

impl Context {
    pub(super) fn dataclass_constructor(
        &self,
        class: &mut Class,
        attributes: &[Attribute],
        variables: &Variables,
    ) {
        let mut parameters = vec![Parameter {
            name: "self".into(),
            ty: Some(class.instance),
            kind: Kind::PositionalOrKeyword,
            required: true,
        }];
        let mut fields = attributes
            .iter()
            .filter(|attribute| attribute.in_init)
            .collect::<Vec<_>>();
        fields.sort_by_key(|attribute| attribute.keyword_only);
        for attribute in fields {
            parameters.push(Parameter {
                name: attribute.alias.as_ref().unwrap_or(&attribute.name).clone(),
                ty: Some(attribute.ty),
                kind: if attribute.keyword_only {
                    Kind::KeywordOnly
                } else {
                    Kind::PositionalOrKeyword
                },
                required: attribute.required,
            });
        }
        let mut signature = Signature::empty("__init__".into(), Type::None);
        signature.owner = Some(class.name.clone());
        signature.parameters = parameters.into();
        signature.variables = variables.clone();
        class.methods.insert(
            "__init__".into(),
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
