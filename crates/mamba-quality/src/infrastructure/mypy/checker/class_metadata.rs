//! Inferred pattern metadata warns when its tuple cannot name attributes.
use super::*;
use quality_syntax::python_ast::StmtClassDef;

impl Checker<'_> {
    pub(super) fn check_special_class_attributes(&mut self, class: &StmtClassDef) {
        for member in &class.body {
            let Stmt::Assign(assignment) = member else {
                continue;
            };
            for target in &assignment.targets {
                if !matches!(target, Expr::Name(name) if name.id.as_str() == "__match_args__") {
                    continue;
                }
                if matches!(assignment.value.as_ref(), Expr::Tuple(tuple) if tuple.elts.iter().all(Expr::is_string_literal_expr))
                {
                    continue;
                }
                let mut diagnostic = super::super::super::diagnostics::at(
                    self.source,
                    QualityTool::Mypy,
                    "literal-required",
                    "__match_args__ must be a tuple containing string literals for checking of match statements to work".into(),
                    target.range(),
                );
                diagnostic.severity = "note".into();
                if !self
                    .directives
                    .suppresses(diagnostic.location.line, "literal-required")
                {
                    self.diagnostics.push(diagnostic);
                }
            }
        }
    }
}
