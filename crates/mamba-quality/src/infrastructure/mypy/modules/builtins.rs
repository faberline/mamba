//! Imported builtin values reuse their embedded signatures and class identities.
use super::*;

impl Lookup {
    pub(super) fn builtin_member(&self, member: &str) -> Option<Export> {
        let context = &self.modules.context;
        let target = context.module_target(&self.namespace());
        if let Some(scalar) = Context::builtin_name(member) {
            return Some(Export {
                annotation_module: false,
                ty: Some(context.class(context.builtin_class(scalar)).object),
                callable: context.builtin_constructor(scalar, target),
                module: None,
                explicit: true,
                module_hidden: Some(false),
                alias: Some(scalar),
            });
        }
        let signature = context
            .collection_constructor(member, target)
            .or_else(|| context.standard_collection_function(member, target))
            .or_else(|| context.aggregation_function(member, target))
            .or_else(|| context.name_builtin_function(member, target));
        if let Some(signature) = signature {
            return Some(Export {
                annotation_module: false,
                ty: Some(context.callable_type(signature.clone())),
                callable: Some(signature),
                module: None,
                explicit: true,
                module_hidden: Some(false),
                alias: context.builtin_constructor_alias(member, target),
            });
        }
        let ty = context.runtime_builtin_value(member, target)?;
        let ty = context.native_source_value(ty, &self.namespace());
        Some(Export {
            annotation_module: false,
            ty: Some(ty),
            callable: context.callable_signature_for(ty, target),
            module: None,
            explicit: true,
            module_hidden: Some(false),
            alias: context
                .is_class_value(ty)
                .then(|| context.annotation_value(ty)),
        })
    }
}
