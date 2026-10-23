//! Legacy container aliases expose constructors with expanded type arguments.
use super::*;
use crate::infrastructure::mypy::signatures::Signature;
use mamba_language::application::quality_syntax::PythonTarget;
use std::sync::Arc;

impl Context {
    pub(in crate::infrastructure::mypy) fn builtin_alias_constructor(
        &self,
        ty: Type,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        self.builtin_alias_constructor_with_arguments(ty, Vec::new(), target)
    }

    pub(in crate::infrastructure::mypy) fn builtin_alias_constructor_with_arguments(
        &self,
        ty: Type,
        arguments: Vec<Type>,
        target: PythonTarget,
    ) -> Option<Arc<Signature>> {
        let id = self.alias_id(ty)?;
        let alias = self.alias(id);
        if alias.modern {
            return None;
        }
        let target_type = self.expand_alias(id, arguments)?;
        let node = self.node(target_type)?;
        let (name, arguments) = match node.as_ref() {
            Node::Container(kind, arguments) => (
                match kind {
                    Container::List => "list",
                    Container::Dict => "dict",
                    Container::Set => "set",
                    Container::FrozenSet => "frozenset",
                    _ => return None,
                },
                arguments.clone(),
            ),
            Node::VariadicTuple(item) => ("tuple", vec![*item]),
            Node::AppliedClass {
                id,
                arguments,
                value: false,
            } if alias.explicit_arguments
                && matches!(
                    self.class(*id).namespace.as_str(),
                    "builtins.reversed" | "builtins.enumerate" | "builtins.zip"
                ) =>
            {
                let name = match self.class(*id).namespace.as_str() {
                    "builtins.reversed" => "reversed",
                    "builtins.enumerate" => "enumerate",
                    "builtins.zip" => "zip",
                    _ => return None,
                };
                (name, arguments.clone())
            }
            _ => return None,
        };
        let signature = self
            .collection_constructor(name, target)
            .or_else(|| self.aggregation_function(name, target))?;
        let solutions = signature.generic.iter().copied().zip(arguments).collect();
        Some(Arc::new(self.substitute_callable(&signature, &solutions)))
    }

    pub(in crate::infrastructure::mypy) fn builtin_fixed_tuple_alias(&self, ty: Type) -> bool {
        self.alias_id(ty).is_some_and(|id| {
            !self.alias(id).modern
                && self
                    .expand_alias(id, Vec::new())
                    .and_then(|ty| self.node(ty))
                    .is_some_and(|node| matches!(node.as_ref(), Node::Tuple(_)))
        })
    }
}
