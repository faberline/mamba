//! Fixed tuple count checks use existing types without changing bindings.
use super::*;

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn check_fixed_tuple_unpack_count(
        &mut self,
        target: &Expr,
        actual: Option<types::Type>,
        location: &impl Ranged,
    ) {
        let targets: &[Expr] = match target {
            Expr::Tuple(tuple) => tuple.elts.as_ref(),
            Expr::List(list) => list.elts.as_ref(),
            _ => return,
        };
        let stars = targets
            .iter()
            .filter(|target| target.is_starred_expr())
            .count();
        if stars > 1 {
            return;
        }
        let Some(actual) = actual else {
            return;
        };
        if self.string_unpack_disallowed(actual, targets.len()) {
            self.emit(
                "str-unpack",
                "Unpacking a string is disallowed".into(),
                location,
            );
        }
        let Some(node) = self.context.node(actual) else {
            return;
        };
        let members = match node.as_ref() {
            types::Node::Tuple(_) => std::slice::from_ref(&actual),
            types::Node::Union(members) => members.as_slice(),
            _ => return,
        };
        // Prove the complete union before emitting any count diagnostic.
        let mut counts = Vec::new();
        for &member in members {
            let Some(node) = self.context.node(member) else {
                return;
            };
            let types::Node::Tuple(items) = node.as_ref() else {
                return;
            };
            if self.context.tuple_pack_position(items).is_some() {
                return;
            }
            counts.push(items.len());
        }
        let expected = targets.len() - stars;
        let mut seen = BTreeSet::new();
        for provided in counts {
            if !seen.insert(provided) {
                continue;
            }
            let message = if provided < expected {
                let values = if provided == 1 { "value" } else { "values" };
                format!("Need more than {provided} {values} to unpack ({expected} expected)")
            } else if stars == 0 && provided > expected {
                format!("Too many values to unpack ({expected} expected, {provided} provided)")
            } else {
                continue;
            };
            self.emit("misc", message, location);
        }
    }

    fn string_unpack_disallowed(&self, actual: types::Type, target_count: usize) -> bool {
        if actual == types::Type::Str {
            return true;
        }
        let Some(node) = self.context.node(actual) else {
            return false;
        };
        match node.as_ref() {
            types::Node::Literal(types::Literal::Str(value))
            | types::Node::LiteralValue(types::Literal::Str(value)) => {
                value.chars().count() != target_count
            }
            types::Node::Union(members) => members
                .iter()
                .any(|&member| self.string_unpack_disallowed(member, target_count)),
            _ => false,
        }
    }

    pub(super) fn sequence_assignment_bindings<'a>(
        &self,
        target: &'a Expr,
        value: &'a Expr,
        actual: Option<types::Type>,
    ) -> Option<Vec<AssignmentBinding<'a>>> {
        if matches!(target, Expr::Name(_)) {
            return Some(vec![AssignmentBinding {
                target,
                expression: Some(value),
                // An unknown item type does not erase its lexical assignment.
                ty: actual
                    .or_else(|| self.infer(value))
                    .unwrap_or(types::Type::Any),
            }]);
        }
        let targets: &[Expr] = match target {
            Expr::Tuple(tuple) => tuple.elts.as_ref(),
            Expr::List(list) => list.elts.as_ref(),
            _ => return None,
        };
        let values: Option<&[Expr]> = match value {
            Expr::Tuple(tuple) => Some(tuple.elts.as_ref()),
            Expr::List(list) => Some(list.elts.as_ref()),
            _ => None,
        };
        if let Some(values) = values {
            if targets.len() != values.len() || values.iter().any(Expr::is_starred_expr) {
                return None;
            }
            let mut bindings = Vec::new();
            for (target, value) in targets.iter().zip(values) {
                bindings.extend(self.sequence_assignment_bindings(
                    target,
                    value,
                    self.infer(value),
                )?);
            }
            return Some(bindings);
        }
        Some(
            self.scopes
                // Unknown RHS types still establish the unpacked lexical names.
                .assignment_bindings(target, actual.unwrap_or(types::Type::Any))?
                .into_iter()
                .map(|(target, ty)| AssignmentBinding {
                    target,
                    expression: None,
                    ty,
                })
                .collect(),
        )
    }
}
