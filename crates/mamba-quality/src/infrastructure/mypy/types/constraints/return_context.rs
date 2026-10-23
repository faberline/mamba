//! A complete nominal return template may be mapped to its real dict context owner.
use super::*;
use mamba_language::application::quality_syntax::PythonTarget;

impl Context {
    pub(super) fn superclass_return_context_constraints(
        &self,
        template: Type,
        target: Type,
        bounds: &mut BTreeMap<Type, Bounds>,
    ) -> bool {
        let projected = (|| {
            if !matches!(
                self.node(target).as_deref(),
                Some(Node::Container(Container::Dict, _))
            ) {
                return None;
            }
            let root = self.class_id(template)?;
            let namespace = self.class(root).namespace.clone();
            let version = self
                .classes
                .borrow()
                .module_targets
                .get(&namespace)
                .copied()?;
            let slot = match version {
                PythonTarget::Py312 => 0,
                PythonTarget::Py313 => 1,
                PythonTarget::Py314 => 2,
            };
            // Builtin class identities are shared. Do not infer across loaded target libraries.
            if self
                .classes
                .borrow()
                .builtin_value_libraries
                .iter()
                .enumerate()
                .any(|(index, &loaded)| loaded != (index == slot))
            {
                return None;
            }
            let context = self.nominal_dict_instance(target, version)?;
            let owner = self.class_id(context)?;
            let arguments = self.return_superclass_arguments(template, owner, version)?;
            Some((self.applied_class(owner, arguments, false), context))
        })();
        let Some((template, target)) = projected else {
            return false;
        };
        self.constraints(template, target, Variance::Covariant, true, bounds);
        true
    }
}
