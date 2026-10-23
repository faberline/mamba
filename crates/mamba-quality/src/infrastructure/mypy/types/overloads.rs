//! Overload subtyping preserves order, overlap constraints, and combined signatures.
use super::super::signatures::{compatibility, Kind, Parameter, Signature};
use super::*;
use std::sync::Arc;

impl Context {
    pub(crate) fn parameters_cover(
        &self,
        parent: &Signature,
        child: &Signature,
        ignore_names: bool,
    ) -> bool {
        if child.generic.is_empty() {
            return compatibility::parameters_accept(self, parent, child, ignore_names);
        }
        let parent = Signature {
            returns: None,
            ..parent.clone()
        };
        let child = Signature {
            returns: None,
            ..child.clone()
        };
        self.instantiate_callable(&child, &parent)
            .is_some_and(|child| {
                compatibility::parameters_accept(self, &parent, &child, ignore_names)
            })
    }

    pub(super) fn accepts_overloads(
        &self,
        parent: &Signature,
        child: &Signature,
        ignore_names: bool,
    ) -> bool {
        let mut previous = 0;
        let mut matched = std::collections::BTreeSet::new();
        for parent in parent.overloads.iter() {
            let mut found = false;
            for (index, child) in child.overloads.iter().enumerate() {
                if index >= previous && self.accepts_callable(parent, child, ignore_names) {
                    previous = index;
                    matched.insert(index);
                    found = true;
                    break;
                }
                if !matched.contains(&index)
                    && (self.parameters_cover(parent, child, ignore_names)
                        || self.parameters_cover(child, parent, ignore_names))
                {
                    return false;
                }
            }
            if !found {
                return false;
            }
        }
        true
    }

    pub(super) fn combine_overloads(&self, signature: &Signature) -> Option<Signature> {
        let first = signature.overloads.first()?;
        let mut generic = Vec::new();
        let mut by_name = BTreeMap::new();
        let mut substitutions = BTreeMap::new();
        for variant in signature.overloads.iter() {
            for &ty in variant.generic.iter() {
                let canonical = *by_name.entry(self.name(ty)).or_insert_with(|| {
                    generic.push(ty);
                    ty
                });
                substitutions.insert(ty, canonical);
            }
        }
        let variants: Vec<_> = signature
            .overloads
            .iter()
            .map(|signature| self.substitute_callable(signature, &substitutions))
            .collect();
        let mut parameters: Vec<Parameter> = first.parameters.to_vec();
        let mut arguments = vec![Vec::new(); parameters.len()];
        let mut returns = Vec::new();
        for variant in &variants {
            if parameters.len() != variant.parameters.len() {
                return None;
            }
            for (index, (parameter, target)) in parameters
                .iter_mut()
                .zip(variant.parameters.iter())
                .enumerate()
            {
                let positional =
                    |kind| matches!(kind, Kind::PositionalOnly | Kind::PositionalOrKeyword);
                if parameter.kind != target.kind {
                    if positional(parameter.kind) && positional(target.kind) {
                        parameter.kind = Kind::PositionalOnly;
                    } else {
                        return None;
                    }
                }
                if parameter.name != target.name {
                    if matches!(target.kind, Kind::KeywordOnly) {
                        return None;
                    }
                    if positional(target.kind) {
                        parameter.name.clear();
                        parameter.kind = Kind::PositionalOnly;
                    }
                }
                parameter.required |= target.required;
                arguments[index].push(target.ty.unwrap_or(Type::Any));
            }
            returns.push(variant.returns.unwrap_or(Type::Any));
        }
        for (parameter, arguments) in parameters.iter_mut().zip(arguments) {
            parameter.ty = Some(self.union(arguments));
        }
        Some(Signature {
            parameters: parameters.into(),
            returns: Some(self.union(returns)),
            generic: generic.into(),
            overloads: Arc::from([]),
            ..(**first).clone()
        })
    }
}
