//! Loaded source modules share delayed class stores without executing code.
use super::super::{build_attributes::Frame, callables::Target, classes::Class, values::Value};
use mamba_language::application::quality_syntax::python_ast::ExprAttribute;
use std::rc::Rc;

pub(super) fn import_failures(
    index: &crate::infrastructure::project::ProjectIndex<'_>,
    workspace: &crate::domain::source::QualityWorkspace,
    target: mamba_language::application::quality_syntax::PythonTarget,
) -> std::collections::BTreeMap<(usize, String), String> {
    index
        .modules
        .iter()
        .enumerate()
        .filter_map(|(id, module)| {
            if module.document.is_some()
                || index.names.get(&(module.project, module.name.clone())) != Some(&id)
            {
                return None;
            }
            super::super::syntax_errors::import_failure(
                &workspace.sources[id],
                &module.name,
                target,
            )
            .map(|error| ((module.project, module.name.clone()), error))
        })
        .collect()
}

impl super::Modules {
    pub(in super::super) fn namespace(&self, project: usize, module: &str) -> bool {
        !self.source_indices.contains_key(&(project, module.into()))
            && self.extensions.namespace(module)
    }

    pub(in super::super) fn function(&self, name: &str) -> bool {
        self.functions.contains(name)
    }
    pub(in super::super) fn import_failure(&self, project: usize, module: &str) -> Option<&str> {
        self.import_failures
            .get(&(project, module.into()))
            .map(String::as_str)
    }
    pub(in super::super) fn builtin(&self, name: &str) -> bool {
        self.builtins.contains(name)
    }

    pub(in super::super) fn target(
        &self,
    ) -> mamba_language::application::quality_syntax::PythonTarget {
        self.target
    }

    pub(super) fn stored_class(&self, target: Target) -> Target {
        if let Target::Class(class) = target {
            let mut model = (*class).clone();
            self.apply_class_stores(&mut model);
            Target::Class(Rc::new(model))
        } else {
            target
        }
    }

    pub(in super::super) fn record_class_store(
        &self,
        class: &Class,
        attribute: &ExprAttribute,
        instance: bool,
        value: Value,
        frame: &Frame,
    ) {
        self.class_stores
            .borrow_mut()
            .record(class, attribute, instance, value, frame);
    }

    pub(in super::super) fn apply_class_stores(&self, class: &mut Class) {
        self.class_stores.borrow().apply(class);
    }
}
