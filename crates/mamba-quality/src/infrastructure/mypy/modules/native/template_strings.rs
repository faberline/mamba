//! Template symbols follow the standard library's Python 3.14 availability.
use super::*;

impl Lookup {
    pub(super) fn template_member(&self, member: &str) -> Option<Export> {
        if !matches!(member, "Template" | "Interpolation" | "convert") {
            return None;
        }
        let context = &self.modules.context;
        let target = context.module_target(&self.namespace());
        if target != PythonTarget::Py314 {
            return Some(Export {
                annotation_module: false,
                ty: Some(Type::Any),
                callable: None,
                module: None,
                explicit: true,
                module_hidden: Some(false),
                alias: (member != "convert").then_some(Type::Any),
            });
        }
        if member == "convert" {
            let signature = context.template_function(member)?;
            return Some(Export {
                annotation_module: false,
                ty: Some(context.callable_type(signature.clone())),
                callable: Some(signature),
                module: None,
                explicit: true,
                module_hidden: Some(false),
                alias: None,
            });
        }
        let instance = context.template_class(member);
        let ty = context.class_value_type(instance);
        Some(Export {
            annotation_module: false,
            ty: Some(ty),
            callable: context.class_constructor(ty),
            module: None,
            explicit: true,
            module_hidden: Some(false),
            alias: Some(instance),
        })
    }
}
