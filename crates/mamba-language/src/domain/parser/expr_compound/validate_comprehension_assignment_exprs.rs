use super::*;

impl<'a> Parser<'a> {
    pub(super) fn validate_comprehension_assignment_exprs(
        &self,
        body_exprs: &[&Spanned<Expr>],
        generators: &[Comprehension],
        span: Span,
    ) -> crate::error::Result<()> {
        for generator in generators {
            if Self::expr_contains_walrus(&generator.iter) {
                return Err(crate::error::MambaError::syntax(
                    generator.iter.span,
                    "assignment expression cannot be used in a comprehension iterable expression",
                ));
            }
        }

        let mut walrus_targets = Vec::new();
        for expr in body_exprs {
            Self::collect_walrus_targets(expr, &mut walrus_targets);
        }
        for generator in generators {
            for condition in &generator.conditions {
                Self::collect_walrus_targets(condition, &mut walrus_targets);
            }
        }

        let iteration_targets: Vec<&str> = generators
            .iter()
            .flat_map(|generator| generator.targets.iter().map(String::as_str))
            .collect();
        for target in &walrus_targets {
            if iteration_targets.contains(&target.as_str()) {
                return Err(crate::error::MambaError::syntax(
                    span,
                    format!(
                        "assignment expression cannot rebind comprehension iteration variable '{target}'",
                    ),
                ));
            }
        }

        if self.in_class_body && !walrus_targets.is_empty() {
            return Err(crate::error::MambaError::syntax(
                span,
                "assignment expression within a comprehension cannot be used in a class body",
            ));
        }

        Ok(())
    }
}
