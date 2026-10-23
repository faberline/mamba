//! Embedded collection allocation preserves original class binders.
use super::*;
use crate::infrastructure::mypy::signatures::Signature;
use mamba_language::application::quality_syntax::PythonTarget;
use std::sync::Arc;

impl Context {
    pub(super) fn collection_class_constructor(
        &self,
        ty: Type,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        let class = self.class(self.class_id(ty)?);
        if !matches!(
            class.namespace.as_str(),
            "builtins.reversed" | "builtins.enumerate" | "builtins.range" | "builtins.zip"
        ) {
            return None;
        }
        let signature = self
            .collection_constructor(&class.name, target)
            .or_else(|| self.aggregation_function(&class.name, target))?;
        if matches!(self.node(ty).as_deref(), Some(Node::Class(_))) {
            return Some(signature);
        }
        let arguments = self.class_arguments(ty);
        if arguments.is_empty() {
            return Some(signature);
        }
        let solutions = class.generic.iter().copied().zip(arguments).collect();
        Some(Arc::new(self.substitute_callable(&signature, &solutions)))
    }

    pub(in crate::infrastructure::mypy) fn collection_call_solution(
        &self,
        signature: &Signature,
        arguments: &[(Type, Type)],
        expected_return: Option<Type>,
    ) -> constraints::Solution {
        if signature.definition_module.as_deref() == Some("builtins")
            && signature.name.as_deref() == Some("enumerate")
        {
            if let Some((returned, expected)) = signature.returns.zip(expected_return) {
                if let Some(id) = self.class_id(returned) {
                    if let Some(items) = self.iterable_item(expected).and_then(|ty| self.node(ty)) {
                        if let Node::Tuple(items) = items.as_ref() {
                            if items.len() == 2 && items[0] == Type::Int {
                                let expected = self.applied_class(id, vec![items[1]], false);
                                return self.solve_call(
                                    &signature.generic,
                                    arguments,
                                    Some((returned, expected)),
                                );
                            }
                        }
                    }
                }
            }
        }
        if signature.definition_module.as_deref() == Some("builtins")
            && signature.name.as_deref() == Some("reversed")
        {
            if let Some((returned, expected)) = signature.returns.zip(expected_return) {
                if matches!(self.node(returned).as_deref(), Some(Node::Variable(_))) {
                    let fixed = BTreeMap::from([(returned, expected)]);
                    let variables: Vec<_> = signature
                        .generic
                        .iter()
                        .copied()
                        .filter(|&variable| variable != returned)
                        .collect();
                    let arguments: Vec<_> = arguments
                        .iter()
                        .map(|&(target, actual)| (self.substitute(target, &fixed), actual))
                        .collect();
                    let mut solution = self.solve_call(&variables, &arguments, None);
                    solution.types.extend(fixed);
                    return solution;
                }
                if let Some(id) = self
                    .class_id(returned)
                    .filter(|&id| self.class(id).namespace == "builtins.reversed")
                {
                    if let Some(item) = self.iterable_item(expected) {
                        let expected = self.applied_class(id, vec![item], false);
                        return self.solve_call(
                            &signature.generic,
                            arguments,
                            Some((returned, expected)),
                        );
                    }
                }
            }
        }
        let mut solution = self.solve_call(
            &signature.generic,
            arguments,
            signature.returns.zip(expected_return),
        );
        if expected_return.is_none() {
            self.apply_linear_factory_poly(signature, arguments, &mut solution);
        }
        solution
    }
}
