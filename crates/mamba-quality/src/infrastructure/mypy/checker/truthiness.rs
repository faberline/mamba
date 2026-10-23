//! Boolean conditions refine names, member values, and named-expression sources.
use super::*;
use types::Type;

impl Checker<'_> {
    pub(super) fn flow_truthiness(&mut self, expression: &Expr, positive: bool) -> bool {
        if !matches!(
            expression,
            Expr::Name(_)
                | Expr::Attribute(_)
                | Expr::Subscript(_)
                | Expr::Named(_)
                | Expr::Call(_)
        ) {
            return false;
        }
        let Some(input) = self.infer(expression) else {
            return true;
        };
        if matches!(expression, Expr::Name(_) | Expr::Attribute(_))
            && self.context.truthiness_type(input, false) == Type::Never
            && (self.context.callable_signature(input).is_some()
                || self.context.is_class_value(input))
        {
            let name = match expression {
                Expr::Name(name) => name.id.as_str(),
                Expr::Attribute(attribute) => attribute.attr.as_str(),
                _ => unreachable!(),
            };
            self.emit(
                "truthy-function",
                format!("Function \"{name}\" could always be true in boolean context"),
                expression,
            );
        }
        let narrowed = self.context.truthiness_type(input, positive);
        self.flow.reachable &= narrowed != Type::Never;
        if let Expr::Named(named) = expression {
            for value in [named.target.as_ref(), named.value.as_ref()] {
                if let Some(key) = scopes::Scopes::attribute_key(value) {
                    self.flow.narrowed.insert(key, narrowed);
                }
            }
        } else if let Some(key) = scopes::Scopes::attribute_key(expression) {
            self.flow.narrowed.insert(key, narrowed);
        }
        true
    }

    pub(super) fn declared_flow_type(
        &self,
        name: &str,
        baseline: &BTreeMap<String, Type>,
    ) -> Option<Type> {
        if let Some(&ty) = baseline.get(name) {
            return Some(ty);
        }
        let mut parts = name.split('.');
        let mut ty = *baseline.get(parts.next()?)?;
        for member in parts {
            ty = self.context.class_field(ty, member)?.ty?;
        }
        Some(ty)
    }

    pub(super) fn visit_conditional_value<'a>(&mut self, expression: &'a Expr) -> bool
    where
        Self: Visitor<'a>,
    {
        let Expr::If(conditional) = expression else {
            return false;
        };
        self.visit_expr(&conditional.test);
        let previous = self.flow.clone();
        self.flow_condition(&conditional.test, true);
        if self.flow.reachable {
            self.visit_expr(&conditional.body);
        }
        self.flow = previous.clone();
        self.flow_condition(&conditional.test, false);
        if self.flow.reachable {
            self.visit_expr(&conditional.orelse);
        }
        self.flow = previous;
        true
    }
}
