//! Only a direct function-local list with a complete RHS can resolve here.
use super::*;
use quality_syntax::python_ast::Operator;

impl Checker<'_> {
    pub(super) fn infer_partial_list_augmented(
        &mut self,
        key: TextSize,
        assignment: &StmtAugAssign,
    ) -> Result<(), &'static str> {
        if assignment.op != Operator::Add || self.partial_list_unknown.contains(&key) {
            return Err("augmented list assignment is not inferred");
        }
        let partial = &self.partial_sets[&key];
        if key >= assignment.target.start()
            || self.flow.deleted.contains(&partial.name)
            || partial.owner != self.partial_set_scope
            || self.scopes.direct_inferred_local_binding(&partial.name)
                != partial.binding_owner.map(|owner| (owner, key))
        {
            return Err("augmented list assignment has no direct local binding owner");
        }
        let value = assignment.value.as_ref();
        let actual = self
            .partial_list_observed_type(value)
            .ok_or("augmented list assignment type is unavailable")?;
        // Accept only an already inferred direct Any RHS or list[Any] RHS.
        let inferred_any_rhs = actual == Type::Any
            || self.context.node(actual).is_some_and(|node| {
                matches!(node.as_ref(), Node::Container(Container::List, items)
                    if items.as_slice() == [Type::Any])
            });
        if !inferred_any_rhs && !self.partial_list_augmented_bound(actual, &mut Default::default())
        {
            return Err("augmented list assignment has an unsupported element bound");
        }
        let mut reads = DeletedReads {
            deleted: &self.flow.deleted,
            found: false,
        };
        reads.visit_expr(value);
        if reads.found {
            return Err("augmented list assignment reads a deleted value");
        }
        let ty = self
            .partial_list_assignment_type(value, Some(actual))
            .map_err(|validity| match validity {
                ListValidity::Unsupported(reason) => reason,
                _ => "augmented list assignment has no valid list bound",
            })?;
        if !self.resolve_partial_list(key, ty) {
            return Err("augmented list assignment binding could not be refined");
        }
        Ok(())
    }

    // This is an admission check, not a substitute inference or operator solver.
    // Other graph shapes stay explicit gaps until their provenance is supported.
    fn partial_list_augmented_bound(
        &self,
        ty: Type,
        seen: &mut std::collections::BTreeSet<Type>,
    ) -> bool {
        if matches!(ty, Type::Any | Type::AnySpecial | Type::Never) {
            return false;
        }
        if !seen.insert(ty) {
            return false;
        }
        let result = if let Some((_, dictionary)) = self.context.typed_dict(ty) {
            dictionary
                .items
                .iter()
                .all(|item| self.partial_list_augmented_bound(item.ty, seen))
        } else {
            match self.context.node(ty).as_deref() {
                None => true,
                Some(
                    Node::Container(_, items)
                    | Node::Tuple(items)
                    | Node::Union(items)
                    | Node::AppliedClass {
                        arguments: items, ..
                    },
                ) => items
                    .iter()
                    .all(|&item| self.partial_list_augmented_bound(item, seen)),
                Some(Node::VariadicTuple(item)) => self.partial_list_augmented_bound(*item, seen),
                Some(Node::AliasInstance(_, _) | Node::InferredAlias { .. }) => {
                    let expanded = self.context.expanded_alias_instance(ty);
                    expanded != ty && self.partial_list_augmented_bound(expanded, seen)
                }
                Some(
                    Node::Instance(_) | Node::Class(_) | Node::Literal(_) | Node::LiteralValue(_),
                ) => true,
                _ => false,
            }
        };
        seen.remove(&ty);
        result
    }
}

struct DeletedReads<'a> {
    deleted: &'a std::collections::BTreeSet<String>,
    found: bool,
}

impl<'a> Visitor<'a> for DeletedReads<'_> {
    fn visit_expr(&mut self, expression: &'a Expr) {
        if matches!(expression, Expr::Name(name)
            if name.ctx.is_load() && self.deleted.contains(name.id.as_str()))
        {
            self.found = true;
        }
        walk_expr(self, expression);
    }
}
