//! Template types use pinned declarations and never run the original library.
use super::*;
use mamba_language::application::quality_syntax::PythonTarget;
use serde_json::Value;
use std::{sync::Arc, sync::OnceLock};
mod declarations;

fn data() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    &DATA.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../assets/stdlib/template-library.json"
        ))
        .expect("locked template library declarations")
    })["targets"]["3.14"]
}

impl Context {
    pub(super) fn template_member_receiver(&self, ty: Type, name: &str) -> Type {
        if name == "__new__"
            && matches!(self.node(ty).as_deref(), Some(Node::Class(_)))
            && self
                .class_id(ty)
                .is_some_and(|id| self.class(id).namespace == "string.templatelib.Interpolation")
        {
            let id = self.class_id(ty).unwrap();
            self.applied_class(id, self.effective_class_arguments(id, Vec::new()), true)
        } else {
            ty
        }
    }

    pub(in crate::infrastructure::mypy) fn template_instance(&self, target: PythonTarget) -> Type {
        if target != PythonTarget::Py314 {
            return Type::Any;
        }
        self.template_class("Template")
    }

    pub(in crate::infrastructure::mypy) fn template_class(&self, name: &str) -> Type {
        self.load_template_library();
        let fullname = format!("string.templatelib.{name}");
        self.class(self.declare_class(&fullname, name, 0)).instance
    }

    pub(in crate::infrastructure::mypy) fn template_function(
        &self,
        name: &str,
    ) -> Option<Arc<super::super::signatures::Signature>> {
        self.load_template_library();
        self.classes.borrow().template_functions.get(name).cloned()
    }

    pub(in crate::infrastructure::mypy) fn is_template_instance(&self, ty: Type) -> bool {
        !self.is_class_value(ty)
            && self
                .class_id(ty)
                .is_some_and(|id| self.class(id).namespace == "string.templatelib.Template")
    }
}
