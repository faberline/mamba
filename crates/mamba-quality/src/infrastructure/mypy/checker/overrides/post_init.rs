//! Post-init methods accept the complete positional InitVar field domain.
use super::super::super::classes::{Abstractness, Class};
use super::*;
use signatures::Parameter;
use std::sync::Arc;

impl Checker<'_> {
    pub(super) fn check_dataclass_post_init(
        &mut self,
        function: &StmtFunctionDef,
        class: &Class,
        method: &Method,
    ) {
        if function.name.as_str() != "__post_init__" {
            return;
        }
        let Some(definition) = &class.dataclass else {
            return;
        };
        let mut fields: Vec<_> = definition
            .attributes
            .iter()
            .filter(|attribute| attribute.init_var)
            .collect();
        fields.sort_by_key(|attribute| attribute.keyword_only);
        let mut parameters = vec![Parameter {
            name: "self".into(),
            ty: Some(class.instance),
            kind: Kind::PositionalOrKeyword,
            required: true,
        }];
        parameters.extend(fields.into_iter().map(|attribute| Parameter {
            name: attribute.name.clone(),
            ty: Some(attribute.ty),
            kind: Kind::PositionalOrKeyword,
            required: true,
        }));
        let mut signature = Signature::empty("__post_init__".into(), Type::None);
        signature.constructor = false;
        signature.parameters = parameters.into();
        signature.variables = method.signature.variables.clone();
        let original = Method {
            signature: Arc::new(signature),
            kind: MethodKind::Instance,
            setter: None,
            final_method: false,
            abstractness: Abstractness::Concrete,
        };
        self.check_override_types(function, method, &original, "dataclass", &mut true);
    }
}
