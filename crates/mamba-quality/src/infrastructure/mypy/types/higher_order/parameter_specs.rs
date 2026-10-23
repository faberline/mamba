//! Generic callbacks preserve free variables captured by a parameter specification.
use super::super::super::signatures::Signature;
use super::*;

impl Context {
    pub(super) fn rebind_parameter_spec_callback(
        &self,
        child: &Signature,
        parent: &Signature,
    ) -> Option<Signature> {
        self.signature_parameter_spec(parent)?;
        let source = child.returns?;
        let target = parent.returns?;
        if !child.generic.contains(&source) {
            return None;
        }
        let (Some(source_node), Some(target_node)) = (self.node(source), self.node(target)) else {
            return None;
        };
        let (Node::Variable(source_variable), Node::Variable(target_variable)) =
            (source_node.as_ref(), target_node.as_ref())
        else {
            return None;
        };
        if source_variable.parameter_spec
            || target_variable.parameter_spec
            || source_variable.bound != target_variable.bound
            || source_variable.values != target_variable.values
        {
            return None;
        }
        let mut signature = self.substitute_callable(child, &BTreeMap::from([(source, target)]));
        let mut generic = signature.generic.to_vec();
        if !generic.contains(&target) {
            generic.push(target);
        }
        signature.generic = generic.into();
        Some(signature)
    }

    pub(super) fn align_callable_variables(
        &self,
        parent: &Signature,
        child: &Signature,
    ) -> Option<Signature> {
        if parent.generic.len() != child.generic.len() {
            return None;
        }
        let mut solutions = BTreeMap::new();
        for (&parent, &child) in parent.generic.iter().zip(child.generic.iter()) {
            let (Some(parent_node), Some(child_node)) = (self.node(parent), self.node(child))
            else {
                return None;
            };
            let (Node::Variable(parent_variable), Node::Variable(child_variable)) =
                (parent_node.as_ref(), child_node.as_ref())
            else {
                return None;
            };
            if parent_variable.parameter_spec != child_variable.parameter_spec
                || !self.accepts(child_variable.bound, parent_variable.bound)
                || parent_variable.values != child_variable.values
            {
                return None;
            }
            solutions.insert(child, parent);
        }
        Some(self.substitute_callable(child, &solutions))
    }
}
