//! Standard system values use embedded type data and need no Python runtime.
use super::*;

impl Lookup {
    pub(super) fn system_member(&self, member: &str) -> Option<Export> {
        if !matches!(member, "version_info" | "_version_info") {
            return None;
        }
        let context = &self.modules.context;
        let instance = context.system_version_info();
        let (ty, alias) = match member {
            "version_info" => (instance, None),
            "_version_info" => (
                context.class(context.class_id(instance)?).object,
                Some(instance),
            ),
            _ => return None,
        };
        Some(Export {
            annotation_module: false,
            ty: Some(ty),
            callable: None,
            module: None,
            explicit: true,
            module_hidden: Some(false),
            alias,
        })
    }
}
