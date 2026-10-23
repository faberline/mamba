//! Capture origins and tuple subjects retain lexical identity across match branches.
use super::*;
use quality_syntax::python_ast::visitor::walk_pattern;

struct CaptureNames(Vec<Identifier>);
impl<'a> Visitor<'a> for CaptureNames {
    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        let name = match pattern {
            Pattern::MatchAs(value) => value.name.as_ref(),
            Pattern::MatchStar(value) => value.name.as_ref(),
            Pattern::MatchMapping(value) => value.rest.as_ref(),
            _ => None,
        };
        if let Some(name) = name {
            self.0.push(name.clone());
        }
        walk_pattern(self, pattern);
    }
}

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn untyped_pattern_value(
        &self,
        expression: &Expr,
    ) -> Option<Type> {
        let Expr::Name(name) = expression else {
            return None;
        };
        (self
            .scopes
            .binding_location(name.id.as_str())
            .is_some_and(|location| self.untyped_pattern_bindings.contains(&location))
            && !self.environment().contains_key(name.id.as_str()))
        .then_some(Type::Any)
    }
    pub(super) fn declare_pattern_names(&mut self, pattern: &Pattern) {
        let mut names = CaptureNames(Vec::new());
        names.visit_pattern(pattern);
        for name in names.0 {
            if !self.scopes.has_binding(name.as_str()) {
                self.scopes.declare(&capture_expression(&name), None);
                if let Some(location) = self.scopes.binding_location(name.as_str()) {
                    self.untyped_pattern_bindings.insert(location);
                }
            }
        }
    }

    pub(super) fn refine_pattern_subject(&self, flow: &mut flow::Flow, subject: &Expr, ty: Type) {
        if let Some(key) = scopes::Scopes::attribute_key(subject) {
            flow.narrowed.insert(key, ty);
        } else if let (Expr::Tuple(tuple), Some(Node::Tuple(items))) =
            (subject, self.context.node(ty).as_deref())
        {
            if tuple.elts.len() == items.len() {
                for (expression, &ty) in tuple.elts.iter().zip(items) {
                    self.refine_pattern_subject(flow, expression, ty);
                }
            }
        }
    }

    pub(super) fn restore_pattern_subject(
        &self,
        flow: &mut flow::Flow,
        subject: &Expr,
        input: Type,
    ) {
        if let Some(key) = scopes::Scopes::attribute_key(subject) {
            if let Some(&output) = flow.narrowed.get(&key) {
                let elements = |ty| match self.context.node(ty).as_deref() {
                    Some(Node::Union(items)) => items
                        .iter()
                        .copied()
                        .collect::<std::collections::BTreeSet<_>>(),
                    _ => std::collections::BTreeSet::from([ty]),
                };
                if elements(input) == elements(output) {
                    flow.narrowed.insert(key, input);
                }
            }
        } else if let (Expr::Tuple(tuple), Some(Node::Tuple(items))) =
            (subject, self.context.node(input).as_deref())
        {
            for (expression, &ty) in tuple.elts.iter().zip(items) {
                self.restore_pattern_subject(flow, expression, ty);
            }
        }
    }

    pub(super) fn ordered_pattern_rest(&self, input: Type, rest: Type) -> Type {
        if let (Some(Node::Union(originals)), Some(Node::Union(items))) = (
            self.context.node(input).as_deref(),
            self.context.node(rest).as_deref(),
        ) {
            let mut items = items.clone();
            items.sort_by_key(|&item| {
                originals
                    .iter()
                    .position(|&original| self.context.accepts(original, item))
                    .unwrap_or(originals.len())
            });
            self.context.union(items)
        } else {
            rest
        }
    }
}
