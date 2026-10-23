//! Nominal generic parameters use declared or member-inferred variance.
use super::*;

impl Context {
    pub(in crate::infrastructure::mypy) fn class_parameter_variance(
        &self,
        id: u32,
        index: usize,
    ) -> ParameterVariance {
        let class = self.class(id);
        let Some(node) = class.generic.get(index).and_then(|&ty| self.node(ty)) else {
            return ParameterVariance::Invariant;
        };
        let Node::Variable(variable) = node.as_ref() else {
            return ParameterVariance::Invariant;
        };
        if variable.variance != ParameterVariance::Inferred {
            return variable.variance;
        }
        if let Some(&variance) = self.classes.borrow().class_variances.get(&(id, index)) {
            return variance;
        }
        for candidate in [
            ParameterVariance::Covariant,
            ParameterVariance::Contravariant,
            ParameterVariance::Invariant,
        ] {
            // Recursive members compare under the current hypothesis.
            self.classes
                .borrow_mut()
                .class_variances
                .insert((id, index), candidate);
            let (covariant, contravariant) = self.infer_class_variance(id, index);
            let inferred = if covariant {
                ParameterVariance::Covariant
            } else if contravariant {
                ParameterVariance::Contravariant
            } else {
                ParameterVariance::Invariant
            };
            if candidate == inferred {
                return inferred;
            }
        }
        ParameterVariance::Invariant
    }

    fn infer_class_variance(&self, id: u32, index: usize) -> (bool, bool) {
        let class = self.class(id);
        let instance = self.applied_class(id, class.generic.clone(), false);
        let substitutions = BTreeMap::from([(class.generic[index], Type::Object)]);
        let mut covariant = true;
        let mut contravariant = true;
        for name in self.variance_members(id) {
            if matches!(
                name.as_str(),
                "__init__" | "__new__" | "__replace__" | "__mypy-replace"
            ) {
                continue;
            }
            let (ty, settable) = if let Some(field) = self.class_field(instance, &name) {
                let private =
                    name.starts_with('_') && !(name.starts_with("__") && name.ends_with("__"));
                (field.ty.unwrap_or(Type::Any), !field.readonly && !private)
            } else if let Some(signature) = self.class_method(instance, &name) {
                (self.variance_callable(&signature, instance), false)
            } else {
                continue;
            };
            let replaced = self.substitute(ty, &substitutions);
            covariant &= self.accepts(replaced, ty);
            if !self.accepts(ty, replaced) {
                contravariant = false;
                covariant &= !settable;
            }
        }
        for &base in &class.base_types {
            let replaced = self.substitute(base, &substitutions);
            covariant &= self.accepts(replaced, base);
            contravariant &= self.accepts(base, replaced);
        }
        (covariant, contravariant)
    }

    fn variance_callable(
        &self,
        signature: &super::super::signatures::Signature,
        instance: Type,
    ) -> Type {
        let mut signature = signature.clone();
        if signature.returns == Some(instance) {
            signature.returns = Some(Type::Any);
        }
        signature.overloads = signature
            .overloads
            .iter()
            .filter_map(|variant| {
                self.callable_signature(self.variance_callable(variant, instance))
            })
            .collect::<Vec<_>>()
            .into();
        self.callable_type(std::sync::Arc::new(signature))
    }
}
