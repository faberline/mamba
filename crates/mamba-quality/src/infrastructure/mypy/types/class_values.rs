//! Literal generic class objects instantiate their own binders against type targets.
use super::*;

impl Context {
    pub(super) fn class_object_instance(&self, expected: Type, actual: Type) -> Option<Type> {
        let instance = self.instance_type(actual)?;
        let actual_node = self.node(actual);
        let Some(Node::Class(id)) = actual_node.as_deref() else {
            return Some(instance);
        };
        let class = self.class(*id);
        if class.generic.is_empty() {
            return Some(instance);
        }
        let expected_node = self.node(expected);
        let Some(Node::AppliedClass {
            id: expected_id,
            value: false,
            ..
        }) = expected_node.as_deref()
        else {
            return Some(instance);
        };
        let arguments = self.class_arguments_for(instance, *expected_id)?;
        let template = self.applied_class(*expected_id, arguments, false);
        let solution = self.solve(&class.generic, &[(template, expected)]);
        (solution.failed.is_empty() && solution.invalid.is_empty())
            .then(|| self.substitute(instance, &solution.types))
    }
}
