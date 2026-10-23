//! Constructor values bind only class methods from the locked builtin class graph.
use super::*;
use crate::infrastructure::mypy::classes::MethodKind;

pub(super) fn lookup(
    context: &Context,
    class: &str,
    name: &str,
    target: PythonTarget,
) -> Option<Arc<Signature>> {
    let instance = context.runtime_builtin_class_instance(class, target)?;
    let receiver = context.class_value_type(instance);
    if context.class_method_kind(receiver, name) != Some(MethodKind::Class) {
        return None;
    }
    context.class_method_for(receiver, name, target)
}
