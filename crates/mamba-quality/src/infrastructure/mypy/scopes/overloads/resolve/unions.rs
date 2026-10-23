//! The pinned checker splits at most four nested argument unions before fallback.
use super::*;

pub(super) fn items(context: &types::Context, ty: Type) -> Option<Vec<Type>> {
    context.node(ty).and_then(|node| match node.as_ref() {
        types::Node::Union(items) if items.len() > 1 => Some(items.clone()),
        _ => None,
    })
}

pub(super) fn force_optional_split(
    scopes: &Scopes,
    call: &ExprCall,
    signature: &Signature,
    values: &[Type],
) -> bool {
    if !values
        .iter()
        .any(|&ty| items(&scopes.context, ty).is_some_and(|items| items.contains(&Type::None)))
    {
        return false;
    }
    let candidates: Vec<_> = signature
        .overloads
        .iter()
        .filter(|variant| binding::pairs(&scopes.context, call, variant, values, true).is_some())
        .collect();
    let prefix = candidates
        .iter()
        .map(|variant| variant.parameters.len())
        .min()
        .unwrap_or(0);
    (0..prefix).any(|index| {
        candidates
            .iter()
            .any(|variant| variant.parameters[index].ty == Some(Type::None))
            && candidates.iter().any(|variant| {
                variant.parameters[index]
                    .ty
                    .and_then(|ty| scopes.context.node(ty))
                    .is_some_and(|node| matches!(node.as_ref(), types::Node::Variable(_)))
            })
    })
}

pub(super) struct Search<'a> {
    pub scopes: &'a Scopes,
    pub call: &'a ExprCall,
    pub signature: &'a Signature,
    pub empty_list_any_placeholders: &'a BTreeSet<usize>,
    pub expected_return: Option<Type>,
    pub forced: bool,
}

impl Search<'_> {
    pub(super) fn resolve(&self, values: &[Type], level: usize) -> Result<Option<Type>, ()> {
        if level >= 5 {
            return Err(());
        }
        let direct = self
            .scopes
            .overload_direct(
                self.call,
                self.signature,
                values,
                self.empty_list_any_placeholders,
                self.expected_return,
                // Mypy union_overload_result freezes actuals with type_overrides_set;
                // accept returns them before contextual lambda shape checking.
                false,
            )
            .map(|(ty, _, _)| ty);
        let Some((index, choices)) = values
            .iter()
            .enumerate()
            .find_map(|(index, &ty)| items(&self.scopes.context, ty).map(|items| (index, items)))
        else {
            return Ok(direct);
        };
        if !self.forced {
            if let Some(ty) =
                direct.filter(|&ty| !ty.is_any() && items(&self.scopes.context, ty).is_none())
            {
                return Ok(Some(ty));
            }
        }
        let mut results = Vec::new();
        for ty in choices {
            let mut branch = values.to_vec();
            branch[index] = ty;
            let Some(ty) = self.resolve(&branch, level + 1)? else {
                return Ok(None);
            };
            if !results.contains(&ty) {
                results.push(ty)
            }
        }
        Ok(Some(self.scopes.context.union(results)))
    }
}
